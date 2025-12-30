//! CSS Style Sharing Cache
//!
//! Implements Servo-inspired style sharing to reduce memory by
//! sharing computed styles across identical elements.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::computed::ComputedStyle;

/// Cache key for style sharing
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StyleCacheKey {
    /// Hash of parent style (or 0 for root)
    parent_hash: u64,
    /// Hash of element's class list
    class_hash: u64,
    /// Hash of element's inline styles
    inline_hash: u64,
    /// Element tag name hash
    tag_hash: u64,
    /// ID attribute hash (or 0 if none)
    id_hash: u64,
}

impl StyleCacheKey {
    /// Create a new cache key
    pub fn new(
        parent_style: Option<&ComputedStyle>,
        tag: &str,
        id: Option<&str>,
        classes: &[String],
        inline_styles: &str,
    ) -> Self {
        use std::collections::hash_map::DefaultHasher;
        
        let parent_hash = parent_style.map(|s| {
            let mut h = DefaultHasher::new();
            // Hash key properties for parent matching
            h.write_u32(s.font_size as u32);
            h.write_u32(s.color.r as u32);
            h.write_u32(s.color.g as u32);
            h.write_u32(s.color.b as u32);
            h.finish()
        }).unwrap_or(0);
        
        let mut class_hasher = DefaultHasher::new();
        for class in classes {
            class.hash(&mut class_hasher);
        }
        let class_hash = class_hasher.finish();
        
        let mut inline_hasher = DefaultHasher::new();
        inline_styles.hash(&mut inline_hasher);
        let inline_hash = inline_hasher.finish();
        
        let mut tag_hasher = DefaultHasher::new();
        tag.to_lowercase().hash(&mut tag_hasher);
        let tag_hash = tag_hasher.finish();
        
        let id_hash = id.map(|i| {
            let mut h = DefaultHasher::new();
            i.hash(&mut h);
            h.finish()
        }).unwrap_or(0);
        
        Self {
            parent_hash,
            class_hash,
            inline_hash,
            tag_hash,
            id_hash,
        }
    }
}

/// Shared computed style (reference counted)
pub type SharedStyle = Arc<ComputedStyle>;

/// Style sharing cache
pub struct StyleCache {
    /// Cached styles
    cache: HashMap<StyleCacheKey, SharedStyle>,
    /// Maximum cache size
    max_size: usize,
    /// Cache hits counter
    hits: u64,
    /// Cache misses counter
    misses: u64,
}

impl Default for StyleCache {
    fn default() -> Self {
        Self::new(1024) // Default 1024 entries
    }
}

impl StyleCache {
    /// Create a new style cache
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_size),
            max_size,
            hits: 0,
            misses: 0,
        }
    }
    
    /// Look up a cached style
    pub fn get(&mut self, key: &StyleCacheKey) -> Option<SharedStyle> {
        if let Some(style) = self.cache.get(key) {
            self.hits += 1;
            Some(Arc::clone(style))
        } else {
            self.misses += 1;
            None
        }
    }
    
    /// Insert a computed style
    pub fn insert(&mut self, key: StyleCacheKey, style: ComputedStyle) -> SharedStyle {
        // Evict if at capacity (simple strategy: clear half)
        if self.cache.len() >= self.max_size {
            self.evict();
        }
        
        let shared = Arc::new(style);
        self.cache.insert(key, Arc::clone(&shared));
        shared
    }
    
    /// Get or compute a style
    pub fn get_or_insert<F>(&mut self, key: StyleCacheKey, compute: F) -> SharedStyle
    where
        F: FnOnce() -> ComputedStyle,
    {
        if let Some(cached) = self.get(&key) {
            return cached;
        }
        
        self.misses -= 1; // Undo miss from get()
        let style = compute();
        self.insert(key, style)
    }
    
    /// Evict entries (clear half when full)
    fn evict(&mut self) {
        let target = self.max_size / 2;
        let mut to_remove = Vec::with_capacity(self.cache.len() - target);
        
        // Remove entries with refcount == 1 (only in cache)
        for (key, style) in &self.cache {
            if Arc::strong_count(style) == 1 {
                to_remove.push(key.clone());
            }
            if to_remove.len() >= self.cache.len() - target {
                break;
            }
        }
        
        // If not enough, take arbitrary entries
        if to_remove.len() < self.cache.len() - target {
            for key in self.cache.keys() {
                if !to_remove.contains(key) {
                    to_remove.push(key.clone());
                }
                if to_remove.len() >= self.cache.len() - target {
                    break;
                }
            }
        }
        
        for key in to_remove {
            self.cache.remove(&key);
        }
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            size: self.cache.len(),
            max_size: self.max_size,
            hits: self.hits,
            misses: self.misses,
            hit_rate: if self.hits + self.misses > 0 {
                self.hits as f64 / (self.hits + self.misses) as f64
            } else {
                0.0
            },
        }
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub size: usize,
    pub max_size: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_key_creation() {
        let key1 = StyleCacheKey::new(
            None,
            "div",
            None,
            &["container".to_string(), "active".to_string()],
            "",
        );
        
        let key2 = StyleCacheKey::new(
            None,
            "div",
            None,
            &["container".to_string(), "active".to_string()],
            "",
        );
        
        assert_eq!(key1, key2);
    }
    
    #[test]
    fn test_cache_hit() {
        let mut cache = StyleCache::new(100);
        
        let key = StyleCacheKey::new(None, "p", None, &[], "");
        let style = ComputedStyle::default();
        
        cache.insert(key.clone(), style);
        
        let cached = cache.get(&key);
        assert!(cached.is_some());
        assert_eq!(cache.stats().hits, 1);
    }
    
    #[test]
    fn test_cache_miss() {
        let mut cache = StyleCache::new(100);
        
        let key = StyleCacheKey::new(None, "div", None, &[], "");
        
        let result = cache.get(&key);
        assert!(result.is_none());
        assert_eq!(cache.stats().misses, 1);
    }
    
    #[test]
    fn test_cache_eviction() {
        let mut cache = StyleCache::new(10);
        
        for i in 0..20 {
            let key = StyleCacheKey::new(None, &format!("tag{}", i), None, &[], "");
            cache.insert(key, ComputedStyle::default());
        }
        
        // Should have evicted some entries
        assert!(cache.cache.len() <= 10);
    }
}
