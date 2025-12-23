//! JavaScript builtins integration
//!
//! Advanced JS built-in objects exposed to browser context.

use std::collections::HashMap;

/// Simple Promise implementation for browser use
#[derive(Debug, Clone)]
pub struct BrowserPromise {
    state: PromiseState,
    value: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromiseState {
    Pending,
    Fulfilled,
    Rejected,
}

impl BrowserPromise {
    pub fn new() -> Self {
        Self {
            state: PromiseState::Pending,
            value: None,
            error: None,
        }
    }
    
    pub fn resolve(&mut self, value: String) {
        if self.state == PromiseState::Pending {
            self.state = PromiseState::Fulfilled;
            self.value = Some(value);
        }
    }
    
    pub fn reject(&mut self, error: String) {
        if self.state == PromiseState::Pending {
            self.state = PromiseState::Rejected;
            self.error = Some(error);
        }
    }
    
    pub fn state(&self) -> PromiseState {
        self.state
    }
    
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
    
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

impl Default for BrowserPromise {
    fn default() -> Self {
        Self::new()
    }
}

/// Browser Symbol implementation
#[derive(Debug, Clone)]
pub struct BrowserSymbol {
    description: Option<String>,
    id: u64,
}

static mut SYMBOL_ID: u64 = 0;

impl BrowserSymbol {
    pub fn new(description: Option<&str>) -> Self {
        let id = unsafe {
            SYMBOL_ID += 1;
            SYMBOL_ID
        };
        Self {
            description: description.map(String::from),
            id,
        }
    }
    
    pub fn for_key(key: &str) -> Self {
        Self::new(Some(key))
    }
    
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    
    pub fn id(&self) -> u64 {
        self.id
    }
}

/// Browser Map implementation
#[derive(Debug, Clone, Default)]
pub struct BrowserMap {
    entries: HashMap<String, String>,
}

impl BrowserMap {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn set(&mut self, key: &str, value: &str) {
        self.entries.insert(key.to_string(), value.to_string());
    }
    
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }
    
    pub fn has(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }
    
    pub fn delete(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }
    
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    
    pub fn size(&self) -> usize {
        self.entries.len()
    }
}

/// Browser Set implementation
#[derive(Debug, Clone, Default)]
pub struct BrowserSet {
    values: Vec<String>,
}

impl BrowserSet {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add(&mut self, value: &str) {
        if !self.has(value) {
            self.values.push(value.to_string());
        }
    }
    
    pub fn has(&self, value: &str) -> bool {
        self.values.iter().any(|v| v == value)
    }
    
    pub fn delete(&mut self, value: &str) -> bool {
        let len_before = self.values.len();
        self.values.retain(|v| v != value);
        self.values.len() < len_before
    }
    
    pub fn clear(&mut self) {
        self.values.clear();
    }
    
    pub fn size(&self) -> usize {
        self.values.len()
    }
}

/// JS builtins manager for optimized access
#[derive(Debug, Default)]
pub struct BuiltinsManager {
    symbols: Vec<BrowserSymbol>,
}

impl BuiltinsManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get or create a well-known symbol
    pub fn well_known_symbol(&mut self, name: &str) -> BrowserSymbol {
        if let Some(sym) = self.symbols.iter().find(|s| s.description() == Some(name)) {
            return sym.clone();
        }
        
        let sym = BrowserSymbol::for_key(name);
        self.symbols.push(sym.clone());
        sym
    }
    
    /// Create a Promise
    pub fn create_promise() -> BrowserPromise {
        BrowserPromise::new()
    }
    
    /// Create a Map
    pub fn create_map() -> BrowserMap {
        BrowserMap::new()
    }
    
    /// Create a Set
    pub fn create_set() -> BrowserSet {
        BrowserSet::new()
    }
}

/// Async/Await support for browser context
#[derive(Debug, Default)]
pub struct AsyncContext {
    pending_promises: Vec<BrowserPromise>,
}

impl AsyncContext {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Track a pending promise
    pub fn track_promise(&mut self, promise: BrowserPromise) {
        self.pending_promises.push(promise);
    }
    
    /// Check pending promises
    pub fn poll_promises(&mut self) -> Vec<(usize, PromiseState)> {
        let mut results = Vec::new();
        for (i, promise) in self.pending_promises.iter().enumerate() {
            let state = promise.state();
            if state != PromiseState::Pending {
                results.push((i, state));
            }
        }
        results
    }
    
    /// Clear settled promises
    pub fn clear_settled(&mut self) {
        self.pending_promises.retain(|p| p.state() == PromiseState::Pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_builtins_manager() {
        let mut mgr = BuiltinsManager::new();
        
        let sym1 = mgr.well_known_symbol("iterator");
        let sym2 = mgr.well_known_symbol("iterator");
        assert_eq!(sym1.description(), sym2.description());
    }
    
    #[test]
    fn test_promise() {
        let mut promise = BuiltinsManager::create_promise();
        assert_eq!(promise.state(), PromiseState::Pending);
        
        promise.resolve("done".to_string());
        assert_eq!(promise.state(), PromiseState::Fulfilled);
    }
    
    #[test]
    fn test_map() {
        let mut map = BuiltinsManager::create_map();
        map.set("key", "value");
        assert_eq!(map.get("key"), Some("value"));
        assert!(map.has("key"));
    }
}
