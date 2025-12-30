//! Promise Implementation
//!
//! JavaScript Promise for async/await support.

use super::value::JsVal;
use std::sync::{Arc, Mutex};

/// Promise state
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromiseState {
    Pending,
    Fulfilled,
    Rejected,
}

/// JavaScript Promise
#[derive(Debug, Clone)]
pub struct JsPromise {
    state: PromiseState,
    value: Option<JsVal>,
    reason: Option<JsVal>,
    then_callbacks: Vec<u32>,    // Function IDs for then handlers
    catch_callbacks: Vec<u32>,   // Function IDs for catch handlers
    finally_callbacks: Vec<u32>, // Function IDs for finally handlers
}

impl Default for JsPromise {
    fn default() -> Self { Self::new() }
}

impl JsPromise {
    pub fn new() -> Self {
        Self {
            state: PromiseState::Pending,
            value: None,
            reason: None,
            then_callbacks: Vec::new(),
            catch_callbacks: Vec::new(),
            finally_callbacks: Vec::new(),
        }
    }
    
    pub fn resolved(value: JsVal) -> Self {
        Self {
            state: PromiseState::Fulfilled,
            value: Some(value),
            reason: None,
            then_callbacks: Vec::new(),
            catch_callbacks: Vec::new(),
            finally_callbacks: Vec::new(),
        }
    }
    
    pub fn rejected(reason: JsVal) -> Self {
        Self {
            state: PromiseState::Rejected,
            value: None,
            reason: Some(reason),
            then_callbacks: Vec::new(),
            catch_callbacks: Vec::new(),
            finally_callbacks: Vec::new(),
        }
    }
    
    pub fn state(&self) -> &PromiseState { &self.state }
    pub fn value(&self) -> Option<&JsVal> { self.value.as_ref() }
    pub fn reason(&self) -> Option<&JsVal> { self.reason.as_ref() }
    
    pub fn resolve(&mut self, value: JsVal) {
        if self.state == PromiseState::Pending {
            self.state = PromiseState::Fulfilled;
            self.value = Some(value);
        }
    }
    
    pub fn reject(&mut self, reason: JsVal) {
        if self.state == PromiseState::Pending {
            self.state = PromiseState::Rejected;
            self.reason = Some(reason);
        }
    }
    
    pub fn add_then(&mut self, callback: u32) {
        self.then_callbacks.push(callback);
    }
    
    pub fn add_catch(&mut self, callback: u32) {
        self.catch_callbacks.push(callback);
    }
    
    pub fn add_finally(&mut self, callback: u32) {
        self.finally_callbacks.push(callback);
    }
    
    pub fn then_callbacks(&self) -> &[u32] { &self.then_callbacks }
    pub fn catch_callbacks(&self) -> &[u32] { &self.catch_callbacks }
    pub fn finally_callbacks(&self) -> &[u32] { &self.finally_callbacks }
}

/// Async function state
#[derive(Debug, Clone)]
pub struct AsyncState {
    /// Current execution state
    pub suspended: bool,
    /// Awaited promise ID
    pub awaiting: Option<u32>,
    /// Resume instruction pointer
    pub resume_ip: usize,
    /// Saved stack state
    pub saved_stack: Vec<JsVal>,
}

impl Default for AsyncState {
    fn default() -> Self { Self::new() }
}

impl AsyncState {
    pub fn new() -> Self {
        Self {
            suspended: false,
            awaiting: None,
            resume_ip: 0,
            saved_stack: Vec::new(),
        }
    }
    
    pub fn suspend(&mut self, promise_id: u32, ip: usize, stack: Vec<JsVal>) {
        self.suspended = true;
        self.awaiting = Some(promise_id);
        self.resume_ip = ip;
        self.saved_stack = stack;
    }
    
    pub fn resume(&mut self) -> (usize, Vec<JsVal>) {
        self.suspended = false;
        self.awaiting = None;
        (self.resume_ip, std::mem::take(&mut self.saved_stack))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_promise_resolve() {
        let mut promise = JsPromise::new();
        assert_eq!(promise.state(), &PromiseState::Pending);
        
        promise.resolve(JsVal::Number(42.0));
        assert_eq!(promise.state(), &PromiseState::Fulfilled);
        assert_eq!(promise.value(), Some(&JsVal::Number(42.0)));
    }
    
    #[test]
    fn test_promise_reject() {
        let mut promise = JsPromise::new();
        promise.reject(JsVal::String("error".into()));
        assert_eq!(promise.state(), &PromiseState::Rejected);
    }
    
    #[test]
    fn test_promise_callbacks() {
        let mut promise = JsPromise::new();
        promise.add_then(1);
        promise.add_catch(2);
        assert_eq!(promise.then_callbacks().len(), 1);
        assert_eq!(promise.catch_callbacks().len(), 1);
    }
}
