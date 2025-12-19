//! fOS JavaScript Runtime
//!
//! QuickJS-based JavaScript engine with minimal footprint.

mod runtime;
mod bindings;

pub use runtime::JsRuntime;

/// Execute JavaScript code
pub fn eval(code: &str) -> Result<JsValue, JsError> {
    let runtime = JsRuntime::new()?;
    runtime.eval(code)
}

/// JavaScript value
#[derive(Debug)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Object,
    Array,
    Function,
}

/// JavaScript error
#[derive(Debug, thiserror::Error)]
pub enum JsError {
    #[error("JavaScript error: {0}")]
    Runtime(String),
    
    #[error("Syntax error: {0}")]
    Syntax(String),
    
    #[error("Type error: {0}")]
    TypeError(String),
}
