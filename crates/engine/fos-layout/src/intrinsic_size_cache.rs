//! Intrinsic Size Cache
//!
//! Caches min-content and max-content sizes for layout nodes.
//! These rarely change and are expensive to compute.

use std::collections::HashMap;

// ============================================================================
// Intrinsic Sizes
// ============================================================================

/// Cached intrinsic sizes for a node
#[derive(Debug, Clone, Copy, Default)]
pub struct IntrinsicSizes {
    /// Minimum content width (shrink-to-fit minimum)
    pub min_content_width: f32,
    /// Maximum content width (no line breaking)
    pub max_content_width: f32,
    /// Minimum content height
    pub min_content_height: f32,
    /// Maximum content height  
    pub max_content_height: f32,
}

impl IntrinsicSizes {
    pub fn new(min_width: f32, max_width: f32, min_height: f32, max_height: f32) -> Self {
        Self {
            min_content_width: min_width,
            max_content_width: max_width,
            min_content_height: min_height,
            max_content_height: max_height,
        }
    }
    
    /// Create from just widths (common case)
    pub fn from_widths(min_width: f32, max_width: f32) -> Self {
        Self {
            min_content_width: min_width,
            max_content_width: max_width,
            min_content_height: 0.0,
            max_content_height: 0.0,
        }
    }
    
    /// Check if sizes are valid
    pub fn is_valid(&self) -> bool {
        self.min_content_width <= self.max_content_width &&
        self.min_content_height <= self.max_content_height
    }
}

// ============================================================================
// Cache Entry
// ============================================================================

/// Cache entry with validity tracking
#[derive(Debug, Clone)]
struct CacheEntry {
    sizes: IntrinsicSizes,
    /// DOM generation when computed
    generation: u64,
    /// Access count for statistics
    access_count: u32,
}

// ============================================================================
// Cache Statistics
// ============================================================================

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct IntrinsicCacheStats {
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Invalidations
    pub invalidations: u64,
    /// Current entries
    pub entries: usize,
}

impl IntrinsicCacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

// ============================================================================
// Intrinsic Size Cache
// ============================================================================

/// Cache for intrinsic sizes with DOM generation invalidation
pub struct IntrinsicSizeCache {
    /// Cached sizes by node ID
    sizes: HashMap<u32, CacheEntry>,
    /// Current DOM generation (incremented on mutations)
    generation: u64,
    /// Maximum entries
    max_entries: usize,
    /// Statistics
    stats: IntrinsicCacheStats,
}

impl Default for IntrinsicSizeCache {
    fn default() -> Self {
        Self::new(5000)
    }
}

impl IntrinsicSizeCache {
    /// Create a new cache
    pub fn new(max_entries: usize) -> Self {
        Self {
            sizes: HashMap::with_capacity(max_entries.min(2048)),
            generation: 0,
            max_entries,
            stats: IntrinsicCacheStats::default(),
        }
    }
    
    /// Get cached intrinsic sizes for a node
    pub fn get(&mut self, node_id: u32) -> Option<IntrinsicSizes> {
        if let Some(entry) = self.sizes.get_mut(&node_id) {
            // Check if still valid
            if entry.generation == self.generation {
                entry.access_count += 1;
                self.stats.hits += 1;
                return Some(entry.sizes);
            }
            // Stale entry
            self.sizes.remove(&node_id);
        }
        self.stats.misses += 1;
        None
    }
    
    /// Get or compute intrinsic sizes
    pub fn get_or_compute<F>(&mut self, node_id: u32, compute: F) -> IntrinsicSizes
    where
        F: FnOnce() -> IntrinsicSizes,
    {
        if let Some(sizes) = self.get(node_id) {
            return sizes;
        }
        
        let sizes = compute();
        self.insert(node_id, sizes);
        sizes
    }
    
    /// Insert sizes into cache
    pub fn insert(&mut self, node_id: u32, sizes: IntrinsicSizes) {
        // Evict if full
        if self.sizes.len() >= self.max_entries {
            self.evict_lru();
        }
        
        self.sizes.insert(node_id, CacheEntry {
            sizes,
            generation: self.generation,
            access_count: 0,
        });
    }
    
    /// Invalidate a specific node (and its subtree if needed)
    pub fn invalidate_node(&mut self, node_id: u32) {
        self.sizes.remove(&node_id);
        self.stats.invalidations += 1;
    }
    
    /// Invalidate all cache (DOM structure changed)
    pub fn invalidate_all(&mut self) {
        self.generation += 1;
        self.stats.invalidations += 1;
        // Don't clear - entries will be invalidated on access by generation check
    }
    
    /// Clear cache and increment generation
    pub fn clear(&mut self) {
        self.sizes.clear();
        self.generation += 1;
    }
    
    /// Evict least accessed entry
    fn evict_lru(&mut self) {
        // Find entry with lowest access count
        if let Some(oldest) = self.sizes.iter()
            .min_by_key(|(_, e)| e.access_count)
            .map(|(k, _)| *k)
        {
            self.sizes.remove(&oldest);
        }
    }
    
    /// Current generation
    pub fn generation(&self) -> u64 {
        self.generation
    }
    
    /// Get statistics
    pub fn stats(&self) -> IntrinsicCacheStats {
        IntrinsicCacheStats {
            entries: self.sizes.len(),
            ..self.stats
        }
    }
    
    /// Number of entries
    pub fn len(&self) -> usize {
        self.sizes.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.sizes.is_empty()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_intrinsic_sizes_creation() {
        let sizes = IntrinsicSizes::new(100.0, 500.0, 50.0, 200.0);
        assert!(sizes.is_valid());
        assert_eq!(sizes.min_content_width, 100.0);
    }
    
    #[test]
    fn test_cache_basic() {
        let mut cache = IntrinsicSizeCache::new(100);
        
        let sizes = IntrinsicSizes::from_widths(50.0, 200.0);
        cache.insert(1, sizes);
        
        let cached = cache.get(1);
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().min_content_width, 50.0);
    }
    
    #[test]
    fn test_cache_invalidation() {
        let mut cache = IntrinsicSizeCache::new(100);
        
        cache.insert(1, IntrinsicSizes::from_widths(50.0, 200.0));
        
        // Invalidate
        cache.invalidate_all();
        
        // Should miss now (stale generation)
        assert!(cache.get(1).is_none());
    }
    
    #[test]
    fn test_cache_get_or_compute() {
        let mut cache = IntrinsicSizeCache::new(100);
        
        let mut compute_count = 0;
        
        let sizes1 = cache.get_or_compute(1, || {
            compute_count += 1;
            IntrinsicSizes::from_widths(100.0, 300.0)
        });
        
        let sizes2 = cache.get_or_compute(1, || {
            compute_count += 1;
            IntrinsicSizes::from_widths(100.0, 300.0)
        });
        
        // Should only compute once
        assert_eq!(compute_count, 1);
        assert_eq!(sizes1.min_content_width, sizes2.min_content_width);
    }
    
    #[test]
    fn test_cache_stats() {
        let mut cache = IntrinsicSizeCache::new(100);
        
        cache.insert(1, IntrinsicSizes::default());
        cache.get(1); // Hit
        cache.get(2); // Miss
        
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }
}
