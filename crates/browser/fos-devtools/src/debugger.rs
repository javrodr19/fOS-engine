//! Debugger
//!
//! JavaScript debugger with breakpoints.

use std::collections::{HashMap, HashSet};

/// Debugger state
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DebuggerState {
    #[default]
    Running,
    Paused,
    Stepping,
}

/// Breakpoint
#[derive(Debug, Clone)]
pub struct Breakpoint {
    pub id: u64,
    pub url: String,
    pub line: u32,
    pub column: Option<u32>,
    pub condition: Option<String>,
    pub enabled: bool,
    pub hit_count: u32,
}

/// Call frame
#[derive(Debug, Clone)]
pub struct CallFrame {
    pub id: u64,
    pub function_name: String,
    pub url: String,
    pub line: u32,
    pub column: u32,
    pub scope_chain: Vec<Scope>,
    pub this_value: Option<String>,
}

/// Scope
#[derive(Debug, Clone)]
pub struct Scope {
    pub scope_type: ScopeType,
    pub name: Option<String>,
    pub variables: HashMap<String, VariableValue>,
}

/// Scope type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeType {
    Global,
    Local,
    Closure,
    Block,
    With,
    Catch,
}

/// Variable value
#[derive(Debug, Clone)]
pub struct VariableValue {
    pub name: String,
    pub value_type: String,
    pub value: String,
    pub expandable: bool,
}

/// Debugger
#[derive(Debug, Default)]
pub struct Debugger {
    state: DebuggerState,
    breakpoints: HashMap<u64, Breakpoint>,
    next_bp_id: u64,
    call_stack: Vec<CallFrame>,
    pause_on_exceptions: bool,
    pause_on_caught_exceptions: bool,
}

impl Debugger {
    pub fn new() -> Self { Self::default() }
    
    /// Get state
    pub fn state(&self) -> DebuggerState {
        self.state
    }
    
    /// Add breakpoint
    pub fn add_breakpoint(&mut self, url: &str, line: u32) -> u64 {
        let id = self.next_bp_id;
        self.next_bp_id += 1;
        
        let bp = Breakpoint {
            id,
            url: url.to_string(),
            line,
            column: None,
            condition: None,
            enabled: true,
            hit_count: 0,
        };
        
        self.breakpoints.insert(id, bp);
        id
    }
    
    /// Add conditional breakpoint
    pub fn add_conditional_breakpoint(&mut self, url: &str, line: u32, condition: &str) -> u64 {
        let id = self.add_breakpoint(url, line);
        if let Some(bp) = self.breakpoints.get_mut(&id) {
            bp.condition = Some(condition.to_string());
        }
        id
    }
    
    /// Remove breakpoint
    pub fn remove_breakpoint(&mut self, id: u64) {
        self.breakpoints.remove(&id);
    }
    
    /// Enable/disable breakpoint
    pub fn set_breakpoint_enabled(&mut self, id: u64, enabled: bool) {
        if let Some(bp) = self.breakpoints.get_mut(&id) {
            bp.enabled = enabled;
        }
    }
    
    /// Get breakpoints
    pub fn get_breakpoints(&self) -> Vec<&Breakpoint> {
        self.breakpoints.values().collect()
    }
    
    /// Check if should pause at location
    pub fn should_pause(&self, url: &str, line: u32) -> bool {
        self.breakpoints.values().any(|bp| {
            bp.enabled && bp.url == url && bp.line == line
        })
    }
    
    /// Pause execution
    pub fn pause(&mut self) {
        self.state = DebuggerState::Paused;
    }
    
    /// Resume execution
    pub fn resume(&mut self) {
        self.state = DebuggerState::Running;
    }
    
    /// Step over
    pub fn step_over(&mut self) {
        self.state = DebuggerState::Stepping;
    }
    
    /// Step into
    pub fn step_into(&mut self) {
        self.state = DebuggerState::Stepping;
    }
    
    /// Step out
    pub fn step_out(&mut self) {
        self.state = DebuggerState::Stepping;
    }
    
    /// Set call stack
    pub fn set_call_stack(&mut self, stack: Vec<CallFrame>) {
        self.call_stack = stack;
    }
    
    /// Get call stack
    pub fn get_call_stack(&self) -> &[CallFrame] {
        &self.call_stack
    }
    
    /// Set pause on exceptions
    pub fn set_pause_on_exceptions(&mut self, pause: bool, include_caught: bool) {
        self.pause_on_exceptions = pause;
        self.pause_on_caught_exceptions = include_caught;
    }
    
    /// Evaluate expression
    pub fn evaluate(&self, expression: &str, frame_id: Option<u64>) -> Result<VariableValue, String> {
        // Would evaluate in JS context
        Ok(VariableValue {
            name: expression.to_string(),
            value_type: "undefined".to_string(),
            value: "undefined".to_string(),
            expandable: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_debugger() {
        let mut dbg = Debugger::new();
        
        let bp_id = dbg.add_breakpoint("script.js", 10);
        assert!(dbg.should_pause("script.js", 10));
        
        dbg.pause();
        assert_eq!(dbg.state(), DebuggerState::Paused);
        
        dbg.resume();
        assert_eq!(dbg.state(), DebuggerState::Running);
    }
}
