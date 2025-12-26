//! JavaScript Engine Trait
//!
//! Abstract interface for JavaScript engines, allowing pluggable implementations.
//! This enables removing rquickjs dependency and implementing a custom engine.

use crate::{JsValue, JsError};
use std::sync::Arc;

/// Abstract JavaScript engine interface.
///
/// Implementations can be:
/// - `StubEngine`: Minimal implementation for compilation
/// - `CustomEngine`: Full custom JS engine (Phase B)
pub trait JsEngine: Send + Sync {
    /// Evaluate JavaScript code and return the result.
    fn eval(&self, code: &str) -> Result<JsValue, JsError>;
    
    /// Execute JavaScript code, ignoring the result.
    fn exec(&self, code: &str) -> Result<(), JsError>;
    
    /// Run any pending async jobs (promises, etc).
    fn run_pending_jobs(&self) -> Result<(), JsError>;
    
    /// Set memory limit in bytes.
    fn set_memory_limit(&self, bytes: usize);
    
    /// Get current memory usage in bytes.
    fn memory_usage(&self) -> usize;
}

/// Handle to a JavaScript object in the engine.
#[derive(Debug, Clone)]
pub struct JsObjectHandle {
    id: u32,
}

impl JsObjectHandle {
    pub fn new(id: u32) -> Self {
        Self { id }
    }
    
    pub fn id(&self) -> u32 {
        self.id
    }
}

/// Handle to a JavaScript function in the engine.
#[derive(Debug, Clone)]
pub struct JsFunctionHandle {
    id: u32,
}

impl JsFunctionHandle {
    pub fn new(id: u32) -> Self {
        Self { id }
    }
    
    pub fn id(&self) -> u32 {
        self.id
    }
}

/// Context for installing global APIs and executing code.
///
/// Provides methods for registering globals, creating objects,
/// and binding native functions to JavaScript.
pub trait JsContextApi {
    /// Set a global value.
    fn set_global(&self, name: &str, value: JsValue) -> Result<(), JsError>;
    
    /// Get a global value.
    fn get_global(&self, name: &str) -> Result<JsValue, JsError>;
    
    /// Create a new empty object.
    fn create_object(&self) -> Result<JsObjectHandle, JsError>;
    
    /// Set a property on an object.
    fn set_property(&self, obj: &JsObjectHandle, name: &str, value: JsValue) -> Result<(), JsError>;
    
    /// Get a property from an object.
    fn get_property(&self, obj: &JsObjectHandle, name: &str) -> Result<JsValue, JsError>;
    
    /// Register a native function as a property on an object.
    fn set_function<F>(&self, obj: &JsObjectHandle, name: &str, func: F) -> Result<(), JsError>
    where
        F: Fn(&[JsValue]) -> Result<JsValue, JsError> + Send + Sync + 'static;
    
    /// Register a native function as a global.
    fn set_global_function<F>(&self, name: &str, func: F) -> Result<(), JsError>
    where
        F: Fn(&[JsValue]) -> Result<JsValue, JsError> + Send + Sync + 'static;
    
    /// Evaluate code in this context.
    fn eval(&self, code: &str) -> Result<JsValue, JsError>;
}

/// Callback type for native functions exposed to JavaScript.
pub type NativeFunction = Arc<dyn Fn(&[JsValue]) -> Result<JsValue, JsError> + Send + Sync>;

/// Registry for tracking native functions bound to the engine.
#[derive(Default)]
pub struct NativeFunctionRegistry {
    functions: Vec<NativeFunction>,
}

impl NativeFunctionRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a function and return its ID.
    pub fn register<F>(&mut self, func: F) -> u32
    where
        F: Fn(&[JsValue]) -> Result<JsValue, JsError> + Send + Sync + 'static,
    {
        let id = self.functions.len() as u32;
        self.functions.push(Arc::new(func));
        id
    }
    
    /// Get a function by ID.
    pub fn get(&self, id: u32) -> Option<&NativeFunction> {
        self.functions.get(id as usize)
    }
    
    /// Call a function by ID with arguments.
    pub fn call(&self, id: u32, args: &[JsValue]) -> Result<JsValue, JsError> {
        self.functions
            .get(id as usize)
            .ok_or_else(|| JsError::Runtime(format!("Function {} not found", id)))
            .and_then(|f| f(args))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_native_function_registry() {
        let mut registry = NativeFunctionRegistry::new();
        
        let id = registry.register(|args| {
            let sum: f64 = args.iter().filter_map(|v| {
                if let JsValue::Number(n) = v { Some(*n) } else { None }
            }).sum();
            Ok(JsValue::Number(sum))
        });
        
        let result = registry.call(id, &[JsValue::Number(1.0), JsValue::Number(2.0)]).unwrap();
        assert!(matches!(result, JsValue::Number(n) if (n - 3.0).abs() < 0.001));
    }
    
    #[test]
    fn test_object_handle() {
        let handle = JsObjectHandle::new(42);
        assert_eq!(handle.id(), 42);
    }
}
