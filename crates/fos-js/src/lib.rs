//! fOS JavaScript Runtime
//!
//! QuickJS-based JavaScript engine with browser APIs.
//!
//! Features:
//! - QuickJS runtime via rquickjs
//! - Console API (log, warn, error)
//! - Timers (setTimeout, setInterval)
//! - DOM bindings (document.getElementById, createElement)
//! - Storage APIs (localStorage, sessionStorage)
//! - Navigation APIs (history, location)

mod runtime;
mod console;
mod timers;
mod bindings;
pub mod storage;
pub mod history;
pub mod location;
pub mod worker;
pub mod media;
pub mod media_bindings;

pub use runtime::JsRuntime;
pub use timers::TimerManager;
pub use storage::Storage;
pub use history::HistoryManager;
pub use location::LocationManager;

use std::sync::{Arc, Mutex};
use std::path::PathBuf;
use fos_dom::Document;

/// Execute JavaScript code
pub fn eval(code: &str) -> Result<JsValue, JsError> {
    let runtime = JsRuntime::new()?;
    runtime.eval(code)
}

/// JavaScript value
#[derive(Debug, Clone)]
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

/// JavaScript context with all browser APIs installed
pub struct JsContext {
    runtime: rquickjs::Runtime,
    context: rquickjs::Context,
    timers: Arc<Mutex<TimerManager>>,
}

impl JsContext {
    /// Create a new JavaScript context with browser APIs
    pub fn new(document: Arc<Mutex<Document>>) -> Result<Self, JsError> {
        Self::with_url(document, "about:blank")
    }
    
    /// Create context with a specific URL
    pub fn with_url(document: Arc<Mutex<Document>>, url: &str) -> Result<Self, JsError> {
        let runtime = rquickjs::Runtime::new().map_err(|e| JsError::Runtime(e.to_string()))?;
        runtime.set_memory_limit(32 * 1024 * 1024);
        
        let context = rquickjs::Context::full(&runtime).map_err(|e| JsError::Runtime(e.to_string()))?;
        let timers = Arc::new(Mutex::new(TimerManager::new()));
        
        // Create storage
        let local_storage = Arc::new(Mutex::new(Storage::session())); // Would be persistent in real use
        let session_storage = Arc::new(Mutex::new(Storage::session()));
        
        // Create history and location
        let history_manager = Arc::new(Mutex::new(HistoryManager::new(url)));
        let location_manager = Arc::new(Mutex::new(
            LocationManager::new(url).unwrap_or_else(|_| LocationManager::new("about:blank").unwrap())
        ));
        
        // Install APIs
        context.with(|ctx| {
            console::install_console(&ctx).map_err(|e| JsError::Runtime(e.to_string()))?;
            timers::install_timers(&ctx, timers.clone()).map_err(|e| JsError::Runtime(e.to_string()))?;
            bindings::install_document(&ctx, document).map_err(|e| JsError::Runtime(e.to_string()))?;
            storage::install_storage(&ctx, local_storage, session_storage).map_err(|e| JsError::Runtime(e.to_string()))?;
            history::install_history(&ctx, history_manager).map_err(|e| JsError::Runtime(e.to_string()))?;
            location::install_location(&ctx, location_manager).map_err(|e| JsError::Runtime(e.to_string()))?;
            Ok::<_, JsError>(())
        })?;
        
        Ok(Self { runtime, context, timers })
    }
    
    /// Evaluate JavaScript code
    pub fn eval(&self, code: &str) -> Result<JsValue, JsError> {
        self.context.with(|ctx| {
            let result: rquickjs::Value = ctx.eval(code).map_err(|e| JsError::Runtime(e.to_string()))?;
            runtime::convert_value_pub(&result)
        })
    }
    
    /// Execute JavaScript (ignore result)
    pub fn exec(&self, code: &str) -> Result<(), JsError> {
        self.context.with(|ctx| {
            let _: rquickjs::Value = ctx.eval(code).map_err(|e| JsError::Runtime(e.to_string()))?;
            Ok(())
        })
    }
    
    /// Process ready timers
    pub fn process_timers(&self) -> Result<(), JsError> {
        let ready = self.timers.lock().unwrap().get_ready_timers();
        
        for timer in ready {
            self.exec(&timer.callback)?;
        }
        
        Ok(())
    }
    
    /// Check if there are pending timers
    pub fn has_pending_timers(&self) -> bool {
        self.timers.lock().unwrap().has_pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_eval_simple() {
        let result = eval("1 + 1").unwrap();
        match result {
            JsValue::Number(n) => assert_eq!(n, 2.0),
            _ => panic!("Expected number"),
        }
    }
    
    #[test]
    fn test_js_context_with_document() {
        let doc = Arc::new(Mutex::new(Document::new("test://page")));
        let ctx = JsContext::new(doc).unwrap();
        
        // Test console is available
        ctx.exec("console.log('Hello from JsContext')").unwrap();
        
        // Test document is available
        let result = ctx.eval("typeof document").unwrap();
        match result {
            JsValue::String(s) => assert_eq!(s, "object"),
            _ => panic!("Expected string"),
        }
    }
    
    #[test]
    fn test_storage_api() {
        let doc = Arc::new(Mutex::new(Document::new("test://page")));
        let ctx = JsContext::new(doc).unwrap();
        
        ctx.exec("localStorage.setItem('key', 'value')").unwrap();
        let result = ctx.eval("localStorage.getItem('key')").unwrap();
        match result {
            JsValue::String(s) => assert_eq!(s, "value"),
            _ => panic!("Expected string"),
        }
    }
    
    #[test]
    fn test_history_api() {
        let doc = Arc::new(Mutex::new(Document::new("test://page")));
        let ctx = JsContext::new(doc).unwrap();
        
        ctx.exec("history.pushState(null, '', '/page1')").unwrap();
        ctx.exec("history.pushState(null, '', '/page2')").unwrap();
        
        let result = ctx.eval("history.getLength()").unwrap();
        match result {
            JsValue::Number(n) => assert_eq!(n, 3.0), // initial + 2 pushes
            _ => panic!("Expected number"),
        }
    }
    
    #[test]
    fn test_location_api() {
        let doc = Arc::new(Mutex::new(Document::new("test://page")));
        let ctx = JsContext::with_url(doc, "https://example.com/path").unwrap();
        
        let result = ctx.eval("location.getHostname()").unwrap();
        match result {
            JsValue::String(s) => assert_eq!(s, "example.com"),
            _ => panic!("Expected string"),
        }
    }
}
