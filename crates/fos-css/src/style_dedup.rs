//! Inline Style Deduplication (Phase 24.2)
//!
//! Hash all inline style strings, store once.
//! Many elements have identical inline styles - 80% memory savings.

use std::collections::HashMap;
use std::sync::Arc;
use std::hash::{Hash, Hasher};

/// Style content ID - reference into the deduplication store
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct StyleId(pub u32);

impl StyleId {
    /// No style
    pub const NONE: Self = StyleId(0);
    
    pub fn is_none(self) -> bool {
        self.0 == 0
    }
}

/// Deduplicated inline style store
#[derive(Debug)]
pub struct StyleDeduplicator {
    /// Hash -> (StyleId, content)
    by_hash: HashMap<u64, StyleId>,
    /// StyleId -> content
    by_id: HashMap<StyleId, Arc<str>>,
    /// Next style ID
    next_id: u32,
    /// Statistics
    stats: DeduplicationStats,
}

/// Statistics for style deduplication
#[derive(Debug, Clone, Copy, Default)]
pub struct DeduplicationStats {
    /// Total styles encountered
    pub total: u64,
    /// Unique styles stored
    pub unique: u64,
    /// Deduplicated (reused) styles
    pub deduplicated: u64,
    /// Bytes stored (unique only)
    pub bytes_stored: u64,
    /// Bytes saved (would have been duplicated)
    pub bytes_saved: u64,
}

impl DeduplicationStats {
    /// Deduplication rate
    pub fn dedup_rate(&self) -> f64 {
        if self.total == 0 {
            0.0
        } else {
            self.deduplicated as f64 / self.total as f64 * 100.0
        }
    }
    
    /// Memory saved as percentage
    pub fn savings_rate(&self) -> f64 {
        let total_would_be = self.bytes_stored + self.bytes_saved;
        if total_would_be == 0 {
            0.0
        } else {
            self.bytes_saved as f64 / total_would_be as f64 * 100.0
        }
    }
}

impl Default for StyleDeduplicator {
    fn default() -> Self {
        Self::new()
    }
}

impl StyleDeduplicator {
    pub fn new() -> Self {
        let mut store = Self {
            by_hash: HashMap::new(),
            by_id: HashMap::new(),
            next_id: 1, // 0 reserved for NONE
            stats: DeduplicationStats::default(),
        };
        
        // Insert empty style at ID 0
        store.by_id.insert(StyleId::NONE, Arc::from(""));
        
        store
    }
    
    /// Hash a style string
    fn hash_style(style: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        style.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Intern a style string, returning its ID
    pub fn intern(&mut self, style: &str) -> StyleId {
        self.stats.total += 1;
        
        if style.is_empty() {
            return StyleId::NONE;
        }
        
        let hash = Self::hash_style(style);
        
        if let Some(&id) = self.by_hash.get(&hash) {
            // Already stored
            self.stats.deduplicated += 1;
            self.stats.bytes_saved += style.len() as u64;
            return id;
        }
        
        // New style - store it
        let id = StyleId(self.next_id);
        self.next_id += 1;
        
        let content: Arc<str> = Arc::from(style);
        self.by_hash.insert(hash, id);
        self.by_id.insert(id, content);
        
        self.stats.unique += 1;
        self.stats.bytes_stored += style.len() as u64;
        
        id
    }
    
    /// Get style content by ID
    pub fn get(&self, id: StyleId) -> Option<&Arc<str>> {
        self.by_id.get(&id)
    }
    
    /// Get style content as &str
    pub fn get_str(&self, id: StyleId) -> Option<&str> {
        self.by_id.get(&id).map(|s| s.as_ref())
    }
    
    /// Number of unique styles stored
    pub fn len(&self) -> usize {
        self.by_id.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.by_id.len() <= 1 // Only NONE
    }
    
    /// Get statistics
    pub fn stats(&self) -> &DeduplicationStats {
        &self.stats
    }
    
    /// Memory usage estimate
    pub fn memory_usage(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.by_hash.capacity() * (std::mem::size_of::<u64>() + std::mem::size_of::<StyleId>())
            + self.by_id.capacity() * (std::mem::size_of::<StyleId>() + std::mem::size_of::<Arc<str>>())
            + self.stats.bytes_stored as usize
    }
}

/// Style reference that can be either inline or deduplicated
#[derive(Debug, Clone, Copy)]
pub enum StyleRef {
    /// No style
    None,
    /// Deduplicated style by ID
    Dedup(StyleId),
}

impl StyleRef {
    pub fn is_none(self) -> bool {
        matches!(self, StyleRef::None)
    }
    
    /// Resolve to style string
    pub fn resolve<'a>(self, store: &'a StyleDeduplicator) -> Option<&'a str> {
        match self {
            StyleRef::None => None,
            StyleRef::Dedup(id) => store.get_str(id),
        }
    }
    
    /// Memory size (just the reference)
    pub const fn memory_size() -> usize {
        std::mem::size_of::<Self>()
    }
}

impl From<StyleId> for StyleRef {
    fn from(id: StyleId) -> Self {
        if id.is_none() {
            StyleRef::None
        } else {
            StyleRef::Dedup(id)
        }
    }
}

/// Parsed inline style with deduplication
#[derive(Debug, Clone)]
pub struct InlineStyle {
    /// Reference to raw style string
    pub raw_ref: StyleRef,
    /// Parsed properties (computed lazily)
    properties: Option<Vec<(Box<str>, Box<str>)>>,
}

impl InlineStyle {
    /// Create from deduplicated style ID
    pub fn from_id(id: StyleId) -> Self {
        Self {
            raw_ref: id.into(),
            properties: None,
        }
    }
    
    /// Create with no style
    pub fn none() -> Self {
        Self {
            raw_ref: StyleRef::None,
            properties: None,
        }
    }
    
    /// Parse the style (if not already parsed)
    pub fn parse(&mut self, store: &StyleDeduplicator) {
        if self.properties.is_some() {
            return;
        }
        
        if let Some(raw) = self.raw_ref.resolve(store) {
            let props = parse_inline_style(raw);
            self.properties = Some(props);
        }
    }
    
    /// Get parsed properties
    pub fn properties(&self) -> Option<&[(Box<str>, Box<str>)]> {
        self.properties.as_deref()
    }
    
    /// Get a specific property value
    pub fn get_property(&self, name: &str) -> Option<&str> {
        self.properties.as_ref()
            .and_then(|props| {
                props.iter()
                    .find(|(n, _)| n.as_ref() == name)
                    .map(|(_, v)| v.as_ref())
            })
    }
}

/// Parse inline style string into property-value pairs
fn parse_inline_style(style: &str) -> Vec<(Box<str>, Box<str>)> {
    style.split(';')
        .filter_map(|decl| {
            let decl = decl.trim();
            if decl.is_empty() {
                return None;
            }
            
            let mut parts = decl.splitn(2, ':');
            let name = parts.next()?.trim();
            let value = parts.next()?.trim();
            
            if name.is_empty() || value.is_empty() {
                return None;
            }
            
            Some((name.into(), value.into()))
        })
        .collect()
}

/// Batch style deduplicator for processing multiple elements
pub struct BatchStyleDeduplicator<'a> {
    store: &'a mut StyleDeduplicator,
    results: Vec<StyleId>,
}

impl<'a> BatchStyleDeduplicator<'a> {
    pub fn new(store: &'a mut StyleDeduplicator) -> Self {
        Self {
            store,
            results: Vec::new(),
        }
    }
    
    /// Add a style to the batch
    pub fn add(&mut self, style: &str) -> usize {
        let id = self.store.intern(style);
        self.results.push(id);
        self.results.len() - 1
    }
    
    /// Get results
    pub fn results(&self) -> &[StyleId] {
        &self.results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_style_deduplicator() {
        let mut store = StyleDeduplicator::new();
        
        // First occurrence
        let id1 = store.intern("color: red; margin: 10px;");
        assert_eq!(store.stats().unique, 1);
        assert_eq!(store.stats().deduplicated, 0);
        
        // Same style again
        let id2 = store.intern("color: red; margin: 10px;");
        assert_eq!(id1, id2);
        assert_eq!(store.stats().unique, 1);
        assert_eq!(store.stats().deduplicated, 1);
        
        // Different style
        let id3 = store.intern("color: blue;");
        assert_ne!(id1, id3);
        assert_eq!(store.stats().unique, 2);
    }
    
    #[test]
    fn test_deduplication_savings() {
        let mut store = StyleDeduplicator::new();
        let style = "font-size: 14px; line-height: 1.5;";
        
        // Add same style 10 times
        for _ in 0..10 {
            store.intern(style);
        }
        
        assert_eq!(store.stats().unique, 1);
        assert_eq!(store.stats().deduplicated, 9);
        assert_eq!(store.stats().bytes_stored, style.len() as u64);
        assert_eq!(store.stats().bytes_saved, (style.len() * 9) as u64);
        
        // ~90% savings rate
        assert!(store.stats().savings_rate() > 85.0);
    }
    
    #[test]
    fn test_style_ref() {
        let mut store = StyleDeduplicator::new();
        let id = store.intern("display: flex;");
        
        let style_ref = StyleRef::from(id);
        assert_eq!(style_ref.resolve(&store), Some("display: flex;"));
        
        let none = StyleRef::None;
        assert!(none.is_none());
        assert_eq!(none.resolve(&store), None);
    }
    
    #[test]
    fn test_inline_style_parsing() {
        let mut store = StyleDeduplicator::new();
        let id = store.intern("color: red; margin: 10px; padding: 5px");
        
        let mut style = InlineStyle::from_id(id);
        style.parse(&store);
        
        let props = style.properties().unwrap();
        assert_eq!(props.len(), 3);
        
        assert_eq!(style.get_property("color"), Some("red"));
        assert_eq!(style.get_property("margin"), Some("10px"));
    }
    
    #[test]
    fn test_parse_inline_style() {
        let props = parse_inline_style("color: red; margin: 10px ; padding:5px");
        
        assert_eq!(props.len(), 3);
        assert_eq!(props[0], ("color".into(), "red".into()));
        assert_eq!(props[1], ("margin".into(), "10px".into()));
        assert_eq!(props[2], ("padding".into(), "5px".into()));
    }
    
    #[test]
    fn test_empty_style() {
        let mut store = StyleDeduplicator::new();
        
        let id = store.intern("");
        assert!(id.is_none());
        
        let second = store.intern("");
        assert!(second.is_none());
    }
}
