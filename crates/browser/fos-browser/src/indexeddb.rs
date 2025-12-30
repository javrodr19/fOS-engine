//! IndexedDB Integration
//!
//! Re-exports IndexedDB types from fos-js with browser-specific wrappers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Re-export core types from fos-js
pub use fos_js::idb::indexeddb::{
    IDBFactory as JsIDBFactory,
    IDBDatabase as JsIDBDatabase,
    IDBObjectStore, IDBKey, IDBValue, IDBIndex,
    IDBTransaction as JsIDBTransaction, IDBTransactionMode,
    IDBOpenRequest, IDBDatabaseInfo, ObjectStoreOptions,
};
pub use fos_js::idb::cache_api::{CacheStorage, Cache, CacheRequest, CacheResponse};
pub use fos_js::idb::cookies::{CookieStore, Cookie, SameSite};

/// Browser's IndexedDB factory with origin isolation
#[derive(Debug, Default)]
pub struct IDBFactory {
    /// Factories per origin (each origin gets isolated storage)
    factories: HashMap<String, JsIDBFactory>,
    /// Open databases by (origin, name)
    databases: HashMap<(String, String), JsIDBDatabase>,
}

impl IDBFactory {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get or create factory for origin
    fn factory_for(&mut self, origin: &str) -> &mut JsIDBFactory {
        self.factories.entry(origin.to_string()).or_insert_with(JsIDBFactory::new)
    }
    
    /// Open a database for an origin
    pub fn open(&mut self, origin: &str, name: &str, version: Option<u64>) -> IDBOpenRequest {
        let factory = self.factory_for(origin);
        factory.open(name, version)
    }
    
    /// Store an opened database
    pub fn store_database(&mut self, origin: &str, name: &str, db: JsIDBDatabase) {
        self.databases.insert((origin.to_string(), name.to_string()), db);
    }
    
    /// Get an open database
    pub fn get_database(&self, origin: &str, name: &str) -> Option<&JsIDBDatabase> {
        self.databases.get(&(origin.to_string(), name.to_string()))
    }
    
    /// Get mutable reference to open database
    pub fn get_database_mut(&mut self, origin: &str, name: &str) -> Option<&mut JsIDBDatabase> {
        self.databases.get_mut(&(origin.to_string(), name.to_string()))
    }
    
    /// Delete a database
    pub fn delete_database(&mut self, origin: &str, name: &str) -> bool {
        self.databases.remove(&(origin.to_string(), name.to_string())).is_some()
    }
    
    /// List all databases for an origin
    pub fn databases(&self, origin: &str) -> Vec<IDBDatabaseInfo> {
        self.factories.get(origin)
            .map(|f| f.databases())
            .unwrap_or_default()
    }
    
    /// Clear all databases for an origin (for privacy)
    pub fn clear_origin(&mut self, origin: &str) {
        self.factories.remove(origin);
        self.databases.retain(|(o, _), _| o != origin);
    }
    
    /// Clear all IndexedDB data
    pub fn clear_all(&mut self) {
        self.factories.clear();
        self.databases.clear();
    }
}

/// Browser's wrapper around IDBDatabase
#[derive(Debug)]
pub struct IDBDatabase {
    /// Origin this database belongs to
    pub origin: String,
    /// The actual database
    pub inner: JsIDBDatabase,
}

impl IDBDatabase {
    /// Create from fos-js database
    pub fn new(origin: &str, db: JsIDBDatabase) -> Self {
        Self {
            origin: origin.to_string(),
            inner: db,
        }
    }
    
    /// Get database name
    pub fn name(&self) -> &str {
        &self.inner.name
    }
    
    /// Get database version
    pub fn version(&self) -> u64 {
        self.inner.version
    }
    
    /// Get object store names
    pub fn object_store_names(&self) -> Vec<&str> {
        self.inner.object_store_names()
    }
    
    /// Create an object store
    pub fn create_object_store(&mut self, name: &str, options: ObjectStoreOptions) -> &IDBObjectStore {
        self.inner.create_object_store(name, options)
    }
    
    /// Delete an object store
    pub fn delete_object_store(&mut self, name: &str) {
        self.inner.delete_object_store(name);
    }
    
    /// Start a transaction
    pub fn transaction(&self, stores: &[&str], mode: IDBTransactionMode) -> JsIDBTransaction {
        self.inner.transaction(stores, mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_idb_factory() {
        let mut factory = IDBFactory::new();
        
        // Open database for origin
        let request = factory.open("https://example.com", "testdb", Some(1));
        assert_eq!(request.name, "testdb");
        assert_eq!(request.version, 1);
    }
    
    #[test]
    fn test_origin_isolation() {
        let mut factory = IDBFactory::new();
        
        // Create DB for one origin
        let db = JsIDBDatabase::new("mydb", 1);
        factory.store_database("https://example.com", "mydb", db);
        
        // Should be visible for same origin
        assert!(factory.get_database("https://example.com", "mydb").is_some());
        
        // Should not be visible to other origin
        assert!(factory.get_database("https://other.com", "mydb").is_none());
    }
}
