//! HTTP Response Cache
//!
//! Caches HTTP responses with ETag and max-age support.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Cached response entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Response body
    pub body: Vec<u8>,
    /// Content type
    pub content_type: String,
    /// ETag for validation
    pub etag: Option<String>,
    /// Time when cached
    pub cached_at: Instant,
    /// Max age (time to live)
    pub max_age: Duration,
    /// Last accessed
    pub last_accessed: Instant,
    /// Access count
    pub access_count: u32,
}

impl CacheEntry {
    /// Check if entry is expired
    pub fn is_expired(&self) -> bool {
        self.cached_at.elapsed() > self.max_age
    }
    
    /// Check if entry is fresh
    pub fn is_fresh(&self) -> bool {
        !self.is_expired()
    }
    
    /// Get remaining TTL
    pub fn ttl(&self) -> Duration {
        let elapsed = self.cached_at.elapsed();
        if elapsed >= self.max_age {
            Duration::ZERO
        } else {
            self.max_age - elapsed
        }
    }
}

/// HTTP cache
pub struct HttpCache {
    entries: HashMap<String, CacheEntry>,
    max_entries: usize,
    max_size_bytes: usize,
    current_size: usize,
}

impl HttpCache {
    /// Create a new cache with limits
    pub fn new(max_entries: usize, max_size_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_entries,
            max_size_bytes,
            current_size: 0,
        }
    }
    
    /// Get a cached response
    pub fn get(&mut self, url: &str) -> Option<&CacheEntry> {
        // Check if exists and not expired
        if let Some(entry) = self.entries.get_mut(url) {
            if entry.is_fresh() {
                entry.last_accessed = Instant::now();
                entry.access_count += 1;
                return self.entries.get(url);
            } else {
                // Remove expired entry
                let size = entry.body.len();
                self.current_size = self.current_size.saturating_sub(size);
                self.entries.remove(url);
            }
        }
        None
    }
    
    /// Check if URL is cached (without updating access time)
    pub fn contains(&self, url: &str) -> bool {
        self.entries.get(url).map(|e| e.is_fresh()).unwrap_or(false)
    }
    
    /// Store a response in cache
    pub fn put(&mut self, url: &str, body: Vec<u8>, content_type: &str, etag: Option<String>, max_age: Duration) {
        let size = body.len();
        
        // Evict if necessary
        while self.entries.len() >= self.max_entries || 
              self.current_size + size > self.max_size_bytes {
            if !self.evict_one() {
                break; // Nothing to evict
            }
        }
        
        // Check if we can fit this entry
        if size > self.max_size_bytes {
            return; // Too large
        }
        
        // Remove existing entry if present
        if let Some(existing) = self.entries.remove(url) {
            self.current_size = self.current_size.saturating_sub(existing.body.len());
        }
        
        let now = Instant::now();
        self.entries.insert(url.to_string(), CacheEntry {
            body,
            content_type: content_type.to_string(),
            etag,
            cached_at: now,
            max_age,
            last_accessed: now,
            access_count: 0,
        });
        
        self.current_size += size;
    }
    
    /// Evict one entry (LRU-ish)
    fn evict_one(&mut self) -> bool {
        // Find least recently accessed entry
        let oldest = self.entries.iter()
            .min_by_key(|(_, e)| e.last_accessed)
            .map(|(k, _)| k.clone());
        
        if let Some(key) = oldest {
            if let Some(entry) = self.entries.remove(&key) {
                self.current_size = self.current_size.saturating_sub(entry.body.len());
                return true;
            }
        }
        false
    }
    
    /// Remove all expired entries
    pub fn cleanup(&mut self) {
        let expired: Vec<_> = self.entries.iter()
            .filter(|(_, e)| e.is_expired())
            .map(|(k, _)| k.clone())
            .collect();
        
        for key in expired {
            if let Some(entry) = self.entries.remove(&key) {
                self.current_size = self.current_size.saturating_sub(entry.body.len());
            }
        }
    }
    
    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_size = 0;
    }
    
    /// Get cache stats
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entry_count: self.entries.len(),
            total_size: self.current_size,
            max_entries: self.max_entries,
            max_size: self.max_size_bytes,
        }
    }
    
    /// Get ETag for conditional request
    pub fn get_etag(&self, url: &str) -> Option<String> {
        self.entries.get(url).and_then(|e| e.etag.clone())
    }
}

impl Default for HttpCache {
    fn default() -> Self {
        Self::new(1000, 50 * 1024 * 1024) // 1000 entries, 50MB
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entry_count: usize,
    pub total_size: usize,
    pub max_entries: usize,
    pub max_size: usize,
}

impl CacheStats {
    pub fn utilization(&self) -> f64 {
        if self.max_size == 0 {
            0.0
        } else {
            self.total_size as f64 / self.max_size as f64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_basic() {
        let mut cache = HttpCache::new(100, 1024 * 1024);
        
        cache.put("http://example.com/page", b"Hello".to_vec(), "text/html", None, Duration::from_secs(60));
        
        assert!(cache.contains("http://example.com/page"));
        
        let entry = cache.get("http://example.com/page").unwrap();
        assert_eq!(entry.body, b"Hello");
    }
    
    #[test]
    fn test_cache_etag() {
        let mut cache = HttpCache::new(100, 1024 * 1024);
        
        cache.put(
            "http://example.com/api", 
            b"data".to_vec(), 
            "application/json", 
            Some("abc123".to_string()), 
            Duration::from_secs(60)
        );
        
        assert_eq!(cache.get_etag("http://example.com/api"), Some("abc123".to_string()));
    }
    
    #[test]
    fn test_cache_max_entries() {
        let mut cache = HttpCache::new(3, 1024 * 1024);
        
        cache.put("url1", b"a".to_vec(), "text/plain", None, Duration::from_secs(60));
        cache.put("url2", b"b".to_vec(), "text/plain", None, Duration::from_secs(60));
        cache.put("url3", b"c".to_vec(), "text/plain", None, Duration::from_secs(60));
        cache.put("url4", b"d".to_vec(), "text/plain", None, Duration::from_secs(60));
        
        assert!(cache.stats().entry_count <= 3);
    }
    
    #[test]
    fn test_cache_max_size() {
        let mut cache = HttpCache::new(100, 10); // 10 bytes max
        
        cache.put("url", b"12345".to_vec(), "text/plain", None, Duration::from_secs(60));
        assert!(cache.contains("url"));
        
        cache.put("url2", b"123456789012".to_vec(), "text/plain", None, Duration::from_secs(60)); // Too big
        assert!(!cache.contains("url2"));
    }
    
    #[test]
    fn test_cache_stats() {
        let mut cache = HttpCache::new(100, 1024);
        
        cache.put("url1", b"hello".to_vec(), "text/plain", None, Duration::from_secs(60));
        cache.put("url2", b"world".to_vec(), "text/plain", None, Duration::from_secs(60));
        
        let stats = cache.stats();
        assert_eq!(stats.entry_count, 2);
        assert_eq!(stats.total_size, 10);
    }
    
    #[test]
    fn test_cache_clear() {
        let mut cache = HttpCache::new(100, 1024);
        
        cache.put("url", b"data".to_vec(), "text/plain", None, Duration::from_secs(60));
        cache.clear();
        
        assert!(!cache.contains("url"));
        assert_eq!(cache.stats().entry_count, 0);
    }
}
