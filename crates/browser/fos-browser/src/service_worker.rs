//! Service Worker integration
//!
//! Offline support and background sync via Service Workers.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Service worker state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceWorkerState {
    Parsed,
    Installing,
    Installed,
    Activating,
    Activated,
    Redundant,
}

/// A registered service worker
#[derive(Debug, Clone)]
pub struct ServiceWorker {
    pub id: u64,
    pub scope: String,
    pub script_url: String,
    pub state: ServiceWorkerState,
}

/// Service worker registration
#[derive(Debug)]
pub struct ServiceWorkerRegistration {
    pub scope: String,
    pub installing: Option<ServiceWorker>,
    pub waiting: Option<ServiceWorker>,
    pub active: Option<ServiceWorker>,
}

/// Service worker container (navigator.serviceWorker)
#[derive(Debug, Default)]
pub struct ServiceWorkerContainer {
    registrations: HashMap<String, ServiceWorkerRegistration>,
    next_id: u64,
}

impl ServiceWorkerContainer {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a service worker
    pub fn register(&mut self, script_url: &str, scope: Option<&str>) -> Result<u64, ServiceWorkerError> {
        let scope = scope
            .map(String::from)
            .unwrap_or_else(|| Self::default_scope(script_url));
        
        let id = self.next_id;
        self.next_id += 1;
        
        let worker = ServiceWorker {
            id,
            scope: scope.clone(),
            script_url: script_url.to_string(),
            state: ServiceWorkerState::Parsed,
        };
        
        let registration = ServiceWorkerRegistration {
            scope: scope.clone(),
            installing: Some(worker),
            waiting: None,
            active: None,
        };
        
        self.registrations.insert(scope, registration);
        
        Ok(id)
    }
    
    /// Get registration for a URL
    pub fn get_registration(&self, url: &str) -> Option<&ServiceWorkerRegistration> {
        // Find the registration with the longest matching scope
        let mut best_match: Option<&ServiceWorkerRegistration> = None;
        let mut best_len = 0;
        
        for reg in self.registrations.values() {
            if url.starts_with(&reg.scope) && reg.scope.len() > best_len {
                best_match = Some(reg);
                best_len = reg.scope.len();
            }
        }
        
        best_match
    }
    
    /// Unregister a service worker
    pub fn unregister(&mut self, scope: &str) -> bool {
        self.registrations.remove(scope).is_some()
    }
    
    /// Get all registrations
    pub fn get_registrations(&self) -> Vec<&ServiceWorkerRegistration> {
        self.registrations.values().collect()
    }
    
    fn default_scope(script_url: &str) -> String {
        // Default scope is the directory containing the script
        if let Some(pos) = script_url.rfind('/') {
            script_url[..=pos].to_string()
        } else {
            "/".to_string()
        }
    }
}

/// Cache storage for service workers
#[derive(Debug, Default)]
pub struct CacheStorage {
    caches: HashMap<String, Cache>,
}

impl CacheStorage {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Open or create a cache
    pub fn open(&mut self, name: &str) -> &mut Cache {
        self.caches.entry(name.to_string()).or_insert_with(Cache::new)
    }
    
    /// Delete a cache
    pub fn delete(&mut self, name: &str) -> bool {
        self.caches.remove(name).is_some()
    }
    
    /// Check if cache exists
    pub fn has(&self, name: &str) -> bool {
        self.caches.contains_key(name)
    }
    
    /// Get all cache names
    pub fn keys(&self) -> Vec<&str> {
        self.caches.keys().map(|s| s.as_str()).collect()
    }
}

/// A cache for storing request/response pairs
#[derive(Debug, Default)]
pub struct Cache {
    entries: HashMap<String, CachedResponse>,
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add a response to the cache
    pub fn put(&mut self, url: &str, response: CachedResponse) {
        self.entries.insert(url.to_string(), response);
    }
    
    /// Get a cached response
    pub fn match_url(&self, url: &str) -> Option<&CachedResponse> {
        self.entries.get(url)
    }
    
    /// Delete a cached response
    pub fn delete(&mut self, url: &str) -> bool {
        self.entries.remove(url).is_some()
    }
    
    /// Get all cached URLs
    pub fn keys(&self) -> Vec<&str> {
        self.entries.keys().map(|s| s.as_str()).collect()
    }
}

/// A cached response
#[derive(Debug, Clone)]
pub struct CachedResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl CachedResponse {
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            status_text: "OK".to_string(),
            headers: HashMap::new(),
            body,
        }
    }
}

/// Service worker errors
#[derive(Debug)]
pub enum ServiceWorkerError {
    SecurityError(String),
    NetworkError(String),
    NotFound,
}

impl std::fmt::Display for ServiceWorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SecurityError(msg) => write!(f, "Security error: {}", msg),
            Self::NetworkError(msg) => write!(f, "Network error: {}", msg),
            Self::NotFound => write!(f, "Service worker not found"),
        }
    }
}

impl std::error::Error for ServiceWorkerError {}

/// Service worker manager - coordinates all service worker functionality
#[derive(Debug, Default)]
pub struct ServiceWorkerManager {
    container: ServiceWorkerContainer,
    cache_storage: CacheStorage,
}

impl ServiceWorkerManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn container(&self) -> &ServiceWorkerContainer {
        &self.container
    }
    
    pub fn container_mut(&mut self) -> &mut ServiceWorkerContainer {
        &mut self.container
    }
    
    pub fn cache_storage(&self) -> &CacheStorage {
        &self.cache_storage
    }
    
    pub fn cache_storage_mut(&mut self) -> &mut CacheStorage {
        &mut self.cache_storage
    }
    
    /// Check if a URL can be served from cache
    pub fn can_serve_offline(&self, url: &str) -> bool {
        for cache in self.cache_storage.caches.values() {
            if cache.entries.contains_key(url) {
                return true;
            }
        }
        false
    }
    
    /// Get cached response for URL
    pub fn get_cached(&self, url: &str) -> Option<&CachedResponse> {
        for cache in self.cache_storage.caches.values() {
            if let Some(response) = cache.match_url(url) {
                return Some(response);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_service_worker_registration() {
        let mut container = ServiceWorkerContainer::new();
        let id = container.register("/sw.js", Some("/app/")).unwrap();
        
        assert!(container.get_registration("/app/page.html").is_some());
        assert!(container.get_registration("/other/page.html").is_none());
    }
    
    #[test]
    fn test_cache_storage() {
        let mut storage = CacheStorage::new();
        let cache = storage.open("v1");
        
        cache.put("/index.html", CachedResponse::new(200, b"<html>".to_vec()));
        assert!(cache.match_url("/index.html").is_some());
    }
}
