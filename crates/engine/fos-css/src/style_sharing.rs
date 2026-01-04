//! Style Sharing
//!
//! Share computed styles between elements with identical styling.
//! Many elements match the same selectors and have the same inherited styles.
//! Sharing reduces memory and computation significantly.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Style sharing cache
#[derive(Debug)]
pub struct StyleSharingCache {
    /// Cache entries by key
    cache: HashMap<StyleKey, SharedStyleRef>,
    /// LRU order tracking
    lru_order: Vec<StyleKey>,
    /// Maximum cache size
    max_size: usize,
    /// Statistics
    stats: SharingStats,
}

/// Key for matching shareable styles
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StyleKey {
    /// Hash of matching rule indices
    rules_hash: u64,
    /// Hash of inherited property values
    inherited_hash: u64,
    /// Parent style key (if any)
    parent_key: Option<Box<StyleKey>>,
}

/// Reference to shared style
#[derive(Debug, Clone)]
pub struct SharedStyleRef {
    /// Reference-counted style data
    inner: Arc<SharedStyleData>,
}

/// Shared style data
#[derive(Debug)]
struct SharedStyleData {
    /// The computed property values (as bytes for generic storage)
    properties: Vec<u8>,
    /// Reference count (atomic)
    ref_count: std::sync::atomic::AtomicUsize,
    /// Creation time for LRU
    created_at: u64,
}

/// Style sharing statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct SharingStats {
    /// Cache hits
    pub hits: usize,
    /// Cache misses
    pub misses: usize,
    /// Total lookups
    pub lookups: usize,
    /// Styles shared
    pub shared: usize,
    /// Unique styles
    pub unique: usize,
    /// Memory saved (bytes)
    pub memory_saved: usize,
}

impl SharingStats {
    /// Hit rate
    pub fn hit_rate(&self) -> f64 {
        if self.lookups == 0 {
            0.0
        } else {
            self.hits as f64 / self.lookups as f64
        }
    }
    
    /// Sharing rate
    pub fn sharing_rate(&self) -> f64 {
        let total = self.shared + self.unique;
        if total == 0 {
            0.0
        } else {
            self.shared as f64 / total as f64
        }
    }
}

impl Default for StyleSharingCache {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleSharingCache {
    /// Create new cache with default size
    pub fn new() -> Self {
        Self::with_capacity(1024)
    }
    
    /// Create new cache with specific capacity
    pub fn with_capacity(max_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_size),
            lru_order: Vec::with_capacity(max_size),
            max_size,
            stats: SharingStats::default(),
        }
    }
    
    /// Look up a style by key
    pub fn get(&mut self, key: &StyleKey) -> Option<SharedStyleRef> {
        self.stats.lookups += 1;
        
        let result = self.cache.get(key).cloned();
        
        if result.is_some() {
            self.stats.hits += 1;
            self.touch(key);
        } else {
            self.stats.misses += 1;
        }
        
        result
    }
    
    /// Insert a style
    pub fn insert(&mut self, key: StyleKey, properties: Vec<u8>) -> SharedStyleRef {
        // Evict if needed
        while self.cache.len() >= self.max_size {
            self.evict_oldest();
        }
        
        let style = SharedStyleRef {
            inner: Arc::new(SharedStyleData {
                properties,
                ref_count: std::sync::atomic::AtomicUsize::new(1),
                created_at: self.stats.lookups as u64,
            }),
        };
        
        self.cache.insert(key.clone(), style.clone());
        self.lru_order.push(key);
        self.stats.unique += 1;
        
        style
    }
    
    /// Get or insert style
    pub fn get_or_insert<F>(&mut self, key: StyleKey, f: F) -> SharedStyleRef
    where
        F: FnOnce() -> Vec<u8>,
    {
        if let Some(style) = self.get(&key) {
            self.stats.shared += 1;
            self.stats.memory_saved += style.inner.properties.len();
            style
        } else {
            let properties = f();
            self.insert(key, properties)
        }
    }
    
    /// Clear cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.lru_order.clear();
    }
    
    /// Get statistics
    pub fn stats(&self) -> &SharingStats {
        &self.stats
    }
    
    /// Cache size
    pub fn len(&self) -> usize {
        self.cache.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
    
    fn touch(&mut self, key: &StyleKey) {
        // Move to end of LRU order
        if let Some(pos) = self.lru_order.iter().position(|k| k == key) {
            let k = self.lru_order.remove(pos);
            self.lru_order.push(k);
        }
    }
    
    fn evict_oldest(&mut self) {
        if let Some(key) = self.lru_order.first().cloned() {
            self.cache.remove(&key);
            self.lru_order.remove(0);
        }
    }
}

impl SharedStyleRef {
    /// Get property data
    pub fn properties(&self) -> &[u8] {
        &self.inner.properties
    }
    
    /// Get reference count
    pub fn ref_count(&self) -> usize {
        self.inner.ref_count.load(std::sync::atomic::Ordering::Relaxed)
    }
    
    /// Check if this is the only reference
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }
}

impl StyleKey {
    /// Create new style key
    pub fn new(rules_hash: u64, inherited_hash: u64) -> Self {
        Self {
            rules_hash,
            inherited_hash,
            parent_key: None,
        }
    }
    
    /// Create with parent key
    pub fn with_parent(rules_hash: u64, inherited_hash: u64, parent: StyleKey) -> Self {
        Self {
            rules_hash,
            inherited_hash,
            parent_key: Some(Box::new(parent)),
        }
    }
}

/// Trait for elements that can share styles
pub trait StyleSharable {
    /// Check if can share style with another element
    fn can_share_with(&self, other: &Self) -> bool;
    
    /// Get style sharing key
    fn style_key(&self) -> StyleKey;
}

/// Bloom filter for quick rejection of non-matching elements
#[derive(Debug, Clone)]
pub struct StyleBloomKey {
    /// Bloom filter bits
    bits: [u64; 4],
}

impl Default for StyleBloomKey {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleBloomKey {
    /// Create empty bloom key
    pub fn new() -> Self {
        Self { bits: [0; 4] }
    }
    
    /// Add a hash value
    pub fn add(&mut self, hash: u64) {
        let h1 = hash;
        let h2 = hash.wrapping_mul(0x9e3779b97f4a7c15);
        
        for i in 0..4 {
            let h = h1.wrapping_add(h2.wrapping_mul(i as u64));
            let bit = h % 256;
            let word = (bit / 64) as usize;
            let pos = bit % 64;
            self.bits[word] |= 1 << pos;
        }
    }
    
    /// Check if might contain hash
    pub fn might_contain(&self, hash: u64) -> bool {
        let h1 = hash;
        let h2 = hash.wrapping_mul(0x9e3779b97f4a7c15);
        
        for i in 0..4 {
            let h = h1.wrapping_add(h2.wrapping_mul(i as u64));
            let bit = h % 256;
            let word = (bit / 64) as usize;
            let pos = bit % 64;
            if self.bits[word] & (1 << pos) == 0 {
                return false;
            }
        }
        true
    }
    
    /// Check if this bloom key is subset of another
    pub fn is_subset_of(&self, other: &StyleBloomKey) -> bool {
        for i in 0..4 {
            if self.bits[i] & !other.bits[i] != 0 {
                return false;
            }
        }
        true
    }
    
    /// Merge with another bloom key
    pub fn merge(&mut self, other: &StyleBloomKey) {
        for i in 0..4 {
            self.bits[i] |= other.bits[i];
        }
    }
    
    /// Count bits set
    pub fn count(&self) -> u32 {
        self.bits.iter().map(|b| b.count_ones()).sum()
    }
}

/// Hash helper for style key computation
pub struct StyleHasher {
    state: u64,
}

impl Default for StyleHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleHasher {
    /// Create new hasher
    pub fn new() -> Self {
        Self { state: 0xcbf29ce484222325 } // FNV offset basis
    }
    
    /// Add bytes to hash
    pub fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= *byte as u64;
            self.state = self.state.wrapping_mul(0x100000001b3); // FNV prime
        }
    }
    
    /// Add u64 to hash
    pub fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }
    
    /// Add u32 to hash
    pub fn write_u32(&mut self, value: u32) {
        self.write(&value.to_le_bytes());
    }
    
    /// Add string to hash
    pub fn write_str(&mut self, s: &str) {
        self.write(s.as_bytes());
    }
    
    /// Finish and get hash
    pub fn finish(&self) -> u64 {
        self.state
    }
}

/// Element context for style sharing checks
#[derive(Debug, Clone)]
pub struct ElementContext {
    /// Tag name hash
    pub tag_hash: u64,
    /// Class list hash
    pub classes_hash: u64,
    /// ID hash
    pub id_hash: u64,
    /// Attribute hashes
    pub attrs_hash: u64,
    /// Pseudo-class state
    pub pseudo_state: u32,
    /// Parent context
    pub parent: Option<Box<ElementContext>>,
}

impl ElementContext {
    /// Create new context
    pub fn new(tag: &str) -> Self {
        let mut hasher = StyleHasher::new();
        hasher.write_str(tag);
        
        Self {
            tag_hash: hasher.finish(),
            classes_hash: 0,
            id_hash: 0,
            attrs_hash: 0,
            pseudo_state: 0,
            parent: None,
        }
    }
    
    /// Add class
    pub fn add_class(&mut self, class: &str) {
        let mut hasher = StyleHasher::new();
        hasher.write_u64(self.classes_hash);
        hasher.write_str(class);
        self.classes_hash = hasher.finish();
    }
    
    /// Set ID
    pub fn set_id(&mut self, id: &str) {
        let mut hasher = StyleHasher::new();
        hasher.write_str(id);
        self.id_hash = hasher.finish();
    }
    
    /// Add attribute
    pub fn add_attr(&mut self, name: &str, value: &str) {
        let mut hasher = StyleHasher::new();
        hasher.write_u64(self.attrs_hash);
        hasher.write_str(name);
        hasher.write_str(value);
        self.attrs_hash = hasher.finish();
    }
    
    /// Compute style key
    pub fn compute_key(&self) -> StyleKey {
        let mut hasher = StyleHasher::new();
        hasher.write_u64(self.tag_hash);
        hasher.write_u64(self.classes_hash);
        hasher.write_u64(self.id_hash);
        hasher.write_u64(self.attrs_hash);
        hasher.write_u32(self.pseudo_state);
        
        let rules_hash = hasher.finish();
        
        let parent_key = self.parent.as_ref().map(|p| p.compute_key());
        let inherited_hash = parent_key
            .as_ref()
            .map(|k| k.rules_hash ^ k.inherited_hash)
            .unwrap_or(0);
        
        if let Some(pk) = parent_key {
            StyleKey::with_parent(rules_hash, inherited_hash, pk)
        } else {
            StyleKey::new(rules_hash, inherited_hash)
        }
    }
    
    /// Check if can share with another context
    pub fn can_share(&self, other: &ElementContext) -> bool {
        self.tag_hash == other.tag_hash
            && self.classes_hash == other.classes_hash
            && self.id_hash == other.id_hash
            && self.attrs_hash == other.attrs_hash
            && self.pseudo_state == other.pseudo_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_basic() {
        let mut cache = StyleSharingCache::new();
        
        let key1 = StyleKey::new(123, 456);
        let key2 = StyleKey::new(789, 012);
        
        cache.insert(key1.clone(), vec![1, 2, 3]);
        cache.insert(key2.clone(), vec![4, 5, 6]);
        
        assert!(cache.get(&key1).is_some());
        assert!(cache.get(&key2).is_some());
        assert_eq!(cache.len(), 2);
    }
    
    #[test]
    fn test_cache_sharing() {
        let mut cache = StyleSharingCache::new();
        
        let key = StyleKey::new(100, 200);
        
        // First lookup - miss
        assert!(cache.get(&key).is_none());
        
        // Insert
        let style1 = cache.insert(key.clone(), vec![1, 2, 3, 4]);
        
        // Second lookup - hit
        let style2 = cache.get(&key).unwrap();
        
        assert_eq!(style1.properties(), style2.properties());
        assert!(cache.stats().hits >= 1);
    }
    
    #[test]
    fn test_bloom_filter() {
        let mut bloom1 = StyleBloomKey::new();
        bloom1.add(123);
        bloom1.add(456);
        
        assert!(bloom1.might_contain(123));
        assert!(bloom1.might_contain(456));
        // May have false positives but should not affect correctness
    }
    
    #[test]
    fn test_element_context() {
        let mut ctx1 = ElementContext::new("div");
        ctx1.add_class("container");
        ctx1.set_id("main");
        
        let mut ctx2 = ElementContext::new("div");
        ctx2.add_class("container");
        ctx2.set_id("main");
        
        assert!(ctx1.can_share(&ctx2));
        assert_eq!(ctx1.compute_key(), ctx2.compute_key());
    }
    
    #[test]
    fn test_element_context_different() {
        let mut ctx1 = ElementContext::new("div");
        ctx1.add_class("foo");
        
        let mut ctx2 = ElementContext::new("div");
        ctx2.add_class("bar");
        
        assert!(!ctx1.can_share(&ctx2));
        assert_ne!(ctx1.compute_key(), ctx2.compute_key());
    }
    
    #[test]
    fn test_lru_eviction() {
        let mut cache = StyleSharingCache::with_capacity(2);
        
        let key1 = StyleKey::new(1, 1);
        let key2 = StyleKey::new(2, 2);
        let key3 = StyleKey::new(3, 3);
        
        cache.insert(key1.clone(), vec![1]);
        cache.insert(key2.clone(), vec![2]);
        
        // Access key1 to make it recently used
        cache.get(&key1);
        
        // Insert key3, should evict key2
        cache.insert(key3.clone(), vec![3]);
        
        assert!(cache.get(&key1).is_some());
        assert!(cache.get(&key2).is_none()); // Evicted
        assert!(cache.get(&key3).is_some());
    }
}
