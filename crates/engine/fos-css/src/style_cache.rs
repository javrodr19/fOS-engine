//! CSS Style Sharing Cache
//!
//! Implements Servo-inspired style sharing to reduce memory by
//! sharing computed styles across identical elements.
//!
//! ## Optimizations (CSS Roadmap Phase 2)
//! - Content hash deduplication: identical styles share memory
//! - Structural sharing: subtree + content hash lookups
//! - LRU eviction with refcount-aware strategy

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

/// Style sharing cache with content hash deduplication
pub struct StyleCache {
    /// Cached styles by key
    cache: HashMap<StyleCacheKey, SharedStyle>,
    /// Content hash based deduplication pool
    content_pool: HashMap<u64, SharedStyle>,
    /// Maximum cache size
    max_size: usize,
    /// Cache hits counter
    hits: u64,
    /// Cache misses counter
    misses: u64,
    /// Content dedup hits
    dedup_hits: u64,
    /// LRU tracking
    access_order: Vec<StyleCacheKey>,
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
            content_pool: HashMap::with_capacity(max_size / 4),
            max_size,
            hits: 0,
            misses: 0,
            dedup_hits: 0,
            access_order: Vec::with_capacity(max_size),
        }
    }
    
    /// Look up a cached style
    pub fn get(&mut self, key: &StyleCacheKey) -> Option<SharedStyle> {
        let style = self.cache.get(key).cloned();
        if style.is_some() {
            self.hits += 1;
            self.touch_key(key.clone());
        } else {
            self.misses += 1;
        }
        style
    }
    
    /// Insert a computed style with content deduplication
    pub fn insert(&mut self, key: StyleCacheKey, style: ComputedStyle) -> SharedStyle {
        // Check content pool for identical style
        let content_hash = hash_style_content(&style);
        
        // Clone the existing style if found (to release borrow)
        let existing = self.content_pool.get(&content_hash).cloned();
        
        if let Some(existing_style) = existing {
            // Reuse existing identical style
            self.dedup_hits += 1;
            self.cache.insert(key.clone(), Arc::clone(&existing_style));
            self.touch_key(key);
            return existing_style;
        }
        
        // Evict if at capacity
        if self.cache.len() >= self.max_size {
            self.evict();
        }
        
        let shared = Arc::new(style);
        self.content_pool.insert(content_hash, Arc::clone(&shared));
        self.cache.insert(key.clone(), Arc::clone(&shared));
        self.touch_key(key);
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
    
    /// Try to find an identical style by content hash
    pub fn find_by_content(&self, style: &ComputedStyle) -> Option<SharedStyle> {
        let hash = hash_style_content(style);
        self.content_pool.get(&hash).map(Arc::clone)
    }
    
    /// Touch a key for LRU tracking
    fn touch_key(&mut self, key: StyleCacheKey) {
        if let Some(pos) = self.access_order.iter().position(|k| k == &key) {
            self.access_order.remove(pos);
        }
        self.access_order.push(key);
        
        // Keep access order bounded
        if self.access_order.len() > self.max_size * 2 {
            self.access_order.drain(0..self.max_size);
        }
    }
    
    /// Evict entries using LRU with refcount awareness
    fn evict(&mut self) {
        let target = self.max_size / 2;
        let mut to_remove = Vec::with_capacity(self.cache.len() - target);
        
        // First priority: remove entries with refcount == 1 (only in cache)
        for (key, style) in &self.cache {
            if Arc::strong_count(style) == 1 {
                to_remove.push(key.clone());
            }
            if to_remove.len() >= self.cache.len() - target {
                break;
            }
        }
        
        // Second priority: use LRU order
        if to_remove.len() < self.cache.len() - target {
            for key in &self.access_order {
                if !to_remove.contains(key) && self.cache.contains_key(key) {
                    to_remove.push(key.clone());
                }
                if to_remove.len() >= self.cache.len() - target {
                    break;
                }
            }
        }
        
        for key in &to_remove {
            self.cache.remove(key);
            self.access_order.retain(|k| k != key);
        }
        
        // Clean content pool of unused styles
        self.content_pool.retain(|_, style| Arc::strong_count(style) > 1);
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
            content_pool_size: self.content_pool.len(),
            dedup_hits: self.dedup_hits,
            memory_saved: self.dedup_hits * std::mem::size_of::<ComputedStyle>() as u64,
        }
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.content_pool.clear();
        self.access_order.clear();
        self.hits = 0;
        self.misses = 0;
        self.dedup_hits = 0;
    }
}

/// Hash the content of a computed style for deduplication
fn hash_style_content(style: &ComputedStyle) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;
    
    let mut hasher = DefaultHasher::new();
    
    // Hash key properties
    hasher.write_u32(style.font_size.to_bits());
    hasher.write_u32(style.line_height.to_bits());
    hasher.write_u8(style.color.r);
    hasher.write_u8(style.color.g);
    hasher.write_u8(style.color.b);
    hasher.write_u8(style.color.a);
    hasher.write_u8(style.background_color.r);
    hasher.write_u8(style.background_color.g);
    hasher.write_u8(style.background_color.b);
    hasher.write_u8(style.background_color.a);
    
    // Hash display and position
    hasher.write_u8(style.display as u8);
    hasher.write_u8(style.position as u8);
    
    // Hash dimensions using simple byte representation
    hash_size_value(&style.width, &mut hasher);
    hash_size_value(&style.height, &mut hasher);
    
    // Hash margin edges
    hash_size_value(&style.margin.top, &mut hasher);
    hash_size_value(&style.margin.right, &mut hasher);
    hash_size_value(&style.margin.bottom, &mut hasher);
    hash_size_value(&style.margin.left, &mut hasher);
    
    // Hash padding edges
    hash_size_value(&style.padding.top, &mut hasher);
    hash_size_value(&style.padding.right, &mut hasher);
    hash_size_value(&style.padding.bottom, &mut hasher);
    hash_size_value(&style.padding.left, &mut hasher);
    
    hasher.finish()
}

/// Hash a SizeValue for content deduplication
fn hash_size_value(size: &crate::computed::SizeValue, hasher: &mut impl std::hash::Hasher) {
    use crate::computed::SizeValue;
    match size {
        SizeValue::Auto => hasher.write_u8(0),
        SizeValue::Length(val, unit) => {
            hasher.write_u8(1);
            hasher.write_u32(val.to_bits());
            hasher.write_u8(*unit as u8);
        }
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
    /// Number of unique styles in content pool
    pub content_pool_size: usize,
    /// Times content deduplication prevented allocation
    pub dedup_hits: u64,
    /// Estimated memory saved by deduplication (bytes)
    pub memory_saved: u64,
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
        
        // Insert different styles (unique content) to trigger eviction
        for i in 0..20 {
            let key = StyleCacheKey::new(None, &format!("tag{}", i), None, &[], "");
            let mut style = ComputedStyle::default();
            style.font_size = i as f32; // Make each style unique
            cache.insert(key, style);
        }
        
        // Should have evicted some entries
        assert!(cache.cache.len() <= 10);
    }
    
    #[test]
    fn test_content_deduplication() {
        let mut cache = StyleCache::new(100);
        
        // Insert same style with different keys
        let key1 = StyleCacheKey::new(None, "div", None, &[], "");
        let key2 = StyleCacheKey::new(None, "span", None, &[], "");
        
        let style1 = ComputedStyle::default();
        let style2 = ComputedStyle::default(); // Identical content
        
        let shared1 = cache.insert(key1, style1);
        let shared2 = cache.insert(key2, style2);
        
        // Both should point to same content
        assert!(Arc::ptr_eq(&shared1, &shared2));
        assert_eq!(cache.stats().dedup_hits, 1);
    }
    
    #[test]
    fn test_find_by_content() {
        let mut cache = StyleCache::new(100);
        
        let key = StyleCacheKey::new(None, "div", None, &[], "");
        let style = ComputedStyle::default();
        
        let inserted = cache.insert(key, style.clone());
        
        // Should find by content
        let found = cache.find_by_content(&style);
        assert!(found.is_some());
        assert!(Arc::ptr_eq(&inserted, &found.unwrap()));
    }
}

