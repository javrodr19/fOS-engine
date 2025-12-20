//! Top-Level Await Support
//!
//! Implementation of ES2022 top-level await for modules.

use std::collections::HashMap;
use std::sync::Arc;

/// Module evaluation state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleState {
    /// Not yet evaluated
    Unlinked,
    /// Linking in progress
    Linking,
    /// Linked (dependencies resolved)
    Linked,
    /// Evaluating (may be async)
    Evaluating,
    /// Evaluation complete
    Evaluated,
    /// Evaluation failed
    Error,
}

/// Async module record
#[derive(Debug)]
pub struct AsyncModule {
    /// Module identifier
    pub id: String,
    /// Module state
    pub state: ModuleState,
    /// Is this module async (contains top-level await)
    pub is_async: bool,
    /// Dependencies (module IDs)
    pub dependencies: Vec<String>,
    /// Modules waiting on this one
    pub pending_async_dependents: Vec<String>,
    /// Async evaluation promise ID (if evaluating asynchronously)
    pub async_promise: Option<u64>,
    /// Result of evaluation
    pub evaluation_result: Option<EvaluationResult>,
}

/// Evaluation result
#[derive(Debug, Clone)]
pub enum EvaluationResult {
    Success,
    Error(String),
}

impl AsyncModule {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            state: ModuleState::Unlinked,
            is_async: false,
            dependencies: Vec::new(),
            pending_async_dependents: Vec::new(),
            async_promise: None,
            evaluation_result: None,
        }
    }
    
    /// Mark module as async
    pub fn set_async(&mut self) {
        self.is_async = true;
    }
    
    /// Add dependency
    pub fn add_dependency(&mut self, dep: &str) {
        if !self.dependencies.contains(&dep.to_string()) {
            self.dependencies.push(dep.to_string());
        }
    }
}

/// Top-level await module graph
#[derive(Debug, Default)]
pub struct TlaModuleGraph {
    /// All modules
    modules: HashMap<String, AsyncModule>,
    /// Execution order (topologically sorted)
    execution_order: Vec<String>,
}

impl TlaModuleGraph {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add a module to the graph
    pub fn add_module(&mut self, module: AsyncModule) {
        self.modules.insert(module.id.clone(), module);
    }
    
    /// Get a module
    pub fn get(&self, id: &str) -> Option<&AsyncModule> {
        self.modules.get(id)
    }
    
    /// Get mutable module
    pub fn get_mut(&mut self, id: &str) -> Option<&mut AsyncModule> {
        self.modules.get_mut(id)
    }
    
    /// Link modules (resolve dependencies)
    pub fn link(&mut self, entry: &str) -> Result<(), TlaError> {
        let mut visited = Vec::new();
        let mut stack = Vec::new();
        
        self.link_module(entry, &mut visited, &mut stack)?;
        
        // Execution order is reverse of the dependency resolution
        self.execution_order = visited;
        
        Ok(())
    }
    
    fn link_module(
        &mut self, 
        id: &str, 
        visited: &mut Vec<String>,
        stack: &mut Vec<String>,
    ) -> Result<(), TlaError> {
        if visited.contains(&id.to_string()) {
            return Ok(());
        }
        
        if stack.contains(&id.to_string()) {
            return Err(TlaError::CyclicDependency(id.to_string()));
        }
        
        stack.push(id.to_string());
        
        if let Some(module) = self.modules.get(id) {
            let deps = module.dependencies.clone();
            for dep in deps {
                self.link_module(&dep, visited, stack)?;
            }
        }
        
        stack.pop();
        visited.push(id.to_string());
        
        if let Some(module) = self.modules.get_mut(id) {
            module.state = ModuleState::Linked;
        }
        
        Ok(())
    }
    
    /// Evaluate modules (handling async)
    pub fn evaluate(&mut self, entry: &str) -> TlaEvaluationHandle {
        TlaEvaluationHandle {
            entry: entry.to_string(),
            pending: self.execution_order.clone(),
            completed: Vec::new(),
            errors: Vec::new(),
        }
    }
    
    /// Check if a module is async
    pub fn is_async_module(&self, id: &str) -> bool {
        self.modules.get(id).map(|m| m.is_async).unwrap_or(false)
    }
    
    /// Get execution order
    pub fn execution_order(&self) -> &[String] {
        &self.execution_order
    }
}

/// Handle for async module evaluation
#[derive(Debug)]
pub struct TlaEvaluationHandle {
    /// Entry point module
    pub entry: String,
    /// Pending modules
    pub pending: Vec<String>,
    /// Completed modules
    pub completed: Vec<String>,
    /// Evaluation errors
    pub errors: Vec<(String, String)>,
}

impl TlaEvaluationHandle {
    /// Get next module to evaluate
    pub fn next(&mut self) -> Option<String> {
        self.pending.pop()
    }
    
    /// Mark module as completed
    pub fn complete(&mut self, id: &str) {
        self.completed.push(id.to_string());
    }
    
    /// Record an error
    pub fn error(&mut self, id: &str, msg: &str) {
        self.errors.push((id.to_string(), msg.to_string()));
    }
    
    /// Check if evaluation is done
    pub fn is_done(&self) -> bool {
        self.pending.is_empty()
    }
    
    /// Check if evaluation succeeded
    pub fn is_success(&self) -> bool {
        self.is_done() && self.errors.is_empty()
    }
}

/// TLA error
#[derive(Debug, Clone, thiserror::Error)]
pub enum TlaError {
    #[error("Cyclic dependency detected: {0}")]
    CyclicDependency(String),
    
    #[error("Module not found: {0}")]
    ModuleNotFound(String),
    
    #[error("Evaluation error in {0}: {1}")]
    EvaluationError(String, String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_async_module() {
        let mut module = AsyncModule::new("test");
        assert!(!module.is_async);
        
        module.set_async();
        assert!(module.is_async);
    }
    
    #[test]
    fn test_module_graph() {
        let mut graph = TlaModuleGraph::new();
        
        let mut a = AsyncModule::new("a");
        a.add_dependency("b");
        
        let b = AsyncModule::new("b");
        
        graph.add_module(a);
        graph.add_module(b);
        
        assert!(graph.link("a").is_ok());
        assert_eq!(graph.execution_order(), &["b", "a"]);
    }
}
