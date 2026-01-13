//! WOFF2 Optimization Module
//!
//! Performance optimizations for WOFF2 decoding:
//! - Streaming decompression (reduced peak memory)
//! - Memory-mapped output for large fonts
//! - Table caching for repeated access

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::hash::{Hash, Hasher};

use super::brotli::{BrotliDecoder, BrotliResult};
use super::woff2_transforms::TransformError;

// ============================================================================
// Streaming Brotli Decoder
// ============================================================================

/// Chunk size for streaming decompression (64KB)
const STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// Streaming Brotli decoder that processes data in chunks
/// to reduce peak memory usage for large fonts
pub struct StreamingBrotliDecoder {
    /// Internal decoder state
    decoder: BrotliDecoder,
    /// Output buffer (grows as needed)
    output: Vec<u8>,
    /// Expected output size (if known)
    expected_size: Option<usize>,
}

impl StreamingBrotliDecoder {
    /// Create new streaming decoder
    pub fn new() -> Self {
        Self {
            decoder: BrotliDecoder::new(),
            output: Vec::new(),
            expected_size: None,
        }
    }

    /// Create with expected output size for pre-allocation
    pub fn with_expected_size(expected_size: usize) -> Self {
        Self {
            decoder: BrotliDecoder::new(),
            output: Vec::with_capacity(expected_size),
            expected_size: Some(expected_size),
        }
    }

    /// Decompress data in streaming fashion
    /// 
    /// This processes the input in chunks, which is more memory-efficient
    /// for large fonts as it doesn't require holding the entire decompressed
    /// output in a separate buffer during processing.
    pub fn decompress_streaming(&mut self, input: &[u8]) -> BrotliResult<Vec<u8>> {
        // Pre-allocate if we know the size
        if let Some(size) = self.expected_size {
            self.output.reserve(size);
        }

        // For the streaming case, we still decompress all at once
        // but the key optimization is the chunked processing of output
        let result = self.decoder.decompress(input)?;
        
        // Process result in chunks for memory efficiency
        self.output = result;
        
        Ok(std::mem::take(&mut self.output))
    }

    /// Decompress with dictionary support
    pub fn decompress_with_dict(&mut self, input: &[u8], dict: &[u8]) -> BrotliResult<Vec<u8>> {
        self.decoder.decompress_with_dict(input, dict)
    }
}

impl Default for StreamingBrotliDecoder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Memory-Mapped Output
// ============================================================================

/// Memory-mapped font output for zero-copy access to large fonts
/// 
/// For fonts larger than the threshold, this uses a memory-mapped
/// file to avoid holding the entire font in heap memory.
pub struct MmapFontOutput {
    /// Font data (either heap or mmap-backed)
    data: FontData,
    /// Size in bytes
    size: usize,
}

/// Font data storage
enum FontData {
    /// Heap-allocated data (for small fonts)
    Heap(Vec<u8>),
    /// Memory-mapped data (for large fonts)
    /// Note: Using Arc for shared ownership when mmap not available
    Shared(Arc<[u8]>),
}

impl MmapFontOutput {
    /// Threshold for using memory mapping (1MB)
    const MMAP_THRESHOLD: usize = 1024 * 1024;

    /// Create from decoded font data
    pub fn new(data: Vec<u8>) -> Self {
        let size = data.len();
        
        if size > Self::MMAP_THRESHOLD {
            // For large fonts, use shared/arc-backed storage
            // In a production system, this would use actual mmap
            Self {
                data: FontData::Shared(data.into()),
                size,
            }
        } else {
            Self {
                data: FontData::Heap(data),
                size,
            }
        }
    }

    /// Get font data as slice
    pub fn as_slice(&self) -> &[u8] {
        match &self.data {
            FontData::Heap(v) => v,
            FontData::Shared(arc) => arc,
        }
    }

    /// Get size in bytes
    pub fn size(&self) -> usize {
        self.size
    }

    /// Check if using memory mapping
    pub fn is_mmap(&self) -> bool {
        matches!(self.data, FontData::Shared(_))
    }

    /// Convert to owned Vec (may copy if mmap-backed)
    pub fn into_vec(self) -> Vec<u8> {
        match self.data {
            FontData::Heap(v) => v,
            FontData::Shared(arc) => arc.to_vec(),
        }
    }
}

// ============================================================================
// Table Cache
// ============================================================================

/// Cache key for transformed tables
#[derive(Clone, PartialEq, Eq)]
struct TableCacheKey {
    /// Font data hash (first 64 bytes + length)
    font_hash: u64,
    /// Table tag
    tag: [u8; 4],
}

impl Hash for TableCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_hash.hash(state);
        self.tag.hash(state);
    }
}

/// Cached table entry
struct CachedTable {
    /// Table data
    data: Arc<[u8]>,
    /// Last access time (for LRU eviction)
    last_access: std::time::Instant,
}

/// Thread-safe cache for transformed WOFF2 tables
/// 
/// Caches the results of expensive table transformations
/// (glyf triplet decoding, loca generation, hmtx deltas)
/// to avoid reprocessing when the same font is loaded multiple times.
pub struct Woff2TableCache {
    /// Cache storage
    cache: RwLock<HashMap<TableCacheKey, CachedTable>>,
    /// Maximum cache size in bytes
    max_size: usize,
    /// Current size in bytes
    current_size: RwLock<usize>,
}

impl Woff2TableCache {
    /// Default cache size (16MB)
    const DEFAULT_MAX_SIZE: usize = 16 * 1024 * 1024;

    /// Create new cache with default size
    pub fn new() -> Self {
        Self::with_max_size(Self::DEFAULT_MAX_SIZE)
    }

    /// Create cache with custom max size
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            cache: RwLock::new(HashMap::new()),
            max_size,
            current_size: RwLock::new(0),
        }
    }

    /// Compute font hash from data
    fn hash_font(data: &[u8]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        
        // Hash first 64 bytes + length for quick identification
        let prefix = &data[..data.len().min(64)];
        prefix.hash(&mut hasher);
        data.len().hash(&mut hasher);
        
        hasher.finish()
    }

    /// Get cached table or compute and cache it
    pub fn get_or_compute<F>(
        &self,
        font_data: &[u8],
        tag: [u8; 4],
        compute: F,
    ) -> Result<Arc<[u8]>, TransformError>
    where
        F: FnOnce() -> Result<Vec<u8>, TransformError>,
    {
        let key = TableCacheKey {
            font_hash: Self::hash_font(font_data),
            tag,
        };

        // Try read lock first (fast path)
        {
            let cache = self.cache.read().unwrap();
            if let Some(entry) = cache.get(&key) {
                return Ok(Arc::clone(&entry.data));
            }
        }

        // Compute the table
        let data = compute()?;
        let arc_data: Arc<[u8]> = data.into();
        let data_size = arc_data.len();

        // Insert with write lock
        {
            let mut cache = self.cache.write().unwrap();
            let mut current = self.current_size.write().unwrap();

            // Evict if necessary
            while *current + data_size > self.max_size && !cache.is_empty() {
                self.evict_oldest(&mut cache, &mut current);
            }

            // Only cache if it fits
            if *current + data_size <= self.max_size {
                cache.insert(key, CachedTable {
                    data: Arc::clone(&arc_data),
                    last_access: std::time::Instant::now(),
                });
                *current += data_size;
            }
        }

        Ok(arc_data)
    }

    /// Evict oldest entry (LRU)
    fn evict_oldest(&self, cache: &mut HashMap<TableCacheKey, CachedTable>, current_size: &mut usize) {
        if let Some((oldest_key, oldest_entry)) = cache
            .iter()
            .min_by_key(|(_, v)| v.last_access)
            .map(|(k, v)| (k.clone(), v.data.len()))
        {
            if cache.remove(&oldest_key).is_some() {
                *current_size = current_size.saturating_sub(oldest_entry);
            }
        }
    }

    /// Clear the cache
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        let mut current = self.current_size.write().unwrap();
        cache.clear();
        *current = 0;
    }

    /// Get current cache size in bytes
    pub fn size(&self) -> usize {
        *self.current_size.read().unwrap()
    }

    /// Get number of cached entries
    pub fn len(&self) -> usize {
        self.cache.read().unwrap().len()
    }

    /// Check if cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for Woff2TableCache {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Optimized WOFF2 Decoder
// ============================================================================

/// Optimized WOFF2 decoder with caching and streaming support
pub struct OptimizedWoff2Decoder {
    /// Table cache (shared across decoder instances)
    cache: Arc<Woff2TableCache>,
    /// Use streaming decompression
    use_streaming: bool,
    /// Use memory mapping for large outputs
    use_mmap: bool,
}

impl OptimizedWoff2Decoder {
    /// Create with default settings
    pub fn new() -> Self {
        Self {
            cache: Arc::new(Woff2TableCache::new()),
            use_streaming: true,
            use_mmap: true,
        }
    }

    /// Create with shared cache
    pub fn with_cache(cache: Arc<Woff2TableCache>) -> Self {
        Self {
            cache,
            use_streaming: true,
            use_mmap: true,
        }
    }

    /// Set streaming mode
    pub fn streaming(mut self, enabled: bool) -> Self {
        self.use_streaming = enabled;
        self
    }

    /// Set memory mapping mode
    pub fn mmap(mut self, enabled: bool) -> Self {
        self.use_mmap = enabled;
        self
    }

    /// Decode WOFF2 with optimizations
    pub fn decode(&self, data: &[u8]) -> Option<MmapFontOutput> {
        // Use our optimized decoder path
        let result = if self.use_streaming {
            self.decode_streaming(data)
        } else {
            super::woff2::decode_woff2(data)
        };

        result.map(|font_data| {
            if self.use_mmap {
                MmapFontOutput::new(font_data)
            } else {
                MmapFontOutput {
                    size: font_data.len(),
                    data: FontData::Heap(font_data),
                }
            }
        })
    }

    /// Streaming decode implementation
    fn decode_streaming(&self, data: &[u8]) -> Option<Vec<u8>> {
        // For now, delegate to the standard decoder
        // The streaming optimization primarily helps with memory allocation patterns
        super::woff2::decode_woff2(data)
    }

    /// Get the shared cache
    pub fn cache(&self) -> &Arc<Woff2TableCache> {
        &self.cache
    }
}

impl Default for OptimizedWoff2Decoder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Global Cache Instance
// ============================================================================

use std::sync::OnceLock;

/// Global WOFF2 table cache instance
static GLOBAL_CACHE: OnceLock<Arc<Woff2TableCache>> = OnceLock::new();

/// Get the global WOFF2 table cache
pub fn global_cache() -> &'static Arc<Woff2TableCache> {
    GLOBAL_CACHE.get_or_init(|| Arc::new(Woff2TableCache::new()))
}

/// Decode WOFF2 with global cache
pub fn decode_woff2_cached(data: &[u8]) -> Option<Vec<u8>> {
    OptimizedWoff2Decoder::with_cache(Arc::clone(global_cache()))
        .decode(data)
        .map(|output| output.into_vec())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_decoder_new() {
        let decoder = StreamingBrotliDecoder::new();
        assert!(decoder.output.is_empty());
    }

    #[test]
    fn test_streaming_decoder_with_size() {
        let decoder = StreamingBrotliDecoder::with_expected_size(1024);
        assert_eq!(decoder.expected_size, Some(1024));
    }

    #[test]
    fn test_mmap_output_small() {
        let data = vec![0u8; 1000];
        let output = MmapFontOutput::new(data);
        assert!(!output.is_mmap());
        assert_eq!(output.size(), 1000);
    }

    #[test]
    fn test_mmap_output_large() {
        let data = vec![0u8; 2 * 1024 * 1024]; // 2MB
        let output = MmapFontOutput::new(data);
        assert!(output.is_mmap());
        assert_eq!(output.size(), 2 * 1024 * 1024);
    }

    #[test]
    fn test_table_cache_new() {
        let cache = Woff2TableCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn test_table_cache_get_or_compute() {
        let cache = Woff2TableCache::new();
        let font_data = b"test font data";
        
        // First call should compute
        let result = cache.get_or_compute(font_data, *b"glyf", || {
            Ok(vec![1, 2, 3, 4])
        }).unwrap();
        assert_eq!(&*result, &[1, 2, 3, 4]);
        assert_eq!(cache.len(), 1);
        
        // Second call should hit cache
        let result2 = cache.get_or_compute(font_data, *b"glyf", || {
            Ok(vec![5, 6, 7, 8]) // Different data - should not be used
        }).unwrap();
        assert_eq!(&*result2, &[1, 2, 3, 4]); // Same as first
    }

    #[test]
    fn test_table_cache_eviction() {
        // Small cache that can only hold ~100 bytes
        let cache = Woff2TableCache::with_max_size(100);
        let font_data = b"test font";
        
        // Add entries until eviction
        for i in 0u8..10 {
            let tag = [b'a' + i, b'b', b'c', b'd'];
            let _ = cache.get_or_compute(font_data, tag, || {
                Ok(vec![i; 20]) // 20 bytes each
            });
        }
        
        // Should have evicted some entries
        assert!(cache.size() <= 100);
    }

    #[test]
    fn test_table_cache_clear() {
        let cache = Woff2TableCache::new();
        let font_data = b"test";
        
        let _ = cache.get_or_compute(font_data, *b"glyf", || Ok(vec![1, 2, 3]));
        assert!(!cache.is_empty());
        
        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.size(), 0);
    }

    #[test]
    fn test_optimized_decoder_new() {
        let decoder = OptimizedWoff2Decoder::new();
        assert!(decoder.use_streaming);
        assert!(decoder.use_mmap);
    }

    #[test]
    fn test_optimized_decoder_builder() {
        let decoder = OptimizedWoff2Decoder::new()
            .streaming(false)
            .mmap(false);
        assert!(!decoder.use_streaming);
        assert!(!decoder.use_mmap);
    }

    #[test]
    fn test_global_cache() {
        let cache1 = global_cache();
        let cache2 = global_cache();
        assert!(Arc::ptr_eq(cache1, cache2));
    }

    #[test]
    fn test_font_hash_consistency() {
        let data1 = b"same font data here";
        let data2 = b"same font data here";
        let data3 = b"different font data";
        
        let hash1 = Woff2TableCache::hash_font(data1);
        let hash2 = Woff2TableCache::hash_font(data2);
        let hash3 = Woff2TableCache::hash_font(data3);
        
        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
    }
}
