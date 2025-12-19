//! JavaScript Runtime

use crate::{JsValue, JsError};

/// QuickJS runtime wrapper
pub struct JsRuntime {
    // TODO: QuickJS context
}

impl JsRuntime {
    /// Create a new JavaScript runtime
    pub fn new() -> Result<Self, JsError> {
        tracing::info!("Creating JavaScript runtime");
        Ok(Self {})
    }
    
    /// Evaluate JavaScript code
    pub fn eval(&self, _code: &str) -> Result<JsValue, JsError> {
        // TODO: Implement using rquickjs
        Ok(JsValue::Undefined)
    }
}
