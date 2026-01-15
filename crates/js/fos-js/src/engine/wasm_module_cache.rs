//! WASM Module Cache
//!
//! Caches compiled WebAssembly modules to avoid expensive recompilation.
//! Supports both instant compilation and streaming compilation states.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::time::Instant;

// ============================================================================
// Module Hash
// ============================================================================

/// Hash of WASM module bytes for cache key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleHash(pub u64);

impl ModuleHash {
    /// Compute hash from module bytes
    pub fn from_bytes(bytes: &[u8]) -> Self {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        Self(hasher.finish())
    }
    
    /// Compute hash from URL and bytes (for uniqueness)
    pub fn from_url_and_bytes(url: &str, bytes: &[u8]) -> Self {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        bytes.hash(&mut hasher);
        Self(hasher.finish())
    }
}

// ============================================================================
// Compiled Module Representation
// ============================================================================

/// Compiled WASM module (cached representation)
#[derive(Debug, Clone)]
pub struct CompiledWasmModule {
    /// Original module hash
    pub hash: ModuleHash,
    /// Compiled bytecode/machine code
    pub compiled_bytes: Vec<u8>,
    /// Number of functions
    pub function_count: u32,
    /// Memory requirements (in pages)
    pub memory_pages: u32,
    /// Table size
    pub table_size: u32,
    /// Compilation time
    pub compile_time_ms: u32,
}

impl CompiledWasmModule {
    pub fn new(hash: ModuleHash, compiled_bytes: Vec<u8>) -> Self {
        Self {
            hash,
            compiled_bytes,
            function_count: 0,
            memory_pages: 0,
            table_size: 0,
            compile_time_ms: 0,
        }
    }
    
    /// Size in bytes
    pub fn size(&self) -> usize {
        self.compiled_bytes.len()
    }
}

// ============================================================================
// Streaming Compilation State
// ============================================================================

/// State of streaming compilation
#[derive(Debug, Clone)]
pub enum StreamingState {
    /// Receiving bytes
    Receiving {
        bytes_received: usize,
        total_bytes: Option<usize>,
        started_at: Instant,
    },
    /// Compiling received bytes
    Compiling {
        bytes_total: usize,
        started_at: Instant,
    },
    /// Compilation complete
    Complete(CompiledWasmModule),
    /// Failed
    Failed(String),
}

impl StreamingState {
    pub fn is_complete(&self) -> bool {
        matches!(self, StreamingState::Complete(_))
    }
    
    pub fn is_failed(&self) -> bool {
        matches!(self, StreamingState::Failed(_))
    }
}

// ============================================================================
// Cache Entry
// ============================================================================

/// Cache entry
#[derive(Debug)]
struct CacheEntry {
    module: CompiledWasmModule,
    last_access: Instant,
    access_count: u32,
}

// ============================================================================
// Cache Statistics
// ============================================================================

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct WasmCacheStats {
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Total compilation time saved (ms)
    pub time_saved_ms: u64,
    /// Current entries
    pub entries: usize,
    /// Current size in bytes
    pub size_bytes: usize,
}

impl WasmCacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

// ============================================================================
// WASM Module Cache
// ============================================================================

/// Cache for compiled WASM modules
pub struct WasmModuleCache {
    /// Compiled modules by hash
    modules: HashMap<ModuleHash, CacheEntry>,
    /// Streaming compilation states by URL
    streaming: HashMap<String, StreamingState>,
    /// Maximum cache size in bytes
    max_size: usize,
    /// Current size
    current_size: usize,
    /// Statistics
    stats: WasmCacheStats,
}

impl Default for WasmModuleCache {
    fn default() -> Self {
        Self::new(64 * 1024 * 1024) // 64MB default
    }
}

impl WasmModuleCache {
    /// Create a new cache with max size in bytes
    pub fn new(max_size: usize) -> Self {
        Self {
            modules: HashMap::new(),
            streaming: HashMap::new(),
            max_size,
            current_size: 0,
            stats: WasmCacheStats::default(),
        }
    }
    
    /// Get a compiled module by hash
    pub fn get(&mut self, hash: &ModuleHash) -> Option<&CompiledWasmModule> {
        if let Some(entry) = self.modules.get_mut(hash) {
            entry.last_access = Instant::now();
            entry.access_count += 1;
            self.stats.hits += 1;
            self.stats.time_saved_ms += entry.module.compile_time_ms as u64;
            return Some(&entry.module);
        }
        self.stats.misses += 1;
        None
    }
    
    /// Check if module is cached
    pub fn contains(&self, hash: &ModuleHash) -> bool {
        self.modules.contains_key(hash)
    }
    
    /// Store a compiled module
    pub fn insert(&mut self, module: CompiledWasmModule) {
        let size = module.size();
        let hash = module.hash;
        
        // Evict if needed
        while self.current_size + size > self.max_size && !self.modules.is_empty() {
            self.evict_lru();
        }
        
        // Don't cache if too large
        if size > self.max_size {
            return;
        }
        
        // Remove existing if present
        if let Some(old) = self.modules.remove(&hash) {
            self.current_size -= old.module.size();
        }
        
        self.current_size += size;
        self.modules.insert(hash, CacheEntry {
            module,
            last_access: Instant::now(),
            access_count: 0,
        });
    }
    
    /// Evict least recently used module
    fn evict_lru(&mut self) {
        if let Some(oldest) = self.modules.iter()
            .min_by_key(|(_, e)| e.last_access)
            .map(|(k, _)| *k)
        {
            if let Some(entry) = self.modules.remove(&oldest) {
                self.current_size -= entry.module.size();
            }
        }
    }
    
    // ========================================================================
    // Streaming Compilation
    // ========================================================================
    
    /// Start streaming compilation for a URL
    pub fn start_streaming(&mut self, url: &str, total_bytes: Option<usize>) {
        self.streaming.insert(url.to_string(), StreamingState::Receiving {
            bytes_received: 0,
            total_bytes,
            started_at: Instant::now(),
        });
    }
    
    /// Update streaming progress
    pub fn update_streaming(&mut self, url: &str, bytes_received: usize) {
        if let Some(StreamingState::Receiving { started_at, total_bytes, .. }) = self.streaming.get(url) {
            let started_at = *started_at;
            let total_bytes = *total_bytes;
            self.streaming.insert(url.to_string(), StreamingState::Receiving {
                bytes_received,
                total_bytes,
                started_at,
            });
        }
    }
    
    /// Mark streaming as compiling
    pub fn mark_compiling(&mut self, url: &str, bytes_total: usize) {
        if let Some(StreamingState::Receiving { started_at, .. }) = self.streaming.get(url) {
            let started_at = *started_at;
            self.streaming.insert(url.to_string(), StreamingState::Compiling {
                bytes_total,
                started_at,
            });
        }
    }
    
    /// Complete streaming with compiled module
    pub fn complete_streaming(&mut self, url: &str, module: CompiledWasmModule) {
        self.streaming.insert(url.to_string(), StreamingState::Complete(module.clone()));
        self.insert(module);
    }
    
    /// Mark streaming as failed
    pub fn fail_streaming(&mut self, url: &str, error: String) {
        self.streaming.insert(url.to_string(), StreamingState::Failed(error));
    }
    
    /// Get streaming state
    pub fn get_streaming(&self, url: &str) -> Option<&StreamingState> {
        self.streaming.get(url)
    }
    
    /// Clear completed/failed streaming states
    pub fn cleanup_streaming(&mut self) {
        self.streaming.retain(|_, state| {
            !matches!(state, StreamingState::Complete(_) | StreamingState::Failed(_))
        });
    }
    
    // ========================================================================
    // Stats and Management
    // ========================================================================
    
    /// Get statistics
    pub fn stats(&self) -> WasmCacheStats {
        WasmCacheStats {
            entries: self.modules.len(),
            size_bytes: self.current_size,
            ..self.stats
        }
    }
    
    /// Clear all cached modules
    pub fn clear(&mut self) {
        self.modules.clear();
        self.streaming.clear();
        self.current_size = 0;
    }
    
    /// Number of cached modules
    pub fn len(&self) -> usize {
        self.modules.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_module_hash() {
        let hash1 = ModuleHash::from_bytes(b"wasm module 1");
        let hash2 = ModuleHash::from_bytes(b"wasm module 1");
        let hash3 = ModuleHash::from_bytes(b"wasm module 2");
        
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
    
    #[test]
    fn test_cache_basic() {
        let mut cache = WasmModuleCache::new(1024 * 1024);
        
        let hash = ModuleHash::from_bytes(b"test");
        let module = CompiledWasmModule::new(hash, vec![0; 100]);
        
        cache.insert(module);
        
        assert!(cache.contains(&hash));
        assert!(cache.get(&hash).is_some());
    }
    
    #[test]
    fn test_cache_eviction() {
        let mut cache = WasmModuleCache::new(200); // Small cache
        
        // Insert 3 modules of 100 bytes each
        for i in 0..3u64 {
            let hash = ModuleHash(i);
            let module = CompiledWasmModule::new(hash, vec![0; 100]);
            cache.insert(module);
        }
        
        // Should have evicted some
        assert!(cache.current_size <= 200);
    }
    
    #[test]
    fn test_streaming_states() {
        let mut cache = WasmModuleCache::new(1024 * 1024);
        
        cache.start_streaming("http://example.com/module.wasm", Some(1000));
        
        let state = cache.get_streaming("http://example.com/module.wasm");
        assert!(matches!(state, Some(StreamingState::Receiving { .. })));
        
        cache.update_streaming("http://example.com/module.wasm", 500);
        cache.mark_compiling("http://example.com/module.wasm", 1000);
        
        let state = cache.get_streaming("http://example.com/module.wasm");
        assert!(matches!(state, Some(StreamingState::Compiling { .. })));
    }
    
    #[test]
    fn test_cache_stats() {
        let mut cache = WasmModuleCache::new(1024 * 1024);
        
        let hash = ModuleHash::from_bytes(b"test");
        let mut module = CompiledWasmModule::new(hash, vec![0; 100]);
        module.compile_time_ms = 50;
        
        cache.insert(module);
        cache.get(&hash); // Hit
        cache.get(&ModuleHash(999)); // Miss
        
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.time_saved_ms, 50);
    }
}
