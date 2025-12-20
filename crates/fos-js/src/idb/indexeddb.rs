//! IndexedDB Implementation
//!
//! Indexed database for structured storage.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// IDBFactory - entry point for IndexedDB
#[derive(Debug, Default)]
pub struct IDBFactory {
    databases: Arc<Mutex<HashMap<String, IDBDatabase>>>,
}

/// IDBDatabase
#[derive(Debug, Clone)]
pub struct IDBDatabase {
    pub name: String,
    pub version: u64,
    object_stores: HashMap<String, IDBObjectStore>,
}

/// IDBObjectStore
#[derive(Debug, Clone, Default)]
pub struct IDBObjectStore {
    pub name: String,
    pub key_path: Option<String>,
    pub auto_increment: bool,
    records: Vec<IDBRecord>,
    indexes: HashMap<String, IDBIndex>,
}

/// IDBIndex
#[derive(Debug, Clone)]
pub struct IDBIndex {
    pub name: String,
    pub key_path: String,
    pub unique: bool,
    pub multi_entry: bool,
}

/// IDBRecord
#[derive(Debug, Clone)]
pub struct IDBRecord {
    pub key: IDBKey,
    pub value: IDBValue,
}

/// IDBKey
#[derive(Debug, Clone, PartialEq)]
pub enum IDBKey {
    Number(f64),
    String(String),
    Binary(Vec<u8>),
    Array(Vec<IDBKey>),
}

/// IDBValue
#[derive(Debug, Clone)]
pub enum IDBValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<IDBValue>),
    Object(HashMap<String, IDBValue>),
}

/// IDBTransaction mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IDBTransactionMode {
    ReadOnly,
    ReadWrite,
    VersionChange,
}

/// IDBRequest state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IDBRequestState {
    Pending,
    Done,
}

impl IDBFactory {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Open a database
    pub fn open(&self, name: &str, version: Option<u64>) -> IDBOpenRequest {
        let version = version.unwrap_or(1);
        IDBOpenRequest {
            name: name.to_string(),
            version,
            state: IDBRequestState::Pending,
            result: None,
            error: None,
        }
    }
    
    /// Delete a database
    pub fn delete_database(&self, name: &str) -> bool {
        self.databases.lock().unwrap().remove(name).is_some()
    }
    
    /// List all databases
    pub fn databases(&self) -> Vec<IDBDatabaseInfo> {
        self.databases.lock().unwrap()
            .iter()
            .map(|(name, db)| IDBDatabaseInfo {
                name: name.clone(),
                version: db.version,
            })
            .collect()
    }
}

/// Database info
#[derive(Debug, Clone)]
pub struct IDBDatabaseInfo {
    pub name: String,
    pub version: u64,
}

/// Open request
#[derive(Debug)]
pub struct IDBOpenRequest {
    pub name: String,
    pub version: u64,
    pub state: IDBRequestState,
    pub result: Option<IDBDatabase>,
    pub error: Option<String>,
}

impl IDBDatabase {
    pub fn new(name: &str, version: u64) -> Self {
        Self {
            name: name.to_string(),
            version,
            object_stores: HashMap::new(),
        }
    }
    
    /// Create object store (only in version change)
    pub fn create_object_store(&mut self, name: &str, options: ObjectStoreOptions) -> &IDBObjectStore {
        let store = IDBObjectStore {
            name: name.to_string(),
            key_path: options.key_path,
            auto_increment: options.auto_increment,
            records: Vec::new(),
            indexes: HashMap::new(),
        };
        self.object_stores.insert(name.to_string(), store);
        self.object_stores.get(name).unwrap()
    }
    
    /// Delete object store
    pub fn delete_object_store(&mut self, name: &str) {
        self.object_stores.remove(name);
    }
    
    /// Get object store names
    pub fn object_store_names(&self) -> Vec<&str> {
        self.object_stores.keys().map(|s| s.as_str()).collect()
    }
    
    /// Start transaction
    pub fn transaction(&self, stores: &[&str], mode: IDBTransactionMode) -> IDBTransaction {
        IDBTransaction {
            mode,
            store_names: stores.iter().map(|s| s.to_string()).collect(),
        }
    }
    
    /// Close database
    pub fn close(&self) {
        // Would close connection
    }
}

/// Object store options
#[derive(Debug, Clone, Default)]
pub struct ObjectStoreOptions {
    pub key_path: Option<String>,
    pub auto_increment: bool,
}

/// IDBTransaction
#[derive(Debug)]
pub struct IDBTransaction {
    pub mode: IDBTransactionMode,
    pub store_names: Vec<String>,
}

impl IDBObjectStore {
    /// Add a record
    pub fn add(&mut self, value: IDBValue, key: Option<IDBKey>) -> Option<IDBKey> {
        let key = key.unwrap_or_else(|| IDBKey::Number(self.records.len() as f64));
        self.records.push(IDBRecord { key: key.clone(), value });
        Some(key)
    }
    
    /// Put a record (upsert)
    pub fn put(&mut self, value: IDBValue, key: Option<IDBKey>) -> Option<IDBKey> {
        let key = key.unwrap_or_else(|| IDBKey::Number(self.records.len() as f64));
        
        // Remove existing if any
        self.records.retain(|r| r.key != key);
        self.records.push(IDBRecord { key: key.clone(), value });
        Some(key)
    }
    
    /// Get a record by key
    pub fn get(&self, key: &IDBKey) -> Option<&IDBValue> {
        self.records.iter()
            .find(|r| &r.key == key)
            .map(|r| &r.value)
    }
    
    /// Delete a record
    pub fn delete(&mut self, key: &IDBKey) -> bool {
        let len = self.records.len();
        self.records.retain(|r| &r.key != key);
        self.records.len() < len
    }
    
    /// Clear all records
    pub fn clear(&mut self) {
        self.records.clear();
    }
    
    /// Count records
    pub fn count(&self) -> usize {
        self.records.len()
    }
    
    /// Create index
    pub fn create_index(&mut self, name: &str, key_path: &str, unique: bool) {
        self.indexes.insert(name.to_string(), IDBIndex {
            name: name.to_string(),
            key_path: key_path.to_string(),
            unique,
            multi_entry: false,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_object_store() {
        let mut store = IDBObjectStore::default();
        store.name = "users".to_string();
        
        let key = store.add(IDBValue::String("Alice".into()), None);
        assert!(key.is_some());
        assert_eq!(store.count(), 1);
    }
    
    #[test]
    fn test_database() {
        let mut db = IDBDatabase::new("test", 1);
        db.create_object_store("items", ObjectStoreOptions::default());
        
        assert_eq!(db.object_store_names().len(), 1);
    }
}
