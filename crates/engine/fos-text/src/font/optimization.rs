//! Font Memory Optimization
//!
//! Utilities for reducing font memory usage including subsetting,
//! glyph streaming, shared caching, and memory mapping.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use std::path::PathBuf;

/// Font subsetting - keep only used glyphs
#[derive(Debug)]
pub struct FontSubsetter {
    /// Used glyph IDs
    used_glyphs: HashSet<u16>,
    /// Used Unicode codepoints
    used_codepoints: HashSet<char>,
    /// Source font data
    source_data: Vec<u8>,
}

impl FontSubsetter {
    /// Create a new subsetter for font data
    pub fn new(font_data: Vec<u8>) -> Self {
        Self {
            used_glyphs: HashSet::new(),
            used_codepoints: HashSet::new(),
            source_data: font_data,
        }
    }
    
    /// Mark a glyph as used
    pub fn use_glyph(&mut self, glyph_id: u16) {
        self.used_glyphs.insert(glyph_id);
    }
    
    /// Mark a codepoint as used
    pub fn use_codepoint(&mut self, c: char) {
        self.used_codepoints.insert(c);
    }
    
    /// Mark all codepoints in text as used
    pub fn use_text(&mut self, text: &str) {
        for c in text.chars() {
            self.used_codepoints.insert(c);
        }
    }
    
    /// Get number of used glyphs
    pub fn used_glyph_count(&self) -> usize {
        self.used_glyphs.len()
    }
    
    /// Create subsetted font containing only used glyphs
    /// Returns the subsetted font data
    pub fn subset(&self) -> Result<Vec<u8>, SubsetError> {
        if self.used_glyphs.is_empty() && self.used_codepoints.is_empty() {
            return Err(SubsetError::NoGlyphsUsed);
        }
        
        // In a real implementation, this would use a font subsetting library
        // like subsetter or fonttools to create a minimal font
        // For now, return placeholder
        Ok(self.source_data.clone())
    }
    
    /// Estimate size reduction percentage
    pub fn estimate_reduction(&self, total_glyphs: usize) -> f32 {
        if total_glyphs == 0 {
            return 0.0;
        }
        let used = self.used_glyphs.len().max(self.used_codepoints.len());
        (1.0 - (used as f32 / total_glyphs as f32)) * 100.0
    }
}

/// Font subsetting errors
#[derive(Debug, thiserror::Error)]
pub enum SubsetError {
    #[error("No glyphs marked as used")]
    NoGlyphsUsed,
    #[error("Invalid font data")]
    InvalidFont,
    #[error("Subsetting failed: {0}")]
    SubsetFailed(String),
}

/// Glyph streaming - load glyphs on demand
#[derive(Debug)]
pub struct GlyphStreamer {
    /// Font file path for streaming
    font_path: PathBuf,
    /// Loaded glyph data
    loaded_glyphs: HashMap<u16, GlyphData>,
    /// Maximum cached glyphs
    max_cached: usize,
    /// Access order for LRU eviction
    access_order: Vec<u16>,
}

/// Glyph data container
#[derive(Debug, Clone)]
pub struct GlyphData {
    /// Glyph ID
    pub id: u16,
    /// Advance width
    pub advance: f32,
    /// Left side bearing
    pub lsb: f32,
    /// Bounding box
    pub bbox: (f32, f32, f32, f32),
    /// Outline data (if loaded)
    pub outline: Option<Vec<u8>>,
}

impl GlyphStreamer {
    /// Create a new glyph streamer
    pub fn new(font_path: PathBuf, max_cached: usize) -> Self {
        Self {
            font_path,
            loaded_glyphs: HashMap::new(),
            max_cached,
            access_order: Vec::new(),
        }
    }
    
    /// Get glyph, loading if necessary
    pub fn get_glyph(&mut self, glyph_id: u16) -> Option<&GlyphData> {
        // Update access order for LRU
        self.access_order.retain(|&id| id != glyph_id);
        self.access_order.push(glyph_id);
        
        // Evict if over limit
        while self.loaded_glyphs.len() > self.max_cached {
            if let Some(oldest) = self.access_order.first().copied() {
                self.access_order.remove(0);
                self.loaded_glyphs.remove(&oldest);
            }
        }
        
        // Load if not cached
        if !self.loaded_glyphs.contains_key(&glyph_id) {
            if let Some(data) = self.load_glyph(glyph_id) {
                self.loaded_glyphs.insert(glyph_id, data);
            }
        }
        
        self.loaded_glyphs.get(&glyph_id)
    }
    
    /// Load glyph from font file
    fn load_glyph(&self, _glyph_id: u16) -> Option<GlyphData> {
        // In a real implementation, this would:
        // 1. Seek to the glyf table entry for this glyph
        // 2. Read only the required data
        // 3. Parse the outline
        
        // Placeholder
        None
    }
    
    /// Preload specific glyphs
    pub fn preload(&mut self, glyph_ids: &[u16]) {
        for &id in glyph_ids {
            self.get_glyph(id);
        }
    }
    
    /// Get cache statistics
    pub fn cache_stats(&self) -> (usize, usize) {
        (self.loaded_glyphs.len(), self.max_cached)
    }
}

/// Shared font cache across tabs/pages
#[derive(Debug, Clone)]
pub struct SharedFontCache {
    /// Cached fonts by hash
    fonts: Arc<RwLock<HashMap<u64, Arc<CachedFont>>>>,
    /// Maximum total size
    max_size: usize,
    /// Current total size
    current_size: Arc<RwLock<usize>>,
}

/// Cached font entry
#[derive(Debug)]
pub struct CachedFont {
    /// Font data
    pub data: Vec<u8>,
    /// Font family name
    pub family: String,
    /// Reference count
    pub ref_count: u32,
    /// Last access time
    pub last_access: std::time::Instant,
}

impl SharedFontCache {
    /// Create a new shared font cache
    pub fn new(max_size: usize) -> Self {
        Self {
            fonts: Arc::new(RwLock::new(HashMap::new())),
            max_size,
            current_size: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Get or load a font
    pub fn get_or_load<F>(&self, hash: u64, loader: F) -> Option<Arc<CachedFont>>
    where
        F: FnOnce() -> Option<Vec<u8>>,
    {
        // Check cache first
        {
            let fonts = self.fonts.read().ok()?;
            if let Some(font) = fonts.get(&hash) {
                return Some(Arc::clone(font));
            }
        }
        
        // Load and cache
        let data = loader()?;
        let size = data.len();
        
        // Evict if needed
        self.evict_if_needed(size);
        
        let font = Arc::new(CachedFont {
            data,
            family: String::new(),
            ref_count: 1,
            last_access: std::time::Instant::now(),
        });
        
        {
            let mut fonts = self.fonts.write().ok()?;
            fonts.insert(hash, Arc::clone(&font));
            
            let mut current = self.current_size.write().ok()?;
            *current += size;
        }
        
        Some(font)
    }
    
    /// Evict fonts if cache is full
    fn evict_if_needed(&self, needed_size: usize) {
        let current = match self.current_size.read() {
            Ok(guard) => *guard,
            Err(_) => 0,
        };
        if current + needed_size <= self.max_size {
            return;
        }
        
        // LRU eviction
        let mut fonts = match self.fonts.write() {
            Ok(f) => f,
            Err(_) => return,
        };
        
        let mut entries: Vec<_> = fonts.iter()
            .map(|(k, v)| (*k, v.last_access, v.data.len()))
            .collect();
        entries.sort_by_key(|(_, time, _)| *time);
        
        let mut freed = 0;
        for (key, _, size) in entries {
            if current - freed + needed_size <= self.max_size {
                break;
            }
            fonts.remove(&key);
            freed += size;
        }
        
        if let Ok(mut c) = self.current_size.write() {
            *c -= freed;
        }
    }
    
    /// Clear the cache
    pub fn clear(&self) {
        if let Ok(mut fonts) = self.fonts.write() {
            fonts.clear();
        }
        if let Ok(mut size) = self.current_size.write() {
            *size = 0;
        }
    }
    
    /// Get cache size
    pub fn size(&self) -> usize {
        match self.current_size.read() {
            Ok(guard) => *guard,
            Err(_) => 0,
        }
    }
}

/// Memory-mapped font file
#[derive(Debug)]
pub struct MmapFont {
    /// Path to font file
    pub path: PathBuf,
    /// File size
    pub size: usize,
    /// Memory-mapped data (placeholder - would use memmap2 in real impl)
    data: Option<Vec<u8>>,
}

impl MmapFont {
    /// Open a font file with memory mapping
    pub fn open(path: PathBuf) -> Result<Self, std::io::Error> {
        let metadata = std::fs::metadata(&path)?;
        let size = metadata.len() as usize;
        
        // In a real implementation, would use memmap2:
        // let file = std::fs::File::open(&path)?;
        // let mmap = unsafe { memmap2::Mmap::map(&file)? };
        
        Ok(Self {
            path,
            size,
            data: None, // Would be mmap
        })
    }
    
    /// Get font data slice
    pub fn as_bytes(&self) -> Option<&[u8]> {
        self.data.as_deref()
    }
    
    /// Prefetch font data into memory
    pub fn prefetch(&self, _offset: usize, _length: usize) {
        // In a real implementation:
        // Use madvise(MADV_WILLNEED) or similar
    }
    
    /// Get file size
    pub fn file_size(&self) -> usize {
        self.size
    }
}

/// Flyweight pattern for glyph metrics
#[derive(Debug, Default)]
pub struct GlyphMetricsCache {
    /// Shared metrics by glyph ID
    metrics: HashMap<u16, Arc<GlyphMetrics>>,
}

/// Shared glyph metrics
#[derive(Debug, Clone)]
pub struct GlyphMetrics {
    pub advance_width: f32,
    pub left_side_bearing: f32,
    pub bbox_x_min: f32,
    pub bbox_y_min: f32,
    pub bbox_x_max: f32,
    pub bbox_y_max: f32,
}

impl GlyphMetricsCache {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get or insert metrics
    pub fn get_or_insert(&mut self, glyph_id: u16, metrics: GlyphMetrics) -> Arc<GlyphMetrics> {
        self.metrics
            .entry(glyph_id)
            .or_insert_with(|| Arc::new(metrics))
            .clone()
    }
    
    /// Get metrics if cached
    pub fn get(&self, glyph_id: u16) -> Option<Arc<GlyphMetrics>> {
        self.metrics.get(&glyph_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_font_subsetter() {
        let mut subsetter = FontSubsetter::new(vec![0; 100]);
        subsetter.use_text("Hello");
        assert_eq!(subsetter.used_codepoints.len(), 4); // H, e, l, o
    }
    
    #[test]
    fn test_glyph_streamer() {
        let streamer = GlyphStreamer::new(PathBuf::from("test.ttf"), 100);
        assert_eq!(streamer.cache_stats(), (0, 100));
    }
    
    #[test]
    fn test_shared_font_cache() {
        let cache = SharedFontCache::new(1024 * 1024);
        assert_eq!(cache.size(), 0);
    }
    
    #[test]
    fn test_glyph_metrics_cache() {
        let mut cache = GlyphMetricsCache::new();
        let metrics = GlyphMetrics {
            advance_width: 10.0,
            left_side_bearing: 1.0,
            bbox_x_min: 0.0,
            bbox_y_min: 0.0,
            bbox_x_max: 9.0,
            bbox_y_max: 12.0,
        };
        
        let m1 = cache.get_or_insert(1, metrics.clone());
        let m2 = cache.get_or_insert(1, metrics);
        
        // Should be the same Arc
        assert!(Arc::ptr_eq(&m1, &m2));
    }
}
