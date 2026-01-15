//! HTTP Response Cache
//!
//! Caches HTTP responses with ETag and max-age support.
//! Implements stale-while-revalidate for improved performance.

use std::collections::HashMap;
use std::time::{Duration, Instant};

// ============================================================================
// Cache Result
// ============================================================================

/// Result of a cache lookup
#[derive(Debug, Clone)]
pub enum CacheResult {
    /// Cache hit - response is fresh
    Fresh(CacheEntry),
    /// Stale but usable - revalidation in progress
    Stale(CacheEntry),
    /// Must revalidate before use
    MustRevalidate(CacheValidators),
    /// Cache miss - not in cache
    Miss,
}

impl CacheResult {
    /// Check if this is a hit (fresh or stale)
    pub fn is_hit(&self) -> bool {
        matches!(self, CacheResult::Fresh(_) | CacheResult::Stale(_))
    }
    
    /// Get the entry if available
    pub fn entry(&self) -> Option<&CacheEntry> {
        match self {
            CacheResult::Fresh(e) | CacheResult::Stale(e) => Some(e),
            _ => None,
        }
    }
}

/// Validators for conditional requests
#[derive(Debug, Clone)]
pub struct CacheValidators {
    /// ETag value
    pub etag: Option<String>,
    /// Last-Modified value
    pub last_modified: Option<String>,
}

impl CacheValidators {
    pub fn is_empty(&self) -> bool {
        self.etag.is_none() && self.last_modified.is_none()
    }
}

// ============================================================================
// Cache Partitioning
// ============================================================================

/// Cache partition key for privacy (prevents cross-site tracking)
/// Caches are partitioned by top-level site to isolate resources
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CachePartitionKey {
    /// Top-level site (eTLD+1) e.g. "example.com"
    pub top_level_site: String,
    /// Whether the request is cross-site
    pub is_cross_site: bool,
}

impl CachePartitionKey {
    /// Create a new partition key
    pub fn new(top_level_site: &str, is_cross_site: bool) -> Self {
        Self {
            top_level_site: top_level_site.to_lowercase(),
            is_cross_site,
        }
    }
    
    /// Create for same-site request
    pub fn same_site(site: &str) -> Self {
        Self::new(site, false)
    }
    
    /// Create for cross-site request
    pub fn cross_site(top_level: &str) -> Self {
        Self::new(top_level, true)
    }
    
    /// Create partitioned cache key from URL and partition
    pub fn partition_url(&self, url: &str) -> String {
        format!("{}|{}|{}", self.top_level_site, self.is_cross_site, url)
    }
    
    /// Extract eTLD+1 from a URL (simplified)
    pub fn extract_site(url: &str) -> Option<String> {
        // Simple extraction: find host portion
        let url = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
        let host = url.split('/').next()?;
        let host = host.split(':').next()?; // Remove port
        
        // Simplified eTLD+1: just take domain + TLD
        let parts: Vec<&str> = host.split('.').collect();
        if parts.len() >= 2 {
            Some(format!("{}.{}", parts[parts.len() - 2], parts[parts.len() - 1]))
        } else {
            Some(host.to_string())
        }
    }
}

// ============================================================================
// Cache Entry
// ============================================================================

/// Cached response entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Response body
    pub body: Vec<u8>,
    /// Content type
    pub content_type: String,
    /// ETag for validation
    pub etag: Option<String>,
    /// Last-Modified header
    pub last_modified: Option<String>,
    /// Time when cached
    pub cached_at: Instant,
    /// Max age (time to live)
    pub max_age: Duration,
    /// Stale-while-revalidate window
    pub stale_while_revalidate: Duration,
    /// Last accessed
    pub last_accessed: Instant,
    /// Access count
    pub access_count: u32,
    /// Revalidation in progress
    pub revalidating: bool,
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
    
    /// Check if entry can be served stale while revalidating
    pub fn can_stale_while_revalidate(&self) -> bool {
        if !self.is_expired() {
            return false;
        }
        let stale_age = self.cached_at.elapsed().saturating_sub(self.max_age);
        stale_age < self.stale_while_revalidate
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
    
    /// Get validators for conditional request
    pub fn validators(&self) -> CacheValidators {
        CacheValidators {
            etag: self.etag.clone(),
            last_modified: self.last_modified.clone(),
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
    
    /// Lookup with full cache semantics (stale-while-revalidate support)
    pub fn lookup(&mut self, url: &str) -> CacheResult {
        let Some(entry) = self.entries.get_mut(url) else {
            return CacheResult::Miss;
        };
        
        entry.last_accessed = Instant::now();
        entry.access_count += 1;
        
        if entry.is_fresh() {
            return CacheResult::Fresh(entry.clone());
        }
        
        if entry.can_stale_while_revalidate() && !entry.revalidating {
            entry.revalidating = true;
            return CacheResult::Stale(entry.clone());
        }
        
        let validators = entry.validators();
        if !validators.is_empty() {
            return CacheResult::MustRevalidate(validators);
        }
        
        // Expired with no validators - treat as miss
        let size = entry.body.len();
        self.current_size = self.current_size.saturating_sub(size);
        self.entries.remove(url);
        CacheResult::Miss
    }
    
    /// Mark revalidation complete (update or remove stale entry)
    pub fn complete_revalidation(&mut self, url: &str, still_valid: bool) {
        if let Some(entry) = self.entries.get_mut(url) {
            entry.revalidating = false;
            if still_valid {
                // Refresh the entry
                entry.cached_at = Instant::now();
            }
        }
    }
    
    /// Check if URL is cached (without updating access time)
    pub fn contains(&self, url: &str) -> bool {
        self.entries.get(url).map(|e| e.is_fresh()).unwrap_or(false)
    }
    
    /// Store a response in cache
    pub fn put(&mut self, url: &str, body: Vec<u8>, content_type: &str, etag: Option<String>, max_age: Duration) {
        self.put_full(url, body, content_type, etag, None, max_age, Duration::ZERO);
    }
    
    /// Store a response with full options
    pub fn put_full(
        &mut self,
        url: &str,
        body: Vec<u8>,
        content_type: &str,
        etag: Option<String>,
        last_modified: Option<String>,
        max_age: Duration,
        stale_while_revalidate: Duration,
    ) {
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
            last_modified,
            cached_at: now,
            max_age,
            stale_while_revalidate,
            last_accessed: now,
            access_count: 0,
            revalidating: false,
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
    
    // ========================================================================
    // Partitioned Cache Methods (Privacy)
    // ========================================================================
    
    /// Get with partition key (for cross-site isolation)
    pub fn get_partitioned(&mut self, url: &str, partition: &CachePartitionKey) -> Option<&CacheEntry> {
        let key = partition.partition_url(url);
        if let Some(entry) = self.entries.get_mut(&key) {
            if entry.is_fresh() {
                entry.last_accessed = Instant::now();
                entry.access_count += 1;
                return self.entries.get(&key);
            }
        }
        None
    }
    
    /// Put with partition key
    pub fn put_partitioned(
        &mut self,
        url: &str,
        partition: &CachePartitionKey,
        body: Vec<u8>,
        content_type: &str,
        etag: Option<String>,
        max_age: Duration,
    ) {
        let key = partition.partition_url(url);
        self.put_full(&key, body, content_type, etag, None, max_age, Duration::ZERO);
    }
    
    /// Lookup with partition key (full semantics)
    pub fn lookup_partitioned(&mut self, url: &str, partition: &CachePartitionKey) -> CacheResult {
        let key = partition.partition_url(url);
        self.lookup(&key)
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
