//! Custom JavaScript Engine
//!
//! Full implementation of JsEngine trait using the custom lexer, parser, compiler, and VM.

use crate::{JsValue, JsError};
use crate::engine_trait::{JsEngine, JsContextApi, JsObjectHandle, NativeFunctionRegistry};
use super::lexer::Lexer;
use super::parser::Parser;
use super::compiler::Compiler;
use super::vm::VirtualMachine;
use super::value::JsVal;
use std::collections::HashMap;
use std::sync::Mutex;

/// Custom JavaScript engine implementation.
///
/// Uses the custom lexer, parser, bytecode compiler, and VM.
pub struct CustomEngine {
    vm: Mutex<VirtualMachine>,
    memory_limit: usize,
    globals: Mutex<HashMap<String, JsValue>>,
    objects: Mutex<Vec<HashMap<String, JsValue>>>,
    functions: Mutex<NativeFunctionRegistry>,
}

impl Default for CustomEngine {
    fn default() -> Self { Self::new() }
}

impl CustomEngine {
    pub fn new() -> Self {
        Self {
            vm: Mutex::new(VirtualMachine::new()),
            memory_limit: 32 * 1024 * 1024,
            globals: Mutex::new(HashMap::new()),
            objects: Mutex::new(Vec::new()),
            functions: Mutex::new(NativeFunctionRegistry::new()),
        }
    }
    
    /// Convert internal JsVal to external JsValue
    fn convert_value(val: &JsVal) -> JsValue {
        use super::value::JsValKind::*;
        match val.kind() {
            Undefined => JsValue::Undefined,
            Null => JsValue::Null,
            Bool(b) => JsValue::Bool(b),
            Number(n) => JsValue::Number(n),
            String(s) => JsValue::String(s.to_string()),
            Object(_) => JsValue::Object,
            Array(_) => JsValue::Array,
            Function(_) => JsValue::Function,
        }
    }
}

impl JsEngine for CustomEngine {
    fn eval(&self, code: &str) -> Result<JsValue, JsError> {
        // Parse
        let parser = Parser::new(code);
        let ast = parser.parse().map_err(|e| JsError::Syntax(e.message))?;
        
        // Compile
        let compiler = Compiler::new();
        let bytecode = compiler.compile(&ast).map_err(|e| JsError::Runtime(e))?;
        
        // Execute
        let mut vm = self.vm.lock().unwrap();
        let result = vm.run(&bytecode).map_err(|e| JsError::Runtime(e))?;
        
        Ok(Self::convert_value(&result))
    }
    
    fn exec(&self, code: &str) -> Result<(), JsError> {
        let _ = self.eval(code)?;
        Ok(())
    }
    
    fn run_pending_jobs(&self) -> Result<(), JsError> {
        // No async jobs yet
        Ok(())
    }
    
    fn set_memory_limit(&self, _bytes: usize) {
        // TODO: Implement memory limits in VM
    }
    
    fn memory_usage(&self) -> usize {
        // TODO: Track actual memory usage
        0
    }
}

/// Custom context for installing APIs
pub struct CustomContext {
    engine: std::sync::Arc<CustomEngine>,
}

impl CustomContext {
    pub fn new(engine: std::sync::Arc<CustomEngine>) -> Self {
        Self { engine }
    }
}

impl JsContextApi for CustomContext {
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
    fn test_custom_engine_literals() {
        let engine = CustomEngine::new();
        
        assert!(matches!(engine.eval("42;").unwrap(), JsValue::Number(n) if (n - 42.0).abs() < 0.001));
        assert!(matches!(engine.eval("true;").unwrap(), JsValue::Bool(true)));
        assert!(matches!(engine.eval("false;").unwrap(), JsValue::Bool(false)));
        assert!(matches!(engine.eval("null;").unwrap(), JsValue::Null));
    }
    
    #[test]
    fn test_custom_engine_arithmetic() {
        let engine = CustomEngine::new();
        
        assert!(matches!(engine.eval("1 + 2;").unwrap(), JsValue::Number(n) if (n - 3.0).abs() < 0.001));
        assert!(matches!(engine.eval("10 - 4;").unwrap(), JsValue::Number(n) if (n - 6.0).abs() < 0.001));
        assert!(matches!(engine.eval("3 * 4;").unwrap(), JsValue::Number(n) if (n - 12.0).abs() < 0.001));
        assert!(matches!(engine.eval("15 / 3;").unwrap(), JsValue::Number(n) if (n - 5.0).abs() < 0.001));
    }
    
    #[test]
    fn test_custom_engine_variables() {
        let engine = CustomEngine::new();
        
        // Variable declaration and usage
        let result = engine.eval("let x = 10; x;").unwrap();
        assert!(matches!(result, JsValue::Number(n) if (n - 10.0).abs() < 0.001));
    }
    
    #[test]
    fn test_custom_context() {
        use std::sync::Arc;
        let engine = Arc::new(CustomEngine::new());
        let ctx = CustomContext::new(engine);
        
        ctx.set_global("x", JsValue::Number(42.0)).unwrap();
        let val = ctx.get_global("x").unwrap();
        assert!(matches!(val, JsValue::Number(n) if (n - 42.0).abs() < 0.001));
    }
}
