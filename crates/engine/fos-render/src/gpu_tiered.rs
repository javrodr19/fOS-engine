//! GPU Compositing with Tiered Memory
//!
//! Manages GPU textures across Hot/Warm/Cold memory tiers for efficient
//! viewport-based rendering. Uses TieredMem patterns for automatic
//! texture promotion and demotion.
//!
//! # fos-engine Compatibility
//!
//! This module follows the same tiered memory patterns as `fos_engine::tiered_memory`:
//! - `TextureTier` maps to `fos_engine::Tier` (Hot=0, Warm=1, Cold=2)
//! - `TextureViewport` mirrors `fos_engine::TierViewport` API
//! - `TieredTextureStats` mirrors `fos_engine::TieredStats` fields
//!
//! Due to Cargo dependency constraints (fos-engine depends on fos-render),
//! this module implements the patterns directly rather than importing them.
//! Use the `From` trait implementations to convert between types when needed.

use std::collections::HashMap;
use std::time::Instant;

/// Texture ID type
pub type TextureId = u64;

/// Memory tier for textures
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum TextureTier {
    /// Current viewport - uncompressed GPU textures, fastest access
    Hot = 0,
    /// ±2 screens - compressed in RAM, ready for quick upload
    Warm = 1,
    /// Offscreen - heavily compressed or paged out
    Cold = 2,
}

/// Texture data that can be compressed/decompressed
#[derive(Debug, Clone)]
pub struct TieredTexture {
    /// Raw pixel data (RGBA)
    data: Vec<u8>,
    /// Texture width
    pub width: u32,
    /// Texture height
    pub height: u32,
    /// Whether data is compressed
    compressed: bool,
}

impl TieredTexture {
    /// Create a new uncompressed texture
    pub fn new(data: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            data,
            width,
            height,
            compressed: false,
        }
    }
    
    /// Get uncompressed pixel data
    pub fn get_pixels(&self) -> Vec<u8> {
        if self.compressed {
            self.decompress()
        } else {
            self.data.clone()
        }
    }
    
    /// Get raw data reference (may be compressed)
    pub fn raw(&self) -> &[u8] {
        &self.data
    }
    
    /// Compress the texture data (simple RLE)
    pub fn compress(&mut self) {
        if self.compressed {
            return;
        }
        self.data = rle_compress(&self.data);
        self.compressed = true;
    }
    
    /// Decompress the texture data
    fn decompress(&self) -> Vec<u8> {
        if !self.compressed {
            return self.data.clone();
        }
        rle_decompress(&self.data)
    }
    
    /// Memory size in bytes
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.data.len()
    }
    
    /// Is this texture compressed?
    pub fn is_compressed(&self) -> bool {
        self.compressed
    }
}

/// Simple RLE compression
fn rle_compress(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return Vec::new();
    }
    
    let mut result = Vec::new();
    let mut current = data[0];
    let mut count: u8 = 1;
    
    for &byte in &data[1..] {
        if byte == current && count < 255 {
            count += 1;
        } else {
            result.push(count);
            result.push(current);
            current = byte;
            count = 1;
        }
    }
    result.push(count);
    result.push(current);
    
    if result.len() < data.len() {
        result
    } else {
        data.to_vec()
    }
}

fn rle_decompress(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    let mut i = 0;
    while i + 1 < data.len() {
        let count = data[i];
        let byte = data[i + 1];
        for _ in 0..count {
            result.push(byte);
        }
        i += 2;
    }
    result
}

/// Texture position for tier calculation
#[derive(Debug, Clone, Copy)]
pub struct TexturePosition {
    /// Y coordinate (vertical position)
    pub y: f64,
    /// Height of the texture region
    pub height: f64,
}

impl TexturePosition {
    pub fn new(y: f64, height: f64) -> Self {
        Self { y, height }
    }
    
    /// Bottom edge
    pub fn bottom(&self) -> f64 {
        self.y + self.height
    }
}

/// Viewport for tier calculation
#[derive(Debug, Clone, Copy)]
pub struct TextureViewport {
    /// Top of viewport
    pub y: f64,
    /// Height of viewport
    pub height: f64,
    /// Distance considered "warm" above/below viewport
    pub warm_distance: f64,
}

impl TextureViewport {
    pub fn new(y: f64, height: f64) -> Self {
        Self {
            y,
            height,
            warm_distance: height * 2.0, // ±2 screens
        }
    }
    
    /// Calculate tier for a texture position
    pub fn tier_for(&self, pos: &TexturePosition) -> TextureTier {
        let vp_top = self.y;
        let vp_bottom = self.y + self.height;
        let warm_top = vp_top - self.warm_distance;
        let warm_bottom = vp_bottom + self.warm_distance;
        
        // Check if overlaps with viewport
        if pos.bottom() >= vp_top && pos.y <= vp_bottom {
            return TextureTier::Hot;
        }
        
        // Check if in warm zone
        if pos.bottom() >= warm_top && pos.y <= warm_bottom {
            return TextureTier::Warm;
        }
        
        TextureTier::Cold
    }
}

/// Entry in the tiered texture storage
#[derive(Debug)]
struct TextureEntry {
    /// Current tier
    tier: TextureTier,
    /// Texture data
    data: TieredTexture,
    /// GPU handle (if uploaded)
    gpu_handle: Option<u64>,
    /// Last access time
    last_access: Instant,
    /// Position for tier calculation
    position: TexturePosition,
    /// Whether texture needs re-upload
    dirty: bool,
}

/// Statistics for tiered texture management
#[derive(Debug, Clone, Copy, Default)]
pub struct TieredTextureStats {
    pub hot_count: usize,
    pub warm_count: usize,
    pub cold_count: usize,
    pub hot_bytes: usize,
    pub warm_bytes: usize,
    pub cold_bytes: usize,
    pub gpu_memory: usize,
    pub promotions: u64,
    pub demotions: u64,
    pub uploads: u64,
    pub evictions: u64,
}

impl TieredTextureStats {
    /// Total RAM usage
    pub fn ram_usage(&self) -> usize {
        self.hot_bytes + self.warm_bytes + self.cold_bytes
    }
}

/// Tiered texture manager for GPU compositing
#[derive(Debug)]
pub struct TieredTextureManager {
    /// Texture entries
    textures: HashMap<TextureId, TextureEntry>,
    /// Current viewport
    viewport: TextureViewport,
    /// Next texture ID
    next_id: TextureId,
    /// GPU memory budget
    gpu_budget: usize,
    /// RAM budget
    ram_budget: usize,
    /// Statistics
    stats: TieredTextureStats,
}

impl TieredTextureManager {
    /// Create a new tiered texture manager
    pub fn new(gpu_budget: usize, ram_budget: usize) -> Self {
        Self {
            textures: HashMap::new(),
            viewport: TextureViewport::new(0.0, 1000.0),
            next_id: 1,
            gpu_budget,
            ram_budget,
            stats: TieredTextureStats::default(),
        }
    }
    
    /// Create with default budgets (256MB GPU, 512MB RAM)
    pub fn with_defaults() -> Self {
        Self::new(256 * 1024 * 1024, 512 * 1024 * 1024)
    }
    
    /// Update viewport position (triggers tier migration)
    pub fn update_viewport(&mut self, y: f64, height: f64) {
        self.viewport = TextureViewport::new(y, height);
        self.migrate_tiers();
    }
    
    /// Add a texture
    pub fn add_texture(
        &mut self,
        data: Vec<u8>,
        width: u32,
        height: u32,
        position: TexturePosition,
    ) -> TextureId {
        let id = self.next_id;
        self.next_id += 1;
        
        let tier = self.viewport.tier_for(&position);
        let mut texture = TieredTexture::new(data, width, height);
        
        // Compress if not hot
        if tier != TextureTier::Hot {
            texture.compress();
        }
        
        let entry = TextureEntry {
            tier,
            data: texture,
            gpu_handle: None,
            last_access: Instant::now(),
            position,
            dirty: true,
        };
        
        self.textures.insert(id, entry);
        self.update_stats();
        self.enforce_budgets();
        
        id
    }
    
    /// Get texture for rendering (promotes if needed)
    pub fn get_texture(&mut self, id: TextureId) -> Option<&TieredTexture> {
        // First pass: promote if cold
        let needs_promote = self.textures.get(&id).map_or(false, |e| e.tier == TextureTier::Cold);
        if needs_promote {
            self.promote_to_warm(id);
        }
        
        // Update access time
        if let Some(entry) = self.textures.get_mut(&id) {
            entry.last_access = Instant::now();
            return Some(&entry.data);
        }
        
        None
    }
    
    /// Get texture pixels for GPU upload
    pub fn get_pixels_for_upload(&mut self, id: TextureId) -> Option<Vec<u8>> {
        if let Some(entry) = self.textures.get_mut(&id) {
            entry.last_access = Instant::now();
            entry.dirty = false;
            self.stats.uploads += 1;
            return Some(entry.data.get_pixels());
        }
        None
    }
    
    /// Mark texture as uploaded to GPU
    pub fn mark_uploaded(&mut self, id: TextureId, gpu_handle: u64) {
        if let Some(entry) = self.textures.get_mut(&id) {
            entry.gpu_handle = Some(gpu_handle);
            let size = entry.data.width as usize * entry.data.height as usize * 4;
            self.stats.gpu_memory += size;
        }
    }
    
    /// Get GPU handle for a texture
    pub fn get_gpu_handle(&self, id: TextureId) -> Option<u64> {
        self.textures.get(&id).and_then(|e| e.gpu_handle)
    }
    
    /// Remove a texture
    pub fn remove(&mut self, id: TextureId) {
        if let Some(entry) = self.textures.remove(&id) {
            if entry.gpu_handle.is_some() {
                let size = entry.data.width as usize * entry.data.height as usize * 4;
                self.stats.gpu_memory = self.stats.gpu_memory.saturating_sub(size);
                self.stats.evictions += 1;
            }
        }
        self.update_stats();
    }
    
    /// Get textures that need GPU upload (dirty + hot)
    pub fn get_pending_uploads(&self) -> Vec<TextureId> {
        self.textures
            .iter()
            .filter(|(_, e)| e.dirty && e.tier == TextureTier::Hot)
            .map(|(&id, _)| id)
            .collect()
    }
    
    /// Get textures to evict from GPU (no longer hot)
    pub fn get_pending_evictions(&self) -> Vec<TextureId> {
        self.textures
            .iter()
            .filter(|(_, e)| e.gpu_handle.is_some() && e.tier != TextureTier::Hot)
            .map(|(&id, _)| id)
            .collect()
    }
    
    /// Migrate textures between tiers based on viewport
    fn migrate_tiers(&mut self) {
        let mut to_hot = Vec::new();
        let mut to_warm = Vec::new();
        let mut to_cold = Vec::new();
        
        for (&id, entry) in &self.textures {
            let new_tier = self.viewport.tier_for(&entry.position);
            if new_tier != entry.tier {
                match new_tier {
                    TextureTier::Hot => to_hot.push(id),
                    TextureTier::Warm => to_warm.push(id),
                    TextureTier::Cold => to_cold.push(id),
                }
            }
        }
        
        for id in to_hot {
            self.promote_to_hot(id);
        }
        
        for id in to_warm {
            self.demote_to_warm(id);
        }
        
        for id in to_cold {
            self.demote_to_cold(id);
        }
        
        self.update_stats();
    }
    
    fn promote_to_hot(&mut self, id: TextureId) {
        if let Some(entry) = self.textures.get_mut(&id) {
            // Decompress if needed
            if entry.data.is_compressed() {
                let pixels = entry.data.get_pixels();
                entry.data = TieredTexture::new(pixels, entry.data.width, entry.data.height);
            }
            entry.tier = TextureTier::Hot;
            entry.dirty = true; // Needs GPU upload
            self.stats.promotions += 1;
        }
    }
    
    fn promote_to_warm(&mut self, id: TextureId) {
        if let Some(entry) = self.textures.get_mut(&id) {
            if entry.tier == TextureTier::Cold {
                entry.tier = TextureTier::Warm;
                self.stats.promotions += 1;
            }
        }
    }
    
    fn demote_to_warm(&mut self, id: TextureId) {
        if let Some(entry) = self.textures.get_mut(&id) {
            // Evict from GPU
            if entry.gpu_handle.is_some() {
                let size = entry.data.width as usize * entry.data.height as usize * 4;
                self.stats.gpu_memory = self.stats.gpu_memory.saturating_sub(size);
                entry.gpu_handle = None;
                self.stats.evictions += 1;
            }
            
            // Compress
            entry.data.compress();
            entry.tier = TextureTier::Warm;
            self.stats.demotions += 1;
        }
    }
    
    fn demote_to_cold(&mut self, id: TextureId) {
        if let Some(entry) = self.textures.get_mut(&id) {
            // Evict from GPU
            if entry.gpu_handle.is_some() {
                let size = entry.data.width as usize * entry.data.height as usize * 4;
                self.stats.gpu_memory = self.stats.gpu_memory.saturating_sub(size);
                entry.gpu_handle = None;
                self.stats.evictions += 1;
            }
            
            // Compress heavily
            entry.data.compress();
            entry.tier = TextureTier::Cold;
            self.stats.demotions += 1;
        }
    }
    
    fn enforce_budgets(&mut self) {
        // Evict cold textures if over RAM budget
        while self.stats.ram_usage() > self.ram_budget {
            // Find oldest cold texture
            let oldest = self.textures
                .iter()
                .filter(|(_, e)| e.tier == TextureTier::Cold)
                .min_by_key(|(_, e)| e.last_access)
                .map(|(&id, _)| id);
            
            if let Some(id) = oldest {
                self.remove(id);
            } else {
                break;
            }
        }
        
        // Evict from GPU if over budget
        while self.stats.gpu_memory > self.gpu_budget {
            let oldest = self.textures
                .iter()
                .filter(|(_, e)| e.gpu_handle.is_some() && e.tier != TextureTier::Hot)
                .min_by_key(|(_, e)| e.last_access)
                .map(|(&id, _)| id);
            
            if let Some(id) = oldest {
                if let Some(entry) = self.textures.get_mut(&id) {
                    let size = entry.data.width as usize * entry.data.height as usize * 4;
                    self.stats.gpu_memory = self.stats.gpu_memory.saturating_sub(size);
                    entry.gpu_handle = None;
                    self.stats.evictions += 1;
                }
            } else {
                break;
            }
        }
    }
    
    fn update_stats(&mut self) {
        self.stats.hot_count = 0;
        self.stats.warm_count = 0;
        self.stats.cold_count = 0;
        self.stats.hot_bytes = 0;
        self.stats.warm_bytes = 0;
        self.stats.cold_bytes = 0;
        
        for entry in self.textures.values() {
            let size = entry.data.memory_size();
            match entry.tier {
                TextureTier::Hot => {
                    self.stats.hot_count += 1;
                    self.stats.hot_bytes += size;
                }
                TextureTier::Warm => {
                    self.stats.warm_count += 1;
                    self.stats.warm_bytes += size;
                }
                TextureTier::Cold => {
                    self.stats.cold_count += 1;
                    self.stats.cold_bytes += size;
                }
            }
        }
    }
    
    /// Get statistics
    pub fn stats(&self) -> &TieredTextureStats {
        &self.stats
    }
    
    /// Number of textures
    pub fn len(&self) -> usize {
        self.textures.len()
    }
    
    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }
}

impl Default for TieredTextureManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tier_calculation() {
        let viewport = TextureViewport::new(500.0, 1000.0);
        
        // In viewport = hot
        let pos_hot = TexturePosition::new(600.0, 100.0);
        assert_eq!(viewport.tier_for(&pos_hot), TextureTier::Hot);
        
        // Near viewport = warm
        let pos_warm = TexturePosition::new(1600.0, 100.0);
        assert_eq!(viewport.tier_for(&pos_warm), TextureTier::Warm);
        
        // Far from viewport = cold
        let pos_cold = TexturePosition::new(5000.0, 100.0);
        assert_eq!(viewport.tier_for(&pos_cold), TextureTier::Cold);
    }
    
    #[test]
    fn test_texture_manager() {
        let mut mgr = TieredTextureManager::with_defaults();
        
        // Add hot texture (in viewport)
        let hot_id = mgr.add_texture(
            vec![255u8; 100 * 100 * 4],
            100, 100,
            TexturePosition::new(100.0, 100.0),
        );
        
        // Add cold texture (far away)
        let cold_id = mgr.add_texture(
            vec![128u8; 50 * 50 * 4],
            50, 50,
            TexturePosition::new(10000.0, 50.0),
        );
        
        assert_eq!(mgr.stats().hot_count, 1);
        assert_eq!(mgr.stats().cold_count, 1);
        
        // Hot texture should be pending upload
        let pending = mgr.get_pending_uploads();
        assert!(pending.contains(&hot_id));
        assert!(!pending.contains(&cold_id));
    }
    
    #[test]
    fn test_tier_migration() {
        let mut mgr = TieredTextureManager::with_defaults();
        
        // Add texture at current viewport
        let id = mgr.add_texture(
            vec![255u8; 100 * 100 * 4],
            100, 100,
            TexturePosition::new(100.0, 100.0),
        );
        
        assert_eq!(mgr.stats().hot_count, 1);
        
        // Scroll viewport away
        mgr.update_viewport(5000.0, 1000.0);
        
        // Texture should be cold now
        assert_eq!(mgr.stats().hot_count, 0);
        assert!(mgr.stats().cold_count > 0 || mgr.stats().warm_count > 0);
    }
    
    #[test]
    fn test_compression() {
        let data = vec![0u8; 1000]; // Compressible
        let mut tex = TieredTexture::new(data.clone(), 10, 25);
        
        tex.compress();
        assert!(tex.is_compressed());
        assert!(tex.raw().len() < 1000);
        
        let decompressed = tex.get_pixels();
        assert_eq!(decompressed, data);
    }
}
