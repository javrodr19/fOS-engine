//! Brotli Shared Dictionary
//!
//! Brotli compression with shared dictionary for common web patterns.

use std::collections::HashMap;

/// Brotli shared dictionary
#[derive(Debug)]
pub struct BrotliSharedDict {
    /// Dictionary ID
    pub id: DictId,
    /// Dictionary data
    data: Vec<u8>,
    /// Patterns included
    patterns: Vec<String>,
}

/// Dictionary ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DictId(pub u64);

impl BrotliSharedDict {
    /// Create with common web patterns
    pub fn web_default() -> Self {
        let patterns = vec![
            "<!DOCTYPE html>".to_string(),
            "<html".to_string(),
            "<head>".to_string(),
            "<body>".to_string(),
            "<div".to_string(),
            "<span".to_string(),
            "<script".to_string(),
            "<style".to_string(),
            "class=\"".to_string(),
            "id=\"".to_string(),
            "href=\"".to_string(),
            "src=\"".to_string(),
            "\"https://".to_string(),
            "</div>".to_string(),
            "</span>".to_string(),
            "function".to_string(),
            "return".to_string(),
            "const ".to_string(),
            "let ".to_string(),
            "var ".to_string(),
        ];
        
        let mut data = Vec::new();
        for p in &patterns {
            data.extend_from_slice(p.as_bytes());
        }
        
        Self { id: DictId(1), data, patterns }
    }
    
    /// Create empty
    pub fn empty() -> Self {
        Self { id: DictId(0), data: Vec::new(), patterns: Vec::new() }
    }
    
    /// Get dictionary data
    pub fn data(&self) -> &[u8] { &self.data }
    
    /// Get patterns
    pub fn patterns(&self) -> &[String] { &self.patterns }
    
    /// Add pattern
    pub fn add_pattern(&mut self, pattern: &str) {
        self.patterns.push(pattern.to_string());
        self.data.extend_from_slice(pattern.as_bytes());
    }
}

/// Dictionary builder
#[derive(Debug, Default)]
pub struct DictionaryBuilder {
    patterns: HashMap<String, u32>,
    min_frequency: u32,
}

impl DictionaryBuilder {
    pub fn new() -> Self { Self { patterns: HashMap::new(), min_frequency: 2 } }
    
    pub fn set_min_frequency(&mut self, freq: u32) { self.min_frequency = freq; }
    
    pub fn add_sample(&mut self, data: &str) {
        // Extract common patterns (simplified n-gram)
        for len in 4..=32 {
            for i in 0..data.len().saturating_sub(len) {
                let pattern = &data[i..i+len];
                if pattern.chars().all(|c| c.is_ascii()) {
                    *self.patterns.entry(pattern.to_string()).or_insert(0) += 1;
                }
            }
        }
    }
    
    pub fn build(self) -> BrotliSharedDict {
        let mut patterns: Vec<_> = self.patterns.into_iter()
            .filter(|(_, c)| *c >= self.min_frequency)
            .collect();
        patterns.sort_by(|a, b| b.1.cmp(&a.1));
        
        let top_patterns: Vec<String> = patterns.into_iter()
            .take(100)
            .map(|(p, _)| p)
            .collect();
        
        let mut data = Vec::new();
        for p in &top_patterns {
            data.extend_from_slice(p.as_bytes());
        }
        
        BrotliSharedDict { id: DictId(2), data, patterns: top_patterns }
    }
}

/// Brotli decompressor with dictionary
#[derive(Debug)]
pub struct BrotliDecompressor {
    dict: Option<BrotliSharedDict>,
}

impl Default for BrotliDecompressor {
    fn default() -> Self { Self::new() }
}

impl BrotliDecompressor {
    pub fn new() -> Self { Self { dict: None } }
    
    pub fn with_dict(dict: BrotliSharedDict) -> Self { Self { dict: Some(dict) } }
    
    pub fn set_dict(&mut self, dict: BrotliSharedDict) { self.dict = Some(dict); }
    
    /// Decompress data
    pub fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, DecompressError> {
        // Simplified: actual brotli would use the dictionary
        // This is a placeholder for the decompression logic
        if data.is_empty() {
            return Ok(Vec::new());
        }
        
        // In reality, we'd call brotli decoder with dictionary
        // For now, just return the data as-is (placeholder)
        Ok(data.to_vec())
    }
    
    pub fn has_dict(&self) -> bool { self.dict.is_some() }
}

/// Decompression error
#[derive(Debug)]
pub enum DecompressError {
    InvalidData,
    DictMismatch,
}

/// Dictionary cache
#[derive(Debug, Default)]
pub struct DictCache {
    dicts: HashMap<DictId, BrotliSharedDict>,
    stats: DictCacheStats,
}

/// Cache stats
#[derive(Debug, Clone, Copy, Default)]
pub struct DictCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub downloads: u64,
}

impl DictCache {
    pub fn new() -> Self { Self::default() }
    
    pub fn get(&mut self, id: DictId) -> Option<&BrotliSharedDict> {
        if self.dicts.contains_key(&id) {
            self.stats.hits += 1;
            self.dicts.get(&id)
        } else {
            self.stats.misses += 1;
            None
        }
    }
    
    pub fn insert(&mut self, dict: BrotliSharedDict) {
        self.dicts.insert(dict.id, dict);
        self.stats.downloads += 1;
    }
    
    pub fn stats(&self) -> &DictCacheStats { &self.stats }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_web_dict() {
        let dict = BrotliSharedDict::web_default();
        assert!(!dict.data().is_empty());
        assert!(!dict.patterns().is_empty());
    }
    
    #[test]
    fn test_builder() {
        let mut builder = DictionaryBuilder::new();
        builder.add_sample("<div class=\"container\"></div>");
        builder.add_sample("<div class=\"wrapper\"></div>");
        
        let dict = builder.build();
        // Should extract common patterns
        assert!(dict.patterns.iter().any(|p| p.contains("div")));
    }
    
    #[test]
    fn test_cache() {
        let mut cache = DictCache::new();
        let dict = BrotliSharedDict::web_default();
        let id = dict.id;
        
        cache.insert(dict);
        assert!(cache.get(id).is_some());
        assert_eq!(cache.stats().hits, 1);
    }
}
