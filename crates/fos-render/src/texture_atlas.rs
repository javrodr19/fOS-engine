//! Texture Atlas Packing (Phase 24.5)
//!
//! Pack all small images into one GPU texture. Single draw call for
//! many images. Bin packing algorithm. 90% fewer texture binds.

use std::collections::HashMap;

/// Texture atlas ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct AtlasId(pub u16);

/// Image ID within an atlas
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageId {
    pub atlas: AtlasId,
    pub index: u16,
}

/// Rectangle in the atlas
#[derive(Debug, Clone, Copy, Default)]
pub struct AtlasRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl AtlasRect {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self { x, y, width, height }
    }
    
    /// Area of this rect
    pub fn area(&self) -> u32 {
        self.width as u32 * self.height as u32
    }
    
    /// Can fit another rect
    pub fn can_fit(&self, width: u16, height: u16) -> bool {
        self.width >= width && self.height >= height
    }
    
    /// Get UV coordinates (normalized 0-1)
    pub fn uv(&self, atlas_size: u16) -> (f32, f32, f32, f32) {
        let inv = 1.0 / atlas_size as f32;
        (
            self.x as f32 * inv,
            self.y as f32 * inv,
            (self.x + self.width) as f32 * inv,
            (self.y + self.height) as f32 * inv,
        )
    }
}

/// Packed image in the atlas
#[derive(Debug, Clone)]
pub struct PackedImage {
    /// Image ID
    pub id: u32,
    /// Location in atlas
    pub rect: AtlasRect,
    /// Original size (may differ due to padding)
    pub original_width: u16,
    pub original_height: u16,
}

/// Free rectangle in the atlas (for bin packing)
#[derive(Debug, Clone, Copy)]
struct FreeRect {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

impl FreeRect {
    fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self { x, y, width, height }
    }
    
    fn can_fit(&self, w: u16, h: u16) -> bool {
        self.width >= w && self.height >= h
    }
    
    fn area(&self) -> u32 {
        self.width as u32 * self.height as u32
    }
}

/// Texture atlas using MaxRects bin packing
#[derive(Debug)]
pub struct TextureAtlas {
    /// Atlas ID
    id: AtlasId,
    /// Size of the atlas (width = height)
    size: u16,
    /// Pixel data (RGBA)
    pixels: Vec<u8>,
    /// Packed images
    images: Vec<PackedImage>,
    /// Free rectangles for packing
    free_rects: Vec<FreeRect>,
    /// Image ID to index mapping
    id_to_index: HashMap<u32, usize>,
    /// Bytes per pixel
    bpp: u8,
    /// Padding between images
    padding: u16,
}

impl TextureAtlas {
    /// Create a new atlas
    pub fn new(id: AtlasId, size: u16) -> Self {
        let pixel_count = (size as usize) * (size as usize);
        
        Self {
            id,
            size,
            pixels: vec![0u8; pixel_count * 4], // RGBA
            images: Vec::new(),
            free_rects: vec![FreeRect::new(0, 0, size, size)],
            id_to_index: HashMap::new(),
            bpp: 4,
            padding: 1,
        }
    }
    
    /// Set padding between images
    pub fn with_padding(mut self, padding: u16) -> Self {
        self.padding = padding;
        self
    }
    
    /// Try to pack an image into the atlas
    pub fn pack(&mut self, id: u32, width: u16, height: u16, pixels: &[u8]) -> Option<ImageId> {
        let padded_width = width + self.padding * 2;
        let padded_height = height + self.padding * 2;
        
        // Find best free rect (best short side fit)
        let best_idx = self.find_best_rect(padded_width, padded_height)?;
        let free_rect = self.free_rects[best_idx];
        
        // Place image at free rect origin (with padding)
        let x = free_rect.x + self.padding;
        let y = free_rect.y + self.padding;
        
        // Copy pixels to atlas
        self.copy_pixels(x, y, width, height, pixels);
        
        // Split remaining space
        self.split_free_rect(best_idx, padded_width, padded_height);
        
        // Record packed image
        let rect = AtlasRect::new(x, y, width, height);
        let index = self.images.len();
        
        self.images.push(PackedImage {
            id,
            rect,
            original_width: width,
            original_height: height,
        });
        
        self.id_to_index.insert(id, index);
        
        Some(ImageId {
            atlas: self.id,
            index: index as u16,
        })
    }
    
    /// Find best rectangle for packing (best short side fit)
    fn find_best_rect(&self, width: u16, height: u16) -> Option<usize> {
        let mut best_idx = None;
        let mut best_short_side = u16::MAX;
        let mut best_area = u32::MAX;
        
        for (i, rect) in self.free_rects.iter().enumerate() {
            if !rect.can_fit(width, height) {
                continue;
            }
            
            let leftover_h = rect.width - width;
            let leftover_v = rect.height - height;
            let short_side = leftover_h.min(leftover_v);
            let area = rect.area();
            
            if short_side < best_short_side || (short_side == best_short_side && area < best_area) {
                best_idx = Some(i);
                best_short_side = short_side;
                best_area = area;
            }
        }
        
        best_idx
    }
    
    /// Split free rectangle after placing image
    fn split_free_rect(&mut self, idx: usize, width: u16, height: u16) {
        let rect = self.free_rects.remove(idx);
        
        // Create right remainder
        if rect.width > width {
            self.free_rects.push(FreeRect::new(
                rect.x + width,
                rect.y,
                rect.width - width,
                height,
            ));
        }
        
        // Create bottom remainder
        if rect.height > height {
            self.free_rects.push(FreeRect::new(
                rect.x,
                rect.y + height,
                rect.width,
                rect.height - height,
            ));
        }
        
        // Merge overlapping free rectangles (simplified)
        self.merge_free_rects();
    }
    
    /// Merge adjacent free rectangles
    fn merge_free_rects(&mut self) {
        // Simple deduplication - real implementation would merge adjacent rects
        self.free_rects.retain(|r| r.width > 0 && r.height > 0);
    }
    
    /// Copy pixels into atlas
    fn copy_pixels(&mut self, x: u16, y: u16, width: u16, height: u16, pixels: &[u8]) {
        let stride = self.size as usize * self.bpp as usize;
        
        for row in 0..height as usize {
            let src_start = row * width as usize * self.bpp as usize;
            let src_end = src_start + width as usize * self.bpp as usize;
            
            if src_end > pixels.len() {
                continue;
            }
            
            let dst_start = (y as usize + row) * stride + x as usize * self.bpp as usize;
            let dst_end = dst_start + width as usize * self.bpp as usize;
            
            if dst_end <= self.pixels.len() {
                self.pixels[dst_start..dst_end].copy_from_slice(&pixels[src_start..src_end]);
            }
        }
    }
    
    /// Get image location by ID
    pub fn get(&self, id: u32) -> Option<&PackedImage> {
        self.id_to_index.get(&id).and_then(|&idx| self.images.get(idx))
    }
    
    /// Get UV coordinates for an image
    pub fn get_uv(&self, id: u32) -> Option<(f32, f32, f32, f32)> {
        self.get(id).map(|img| img.rect.uv(self.size))
    }
    
    /// Get atlas size
    pub fn size(&self) -> u16 {
        self.size
    }
    
    /// Get atlas ID
    pub fn id(&self) -> AtlasId {
        self.id
    }
    
    /// Get raw pixels
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    
    /// Number of packed images
    pub fn len(&self) -> usize {
        self.images.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
    
    /// Usage percentage
    pub fn usage(&self) -> f32 {
        let total = self.size as u32 * self.size as u32;
        let used: u32 = self.images.iter().map(|i| i.rect.area()).sum();
        used as f32 / total as f32
    }
    
    /// Memory size
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.pixels.len()
    }
}

/// Atlas manager - manages multiple atlases
#[derive(Debug)]
pub struct AtlasManager {
    /// All atlases
    atlases: Vec<TextureAtlas>,
    /// Atlas size
    atlas_size: u16,
    /// Maximum image size for atlas (larger images get own texture)
    max_image_size: u16,
    /// Next atlas ID
    next_id: u16,
    /// Image ID to atlas/rect mapping
    image_map: HashMap<u32, ImageId>,
    /// Stats
    stats: AtlasStats,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AtlasStats {
    pub atlases_created: u32,
    pub images_packed: u32,
    pub images_rejected: u32,
    pub total_pixels: u64,
    pub used_pixels: u64,
}

impl AtlasStats {
    pub fn utilization(&self) -> f32 {
        if self.total_pixels == 0 {
            0.0
        } else {
            self.used_pixels as f32 / self.total_pixels as f32
        }
    }
}

impl Default for AtlasManager {
    fn default() -> Self {
        Self::new()
    }
}

impl AtlasManager {
    pub fn new() -> Self {
        Self {
            atlases: Vec::new(),
            atlas_size: 2048,
            max_image_size: 512,
            next_id: 0,
            image_map: HashMap::new(),
            stats: AtlasStats::default(),
        }
    }
    
    /// Set atlas size
    pub fn with_atlas_size(mut self, size: u16) -> Self {
        self.atlas_size = size;
        self
    }
    
    /// Set max image size for atlas
    pub fn with_max_image_size(mut self, size: u16) -> Self {
        self.max_image_size = size;
        self
    }
    
    /// Add an image to appropriate atlas
    pub fn add(&mut self, id: u32, width: u16, height: u16, pixels: &[u8]) -> Option<ImageId> {
        // Check if too large for atlas
        if width > self.max_image_size || height > self.max_image_size {
            self.stats.images_rejected += 1;
            return None;
        }
        
        // Try existing atlases
        for atlas in &mut self.atlases {
            if let Some(image_id) = atlas.pack(id, width, height, pixels) {
                self.image_map.insert(id, image_id);
                self.stats.images_packed += 1;
                self.stats.used_pixels += width as u64 * height as u64;
                return Some(image_id);
            }
        }
        
        // Create new atlas
        let atlas_id = AtlasId(self.next_id);
        self.next_id += 1;
        
        let mut atlas = TextureAtlas::new(atlas_id, self.atlas_size);
        
        if let Some(image_id) = atlas.pack(id, width, height, pixels) {
            self.image_map.insert(id, image_id);
            self.stats.atlases_created += 1;
            self.stats.images_packed += 1;
            self.stats.total_pixels += self.atlas_size as u64 * self.atlas_size as u64;
            self.stats.used_pixels += width as u64 * height as u64;
            self.atlases.push(atlas);
            Some(image_id)
        } else {
            self.stats.images_rejected += 1;
            None
        }
    }
    
    /// Get image location
    pub fn get(&self, id: u32) -> Option<&PackedImage> {
        let image_id = self.image_map.get(&id)?;
        self.atlases.get(image_id.atlas.0 as usize)?.get(id)
    }
    
    /// Get atlas by ID
    pub fn get_atlas(&self, id: AtlasId) -> Option<&TextureAtlas> {
        self.atlases.get(id.0 as usize)
    }
    
    /// Number of atlases
    pub fn atlas_count(&self) -> usize {
        self.atlases.len()
    }
    
    /// Stats
    pub fn stats(&self) -> &AtlasStats {
        &self.stats
    }
    
    /// Total memory usage
    pub fn memory_size(&self) -> usize {
        self.atlases.iter().map(|a| a.memory_size()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_atlas_packing() {
        let mut atlas = TextureAtlas::new(AtlasId(0), 256);
        
        // Pack some images
        let pixels = vec![255u8; 32 * 32 * 4]; // White 32x32 RGBA
        
        let id1 = atlas.pack(1, 32, 32, &pixels);
        assert!(id1.is_some());
        
        let id2 = atlas.pack(2, 32, 32, &pixels);
        assert!(id2.is_some());
        
        assert_eq!(atlas.len(), 2);
    }
    
    #[test]
    fn test_atlas_uv() {
        let mut atlas = TextureAtlas::new(AtlasId(0), 256).with_padding(0);
        
        let pixels = vec![255u8; 64 * 64 * 4];
        atlas.pack(1, 64, 64, &pixels);
        
        let (u0, v0, u1, v1) = atlas.get_uv(1).unwrap();
        assert_eq!(u0, 0.0);
        assert_eq!(v0, 0.0);
        assert_eq!(u1, 64.0 / 256.0);
        assert_eq!(v1, 64.0 / 256.0);
    }
    
    #[test]
    fn test_atlas_manager() {
        let mut manager = AtlasManager::new().with_atlas_size(256);
        
        let pixels = vec![0u8; 64 * 64 * 4];
        
        // Add multiple images
        for i in 0..10 {
            let result = manager.add(i, 64, 64, &pixels);
            assert!(result.is_some());
        }
        
        assert!(manager.atlas_count() >= 1);
        assert_eq!(manager.stats().images_packed, 10);
    }
    
    #[test]
    fn test_oversized_rejection() {
        let mut manager = AtlasManager::new().with_max_image_size(256);
        
        let pixels = vec![0u8; 512 * 512 * 4];
        let result = manager.add(1, 512, 512, &pixels);
        
        assert!(result.is_none());
        assert_eq!(manager.stats().images_rejected, 1);
    }
}
