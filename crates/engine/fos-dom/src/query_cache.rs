//! DOM Query Cache and Normalization
//!
//! Selector-result memoization and DOM normalization operations.

use std::collections::HashMap;
use super::compact_node::DomGeneration;

/// Selector result cache with DOM generation validation
#[derive(Debug, Default)]
pub struct QueryCache {
    /// Cached query results
    cache: HashMap<QueryKey, CachedResult>,
    /// Current DOM generation
    generation: DomGeneration,
    /// Maximum cache entries
    max_entries: usize,
}

/// Cache key for selector queries
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct QueryKey {
    /// Root node for query
    pub root: u32,
    /// Selector string
    pub selector: String,
    /// Query type
    pub query_type: QueryType,
}

/// Query type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueryType {
    QuerySelector,
    QuerySelectorAll,
    GetElementsByClassName,
    GetElementsByTagName,
    Matches,
    Closest,
}

/// Cached query result
#[derive(Debug, Clone)]
pub struct CachedResult {
    /// DOM generation when cached
    pub generation: DomGeneration,
    /// Result node IDs
    pub results: Vec<u32>,
}

impl QueryCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            cache: HashMap::new(),
            generation: DomGeneration::new(),
            max_entries,
        }
    }
    
    /// Get cached result if valid
    pub fn get(&self, key: &QueryKey) -> Option<&[u32]> {
        if let Some(cached) = self.cache.get(key) {
            if cached.generation == self.generation {
                return Some(&cached.results);
            }
        }
        None
    }
    
    /// Store result in cache
    pub fn set(&mut self, key: QueryKey, results: Vec<u32>) {
        // Evict if at capacity
        if self.cache.len() >= self.max_entries {
            self.evict_oldest();
        }
        
        self.cache.insert(key, CachedResult {
            generation: self.generation,
            results,
        });
    }
    
    /// Invalidate cache (DOM mutated)
    pub fn invalidate(&mut self) {
        self.generation.increment();
    }
    
    /// Clear all cached results
    pub fn clear(&mut self) {
        self.cache.clear();
        self.generation.increment();
    }
    
    /// Get current generation
    pub fn generation(&self) -> DomGeneration {
        self.generation
    }
    
    fn evict_oldest(&mut self) {
        // Simple strategy: remove entries from old generations
        let current = self.generation.0;
        self.cache.retain(|_, v| current - v.generation.0 < 10);
    }
    
    /// Stats
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.cache.len(),
            generation: self.generation.0,
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entries: usize,
    pub generation: u64,
}

/// DOM normalization operations
pub struct DomNormalizer;

impl DomNormalizer {
    /// Normalize a node (merge adjacent text nodes)
    /// Returns list of nodes to remove
    pub fn normalize_children(children: &[TextNodeInfo]) -> Vec<NormalizeAction> {
        let mut actions = Vec::new();
        let mut i = 0;
        
        while i < children.len() {
            if !children[i].is_text {
                i += 1;
                continue;
            }
            
            // Start of text run
            let run_start = i;
            let mut merged_text = children[i].text.clone();
            i += 1;
            
            // Merge consecutive text nodes
            while i < children.len() && children[i].is_text {
                merged_text.push_str(&children[i].text);
                i += 1;
            }
            
            let run_end = i;
            
            if run_end - run_start > 1 {
                // Multiple text nodes to merge
                actions.push(NormalizeAction::Merge {
                    keep_index: run_start,
                    remove_indices: (run_start + 1..run_end).collect(),
                    merged_text,
                });
            } else if merged_text.is_empty() {
                // Single empty text node
                actions.push(NormalizeAction::Remove {
                    index: run_start,
                });
            }
        }
        
        actions
    }
}

/// Information about a text node for normalization
#[derive(Debug, Clone)]
pub struct TextNodeInfo {
    pub node_id: u32,
    pub is_text: bool,
    pub text: String,
}

/// Normalization action
#[derive(Debug, Clone)]
pub enum NormalizeAction {
    /// Merge text nodes
    Merge {
        keep_index: usize,
        remove_indices: Vec<usize>,
        merged_text: String,
    },
    /// Remove empty text node
    Remove {
        index: usize,
    },
}

/// Deduplicated attribute storage
#[derive(Debug, Default)]
pub struct AttributeDeduplicator {
    /// Unique attribute strings
    strings: HashMap<String, u16>,
    /// Reverse lookup
    values: Vec<String>,
}

impl AttributeDeduplicator {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Intern an attribute string
    pub fn intern(&mut self, s: &str) -> u16 {
        if let Some(&id) = self.strings.get(s) {
            return id;
        }
        
        let id = self.values.len() as u16;
        self.values.push(s.to_string());
        self.strings.insert(s.to_string(), id);
        id
    }
    
    /// Get string by ID
    pub fn get(&self, id: u16) -> Option<&str> {
        self.values.get(id as usize).map(|s| s.as_str())
    }
    
    /// Number of unique strings
    pub fn len(&self) -> usize {
        self.values.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Borrowed DOM string (zero-allocation for parsing)
#[derive(Debug, Clone, Copy)]
pub struct BorrowedStr<'a> {
    data: &'a str,
}

impl<'a> BorrowedStr<'a> {
    pub fn new(s: &'a str) -> Self {
        Self { data: s }
    }
    
    pub fn as_str(&self) -> &str {
        self.data
    }
    
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    
    /// Slice without allocation
    pub fn slice(&self, start: usize, end: usize) -> Self {
        Self {
            data: &self.data[start..end],
        }
    }
    
    /// Split without allocation
    pub fn split_at(&self, mid: usize) -> (Self, Self) {
        let (a, b) = self.data.split_at(mid);
        (Self { data: a }, Self { data: b })
    }
}

impl<'a> AsRef<str> for BorrowedStr<'a> {
    fn as_ref(&self) -> &str {
        self.data
    }
}

/// Zero-copy token for parsing
#[derive(Debug, Clone, Copy)]
pub enum ParseToken<'a> {
    StartTag {
        name: BorrowedStr<'a>,
        self_closing: bool,
    },
    EndTag {
        name: BorrowedStr<'a>,
    },
    Text {
        content: BorrowedStr<'a>,
    },
    Comment {
        content: BorrowedStr<'a>,
    },
    Attribute {
        name: BorrowedStr<'a>,
        value: BorrowedStr<'a>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_query_cache() {
        let mut cache = QueryCache::new(100);
        
        let key = QueryKey {
            root: 0,
            selector: "div.foo".to_string(),
            query_type: QueryType::QuerySelectorAll,
        };
        
        cache.set(key.clone(), vec![1, 2, 3]);
        assert_eq!(cache.get(&key), Some(&[1, 2, 3][..]));
        
        cache.invalidate();
        assert_eq!(cache.get(&key), None);
    }
    
    #[test]
    fn test_normalize() {
        let children = vec![
            TextNodeInfo { node_id: 1, is_text: true, text: "Hello".to_string() },
            TextNodeInfo { node_id: 2, is_text: true, text: " ".to_string() },
            TextNodeInfo { node_id: 3, is_text: true, text: "World".to_string() },
        ];
        
        let actions = DomNormalizer::normalize_children(&children);
        assert_eq!(actions.len(), 1);
        
        match &actions[0] {
            NormalizeAction::Merge { merged_text, remove_indices, .. } => {
                assert_eq!(merged_text, "Hello World");
                assert_eq!(remove_indices.len(), 2);
            }
            _ => panic!("Expected merge action"),
        }
    }
    
    #[test]
    fn test_attribute_deduplicator() {
        let mut dedup = AttributeDeduplicator::new();
        
        let id1 = dedup.intern("class");
        let id2 = dedup.intern("id");
        let id3 = dedup.intern("class"); // Duplicate
        
        assert_eq!(id1, id3);
        assert_ne!(id1, id2);
        assert_eq!(dedup.get(id1), Some("class"));
    }
    
    #[test]
    fn test_borrowed_str() {
        let s = "Hello World";
        let borrowed = BorrowedStr::new(s);
        
        let (a, b) = borrowed.split_at(5);
        assert_eq!(a.as_str(), "Hello");
        assert_eq!(b.as_str(), " World");
    }
}
