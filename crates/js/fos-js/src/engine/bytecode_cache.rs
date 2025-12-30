//! Bytecode Caching
//!
//! Cache compiled bytecode to avoid recompilation.
//! Uses hash of source code for cache invalidation.

use super::bytecode::Bytecode;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Bytecode cache entry
#[derive(Debug, Clone)]
struct CacheEntry {
    bytecode: Bytecode,
    source_hash: u64,
}

/// Bytecode cache manager
#[derive(Debug, Default)]
pub struct BytecodeCache {
    entries: HashMap<String, CacheEntry>,
    hits: u64,
    misses: u64,
}

impl BytecodeCache {
    pub fn new() -> Self { Self::default() }
    
    /// Compute hash of source code
    fn hash_source(source: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Get cached bytecode if valid
    pub fn get(&mut self, filename: &str, source: &str) -> Option<&Bytecode> {
        let source_hash = Self::hash_source(source);
        if let Some(entry) = self.entries.get(filename) {
            if entry.source_hash == source_hash {
                self.hits += 1;
                return Some(&entry.bytecode);
            }
        }
        self.misses += 1;
        None
    }
    
    /// Store compiled bytecode
    pub fn put(&mut self, filename: String, source: &str, bytecode: Bytecode) {
        let source_hash = Self::hash_source(source);
        self.entries.insert(filename, CacheEntry { bytecode, source_hash });
    }
    
    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> (u64, u64) { (self.hits, self.misses) }
    
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

/// Global bytecode cache (thread-local)
thread_local! {
    static BYTECODE_CACHE: std::cell::RefCell<BytecodeCache> = std::cell::RefCell::new(BytecodeCache::new());
}

/// Get or compile bytecode with caching
pub fn get_or_compile<F>(filename: &str, source: &str, compile_fn: F) -> Bytecode
where
    F: FnOnce(&str) -> Result<Bytecode, String>,
{
    BYTECODE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        
        // Check cache first
        if let Some(bytecode) = cache.get(filename, source) {
            return bytecode.clone();
        }
        
        // Cache miss - compile and store
        match compile_fn(source) {
            Ok(bytecode) => {
                cache.put(filename.to_string(), source, bytecode.clone());
                bytecode
            }
            Err(_) => Bytecode::default(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_hit() {
        let mut cache = BytecodeCache::new();
        let bytecode = Bytecode::default();
        cache.put("test.js".to_string(), "let x = 1;", bytecode.clone());
        
        assert!(cache.get("test.js", "let x = 1;").is_some());
        assert!(cache.get("test.js", "let x = 2;").is_none()); // Different source
    }
}
