//! Predictive Layout Cache (Phase 24.1)
//!
//! Caches layout results keyed by Hash(DOM structure + viewport) for
//! instant layout skip on repeat visits. Persists cache to disk
//! between sessions for 100% layout skip on revisits.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Layout cache key - uniquely identifies a layout computation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayoutCacheKey {
    /// Hash of DOM structure
    pub dom_hash: u64,
    /// Viewport dimensions
    pub viewport_width: u32,
    pub viewport_height: u32,
    /// Device pixel ratio (scaled by 100 for integer comparison)
    pub dpr_scaled: u32,
    /// Font size base (for responsive layouts)
    pub font_size_base: u16,
}

impl LayoutCacheKey {
    pub fn new(dom_hash: u64, width: u32, height: u32, dpr: f32, font_size: u16) -> Self {
        Self {
            dom_hash,
            viewport_width: width,
            viewport_height: height,
            dpr_scaled: (dpr * 100.0) as u32,
            font_size_base: font_size,
        }
    }
    
    /// Create a fuzzy key that matches similar viewports
    pub fn fuzzy(dom_hash: u64, width: u32, height: u32) -> Self {
        // Round to nearest 100px for fuzzy matching
        Self {
            dom_hash,
            viewport_width: (width / 100) * 100,
            viewport_height: (height / 100) * 100,
            dpr_scaled: 100, // Assume 1.0
            font_size_base: 16,
        }
    }
}

/// Cached layout result
#[derive(Debug, Clone)]
pub struct CachedLayout {
    /// Serialized layout tree
    pub layout_data: Vec<u8>,
    /// When this was cached
    pub cached_at: SystemTime,
    /// How many times this cache was hit
    pub hit_count: u32,
    /// Size of the DOM that produced this layout
    pub dom_node_count: u32,
    /// Time the original layout took (for statistics)
    pub original_layout_time: Duration,
}

impl CachedLayout {
    pub fn new(layout_data: Vec<u8>, node_count: u32, layout_time: Duration) -> Self {
        Self {
            layout_data,
            cached_at: SystemTime::now(),
            hit_count: 0,
            dom_node_count: node_count,
            original_layout_time: layout_time,
        }
    }
    
    /// Mark as hit and return the data
    pub fn hit(&mut self) -> &[u8] {
        self.hit_count += 1;
        &self.layout_data
    }
    
    /// Age of this cache entry
    pub fn age(&self) -> Duration {
        SystemTime::now()
            .duration_since(self.cached_at)
            .unwrap_or(Duration::ZERO)
    }
    
    /// Estimated memory size
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.layout_data.len()
    }
}

/// LRU entry for the cache
struct LruEntry {
    key: LayoutCacheKey,
    last_access: SystemTime,
}

/// Predictive layout cache with LRU eviction
pub struct LayoutCache {
    /// Cached layouts
    cache: HashMap<LayoutCacheKey, CachedLayout>,
    /// LRU tracking
    lru: Vec<LruEntry>,
    /// Maximum cache size in bytes
    max_size_bytes: usize,
    /// Current size
    current_size: usize,
    /// Maximum entries
    max_entries: usize,
    /// Disk persistence path
    disk_path: Option<PathBuf>,
    /// Statistics
    stats: CacheStats,
}

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub total_time_saved: Duration,
    pub entries_persisted: u64,
    pub entries_loaded: u64,
}

impl CacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl Default for LayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutCache {
    /// Create a new cache with default settings
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            lru: Vec::new(),
            max_size_bytes: 64 * 1024 * 1024, // 64 MB
            current_size: 0,
            max_entries: 10000,
            disk_path: None,
            stats: CacheStats::default(),
        }
    }
    
    /// Set maximum size in bytes
    pub fn with_max_size(mut self, bytes: usize) -> Self {
        self.max_size_bytes = bytes;
        self
    }
    
    /// Set maximum entries
    pub fn with_max_entries(mut self, entries: usize) -> Self {
        self.max_entries = entries;
        self
    }
    
    /// Enable disk persistence
    pub fn with_persistence(mut self, path: PathBuf) -> Self {
        self.disk_path = Some(path);
        self
    }
    
    /// Get a cached layout
    pub fn get(&mut self, key: &LayoutCacheKey) -> Option<&[u8]> {
        if let Some(entry) = self.cache.get_mut(key) {
            self.stats.hits += 1;
            self.stats.total_time_saved += entry.original_layout_time;
            
            // Update LRU
            if let Some(lru_entry) = self.lru.iter_mut().find(|e| &e.key == key) {
                lru_entry.last_access = SystemTime::now();
            }
            
            Some(entry.hit())
        } else {
            self.stats.misses += 1;
            None
        }
    }
    
    /// Insert a layout into the cache
    pub fn insert(&mut self, key: LayoutCacheKey, layout: CachedLayout) {
        let size = layout.memory_size();
        
        // Evict if necessary
        while self.current_size + size > self.max_size_bytes || self.cache.len() >= self.max_entries {
            if !self.evict_lru() {
                break;
            }
        }
        
        self.current_size += size;
        self.lru.push(LruEntry {
            key: key.clone(),
            last_access: SystemTime::now(),
        });
        self.cache.insert(key, layout);
    }
    
    /// Evict least recently used entry
    fn evict_lru(&mut self) -> bool {
        if self.lru.is_empty() {
            return false;
        }
        
        // Find oldest entry
        let oldest_idx = self.lru.iter()
            .enumerate()
            .min_by_key(|(_, e)| e.last_access)
            .map(|(i, _)| i);
        
        if let Some(idx) = oldest_idx {
            let entry = self.lru.remove(idx);
            if let Some(cached) = self.cache.remove(&entry.key) {
                self.current_size = self.current_size.saturating_sub(cached.memory_size());
                self.stats.evictions += 1;
                return true;
            }
        }
        
        false
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.lru.clear();
        self.current_size = 0;
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }
    
    /// Number of entries
    pub fn len(&self) -> usize {
        self.cache.len()
    }
    
    /// Is the cache empty?
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
    
    /// Current memory usage
    pub fn memory_usage(&self) -> usize {
        self.current_size
    }
    
    /// Persist cache to disk
    pub fn persist(&mut self) -> std::io::Result<()> {
        let path = match &self.disk_path {
            Some(p) => p.clone(),
            None => return Ok(()),
        };
        
        let file = std::fs::File::create(&path)?;
        let mut writer = std::io::BufWriter::new(file);
        
        // Simple binary format: count, then entries
        let count = self.cache.len() as u32;
        writer.write_all(&count.to_le_bytes())?;
        
        for (key, layout) in &self.cache {
            // Write key
            writer.write_all(&key.dom_hash.to_le_bytes())?;
            writer.write_all(&key.viewport_width.to_le_bytes())?;
            writer.write_all(&key.viewport_height.to_le_bytes())?;
            writer.write_all(&key.dpr_scaled.to_le_bytes())?;
            writer.write_all(&key.font_size_base.to_le_bytes())?;
            
            // Write layout data length and content
            let len = layout.layout_data.len() as u32;
            writer.write_all(&len.to_le_bytes())?;
            writer.write_all(&layout.layout_data)?;
            
            // Write metadata
            writer.write_all(&layout.dom_node_count.to_le_bytes())?;
            writer.write_all(&layout.original_layout_time.as_nanos().to_le_bytes())?;
            
            self.stats.entries_persisted += 1;
        }
        
        writer.flush()?;
        Ok(())
    }
    
    /// Load cache from disk
    pub fn load(&mut self) -> std::io::Result<()> {
        let path = match &self.disk_path {
            Some(p) => p.clone(),
            None => return Ok(()),
        };
        
        if !path.exists() {
            return Ok(());
        }
        
        let file = std::fs::File::open(&path)?;
        let mut reader = std::io::BufReader::new(file);
        
        let mut buf4 = [0u8; 4];
        let mut buf8 = [0u8; 8];
        let mut buf16 = [0u8; 16];
        let mut buf2 = [0u8; 2];
        
        // Read count
        reader.read_exact(&mut buf4)?;
        let count = u32::from_le_bytes(buf4);
        
        for _ in 0..count {
            // Read key
            reader.read_exact(&mut buf8)?;
            let dom_hash = u64::from_le_bytes(buf8);
            
            reader.read_exact(&mut buf4)?;
            let viewport_width = u32::from_le_bytes(buf4);
            
            reader.read_exact(&mut buf4)?;
            let viewport_height = u32::from_le_bytes(buf4);
            
            reader.read_exact(&mut buf4)?;
            let dpr_scaled = u32::from_le_bytes(buf4);
            
            reader.read_exact(&mut buf2)?;
            let font_size_base = u16::from_le_bytes(buf2);
            
            let key = LayoutCacheKey {
                dom_hash,
                viewport_width,
                viewport_height,
                dpr_scaled,
                font_size_base,
            };
            
            // Read layout data
            reader.read_exact(&mut buf4)?;
            let len = u32::from_le_bytes(buf4) as usize;
            let mut layout_data = vec![0u8; len];
            reader.read_exact(&mut layout_data)?;
            
            // Read metadata
            reader.read_exact(&mut buf4)?;
            let dom_node_count = u32::from_le_bytes(buf4);
            
            reader.read_exact(&mut buf16)?;
            let nanos = u128::from_le_bytes(buf16);
            let original_layout_time = Duration::from_nanos(nanos as u64);
            
            let layout = CachedLayout {
                layout_data,
                cached_at: SystemTime::now(),
                hit_count: 0,
                dom_node_count,
                original_layout_time,
            };
            
            self.insert(key, layout);
            self.stats.entries_loaded += 1;
        }
        
        Ok(())
    }
}

/// DOM structure hasher for cache keys
pub struct DomHasher {
    state: u64,
}

impl Default for DomHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl DomHasher {
    pub fn new() -> Self {
        Self { state: 0xcbf29ce484222325 } // FNV offset basis
    }
    
    /// Hash a tag name
    pub fn hash_tag(&mut self, tag: &str) {
        for byte in tag.bytes() {
            self.state ^= byte as u64;
            self.state = self.state.wrapping_mul(0x100000001b3); // FNV prime
        }
    }
    
    /// Hash an attribute name (not value - structure only)
    pub fn hash_attr_name(&mut self, name: &str) {
        for byte in name.bytes() {
            self.state ^= byte as u64;
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }
    
    /// Hash child count
    pub fn hash_child_count(&mut self, count: usize) {
        self.state ^= count as u64;
        self.state = self.state.wrapping_mul(0x100000001b3);
    }
    
    /// Finalize and get the hash
    pub fn finish(self) -> u64 {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_key() {
        let key1 = LayoutCacheKey::new(12345, 1920, 1080, 1.0, 16);
        let key2 = LayoutCacheKey::new(12345, 1920, 1080, 1.0, 16);
        let key3 = LayoutCacheKey::new(12345, 1920, 1080, 2.0, 16);
        
        assert_eq!(key1, key2);
        assert_ne!(key1, key3);
    }
    
    #[test]
    fn test_fuzzy_key() {
        let fuzzy1 = LayoutCacheKey::fuzzy(123, 1920, 1080);
        let fuzzy2 = LayoutCacheKey::fuzzy(123, 1950, 1050);
        
        // Both round to 1900x1000
        assert_eq!(fuzzy1, fuzzy2);
    }
    
    #[test]
    fn test_cache_insert_get() {
        let mut cache = LayoutCache::new();
        let key = LayoutCacheKey::new(1, 800, 600, 1.0, 16);
        let layout = CachedLayout::new(vec![1, 2, 3], 100, Duration::from_millis(10));
        
        cache.insert(key.clone(), layout);
        
        let result = cache.get(&key);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), &[1, 2, 3]);
    }
    
    #[test]
    fn test_cache_eviction() {
        let mut cache = LayoutCache::new()
            .with_max_entries(2);
        
        for i in 0..5 {
            let key = LayoutCacheKey::new(i, 800, 600, 1.0, 16);
            let layout = CachedLayout::new(vec![i as u8], 10, Duration::from_millis(1));
            cache.insert(key, layout);
        }
        
        // Should have at most 2 entries
        assert!(cache.len() <= 2);
        assert!(cache.stats().evictions > 0);
    }
    
    #[test]
    fn test_dom_hasher() {
        let mut hasher1 = DomHasher::new();
        hasher1.hash_tag("div");
        hasher1.hash_attr_name("class");
        hasher1.hash_child_count(3);
        let hash1 = hasher1.finish();
        
        let mut hasher2 = DomHasher::new();
        hasher2.hash_tag("div");
        hasher2.hash_attr_name("class");
        hasher2.hash_child_count(3);
        let hash2 = hasher2.finish();
        
        assert_eq!(hash1, hash2);
        
        let mut hasher3 = DomHasher::new();
        hasher3.hash_tag("span");
        let hash3 = hasher3.finish();
        
        assert_ne!(hash1, hash3);
    }
    
    #[test]
    fn test_cache_stats() {
        let mut cache = LayoutCache::new();
        let key = LayoutCacheKey::new(1, 800, 600, 1.0, 16);
        let layout = CachedLayout::new(vec![1], 10, Duration::from_millis(5));
        
        cache.insert(key.clone(), layout);
        
        // Miss first on different key
        cache.get(&LayoutCacheKey::new(999, 800, 600, 1.0, 16));
        assert_eq!(cache.stats().misses, 1);
        
        // Hit on the inserted key
        cache.get(&key);
        assert_eq!(cache.stats().hits, 1);
        assert_eq!(cache.stats().total_time_saved, Duration::from_millis(5));
    }
}
