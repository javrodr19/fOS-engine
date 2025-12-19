//! Text Run Cache
//!
//! Caches shaped text runs to avoid reshaping identical text.

use std::collections::HashMap;

/// Text run cache key
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TextRunKey {
    /// Font identifier
    pub font_id: u16,
    /// Font size (scaled to avoid float hashing)
    pub font_size_scaled: u32,
    /// Text content hash
    pub text_hash: u64,
}

impl TextRunKey {
    /// Create a new text run key
    pub fn new(font_id: u16, font_size: f32, text: &str) -> Self {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        
        Self {
            font_id,
            font_size_scaled: (font_size * 100.0) as u32,
            text_hash: hasher.finish(),
        }
    }
}

/// Cached shaped glyph data
#[derive(Clone)]
pub struct CachedTextRun {
    /// Glyph IDs
    pub glyphs: Vec<u32>,
    /// X positions for each glyph
    pub x_positions: Vec<f32>,
    /// Total width of the run
    pub total_width: f32,
    /// Ascent
    pub ascent: f32,
    /// Descent
    pub descent: f32,
}

/// Text run cache with LRU eviction
pub struct TextRunCache {
    /// Cached runs
    cache: HashMap<TextRunKey, CachedTextRun>,
    /// Maximum entries
    max_entries: usize,
    /// Usage order for LRU (key -> last_used)
    usage: HashMap<TextRunKey, u64>,
    /// Counter for usage tracking
    counter: u64,
    /// Stats
    hits: u64,
    misses: u64,
}

impl Default for TextRunCache {
    fn default() -> Self {
        Self::new(512)
    }
}

impl TextRunCache {
    /// Create a new text run cache
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_entries),
            max_entries,
            usage: HashMap::with_capacity(max_entries),
            counter: 0,
            hits: 0,
            misses: 0,
        }
    }
    
    /// Get a cached text run
    pub fn get(&mut self, key: &TextRunKey) -> Option<&CachedTextRun> {
        if self.cache.contains_key(key) {
            self.counter += 1;
            self.usage.insert(key.clone(), self.counter);
            self.hits += 1;
            self.cache.get(key)
        } else {
            self.misses += 1;
            None
        }
    }
    
    /// Insert a shaped text run
    pub fn insert(&mut self, key: TextRunKey, run: CachedTextRun) {
        if self.cache.len() >= self.max_entries {
            self.evict_lru();
        }
        
        self.counter += 1;
        self.usage.insert(key.clone(), self.counter);
        self.cache.insert(key, run);
    }
    
    /// Evict least recently used entry
    fn evict_lru(&mut self) {
        if let Some((oldest_key, _)) = self.usage.iter()
            .min_by_key(|&(_, usage)| usage)
            .map(|(k, v)| (k.clone(), *v))
        {
            self.cache.remove(&oldest_key);
            self.usage.remove(&oldest_key);
        }
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> TextRunCacheStats {
        TextRunCacheStats {
            size: self.cache.len(),
            max_size: self.max_entries,
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
        self.usage.clear();
        self.hits = 0;
        self.misses = 0;
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct TextRunCacheStats {
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
    fn test_cache_hit() {
        let mut cache = TextRunCache::new(100);
        let key = TextRunKey::new(0, 16.0, "Hello");
        
        cache.insert(key.clone(), CachedTextRun {
            glyphs: vec![1, 2, 3, 4, 5],
            x_positions: vec![0.0, 10.0, 20.0, 30.0, 40.0],
            total_width: 50.0,
            ascent: 12.0,
            descent: 4.0,
        });
        
        assert!(cache.get(&key).is_some());
        assert_eq!(cache.stats().hits, 1);
    }
    
    #[test]
    fn test_cache_miss() {
        let mut cache = TextRunCache::new(100);
        let key = TextRunKey::new(0, 16.0, "Hello");
        
        assert!(cache.get(&key).is_none());
        assert_eq!(cache.stats().misses, 1);
    }
    
    #[test]
    fn test_lru_eviction() {
        let mut cache = TextRunCache::new(3);
        
        for i in 0..5 {
            let key = TextRunKey::new(0, 16.0, &format!("text{}", i));
            cache.insert(key, CachedTextRun {
                glyphs: vec![i],
                x_positions: vec![0.0],
                total_width: 10.0,
                ascent: 12.0,
                descent: 4.0,
            });
        }
        
        // Should have evicted oldest entries
        assert!(cache.cache.len() <= 3);
    }
}
