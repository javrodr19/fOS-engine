//! Resource Deduplication Integration
//!
//! Roaring bitmaps for efficient deduplication of resources.

use std::collections::HashMap;

/// Roaring bitmap for efficient set operations
#[derive(Debug, Clone, Default)]
pub struct RoaringBitmap {
    /// Values stored in sorted array (simplified implementation)
    values: Vec<u32>,
}

impl RoaringBitmap {
    /// Create new empty bitmap
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if contains a value
    pub fn contains(&self, value: u32) -> bool {
        self.values.binary_search(&value).is_ok()
    }
    
    /// Insert a value
    pub fn insert(&mut self, value: u32) -> bool {
        match self.values.binary_search(&value) {
            Ok(_) => false,
            Err(idx) => {
                self.values.insert(idx, value);
                true
            }
        }
    }
    
    /// Remove a value
    pub fn remove(&mut self, value: u32) -> bool {
        match self.values.binary_search(&value) {
            Ok(idx) => {
                self.values.remove(idx);
                true
            }
            Err(_) => false,
        }
    }
    
    /// Count of values
    pub fn len(&self) -> usize {
        self.values.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
    
    /// Clear all values
    pub fn clear(&mut self) {
        self.values.clear();
    }
    
    /// Intersection with another bitmap
    pub fn intersection(&self, other: &Self) -> Self {
        let mut result = Self::new();
        for &v in &self.values {
            if other.contains(v) {
                result.values.push(v);
            }
        }
        result
    }
    
    /// Union with another bitmap
    pub fn union(&self, other: &Self) -> Self {
        let mut result = self.clone();
        for &v in &other.values {
            result.insert(v);
        }
        result
    }
}

/// Resource deduplication manager
pub struct DeduplicationManager {
    /// Seen URL hashes
    url_hashes: RoaringBitmap,
    /// Seen content hashes
    content_hashes: RoaringBitmap,
    /// URL to hash mapping
    url_to_hash: HashMap<String, u32>,
    /// Stats
    stats: DedupStats,
}

/// Deduplication statistics
#[derive(Debug, Clone, Default)]
pub struct DedupStats {
    pub urls_tracked: usize,
    pub duplicates_found: usize,
    pub bytes_saved: usize,
}

impl DeduplicationManager {
    /// Create new manager
    pub fn new() -> Self {
        Self {
            url_hashes: RoaringBitmap::new(),
            content_hashes: RoaringBitmap::new(),
            url_to_hash: HashMap::new(),
            stats: DedupStats::default(),
        }
    }
    
    /// Check if URL is duplicate
    pub fn is_url_duplicate(&self, url: &str) -> bool {
        let hash = Self::hash_string(url);
        self.url_hashes.contains(hash)
    }
    
    /// Track a URL
    pub fn track_url(&mut self, url: &str) -> bool {
        let hash = Self::hash_string(url);
        if self.url_hashes.insert(hash) {
            self.url_to_hash.insert(url.to_string(), hash);
            self.stats.urls_tracked += 1;
            true
        } else {
            self.stats.duplicates_found += 1;
            false
        }
    }
    
    /// Check if content is duplicate 
    pub fn is_content_duplicate(&self, content: &[u8]) -> bool {
        let hash = Self::hash_bytes(content);
        self.content_hashes.contains(hash)
    }
    
    /// Track content
    pub fn track_content(&mut self, content: &[u8]) -> bool {
        let hash = Self::hash_bytes(content);
        if self.content_hashes.insert(hash) {
            true
        } else {
            self.stats.duplicates_found += 1;
            self.stats.bytes_saved += content.len();
            false
        }
    }
    
    /// Simple string hash
    fn hash_string(s: &str) -> u32 {
        let mut hash: u32 = 5381;
        for byte in s.bytes() {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
        }
        hash
    }
    
    /// Simple bytes hash
    fn hash_bytes(data: &[u8]) -> u32 {
        let mut hash: u32 = 5381;
        for &byte in data {
            hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
        }
        hash
    }
    
    /// Get stats
    pub fn stats(&self) -> &DedupStats {
        &self.stats
    }
    
    /// Clear all tracking
    pub fn clear(&mut self) {
        self.url_hashes.clear();
        self.content_hashes.clear();
        self.url_to_hash.clear();
        self.stats = DedupStats::default();
    }
}

impl Default for DeduplicationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_roaring_basic() {
        let mut bitmap = RoaringBitmap::new();
        assert!(bitmap.is_empty());
        
        bitmap.insert(42);
        assert!(bitmap.contains(42));
        
        bitmap.remove(42);
        assert!(!bitmap.contains(42));
    }
    
    #[test]
    fn test_dedup_urls() {
        let mut manager = DeduplicationManager::new();
        
        assert!(manager.track_url("https://example.com"));
        assert!(!manager.track_url("https://example.com")); // Duplicate
        
        assert!(manager.is_url_duplicate("https://example.com"));
        assert_eq!(manager.stats().duplicates_found, 1);
    }
    
    #[test]
    fn test_dedup_content() {
        let mut manager = DeduplicationManager::new();
        let content = b"Hello, World!";
        
        assert!(manager.track_content(content));
        assert!(!manager.track_content(content)); // Duplicate
        
        assert!(manager.is_content_duplicate(content));
    }
}
