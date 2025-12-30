//! WeakRef and FinalizationRegistry
//!
//! Weak references for garbage collection.

use std::sync::{Arc, Mutex, Weak};

/// WeakRef - weak reference to an object
#[derive(Debug)]
pub struct JsWeakRef {
    target: Weak<u32>, // Weak reference to object ID
    held_value: Option<u32>,
}

impl JsWeakRef {
    /// Create a new weak reference
    pub fn new(target_id: u32) -> Self {
        // In real impl, would get Weak from object registry
        Self {
            target: Weak::new(),
            held_value: Some(target_id),
        }
    }
    
    /// Dereference - get target if still alive
    pub fn deref(&self) -> Option<u32> {
        // In real impl, would check if object still exists
        self.held_value
    }
}

impl Clone for JsWeakRef {
    fn clone(&self) -> Self {
        Self {
            target: self.target.clone(),
            held_value: self.held_value,
        }
    }
}

/// FinalizationRegistry - cleanup callbacks for GC
#[derive(Debug)]
pub struct FinalizationRegistry {
    callback_id: u32,
    registrations: Arc<Mutex<Vec<Registration>>>,
}

#[derive(Debug, Clone)]
struct Registration {
    target_id: u32,
    held_value: RegistryHeldValue,
    unregister_token: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum RegistryHeldValue {
    Undefined,
    Number(f64),
    String(String),
    Object(u32),
}

impl FinalizationRegistry {
    /// Create a new finalization registry
    pub fn new(callback_id: u32) -> Self {
        Self {
            callback_id,
            registrations: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Register a target for cleanup
    pub fn register(&self, target_id: u32, held_value: RegistryHeldValue, unregister_token: Option<u32>) {
        self.registrations.lock().unwrap().push(Registration {
            target_id,
            held_value,
            unregister_token,
        });
    }
    
    /// Unregister by token
    pub fn unregister(&self, token: u32) -> bool {
        let mut regs = self.registrations.lock().unwrap();
        let before = regs.len();
        regs.retain(|r| r.unregister_token != Some(token));
        regs.len() < before
    }
    
    /// Called when objects are collected (by GC)
    pub fn cleanup_some(&self, collected_ids: &[u32]) -> Vec<RegistryHeldValue> {
        let mut regs = self.registrations.lock().unwrap();
        let mut held_values = Vec::new();
        
        regs.retain(|r| {
            if collected_ids.contains(&r.target_id) {
                held_values.push(r.held_value.clone());
                false
            } else {
                true
            }
        });
        
        held_values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_weakref() {
        let wr = JsWeakRef::new(42);
        assert_eq!(wr.deref(), Some(42));
    }
    
    #[test]
    fn test_finalization_registry() {
        let registry = FinalizationRegistry::new(1);
        
        registry.register(100, RegistryHeldValue::String("cleanup".into()), Some(1));
        registry.register(101, RegistryHeldValue::Number(42.0), None);
        
        // Simulate GC collecting object 100
        let values = registry.cleanup_some(&[100]);
        assert_eq!(values.len(), 1);
    }
}
