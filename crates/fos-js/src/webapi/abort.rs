//! AbortController and AbortSignal
//!
//! Request cancellation mechanism.

use std::sync::{Arc, Mutex};

/// AbortController - cancellation controller
#[derive(Debug, Clone)]
pub struct AbortController {
    signal: AbortSignal,
}

/// AbortSignal - cancellation state
#[derive(Debug, Clone)]
pub struct AbortSignal {
    inner: Arc<Mutex<AbortSignalInner>>,
}

#[derive(Debug)]
struct AbortSignalInner {
    aborted: bool,
    reason: Option<String>,
    listeners: Vec<u32>, // callback IDs
}

impl AbortController {
    /// Create a new abort controller
    pub fn new() -> Self {
        Self {
            signal: AbortSignal::new(),
        }
    }
    
    /// Get the associated signal
    pub fn signal(&self) -> &AbortSignal {
        &self.signal
    }
    
    /// Abort with optional reason
    pub fn abort(&self, reason: Option<&str>) {
        self.signal.abort(reason);
    }
}

impl Default for AbortController {
    fn default() -> Self {
        Self::new()
    }
}

impl AbortSignal {
    fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(AbortSignalInner {
                aborted: false,
                reason: None,
                listeners: Vec::new(),
            })),
        }
    }
    
    /// Create an already-aborted signal
    pub fn aborted(reason: Option<&str>) -> Self {
        let signal = Self::new();
        signal.inner.lock().unwrap().aborted = true;
        signal.inner.lock().unwrap().reason = reason.map(|s| s.to_string());
        signal
    }
    
    /// Create a signal that aborts after timeout
    pub fn timeout(_ms: u64) -> Self {
        // Would set up timer
        Self::new()
    }
    
    /// Check if aborted
    pub fn is_aborted(&self) -> bool {
        self.inner.lock().unwrap().aborted
    }
    
    /// Get abort reason
    pub fn reason(&self) -> Option<String> {
        self.inner.lock().unwrap().reason.clone()
    }
    
    /// Add abort listener
    pub fn add_event_listener(&self, callback_id: u32) {
        let mut inner = self.inner.lock().unwrap();
        if inner.aborted {
            // Would invoke immediately
        } else {
            inner.listeners.push(callback_id);
        }
    }
    
    /// Remove abort listener
    pub fn remove_event_listener(&self, callback_id: u32) {
        self.inner.lock().unwrap().listeners.retain(|&id| id != callback_id);
    }
    
    /// Throw if aborted
    pub fn throw_if_aborted(&self) -> Result<(), AbortError> {
        if self.is_aborted() {
            Err(AbortError {
                reason: self.reason().unwrap_or_default(),
            })
        } else {
            Ok(())
        }
    }
    
    /// Abort the signal
    fn abort(&self, reason: Option<&str>) {
        let mut inner = self.inner.lock().unwrap();
        if !inner.aborted {
            inner.aborted = true;
            inner.reason = reason.map(|s| s.to_string());
            // Would invoke all listeners
            inner.listeners.clear();
        }
    }
}

/// Abort error
#[derive(Debug, Clone)]
pub struct AbortError {
    pub reason: String,
}

impl std::fmt::Display for AbortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AbortError: {}", self.reason)
    }
}

impl std::error::Error for AbortError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_abort_controller() {
        let controller = AbortController::new();
        
        assert!(!controller.signal().is_aborted());
        
        controller.abort(Some("User cancelled"));
        
        assert!(controller.signal().is_aborted());
        assert_eq!(controller.signal().reason(), Some("User cancelled".to_string()));
    }
    
    #[test]
    fn test_throw_if_aborted() {
        let signal = AbortSignal::aborted(Some("test"));
        
        let result = signal.throw_if_aborted();
        assert!(result.is_err());
    }
}
