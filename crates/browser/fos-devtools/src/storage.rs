//! Storage Inspector
//!
//! LocalStorage, SessionStorage, Cookies, IndexedDB, Cache Storage inspection.

use std::collections::HashMap;

/// Storage type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageType { LocalStorage, SessionStorage, Cookies, IndexedDb, CacheStorage }

/// Storage entry
#[derive(Debug, Clone)]
pub struct StorageEntry {
    pub key: String,
    pub value: String,
    pub size: usize,
    pub storage_type: StorageType,
}

/// Cookie entry for inspection
#[derive(Debug, Clone)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<u64>,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: SameSite,
    pub size: usize,
}

/// SameSite attribute
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SameSite { #[default] None, Lax, Strict }

/// IndexedDB database info
#[derive(Debug, Clone)]
pub struct IdbDatabaseInfo {
    pub name: String,
    pub version: u64,
    pub object_stores: Vec<IdbObjectStoreInfo>,
}

/// IndexedDB object store info
#[derive(Debug, Clone)]
pub struct IdbObjectStoreInfo {
    pub name: String,
    pub key_path: Option<String>,
    pub auto_increment: bool,
    pub indexes: Vec<IdbIndexInfo>,
    pub record_count: usize,
}

/// IndexedDB index info
#[derive(Debug, Clone)]
pub struct IdbIndexInfo {
    pub name: String,
    pub key_path: String,
    pub unique: bool,
    pub multi_entry: bool,
}

/// Cache info
#[derive(Debug, Clone)]
pub struct CacheInfo {
    pub name: String,
    pub entries: Vec<CacheEntry>,
}

/// Cache entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub url: String,
    pub response_type: String,
    pub content_type: Option<String>,
    pub content_length: Option<usize>,
}

/// Storage inspector
#[derive(Debug, Default)]
pub struct StorageInspector {
    origin: String,
}

impl StorageInspector {
    pub fn new(origin: &str) -> Self { Self { origin: origin.into() } }
    
    /// Get all local storage entries
    pub fn get_local_storage(&self, storage: &HashMap<String, String>) -> Vec<StorageEntry> {
        storage.iter().map(|(k, v)| StorageEntry {
            key: k.clone(), value: v.clone(), size: k.len() + v.len(), storage_type: StorageType::LocalStorage,
        }).collect()
    }
    
    /// Get all session storage entries
    pub fn get_session_storage(&self, storage: &HashMap<String, String>) -> Vec<StorageEntry> {
        storage.iter().map(|(k, v)| StorageEntry {
            key: k.clone(), value: v.clone(), size: k.len() + v.len(), storage_type: StorageType::SessionStorage,
        }).collect()
    }
    
    /// Get storage quota info
    pub fn get_quota_info(&self) -> StorageQuota {
        StorageQuota { usage: 0, quota: 50 * 1024 * 1024, persistent: false }
    }
    
    /// Clear storage by type
    pub fn clear_storage(&self, storage_type: StorageType) -> ClearResult {
        ClearResult { storage_type, cleared: true, items_removed: 0 }
    }
}

/// Storage quota info
#[derive(Debug, Clone)]
pub struct StorageQuota {
    pub usage: usize,
    pub quota: usize,
    pub persistent: bool,
}

impl StorageQuota {
    pub fn usage_percentage(&self) -> f64 {
        if self.quota == 0 { 0.0 } else { (self.usage as f64 / self.quota as f64) * 100.0 }
    }
}

/// Clear result
#[derive(Debug, Clone)]
pub struct ClearResult {
    pub storage_type: StorageType,
    pub cleared: bool,
    pub items_removed: usize,
}

/// Storage panel for DevTools
#[derive(Debug, Default)]
pub struct StoragePanel {
    inspectors: HashMap<String, StorageInspector>,
    selected_origin: Option<String>,
    selected_type: Option<StorageType>,
}

impl StoragePanel {
    pub fn new() -> Self { Self::default() }
    
    pub fn add_origin(&mut self, origin: &str) {
        self.inspectors.insert(origin.into(), StorageInspector::new(origin));
    }
    
    pub fn select_origin(&mut self, origin: &str) { self.selected_origin = Some(origin.into()); }
    pub fn select_type(&mut self, storage_type: StorageType) { self.selected_type = Some(storage_type); }
    
    pub fn get_origins(&self) -> Vec<&str> {
        self.inspectors.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_storage_entry() {
        let entry = StorageEntry {
            key: "test".into(), value: "value".into(), size: 9, storage_type: StorageType::LocalStorage,
        };
        assert_eq!(entry.size, 9);
    }
    
    #[test]
    fn test_quota() {
        let quota = StorageQuota { usage: 25 * 1024 * 1024, quota: 50 * 1024 * 1024, persistent: false };
        assert!((quota.usage_percentage() - 50.0).abs() < 0.01);
    }
}
