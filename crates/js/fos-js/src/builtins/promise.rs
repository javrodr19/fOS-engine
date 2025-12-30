//! Promise Implementation
//!
//! JavaScript Promise with states and async support.

use std::sync::{Arc, Mutex};

/// Promise state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromiseState {
    Pending,
    Fulfilled,
    Rejected,
}

/// Promise value
#[derive(Debug, Clone)]
pub enum PromiseValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Object(u32), // reference ID
}

impl Default for PromiseValue {
    fn default() -> Self {
        Self::Undefined
    }
}

/// JavaScript Promise
#[derive(Clone)]
pub struct JsPromise {
    inner: Arc<Mutex<PromiseInner>>,
}

struct PromiseInner {
    state: PromiseState,
    value: PromiseValue,
    on_fulfilled: Vec<u32>, // callback IDs
    on_rejected: Vec<u32>,
}

impl JsPromise {
    /// Create a new pending promise
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(PromiseInner {
                state: PromiseState::Pending,
                value: PromiseValue::Undefined,
                on_fulfilled: Vec::new(),
                on_rejected: Vec::new(),
            })),
        }
    }
    
    /// Create a resolved promise
    pub fn resolve(value: PromiseValue) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PromiseInner {
                state: PromiseState::Fulfilled,
                value,
                on_fulfilled: Vec::new(),
                on_rejected: Vec::new(),
            })),
        }
    }
    
    /// Create a rejected promise
    pub fn reject(reason: PromiseValue) -> Self {
        Self {
            inner: Arc::new(Mutex::new(PromiseInner {
                state: PromiseState::Rejected,
                value: reason,
                on_fulfilled: Vec::new(),
                on_rejected: Vec::new(),
            })),
        }
    }
    
    /// Get current state
    pub fn state(&self) -> PromiseState {
        self.inner.lock().unwrap().state.clone()
    }
    
    /// Fulfill the promise
    pub fn fulfill(&self, value: PromiseValue) -> Vec<u32> {
        let mut inner = self.inner.lock().unwrap();
        if inner.state != PromiseState::Pending {
            return Vec::new();
        }
        inner.state = PromiseState::Fulfilled;
        inner.value = value;
        std::mem::take(&mut inner.on_fulfilled)
    }
    
    /// Reject the promise
    pub fn reject_inner(&self, reason: PromiseValue) -> Vec<u32> {
        let mut inner = self.inner.lock().unwrap();
        if inner.state != PromiseState::Pending {
            return Vec::new();
        }
        inner.state = PromiseState::Rejected;
        inner.value = reason;
        std::mem::take(&mut inner.on_rejected)
    }
    
    /// Add then callback
    pub fn then(&self, on_fulfilled: u32, on_rejected: Option<u32>) {
        let mut inner = self.inner.lock().unwrap();
        match inner.state {
            PromiseState::Pending => {
                inner.on_fulfilled.push(on_fulfilled);
                if let Some(cb) = on_rejected {
                    inner.on_rejected.push(cb);
                }
            }
            PromiseState::Fulfilled => {
                // Would invoke immediately
            }
            PromiseState::Rejected => {
                // Would invoke on_rejected immediately
            }
        }
    }
    
    /// Add catch callback
    pub fn catch(&self, on_rejected: u32) {
        self.then(0, Some(on_rejected));
    }
    
    /// Add finally callback
    pub fn finally(&self, callback: u32) {
        let mut inner = self.inner.lock().unwrap();
        inner.on_fulfilled.push(callback);
        inner.on_rejected.push(callback);
    }
    
    /// Get the resolved value (if fulfilled)
    pub fn value(&self) -> Option<PromiseValue> {
        let inner = self.inner.lock().unwrap();
        if inner.state == PromiseState::Fulfilled {
            Some(inner.value.clone())
        } else {
            None
        }
    }
}

impl Default for JsPromise {
    fn default() -> Self {
        Self::new()
    }
}

/// Promise.all - wait for all promises
pub fn promise_all(promises: Vec<JsPromise>) -> JsPromise {
    if promises.is_empty() {
        return JsPromise::resolve(PromiseValue::Undefined);
    }
    
    // Check if all already resolved
    let all_fulfilled = promises.iter().all(|p| p.state() == PromiseState::Fulfilled);
    if all_fulfilled {
        JsPromise::resolve(PromiseValue::Undefined)
    } else {
        JsPromise::new()
    }
}

/// Promise.race - first to settle wins
pub fn promise_race(promises: Vec<JsPromise>) -> JsPromise {
    for p in &promises {
        match p.state() {
            PromiseState::Fulfilled | PromiseState::Rejected => {
                return p.clone();
            }
            _ => {}
        }
    }
    JsPromise::new()
}

/// Promise.any - first fulfilled wins
pub fn promise_any(promises: Vec<JsPromise>) -> JsPromise {
    for p in &promises {
        if p.state() == PromiseState::Fulfilled {
            return p.clone();
        }
    }
    JsPromise::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_promise_states() {
        let p = JsPromise::new();
        assert_eq!(p.state(), PromiseState::Pending);
        
        let resolved = JsPromise::resolve(PromiseValue::Number(42.0));
        assert_eq!(resolved.state(), PromiseState::Fulfilled);
        
        let rejected = JsPromise::reject(PromiseValue::String("error".into()));
        assert_eq!(rejected.state(), PromiseState::Rejected);
    }
    
    #[test]
    fn test_fulfill() {
        let p = JsPromise::new();
        p.then(1, Some(2));
        
        let callbacks = p.fulfill(PromiseValue::Bool(true));
        assert_eq!(callbacks, vec![1]);
        assert_eq!(p.state(), PromiseState::Fulfilled);
    }
}
