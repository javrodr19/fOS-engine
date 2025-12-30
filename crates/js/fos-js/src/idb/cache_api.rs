//! Cache API
//!
//! Service Worker Cache for offline support.

use std::collections::HashMap;

/// CacheStorage - container for named caches
#[derive(Debug, Default)]
pub struct CacheStorage {
    caches: HashMap<String, Cache>,
}

/// Cache - named cache for request/response pairs
#[derive(Debug, Clone, Default)]
pub struct Cache {
    entries: Vec<CacheEntry>,
}

/// Cache entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub request: CacheRequest,
    pub response: CacheResponse,
}

/// Cache request (simplified)
#[derive(Debug, Clone)]
pub struct CacheRequest {
    pub url: String,
    pub method: String,
    pub headers: Vec<(String, String)>,
}

/// Cache response
#[derive(Debug, Clone)]
pub struct CacheResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl CacheStorage {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Open or create a cache
    pub fn open(&mut self, name: &str) -> &mut Cache {
        self.caches.entry(name.to_string()).or_insert_with(Cache::new)
    }
    
    /// Check if cache exists
    pub fn has(&self, name: &str) -> bool {
        self.caches.contains_key(name)
    }
    
    /// Delete a cache
    pub fn delete(&mut self, name: &str) -> bool {
        self.caches.remove(name).is_some()
    }
    
    /// List all cache names
    pub fn keys(&self) -> Vec<&str> {
        self.caches.keys().map(|s| s.as_str()).collect()
    }
    
    /// Find a matching response across all caches
    pub fn match_request(&self, request: &CacheRequest) -> Option<&CacheResponse> {
        for cache in self.caches.values() {
            if let Some(response) = cache.match_request(request) {
                return Some(response);
            }
        }
        None
    }
}

impl Cache {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add URL to cache (fetch and store)
    pub fn add(&mut self, url: &str) {
        // Would fetch and cache
        self.entries.push(CacheEntry {
            request: CacheRequest {
                url: url.to_string(),
                method: "GET".to_string(),
                headers: vec![],
            },
            response: CacheResponse {
                status: 200,
                status_text: "OK".to_string(),
                headers: vec![],
                body: vec![],
            },
        });
    }
    
    /// Add multiple URLs
    pub fn add_all(&mut self, urls: &[&str]) {
        for url in urls {
            self.add(url);
        }
    }
    
    /// Put a request/response pair
    pub fn put(&mut self, request: CacheRequest, response: CacheResponse) {
        // Remove existing
        self.entries.retain(|e| e.request.url != request.url);
        self.entries.push(CacheEntry { request, response });
    }
    
    /// Find a matching response
    pub fn match_request(&self, request: &CacheRequest) -> Option<&CacheResponse> {
        self.entries.iter()
            .find(|e| Self::requests_match(&e.request, request))
            .map(|e| &e.response)
    }
    
    /// Find all matching responses
    pub fn match_all(&self, request: Option<&CacheRequest>) -> Vec<&CacheResponse> {
        match request {
            Some(req) => self.entries.iter()
                .filter(|e| Self::requests_match(&e.request, req))
                .map(|e| &e.response)
                .collect(),
            None => self.entries.iter().map(|e| &e.response).collect(),
        }
    }
    
    /// Delete matching entries
    pub fn delete(&mut self, request: &CacheRequest) -> bool {
        let len = self.entries.len();
        self.entries.retain(|e| !Self::requests_match(&e.request, request));
        self.entries.len() < len
    }
    
    /// List all cached requests
    pub fn keys(&self) -> Vec<&CacheRequest> {
        self.entries.iter().map(|e| &e.request).collect()
    }
    
    fn requests_match(cached: &CacheRequest, incoming: &CacheRequest) -> bool {
        cached.url == incoming.url && cached.method == incoming.method
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_storage() {
        let mut storage = CacheStorage::new();
        storage.open("v1");
        
        assert!(storage.has("v1"));
        assert_eq!(storage.keys().len(), 1);
    }
    
    #[test]
    fn test_cache_put_match() {
        let mut cache = Cache::new();
        
        let request = CacheRequest {
            url: "https://example.com/api".into(),
            method: "GET".into(),
            headers: vec![],
        };
        let response = CacheResponse {
            status: 200,
            status_text: "OK".into(),
            headers: vec![],
            body: b"data".to_vec(),
        };
        
        cache.put(request.clone(), response);
        
        let result = cache.match_request(&request);
        assert!(result.is_some());
        assert_eq!(result.unwrap().status, 200);
    }
}
