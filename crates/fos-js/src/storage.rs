//! Storage APIs
//!
//! localStorage and sessionStorage implementations.

use rquickjs::{Ctx, Function, Object, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::fs;
use std::path::PathBuf;

/// Storage backend
#[derive(Debug, Default)]
pub struct Storage {
    data: HashMap<String, String>,
    persistent: bool,
    path: Option<PathBuf>,
}

impl Storage {
    /// Create in-memory storage (sessionStorage)
    pub fn session() -> Self {
        Self {
            data: HashMap::new(),
            persistent: false,
            path: None,
        }
    }
    
    /// Create persistent storage (localStorage)
    pub fn local(path: PathBuf) -> Self {
        let mut storage = Self {
            data: HashMap::new(),
            persistent: true,
            path: Some(path.clone()),
        };
        
        // Load existing data
        if path.exists() {
            if let Ok(contents) = fs::read_to_string(&path) {
                for line in contents.lines() {
                    if let Some((key, value)) = line.split_once('\t') {
                        storage.data.insert(key.to_string(), value.to_string());
                    }
                }
            }
        }
        
        storage
    }
    
    /// Get item
    pub fn get_item(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(|s| s.as_str())
    }
    
    /// Set item
    pub fn set_item(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
        self.persist();
    }
    
    /// Remove item
    pub fn remove_item(&mut self, key: &str) {
        self.data.remove(key);
        self.persist();
    }
    
    /// Clear all items
    pub fn clear(&mut self) {
        self.data.clear();
        self.persist();
    }
    
    /// Get key at index
    pub fn key(&self, index: usize) -> Option<&str> {
        self.data.keys().nth(index).map(|s| s.as_str())
    }
    
    /// Get number of items
    pub fn length(&self) -> usize {
        self.data.len()
    }
    
    /// Persist to disk if persistent
    fn persist(&self) {
        if self.persistent {
            if let Some(path) = &self.path {
                let contents: String = self.data
                    .iter()
                    .map(|(k, v)| format!("{}\t{}", k, v))
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = fs::write(path, contents);
            }
        }
    }
}

/// Install localStorage and sessionStorage into global
pub fn install_storage(
    ctx: &Ctx,
    local: Arc<Mutex<Storage>>,
    session: Arc<Mutex<Storage>>,
) -> Result<(), rquickjs::Error> {
    let globals = ctx.globals();
    
    // localStorage
    globals.set("localStorage", create_storage_object(ctx, local)?)?;
    
    // sessionStorage
    globals.set("sessionStorage", create_storage_object(ctx, session)?)?;
    
    Ok(())
}

fn create_storage_object<'js>(ctx: &Ctx<'js>, storage: Arc<Mutex<Storage>>) -> Result<Object<'js>, rquickjs::Error> {
    let obj = Object::new(ctx.clone())?;
    
    // getItem
    let s = storage.clone();
    obj.set("getItem", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<Option<String>, rquickjs::Error> {
        if let Some(key) = args.first().and_then(|v| v.as_string()) {
            let key = key.to_string().unwrap_or_default();
            let storage = s.lock().unwrap();
            return Ok(storage.get_item(&key).map(|v| v.to_string()));
        }
        Ok(None)
    })?)?;
    
    // setItem
    let s = storage.clone();
    obj.set("setItem", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        if args.len() >= 2 {
            if let (Some(key), Some(value)) = (args[0].as_string(), args[1].as_string()) {
                let key = key.to_string().unwrap_or_default();
                let value = value.to_string().unwrap_or_default();
                let mut storage = s.lock().unwrap();
                storage.set_item(&key, &value);
            }
        }
        Ok(())
    })?)?;
    
    // removeItem
    let s = storage.clone();
    obj.set("removeItem", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        if let Some(key) = args.first().and_then(|v| v.as_string()) {
            let key = key.to_string().unwrap_or_default();
            let mut storage = s.lock().unwrap();
            storage.remove_item(&key);
        }
        Ok(())
    })?)?;
    
    // clear
    let s = storage.clone();
    obj.set("clear", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<(), rquickjs::Error> {
        let mut storage = s.lock().unwrap();
        storage.clear();
        Ok(())
    })?)?;
    
    // length
    let s = storage.clone();
    obj.set("getLength", Function::new(ctx.clone(), move |_ctx: Ctx, _args: rquickjs::function::Rest<Value>| -> Result<i32, rquickjs::Error> {
        let storage = s.lock().unwrap();
        Ok(storage.length() as i32)
    })?)?;
    
    // key
    let s = storage;
    obj.set("key", Function::new(ctx.clone(), move |_ctx: Ctx, args: rquickjs::function::Rest<Value>| -> Result<Option<String>, rquickjs::Error> {
        if let Some(index) = args.first().and_then(|v| v.as_int()) {
            let storage = s.lock().unwrap();
            return Ok(storage.key(index as usize).map(|k| k.to_string()));
        }
        Ok(None)
    })?)?;
    
    Ok(obj)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_session_storage() {
        let mut storage = Storage::session();
        
        storage.set_item("key1", "value1");
        assert_eq!(storage.get_item("key1"), Some("value1"));
        
        storage.set_item("key2", "value2");
        assert_eq!(storage.length(), 2);
        
        storage.remove_item("key1");
        assert_eq!(storage.get_item("key1"), None);
        
        storage.clear();
        assert_eq!(storage.length(), 0);
    }
    
    #[test]
    fn test_storage_key() {
        let mut storage = Storage::session();
        storage.set_item("a", "1");
        storage.set_item("b", "2");
        
        // Keys may be in any order
        let key0 = storage.key(0);
        let key1 = storage.key(1);
        let key2 = storage.key(2);
        
        assert!(key0.is_some());
        assert!(key1.is_some());
        assert!(key2.is_none());
    }
}
