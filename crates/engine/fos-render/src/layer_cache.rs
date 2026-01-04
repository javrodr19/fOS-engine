//! Layer Tree Caching
//!
//! Cache rendered layer trees across frames.

use std::collections::HashMap;
use std::sync::Arc;

/// Layer cache key
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct LayerCacheKey {
    pub content_hash: u64,
    pub transform_hash: u64,
    pub width: u32,
    pub height: u32,
}

impl LayerCacheKey {
    pub fn new(content_hash: u64, transform_hash: u64, width: u32, height: u32) -> Self {
        Self { content_hash, transform_hash, width, height }
    }
}

/// Cached layer content
#[derive(Debug)]
pub struct CachedLayer {
    pixels: Arc<Vec<u8>>,
    width: u32,
    height: u32,
    last_access: u64,
    hits: u32,
}

impl CachedLayer {
    pub fn new(pixels: Vec<u8>, width: u32, height: u32, frame: u64) -> Self {
        Self { pixels: Arc::new(pixels), width, height, last_access: frame, hits: 0 }
    }
    
    pub fn pixels(&self) -> &[u8] { &self.pixels }
    pub fn pixels_arc(&self) -> Arc<Vec<u8>> { self.pixels.clone() }
    pub fn dimensions(&self) -> (u32, u32) { (self.width, self.height) }
    pub fn memory_size(&self) -> usize { self.pixels.len() }
}

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub lookups: u64,
    pub evictions: u64,
    pub bytes_saved: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        if self.lookups == 0 { 0.0 } else { self.hits as f64 / self.lookups as f64 }
    }
}

/// Layer tree cache
#[derive(Debug)]
pub struct LayerTreeCache {
    cache: HashMap<LayerCacheKey, CachedLayer>,
    current_frame: u64,
    max_size: usize,
    current_size: usize,
    stats: CacheStats,
}

impl Default for LayerTreeCache {
    fn default() -> Self { Self::new() }
}

impl LayerTreeCache {
    pub fn new() -> Self { Self::with_capacity(64 * 1024 * 1024) }
    
    pub fn with_capacity(max_size: usize) -> Self {
        Self { cache: HashMap::new(), current_frame: 0, max_size, current_size: 0, stats: CacheStats::default() }
    }
    
    pub fn advance_frame(&mut self) { self.current_frame += 1; }
    
    pub fn get(&mut self, key: &LayerCacheKey) -> Option<&CachedLayer> {
        self.stats.lookups += 1;
        if let Some(layer) = self.cache.get_mut(key) {
            layer.last_access = self.current_frame;
            layer.hits += 1;
            self.stats.hits += 1;
            self.stats.bytes_saved += layer.memory_size() as u64;
            Some(layer)
        } else {
            self.stats.misses += 1;
            None
        }
    }
    
    pub fn contains(&self, key: &LayerCacheKey) -> bool { self.cache.contains_key(key) }
    
    pub fn insert(&mut self, key: LayerCacheKey, pixels: Vec<u8>, width: u32, height: u32) {
        let size = pixels.len();
        while self.current_size + size > self.max_size && !self.cache.is_empty() {
            self.evict_one();
        }
        if size > self.max_size { return; }
        
        if let Some(old) = self.cache.remove(&key) { self.current_size -= old.memory_size(); }
        self.current_size += size;
        self.cache.insert(key, CachedLayer::new(pixels, width, height, self.current_frame));
    }
    
    pub fn remove(&mut self, key: &LayerCacheKey) -> Option<CachedLayer> {
        self.cache.remove(key).map(|l| { self.current_size -= l.memory_size(); l })
    }
    
    pub fn clear(&mut self) { self.cache.clear(); self.current_size = 0; }
    pub fn stats(&self) -> &CacheStats { &self.stats }
    pub fn len(&self) -> usize { self.cache.len() }
    pub fn is_empty(&self) -> bool { self.cache.is_empty() }
    
    fn evict_one(&mut self) {
        if let Some(key) = self.cache.iter().min_by_key(|(_, l)| l.last_access).map(|(k, _)| k.clone()) {
            if let Some(l) = self.cache.remove(&key) { self.current_size -= l.memory_size(); self.stats.evictions += 1; }
        }
    }
}

/// Layer tree
#[derive(Debug, Default)]
pub struct LayerTree {
    pub root: Option<u32>,
    layers: HashMap<u32, Layer>,
    next_id: u32,
}

/// Layer
#[derive(Debug, Clone)]
pub struct Layer {
    pub id: u32,
    pub parent: Option<u32>,
    pub children: Vec<u32>,
    pub content_hash: u64,
    pub opacity: f32,
    pub needs_repaint: bool,
}

impl LayerTree {
    pub fn new() -> Self { Self { root: None, layers: HashMap::new(), next_id: 1 } }
    
    pub fn create_layer(&mut self, parent: Option<u32>) -> u32 {
        let id = self.next_id; self.next_id += 1;
        let layer = Layer { id, parent, children: Vec::new(), content_hash: 0, opacity: 1.0, needs_repaint: true };
        self.layers.insert(id, layer);
        if let Some(p) = parent { if let Some(pl) = self.layers.get_mut(&p) { pl.children.push(id); } }
        if self.root.is_none() { self.root = Some(id); }
        id
    }
    
    pub fn get(&self, id: u32) -> Option<&Layer> { self.layers.get(&id) }
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Layer> { self.layers.get_mut(&id) }
    pub fn mark_dirty(&mut self, id: u32) { if let Some(l) = self.layers.get_mut(&id) { l.needs_repaint = true; } }
    pub fn len(&self) -> usize { self.layers.len() }
    pub fn is_empty(&self) -> bool { self.layers.is_empty() }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_basic() {
        let mut cache = LayerTreeCache::new();
        let key = LayerCacheKey::new(1, 0, 100, 100);
        cache.insert(key.clone(), vec![0u8; 40000], 100, 100);
        assert!(cache.contains(&key));
    }
}
