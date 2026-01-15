//! Bytecode Caching
//!
//! Cache compiled bytecode to avoid recompilation.
//! Supports both in-memory and disk-backed caching.
//! Uses hash of source code + URL for cache invalidation.

use super::bytecode::Bytecode;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use std::path::PathBuf;
use std::fs;
use std::io::{Read, Write};

// ============================================================================
// Script Hash
// ============================================================================

/// Hash combining source and URL for unique identification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScriptHash(pub u64);

impl ScriptHash {
    /// Compute hash from source only
    pub fn from_source(source: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        Self(hasher.finish())
    }
    
    /// Compute hash from source and URL (for uniqueness)
    pub fn from_source_and_url(source: &str, url: &str) -> Self {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        url.hash(&mut hasher);
        Self(hasher.finish())
    }
}

// ============================================================================
// Disk Cache
// ============================================================================

/// Disk-backed cache for compiled scripts
pub struct DiskCache {
    /// Cache directory
    cache_dir: PathBuf,
    /// Maximum cache size in bytes
    max_size: usize,
    /// Current cache size
    current_size: usize,
}

impl DiskCache {
    /// Create a new disk cache
    pub fn new(cache_dir: PathBuf, max_size: usize) -> std::io::Result<Self> {
        fs::create_dir_all(&cache_dir)?;
        
        // Calculate current size
        let current_size = Self::calculate_dir_size(&cache_dir);
        
        Ok(Self {
            cache_dir,
            max_size,
            current_size,
        })
    }
    
    /// Calculate directory size
    fn calculate_dir_size(dir: &PathBuf) -> usize {
        fs::read_dir(dir)
            .map(|entries| {
                entries.filter_map(|e| e.ok())
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len() as usize)
                    .sum()
            })
            .unwrap_or(0)
    }
    
    /// Get path for a hash
    fn path_for(&self, hash: &ScriptHash) -> PathBuf {
        self.cache_dir.join(format!("{:016x}.bc", hash.0))
    }
    
    /// Get cached bytecode from disk
    pub fn get(&self, hash: &ScriptHash) -> Option<Vec<u8>> {
        let path = self.path_for(hash);
        fs::read(&path).ok()
    }
    
    /// Store bytecode to disk
    pub fn put(&mut self, hash: ScriptHash, data: &[u8]) -> std::io::Result<()> {
        // Evict if needed
        while self.current_size + data.len() > self.max_size {
            if !self.evict_oldest() {
                break;
            }
        }
        
        let path = self.path_for(&hash);
        fs::write(&path, data)?;
        self.current_size += data.len();
        Ok(())
    }
    
    /// Evict oldest file
    fn evict_oldest(&mut self) -> bool {
        let entries: Vec<_> = fs::read_dir(&self.cache_dir)
            .ok()
            .map(|e| e.filter_map(|e| e.ok()).collect())
            .unwrap_or_default();
        
        // Find oldest by modification time
        let oldest = entries.iter()
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                let mtime = meta.modified().ok()?;
                Some((e.path(), mtime, meta.len() as usize))
            })
            .min_by_key(|(_, mtime, _)| *mtime);
        
        if let Some((path, _, size)) = oldest {
            if fs::remove_file(&path).is_ok() {
                self.current_size = self.current_size.saturating_sub(size);
                return true;
            }
        }
        false
    }
    
    /// Check if hash exists on disk
    pub fn contains(&self, hash: &ScriptHash) -> bool {
        self.path_for(hash).exists()
    }
    
    /// Clear disk cache
    pub fn clear(&mut self) -> std::io::Result<()> {
        for entry in fs::read_dir(&self.cache_dir)? {
            if let Ok(entry) = entry {
                let _ = fs::remove_file(entry.path());
            }
        }
        self.current_size = 0;
        Ok(())
    }
}

// ============================================================================
// Bytecode Cache
// ============================================================================

/// Bytecode cache entry
#[derive(Debug, Clone)]
struct CacheEntry {
    bytecode: Bytecode,
    source_hash: u64,
}

/// Bytecode cache manager with optional disk backing
#[derive(Debug, Default)]
pub struct BytecodeCache {
    /// In-memory cache (L1)
    entries: HashMap<String, CacheEntry>,
    /// Disk cache path (optional)
    disk_path: Option<PathBuf>,
    /// Cache statistics
    hits: u64,
    misses: u64,
    disk_hits: u64,
}

impl BytecodeCache {
    pub fn new() -> Self { Self::default() }
    
    /// Create with disk persistence
    pub fn with_disk_cache(cache_dir: PathBuf) -> Self {
        Self {
            entries: HashMap::new(),
            disk_path: Some(cache_dir),
            hits: 0,
            misses: 0,
            disk_hits: 0,
        }
    }
    
    /// Compute hash of source code
    fn hash_source(source: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Get cached bytecode if valid (checks memory then disk)
    pub fn get(&mut self, filename: &str, source: &str) -> Option<&Bytecode> {
        let source_hash = Self::hash_source(source);
        
        // Check memory cache first
        if let Some(entry) = self.entries.get(filename) {
            if entry.source_hash == source_hash {
                self.hits += 1;
                return Some(&entry.bytecode);
            }
        }
        
        self.misses += 1;
        None
    }
    
    /// Get from disk cache (requires deserialization by caller)
    pub fn get_from_disk(&mut self, source: &str, url: &str) -> Option<Vec<u8>> {
        let Some(ref disk_path) = self.disk_path else {
            return None;
        };
        
        let hash = ScriptHash::from_source_and_url(source, url);
        let path = disk_path.join(format!("{:016x}.bc", hash.0));
        
        if let Ok(data) = fs::read(&path) {
            self.disk_hits += 1;
            return Some(data);
        }
        
        None
    }
    
    /// Store to disk cache
    pub fn store_to_disk(&self, source: &str, url: &str, compiled_bytes: &[u8]) -> std::io::Result<()> {
        let Some(ref disk_path) = self.disk_path else {
            return Ok(());
        };
        
        fs::create_dir_all(disk_path)?;
        
        let hash = ScriptHash::from_source_and_url(source, url);
        let path = disk_path.join(format!("{:016x}.bc", hash.0));
        fs::write(&path, compiled_bytes)
    }
    
    /// Store compiled bytecode in memory
    pub fn put(&mut self, filename: String, source: &str, bytecode: Bytecode) {
        let source_hash = Self::hash_source(source);
        self.entries.insert(filename, CacheEntry { bytecode, source_hash });
    }
    
    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    
    /// Clear disk cache too
    pub fn clear_disk(&mut self) -> std::io::Result<()> {
        if let Some(ref disk_path) = self.disk_path {
            for entry in fs::read_dir(disk_path)? {
                if let Ok(entry) = entry {
                    if entry.path().extension().map(|e| e == "bc").unwrap_or(false) {
                        let _ = fs::remove_file(entry.path());
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            memory_hits: self.hits,
            disk_hits: self.disk_hits,
            misses: self.misses,
            memory_entries: self.entries.len(),
        }
    }
    
    /// Compute hit rate (memory only)
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
    
    /// Compute total hit rate (memory + disk)
    pub fn total_hit_rate(&self) -> f64 {
        let total = self.hits + self.disk_hits + self.misses;
        if total == 0 { 0.0 } else { (self.hits + self.disk_hits) as f64 / total as f64 }
    }
}

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    pub memory_hits: u64,
    pub disk_hits: u64,
    pub misses: u64,
    pub memory_entries: usize,
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
    
    #[test]
    fn test_script_hash() {
        let h1 = ScriptHash::from_source("code");
        let h2 = ScriptHash::from_source("code");
        let h3 = ScriptHash::from_source("different");
        
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
    }
    
    #[test]
    fn test_script_hash_with_url() {
        let h1 = ScriptHash::from_source_and_url("code", "http://a.com");
        let h2 = ScriptHash::from_source_and_url("code", "http://b.com");
        
        assert_ne!(h1, h2); // Same code, different URL
    }
    
    #[test]
    fn test_cache_stats() {
        let mut cache = BytecodeCache::new();
        let bytecode = Bytecode::default();
        
        cache.put("test.js".to_string(), "code", bytecode);
        cache.get("test.js", "code"); // Hit
        cache.get("test.js", "other"); // Miss
        
        let stats = cache.stats();
        assert_eq!(stats.memory_hits, 1);
        assert_eq!(stats.misses, 1);
    }
}

