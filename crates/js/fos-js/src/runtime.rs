//! JavaScript Runtime
//!
//! Internal runtime implementation for evaluation.

use crate::{JsValue, JsError};
use crate::engine_trait::JsEngine;
use crate::stub_engine::StubEngine;
use std::sync::Arc;

/// JavaScript runtime wrapper.
/// This module is kept for backwards compatibility but
/// now delegates to the StubEngine/custom engine.
pub struct JsRuntimeInternal {
    engine: Arc<StubEngine>,
}

impl JsRuntimeInternal {
    /// Create a new JavaScript runtime
    pub fn new() -> Result<Self, JsError> {
        Ok(Self {
            engine: Arc::new(StubEngine::new()),
        })
    }
    
    /// Evaluate JavaScript code and return result
    pub fn eval(&self, code: &str) -> Result<JsValue, JsError> {
        self.engine.eval(code)
    }
    
    /// Evaluate JavaScript and ignore result
    pub fn exec(&self, code: &str) -> Result<(), JsError> {
        self.engine.exec(code)
    }
    
    /// Run pending jobs (for async operations)
    pub fn run_pending_jobs(&self) -> Result<(), JsError> {
        self.engine.run_pending_jobs()
    }
}

impl Default for JsRuntimeInternal {
    fn default() -> Self {
        Self::new().expect("Failed to create JS runtime")
    }
}

/// Convert JsValue to string (for internal use)
pub fn convert_value_pub(value: &JsValue) -> Result<JsValue, JsError> {
    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_runtime() {
        let runtime = JsRuntimeInternal::new();
        assert!(runtime.is_ok());
    }
    
    #[test]
    fn test_eval_number() {
        let runtime = JsRuntimeInternal::new().unwrap();
        let result = runtime.eval("1 + 2").unwrap();
        
        match result {
            JsValue::Number(n) => assert_eq!(n, 3.0),
            _ => panic!("Expected number"),
        }
    }
    
    #[test]
    fn test_eval_string() {
        let runtime = JsRuntimeInternal::new().unwrap();
        let result = runtime.eval("\"hello world\"").unwrap();
        
        match result {
            JsValue::String(s) => assert_eq!(s, "hello world"),
            _ => panic!("Expected string"),
        }
    }
}
