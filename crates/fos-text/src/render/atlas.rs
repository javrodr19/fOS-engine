//! Glyph atlas (cache)

use std::collections::HashMap;
use super::RasterizedGlyph;

/// Key for glyph cache lookup
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    /// Font ID (for multi-font rendering)
    pub font_id: u32,
    /// Glyph ID in font
    pub glyph_id: u16,
    /// Font size in pixels (quantized to avoid cache explosion)
    pub size_px: u16,
}

impl GlyphKey {
    /// Create a new glyph key
    pub fn new(font_id: u32, glyph_id: u16, font_size: f32) -> Self {
        Self {
            font_id,
            glyph_id,
            // Quantize to nearest pixel to reduce cache entries
            size_px: font_size.round() as u16,
        }
    }
}

/// Glyph atlas for caching rasterized glyphs
pub struct GlyphAtlas {
    /// Cached glyphs
    cache: HashMap<GlyphKey, RasterizedGlyph>,
    /// Maximum cache size
    max_entries: usize,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
}

impl GlyphAtlas {
    /// Create a new glyph atlas
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_entries.min(1024)),
            max_entries,
            hits: 0,
            misses: 0,
        }
    }
    
    /// Get a cached glyph
    pub fn get(&mut self, key: &GlyphKey) -> Option<&RasterizedGlyph> {
        if self.cache.contains_key(key) {
            self.hits += 1;
            self.cache.get(key)
        } else {
            self.misses += 1;
            None
        }
    }
    
    /// Insert a rasterized glyph into the cache
    pub fn insert(&mut self, key: GlyphKey, glyph: RasterizedGlyph) {
        // Simple eviction: clear half the cache when full
        if self.cache.len() >= self.max_entries {
            self.evict_half();
        }
        self.cache.insert(key, glyph);
    }
    
    /// Get or insert a glyph
    pub fn get_or_insert_with<F>(&mut self, key: GlyphKey, f: F) -> &RasterizedGlyph
    where
        F: FnOnce() -> RasterizedGlyph,
    {
        if !self.cache.contains_key(&key) {
            self.misses += 1;
            let glyph = f();
            if self.cache.len() >= self.max_entries {
                self.evict_half();
            }
            self.cache.insert(key, glyph);
        } else {
            self.hits += 1;
        }
        self.cache.get(&key).unwrap()
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }
    
    /// Number of cached glyphs
    pub fn len(&self) -> usize {
        self.cache.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
    
    /// Cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
    
    /// Evict half the entries (simple LRU-like behavior)
    fn evict_half(&mut self) {
        let to_remove = self.cache.len() / 2;
        let keys: Vec<_> = self.cache.keys().take(to_remove).cloned().collect();
        for key in keys {
            self.cache.remove(&key);
        }
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new(4096)  // Cache up to 4K glyphs by default
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_glyph_key() {
        let k1 = GlyphKey::new(0, 65, 16.0);
        let k2 = GlyphKey::new(0, 65, 16.4);  // Same after quantization
        assert_eq!(k1, k2);
    }
    
    #[test]
    fn test_atlas_cache() {
        let mut atlas = GlyphAtlas::new(100);
        let key = GlyphKey::new(0, 65, 16.0);
        
        assert!(atlas.get(&key).is_none());
        
        atlas.insert(key, RasterizedGlyph::empty(65));
        assert!(atlas.get(&key).is_some());
        assert_eq!(atlas.hits, 1);
    }
}
