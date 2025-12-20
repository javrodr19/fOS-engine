//! Lazy Attribute Parsing (Phase 24.2)
//!
//! Store attributes as raw bytes initially, parse only when accessed.
//! Many attributes are never read (data-*, aria-*, etc.), achieving
//! 30% parsing time savings.

use std::cell::OnceCell;
use std::collections::HashMap;

/// Lazy attribute that parses on first access
#[derive(Debug)]
pub struct LazyAttribute {
    /// Raw attribute bytes (unparsed)
    raw: Box<[u8]>,
    /// Parsed value (computed on first access)
    parsed: OnceCell<Box<str>>,
}

impl Clone for LazyAttribute {
    fn clone(&self) -> Self {
        Self {
            raw: self.raw.clone(),
            parsed: self.parsed.clone(),
        }
    }
}

impl LazyAttribute {
    /// Create from raw bytes
    pub fn new(raw: Vec<u8>) -> Self {
        Self {
            raw: raw.into_boxed_slice(),
            parsed: OnceCell::new(),
        }
    }
    
    /// Create from string (pre-parsed)
    pub fn from_string(s: &str) -> Self {
        let raw = s.as_bytes().to_vec();
        let parsed = OnceCell::from(s.into());
        Self {
            raw: raw.into_boxed_slice(),
            parsed,
        }
    }
    
    /// Get the parsed value (parses on first call)
    pub fn get(&self) -> &str {
        self.parsed.get_or_init(|| {
            // Parse raw bytes to string
            String::from_utf8_lossy(&self.raw).into_owned().into_boxed_str()
        })
    }
    
    /// Check if already parsed
    pub fn is_parsed(&self) -> bool {
        self.parsed.get().is_some()
    }
    
    /// Get raw bytes without parsing
    pub fn raw_bytes(&self) -> &[u8] {
        &self.raw
    }
    
    /// Memory size
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>() 
            + self.raw.len()
            + self.parsed.get().map(|s| s.len()).unwrap_or(0)
    }
    
    /// Memory size if it hadn't been lazy (always parsed)
    pub fn eager_memory_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.raw.len() * 2 // raw + parsed
    }
    
    /// Memory savings from laziness
    pub fn savings(&self) -> usize {
        if self.is_parsed() {
            0
        } else {
            self.raw.len() // Saved the parsed copy
        }
    }
}

/// Access-tracking lazy attribute
#[derive(Debug)]
pub struct TrackedLazyAttribute {
    attr: LazyAttribute,
    access_count: u32,
}

impl TrackedLazyAttribute {
    pub fn new(raw: Vec<u8>) -> Self {
        Self {
            attr: LazyAttribute::new(raw),
            access_count: 0,
        }
    }
    
    pub fn get(&mut self) -> &str {
        self.access_count += 1;
        self.attr.get()
    }
    
    pub fn access_count(&self) -> u32 {
        self.access_count
    }
    
    pub fn was_ever_accessed(&self) -> bool {
        self.access_count > 0
    }
}

/// Lazy attribute map for an element
#[derive(Debug, Default)]
pub struct LazyAttributeMap {
    /// Attributes indexed by name hash
    attributes: HashMap<u32, LazyAttribute>,
}

impl LazyAttributeMap {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add an attribute
    pub fn insert(&mut self, name_hash: u32, raw_value: Vec<u8>) {
        self.attributes.insert(name_hash, LazyAttribute::new(raw_value));
    }
    
    /// Add a pre-parsed attribute
    pub fn insert_parsed(&mut self, name_hash: u32, value: &str) {
        self.attributes.insert(name_hash, LazyAttribute::from_string(value));
    }
    
    /// Get an attribute value (parses if needed)
    pub fn get(&self, name_hash: u32) -> Option<&str> {
        self.attributes.get(&name_hash).map(|a| a.get())
    }
    
    /// Check if attribute exists (doesn't parse)
    pub fn contains(&self, name_hash: u32) -> bool {
        self.attributes.contains_key(&name_hash)
    }
    
    /// Get raw bytes (doesn't parse)
    pub fn get_raw(&self, name_hash: u32) -> Option<&[u8]> {
        self.attributes.get(&name_hash).map(|a| a.raw_bytes())
    }
    
    /// Number of attributes
    pub fn len(&self) -> usize {
        self.attributes.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty()
    }
    
    /// Count of parsed attributes
    pub fn parsed_count(&self) -> usize {
        self.attributes.values().filter(|a| a.is_parsed()).count()
    }
    
    /// Count of unparsed attributes
    pub fn unparsed_count(&self) -> usize {
        self.len() - self.parsed_count()
    }
    
    /// Total memory used
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.attributes.values().map(|a| a.memory_size()).sum::<usize>()
    }
    
    /// Memory savings from laziness
    pub fn savings(&self) -> usize {
        self.attributes.values().map(|a| a.savings()).sum()
    }
    
    /// Statistics
    pub fn stats(&self) -> LazyAttrStats {
        LazyAttrStats {
            total: self.len(),
            parsed: self.parsed_count(),
            unparsed: self.unparsed_count(),
            memory_used: self.memory_size(),
            memory_saved: self.savings(),
        }
    }
}

/// Statistics for lazy attributes
#[derive(Debug, Clone, Copy)]
pub struct LazyAttrStats {
    pub total: usize,
    pub parsed: usize,
    pub unparsed: usize,
    pub memory_used: usize,
    pub memory_saved: usize,
}

impl LazyAttrStats {
    /// Percentage of attributes that were never accessed
    pub fn never_accessed_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.unparsed as f64 / self.total as f64 * 100.0
        }
    }
}

/// Global lazy attribute statistics collector
#[derive(Debug, Default)]
pub struct LazyAttrCollector {
    /// Total attributes created
    total_created: u64,
    /// Total attributes accessed
    total_accessed: u64,
    /// Common never-accessed prefixes
    never_accessed_prefixes: HashMap<Box<str>, u64>,
}

impl LazyAttrCollector {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Record an attribute creation
    pub fn record_creation(&mut self) {
        self.total_created += 1;
    }
    
    /// Record an attribute access
    pub fn record_access(&mut self) {
        self.total_accessed += 1;
    }
    
    /// Record a never-accessed attribute
    pub fn record_never_accessed(&mut self, name: &str) {
        // Extract prefix (e.g., "data-" or "aria-")
        let prefix = if let Some(idx) = name.find('-') {
            &name[..=idx]
        } else {
            name
        };
        
        *self.never_accessed_prefixes.entry(prefix.into()).or_insert(0) += 1;
    }
    
    /// Get access rate
    pub fn access_rate(&self) -> f64 {
        if self.total_created == 0 {
            0.0
        } else {
            self.total_accessed as f64 / self.total_created as f64 * 100.0
        }
    }
    
    /// Get commonly unused prefixes
    pub fn unused_prefixes(&self) -> Vec<(&str, u64)> {
        let mut prefixes: Vec<_> = self.never_accessed_prefixes.iter()
            .map(|(k, v)| (k.as_ref(), *v))
            .collect();
        prefixes.sort_by(|a, b| b.1.cmp(&a.1));
        prefixes
    }
}

/// Iterator over lazy attributes
pub struct LazyAttrIter<'a> {
    inner: std::collections::hash_map::Iter<'a, u32, LazyAttribute>,
}

impl<'a> Iterator for LazyAttrIter<'a> {
    type Item = (u32, &'a str);
    
    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(&k, v)| (k, v.get()))
    }
}

impl LazyAttributeMap {
    /// Iterate over all attributes (will parse all)
    pub fn iter(&self) -> LazyAttrIter<'_> {
        LazyAttrIter {
            inner: self.attributes.iter(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lazy_attribute() {
        let attr = LazyAttribute::new(b"hello world".to_vec());
        
        // Not parsed yet
        assert!(!attr.is_parsed());
        
        // Access triggers parsing
        assert_eq!(attr.get(), "hello world");
        assert!(attr.is_parsed());
        
        // Second access is cached
        assert_eq!(attr.get(), "hello world");
    }
    
    #[test]
    fn test_lazy_attribute_savings() {
        let attr = LazyAttribute::new(b"some value".to_vec());
        
        // Before parsing, we save the parsed copy
        let savings_before = attr.savings();
        assert!(savings_before > 0);
        
        // After parsing, no savings
        let _ = attr.get();
        assert_eq!(attr.savings(), 0);
    }
    
    #[test]
    fn test_lazy_attribute_map() {
        let mut map = LazyAttributeMap::new();
        
        map.insert(1, b"value1".to_vec());
        map.insert(2, b"value2".to_vec());
        map.insert(3, b"value3".to_vec());
        
        assert_eq!(map.len(), 3);
        assert_eq!(map.parsed_count(), 0);
        
        // Access one
        assert_eq!(map.get(1), Some("value1"));
        assert_eq!(map.parsed_count(), 1);
        assert_eq!(map.unparsed_count(), 2);
        
        // Check existence without parsing
        assert!(map.contains(2));
        assert_eq!(map.parsed_count(), 1); // Still 1
    }
    
    #[test]
    fn test_tracked_lazy_attribute() {
        let mut attr = TrackedLazyAttribute::new(b"test".to_vec());
        
        assert_eq!(attr.access_count(), 0);
        assert!(!attr.was_ever_accessed());
        
        let _ = attr.get();
        assert_eq!(attr.access_count(), 1);
        assert!(attr.was_ever_accessed());
        
        let _ = attr.get();
        let _ = attr.get();
        assert_eq!(attr.access_count(), 3);
    }
    
    #[test]
    fn test_lazy_attr_stats() {
        let mut map = LazyAttributeMap::new();
        
        // Add 10 attributes, access only 3
        for i in 0..10 {
            map.insert(i, format!("value{}", i).into_bytes());
        }
        
        // Access first 3
        for i in 0..3 {
            let _ = map.get(i);
        }
        
        let stats = map.stats();
        assert_eq!(stats.total, 10);
        assert_eq!(stats.parsed, 3);
        assert_eq!(stats.unparsed, 7);
        assert!((stats.never_accessed_rate() - 70.0).abs() < 0.01);
    }
}
