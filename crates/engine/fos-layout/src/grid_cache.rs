//! Grid Layout Cache (Phase 1.2)
//!
//! Cache track sizing and placement for similar grids.
//! Avoid recomputing expensive track sizing algorithm.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

// ============================================================================
// Track Sizing Cache
// ============================================================================

/// Key for track sizing cache
#[derive(Debug, Clone)]
pub struct TrackSizingKey {
    /// Hash of column template
    column_template_hash: u64,
    /// Hash of row template
    row_template_hash: u64,
    /// Container width (quantized to reduce cache misses)
    container_width: u32,
    /// Container height (quantized)
    container_height: u32,
    /// Column gap
    column_gap: u32,
    /// Row gap
    row_gap: u32,
}

impl TrackSizingKey {
    /// Create a new track sizing key
    /// 
    /// Sizes are quantized to nearest 8px to improve cache hit rate
    pub fn new(
        column_template_hash: u64,
        row_template_hash: u64,
        container_width: f32,
        container_height: f32,
        column_gap: f32,
        row_gap: f32,
    ) -> Self {
        Self {
            column_template_hash,
            row_template_hash,
            // Quantize to nearest 8px
            container_width: (container_width / 8.0).round() as u32,
            container_height: (container_height / 8.0).round() as u32,
            column_gap: (column_gap * 10.0) as u32,
            row_gap: (row_gap * 10.0) as u32,
        }
    }
}

impl PartialEq for TrackSizingKey {
    fn eq(&self, other: &Self) -> bool {
        self.column_template_hash == other.column_template_hash
            && self.row_template_hash == other.row_template_hash
            && self.container_width == other.container_width
            && self.container_height == other.container_height
            && self.column_gap == other.column_gap
            && self.row_gap == other.row_gap
    }
}

impl Eq for TrackSizingKey {}

impl Hash for TrackSizingKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.column_template_hash.hash(state);
        self.row_template_hash.hash(state);
        self.container_width.hash(state);
        self.container_height.hash(state);
        self.column_gap.hash(state);
        self.row_gap.hash(state);
    }
}

/// Cached track sizes
#[derive(Debug, Clone)]
pub struct CachedTrackSizes {
    /// Column sizes
    pub column_sizes: Vec<f32>,
    /// Row sizes
    pub row_sizes: Vec<f32>,
    /// Column positions
    pub column_positions: Vec<f32>,
    /// Row positions
    pub row_positions: Vec<f32>,
}

// ============================================================================
// Placement Cache
// ============================================================================

/// Key for placement cache
#[derive(Debug, Clone)]
pub struct PlacementKey {
    /// Number of columns
    num_columns: usize,
    /// Number of rows
    num_rows: usize,
    /// Hash of all item placements
    placement_hash: u64,
}

impl PlacementKey {
    /// Create a new placement key
    pub fn new(num_columns: usize, num_rows: usize, placement_hash: u64) -> Self {
        Self {
            num_columns,
            num_rows,
            placement_hash,
        }
    }
}

impl PartialEq for PlacementKey {
    fn eq(&self, other: &Self) -> bool {
        self.num_columns == other.num_columns
            && self.num_rows == other.num_rows
            && self.placement_hash == other.placement_hash
    }
}

impl Eq for PlacementKey {}

impl Hash for PlacementKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.num_columns.hash(state);
        self.num_rows.hash(state);
        self.placement_hash.hash(state);
    }
}

/// Cached grid area (resolved placement)
#[derive(Debug, Clone, Copy)]
pub struct CachedGridArea {
    pub column_start: usize,
    pub column_end: usize,
    pub row_start: usize,
    pub row_end: usize,
}

/// Cached placements for all items
#[derive(Debug, Clone)]
pub struct CachedPlacements {
    /// Resolved areas for each item
    pub areas: Vec<CachedGridArea>,
}

// ============================================================================
// Grid Layout Cache
// ============================================================================

/// Statistics for grid cache
#[derive(Debug, Clone, Copy, Default)]
pub struct GridCacheStats {
    /// Track cache hits
    pub track_hits: usize,
    /// Track cache misses
    pub track_misses: usize,
    /// Placement cache hits
    pub placement_hits: usize,
    /// Placement cache misses
    pub placement_misses: usize,
}

impl GridCacheStats {
    /// Track hit rate
    pub fn track_hit_rate(&self) -> f64 {
        let total = self.track_hits + self.track_misses;
        if total == 0 {
            0.0
        } else {
            self.track_hits as f64 / total as f64
        }
    }
    
    /// Placement hit rate
    pub fn placement_hit_rate(&self) -> f64 {
        let total = self.placement_hits + self.placement_misses;
        if total == 0 {
            0.0
        } else {
            self.placement_hits as f64 / total as f64
        }
    }
    
    /// Overall hit rate
    pub fn overall_hit_rate(&self) -> f64 {
        let total = self.track_hits + self.track_misses + self.placement_hits + self.placement_misses;
        let hits = self.track_hits + self.placement_hits;
        if total == 0 {
            0.0
        } else {
            hits as f64 / total as f64
        }
    }
}

/// Grid layout cache for reusing track sizing and placement results
#[derive(Debug)]
pub struct GridLayoutCache {
    /// Cache track sizing for similar grids
    track_cache: HashMap<TrackSizingKey, CachedTrackSizes>,
    /// Reuse placement for static grids
    placement_cache: HashMap<PlacementKey, CachedPlacements>,
    /// Maximum entries per cache
    max_entries: usize,
    /// Cache statistics
    stats: GridCacheStats,
}

impl Default for GridLayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

impl GridLayoutCache {
    /// Create a new grid layout cache
    pub fn new() -> Self {
        Self {
            track_cache: HashMap::new(),
            placement_cache: HashMap::new(),
            max_entries: 256,
            stats: GridCacheStats::default(),
        }
    }
    
    /// Set maximum entries per cache
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }
    
    /// Get cached track sizes
    pub fn get_track_sizes(&mut self, key: &TrackSizingKey) -> Option<&CachedTrackSizes> {
        if self.track_cache.contains_key(key) {
            self.stats.track_hits += 1;
            self.track_cache.get(key)
        } else {
            self.stats.track_misses += 1;
            None
        }
    }
    
    /// Insert track sizes into cache
    pub fn insert_track_sizes(&mut self, key: TrackSizingKey, sizes: CachedTrackSizes) {
        // Evict if at capacity
        if self.track_cache.len() >= self.max_entries {
            self.evict_track_lru();
        }
        self.track_cache.insert(key, sizes);
    }
    
    /// Get cached placements
    pub fn get_placements(&mut self, key: &PlacementKey) -> Option<&CachedPlacements> {
        if self.placement_cache.contains_key(key) {
            self.stats.placement_hits += 1;
            self.placement_cache.get(key)
        } else {
            self.stats.placement_misses += 1;
            None
        }
    }
    
    /// Insert placements into cache
    pub fn insert_placements(&mut self, key: PlacementKey, placements: CachedPlacements) {
        // Evict if at capacity
        if self.placement_cache.len() >= self.max_entries {
            self.evict_placement_lru();
        }
        self.placement_cache.insert(key, placements);
    }
    
    /// Evict one entry from track cache (simple strategy: remove arbitrary)
    fn evict_track_lru(&mut self) {
        if let Some(key) = self.track_cache.keys().next().cloned() {
            self.track_cache.remove(&key);
        }
    }
    
    /// Evict one entry from placement cache
    fn evict_placement_lru(&mut self) {
        if let Some(key) = self.placement_cache.keys().next().cloned() {
            self.placement_cache.remove(&key);
        }
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> &GridCacheStats {
        &self.stats
    }
    
    /// Clear all caches
    pub fn clear(&mut self) {
        self.track_cache.clear();
        self.placement_cache.clear();
        self.stats = GridCacheStats::default();
    }
    
    /// Number of entries in track cache
    pub fn track_cache_len(&self) -> usize {
        self.track_cache.len()
    }
    
    /// Number of entries in placement cache
    pub fn placement_cache_len(&self) -> usize {
        self.placement_cache.len()
    }
}

// ============================================================================
// Hashing Utilities
// ============================================================================

/// Hash a slice of track sizes for cache key generation
pub fn hash_track_template<H: Hasher>(tracks: &[f32], state: &mut H) {
    for track in tracks {
        (*track as u32).hash(state);
    }
}

/// Compute hash for a track template (for cache keying)
pub fn compute_template_hash(tracks: &[f32]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    hash_track_template(tracks, &mut hasher);
    hasher.finish()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_track_sizing_key_equality() {
        let key1 = TrackSizingKey::new(123, 456, 800.0, 600.0, 10.0, 10.0);
        let key2 = TrackSizingKey::new(123, 456, 800.0, 600.0, 10.0, 10.0);
        assert_eq!(key1, key2);
    }
    
    #[test]
    fn test_track_sizing_key_quantization() {
        // Keys with slightly different sizes should still match (within 8px)
        let key1 = TrackSizingKey::new(123, 456, 800.0, 600.0, 10.0, 10.0);
        let key2 = TrackSizingKey::new(123, 456, 804.0, 603.0, 10.0, 10.0);
        assert_eq!(key1, key2);
    }
    
    #[test]
    fn test_grid_cache_basic() {
        let mut cache = GridLayoutCache::new();
        
        let key = TrackSizingKey::new(123, 456, 800.0, 600.0, 10.0, 10.0);
        
        // Miss first
        assert!(cache.get_track_sizes(&key).is_none());
        
        // Insert
        cache.insert_track_sizes(key.clone(), CachedTrackSizes {
            column_sizes: vec![100.0, 200.0, 100.0],
            row_sizes: vec![50.0, 50.0],
            column_positions: vec![0.0, 100.0, 300.0, 400.0],
            row_positions: vec![0.0, 50.0, 100.0],
        });
        
        // Hit
        assert!(cache.get_track_sizes(&key).is_some());
        
        // Check stats
        assert_eq!(cache.stats().track_misses, 1);
        assert_eq!(cache.stats().track_hits, 1);
    }
    
    #[test]
    fn test_placement_cache() {
        let mut cache = GridLayoutCache::new();
        
        let key = PlacementKey::new(3, 2, 12345);
        
        // Miss
        assert!(cache.get_placements(&key).is_none());
        
        // Insert
        cache.insert_placements(key.clone(), CachedPlacements {
            areas: vec![
                CachedGridArea { column_start: 0, column_end: 1, row_start: 0, row_end: 1 },
                CachedGridArea { column_start: 1, column_end: 2, row_start: 0, row_end: 1 },
            ],
        });
        
        // Hit
        let placements = cache.get_placements(&key).unwrap();
        assert_eq!(placements.areas.len(), 2);
    }
    
    #[test]
    fn test_template_hash() {
        let template1 = vec![100.0, 200.0, 100.0];
        let template2 = vec![100.0, 200.0, 100.0];
        let template3 = vec![100.0, 300.0, 100.0];
        
        assert_eq!(compute_template_hash(&template1), compute_template_hash(&template2));
        assert_ne!(compute_template_hash(&template1), compute_template_hash(&template3));
    }
}
