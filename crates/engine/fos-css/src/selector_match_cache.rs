//! Selector Match Cache
//!
//! Caches CSS selector match results using bloom filter for negative
//! caching and LRU cache for positive matches.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

// ============================================================================
// Bloom Filter (Custom Implementation)
// ============================================================================

/// Compact bloom filter for "definitely doesn't match" fast-path
/// Uses fixed-size bit array with multiple hash functions
#[derive(Debug, Clone)]
pub struct BloomFilter<const SIZE: usize = 16> {
    /// Bit storage (SIZE * 64 bits)
    bits: [u64; SIZE],
    /// Number of hash functions to use
    hash_count: u8,
    /// Number of items added
    item_count: u32,
}

impl<const SIZE: usize> Default for BloomFilter<SIZE> {
    fn default() -> Self {
        Self::new(3)
    }
}

impl<const SIZE: usize> BloomFilter<SIZE> {
    /// Create a new bloom filter
    pub fn new(hash_count: u8) -> Self {
        Self {
            bits: [0; SIZE],
            hash_count: hash_count.max(1).min(8),
            item_count: 0,
        }
    }
    
    /// Total bits in filter
    const fn total_bits() -> usize {
        SIZE * 64
    }
    
    /// Add an item to the filter
    pub fn insert<T: Hash>(&mut self, item: &T) {
        let hashes = self.compute_hashes(item);
        for h in hashes {
            let (word, bit) = self.bit_position(h);
            self.bits[word] |= 1 << bit;
        }
        self.item_count += 1;
    }
    
    /// Check if item might be in filter
    /// Returns false = definitely not in set
    /// Returns true = possibly in set (may be false positive)
    pub fn might_contain<T: Hash>(&self, item: &T) -> bool {
        let hashes = self.compute_hashes(item);
        for h in hashes {
            let (word, bit) = self.bit_position(h);
            if (self.bits[word] & (1 << bit)) == 0 {
                return false;
            }
        }
        true
    }
    
    /// Clear the filter
    pub fn clear(&mut self) {
        self.bits = [0; SIZE];
        self.item_count = 0;
    }
    
    /// Estimated false positive rate
    pub fn false_positive_rate(&self) -> f64 {
        let m = Self::total_bits() as f64;
        let n = self.item_count as f64;
        let k = self.hash_count as f64;
        (1.0 - (-k * n / m).exp()).powf(k)
    }
    
    /// Number of items added
    pub fn len(&self) -> u32 {
        self.item_count
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.item_count == 0
    }
    
    /// Compute hash positions
    fn compute_hashes<T: Hash>(&self, item: &T) -> Vec<u64> {
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        item.hash(&mut hasher);
        let h1 = hasher.finish();
        
        // Use double hashing technique: h(i) = h1 + i * h2
        let h2 = h1.wrapping_mul(0x517cc1b727220a95);
        
        (0..self.hash_count)
            .map(|i| h1.wrapping_add((i as u64).wrapping_mul(h2)))
            .collect()
    }
    
    /// Get bit position (word index, bit index)
    fn bit_position(&self, hash: u64) -> (usize, u64) {
        let bit_index = hash % (Self::total_bits() as u64);
        let word = (bit_index / 64) as usize;
        let bit = bit_index % 64;
        (word, bit)
    }
}

// ============================================================================
// Selector Match Key
// ============================================================================

/// Key for selector match cache
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SelectorMatchKey {
    /// Element ID (unique within document)
    pub element_id: u32,
    /// Selector ID (index in stylesheet)
    pub selector_id: u32,
}

impl SelectorMatchKey {
    pub fn new(element_id: u32, selector_id: u32) -> Self {
        Self { element_id, selector_id }
    }
}

// ============================================================================
// Selector Match Cache
// ============================================================================

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct MatchCacheStats {
    /// Bloom filter negative hits (fast "no match")
    pub bloom_negatives: u64,
    /// Bloom filter passed (might match)
    pub bloom_passed: u64,
    /// Positive cache hits
    pub positive_hits: u64,
    /// Positive cache misses
    pub positive_misses: u64,
}

impl MatchCacheStats {
    /// Total lookups
    pub fn total_lookups(&self) -> u64 {
        self.bloom_negatives + self.bloom_passed
    }
    
    /// Bloom filter effectiveness (how many matches avoided)
    pub fn bloom_effectiveness(&self) -> f64 {
        let total = self.total_lookups();
        if total == 0 { 0.0 } else { self.bloom_negatives as f64 / total as f64 }
    }
}

/// Selector match cache with bloom filter and LRU positive cache
pub struct SelectorMatchCache {
    /// Bloom filter for negative matches ("definitely doesn't match")
    negative_bloom: BloomFilter<16>,
    /// Positive cache (element, selector) -> matches
    positive_cache: HashMap<SelectorMatchKey, CacheEntry>,
    /// LRU counter
    lru_counter: u64,
    /// Maximum positive cache entries
    max_entries: usize,
    /// Statistics
    stats: MatchCacheStats,
    /// DOM generation for invalidation
    dom_generation: u64,
}

struct CacheEntry {
    matches: bool,
    last_access: u64,
}

impl Default for SelectorMatchCache {
    fn default() -> Self {
        Self::new(10000)
    }
}

impl SelectorMatchCache {
    /// Create a new cache with max entries for positive cache
    pub fn new(max_entries: usize) -> Self {
        Self {
            negative_bloom: BloomFilter::new(3),
            positive_cache: HashMap::with_capacity(max_entries.min(4096)),
            lru_counter: 0,
            max_entries,
            stats: MatchCacheStats::default(),
            dom_generation: 0,
        }
    }
    
    /// Check if a selector matches an element
    /// Returns Some(bool) if cached, None if not in cache
    pub fn get(&mut self, key: &SelectorMatchKey) -> Option<bool> {
        // Check positive cache first
        if let Some(entry) = self.positive_cache.get_mut(key) {
            self.lru_counter += 1;
            entry.last_access = self.lru_counter;
            self.stats.positive_hits += 1;
            return Some(entry.matches);
        }
        
        // Check bloom filter for negative cache (only if bloom has entries)
        if !self.negative_bloom.is_empty() && !self.negative_bloom.might_contain(key) {
            self.stats.bloom_negatives += 1;
            return Some(false); // Definitely doesn't match
        }
        
        self.stats.bloom_passed += 1;
        self.stats.positive_misses += 1;
        None
    }
    
    /// Cache a match result
    pub fn insert(&mut self, key: SelectorMatchKey, matches: bool) {
        // If doesn't match, add to bloom filter
        if !matches {
            self.negative_bloom.insert(&key);
        }
        
        // Evict if needed
        while self.positive_cache.len() >= self.max_entries {
            self.evict_lru();
        }
        
        // Add to positive cache
        self.lru_counter += 1;
        self.positive_cache.insert(key, CacheEntry {
            matches,
            last_access: self.lru_counter,
        });
    }
    
    /// Evict least recently used entry
    fn evict_lru(&mut self) {
        if let Some(oldest) = self.positive_cache.iter()
            .min_by_key(|(_, e)| e.last_access)
            .map(|(k, _)| k.clone())
        {
            self.positive_cache.remove(&oldest);
        }
    }
    
    /// Invalidate cache (DOM changed)
    pub fn invalidate(&mut self) {
        self.positive_cache.clear();
        self.negative_bloom.clear();
        self.dom_generation += 1;
    }
    
    /// Get statistics
    pub fn stats(&self) -> &MatchCacheStats {
        &self.stats
    }
    
    /// DOM generation (for checking if cache is stale)
    pub fn dom_generation(&self) -> u64 {
        self.dom_generation
    }
    
    /// Number of positive cache entries
    pub fn len(&self) -> usize {
        self.positive_cache.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.positive_cache.is_empty()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bloom_filter_basic() {
        let mut filter: BloomFilter<4> = BloomFilter::new(3);
        
        filter.insert(&"hello");
        filter.insert(&"world");
        
        assert!(filter.might_contain(&"hello"));
        assert!(filter.might_contain(&"world"));
        // "foo" might have false positive, but likely doesn't
    }
    
    #[test]
    fn test_bloom_filter_false_negative() {
        let mut filter: BloomFilter<4> = BloomFilter::new(3);
        
        filter.insert(&42u32);
        
        // Should never have false negatives
        assert!(filter.might_contain(&42u32));
    }
    
    #[test]
    fn test_selector_match_cache_basic() {
        let mut cache = SelectorMatchCache::new(100);
        
        let key = SelectorMatchKey::new(1, 1);
        
        // Initially not cached
        assert_eq!(cache.get(&key), None);
        
        // Cache a match
        cache.insert(key.clone(), true);
        assert_eq!(cache.get(&key), Some(true));
    }
    
    #[test]
    fn test_selector_match_cache_negative() {
        let mut cache = SelectorMatchCache::new(100);
        
        let key = SelectorMatchKey::new(1, 2);
        
        // Cache a non-match
        cache.insert(key.clone(), false);
        
        // Should be in bloom filter and positive cache
        let result = cache.get(&key);
        assert_eq!(result, Some(false));
    }
    
    #[test]
    fn test_selector_match_cache_invalidate() {
        let mut cache = SelectorMatchCache::new(100);
        
        let key = SelectorMatchKey::new(1, 1);
        cache.insert(key.clone(), true);
        
        cache.invalidate();
        
        // Should be cleared
        assert!(cache.is_empty());
        assert_eq!(cache.dom_generation(), 1);
    }
    
    #[test]
    fn test_bloom_filter_false_positive_rate() {
        let mut filter: BloomFilter<16> = BloomFilter::new(3);
        
        for i in 0..100u32 {
            filter.insert(&i);
        }
        
        // With 1024 bits (16*64) and 100 items, theoretical FPR is low
        let fpr = filter.false_positive_rate();
        assert!(fpr < 0.05); // Less than 5% with fewer items
    }
}
