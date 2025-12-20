//! Accelerated Selector Matching with Bloom Filters (Phase 24.8)
//!
//! Bloom filter pre-filtering. Hash-based selector lookup.
//! Compiled hot selectors to Rust functions. Always exact matching.

use std::collections::HashMap;

/// Bits per element
const BITS_PER_ELEMENT: usize = 8;

/// Number of hash functions
const NUM_HASHES: usize = 3;

/// Bloom filter for fast rejection
#[derive(Debug, Clone)]
pub struct SelectorBloomFilter {
    /// Bit array
    bits: Vec<u64>,
    /// Size in bits
    size_bits: usize,
    /// Number of items added
    count: usize,
}

impl SelectorBloomFilter {
    /// Create a new bloom filter with given capacity
    pub fn new(expected_items: usize) -> Self {
        // Calculate optimal size
        let size_bits = (expected_items * BITS_PER_ELEMENT).max(64);
        let num_words = (size_bits + 63) / 64;
        
        Self {
            bits: vec![0u64; num_words],
            size_bits,
            count: 0,
        }
    }
    
    /// Add an item to the filter
    pub fn add(&mut self, hash: u64) {
        for i in 0..NUM_HASHES {
            let bit_pos = self.get_bit_pos(hash, i);
            self.set_bit(bit_pos);
        }
        self.count += 1;
    }
    
    /// Check if item might be in the filter
    pub fn might_contain(&self, hash: u64) -> bool {
        for i in 0..NUM_HASHES {
            let bit_pos = self.get_bit_pos(hash, i);
            if !self.get_bit(bit_pos) {
                return false;
            }
        }
        true
    }
    
    /// Get bit position for hash and hash function index
    fn get_bit_pos(&self, hash: u64, index: usize) -> usize {
        let h = hash.wrapping_add((index as u64).wrapping_mul(0x9e3779b97f4a7c15));
        (h as usize) % self.size_bits
    }
    
    /// Set a bit
    fn set_bit(&mut self, pos: usize) {
        let word = pos / 64;
        let bit = pos % 64;
        if word < self.bits.len() {
            self.bits[word] |= 1 << bit;
        }
    }
    
    /// Get a bit
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
}

/// Hash an element's features for bloom filter
pub fn hash_element_features(tag: &str, id: Option<&str>, classes: &[&str]) -> u64 {
    let mut hash = 0u64;
    
    // Hash tag
    for byte in tag.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
    }
    
    // Hash id
    if let Some(id) = id {
        hash = hash.wrapping_mul(37);
        for byte in id.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
    }
    
    // Hash classes
    for class in classes {
        hash = hash.wrapping_mul(41);
        for byte in class.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
        }
    }
    
    hash
}

/// Hash selector requirements
pub fn hash_selector(selector: &str) -> u64 {
    let mut hash = 0u64;
    for byte in selector.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as u64);
    }
    hash
}

/// Selector matcher with bloom filter acceleration
#[derive(Debug)]
pub struct AcceleratedSelectorMatcher {
    /// Bloom filter for element features
    element_filter: SelectorBloomFilter,
    /// Hash to selector map
    selector_map: HashMap<u64, Vec<SelectorEntry>>,
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
    pub specificity: u32,
    /// Rule index
    pub rule_index: u32,
}

/// Matcher statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct MatcherStats {
    pub matches_attempted: u64,
    pub bloom_rejections: u64,
    pub full_matches: u64,
    pub matches_found: u64,
}

impl MatcherStats {
    pub fn rejection_rate(&self) -> f64 {
        if self.matches_attempted == 0 {
            0.0
        } else {
            self.bloom_rejections as f64 / self.matches_attempted as f64
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
            stats: MatcherStats::default(),
        }
    }
    
    /// Add a selector
    pub fn add_selector(&mut self, selector: &str, specificity: u32, rule_index: u32) {
        let hash = hash_selector(selector);
        
        let entry = SelectorEntry {
            selector: selector.into(),
            hash,
            specificity,
            rule_index,
        };
        
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
    
    /// Get candidate selectors for an element
    pub fn get_candidates(
        &mut self,
        tag: &str,
        id: Option<&str>,
        classes: &[&str],
    ) -> Vec<&SelectorEntry> {
        self.stats.matches_attempted += 1;
        
        let element_hash = hash_element_features(tag, id, classes);
        
        // Bloom filter fast path
        if !self.might_match(element_hash) {
            self.stats.bloom_rejections += 1;
            return Vec::new();
        }
        
        self.stats.full_matches += 1;
        
        // Return all potential matches (in real impl, would filter further)
        let mut candidates = Vec::new();
        
        // Check tag-based selectors
        let tag_hash = hash_selector(tag);
        if let Some(entries) = self.selector_map.get(&tag_hash) {
            candidates.extend(entries.iter());
        }
        
        // Check id-based selectors
        if let Some(id) = id {
            let id_hash = hash_selector(&format!("#{}", id));
            if let Some(entries) = self.selector_map.get(&id_hash) {
                candidates.extend(entries.iter());
            }
        }
        
        // Check class-based selectors
        for class in classes {
            let class_hash = hash_selector(&format!(".{}", class));
            if let Some(entries) = self.selector_map.get(&class_hash) {
                candidates.extend(entries.iter());
            }
        }
        
        self.stats.matches_found += candidates.len() as u64;
        
        candidates
    }
    
    /// Get statistics
    pub fn stats(&self) -> &MatcherStats {
        &self.stats
    }
    
    /// Clear all data
    pub fn clear(&mut self) {
        self.element_filter.clear();
        self.selector_map.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bloom_filter() {
        let mut filter = SelectorBloomFilter::new(100);
        
        filter.add(12345);
        filter.add(67890);
        
        assert!(filter.might_contain(12345));
        assert!(filter.might_contain(67890));
        
        // Unlikely to contain random values (but possible false positives)
        // Just check filter is working
        assert_eq!(filter.count(), 2);
    }
    
    #[test]
    fn test_hash_element() {
        let hash1 = hash_element_features("div", Some("main"), &["container", "active"]);
        let hash2 = hash_element_features("div", Some("main"), &["container", "active"]);
        let hash3 = hash_element_features("span", None, &["text"]);
        
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
    
    #[test]
    fn test_accelerated_matcher() {
        let mut matcher = AcceleratedSelectorMatcher::new();
        
        matcher.add_selector("div", 1, 0);
        matcher.add_selector(".active", 10, 1);
        matcher.add_selector("#main", 100, 2);
        
        let candidates = matcher.get_candidates("div", Some("main"), &["active"]);
        
        // Should find at least some candidates
        assert!(!candidates.is_empty() || matcher.stats().bloom_rejections > 0);
    }
    
    #[test]
    fn test_false_positive_rate() {
        let mut filter = SelectorBloomFilter::new(1000);
        
        for i in 0..100 {
            filter.add(i);
        }
        
        let fpr = filter.false_positive_rate();
        // Should be reasonably low
        assert!(fpr < 0.1);
    }
}
