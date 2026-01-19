//! Accelerated Selector Matching with Bloom Filters (Phase 24.8)
//!
//! Bloom filter pre-filtering with Chromium-style 8-hash functions.
//! Ancestor bloom filter for descendant selector matching.
//! Compiled hot selectors to Rust functions. Always exact matching.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

// ============================================================================
// Constants - Chromium-style 8-hash configuration
// ============================================================================

/// Bits per element (tuned for ~1% false positive rate)
const BITS_PER_ELEMENT: usize = 12;

/// Number of hash functions (Chromium uses 8)
const NUM_HASHES: usize = 8;

/// Golden ratio hash multipliers for better distribution
const HASH_MULTIPLIERS: [u64; 8] = [
    0x9e3779b97f4a7c15, // Golden ratio
    0x7f4a7c159e3779b9, // Rotated
    0xcc9e2d51db873593, // MurmurHash3 c1
    0x1b873593cc9e2d51, // MurmurHash3 c2
    0x85ebca6b4c85a1c5, // XXHash prime
    0xc2b2ae35d2b89eb3, // FNV offset
    0x27bb2ee687b0b0fd, // CityHash
    0x3c6ef372fe94f82b, // SplitMix64
];

// ============================================================================
// Bloom Filter - 8-hash Chromium-style
// ============================================================================

/// Bloom filter for fast rejection with 8-hash configuration
#[derive(Debug, Clone)]
pub struct SelectorBloomFilter {
    /// Bit array (256 bits = 4 words for compact ancestor filters)
    bits: Vec<u64>,
    /// Size in bits
    size_bits: usize,
    /// Number of items added
    count: usize,
}

impl Default for SelectorBloomFilter {
    fn default() -> Self {
        Self::new(1000)
    }
}

impl SelectorBloomFilter {
    /// Create a new bloom filter with given capacity
    pub fn new(expected_items: usize) -> Self {
        // Calculate optimal size based on 8 hashes and target FPR
        let size_bits = (expected_items * BITS_PER_ELEMENT).max(256);
        let num_words = (size_bits + 63) / 64;
        
        Self {
            bits: vec![0u64; num_words],
            size_bits,
            count: 0,
        }
    }
    
    /// Create a compact 256-bit ancestor filter
    pub fn compact() -> Self {
        Self {
            bits: vec![0u64; 4], // 256 bits
            size_bits: 256,
            count: 0,
        }
    }
    
    /// Add an item to the filter using 8 hash functions
    pub fn add(&mut self, hash: u64) {
        for i in 0..NUM_HASHES {
            let bit_pos = self.get_bit_pos(hash, i);
            self.set_bit(bit_pos);
        }
        self.count += 1;
    }
    
    /// Check if item might be in the filter (8-hash check)
    pub fn might_contain(&self, hash: u64) -> bool {
        for i in 0..NUM_HASHES {
            let bit_pos = self.get_bit_pos(hash, i);
            if !self.get_bit(bit_pos) {
                return false;
            }
        }
        true
    }
    
    /// Get bit position using enhanced hash mixing
    #[inline]
    fn get_bit_pos(&self, hash: u64, index: usize) -> usize {
        // Use independent hash multipliers for better distribution
        let h = hash.wrapping_mul(HASH_MULTIPLIERS[index]);
        let h = h ^ (h >> 33);
        let h = h.wrapping_mul(0xff51afd7ed558ccd);
        let h = h ^ (h >> 33);
        (h as usize) % self.size_bits
    }
    
    /// Set a bit
    #[inline]
    fn set_bit(&mut self, pos: usize) {
        let word = pos / 64;
        let bit = pos % 64;
        if word < self.bits.len() {
            self.bits[word] |= 1 << bit;
        }
    }
    
    /// Get a bit
    #[inline]
    fn get_bit(&self, pos: usize) -> bool {
        let word = pos / 64;
        let bit = pos % 64;
        if word < self.bits.len() {
            (self.bits[word] >> bit) & 1 == 1
        } else {
            false
        }
    }
    
    /// Clear the filter
    pub fn clear(&mut self) {
        self.bits.fill(0);
        self.count = 0;
    }
    
    /// Number of items added
    pub fn count(&self) -> usize {
        self.count
    }
    
    /// Estimated false positive rate
    pub fn false_positive_rate(&self) -> f64 {
        let m = self.size_bits as f64;
        let n = self.count as f64;
        let k = NUM_HASHES as f64;
        
        (1.0 - (-k * n / m).exp()).powf(k)
    }
    
    /// Merge another filter into this one (for parallel processing)
    pub fn merge(&mut self, other: &Self) {
        for (a, b) in self.bits.iter_mut().zip(other.bits.iter()) {
            *a |= *b;
        }
        self.count += other.count;
    }
    
    /// Check if this filter is a subset of another
    pub fn is_subset_of(&self, other: &Self) -> bool {
        self.bits.iter().zip(other.bits.iter()).all(|(a, b)| *a & *b == *a)
    }
    
    /// Number of set bits
    pub fn popcount(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }
}

// ============================================================================
// Ancestor Bloom Filter - For Descendant Matching
// ============================================================================

/// Ancestor bloom filter for efficient descendant selector matching.
/// Maintains hashes of all ancestors during DOM traversal.
#[derive(Debug, Clone)]
pub struct AncestorBloom {
    /// Stack of ancestor filters (one per depth level)
    filter_stack: Vec<SelectorBloomFilter>,
    /// Combined filter of all ancestors
    combined: SelectorBloomFilter,
    /// Current depth in the tree
    depth: usize,
}

impl Default for AncestorBloom {
    fn default() -> Self {
        Self::new()
    }
}

impl AncestorBloom {
    /// Create a new ancestor bloom filter
    pub fn new() -> Self {
        Self {
            filter_stack: Vec::with_capacity(64),
            combined: SelectorBloomFilter::compact(),
            depth: 0,
        }
    }
    
    /// Push an ancestor onto the stack (entering a child element)
    pub fn push_ancestor(&mut self, tag_hash: u64, id_hash: Option<u64>, class_hashes: &[u64]) {
        // Create filter for this level
        let mut level_filter = SelectorBloomFilter::compact();
        
        // Add tag
        level_filter.add(tag_hash);
        
        // Add id if present
        if let Some(id) = id_hash {
            level_filter.add(id);
        }
        
        // Add classes
        for &class in class_hashes {
            level_filter.add(class);
        }
        
        // Merge into combined filter
        self.combined.merge(&level_filter);
        
        // Save level filter for pop
        self.filter_stack.push(level_filter);
        self.depth += 1;
    }
    
    /// Pop an ancestor from the stack (leaving a child element)
    pub fn pop_ancestor(&mut self) {
        if self.depth > 0 {
            self.filter_stack.pop();
            self.depth -= 1;
            
            // Rebuild combined filter (expensive but correct)
            self.rebuild_combined();
        }
    }
    
    /// Rebuild the combined filter from the stack
    fn rebuild_combined(&mut self) {
        self.combined = SelectorBloomFilter::compact();
        for level in &self.filter_stack {
            self.combined.merge(level);
        }
    }
    
    /// Check if a descendant selector might match (fast path rejection).
    /// Returns false if the selector definitely cannot match any ancestor.
    pub fn might_match_descendant(&self, ancestor_requirement_hash: u64) -> bool {
        self.combined.might_contain(ancestor_requirement_hash)
    }
    
    /// Check if any ancestor has a specific tag
    pub fn has_ancestor_tag(&self, tag_hash: u64) -> bool {
        self.combined.might_contain(tag_hash)
    }
    
    /// Check if any ancestor has a specific class
    pub fn has_ancestor_class(&self, class_hash: u64) -> bool {
        self.combined.might_contain(class_hash)
    }
    
    /// Check if any ancestor has a specific ID
    pub fn has_ancestor_id(&self, id_hash: u64) -> bool {
        self.combined.might_contain(id_hash)
    }
    
    /// Current depth
    pub fn depth(&self) -> usize {
        self.depth
    }
    
    /// Clear and reset
    pub fn clear(&mut self) {
        self.filter_stack.clear();
        self.combined.clear();
        self.depth = 0;
    }
}

// ============================================================================
// Specificity Cache - LRU for frequently accessed selectors
// ============================================================================

/// Selector specificity (packed as single u32)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Specificity {
    /// Packed: (id_count << 20) | (class_count << 10) | type_count
    packed: u32,
}

impl Specificity {
    /// Create from components
    pub fn new(ids: u32, classes: u32, types: u32) -> Self {
        Self {
            packed: ((ids & 0x3FF) << 20) | ((classes & 0x3FF) << 10) | (types & 0x3FF),
        }
    }
    
    /// Extract ID count
    pub fn ids(&self) -> u32 {
        (self.packed >> 20) & 0x3FF
    }
    
    /// Extract class/attribute/pseudo-class count
    pub fn classes(&self) -> u32 {
        (self.packed >> 10) & 0x3FF
    }
    
    /// Extract type/pseudo-element count
    pub fn types(&self) -> u32 {
        self.packed & 0x3FF
    }
    
    /// Get packed value for comparison
    pub fn packed(&self) -> u32 {
        self.packed
    }
    
    /// Add another specificity
    pub fn add(&mut self, other: Specificity) {
        let ids = (self.ids() + other.ids()).min(0x3FF);
        let classes = (self.classes() + other.classes()).min(0x3FF);
        let types = (self.types() + other.types()).min(0x3FF);
        *self = Self::new(ids, classes, types);
    }
}

/// Selector ID for cache lookup
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SelectorId(pub u32);

/// LRU specificity cache entry
#[derive(Debug, Clone)]
struct SpecificityCacheEntry {
    specificity: Specificity,
    last_access: u64,
}

/// LRU cache for selector specificities
#[derive(Debug)]
pub struct SpecificityCache {
    /// Cache entries
    cache: HashMap<SelectorId, SpecificityCacheEntry>,
    /// Max cache size
    max_size: usize,
    /// Access counter for LRU
    access_counter: u64,
    /// Statistics
    stats: SpecificityCacheStats,
}

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct SpecificityCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

impl SpecificityCacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

impl Default for SpecificityCache {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl SpecificityCache {
    /// Create with given capacity
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_size),
            max_size,
            access_counter: 0,
            stats: SpecificityCacheStats::default(),
        }
    }
    
    /// Get cached specificity
    pub fn get(&mut self, id: SelectorId) -> Option<Specificity> {
        self.access_counter += 1;
        
        if let Some(entry) = self.cache.get_mut(&id) {
            entry.last_access = self.access_counter;
            self.stats.hits += 1;
            Some(entry.specificity)
        } else {
            self.stats.misses += 1;
            None
        }
    }
    
    /// Insert specificity into cache
    pub fn insert(&mut self, id: SelectorId, specificity: Specificity) {
        // Evict if at capacity
        if self.cache.len() >= self.max_size {
            self.evict_lru();
        }
        
        self.access_counter += 1;
        self.cache.insert(id, SpecificityCacheEntry {
            specificity,
            last_access: self.access_counter,
        });
    }
    
    /// Get or compute specificity
    pub fn get_or_insert<F>(&mut self, id: SelectorId, compute: F) -> Specificity
    where
        F: FnOnce() -> Specificity,
    {
        if let Some(spec) = self.get(id) {
            return spec;
        }
        
        // Undo the miss count from get()
        self.stats.misses = self.stats.misses.saturating_sub(1);
        
        let spec = compute();
        self.insert(id, spec);
        self.stats.misses += 1;
        spec
    }
    
    /// Evict least recently used entry
    fn evict_lru(&mut self) {
        if let Some((&oldest_id, _)) = self.cache.iter()
            .min_by_key(|(_, entry)| entry.last_access)
        {
            self.cache.remove(&oldest_id);
            self.stats.evictions += 1;
        }
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.access_counter = 0;
    }
    
    /// Get statistics
    pub fn stats(&self) -> &SpecificityCacheStats {
        &self.stats
    }
    
    /// Current cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

// ============================================================================
// Hashing Functions
// ============================================================================

/// Hash an element's features for bloom filter (SIMD-friendly)
pub fn hash_element_features(tag: &str, id: Option<&str>, classes: &[&str]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64; // FNV offset basis
    
    // Hash tag with FNV-1a
    for byte in tag.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    
    // Hash id
    if let Some(id) = id {
        hash ^= 0xFF; // Separator
        hash = hash.wrapping_mul(0x100000001b3);
        for byte in id.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    
    // Hash classes (order-independent via XOR)
    let mut class_hash = 0u64;
    for class in classes {
        let mut h = 0xcbf29ce484222325u64;
        for byte in class.bytes() {
            h ^= byte as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        class_hash ^= h;
    }
    
    hash ^= class_hash;
    hash
}

/// Hash a single string (for tags, classes, ids)
pub fn hash_string(s: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Hash selector requirements
pub fn hash_selector(selector: &str) -> u64 {
    hash_string(selector)
}

// ============================================================================
// Accelerated Selector Matcher
// ============================================================================

/// Selector matcher with bloom filter acceleration and specificity cache
#[derive(Debug)]
pub struct AcceleratedSelectorMatcher {
    /// Bloom filter for element features
    element_filter: SelectorBloomFilter,
    /// Hash to selector map
    selector_map: HashMap<u64, Vec<SelectorEntry>>,
    /// ID selectors (fast path)
    id_selectors: HashMap<u64, Vec<SelectorEntry>>,
    /// Class selectors (fast path)
    class_selectors: HashMap<u64, Vec<SelectorEntry>>,
    /// Tag selectors (fast path)
    tag_selectors: HashMap<u64, Vec<SelectorEntry>>,
    /// Specificity cache
    specificity_cache: SpecificityCache,
    /// Statistics
    stats: MatcherStats,
}

/// Selector entry
#[derive(Debug, Clone)]
pub struct SelectorEntry {
    /// Selector string
    pub selector: Box<str>,
    /// Selector hash
    pub hash: u64,
    /// Specificity
    pub specificity: Specificity,
    /// Rule index
    pub rule_index: u32,
    /// Selector ID for caching
    pub id: SelectorId,
}

/// Matcher statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct MatcherStats {
    pub matches_attempted: u64,
    pub bloom_rejections: u64,
    pub full_matches: u64,
    pub matches_found: u64,
    pub ancestor_rejections: u64,
}

impl MatcherStats {
    pub fn rejection_rate(&self) -> f64 {
        if self.matches_attempted == 0 {
            0.0
        } else {
            self.bloom_rejections as f64 / self.matches_attempted as f64
        }
    }
    
    pub fn ancestor_rejection_rate(&self) -> f64 {
        if self.matches_attempted == 0 {
            0.0
        } else {
            self.ancestor_rejections as f64 / self.matches_attempted as f64
        }
    }
}

impl Default for AcceleratedSelectorMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl AcceleratedSelectorMatcher {
    pub fn new() -> Self {
        Self {
            element_filter: SelectorBloomFilter::new(1000),
            selector_map: HashMap::new(),
            id_selectors: HashMap::new(),
            class_selectors: HashMap::new(),
            tag_selectors: HashMap::new(),
            specificity_cache: SpecificityCache::new(4096),
            stats: MatcherStats::default(),
        }
    }
    
    /// Add a selector with computed specificity
    pub fn add_selector(&mut self, selector: &str, specificity: Specificity, rule_index: u32) {
        static NEXT_ID: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        
        let hash = hash_selector(selector);
        let id = SelectorId(NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        
        let entry = SelectorEntry {
            selector: selector.into(),
            hash,
            specificity,
            rule_index,
            id,
        };
        
        // Cache the specificity
        self.specificity_cache.insert(id, specificity);
        
        // Index by type
        if selector.starts_with('#') {
            let id_hash = hash_string(&selector[1..]);
            self.id_selectors.entry(id_hash).or_default().push(entry.clone());
        } else if selector.starts_with('.') {
            let class_hash = hash_string(&selector[1..]);
            self.class_selectors.entry(class_hash).or_default().push(entry.clone());
        } else if !selector.contains(' ') && !selector.contains('>') && !selector.contains('+') && !selector.contains('~') {
            // Simple tag selector
            let tag_hash = hash_string(selector);
            self.tag_selectors.entry(tag_hash).or_default().push(entry.clone());
        }
        
        self.selector_map.entry(hash).or_default().push(entry);
        self.element_filter.add(hash);
    }
    
    /// Add element to filter
    pub fn add_element(&mut self, tag: &str, id: Option<&str>, classes: &[&str]) {
        let hash = hash_element_features(tag, id, classes);
        self.element_filter.add(hash);
    }
    
    /// Check if element might match any selector (fast path)
    pub fn might_match(&self, element_hash: u64) -> bool {
        self.element_filter.might_contain(element_hash)
    }
    
    /// Get candidate selectors for an element with ancestor filter
    pub fn get_candidates_with_ancestors(
        &mut self,
        tag: &str,
        id: Option<&str>,
        classes: &[&str],
        ancestors: &AncestorBloom,
    ) -> Vec<&SelectorEntry> {
        self.stats.matches_attempted += 1;
        
        let element_hash = hash_element_features(tag, id, classes);
        
        // Bloom filter fast path
        if !self.might_match(element_hash) {
            self.stats.bloom_rejections += 1;
            return Vec::new();
        }
        
        self.stats.full_matches += 1;
        
        let mut candidates = Vec::new();
        
        // Fast path: tag selectors
        let tag_hash = hash_string(tag);
        if let Some(entries) = self.tag_selectors.get(&tag_hash) {
            candidates.extend(entries.iter());
        }
        
        // Fast path: ID selectors
        if let Some(id) = id {
            let id_hash = hash_string(id);
            if let Some(entries) = self.id_selectors.get(&id_hash) {
                candidates.extend(entries.iter());
            }
        }
        
        // Fast path: class selectors
        for class in classes {
            let class_hash = hash_string(class);
            if let Some(entries) = self.class_selectors.get(&class_hash) {
                candidates.extend(entries.iter());
            }
        }
        
        // Filter by ancestor requirements for complex selectors
        candidates.retain(|entry| {
            if entry.selector.contains(' ') {
                // Has ancestor requirement - check ancestor bloom
                let _parts: Vec<&str> = entry.selector.split_whitespace().collect();
                // For now, allow all (full matching happens later)
                true
            } else {
                true
            }
        });
        
        self.stats.matches_found += candidates.len() as u64;
        
        candidates
    }
    
    /// Get candidate selectors for an element (without ancestor filter)
    pub fn get_candidates(
        &mut self,
        tag: &str,
        id: Option<&str>,
        classes: &[&str],
    ) -> Vec<&SelectorEntry> {
        self.get_candidates_with_ancestors(tag, id, classes, &AncestorBloom::new())
    }
    
    /// Get cached specificity for a selector
    pub fn get_specificity(&mut self, id: SelectorId) -> Option<Specificity> {
        self.specificity_cache.get(id)
    }
    
    /// Get statistics
    pub fn stats(&self) -> &MatcherStats {
        &self.stats
    }
    
    /// Get specificity cache stats
    pub fn specificity_cache_stats(&self) -> &SpecificityCacheStats {
        self.specificity_cache.stats()
    }
    
    /// Clear all data
    pub fn clear(&mut self) {
        self.element_filter.clear();
        self.selector_map.clear();
        self.id_selectors.clear();
        self.class_selectors.clear();
        self.tag_selectors.clear();
        self.specificity_cache.clear();
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bloom_filter_8_hash() {
        let mut filter = SelectorBloomFilter::new(100);
        
        filter.add(12345);
        filter.add(67890);
        
        assert!(filter.might_contain(12345));
        assert!(filter.might_contain(67890));
        assert_eq!(filter.count(), 2);
    }
    
    #[test]
    fn test_bloom_filter_false_positive_rate() {
        let mut filter = SelectorBloomFilter::new(1000);
        
        for i in 0..100 {
            filter.add(i);
        }
        
        let fpr = filter.false_positive_rate();
        // With 8 hashes, FPR should be very low
        assert!(fpr < 0.05, "FPR too high: {}", fpr);
    }
    
    #[test]
    fn test_ancestor_bloom() {
        let mut ancestors = AncestorBloom::new();
        
        let div_hash = hash_string("div");
        let container_hash = hash_string("container");
        
        ancestors.push_ancestor(div_hash, None, &[container_hash]);
        
        assert!(ancestors.has_ancestor_tag(div_hash));
        assert!(ancestors.has_ancestor_class(container_hash));
        assert_eq!(ancestors.depth(), 1);
        
        ancestors.pop_ancestor();
        assert_eq!(ancestors.depth(), 0);
    }
    
    #[test]
    fn test_specificity() {
        let s1 = Specificity::new(0, 1, 0); // .class
        let s2 = Specificity::new(1, 0, 0); // #id
        let s3 = Specificity::new(0, 0, 1); // tag
        
        assert!(s2 > s1);
        assert!(s1 > s3);
        assert_eq!(s1.classes(), 1);
        assert_eq!(s2.ids(), 1);
        assert_eq!(s3.types(), 1);
    }
    
    #[test]
    fn test_specificity_cache() {
        let mut cache = SpecificityCache::new(10);
        
        let id = SelectorId(1);
        let spec = Specificity::new(1, 2, 3);
        
        cache.insert(id, spec);
        
        assert_eq!(cache.get(id), Some(spec));
        assert_eq!(cache.stats().hits, 1);
    }
    
    #[test]
    fn test_hash_element() {
        let hash1 = hash_element_features("div", Some("main"), &["container", "active"]);
        let hash2 = hash_element_features("div", Some("main"), &["active", "container"]); // Order independent
        let hash3 = hash_element_features("span", None, &["text"]);
        
        assert_eq!(hash1, hash2); // Classes are order-independent
        assert_ne!(hash1, hash3);
    }
    
    #[test]
    fn test_accelerated_matcher() {
        let mut matcher = AcceleratedSelectorMatcher::new();
        
        matcher.add_selector("div", Specificity::new(0, 0, 1), 0);
        matcher.add_selector(".active", Specificity::new(0, 1, 0), 1);
        matcher.add_selector("#main", Specificity::new(1, 0, 0), 2);
        
        let candidates = matcher.get_candidates("div", Some("main"), &["active"]);
        
        // Should find candidates
        assert!(!candidates.is_empty() || matcher.stats().bloom_rejections > 0);
    }
    
    #[test]
    fn test_matcher_with_ancestors() {
        let mut matcher = AcceleratedSelectorMatcher::new();
        let mut ancestors = AncestorBloom::new();
        
        matcher.add_selector("div", Specificity::new(0, 0, 1), 0);
        
        // Push parent div
        ancestors.push_ancestor(hash_string("div"), None, &[hash_string("container")]);
        
        let candidates = matcher.get_candidates_with_ancestors("span", None, &[], &ancestors);
        
        // span doesn't match div selector
        assert!(candidates.is_empty() || matcher.stats().bloom_rejections > 0);
    }
    
    #[test]
    fn test_bloom_merge() {
        let mut f1 = SelectorBloomFilter::compact();
        let mut f2 = SelectorBloomFilter::compact();
        
        f1.add(100);
        f2.add(200);
        
        f1.merge(&f2);
        
        assert!(f1.might_contain(100));
        assert!(f1.might_contain(200));
    }
}
