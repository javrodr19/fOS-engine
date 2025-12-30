//! Console API
//!
//! Implements console.log, console.warn, console.error, etc.

use crate::{JsValue, JsError};
use crate::engine_trait::JsContextApi;

/// Install console API into the global object
pub fn install_console<C: JsContextApi>(ctx: &C) -> Result<(), JsError> {
    let console = ctx.create_object()?;
    
    // console.log
    ctx.set_function(&console, "log", |args| {
        log_with_level("LOG", args);
        Ok(JsValue::Undefined)
    })?;
    
    // console.info
    ctx.set_function(&console, "info", |args| {
        log_with_level("INFO", args);
        Ok(JsValue::Undefined)
    })?;
    
    // console.warn
    ctx.set_function(&console, "warn", |args| {
        log_with_level("WARN", args);
        Ok(JsValue::Undefined)
    })?;
    
    // console.error
    ctx.set_function(&console, "error", |args| {
        log_with_level("ERROR", args);
        Ok(JsValue::Undefined)
    })?;
    
    // console.debug
    ctx.set_function(&console, "debug", |args| {
        log_with_level("DEBUG", args);
        Ok(JsValue::Undefined)
    })?;
    
    ctx.set_global("console", JsValue::Object)?;
    
    Ok(())
}

/// Log values with a specific level
fn log_with_level(level: &str, values: &[JsValue]) {
    let mut output = String::new();
    
    for (i, value) in values.iter().enumerate() {
        if i > 0 {
            output.push(' ');
        }
        format_value(&mut output, value);
    }
    
    match level {
        "ERROR" => tracing::error!("[JS] {}", output),
        "WARN" => tracing::warn!("[JS] {}", output),
        "DEBUG" => tracing::debug!("[JS] {}", output),
        _ => tracing::info!("[JS] {}", output),
    }
}

/// Format a JavaScript value for logging
fn format_value(out: &mut String, value: &JsValue) {
    match value {
        JsValue::Undefined => out.push_str("undefined"),
        JsValue::Null => out.push_str("null"),
        JsValue::Bool(b) => out.push_str(&b.to_string()),
        JsValue::Number(n) => out.push_str(&n.to_string()),
        JsValue::String(s) => out.push_str(s),
        JsValue::Array => out.push_str("[Array]"),
        JsValue::Function => out.push_str("[Function]"),
        JsValue::Object => out.push_str("[Object]"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stub_engine::{StubEngine, StubContext};
    use std::sync::Arc;
    
    #[test]
    fn test_console_install() {
        let engine = Arc::new(StubEngine::new());
        let ctx = StubContext::new(engine);
        
        install_console(&ctx).unwrap();
    }
    
    #[test]
    fn test_format_values() {
        let mut output = String::new();
        format_value(&mut output, &JsValue::String("test".to_string()));
        assert_eq!(output, "test");
        
        output.clear();
        format_value(&mut output, &JsValue::Number(42.0));
        assert_eq!(output, "42");
        
        output.clear();
        format_value(&mut output, &JsValue::Bool(true));
        assert_eq!(output, "true");
    }
}
