//! Lazy Function Compilation (Phase 24.6)
//!
//! Parse but don't compile until called. Many functions never called.
//! Compile on first invocation. 50% faster page load.

use std::collections::HashMap;

/// Function ID
pub type FuncId = u32;

/// Function state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionState {
    /// Only parsed, not compiled
    Parsed,
    /// Currently being compiled
    Compiling,
    /// Compiled and ready
    Compiled,
    /// Marked as never needed
    Dead,
}

/// Parsed function info (minimal)
#[derive(Debug, Clone)]
pub struct ParsedFunction {
    /// Function ID
    pub id: FuncId,
    /// Function name
    pub name: Option<Box<str>>,
    /// Source location
    pub source_start: u32,
    pub source_end: u32,
    /// Parameter count
    pub params: u8,
    /// Is strict mode
    pub strict: bool,
    /// Is async
    pub is_async: bool,
    /// Is generator
    pub is_generator: bool,
    /// State
    pub state: FunctionState,
    /// Call count
    pub call_count: u32,
}

impl ParsedFunction {
    pub fn new(id: FuncId, source_start: u32, source_end: u32) -> Self {
        Self {
            id,
            name: None,
            source_start,
            source_end,
            params: 0,
            strict: false,
            is_async: false,
            is_generator: false,
            state: FunctionState::Parsed,
            call_count: 0,
        }
    }
    
    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.into());
        self
    }
    
    pub fn with_params(mut self, params: u8) -> Self {
        self.params = params;
        self
    }
}

/// Compiled function
#[derive(Debug)]
pub struct CompiledFunction {
    /// Function ID
    pub id: FuncId,
    /// Bytecode
    pub bytecode: Vec<u8>,
    /// Constants
    pub constants: Vec<JsConstant>,
    /// Local variable count
    pub locals: u16,
    /// Stack size
    pub stack_size: u16,
}

/// JavaScript constant
#[derive(Debug, Clone)]
pub enum JsConstant {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(Box<str>),
    Regex(Box<str>, Box<str>),
}

/// Lazy function manager
#[derive(Debug)]
pub struct LazyFunctionManager {
    /// Parsed functions
    parsed: HashMap<FuncId, ParsedFunction>,
    /// Compiled functions
    compiled: HashMap<FuncId, CompiledFunction>,
    /// Source code (for re-parsing during compilation)
    source: Option<Box<str>>,
    /// Next function ID
    next_id: FuncId,
    /// Compilation threshold
    compile_threshold: u32,
    /// Statistics
    stats: LazyCompileStats,
}

/// Lazy compilation statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct LazyCompileStats {
    pub functions_parsed: u64,
    pub functions_compiled: u64,
    pub functions_never_called: u64,
    pub compile_on_first_call: u64,
    pub immediate_compiles: u64,
}

impl LazyCompileStats {
    pub fn lazy_ratio(&self) -> f64 {
        if self.functions_parsed == 0 {
            0.0
        } else {
            self.functions_never_called as f64 / self.functions_parsed as f64
        }
    }
    
    pub fn compile_savings(&self) -> f64 {
        if self.functions_parsed == 0 {
            0.0
        } else {
            (self.functions_parsed - self.functions_compiled) as f64 / self.functions_parsed as f64
        }
    }
}

impl Default for LazyFunctionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LazyFunctionManager {
    pub fn new() -> Self {
        Self {
            parsed: HashMap::new(),
            compiled: HashMap::new(),
            source: None,
            next_id: 0,
            compile_threshold: 1, // Compile on first call
            stats: LazyCompileStats::default(),
        }
    }
    
    /// Set source code
    pub fn set_source(&mut self, source: &str) {
        self.source = Some(source.into());
    }
    
    /// Set compile threshold (compile after N calls)
    pub fn with_compile_threshold(mut self, threshold: u32) -> Self {
        self.compile_threshold = threshold;
        self
    }
    
    /// Register a parsed function
    pub fn register_parsed(&mut self, func: ParsedFunction) -> FuncId {
        let id = func.id;
        self.parsed.insert(id, func);
        self.stats.functions_parsed += 1;
        id
    }
    
    /// Create and register a new function
    pub fn create_function(&mut self, source_start: u32, source_end: u32) -> FuncId {
        let id = self.next_id;
        self.next_id += 1;
        
        let func = ParsedFunction::new(id, source_start, source_end);
        self.register_parsed(func);
        id
    }
    
    /// Called when function is invoked
    pub fn on_call(&mut self, id: FuncId) -> CallResult {
        if let Some(compiled) = self.compiled.get(&id) {
            return CallResult::Ready(compiled);
        }
        
        if let Some(parsed) = self.parsed.get_mut(&id) {
            parsed.call_count += 1;
            
            if parsed.call_count >= self.compile_threshold {
                // Need to compile
                parsed.state = FunctionState::Compiling;
                return CallResult::NeedsCompile(id);
            } else {
                // Interpret for now
                return CallResult::Interpret(id);
            }
        }
        
        CallResult::NotFound
    }
    
    /// Register compiled function
    pub fn register_compiled(&mut self, func: CompiledFunction) {
        let id = func.id;
        
        if let Some(parsed) = self.parsed.get_mut(&id) {
            parsed.state = FunctionState::Compiled;
        }
        
        self.compiled.insert(id, func);
        self.stats.functions_compiled += 1;
        self.stats.compile_on_first_call += 1;
    }
    
    /// Mark function as dead (never needed)
    pub fn mark_dead(&mut self, id: FuncId) {
        if let Some(parsed) = self.parsed.get_mut(&id) {
            parsed.state = FunctionState::Dead;
            self.stats.functions_never_called += 1;
        }
    }
    
    /// Get parsed function
    pub fn get_parsed(&self, id: FuncId) -> Option<&ParsedFunction> {
        self.parsed.get(&id)
    }
    
    /// Get compiled function
    pub fn get_compiled(&self, id: FuncId) -> Option<&CompiledFunction> {
        self.compiled.get(&id)
    }
    
    /// Check if function is compiled
    pub fn is_compiled(&self, id: FuncId) -> bool {
        self.compiled.contains_key(&id)
    }
    
    /// Get source for a function
    pub fn get_source(&self, id: FuncId) -> Option<&str> {
        let parsed = self.parsed.get(&id)?;
        let source = self.source.as_ref()?;
        source.get(parsed.source_start as usize..parsed.source_end as usize)
    }
    
    /// Get statistics
    pub fn stats(&self) -> &LazyCompileStats {
        &self.stats
    }
    
    /// Finalize - count never-called functions
    pub fn finalize(&mut self) {
        for parsed in self.parsed.values() {
            if parsed.call_count == 0 && parsed.state == FunctionState::Parsed {
                self.stats.functions_never_called += 1;
            }
        }
    }
    
    /// Count of parsed functions
    pub fn parsed_count(&self) -> usize {
        self.parsed.len()
    }
    
    /// Count of compiled functions
    pub fn compiled_count(&self) -> usize {
        self.compiled.len()
    }
}

/// Result of function call
pub enum CallResult<'a> {
    /// Function is compiled and ready
    Ready(&'a CompiledFunction),
    /// Function needs compilation
    NeedsCompile(FuncId),
    /// Interpret for now (below threshold)
    Interpret(FuncId),
    /// Function not found
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lazy_registration() {
        let mut manager = LazyFunctionManager::new();
        
        let id = manager.create_function(0, 100);
        
        assert!(manager.get_parsed(id).is_some());
        assert!(!manager.is_compiled(id));
    }
    
    #[test]
    fn test_compile_on_call() {
        let mut manager = LazyFunctionManager::new();
        
        let id = manager.create_function(0, 100);
        
        // First call should trigger compile
        match manager.on_call(id) {
            CallResult::NeedsCompile(func_id) => {
                assert_eq!(func_id, id);
                
                // Register compiled
                manager.register_compiled(CompiledFunction {
                    id,
                    bytecode: vec![0, 1, 2],
                    constants: vec![],
                    locals: 5,
                    stack_size: 10,
                });
            }
            _ => panic!("Expected NeedsCompile"),
        }
        
        // Second call should be ready
        match manager.on_call(id) {
            CallResult::Ready(_) => {}
            _ => panic!("Expected Ready"),
        }
    }
    
    #[test]
    fn test_threshold() {
        let mut manager = LazyFunctionManager::new().with_compile_threshold(3);
        
        let id = manager.create_function(0, 100);
        
        // First two calls should interpret
        assert!(matches!(manager.on_call(id), CallResult::Interpret(_)));
        assert!(matches!(manager.on_call(id), CallResult::Interpret(_)));
        
        // Third call should compile
        assert!(matches!(manager.on_call(id), CallResult::NeedsCompile(_)));
    }
    
    #[test]
    fn test_stats() {
        let mut manager = LazyFunctionManager::new();
        
        manager.create_function(0, 100);
        manager.create_function(100, 200);
        manager.create_function(200, 300);
        
        // Only call one
        let id = 0;
        if let CallResult::NeedsCompile(func_id) = manager.on_call(id) {
            manager.register_compiled(CompiledFunction {
                id: func_id,
                bytecode: vec![],
                constants: vec![],
                locals: 0,
                stack_size: 0,
            });
        }
        
        manager.finalize();
        
        assert_eq!(manager.stats().functions_parsed, 3);
        assert_eq!(manager.stats().functions_compiled, 1);
        assert_eq!(manager.stats().functions_never_called, 2);
    }
}
