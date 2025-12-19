//! Image cache with LRU eviction
//!
//! Caches decoded images by URL/path with memory limits.

use std::collections::HashMap;
use super::DecodedImage;

/// Cache key for images
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImageKey {
    /// URL or file path
    pub source: String,
    /// Target width (0 = original)
    pub width: u32,
    /// Target height (0 = original)
    pub height: u32,
}

impl ImageKey {
    /// Create a key for original size
    pub fn original(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            width: 0,
            height: 0,
        }
    }
    
    /// Create a key for specific size
    pub fn sized(source: impl Into<String>, width: u32, height: u32) -> Self {
        Self {
            source: source.into(),
            width,
            height,
        }
    }
}

/// LRU image cache
pub struct ImageCache {
    /// Cached images
    entries: HashMap<ImageKey, CacheEntry>,
    /// Maximum memory in bytes
    max_memory: usize,
    /// Current memory usage
    current_memory: usize,
    /// Access counter for LRU
    access_counter: u64,
    /// Statistics
    pub hits: u64,
    pub misses: u64,
}

struct CacheEntry {
    image: DecodedImage,
    last_access: u64,
}

impl ImageCache {
    /// Create a new cache with memory limit (in bytes)
    pub fn new(max_memory: usize) -> Self {
        Self {
            entries: HashMap::new(),
            max_memory,
            current_memory: 0,
            access_counter: 0,
            hits: 0,
            misses: 0,
        }
    }
    
    /// Create with default 50MB limit
    pub fn default_limit() -> Self {
        Self::new(50 * 1024 * 1024)
    }
    
    /// Get an image from cache
    pub fn get(&mut self, key: &ImageKey) -> Option<&DecodedImage> {
        self.access_counter += 1;
        
        if let Some(entry) = self.entries.get_mut(key) {
            entry.last_access = self.access_counter;
            self.hits += 1;
            Some(&entry.image)
        } else {
            self.misses += 1;
            None
        }
    }
    
    /// Insert an image into cache
    pub fn insert(&mut self, key: ImageKey, image: DecodedImage) {
        let size = image.memory_size();
        
        // Evict until we have space
        while self.current_memory + size > self.max_memory && !self.entries.is_empty() {
            self.evict_lru();
        }
        
        // Don't cache if image is larger than entire cache
        if size > self.max_memory {
            return;
        }
        
        self.access_counter += 1;
        self.current_memory += size;
        
        self.entries.insert(key, CacheEntry {
            image,
            last_access: self.access_counter,
        });
    }
    
    /// Get or insert with decoder function
    pub fn get_or_insert_with<F>(&mut self, key: ImageKey, f: F) -> Option<&DecodedImage>
    where
        F: FnOnce() -> Option<DecodedImage>,
    {
        if self.entries.contains_key(&key) {
            return self.get(&key);
        }
        
        if let Some(image) = f() {
            self.insert(key.clone(), image);
            self.get(&key)
        } else {
            None
        }
    }
    
    /// Evict least recently used entry
    fn evict_lru(&mut self) {
        let lru_key = self.entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_access)
            .map(|(k, _)| k.clone());
        
        if let Some(key) = lru_key {
            if let Some(entry) = self.entries.remove(&key) {
                self.current_memory -= entry.image.memory_size();
            }
        }
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_memory = 0;
    }
    
    /// Number of cached images
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    
    /// Current memory usage
    pub fn memory_usage(&self) -> usize {
        self.current_memory
    }
    
    /// Hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

impl Default for ImageCache {
    fn default() -> Self {
        Self::default_limit()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_insert_get() {
        let mut cache = ImageCache::new(1024 * 1024);
        
        let key = ImageKey::original("test.png");
        let img = DecodedImage::from_rgba(vec![0; 400], 10, 10);
        
        cache.insert(key.clone(), img);
        
        assert!(cache.get(&key).is_some());
        assert_eq!(cache.hits, 1);
    }
    
    #[test]
    fn test_cache_eviction() {
        // 1KB cache
        let mut cache = ImageCache::new(1024);
        
        // Insert 500 bytes
        let key1 = ImageKey::original("img1.png");
        cache.insert(key1.clone(), DecodedImage::from_rgba(vec![0; 500], 10, 10));
        
        // Insert 500 bytes more
        let key2 = ImageKey::original("img2.png");
        cache.insert(key2.clone(), DecodedImage::from_rgba(vec![0; 500], 10, 10));
        
        // Insert 500 more - should evict key1
        let key3 = ImageKey::original("img3.png");
        cache.insert(key3.clone(), DecodedImage::from_rgba(vec![0; 500], 10, 10));
        
        // key1 should have been evicted
        assert!(cache.get(&key1).is_none());
        assert!(cache.get(&key2).is_some());
        assert!(cache.get(&key3).is_some());
    }
}
