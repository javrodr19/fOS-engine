//! Unified Cache Manager
//!
//! Orchestrates all per-resource-type caches with memory budget management
//! and cross-cache LRU eviction. Part of Phase 1 caching optimization.

use std::collections::HashMap;
use std::time::{Duration, Instant};

// ============================================================================
// Cache Statistics
// ============================================================================

/// Statistics for a cache
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    /// Number of entries in cache
    pub entries: usize,
    /// Size in bytes
    pub size_bytes: usize,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
}

impl CacheStats {
    /// Calculate hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

/// Combined statistics across all caches
#[derive(Debug, Clone, Default)]
pub struct CacheManagerStats {
    pub http: CacheStats,
    pub dom: CacheStats,
    pub style: CacheStats,
    pub layout: CacheStats,
    pub bytecode: CacheStats,
    pub image: CacheStats,
    pub total_memory: usize,
    pub memory_budget: usize,
}

impl CacheManagerStats {
    /// Overall memory pressure (0.0 - 1.0+)
    pub fn memory_pressure(&self) -> f32 {
        if self.memory_budget == 0 { 0.0 }
        else { self.total_memory as f32 / self.memory_budget as f32 }
    }
    
    /// Overall hit rate weighted by lookups
    pub fn overall_hit_rate(&self) -> f64 {
        let caches = [&self.http, &self.dom, &self.style, &self.layout, &self.bytecode, &self.image];
        let total_hits: u64 = caches.iter().map(|c| c.hits).sum();
        let total_misses: u64 = caches.iter().map(|c| c.misses).sum();
        let total = total_hits + total_misses;
        if total == 0 { 0.0 } else { total_hits as f64 / total as f64 }
    }
}

// ============================================================================
// Cache Entry Abstraction
// ============================================================================

/// Cache entry with LRU tracking
#[derive(Debug, Clone)]
pub struct CacheEntry<T> {
    /// Cached value
    pub value: T,
    /// Size in bytes
    pub size: usize,
    /// Last access time
    pub last_access: Instant,
    /// Access count
    pub access_count: u32,
}

impl<T> CacheEntry<T> {
    pub fn new(value: T, size: usize) -> Self {
        Self {
            value,
            size,
            last_access: Instant::now(),
            access_count: 0,
        }
    }
    
    pub fn touch(&mut self) {
        self.last_access = Instant::now();
        self.access_count = self.access_count.saturating_add(1);
    }
}

// ============================================================================
// LRU Cache (Custom Implementation)
// ============================================================================

/// LRU cache with size limits - fully custom implementation
#[derive(Debug)]
pub struct LruCache<K, V> {
    entries: HashMap<K, CacheEntry<V>>,
    max_entries: usize,
    max_bytes: usize,
    current_bytes: usize,
    hits: u64,
    misses: u64,
}

impl<K: std::hash::Hash + Eq + Clone, V> LruCache<K, V> {
    /// Create new cache with limits
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(max_entries.min(1024)),
            max_entries,
            max_bytes,
            current_bytes: 0,
            hits: 0,
            misses: 0,
        }
    }
    
    /// Get an entry (updates access time)
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.touch();
            self.hits += 1;
            Some(&entry.value)
        } else {
            self.misses += 1;
            None
        }
    }
    
    /// Check if key exists without updating access time
    pub fn contains(&self, key: &K) -> bool {
        self.entries.contains_key(key)
    }
    
    /// Insert an entry
    pub fn insert(&mut self, key: K, value: V, size: usize) {
        // Evict if needed
        while self.entries.len() >= self.max_entries || 
              self.current_bytes + size > self.max_bytes {
            if !self.evict_lru() { break; }
        }
        
        // Don't cache if too large
        if size > self.max_bytes { return; }
        
        // Remove existing entry if present
        if let Some(old) = self.entries.remove(&key) {
            self.current_bytes = self.current_bytes.saturating_sub(old.size);
        }
        
        self.current_bytes += size;
        self.entries.insert(key, CacheEntry::new(value, size));
    }
    
    /// Evict the least recently used entry
    pub fn evict_lru(&mut self) -> bool {
        let oldest = self.entries.iter()
            .min_by_key(|(_, e)| e.last_access)
            .map(|(k, _)| k.clone());
        
        if let Some(key) = oldest {
            if let Some(entry) = self.entries.remove(&key) {
                self.current_bytes = self.current_bytes.saturating_sub(entry.size);
                return true;
            }
        }
        false
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.entries.clear();
        self.current_bytes = 0;
    }
    
    /// Get statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.entries.len(),
            size_bytes: self.current_bytes,
            hits: self.hits,
            misses: self.misses,
        }
    }
    
    /// Current number of entries
    pub fn len(&self) -> usize { self.entries.len() }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool { self.entries.is_empty() }
    
    /// Current size in bytes
    pub fn size_bytes(&self) -> usize { self.current_bytes }
}

// ============================================================================
// DOM Cache (Lightweight)
// ============================================================================

/// DOM node cache for fast lookups
#[derive(Debug, Default)]
pub struct DomCache {
    /// Cache by ID attribute
    by_id: HashMap<String, u32>,
    /// Cache by class names (class -> list of node ids)
    by_class: HashMap<String, Vec<u32>>,
    /// DOM generation for invalidation
    generation: u64,
    hits: u64,
    misses: u64,
}

impl DomCache {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Invalidate cache (DOM mutated)
    pub fn invalidate(&mut self) {
        self.by_id.clear();
        self.by_class.clear();
        self.generation += 1;
    }
    
    /// Cache element by ID
    pub fn cache_id(&mut self, id: String, node_id: u32) {
        self.by_id.insert(id, node_id);
    }
    
    /// Lookup by ID
    pub fn get_by_id(&mut self, id: &str) -> Option<u32> {
        if let Some(&node_id) = self.by_id.get(id) {
            self.hits += 1;
            Some(node_id)
        } else {
            self.misses += 1;
            None
        }
    }
    
    /// Get statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.by_id.len() + self.by_class.len(),
            size_bytes: (self.by_id.len() * 32) + (self.by_class.len() * 64), // Estimate
            hits: self.hits,
            misses: self.misses,
        }
    }
    
    pub fn generation(&self) -> u64 { self.generation }
}

// ============================================================================
// Cache Type Enum
// ============================================================================

/// Identifies which cache to evict from
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CacheType {
    Http,
    Dom,
    Style,
    Layout,
    Bytecode,
    Image,
}

/// Cache priority for eviction (lower = evict first)
impl CacheType {
    pub fn priority(&self) -> u8 {
        match self {
            CacheType::Http => 3,      // Medium - network is slow to refetch
            CacheType::Dom => 1,       // Low - usually quick to rebuild
            CacheType::Style => 4,     // High - frequent lookups
            CacheType::Layout => 5,    // Highest - expensive to recompute
            CacheType::Bytecode => 4,  // High - expensive to recompile
            CacheType::Image => 2,     // Low-Medium - large but reloadable
        }
    }
}

// ============================================================================
// Unified Cache Manager
// ============================================================================

/// Unified cache manager orchestrating all caches
pub struct CacheManager {
    /// Memory budget across all caches
    memory_budget: usize,
    /// Disk budget (for persistence)
    disk_budget: usize,
    
    // Per-resource-type caches
    http_cache: LruCache<String, Vec<u8>>,
    style_cache: LruCache<u64, Vec<u8>>,       // StyleCacheKey hash -> Computed style bytes
    layout_cache: LruCache<u64, Vec<u8>>,      // LayoutCacheKey hash -> Layout bytes  
    bytecode_cache: LruCache<u64, Vec<u8>>,    // Script hash -> Bytecode
    image_cache: LruCache<String, Vec<u8>>,    // URL -> Decoded image bytes
    dom_cache: DomCache,
    
    /// Memory pressure threshold to start eviction
    pressure_threshold: f32,
    /// Target pressure after eviction
    target_pressure: f32,
}

impl CacheManager {
    /// Create a new cache manager with budget
    pub fn new(memory_budget: usize, disk_budget: usize) -> Self {
        // Allocate budget across caches
        let http_budget = memory_budget / 4;        // 25%
        let style_budget = memory_budget / 6;       // ~17%
        let layout_budget = memory_budget / 6;      // ~17%
        let bytecode_budget = memory_budget / 6;    // ~17%
        let image_budget = memory_budget / 4;       // 25%
        
        Self {
            memory_budget,
            disk_budget,
            http_cache: LruCache::new(1000, http_budget),
            style_cache: LruCache::new(5000, style_budget),
            layout_cache: LruCache::new(2000, layout_budget),
            bytecode_cache: LruCache::new(500, bytecode_budget),
            image_cache: LruCache::new(200, image_budget),
            dom_cache: DomCache::new(),
            pressure_threshold: 0.9,
            target_pressure: 0.7,
        }
    }
    
    /// Create with default 100MB budget
    pub fn default_budget() -> Self {
        Self::new(100 * 1024 * 1024, 500 * 1024 * 1024)
    }
    
    /// Calculate current memory pressure (0.0 - 1.0+)
    pub fn memory_pressure(&self) -> f32 {
        self.current_usage() as f32 / self.memory_budget as f32
    }
    
    /// Current memory usage across all caches
    pub fn current_usage(&self) -> usize {
        self.http_cache.size_bytes() +
        self.style_cache.size_bytes() +
        self.layout_cache.size_bytes() +
        self.bytecode_cache.size_bytes() +
        self.image_cache.size_bytes() +
        self.dom_cache.stats().size_bytes
    }
    
    /// Evict until reaching target pressure
    pub fn evict_to_target(&mut self, target: f32) {
        while self.memory_pressure() > target {
            if !self.evict_coldest() {
                break;
            }
        }
    }
    
    /// Evict from lowest priority cache with data
    fn evict_coldest(&mut self) -> bool {
        // Find cache with lowest priority that has data
        let candidates = [
            (CacheType::Dom, self.dom_cache.stats().size_bytes, CacheType::Dom.priority()),
            (CacheType::Image, self.image_cache.size_bytes(), CacheType::Image.priority()),
            (CacheType::Http, self.http_cache.size_bytes(), CacheType::Http.priority()),
            (CacheType::Style, self.style_cache.size_bytes(), CacheType::Style.priority()),
            (CacheType::Bytecode, self.bytecode_cache.size_bytes(), CacheType::Bytecode.priority()),
            (CacheType::Layout, self.layout_cache.size_bytes(), CacheType::Layout.priority()),
        ];
        
        // Find lowest priority cache with data
        if let Some((cache_type, _, _)) = candidates.iter()
            .filter(|(_, size, _)| *size > 0)
            .min_by_key(|(_, _, priority)| *priority)
        {
            match cache_type {
                CacheType::Dom => { self.dom_cache.invalidate(); true }
                CacheType::Image => self.image_cache.evict_lru(),
                CacheType::Http => self.http_cache.evict_lru(),
                CacheType::Style => self.style_cache.evict_lru(),
                CacheType::Bytecode => self.bytecode_cache.evict_lru(),
                CacheType::Layout => self.layout_cache.evict_lru(),
            }
        } else {
            false
        }
    }
    
    /// Check memory pressure and evict if needed
    pub fn maybe_evict(&mut self) {
        if self.memory_pressure() > self.pressure_threshold {
            self.evict_to_target(self.target_pressure);
        }
    }
    
    /// Get combined statistics
    pub fn stats(&self) -> CacheManagerStats {
        CacheManagerStats {
            http: self.http_cache.stats(),
            dom: self.dom_cache.stats(),
            style: self.style_cache.stats(),
            layout: self.layout_cache.stats(),
            bytecode: self.bytecode_cache.stats(),
            image: self.image_cache.stats(),
            total_memory: self.current_usage(),
            memory_budget: self.memory_budget,
        }
    }
    
    /// Clear all caches
    pub fn clear_all(&mut self) {
        self.http_cache.clear();
        self.style_cache.clear();
        self.layout_cache.clear();
        self.bytecode_cache.clear();
        self.image_cache.clear();
        self.dom_cache.invalidate();
    }
    
    // ========================================================================
    // HTTP Cache accessors
    // ========================================================================
    
    pub fn http_cache(&mut self) -> &mut LruCache<String, Vec<u8>> {
        &mut self.http_cache
    }
    
    // ========================================================================
    // Style Cache accessors
    // ========================================================================
    
    pub fn style_cache(&mut self) -> &mut LruCache<u64, Vec<u8>> {
        &mut self.style_cache
    }
    
    // ========================================================================
    // Layout Cache accessors
    // ========================================================================
    
    pub fn layout_cache(&mut self) -> &mut LruCache<u64, Vec<u8>> {
        &mut self.layout_cache
    }
    
    // ========================================================================
    // Bytecode Cache accessors
    // ========================================================================
    
    pub fn bytecode_cache(&mut self) -> &mut LruCache<u64, Vec<u8>> {
        &mut self.bytecode_cache
    }
    
    // ========================================================================
    // Image Cache accessors
    // ========================================================================
    
    pub fn image_cache(&mut self) -> &mut LruCache<String, Vec<u8>> {
        &mut self.image_cache
    }
    
    // ========================================================================
    // DOM Cache accessors
    // ========================================================================
    
    pub fn dom_cache(&mut self) -> &mut DomCache {
        &mut self.dom_cache
    }
}

impl Default for CacheManager {
    fn default() -> Self {
        Self::default_budget()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lru_cache_basic() {
        let mut cache: LruCache<String, Vec<u8>> = LruCache::new(10, 1024);
        
        cache.insert("key1".to_string(), vec![1, 2, 3], 3);
        assert!(cache.contains(&"key1".to_string()));
        assert_eq!(cache.get(&"key1".to_string()), Some(&vec![1, 2, 3]));
    }
    
    #[test]
    fn test_lru_cache_eviction() {
        let mut cache: LruCache<String, Vec<u8>> = LruCache::new(3, 1024);
        
        cache.insert("key1".to_string(), vec![1], 1);
        cache.insert("key2".to_string(), vec![2], 1);
        cache.insert("key3".to_string(), vec![3], 1);
        
        // Touch key1 and key2
        cache.get(&"key1".to_string());
        cache.get(&"key2".to_string());
        
        // Insert key4, should evict key3 (least recently used)
        cache.insert("key4".to_string(), vec![4], 1);
        
        assert_eq!(cache.len(), 3);
        assert!(!cache.contains(&"key3".to_string()));
    }
    
    #[test]
    fn test_lru_cache_size_limit() {
        let mut cache: LruCache<String, Vec<u8>> = LruCache::new(100, 10);
        
        cache.insert("key1".to_string(), vec![0; 5], 5);
        cache.insert("key2".to_string(), vec![0; 5], 5);
        
        // Cache is at limit
        assert_eq!(cache.size_bytes(), 10);
        
        // Insert more - should evict
        cache.insert("key3".to_string(), vec![0; 5], 5);
        assert!(cache.size_bytes() <= 10);
    }
    
    #[test]
    fn test_cache_manager_creation() {
        let manager = CacheManager::default_budget();
        assert!(manager.memory_pressure() < 0.01);
    }
    
    #[test]
    fn test_cache_manager_eviction() {
        let mut manager = CacheManager::new(100, 200);
        
        // Fill up caches
        for i in 0..20 {
            manager.http_cache().insert(format!("url{}", i), vec![0; 10], 10);
        }
        
        // Should have evicted some
        assert!(manager.current_usage() <= 100);
    }
    
    #[test]
    fn test_cache_stats() {
        let mut cache: LruCache<String, Vec<u8>> = LruCache::new(10, 1024);
        
        cache.insert("key1".to_string(), vec![1, 2, 3], 3);
        cache.get(&"key1".to_string());
        cache.get(&"missing".to_string());
        
        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.size_bytes, 3);
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate() - 0.5).abs() < 0.01);
    }
    
    #[test]
    fn test_dom_cache() {
        let mut cache = DomCache::new();
        
        cache.cache_id("header".to_string(), 42);
        assert_eq!(cache.get_by_id("header"), Some(42));
        assert_eq!(cache.get_by_id("footer"), None);
        
        cache.invalidate();
        assert_eq!(cache.get_by_id("header"), None);
        assert_eq!(cache.generation(), 1);
    }
}
