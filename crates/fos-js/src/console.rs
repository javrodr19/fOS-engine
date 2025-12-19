//! Console API
//!
//! Implements console.log, console.warn, console.error, etc.

use rquickjs::{Context, Function, Object, Value, Ctx};
use std::fmt::Write;

/// Install console API into the global object
pub fn install_console(ctx: &Ctx) -> Result<(), rquickjs::Error> {
    let globals = ctx.globals();
    
    let console = Object::new(ctx.clone())?;
    
    // console.log
    console.set("log", Function::new(ctx.clone(), |ctx: Ctx, args: rquickjs::function::Rest<Value>| {
        log_with_level("LOG", &ctx, args.0);
        Ok::<(), rquickjs::Error>(())
    })?)?;
    
    // console.info
    console.set("info", Function::new(ctx.clone(), |ctx: Ctx, args: rquickjs::function::Rest<Value>| {
        log_with_level("INFO", &ctx, args.0);
        Ok::<(), rquickjs::Error>(())
    })?)?;
    
    // console.warn
    console.set("warn", Function::new(ctx.clone(), |ctx: Ctx, args: rquickjs::function::Rest<Value>| {
        log_with_level("WARN", &ctx, args.0);
        Ok::<(), rquickjs::Error>(())
    })?)?;
    
    // console.error
    console.set("error", Function::new(ctx.clone(), |ctx: Ctx, args: rquickjs::function::Rest<Value>| {
        log_with_level("ERROR", &ctx, args.0);
        Ok::<(), rquickjs::Error>(())
    })?)?;
    
    // console.debug
    console.set("debug", Function::new(ctx.clone(), |ctx: Ctx, args: rquickjs::function::Rest<Value>| {
        log_with_level("DEBUG", &ctx, args.0);
        Ok::<(), rquickjs::Error>(())
    })?)?;
    
    globals.set("console", console)?;
    
    Ok(())
}

/// Log values with a specific level
fn log_with_level(level: &str, _ctx: &Ctx, values: Vec<Value>) {
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
fn format_value(out: &mut String, value: &Value) {
    if value.is_undefined() {
        out.push_str("undefined");
    } else if value.is_null() {
        out.push_str("null");
    } else if let Some(b) = value.as_bool() {
        write!(out, "{}", b).ok();
    } else if let Some(n) = value.as_int() {
        write!(out, "{}", n).ok();
    } else if let Some(n) = value.as_float() {
        write!(out, "{}", n).ok();
    } else if let Some(s) = value.as_string() {
        if let Ok(s) = s.to_string() {
            out.push_str(&s);
        }
    } else if value.is_array() {
        out.push_str("[Array]");
    } else if value.is_function() {
        out.push_str("[Function]");
    } else if value.is_object() {
        out.push_str("[Object]");
    } else {
        out.push_str("[unknown]");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rquickjs::Runtime;
    
    #[test]
    fn test_console_log() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        
        context.with(|ctx| {
            install_console(&ctx).unwrap();
            let _: Value = ctx.eval("console.log('test message')").unwrap();
        });
    }
    
    #[test]
    fn test_console_multiple_args() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        
        context.with(|ctx| {
            install_console(&ctx).unwrap();
            let _: Value = ctx.eval("console.log('Hello', 42, true)").unwrap();
        });
    }
    
    #[test]
    fn test_console_levels() {
        let runtime = Runtime::new().unwrap();
        let context = Context::full(&runtime).unwrap();
        
        context.with(|ctx| {
            install_console(&ctx).unwrap();
            let _: Value = ctx.eval("console.info('info'); console.warn('warn'); console.error('error'); console.debug('debug')").unwrap();
        });
    }
}
