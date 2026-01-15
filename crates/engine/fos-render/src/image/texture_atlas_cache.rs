//! Texture Atlas Cache
//!
//! Packs multiple small images into texture atlases for efficient GPU rendering.
//! Reduces draw calls and GPU state changes.

use std::collections::HashMap;

// ============================================================================
// Atlas Entry
// ============================================================================

/// Entry in a texture atlas
#[derive(Debug, Clone, Copy)]
pub struct AtlasEntry {
    /// Atlas index (which atlas this is in)
    pub atlas_id: u16,
    /// X position in atlas
    pub x: u16,
    /// Y position in atlas  
    pub y: u16,
    /// Width in pixels
    pub width: u16,
    /// Height in pixels
    pub height: u16,
    /// UV coordinates (normalized 0-1)
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

impl AtlasEntry {
    pub fn uv_rect(&self) -> (f32, f32, f32, f32) {
        (self.u0, self.v0, self.u1, self.v1)
    }
    
    pub fn pixel_rect(&self) -> (u16, u16, u16, u16) {
        (self.x, self.y, self.width, self.height)
    }
}

// ============================================================================
// Image ID
// ============================================================================

/// Unique identifier for an image in the atlas system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageId(pub u64);

impl ImageId {
    pub fn from_url(url: &str) -> Self {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        Self(hasher.finish())
    }
}

// ============================================================================
// Shelf Packing Algorithm
// ============================================================================

/// A shelf in the shelf packing algorithm
#[derive(Debug, Clone)]
struct Shelf {
    /// Y position of shelf
    y: u16,
    /// Height of shelf
    height: u16,
    /// Current X position (next available spot)
    x: u16,
}

/// Simple shelf-based atlas packer
#[derive(Debug)]
struct ShelfPacker {
    /// Atlas dimensions
    width: u16,
    height: u16,
    /// Current shelves
    shelves: Vec<Shelf>,
    /// Next Y position for new shelf
    next_y: u16,
}

impl ShelfPacker {
    fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            shelves: Vec::new(),
            next_y: 0,
        }
    }
    
    /// Try to pack an image, returns (x, y) if successful
    fn pack(&mut self, img_width: u16, img_height: u16) -> Option<(u16, u16)> {
        // Try to fit in existing shelf
        for shelf in &mut self.shelves {
            if shelf.height >= img_height && shelf.x + img_width <= self.width {
                let x = shelf.x;
                shelf.x += img_width;
                return Some((x, shelf.y));
            }
        }
        
        // Create new shelf
        if self.next_y + img_height <= self.height {
            let shelf = Shelf {
                y: self.next_y,
                height: img_height,
                x: img_width,
            };
            let y = self.next_y;
            self.next_y += img_height;
            self.shelves.push(shelf);
            return Some((0, y));
        }
        
        None
    }
    
    /// Reset packer
    fn clear(&mut self) {
        self.shelves.clear();
        self.next_y = 0;
    }
    
    /// Utilization ratio
    fn utilization(&self) -> f32 {
        let total = self.width as u32 * self.height as u32;
        let used: u32 = self.shelves.iter()
            .map(|s| s.x as u32 * s.height as u32)
            .sum();
        used as f32 / total as f32
    }
}

// ============================================================================
// Texture Atlas
// ============================================================================

/// A single texture atlas
#[derive(Debug)]
pub struct TextureAtlas {
    /// Atlas ID
    pub id: u16,
    /// Width in pixels
    pub width: u16,
    /// Height in pixels
    pub height: u16,
    /// Pixel data (RGBA)
    pub pixels: Vec<u8>,
    /// Shelf packer
    packer: ShelfPacker,
    /// Dirty flag (needs upload to GPU)
    pub dirty: bool,
}

impl TextureAtlas {
    /// Create a new atlas
    pub fn new(id: u16, width: u16, height: u16) -> Self {
        let pixel_count = (width as usize) * (height as usize) * 4;
        Self {
            id,
            width,
            height,
            pixels: vec![0; pixel_count],
            packer: ShelfPacker::new(width, height),
            dirty: true,
        }
    }
    
    /// Try to add an image to this atlas
    /// Returns entry if successful
    pub fn add_image(&mut self, id: ImageId, pixels: &[u8], width: u16, height: u16) -> Option<AtlasEntry> {
        let (x, y) = self.packer.pack(width, height)?;
        
        // Copy pixels
        for row in 0..height as usize {
            let src_start = row * (width as usize) * 4;
            let dst_start = ((y as usize + row) * self.width as usize + x as usize) * 4;
            let row_bytes = width as usize * 4;
            
            if src_start + row_bytes <= pixels.len() && dst_start + row_bytes <= self.pixels.len() {
                self.pixels[dst_start..dst_start + row_bytes]
                    .copy_from_slice(&pixels[src_start..src_start + row_bytes]);
            }
        }
        
        self.dirty = true;
        
        // Calculate UV coordinates
        let u0 = x as f32 / self.width as f32;
        let v0 = y as f32 / self.height as f32;
        let u1 = (x + width) as f32 / self.width as f32;
        let v1 = (y + height) as f32 / self.height as f32;
        
        Some(AtlasEntry {
            atlas_id: self.id,
            x,
            y,
            width,
            height,
            u0, v0, u1, v1,
        })
    }
    
    /// Get pixel at position
    pub fn get_pixel(&self, x: u16, y: u16) -> [u8; 4] {
        let idx = ((y as usize) * (self.width as usize) + (x as usize)) * 4;
        if idx + 4 <= self.pixels.len() {
            [self.pixels[idx], self.pixels[idx + 1], self.pixels[idx + 2], self.pixels[idx + 3]]
        } else {
            [0, 0, 0, 0]
        }
    }
    
    /// Utilization ratio
    pub fn utilization(&self) -> f32 {
        self.packer.utilization()
    }
    
    /// Clear the atlas
    pub fn clear(&mut self) {
        self.pixels.fill(0);
        self.packer.clear();
        self.dirty = true;
    }
}

// ============================================================================
// Cache Statistics
// ============================================================================

/// Cache statistics
#[derive(Debug, Clone, Default)]
pub struct AtlasCacheStats {
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Total atlases
    pub atlas_count: usize,
    /// Total entries
    pub entry_count: usize,
    /// Total bytes used
    pub bytes_used: usize,
}

impl AtlasCacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

// ============================================================================
// Texture Atlas Cache
// ============================================================================

/// Cache managing multiple texture atlases
pub struct TextureAtlasCache {
    /// All atlases
    atlases: Vec<TextureAtlas>,
    /// Image ID to atlas entry mapping
    entries: HashMap<ImageId, AtlasEntry>,
    /// Atlas dimensions
    atlas_width: u16,
    atlas_height: u16,
    /// Maximum number of atlases
    max_atlases: usize,
    /// Statistics
    stats: AtlasCacheStats,
}

impl Default for TextureAtlasCache {
    fn default() -> Self {
        Self::new(2048, 2048, 8)
    }
}

impl TextureAtlasCache {
    /// Create a new cache
    pub fn new(atlas_width: u16, atlas_height: u16, max_atlases: usize) -> Self {
        Self {
            atlases: Vec::new(),
            entries: HashMap::new(),
            atlas_width,
            atlas_height,
            max_atlases,
            stats: AtlasCacheStats::default(),
        }
    }
    
    /// Get or upload an image to an atlas
    pub fn get_or_upload(&mut self, id: ImageId, pixels: &[u8], width: u16, height: u16) -> Option<AtlasEntry> {
        // Check if already cached
        if let Some(entry) = self.entries.get(&id) {
            self.stats.hits += 1;
            return Some(*entry);
        }
        
        self.stats.misses += 1;
        
        // Image too large for atlas
        if width > self.atlas_width || height > self.atlas_height {
            return None;
        }
        
        // Try to add to existing atlas
        for atlas in &mut self.atlases {
            if let Some(entry) = atlas.add_image(id, pixels, width, height) {
                self.entries.insert(id, entry);
                return Some(entry);
            }
        }
        
        // Create new atlas if allowed
        if self.atlases.len() < self.max_atlases {
            let atlas_id = self.atlases.len() as u16;
            let mut atlas = TextureAtlas::new(atlas_id, self.atlas_width, self.atlas_height);
            
            if let Some(entry) = atlas.add_image(id, pixels, width, height) {
                self.entries.insert(id, entry);
                self.atlases.push(atlas);
                return Some(entry);
            }
        }
        
        None
    }
    
    /// Look up an existing entry
    pub fn get(&mut self, id: &ImageId) -> Option<AtlasEntry> {
        if let Some(entry) = self.entries.get(id) {
            self.stats.hits += 1;
            Some(*entry)
        } else {
            self.stats.misses += 1;
            None
        }
    }
    
    /// Check if image is cached
    pub fn contains(&self, id: &ImageId) -> bool {
        self.entries.contains_key(id)
    }
    
    /// Get atlas by ID
    pub fn atlas(&self, id: u16) -> Option<&TextureAtlas> {
        self.atlases.get(id as usize)
    }
    
    /// Get mutable atlas by ID
    pub fn atlas_mut(&mut self, id: u16) -> Option<&mut TextureAtlas> {
        self.atlases.get_mut(id as usize)
    }
    
    /// Get all dirty atlases
    pub fn dirty_atlases(&self) -> impl Iterator<Item = &TextureAtlas> {
        self.atlases.iter().filter(|a| a.dirty)
    }
    
    /// Mark all atlases as clean
    pub fn mark_all_clean(&mut self) {
        for atlas in &mut self.atlases {
            atlas.dirty = false;
        }
    }
    
    /// Clear all caches
    pub fn clear(&mut self) {
        self.atlases.clear();
        self.entries.clear();
    }
    
    /// Get statistics
    pub fn stats(&self) -> AtlasCacheStats {
        AtlasCacheStats {
            atlas_count: self.atlases.len(),
            entry_count: self.entries.len(),
            bytes_used: self.atlases.iter().map(|a| a.pixels.len()).sum(),
            ..self.stats
        }
    }
    
    /// Number of atlases
    pub fn atlas_count(&self) -> usize {
        self.atlases.len()
    }
    
    /// Number of cached entries
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_image_id() {
        let id1 = ImageId::from_url("http://example.com/img1.png");
        let id2 = ImageId::from_url("http://example.com/img1.png");
        let id3 = ImageId::from_url("http://example.com/img2.png");
        
        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }
    
    #[test]
    fn test_shelf_packer() {
        let mut packer = ShelfPacker::new(256, 256);
        
        // Pack some images
        let pos1 = packer.pack(50, 50);
        let pos2 = packer.pack(50, 50);
        let pos3 = packer.pack(50, 50);
        
        assert!(pos1.is_some());
        assert!(pos2.is_some());
        assert!(pos3.is_some());
        
        // Should be on same shelf
        let (_, y1) = pos1.unwrap();
        let (_, y2) = pos2.unwrap();
        assert_eq!(y1, y2);
    }
    
    #[test]
    fn test_texture_atlas() {
        let mut atlas = TextureAtlas::new(0, 256, 256);
        
        let id = ImageId(1);
        let pixels = vec![255u8; 40 * 40 * 4]; // 40x40 RGBA
        
        let entry = atlas.add_image(id, &pixels, 40, 40);
        assert!(entry.is_some());
        
        let entry = entry.unwrap();
        assert_eq!(entry.width, 40);
        assert_eq!(entry.height, 40);
    }
    
    #[test]
    fn test_atlas_cache() {
        let mut cache = TextureAtlasCache::new(256, 256, 4);
        
        let id = ImageId::from_url("test.png");
        let pixels = vec![255u8; 32 * 32 * 4];
        
        let entry = cache.get_or_upload(id, &pixels, 32, 32);
        assert!(entry.is_some());
        
        // Should hit cache on second lookup
        let entry2 = cache.get(&id);
        assert!(entry2.is_some());
        assert_eq!(cache.stats().hits, 1);
    }
    
    #[test]
    fn test_atlas_utilization() {
        let mut atlas = TextureAtlas::new(0, 100, 100);
        
        // Pack 50x50 image
        let id = ImageId(1);
        let pixels = vec![0u8; 50 * 50 * 4];
        atlas.add_image(id, &pixels, 50, 50);
        
        // Utilization should be around 25%
        let util = atlas.utilization();
        assert!(util > 0.2 && util < 0.3);
    }
}
