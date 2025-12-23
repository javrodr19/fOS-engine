//! IndexedDB integration
//!
//! Client-side database storage.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// IndexedDB database
#[derive(Debug)]
pub struct IDBDatabase {
    pub name: String,
    pub version: u64,
    object_stores: HashMap<String, ObjectStore>,
}

impl IDBDatabase {
    pub fn new(name: &str, version: u64) -> Self {
        Self {
            name: name.to_string(),
            version,
            object_stores: HashMap::new(),
        }
    }
    
    /// Create an object store
    pub fn create_object_store(&mut self, name: &str, options: ObjectStoreOptions) -> &mut ObjectStore {
        let store = ObjectStore::new(name, options);
        self.object_stores.insert(name.to_string(), store);
        self.object_stores.get_mut(name).unwrap()
    }
    
    /// Delete an object store
    pub fn delete_object_store(&mut self, name: &str) -> bool {
        self.object_stores.remove(name).is_some()
    }
    
    /// Get object store names
    pub fn object_store_names(&self) -> Vec<&str> {
        self.object_stores.keys().map(|s| s.as_str()).collect()
    }
    
    /// Start a transaction
    pub fn transaction(&mut self, store_names: &[&str], mode: TransactionMode) -> Transaction {
        Transaction {
            stores: store_names.iter().map(|s| s.to_string()).collect(),
            mode,
            completed: false,
        }
    }
    
    /// Get object store (for operations)
    pub fn get_store(&self, name: &str) -> Option<&ObjectStore> {
        self.object_stores.get(name)
    }
    
    pub fn get_store_mut(&mut self, name: &str) -> Option<&mut ObjectStore> {
        self.object_stores.get_mut(name)
    }
}

/// Object store options
#[derive(Debug, Clone, Default)]
pub struct ObjectStoreOptions {
    pub key_path: Option<String>,
    pub auto_increment: bool,
}

/// An object store (like a table)
#[derive(Debug)]
pub struct ObjectStore {
    pub name: String,
    options: ObjectStoreOptions,
    records: HashMap<IDBKey, IDBValue>,
    indexes: HashMap<String, Index>,
    next_key: u64,
}

impl ObjectStore {
    pub fn new(name: &str, options: ObjectStoreOptions) -> Self {
        Self {
            name: name.to_string(),
            options,
            records: HashMap::new(),
            indexes: HashMap::new(),
            next_key: 1,
        }
    }
    
    /// Add a record
    pub fn add(&mut self, value: IDBValue, key: Option<IDBKey>) -> Result<IDBKey, IDBError> {
        let key = self.resolve_key(key, &value)?;
        
        if self.records.contains_key(&key) {
            return Err(IDBError::ConstraintError("Key already exists".into()));
        }
        
        self.records.insert(key.clone(), value);
        Ok(key)
    }
    
    /// Put a record (insert or update)
    pub fn put(&mut self, value: IDBValue, key: Option<IDBKey>) -> Result<IDBKey, IDBError> {
        let key = self.resolve_key(key, &value)?;
        self.records.insert(key.clone(), value);
        Ok(key)
    }
    
    /// Get a record by key
    pub fn get(&self, key: &IDBKey) -> Option<&IDBValue> {
        self.records.get(key)
    }
    
    /// Delete a record
    pub fn delete(&mut self, key: &IDBKey) -> bool {
        self.records.remove(key).is_some()
    }
    
    /// Clear all records
    pub fn clear(&mut self) {
        self.records.clear();
    }
    
    /// Count records
    pub fn count(&self) -> usize {
        self.records.len()
    }
    
    /// Get all records
    pub fn get_all(&self) -> Vec<(&IDBKey, &IDBValue)> {
        self.records.iter().collect()
    }
    
    /// Create an index
    pub fn create_index(&mut self, name: &str, key_path: &str, options: IndexOptions) {
        let index = Index {
            name: name.to_string(),
            key_path: key_path.to_string(),
            unique: options.unique,
            multi_entry: options.multi_entry,
        };
        self.indexes.insert(name.to_string(), index);
    }
    
    fn resolve_key(&mut self, key: Option<IDBKey>, value: &IDBValue) -> Result<IDBKey, IDBError> {
        if let Some(k) = key {
            return Ok(k);
        }
        
        // Try to extract from key path
        if let Some(ref key_path) = self.options.key_path {
            if let IDBValue::Object(map) = value {
                if let Some(v) = map.get(key_path) {
                    return Ok(value_to_key(v)?);
                }
            }
        }
        
        // Auto-increment
        if self.options.auto_increment {
            let key = IDBKey::Number(self.next_key as f64);
            self.next_key += 1;
            return Ok(key);
        }
        
        Err(IDBError::DataError("No key provided".into()))
    }
}

/// Index options
#[derive(Debug, Clone, Default)]
pub struct IndexOptions {
    pub unique: bool,
    pub multi_entry: bool,
}

/// An index on an object store
#[derive(Debug)]
pub struct Index {
    pub name: String,
    pub key_path: String,
    pub unique: bool,
    pub multi_entry: bool,
}

/// Transaction mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionMode {
    ReadOnly,
    ReadWrite,
    VersionChange,
}

/// A database transaction
#[derive(Debug)]
pub struct Transaction {
    pub stores: Vec<String>,
    pub mode: TransactionMode,
    pub completed: bool,
}

impl Transaction {
    pub fn abort(&mut self) {
        self.completed = true;
    }
    
    pub fn commit(&mut self) {
        self.completed = true;
    }
}

/// IndexedDB key types
#[derive(Debug, Clone, PartialEq)]
pub enum IDBKey {
    Number(f64),
    String(String),
    Array(Vec<IDBKey>),
    Binary(Vec<u8>),
}

impl Eq for IDBKey {}

impl std::hash::Hash for IDBKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            IDBKey::Number(n) => {
                0u8.hash(state);
                n.to_bits().hash(state);
            }
            IDBKey::String(s) => {
                1u8.hash(state);
                s.hash(state);
            }
            IDBKey::Array(arr) => {
                2u8.hash(state);
                for k in arr {
                    k.hash(state);
                }
            }
            IDBKey::Binary(b) => {
                3u8.hash(state);
                b.hash(state);
            }
        }
    }
}

/// IndexedDB value types
#[derive(Debug, Clone)]
pub enum IDBValue {
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Array(Vec<IDBValue>),
    Object(HashMap<String, IDBValue>),
    Binary(Vec<u8>),
}

fn value_to_key(value: &IDBValue) -> Result<IDBKey, IDBError> {
    match value {
        IDBValue::Number(n) => Ok(IDBKey::Number(*n)),
        IDBValue::String(s) => Ok(IDBKey::String(s.clone())),
        IDBValue::Binary(b) => Ok(IDBKey::Binary(b.clone())),
        _ => Err(IDBError::DataError("Invalid key type".into())),
    }
}

/// IndexedDB errors
#[derive(Debug)]
pub enum IDBError {
    NotFoundError(String),
    ConstraintError(String),
    DataError(String),
    TransactionInactiveError,
    ReadOnlyError,
    VersionError,
}

impl std::fmt::Display for IDBError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFoundError(msg) => write!(f, "NotFoundError: {}", msg),
            Self::ConstraintError(msg) => write!(f, "ConstraintError: {}", msg),
            Self::DataError(msg) => write!(f, "DataError: {}", msg),
            Self::TransactionInactiveError => write!(f, "TransactionInactiveError"),
            Self::ReadOnlyError => write!(f, "ReadOnlyError"),
            Self::VersionError => write!(f, "VersionError"),
        }
    }
}

impl std::error::Error for IDBError {}

/// IndexedDB factory (window.indexedDB)
#[derive(Debug, Default)]
pub struct IDBFactory {
    databases: HashMap<String, Arc<Mutex<IDBDatabase>>>,
}

impl IDBFactory {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Open a database
    pub fn open(&mut self, name: &str, version: Option<u64>) -> Result<Arc<Mutex<IDBDatabase>>, IDBError> {
        let version = version.unwrap_or(1);
        
        if let Some(db) = self.databases.get(name) {
            let db_guard = db.lock().unwrap();
            if db_guard.version < version {
                // Would trigger upgrade, for now just update version
                drop(db_guard);
                let new_db = Arc::new(Mutex::new(IDBDatabase::new(name, version)));
                self.databases.insert(name.to_string(), new_db.clone());
                return Ok(new_db);
            }
            return Ok(db.clone());
        }
        
        let db = Arc::new(Mutex::new(IDBDatabase::new(name, version)));
        self.databases.insert(name.to_string(), db.clone());
        Ok(db)
    }
    
    /// Delete a database
    pub fn delete_database(&mut self, name: &str) -> bool {
        self.databases.remove(name).is_some()
    }
    
    /// List all databases
    pub fn databases(&self) -> Vec<(&str, u64)> {
        self.databases.iter()
            .map(|(name, db)| {
                let guard = db.lock().unwrap();
                (name.as_str(), guard.version)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_indexeddb_basic() {
        let mut factory = IDBFactory::new();
        let db = factory.open("testdb", Some(1)).unwrap();
        
        {
            let mut db = db.lock().unwrap();
            db.create_object_store("users", ObjectStoreOptions {
                key_path: Some("id".to_string()),
                auto_increment: false,
            });
            
            let store = db.get_store_mut("users").unwrap();
            let mut user = HashMap::new();
            user.insert("id".to_string(), IDBValue::Number(1.0));
            user.insert("name".to_string(), IDBValue::String("Alice".to_string()));
            
            store.put(IDBValue::Object(user), None).unwrap();
        }
        
        {
            let db = db.lock().unwrap();
            let store = db.get_store("users").unwrap();
            assert_eq!(store.count(), 1);
        }
    }
}
