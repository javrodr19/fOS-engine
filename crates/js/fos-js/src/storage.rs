//! Storage APIs
//!
//! Implements localStorage and sessionStorage.

use crate::{JsValue, JsError};
use crate::engine_trait::JsContextApi;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Web Storage implementation
pub struct Storage {
    data: HashMap<String, String>,
    is_persistent: bool,
}

impl Storage {
    /// Create localStorage (persistent)
    pub fn local() -> Self {
        Self {
            data: HashMap::new(),
            is_persistent: true,
        }
    }
    
    /// Create sessionStorage (session-only)
    pub fn session() -> Self {
        Self {
            data: HashMap::new(),
            is_persistent: false,
        }
    }
    
    /// Get item by key
    pub fn get_item(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(|s| s.as_str())
    }
    
    /// Set item
    pub fn set_item(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
    }
    
    /// Remove item
    pub fn remove_item(&mut self, key: &str) {
        self.data.remove(key);
    }
    
    /// Clear all items
    pub fn clear(&mut self) {
        self.data.clear();
    }
    
    /// Number of items
    pub fn length(&self) -> usize {
        self.data.len()
    }
    
    /// Get key at index
    pub fn key(&self, index: usize) -> Option<&str> {
        self.data.keys().nth(index).map(|s| s.as_str())
    }
    
    /// Check if persistent
    pub fn is_persistent(&self) -> bool {
        self.is_persistent
    }
}

/// Install storage APIs into global object
pub fn install_storage<C: JsContextApi>(
    ctx: &C,
    local_storage: Arc<Mutex<Storage>>,
    session_storage: Arc<Mutex<Storage>>,
) -> Result<(), JsError> {
    // Create localStorage object
    let local_obj = ctx.create_object()?;
    install_storage_methods(ctx, &local_obj, local_storage)?;
    ctx.set_global("localStorage", JsValue::Object)?;
    
    // Create sessionStorage object
    let session_obj = ctx.create_object()?;
    install_storage_methods(ctx, &session_obj, session_storage)?;
    ctx.set_global("sessionStorage", JsValue::Object)?;
    
    Ok(())
}

fn install_storage_methods<C: JsContextApi>(
    ctx: &C,
    obj: &crate::engine_trait::JsObjectHandle,
    storage: Arc<Mutex<Storage>>,
) -> Result<(), JsError> {
    // getItem
    let s = storage.clone();
    ctx.set_function(obj, "getItem", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Null);
        }
        
        let key = args[0].as_string().unwrap_or("");
        let storage = s.lock().unwrap();
        
        match storage.get_item(key) {
            Some(value) => Ok(JsValue::String(value.to_string())),
            None => Ok(JsValue::Null),
        }
    })?;
    
    // setItem
    let s = storage.clone();
    ctx.set_function(obj, "setItem", move |args| {
        if args.len() < 2 {
            return Ok(JsValue::Undefined);
        }
        
        let key = args[0].as_string().unwrap_or("");
        let value = args[1].as_string().unwrap_or("");
        
        let mut storage = s.lock().unwrap();
        storage.set_item(key, value);
        Ok(JsValue::Undefined)
    })?;
    
    // removeItem
    let s = storage.clone();
    ctx.set_function(obj, "removeItem", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Undefined);
        }
        
        let key = args[0].as_string().unwrap_or("");
        let mut storage = s.lock().unwrap();
        storage.remove_item(key);
        Ok(JsValue::Undefined)
    })?;
    
    // clear
    let s = storage.clone();
    ctx.set_function(obj, "clear", move |_args| {
        let mut storage = s.lock().unwrap();
        storage.clear();
        Ok(JsValue::Undefined)
    })?;
    
    // getLength
    let s = storage.clone();
    ctx.set_function(obj, "getLength", move |_args| {
        let storage = s.lock().unwrap();
        Ok(JsValue::Number(storage.length() as f64))
    })?;
    
    // key
    let s = storage;
    ctx.set_function(obj, "key", move |args| {
        if args.is_empty() {
            return Ok(JsValue::Null);
        }
        
        let index = args[0].as_number().unwrap_or(0.0) as usize;
        let storage = s.lock().unwrap();
        
        match storage.key(index) {
            Some(key) => Ok(JsValue::String(key.to_string())),
            None => Ok(JsValue::Null),
        }
    })?;
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_storage_basic() {
        let mut storage = Storage::session();
        
        storage.set_item("key", "value");
        assert_eq!(storage.get_item("key"), Some("value"));
        assert_eq!(storage.length(), 1);
        
        storage.remove_item("key");
        assert_eq!(storage.get_item("key"), None);
        assert_eq!(storage.length(), 0);
    }
    
    #[test]
    fn test_storage_clear() {
        let mut storage = Storage::session();
        
        storage.set_item("a", "1");
        storage.set_item("b", "2");
        assert_eq!(storage.length(), 2);
        
        storage.clear();
        assert_eq!(storage.length(), 0);
    }
}
