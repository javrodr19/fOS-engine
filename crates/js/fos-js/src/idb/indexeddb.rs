//! IndexedDB Implementation
//!
//! Indexed database for structured storage with LZ4 compression.
//!
//! Large values (>1KB) are automatically compressed using LZ4.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::compress::Lz4Compressor;

/// Compression threshold (bytes) - values larger than this are compressed
const COMPRESSION_THRESHOLD: usize = 1024;

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
    /// Enable LZ4 compression for large values
    pub compression_enabled: bool,
}

/// IDBIndex
#[derive(Debug, Clone)]
pub struct IDBIndex {
    pub name: String,
    pub key_path: String,
    pub unique: bool,
    pub multi_entry: bool,
}

/// IDBRecord with optional compression
#[derive(Debug, Clone)]
pub struct IDBRecord {
    pub key: IDBKey,
    /// The value (may be compressed)
    value_data: CompressedValue,
}

/// Compressed value wrapper
#[derive(Debug, Clone)]
enum CompressedValue {
    /// Uncompressed value
    Raw(IDBValue),
    /// LZ4 compressed bytes
    Compressed(Vec<u8>),
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
    Binary(Vec<u8>),
    Array(Vec<IDBValue>),
    Object(HashMap<String, IDBValue>),
}

/// IDBKeyRange for querying
#[derive(Debug, Clone)]
pub struct IDBKeyRange {
    pub lower: Option<IDBKey>,
    pub upper: Option<IDBKey>,
    pub lower_open: bool,
    pub upper_open: bool,
}

impl IDBKeyRange {
    /// Create a range that matches a single key
    pub fn only(key: IDBKey) -> Self {
        Self {
            lower: Some(key.clone()),
            upper: Some(key),
            lower_open: false,
            upper_open: false,
        }
    }

    /// Create a range with a lower bound
    pub fn lower_bound(lower: IDBKey, open: bool) -> Self {
        Self {
            lower: Some(lower),
            upper: None,
            lower_open: open,
            upper_open: true,
        }
    }

    /// Create a range with an upper bound
    pub fn upper_bound(upper: IDBKey, open: bool) -> Self {
        Self {
            lower: None,
            upper: Some(upper),
            lower_open: true,
            upper_open: open,
        }
    }

    /// Create a range with both bounds
    pub fn bound(lower: IDBKey, upper: IDBKey, lower_open: bool, upper_open: bool) -> Self {
        Self {
            lower: Some(lower),
            upper: Some(upper),
            lower_open,
            upper_open,
        }
    }

    /// Check if a key is within this range
    pub fn includes(&self, key: &IDBKey) -> bool {
        if let Some(ref lower) = self.lower {
            let cmp = Self::compare_keys(key, lower);
            if self.lower_open && cmp <= 0 {
                return false;
            }
            if !self.lower_open && cmp < 0 {
                return false;
            }
        }

        if let Some(ref upper) = self.upper {
            let cmp = Self::compare_keys(key, upper);
            if self.upper_open && cmp >= 0 {
                return false;
            }
            if !self.upper_open && cmp > 0 {
                return false;
            }
        }

        true
    }

    /// Simple key comparison
    fn compare_keys(a: &IDBKey, b: &IDBKey) -> i32 {
        match (a, b) {
            (IDBKey::Number(na), IDBKey::Number(nb)) => {
                if na < nb { -1 }
                else if na > nb { 1 }
                else { 0 }
            }
            (IDBKey::String(sa), IDBKey::String(sb)) => sa.cmp(sb) as i32,
            _ => 0,
        }
    }
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
            compression_enabled: true, // Enable compression by default
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

impl IDBRecord {
    /// Create a new record, optionally compressing large values
    fn new(key: IDBKey, value: IDBValue, compress: bool) -> Self {
        let value_data = if compress {
            // Serialize value and check size
            let serialized = Self::serialize_value(&value);
            if serialized.len() > COMPRESSION_THRESHOLD {
                let compressed = Lz4Compressor::compress(&serialized);
                CompressedValue::Compressed(compressed)
            } else {
                CompressedValue::Raw(value)
            }
        } else {
            CompressedValue::Raw(value)
        };
        Self { key, value_data }
    }

    /// Get the value, decompressing if needed
    pub fn value(&self) -> IDBValue {
        match &self.value_data {
            CompressedValue::Raw(v) => v.clone(),
            CompressedValue::Compressed(data) => {
                if let Some(decompressed) = Lz4Compressor::decompress(data) {
                    Self::deserialize_value(&decompressed)
                        .unwrap_or(IDBValue::Null)
                } else {
                    IDBValue::Null
                }
            }
        }
    }

    /// Simple serialization for compression purposes
    fn serialize_value(value: &IDBValue) -> Vec<u8> {
        // Simple JSON-like serialization
        match value {
            IDBValue::Null => b"null".to_vec(),
            IDBValue::Bool(b) => if *b { b"true".to_vec() } else { b"false".to_vec() },
            IDBValue::Number(n) => n.to_string().into_bytes(),
            IDBValue::String(s) => s.as_bytes().to_vec(),
            IDBValue::Binary(b) => b.clone(),
            IDBValue::Array(_) => b"[]".to_vec(), // Simplified
            IDBValue::Object(_) => b"{}".to_vec(), // Simplified
        }
    }

    /// Simple deserialization
    fn deserialize_value(data: &[u8]) -> Option<IDBValue> {
        let s = String::from_utf8_lossy(data);
        if s == "null" {
            Some(IDBValue::Null)
        } else if s == "true" {
            Some(IDBValue::Bool(true))
        } else if s == "false" {
            Some(IDBValue::Bool(false))
        } else if let Ok(n) = s.parse::<f64>() {
            Some(IDBValue::Number(n))
        } else {
            Some(IDBValue::String(s.to_string()))
        }
    }
    
    /// Check if the record is compressed
    pub fn is_compressed(&self) -> bool {
        matches!(self.value_data, CompressedValue::Compressed(_))
    }
}

impl IDBObjectStore {
    /// Create new object store with compression
    pub fn with_compression(name: &str, compression: bool) -> Self {
        Self {
            name: name.to_string(),
            compression_enabled: compression,
            ..Default::default()
        }
    }

    /// Add a record (with optional compression)
    pub fn add(&mut self, value: IDBValue, key: Option<IDBKey>) -> Option<IDBKey> {
        let key = key.unwrap_or_else(|| IDBKey::Number(self.records.len() as f64));
        let record = IDBRecord::new(key.clone(), value, self.compression_enabled);
        self.records.push(record);
        Some(key)
    }
    
    /// Put a record (upsert with compression)
    pub fn put(&mut self, value: IDBValue, key: Option<IDBKey>) -> Option<IDBKey> {
        let key = key.unwrap_or_else(|| IDBKey::Number(self.records.len() as f64));
        
        // Remove existing if any
        self.records.retain(|r| r.key != key);
        let record = IDBRecord::new(key.clone(), value, self.compression_enabled);
        self.records.push(record);
        Some(key)
    }
    
    /// Get a record by key (auto-decompresses)
    pub fn get(&self, key: &IDBKey) -> Option<IDBValue> {
        self.records.iter()
            .find(|r| &r.key == key)
            .map(|r| r.value())
    }

    /// Get all records matching a key range
    pub fn get_all(&self, range: Option<&IDBKeyRange>, count: Option<usize>) -> Vec<IDBValue> {
        let iter = self.records.iter()
            .filter(|r| range.map_or(true, |rng| rng.includes(&r.key)))
            .map(|r| r.value());

        if let Some(n) = count {
            iter.take(n).collect()
        } else {
            iter.collect()
        }
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

    /// Count records in range
    pub fn count_range(&self, range: &IDBKeyRange) -> usize {
        self.records.iter()
            .filter(|r| range.includes(&r.key))
            .count()
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

    /// Get compression stats
    pub fn compression_stats(&self) -> (usize, usize) {
        let compressed = self.records.iter().filter(|r| r.is_compressed()).count();
        (compressed, self.records.len())
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
