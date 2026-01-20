//! DOM Query Index
//!
//! Bitmap-based indexing for O(1) element lookups by id, class, and tag.
//! Custom BitSet implementation to avoid external dependencies.

use std::collections::HashMap;
use crate::NodeId;

/// Compact bitset for node ID tracking
#[derive(Debug, Clone, Default)]
pub struct BitSet {
    /// Bit storage (64 bits per word)
    words: Vec<u64>,
}

impl BitSet {
    const BITS_PER_WORD: usize = 64;

    /// Create a new empty bitset
    pub fn new() -> Self {
        Self { words: Vec::new() }
    }

    /// Create with capacity for n bits
    pub fn with_capacity(n: usize) -> Self {
        let num_words = (n + Self::BITS_PER_WORD - 1) / Self::BITS_PER_WORD;
        Self {
            words: vec![0; num_words],
        }
    }

    /// Set bit at index
    pub fn insert(&mut self, index: u32) {
        let word_idx = index as usize / Self::BITS_PER_WORD;
        let bit_idx = index as usize % Self::BITS_PER_WORD;

        // Ensure capacity
        if word_idx >= self.words.len() {
            self.words.resize(word_idx + 1, 0);
        }

        self.words[word_idx] |= 1 << bit_idx;
    }

    /// Clear bit at index
    pub fn remove(&mut self, index: u32) {
        let word_idx = index as usize / Self::BITS_PER_WORD;
        let bit_idx = index as usize % Self::BITS_PER_WORD;

        if word_idx < self.words.len() {
            self.words[word_idx] &= !(1 << bit_idx);
        }
    }

    /// Check if bit is set
    pub fn contains(&self, index: u32) -> bool {
        let word_idx = index as usize / Self::BITS_PER_WORD;
        let bit_idx = index as usize % Self::BITS_PER_WORD;

        if word_idx >= self.words.len() {
            return false;
        }

        (self.words[word_idx] & (1 << bit_idx)) != 0
    }

    /// Count set bits (population count)
    pub fn count(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Clear all bits
    pub fn clear(&mut self) {
        for word in &mut self.words {
            *word = 0;
        }
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&w| w == 0)
    }

    /// Intersection with another bitset
    pub fn intersect(&self, other: &BitSet) -> BitSet {
        let min_len = self.words.len().min(other.words.len());
        let mut result = Vec::with_capacity(min_len);

        for i in 0..min_len {
            result.push(self.words[i] & other.words[i]);
        }

        BitSet { words: result }
    }

    /// Union with another bitset
    pub fn union(&self, other: &BitSet) -> BitSet {
        let max_len = self.words.len().max(other.words.len());
        let mut result = vec![0u64; max_len];

        for (i, word) in self.words.iter().enumerate() {
            result[i] |= word;
        }
        for (i, word) in other.words.iter().enumerate() {
            result[i] |= word;
        }

        BitSet { words: result }
    }

    /// Iterate over set bits
    pub fn iter(&self) -> BitSetIter<'_> {
        BitSetIter {
            bitset: self,
            word_idx: 0,
            bit_idx: 0,
        }
    }

    /// Get first set bit
    pub fn first(&self) -> Option<u32> {
        for (word_idx, &word) in self.words.iter().enumerate() {
            if word != 0 {
                let bit_idx = word.trailing_zeros() as usize;
                return Some((word_idx * Self::BITS_PER_WORD + bit_idx) as u32);
            }
        }
        None
    }

    /// Memory usage in bytes
    pub fn memory_size(&self) -> usize {
        self.words.len() * 8
    }
}

/// Iterator over set bits
pub struct BitSetIter<'a> {
    bitset: &'a BitSet,
    word_idx: usize,
    bit_idx: usize,
}

impl Iterator for BitSetIter<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        while self.word_idx < self.bitset.words.len() {
            let word = self.bitset.words[self.word_idx];

            // Skip to next set bit in current word
            while self.bit_idx < BitSet::BITS_PER_WORD {
                if (word & (1 << self.bit_idx)) != 0 {
                    let result = (self.word_idx * BitSet::BITS_PER_WORD + self.bit_idx) as u32;
                    self.bit_idx += 1;
                    return Some(result);
                }
                self.bit_idx += 1;
            }

            // Move to next word
            self.word_idx += 1;
            self.bit_idx = 0;
        }

        None
    }
}

/// Query index for fast element lookups
#[derive(Debug, Default)]
pub struct QueryIndex {
    /// Index by element ID (id attribute)
    by_id: HashMap<u32, NodeId>,
    /// Index by class name
    by_class: HashMap<u32, BitSet>,
    /// Index by tag name
    by_tag: HashMap<u32, BitSet>,
    /// Dirty flag - needs rebuild
    dirty: bool,
    /// Statistics
    stats: QueryIndexStats,
}

/// Query index statistics
#[derive(Debug, Clone, Default)]
pub struct QueryIndexStats {
    pub id_entries: usize,
    pub class_entries: usize,
    pub tag_entries: usize,
    pub total_memory: usize,
    pub hits: u64,
    pub misses: u64,
}

impl QueryIndex {
    /// Create a new query index
    pub fn new() -> Self {
        Self::default()
    }

    /// Index an element by ID
    pub fn index_id(&mut self, id_hash: u32, node: NodeId) {
        self.by_id.insert(id_hash, node);
        self.stats.id_entries = self.by_id.len();
    }

    /// Index an element by class
    pub fn index_class(&mut self, class_hash: u32, node: NodeId) {
        self.by_class
            .entry(class_hash)
            .or_insert_with(BitSet::new)
            .insert(node.0);
        self.update_class_stats();
    }

    /// Index an element by tag
    pub fn index_tag(&mut self, tag_hash: u32, node: NodeId) {
        self.by_tag
            .entry(tag_hash)
            .or_insert_with(BitSet::new)
            .insert(node.0);
        self.update_tag_stats();
    }

    /// Remove element from ID index
    pub fn remove_id(&mut self, id_hash: u32) {
        self.by_id.remove(&id_hash);
        self.stats.id_entries = self.by_id.len();
    }

    /// Remove element from class index
    pub fn remove_class(&mut self, class_hash: u32, node: NodeId) {
        if let Some(bitset) = self.by_class.get_mut(&class_hash) {
            bitset.remove(node.0);
            if bitset.is_empty() {
                self.by_class.remove(&class_hash);
            }
        }
        self.update_class_stats();
    }

    /// Remove element from tag index
    pub fn remove_tag(&mut self, tag_hash: u32, node: NodeId) {
        if let Some(bitset) = self.by_tag.get_mut(&tag_hash) {
            bitset.remove(node.0);
            if bitset.is_empty() {
                self.by_tag.remove(&tag_hash);
            }
        }
        self.update_tag_stats();
    }

    /// Lookup by ID - O(1)
    pub fn get_by_id(&mut self, id_hash: u32) -> Option<NodeId> {
        match self.by_id.get(&id_hash) {
            Some(&node) => {
                self.stats.hits += 1;
                Some(node)
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// Lookup by class - O(1)
    pub fn get_by_class(&mut self, class_hash: u32) -> Option<&BitSet> {
        match self.by_class.get(&class_hash) {
            Some(bitset) => {
                self.stats.hits += 1;
                Some(bitset)
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// Lookup by tag - O(1)
    pub fn get_by_tag(&mut self, tag_hash: u32) -> Option<&BitSet> {
        match self.by_tag.get(&tag_hash) {
            Some(bitset) => {
                self.stats.hits += 1;
                Some(bitset)
            }
            None => {
                self.stats.misses += 1;
                None
            }
        }
    }

    /// Get first element with class
    pub fn first_with_class(&mut self, class_hash: u32) -> Option<NodeId> {
        self.get_by_class(class_hash)
            .and_then(|bs| bs.first())
            .map(NodeId)
    }

    /// Get first element with tag
    pub fn first_with_tag(&mut self, tag_hash: u32) -> Option<NodeId> {
        self.get_by_tag(tag_hash)
            .and_then(|bs| bs.first())
            .map(NodeId)
    }

    /// Query with intersection (e.g., elements with class A AND class B)
    pub fn intersect_classes(&self, class_hashes: &[u32]) -> BitSet {
        let mut result: Option<BitSet> = None;

        for &hash in class_hashes {
            if let Some(bitset) = self.by_class.get(&hash) {
                result = Some(match result {
                    Some(r) => r.intersect(bitset),
                    None => bitset.clone(),
                });
            } else {
                // Class not found, intersection is empty
                return BitSet::new();
            }
        }

        result.unwrap_or_default()
    }

    /// Query with intersection of tag and class
    pub fn query_tag_and_class(&self, tag_hash: u32, class_hash: u32) -> BitSet {
        let tag_set = self.by_tag.get(&tag_hash);
        let class_set = self.by_class.get(&class_hash);

        match (tag_set, class_set) {
            (Some(t), Some(c)) => t.intersect(c),
            _ => BitSet::new(),
        }
    }

    /// Mark index as dirty (needs rebuild)
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Check if index needs rebuilding
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark index as clean
    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }

    /// Clear all indices
    pub fn clear(&mut self) {
        self.by_id.clear();
        self.by_class.clear();
        self.by_tag.clear();
        self.dirty = true;
        self.stats = QueryIndexStats::default();
    }

    /// Get statistics
    pub fn stats(&self) -> &QueryIndexStats {
        &self.stats
    }

    fn update_class_stats(&mut self) {
        self.stats.class_entries = self.by_class.values().map(|bs| bs.count()).sum();
        self.update_memory_stats();
    }

    fn update_tag_stats(&mut self) {
        self.stats.tag_entries = self.by_tag.values().map(|bs| bs.count()).sum();
        self.update_memory_stats();
    }

    fn update_memory_stats(&mut self) {
        let id_mem = self.by_id.len() * (4 + 4); // hash + NodeId
        let class_mem: usize = self.by_class.values().map(|bs| bs.memory_size()).sum();
        let tag_mem: usize = self.by_tag.values().map(|bs| bs.memory_size()).sum();
        self.stats.total_memory = id_mem + class_mem + tag_mem;
    }
}

/// Simple string hasher for consistent hashing
pub fn hash_string(s: &str) -> u32 {
    let mut hash: u32 = 5381;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(byte as u32);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitset_basic() {
        let mut bs = BitSet::new();
        
        bs.insert(0);
        bs.insert(5);
        bs.insert(64);
        bs.insert(100);

        assert!(bs.contains(0));
        assert!(bs.contains(5));
        assert!(bs.contains(64));
        assert!(bs.contains(100));
        assert!(!bs.contains(1));
        assert!(!bs.contains(65));

        assert_eq!(bs.count(), 4);
    }

    #[test]
    fn test_bitset_remove() {
        let mut bs = BitSet::new();
        bs.insert(10);
        bs.insert(20);
        
        assert!(bs.contains(10));
        bs.remove(10);
        assert!(!bs.contains(10));
        assert!(bs.contains(20));
    }

    #[test]
    fn test_bitset_iter() {
        let mut bs = BitSet::new();
        bs.insert(1);
        bs.insert(3);
        bs.insert(7);
        bs.insert(65);

        let bits: Vec<_> = bs.iter().collect();
        assert_eq!(bits, vec![1, 3, 7, 65]);
    }

    #[test]
    fn test_bitset_intersect() {
        let mut a = BitSet::new();
        a.insert(1);
        a.insert(2);
        a.insert(3);

        let mut b = BitSet::new();
        b.insert(2);
        b.insert(3);
        b.insert(4);

        let result = a.intersect(&b);
        assert!(!result.contains(1));
        assert!(result.contains(2));
        assert!(result.contains(3));
        assert!(!result.contains(4));
    }

    #[test]
    fn test_bitset_union() {
        let mut a = BitSet::new();
        a.insert(1);
        a.insert(2);

        let mut b = BitSet::new();
        b.insert(3);
        b.insert(4);

        let result = a.union(&b);
        assert!(result.contains(1));
        assert!(result.contains(2));
        assert!(result.contains(3));
        assert!(result.contains(4));
    }

    #[test]
    fn test_query_index_id() {
        let mut index = QueryIndex::new();
        
        let id_hash = hash_string("my-element");
        index.index_id(id_hash, NodeId(42));

        assert_eq!(index.get_by_id(id_hash), Some(NodeId(42)));
        assert_eq!(index.get_by_id(hash_string("other")), None);
    }

    #[test]
    fn test_query_index_class() {
        let mut index = QueryIndex::new();
        
        let class_hash = hash_string("container");
        index.index_class(class_hash, NodeId(1));
        index.index_class(class_hash, NodeId(5));
        index.index_class(class_hash, NodeId(10));

        let result = index.get_by_class(class_hash).unwrap();
        assert!(result.contains(1));
        assert!(result.contains(5));
        assert!(result.contains(10));
        assert!(!result.contains(2));
    }

    #[test]
    fn test_query_index_tag() {
        let mut index = QueryIndex::new();
        
        let div_hash = hash_string("div");
        index.index_tag(div_hash, NodeId(1));
        index.index_tag(div_hash, NodeId(2));
        index.index_tag(div_hash, NodeId(3));

        let result = index.get_by_tag(div_hash).unwrap();
        assert_eq!(result.count(), 3);
    }

    #[test]
    fn test_query_intersection() {
        let mut index = QueryIndex::new();
        
        let class_a = hash_string("active");
        let class_b = hash_string("visible");

        // Nodes 1, 2, 3 have class 'active'
        index.index_class(class_a, NodeId(1));
        index.index_class(class_a, NodeId(2));
        index.index_class(class_a, NodeId(3));

        // Nodes 2, 3, 4 have class 'visible'
        index.index_class(class_b, NodeId(2));
        index.index_class(class_b, NodeId(3));
        index.index_class(class_b, NodeId(4));

        // Intersection should be nodes 2, 3
        let result = index.intersect_classes(&[class_a, class_b]);
        assert!(!result.contains(1));
        assert!(result.contains(2));
        assert!(result.contains(3));
        assert!(!result.contains(4));
    }

    #[test]
    fn test_first_with_class() {
        let mut index = QueryIndex::new();
        
        let class_hash = hash_string("item");
        index.index_class(class_hash, NodeId(5));
        index.index_class(class_hash, NodeId(10));
        index.index_class(class_hash, NodeId(3));

        let first = index.first_with_class(class_hash);
        assert!(first.is_some());
        // First should be the lowest ID (3)
        assert_eq!(first.unwrap(), NodeId(3));
    }

    #[test]
    fn test_stats() {
        let mut index = QueryIndex::new();
        
        index.index_id(hash_string("id1"), NodeId(1));
        index.index_class(hash_string("class1"), NodeId(1));
        index.index_class(hash_string("class1"), NodeId(2));
        index.index_tag(hash_string("div"), NodeId(1));

        let stats = index.stats();
        assert_eq!(stats.id_entries, 1);
        assert_eq!(stats.class_entries, 2);
        assert_eq!(stats.tag_entries, 1);
    }
}
