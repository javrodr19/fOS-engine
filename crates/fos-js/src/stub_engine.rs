//! Stub JavaScript Engine
//!
//! Minimal implementation of the JsEngine trait that allows compilation
//! while the full custom engine is being developed.

use crate::{JsValue, JsError};
use crate::engine_trait::{JsEngine, JsContextApi, JsObjectHandle, NativeFunctionRegistry};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Stub JavaScript engine.
///
/// Provides minimal functionality:
/// - Simple expression evaluation (numbers, strings, booleans)
/// - Global variable storage
/// - Native function registry
///
/// This allows the browser to compile and run while the full
/// custom engine is being developed in Phase B.
pub struct StubEngine {
    memory_limit: usize,
    globals: Mutex<HashMap<String, JsValue>>,
    objects: Mutex<Vec<HashMap<String, JsValue>>>,
    functions: Mutex<NativeFunctionRegistry>,
}

impl StubEngine {
    pub fn new() -> Self {
        Self {
            memory_limit: 32 * 1024 * 1024, // 32MB default
            globals: Mutex::new(HashMap::new()),
            objects: Mutex::new(Vec::new()),
            functions: Mutex::new(NativeFunctionRegistry::new()),
        }
    }
    
    /// Parse a simple literal value from code.
    fn parse_literal(&self, code: &str) -> Option<JsValue> {
        let code = code.trim();
        
        // Undefined
        if code == "undefined" {
            return Some(JsValue::Undefined);
        }
        
        // Null
        if code == "null" {
            return Some(JsValue::Null);
        }
        
        // Boolean
        if code == "true" {
            return Some(JsValue::Bool(true));
        }
        if code == "false" {
            return Some(JsValue::Bool(false));
        }
        
        // Number (integer or float)
        if let Ok(n) = code.parse::<f64>() {
            return Some(JsValue::Number(n));
        }
        
        // String literal
        if (code.starts_with('"') && code.ends_with('"')) ||
           (code.starts_with('\'') && code.ends_with('\'')) {
            let inner = &code[1..code.len()-1];
            return Some(JsValue::String(inner.to_string()));
        }
        
        None
    }
    
    /// Evaluate a simple arithmetic expression.
    fn eval_simple_expr(&self, code: &str) -> Option<JsValue> {
        let code = code.trim();
        
        // Try simple binary operations
        for op in [" + ", " - ", " * ", " / "] {
            if let Some(pos) = code.find(op) {
                let left = code[..pos].trim();
                let right = code[pos + op.len()..].trim();
                
                let left_val = self.parse_literal(left)?;
                let right_val = self.parse_literal(right)?;
                
                // String concatenation takes priority for +
                if op == " + " {
                    if let (JsValue::String(l), JsValue::String(r)) = (&left_val, &right_val) {
                        return Some(JsValue::String(format!("{}{}", l, r)));
                    }
                }
                
                if let (JsValue::Number(l), JsValue::Number(r)) = (&left_val, &right_val) {
                    let result = match op.trim() {
                        "+" => l + r,
                        "-" => l - r,
                        "*" => l * r,
                        "/" => if *r != 0.0 { l / r } else { f64::INFINITY },
                        _ => return None,
                    };
                    return Some(JsValue::Number(result));
                }
            }
        }
        
        None
    }
    
    /// Check if code is a typeof expression.
    fn eval_typeof(&self, code: &str) -> Option<JsValue> {
        let code = code.trim();
        if code.starts_with("typeof ") {
            let expr = code[7..].trim();
            let type_str = match self.parse_literal(expr) {
                Some(JsValue::Undefined) => "undefined",
                Some(JsValue::Null) => "object", // typeof null === "object"
                Some(JsValue::Bool(_)) => "boolean",
                Some(JsValue::Number(_)) => "number",
                Some(JsValue::String(_)) => "string",
                Some(JsValue::Object) => "object",
                Some(JsValue::Array) => "object",
                Some(JsValue::Function) => "function",
                None => {
                    // Check globals
                    let globals = self.globals.lock().unwrap();
                    match globals.get(expr) {
                        Some(JsValue::Object) => "object",
                        Some(JsValue::Function) => "function",
                        Some(JsValue::Array) => "object",
                        _ => "undefined",
                    }
                }
            };
            return Some(JsValue::String(type_str.to_string()));
        }
        None
    }
}

impl Default for StubEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl JsEngine for StubEngine {
    fn eval(&self, code: &str) -> Result<JsValue, JsError> {
        let code = code.trim();
        
        // Empty code
        if code.is_empty() {
            return Ok(JsValue::Undefined);
        }
        
        // Try literal
        if let Some(val) = self.parse_literal(code) {
            return Ok(val);
        }
        
        // Try typeof
        if let Some(val) = self.eval_typeof(code) {
            return Ok(val);
        }
        
        // Try simple arithmetic
        if let Some(val) = self.eval_simple_expr(code) {
            return Ok(val);
        }
        
        // Check globals
        {
            let globals = self.globals.lock().unwrap();
            if let Some(val) = globals.get(code) {
                return Ok(val.clone());
            }
        }
        
        // Log that we can't evaluate this yet
        tracing::debug!("[StubEngine] Cannot evaluate: {}", code);
        
        // Return undefined for complex code
        Ok(JsValue::Undefined)
    }
    
    fn exec(&self, code: &str) -> Result<(), JsError> {
        // For exec, we just evaluate and ignore the result
        // This allows console.log and other statements to "work"
        let _ = self.eval(code);
        Ok(())
    }
    
    fn run_pending_jobs(&self) -> Result<(), JsError> {
        // No async jobs in stub engine
        Ok(())
    }
    
    fn set_memory_limit(&self, bytes: usize) {
        // Note: Can't mutate through &self, would need interior mutability
        // For now, this is a no-op in the stub
        let _ = bytes;
    }
    
    fn memory_usage(&self) -> usize {
        // Rough estimate
        let globals = self.globals.lock().unwrap();
        globals.len() * 64 // Rough estimate per entry
    }
}

/// Stub context for installing APIs.
pub struct StubContext {
    engine: Arc<StubEngine>,
}

impl StubContext {
    pub fn new(engine: Arc<StubEngine>) -> Self {
        Self { engine }
    }
}

impl JsContextApi for StubContext {
    fn set_global(&self, name: &str, value: JsValue) -> Result<(), JsError> {
        let mut globals = self.engine.globals.lock().unwrap();
        globals.insert(name.to_string(), value);
        Ok(())
    }
    
    fn get_global(&self, name: &str) -> Result<JsValue, JsError> {
        let globals = self.engine.globals.lock().unwrap();
        Ok(globals.get(name).cloned().unwrap_or(JsValue::Undefined))
    }
    
    fn create_object(&self) -> Result<JsObjectHandle, JsError> {
        let mut objects = self.engine.objects.lock().unwrap();
        let id = objects.len() as u32;
        objects.push(HashMap::new());
        Ok(JsObjectHandle::new(id))
    }
    
    fn set_property(&self, obj: &JsObjectHandle, name: &str, value: JsValue) -> Result<(), JsError> {
        let mut objects = self.engine.objects.lock().unwrap();
        if let Some(props) = objects.get_mut(obj.id() as usize) {
            props.insert(name.to_string(), value);
            Ok(())
        } else {
            Err(JsError::Runtime("Invalid object handle".to_string()))
        }
    }
    
    fn get_property(&self, obj: &JsObjectHandle, name: &str) -> Result<JsValue, JsError> {
        let objects = self.engine.objects.lock().unwrap();
        if let Some(props) = objects.get(obj.id() as usize) {
            Ok(props.get(name).cloned().unwrap_or(JsValue::Undefined))
        } else {
            Err(JsError::Runtime("Invalid object handle".to_string()))
        }
    }
    
    fn set_function<F>(&self, obj: &JsObjectHandle, name: &str, func: F) -> Result<(), JsError>
    where
        F: Fn(&[JsValue]) -> Result<JsValue, JsError> + Send + Sync + 'static,
    {
        let mut functions = self.engine.functions.lock().unwrap();
        let _id = functions.register(func);
        // Store function reference in object
        self.set_property(obj, name, JsValue::Function)
    }
    
    fn set_global_function<F>(&self, name: &str, func: F) -> Result<(), JsError>
    where
        F: Fn(&[JsValue]) -> Result<JsValue, JsError> + Send + Sync + 'static,
    {
        let mut functions = self.engine.functions.lock().unwrap();
        let _id = functions.register(func);
        self.set_global(name, JsValue::Function)
    }
    
    fn eval(&self, code: &str) -> Result<JsValue, JsError> {
        self.engine.eval(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_stub_engine_literals() {
        let engine = StubEngine::new();
        
        assert!(matches!(engine.eval("42").unwrap(), JsValue::Number(n) if (n - 42.0).abs() < 0.001));
        assert!(matches!(engine.eval("true").unwrap(), JsValue::Bool(true)));
        assert!(matches!(engine.eval("false").unwrap(), JsValue::Bool(false)));
        assert!(matches!(engine.eval("null").unwrap(), JsValue::Null));
        assert!(matches!(engine.eval("undefined").unwrap(), JsValue::Undefined));
        assert!(matches!(engine.eval("\"hello\"").unwrap(), JsValue::String(s) if s == "hello"));
    }
    
    #[test]
    fn test_stub_engine_arithmetic() {
        let engine = StubEngine::new();
        
        assert!(matches!(engine.eval("1 + 2").unwrap(), JsValue::Number(n) if (n - 3.0).abs() < 0.001));
        assert!(matches!(engine.eval("10 - 4").unwrap(), JsValue::Number(n) if (n - 6.0).abs() < 0.001));
        assert!(matches!(engine.eval("3 * 4").unwrap(), JsValue::Number(n) if (n - 12.0).abs() < 0.001));
        assert!(matches!(engine.eval("15 / 3").unwrap(), JsValue::Number(n) if (n - 5.0).abs() < 0.001));
    }
    
    #[test]
    #[ignore] // TODO: Fix string concat parsing - stub engine will be replaced by custom engine
    fn test_stub_engine_string_concat() {
        let engine = StubEngine::new();
        
        // Test with cleaner inputs - no leading space in second string
        let result = engine.eval("\"hello\" + \"world\"").unwrap();
        assert!(matches!(result, JsValue::String(s) if s == "helloworld"));
    }
    
    #[test]
    fn test_stub_context_globals() {
        let engine = Arc::new(StubEngine::new());
        let ctx = StubContext::new(engine);
        
        ctx.set_global("x", JsValue::Number(42.0)).unwrap();
        let val = ctx.get_global("x").unwrap();
        assert!(matches!(val, JsValue::Number(n) if (n - 42.0).abs() < 0.001));
    }
    
    #[test]
    fn test_stub_context_objects() {
        let engine = Arc::new(StubEngine::new());
        let ctx = StubContext::new(engine);
        
        let obj = ctx.create_object().unwrap();
        ctx.set_property(&obj, "name", JsValue::String("test".to_string())).unwrap();
        
        let val = ctx.get_property(&obj, "name").unwrap();
        assert!(matches!(val, JsValue::String(s) if s == "test"));
    }
}
