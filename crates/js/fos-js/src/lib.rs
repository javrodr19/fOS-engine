//! fOS JavaScript Runtime
//!
//! Custom JavaScript engine with browser APIs.
//!
//! Features:
//! - Pluggable engine architecture (trait-based)
//! - Console API (log, warn, error)
//! - Timers (setTimeout, setInterval)
//! - DOM bindings (document.getElementById, createElement)
//! - Storage APIs (localStorage, sessionStorage, IndexedDB)
//! - Navigation APIs (history, location)
//! - Input events (keyboard, mouse, focus, clipboard)
//! - Built-in objects (Promise, Map, Set, Symbol, Proxy)
//! - Web APIs (URL, Blob, TextEncoder, AbortController, Geolocation)

mod engine_trait;
mod stub_engine;
mod console;
mod timers;
mod bindings;
mod runtime;
pub mod engine;
pub mod storage;
pub mod history;
pub mod location;
pub mod worker;
pub mod media;
pub mod media_bindings;
pub mod events;
pub mod builtins;
pub mod webapi;
pub mod idb;
pub mod js_optimizations;

// Phase B modules (custom engine)
pub mod lazy_compile;
pub mod const_fold;
pub mod escape_analysis;
pub mod jit_bytecode;

pub use engine_trait::{JsEngine, JsContextApi, JsObjectHandle, JsFunctionHandle, NativeFunctionRegistry};
pub use stub_engine::{StubEngine, StubContext};
pub use timers::TimerManager;
pub use storage::Storage;
pub use history::HistoryManager;
pub use location::LocationManager;
pub use events::{
    KeyboardEvent, Key, KeyModifiers, MouseEvent, MouseButton,
    FocusEvent, FocusManager, ClipboardEvent, ClipboardData,
    TouchEvent, Touch, DragEvent, DataTransfer,
};
pub use builtins::{JsPromise, PromiseState, JsMap, JsSet, JsSymbol, JsProxy, JsBigInt, JsWeakRef, SharedArrayBuffer, AsyncModule, TlaModuleGraph};
pub use webapi::{JsUrl, JsUrlSearchParams, TextEncoder, TextDecoder, Blob, File, AbortController, Geolocation, Notification, Permissions, FormData, FileReader};
pub use idb::{IDBFactory, IDBDatabase, CacheStorage, CookieStore};
pub use js_optimizations::{LazyCompiler, ConstantFolder, EscapeAnalyzer, BytecodeCache, HeapCompressor, SharedBuiltins};

use std::sync::{Arc, Mutex};
use fos_dom::Document;

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

impl JsValue {
    /// Convert value to string representation
    pub fn to_string_repr(&self) -> String {
        match self {
            JsValue::Undefined => "undefined".to_string(),
            JsValue::Null => "null".to_string(),
            JsValue::Bool(b) => b.to_string(),
            JsValue::Number(n) => n.to_string(),
            JsValue::String(s) => s.clone(),
            JsValue::Object => "[object Object]".to_string(),
            JsValue::Array => "[Array]".to_string(),
            JsValue::Function => "[Function]".to_string(),
        }
    }
    
    /// Try to get as number
    pub fn as_number(&self) -> Option<f64> {
        match self {
            JsValue::Number(n) => Some(*n),
            _ => None,
        }
    }
    
    /// Try to get as string
    pub fn as_string(&self) -> Option<&str> {
        match self {
            JsValue::String(s) => Some(s),
            _ => None,
        }
    }
    
    /// Try to get as bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            JsValue::Bool(b) => Some(*b),
            _ => None,
        }
    }
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

/// Execute JavaScript code
pub fn eval(code: &str) -> Result<JsValue, JsError> {
    let engine = StubEngine::new();
    engine.eval(code)
}

/// JavaScript runtime wrapper
pub struct JsRuntime {
    engine: Arc<StubEngine>,
}

impl JsRuntime {
    /// Create a new JavaScript runtime
    pub fn new() -> Result<Self, JsError> {
        tracing::info!("Creating JavaScript runtime");
        Ok(Self {
            engine: Arc::new(StubEngine::new()),
        })
    }
    
    /// Create runtime with custom memory limit (in bytes)
    pub fn with_memory_limit(limit: usize) -> Result<Self, JsError> {
        let engine = Arc::new(StubEngine::new());
        engine.set_memory_limit(limit);
        Ok(Self { engine })
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

impl Default for JsRuntime {
    fn default() -> Self {
        Self::new().expect("Failed to create JS runtime")
    }
}

/// JavaScript context with all browser APIs installed
pub struct JsContext {
    engine: Arc<StubEngine>,
    context: StubContext,
    timers: Arc<Mutex<TimerManager>>,
}

impl JsContext {
    /// Create a new JavaScript context with browser APIs
    pub fn new(document: Arc<Mutex<Document>>) -> Result<Self, JsError> {
        Self::with_url(document, "about:blank")
    }
    
    /// Create context with a specific URL
    pub fn with_url(document: Arc<Mutex<Document>>, url: &str) -> Result<Self, JsError> {
        let engine = Arc::new(StubEngine::new());
        let context = StubContext::new(engine.clone());
        let timers = Arc::new(Mutex::new(TimerManager::new()));
        
        // Create storage
        let local_storage = Arc::new(Mutex::new(Storage::session()));
        let session_storage = Arc::new(Mutex::new(Storage::session()));
        
        // Create history and location
        let history_manager = Arc::new(Mutex::new(HistoryManager::new(url)));
        let location_manager = Arc::new(Mutex::new(
            LocationManager::new(url).unwrap_or_else(|_| LocationManager::new("about:blank").unwrap())
        ));
        
        // Install APIs using abstract interface
        console::install_console(&context)?;
        timers::install_timers(&context, timers.clone())?;
        bindings::install_document(&context, document)?;
        storage::install_storage(&context, local_storage, session_storage)?;
        history::install_history(&context, history_manager)?;
        location::install_location(&context, location_manager)?;
        
        Ok(Self { engine, context, timers })
    }
    
    /// Evaluate JavaScript code
    pub fn eval(&self, code: &str) -> Result<JsValue, JsError> {
        self.engine.eval(code)
    }
    
    /// Execute JavaScript (ignore result)
    pub fn exec(&self, code: &str) -> Result<(), JsError> {
        self.engine.exec(code)
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
    fn test_eval_string() {
        let result = eval("\"hello\"").unwrap();
        match result {
            JsValue::String(s) => assert_eq!(s, "hello"),
            _ => panic!("Expected string"),
        }
    }
    
    #[test]
    fn test_eval_bool() {
        assert!(matches!(eval("true").unwrap(), JsValue::Bool(true)));
        assert!(matches!(eval("false").unwrap(), JsValue::Bool(false)));
    }
    
    #[test]
    fn test_js_runtime() {
        let runtime = JsRuntime::new().unwrap();
        let result = runtime.eval("10 * 5").unwrap();
        match result {
            JsValue::Number(n) => assert_eq!(n, 50.0),
            _ => panic!("Expected number"),
        }
    }
}
