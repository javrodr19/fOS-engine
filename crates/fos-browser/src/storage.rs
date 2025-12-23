//! Storage Integration
//!
//! Integrates fos-js storage APIs: IndexedDB, Cache API, Cookies, LocalStorage.

use fos_js::{
    IDBFactory, CacheStorage, CookieStore,
};
use std::collections::HashMap;

/// Storage manager for the browser
pub struct StorageManager {
    /// IndexedDB factory
    idb: IDBFactory,
    /// Cache storage
    cache: CacheStorage,
    /// Cookie store
    cookies: CookieStore,
    /// LocalStorage per origin
    local_storage: HashMap<String, HashMap<String, String>>,
    /// SessionStorage per origin
    session_storage: HashMap<String, HashMap<String, String>>,
}

impl StorageManager {
    /// Create new storage manager
    pub fn new() -> Self {
        Self {
            idb: IDBFactory::new(),
            cache: CacheStorage::new(),
            cookies: CookieStore::new(),
            local_storage: HashMap::new(),
            session_storage: HashMap::new(),
        }
    }
    
    // === IndexedDB ===
    
    /// Get IDB factory reference
    pub fn idb(&self) -> &IDBFactory {
        &self.idb
    }
    
    /// List all databases
    pub fn idb_databases(&self) -> Vec<String> {
        self.idb.databases().iter().map(|d| d.name.clone()).collect()
    }
    
    /// Delete IndexedDB database
    pub fn idb_delete(&self, name: &str) -> bool {
        self.idb.delete_database(name)
    }
    
    // === Cache API ===
    
    /// Open a cache (creates if not exists)
    pub fn cache_open(&mut self, name: &str) {
        self.cache.open(name);
    }
    
    /// Delete a cache
    pub fn cache_delete(&mut self, name: &str) -> bool {
        self.cache.delete(name)
    }
    
    /// Check if cache exists
    pub fn cache_has(&self, name: &str) -> bool {
        self.cache.has(name)
    }
    
    /// List all caches
    pub fn cache_keys(&self) -> Vec<String> {
        self.cache.keys().iter().map(|s| s.to_string()).collect()
    }
    
    // === Cookies ===
    
    /// Get cookie store reference
    pub fn cookies(&self) -> &CookieStore {
        &self.cookies
    }
    
    /// Get mutable cookie store reference
    pub fn cookies_mut(&mut self) -> &mut CookieStore {
        &mut self.cookies
    }
    
    /// Clear all cookies
    pub fn clear_cookies(&mut self) {
        self.cookies.clear();
    }
    
    /// Get cookies for URL (for Cookie header)
    pub fn get_cookie_header(&self, url: &str, secure: bool) -> String {
        self.cookies.to_cookie_header(url, secure)
    }
    
    // === LocalStorage ===
    
    /// Get localStorage for origin
    fn local_storage_for(&mut self, origin: &str) -> &mut HashMap<String, String> {
        self.local_storage.entry(origin.to_string()).or_default()
    }
    
    /// Set localStorage item
    pub fn local_set(&mut self, origin: &str, key: &str, value: &str) {
        self.local_storage_for(origin).insert(key.to_string(), value.to_string());
    }
    
    /// Get localStorage item
    pub fn local_get(&mut self, origin: &str, key: &str) -> Option<String> {
        self.local_storage_for(origin).get(key).cloned()
    }
    
    /// Remove localStorage item
    pub fn local_remove(&mut self, origin: &str, key: &str) {
        self.local_storage_for(origin).remove(key);
    }
    
    /// Clear localStorage for origin
    pub fn local_clear(&mut self, origin: &str) {
        if let Some(storage) = self.local_storage.get_mut(origin) {
            storage.clear();
        }
    }
    
    /// Get localStorage length
    pub fn local_length(&mut self, origin: &str) -> usize {
        self.local_storage_for(origin).len()
    }
    
    // === SessionStorage ===
    
    /// Get sessionStorage for origin
    fn session_storage_for(&mut self, origin: &str) -> &mut HashMap<String, String> {
        self.session_storage.entry(origin.to_string()).or_default()
    }
    
    /// Set sessionStorage item
    pub fn session_set(&mut self, origin: &str, key: &str, value: &str) {
        self.session_storage_for(origin).insert(key.to_string(), value.to_string());
    }
    
    /// Get sessionStorage item
    pub fn session_get(&mut self, origin: &str, key: &str) -> Option<String> {
        self.session_storage_for(origin).get(key).cloned()
    }
    
    /// Clear all session storage (when browser closes)
    pub fn clear_session_storage(&mut self) {
        self.session_storage.clear();
    }
    
    /// Get storage statistics
    pub fn stats(&self) -> StorageStats {
        StorageStats {
            idb_databases: self.idb.databases().len(),
            cache_count: self.cache.keys().len(),
            local_storage_origins: self.local_storage.len(),
            session_storage_origins: self.session_storage.len(),
        }
    }
}

impl Default for StorageManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Storage statistics
#[derive(Debug, Clone)]
pub struct StorageStats {
    pub idb_databases: usize,
    pub cache_count: usize,
    pub local_storage_origins: usize,
    pub session_storage_origins: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_storage_manager_creation() {
        let manager = StorageManager::new();
        let stats = manager.stats();
        assert_eq!(stats.idb_databases, 0);
    }
    
    #[test]
    fn test_local_storage() {
        let mut manager = StorageManager::new();
        manager.local_set("https://example.com", "key", "value");
        
        assert_eq!(
            manager.local_get("https://example.com", "key"),
            Some("value".to_string())
        );
    }
    
    #[test]
    fn test_session_storage() {
        let mut manager = StorageManager::new();
        manager.session_set("https://example.com", "temp", "data");
        
        assert_eq!(
            manager.session_get("https://example.com", "temp"),
            Some("data".to_string())
        );
        
        manager.clear_session_storage();
        assert_eq!(manager.session_get("https://example.com", "temp"), None);
    }
    
    #[test]
    fn test_cache() {
        let mut manager = StorageManager::new();
        manager.cache_open("v1");
        assert!(manager.cache_has("v1"));
    }
}
