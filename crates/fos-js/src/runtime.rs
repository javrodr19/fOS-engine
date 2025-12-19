//! JavaScript Runtime
//!
//! QuickJS-based runtime using rquickjs.

use crate::{JsValue, JsError};
use rquickjs::{Runtime, Context, Function, Value, Object};
use std::sync::Arc;

/// QuickJS runtime wrapper
pub struct JsRuntime {
    runtime: Runtime,
    context: Context,
}

impl JsRuntime {
    /// Create a new JavaScript runtime
    pub fn new() -> Result<Self, JsError> {
        tracing::info!("Creating JavaScript runtime");
        
        let runtime = Runtime::new().map_err(|e| JsError::Runtime(e.to_string()))?;
        
        // Limit memory to 32MB
        runtime.set_memory_limit(32 * 1024 * 1024);
        
        let context = Context::full(&runtime).map_err(|e| JsError::Runtime(e.to_string()))?;
        
        Ok(Self { runtime, context })
    }
    
    /// Create runtime with custom memory limit (in bytes)
    pub fn with_memory_limit(limit: usize) -> Result<Self, JsError> {
        let runtime = Runtime::new().map_err(|e| JsError::Runtime(e.to_string()))?;
        runtime.set_memory_limit(limit);
        let context = Context::full(&runtime).map_err(|e| JsError::Runtime(e.to_string()))?;
        Ok(Self { runtime, context })
    }
    
    /// Evaluate JavaScript code and return result
    pub fn eval(&self, code: &str) -> Result<JsValue, JsError> {
        self.context.with(|ctx| {
            let result: Value = ctx.eval(code).map_err(|e| JsError::Runtime(e.to_string()))?;
            convert_value(&result)
        })
    }
    
    /// Evaluate JavaScript and ignore result
    pub fn exec(&self, code: &str) -> Result<(), JsError> {
        self.context.with(|ctx| {
            let _: Value = ctx.eval(code).map_err(|e| JsError::Runtime(e.to_string()))?;
            Ok(())
        })
    }
    
    /// Get global object
    pub fn global<F, R>(&self, f: F) -> R 
    where
        F: FnOnce(&Object) -> R,
    {
        self.context.with(|ctx| {
            let global = ctx.globals();
            f(&global)
        })
    }
    
    /// Register a global function
    pub fn register_function<'js, F>(&self, name: &str, func: F) -> Result<(), JsError>
    where
        F: Fn(Vec<JsValue>) -> JsValue + 'static,
    {
        // Note: This is a simplified version - full implementation would need proper lifetime handling
        self.context.with(|ctx| {
            // For now, we'll add functions through eval
            Ok(())
        })
    }
    
    /// Run pending jobs (for async operations)
    pub fn run_pending_jobs(&self) -> Result<(), JsError> {
        loop {
            match self.runtime.execute_pending_job() {
                Ok(false) => break, // No more jobs
                Ok(true) => continue,
                Err(e) => return Err(JsError::Runtime(e.to_string())),
            }
        }
        Ok(())
    }
}

/// Convert rquickjs Value to our JsValue
fn convert_value(value: &Value) -> Result<JsValue, JsError> {
    convert_value_inner(value)
}

/// Public version for lib.rs
pub fn convert_value_pub(value: &Value) -> Result<JsValue, JsError> {
    convert_value_inner(value)
}

fn convert_value_inner(value: &Value) -> Result<JsValue, JsError> {
    if value.is_undefined() {
        Ok(JsValue::Undefined)
    } else if value.is_null() {
        Ok(JsValue::Null)
    } else if let Some(b) = value.as_bool() {
        Ok(JsValue::Bool(b))
    } else if let Some(n) = value.as_int() {
        Ok(JsValue::Number(n as f64))
    } else if let Some(n) = value.as_float() {
        Ok(JsValue::Number(n))
    } else if let Some(s) = value.as_string() {
        Ok(JsValue::String(s.to_string().unwrap_or_default()))
    } else if value.is_array() {
        Ok(JsValue::Array)
    } else if value.is_function() {
        Ok(JsValue::Function)
    } else if value.is_object() {
        Ok(JsValue::Object)
    } else {
        Ok(JsValue::Undefined)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_runtime() {
        let runtime = JsRuntime::new();
        assert!(runtime.is_ok());
    }
    
    #[test]
    fn test_eval_number() {
        let runtime = JsRuntime::new().unwrap();
        let result = runtime.eval("1 + 2").unwrap();
        
        match result {
            JsValue::Number(n) => assert_eq!(n, 3.0),
            _ => panic!("Expected number"),
        }
    }
    
    #[test]
    fn test_eval_string() {
        let runtime = JsRuntime::new().unwrap();
        let result = runtime.eval("'hello' + ' ' + 'world'").unwrap();
        
        match result {
            JsValue::String(s) => assert_eq!(s, "hello world"),
            _ => panic!("Expected string"),
        }
    }
    
    #[test]
    fn test_eval_bool() {
        let runtime = JsRuntime::new().unwrap();
        
        let result = runtime.eval("true").unwrap();
        assert!(matches!(result, JsValue::Bool(true)));
        
        let result = runtime.eval("false").unwrap();
        assert!(matches!(result, JsValue::Bool(false)));
    }
    
    #[test]
    fn test_eval_undefined() {
        let runtime = JsRuntime::new().unwrap();
        let result = runtime.eval("undefined").unwrap();
        assert!(matches!(result, JsValue::Undefined));
    }
    
    #[test]
    fn test_eval_null() {
        let runtime = JsRuntime::new().unwrap();
        let result = runtime.eval("null").unwrap();
        assert!(matches!(result, JsValue::Null));
    }
    
    #[test]
    fn test_eval_object() {
        let runtime = JsRuntime::new().unwrap();
        let result = runtime.eval("({a: 1, b: 2})").unwrap();
        assert!(matches!(result, JsValue::Object));
    }
    
    #[test]
    fn test_eval_array() {
        let runtime = JsRuntime::new().unwrap();
        let result = runtime.eval("[1, 2, 3]").unwrap();
        assert!(matches!(result, JsValue::Array));
    }
    
    #[test]
    fn test_eval_function() {
        let runtime = JsRuntime::new().unwrap();
        let result = runtime.eval("(function() {})").unwrap();
        assert!(matches!(result, JsValue::Function));
    }
    
    #[test]
    fn test_eval_expression() {
        let runtime = JsRuntime::new().unwrap();
        
        // Complex expression
        let result = runtime.eval("(function() { var x = 10; return x * 2; })()").unwrap();
        match result {
            JsValue::Number(n) => assert_eq!(n, 20.0),
            _ => panic!("Expected number"),
        }
    }
    
    #[test]
    fn test_syntax_error() {
        let runtime = JsRuntime::new().unwrap();
        let result = runtime.eval("function {");
        assert!(result.is_err());
    }
    
    #[test]
    fn test_exec() {
        let runtime = JsRuntime::new().unwrap();
        let result = runtime.exec("var x = 42;");
        assert!(result.is_ok());
    }
}
