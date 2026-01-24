//! Shared Brotli/Zstandard Dictionary
//!
//! Shared dictionaries for compression across HTTP connections.
//! URL pattern matching for dictionary selection.

use std::collections::HashMap;
use std::sync::Arc;

/// Shared dictionary for compression
#[derive(Debug, Clone)]
pub struct SharedDictionary {
    /// URL pattern for matching
    pub url_pattern: Pattern,
    /// Dictionary hash (SHA-256)
    pub dictionary_hash: [u8; 32],
    /// Dictionary data (shared across connections)
    pub data: Arc<[u8]>,
    /// Dictionary ID
    pub id: u64,
    /// Match scope (path-based or origin-based)
    pub match_scope: MatchScope,
    /// Expiration time in seconds
    pub expiration: u64,
}

/// URL pattern for dictionary matching
#[derive(Debug, Clone)]
pub struct Pattern {
    /// Host pattern (exact or wildcard)
    host: String,
    /// Path prefix
    path_prefix: String,
    /// Is wildcard host
    is_wildcard: bool,
}

impl Pattern {
    /// Create a new pattern
    pub fn new(host: &str, path_prefix: &str) -> Self {
        let is_wildcard = host.starts_with("*.");
        Self {
            host: host.to_lowercase(),
            path_prefix: path_prefix.to_string(),
            is_wildcard,
        }
    }
    
    /// Create pattern for entire origin
    pub fn origin(host: &str) -> Self {
        Self::new(host, "/")
    }
    
    /// Check if URL matches this pattern
    pub fn matches(&self, host: &str, path: &str) -> bool {
        let host_match = if self.is_wildcard {
            let suffix = &self.host[1..]; // Remove "*"
            host.to_lowercase().ends_with(suffix)
        } else {
            host.to_lowercase() == self.host
        };
        
        let path_match = path.starts_with(&self.path_prefix);
        
        host_match && path_match
    }
}

/// Match scope for dictionary
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchScope {
    /// Match within same origin only
    SameOrigin,
    /// Match across related origins (with CORS)
    CrossOrigin,
}

impl Default for MatchScope {
    fn default() -> Self {
        Self::SameOrigin
    }
}

impl SharedDictionary {
    /// Create a new shared dictionary
    pub fn new(url_pattern: Pattern, data: Vec<u8>) -> Self {
        let dictionary_hash = Self::compute_hash(&data);
        Self {
            url_pattern,
            dictionary_hash,
            data: Arc::from(data),
            id: Self::generate_id(),
            match_scope: MatchScope::SameOrigin,
            expiration: 86400, // 24 hours default
        }
    }
    
    /// Create with cross-origin scope
    pub fn cross_origin(url_pattern: Pattern, data: Vec<u8>) -> Self {
        let mut dict = Self::new(url_pattern, data);
        dict.match_scope = MatchScope::CrossOrigin;
        dict
    }
    
    /// Check if dictionary matches URL
    pub fn matches(&self, host: &str, path: &str) -> bool {
        self.url_pattern.matches(host, path)
    }
    
    /// Get dictionary data
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    
    /// Get hash as hex string
    pub fn hash_hex(&self) -> String {
        self.dictionary_hash
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect()
    }
    
    /// Verify hash matches
    pub fn verify_hash(&self, expected: &[u8; 32]) -> bool {
        self.dictionary_hash == *expected
    }
    
    fn compute_hash(data: &[u8]) -> [u8; 32] {
        // Simple hash computation (in production, use SHA-256)
        let mut hash = [0u8; 32];
        let mut state = 0u64;
        
        for (i, byte) in data.iter().enumerate() {
            state = state.wrapping_mul(31).wrapping_add(*byte as u64);
            if i % 8 == 7 || i == data.len() - 1 {
                let idx = (i / 8) % 4;
                let bytes = state.to_le_bytes();
                for j in 0..8 {
                    hash[idx * 8 + j] ^= bytes[j];
                }
            }
        }
        
        hash
    }
    
    fn generate_id() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64
    }
}

/// Dictionary cache for managing shared dictionaries
#[derive(Debug, Default)]
pub struct DictionaryCache {
    /// Dictionaries by ID
    dictionaries: HashMap<u64, SharedDictionary>,
    /// Hash to ID lookup
    hash_index: HashMap<[u8; 32], u64>,
    /// Statistics
    stats: DictionaryCacheStats,
}

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct DictionaryCacheStats {
    /// Cache lookups
    pub lookups: u64,
    /// Cache hits
    pub hits: u64,
    /// Dictionaries stored
    pub stored: u64,
    /// Bytes saved by compression
    pub bytes_saved: u64,
}

impl DictionaryCacheStats {
    /// Get hit rate
    pub fn hit_rate(&self) -> f64 {
        if self.lookups == 0 {
            0.0
        } else {
            self.hits as f64 / self.lookups as f64
        }
    }
}

impl DictionaryCache {
    /// Create a new cache
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Store a dictionary
    pub fn store(&mut self, dict: SharedDictionary) {
        let id = dict.id;
        let hash = dict.dictionary_hash;
        self.dictionaries.insert(id, dict);
        self.hash_index.insert(hash, id);
        self.stats.stored += 1;
    }
    
    /// Find dictionary matching URL
    pub fn find(&mut self, host: &str, path: &str) -> Option<&SharedDictionary> {
        self.stats.lookups += 1;
        
        for dict in self.dictionaries.values() {
            if dict.matches(host, path) {
                self.stats.hits += 1;
                return Some(dict);
            }
        }
        
        None
    }
    
    /// Get dictionary by hash
    pub fn get_by_hash(&mut self, hash: &[u8; 32]) -> Option<&SharedDictionary> {
        self.stats.lookups += 1;
        
        if let Some(&id) = self.hash_index.get(hash) {
            if let Some(dict) = self.dictionaries.get(&id) {
                self.stats.hits += 1;
                return Some(dict);
            }
        }
        
        None
    }
    
    /// Get dictionary by ID
    pub fn get(&self, id: u64) -> Option<&SharedDictionary> {
        self.dictionaries.get(&id)
    }
    
    /// Record bytes saved
    pub fn record_savings(&mut self, bytes: u64) {
        self.stats.bytes_saved += bytes;
    }
    
    /// Get statistics
    pub fn stats(&self) -> &DictionaryCacheStats {
        &self.stats
    }
    
    /// Remove expired dictionaries
    pub fn evict_expired(&mut self) {
        // In a real implementation, check expiration times
        // For now, just limit size
        while self.dictionaries.len() > 100 {
            if let Some((&id, _)) = self.dictionaries.iter().next() {
                if let Some(dict) = self.dictionaries.remove(&id) {
                    self.hash_index.remove(&dict.dictionary_hash);
                }
            }
        }
    }
    
    /// Clear all dictionaries
    pub fn clear(&mut self) {
        self.dictionaries.clear();
        self.hash_index.clear();
    }
    
    /// Number of cached dictionaries
    pub fn len(&self) -> usize {
        self.dictionaries.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.dictionaries.is_empty()
    }
}

/// HTTP header names for shared dictionary
pub mod headers {
    /// Use-As-Dictionary header
    pub const USE_AS_DICTIONARY: &str = "use-as-dictionary";
    /// Available-Dictionary header
    pub const AVAILABLE_DICTIONARY: &str = "available-dictionary";
    /// Dictionary-ID header
    pub const DICTIONARY_ID: &str = "dictionary-id";
}

/// Parse Use-As-Dictionary header
#[derive(Debug, Clone)]
pub struct UseAsDictionary {
    /// Match pattern
    pub match_pattern: String,
    /// Match scope
    pub match_scope: MatchScope,
    /// ID for the dictionary
    pub id: Option<String>,
    /// Expiration in seconds
    pub expiration: Option<u64>,
}

impl UseAsDictionary {
    /// Parse from header value
    pub fn parse(value: &str) -> Option<Self> {
        let mut match_pattern = String::new();
        let mut match_scope = MatchScope::SameOrigin;
        let mut id = None;
        let mut expiration = None;
        
        for part in value.split(',') {
            let part = part.trim();
            if let Some((key, val)) = part.split_once('=') {
                let key = key.trim();
                let val = val.trim().trim_matches('"');
                
                match key {
                    "match" => match_pattern = val.to_string(),
                    "match-dest" if val == "cross-origin" => {
                        match_scope = MatchScope::CrossOrigin;
                    }
                    "id" => id = Some(val.to_string()),
                    "expires" => expiration = val.parse().ok(),
                    _ => {}
                }
            }
        }
        
        if match_pattern.is_empty() {
            return None;
        }
        
        Some(Self {
            match_pattern,
            match_scope,
            id,
            expiration,
        })
    }
    
    /// Serialize to header value
    pub fn to_header_value(&self) -> String {
        let mut parts = vec![format!("match=\"{}\"", self.match_pattern)];
        
        if self.match_scope == MatchScope::CrossOrigin {
            parts.push("match-dest=cross-origin".to_string());
        }
        
        if let Some(ref id) = self.id {
            parts.push(format!("id=\"{}\"", id));
        }
        
        if let Some(exp) = self.expiration {
            parts.push(format!("expires={}", exp));
        }
        
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pattern_exact() {
        let pattern = Pattern::new("example.com", "/api/");
        
        assert!(pattern.matches("example.com", "/api/users"));
        assert!(pattern.matches("EXAMPLE.COM", "/api/items"));
        assert!(!pattern.matches("example.com", "/web/"));
        assert!(!pattern.matches("other.com", "/api/"));
    }
    
    #[test]
    fn test_pattern_wildcard() {
        let pattern = Pattern::new("*.example.com", "/");
        
        assert!(pattern.matches("api.example.com", "/test"));
        assert!(pattern.matches("web.example.com", "/page"));
        assert!(!pattern.matches("example.com", "/test"));
    }
    
    #[test]
    fn test_shared_dictionary() {
        let dict = SharedDictionary::new(
            Pattern::origin("example.com"),
            vec![1, 2, 3, 4, 5],
        );
        
        assert!(dict.matches("example.com", "/any/path"));
        assert!(!dict.matches("other.com", "/any/path"));
        assert_eq!(dict.data().len(), 5);
    }
    
    #[test]
    fn test_dictionary_cache() {
        let mut cache = DictionaryCache::new();
        
        let dict = SharedDictionary::new(
            Pattern::new("example.com", "/api/"),
            vec![1, 2, 3],
        );
        
        cache.store(dict);
        
        assert_eq!(cache.len(), 1);
        assert!(cache.find("example.com", "/api/v1").is_some());
        assert!(cache.find("example.com", "/web/").is_none());
    }
    
    #[test]
    fn test_use_as_dictionary_parse() {
        let header = r#"match="/api/*", expires=86400, id="api-dict""#;
        let parsed = UseAsDictionary::parse(header).unwrap();
        
        assert_eq!(parsed.match_pattern, "/api/*");
        assert_eq!(parsed.expiration, Some(86400));
        assert_eq!(parsed.id, Some("api-dict".to_string()));
    }
}
