//! Image Sprites Support
//!
//! CSS sprite sheet handling for efficient image loading.

use std::collections::HashMap;

/// Sprite sheet definition
#[derive(Debug, Clone)]
pub struct SpriteSheet {
    /// Sprite sheet image data (RGBA)
    pub data: Vec<u8>,
    /// Sheet width in pixels
    pub width: u32,
    /// Sheet height in pixels  
    pub height: u32,
    /// Named sprites with their regions
    pub sprites: HashMap<String, SpriteRegion>,
}

/// Individual sprite region within a sheet
#[derive(Debug, Clone, Copy)]
pub struct SpriteRegion {
    /// X offset in sheet
    pub x: u32,
    /// Y offset in sheet
    pub y: u32,
    /// Sprite width
    pub width: u32,
    /// Sprite height
    pub height: u32,
}

impl SpriteRegion {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Check if point is within this region
    pub fn contains(&self, px: u32, py: u32) -> bool {
        px >= self.x && px < self.x + self.width &&
        py >= self.y && py < self.y + self.height
    }
    
    /// Calculate local coordinates within sprite
    pub fn local_coords(&self, px: u32, py: u32) -> Option<(u32, u32)> {
        if self.contains(px, py) {
            Some((px - self.x, py - self.y))
        } else {
            None
        }
    }
}

impl SpriteSheet {
    /// Create a new sprite sheet from image data
    pub fn new(data: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            data,
            width,
            height,
            sprites: HashMap::new(),
        }
    }
    
    /// Add a named sprite region
    pub fn add_sprite(&mut self, name: &str, region: SpriteRegion) {
        self.sprites.insert(name.to_string(), region);
    }
    
    /// Get sprite by name
    pub fn get_sprite(&self, name: &str) -> Option<&SpriteRegion> {
        self.sprites.get(name)
    }
    
    /// Extract sprite image data
    pub fn extract(&self, region: &SpriteRegion) -> Vec<u8> {
        let mut result = Vec::with_capacity((region.width * region.height * 4) as usize);
        
        for y in 0..region.height {
            let src_y = region.y + y;
            if src_y >= self.height {
                continue;
            }
            
            for x in 0..region.width {
                let src_x = region.x + x;
                if src_x >= self.width {
                    result.extend_from_slice(&[0, 0, 0, 0]);
                    continue;
                }
                
                let offset = ((src_y * self.width + src_x) * 4) as usize;
                if offset + 4 <= self.data.len() {
                    result.extend_from_slice(&self.data[offset..offset + 4]);
                } else {
                    result.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
        }
        
        result
    }
    
    /// Create uniform grid of sprites
    pub fn create_grid(&mut self, sprite_width: u32, sprite_height: u32, prefix: &str) {
        let cols = self.width / sprite_width;
        let rows = self.height / sprite_height;
        
        for row in 0..rows {
            for col in 0..cols {
                let name = format!("{}_{}", prefix, row * cols + col);
                let region = SpriteRegion::new(
                    col * sprite_width,
                    row * sprite_height,
                    sprite_width,
                    sprite_height,
                );
                self.sprites.insert(name, region);
            }
        }
    }
}

/// CSS background-position to sprite region converter
#[derive(Debug, Clone)]
pub struct CssSpriteResolver {
    sheet: SpriteSheet,
}

impl CssSpriteResolver {
    pub fn new(sheet: SpriteSheet) -> Self {
        Self { sheet }
    }
    
    /// Resolve CSS background-position to sprite data
    /// background-position format: "-Xpx -Ypx"
    pub fn resolve(&self, background_position: &str, width: u32, height: u32) -> Option<Vec<u8>> {
        let (x, y) = self.parse_position(background_position)?;
        
        // CSS uses negative offsets for sprites
        let x = (-x) as u32;
        let y = (-y) as u32;
        
        let region = SpriteRegion::new(x, y, width, height);
        Some(self.sheet.extract(&region))
    }
    
    fn parse_position(&self, pos: &str) -> Option<(i32, i32)> {
        let parts: Vec<&str> = pos.split_whitespace().collect();
        if parts.len() != 2 {
            return None;
        }
        
        let x = self.parse_px(parts[0])?;
        let y = self.parse_px(parts[1])?;
        
        Some((x, y))
    }
    
    fn parse_px(&self, value: &str) -> Option<i32> {
        let num = value.trim_end_matches("px");
        num.parse().ok()
    }
}

/// Sprite atlas packer (for generating sprite sheets)
#[derive(Debug)]
pub struct SpritePacker {
    /// Maximum atlas size
    max_width: u32,
    max_height: u32,
    /// Padding between sprites
    padding: u32,
    /// Sprites to pack
    sprites: Vec<(String, u32, u32, Vec<u8>)>,
}

impl SpritePacker {
    pub fn new(max_width: u32, max_height: u32, padding: u32) -> Self {
        Self {
            max_width,
            max_height,
            padding,
            sprites: Vec::new(),
        }
    }
    
    /// Add a sprite to pack
    pub fn add(&mut self, name: &str, width: u32, height: u32, data: Vec<u8>) {
        self.sprites.push((name.to_string(), width, height, data));
    }
    
    /// Pack sprites into atlas (simple row-based packing)
    pub fn pack(&self) -> Option<SpriteSheet> {
        if self.sprites.is_empty() {
            return None;
        }
        
        // Calculate required size
        let mut current_x = 0u32;
        let mut current_y = 0u32;
        let mut row_height = 0u32;
        let mut positions = Vec::new();
        
        for (_, width, height, _) in &self.sprites {
            if current_x + width > self.max_width {
                current_x = 0;
                current_y += row_height + self.padding;
                row_height = 0;
            }
            
            positions.push((current_x, current_y));
            current_x += width + self.padding;
            row_height = row_height.max(*height);
        }
        
        let atlas_height = current_y + row_height;
        if atlas_height > self.max_height {
            return None;
        }
        
        // Create atlas
        let mut data = vec![0u8; (self.max_width * atlas_height * 4) as usize];
        let mut sheet = SpriteSheet::new(data.clone(), self.max_width, atlas_height);
        
        for (i, (name, width, height, sprite_data)) in self.sprites.iter().enumerate() {
            let (x, y) = positions[i];
            
            // Copy sprite data
            for sy in 0..*height {
                for sx in 0..*width {
                    let src_offset = ((sy * width + sx) * 4) as usize;
                    let dst_offset = (((y + sy) * self.max_width + (x + sx)) * 4) as usize;
                    
                    if src_offset + 4 <= sprite_data.len() && dst_offset + 4 <= data.len() {
                        data[dst_offset..dst_offset + 4]
                            .copy_from_slice(&sprite_data[src_offset..src_offset + 4]);
                    }
                }
            }
            
            sheet.add_sprite(name, SpriteRegion::new(x, y, *width, *height));
        }
        
        sheet.data = data;
        Some(sheet)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sprite_region() {
        let region = SpriteRegion::new(10, 20, 32, 32);
        assert!(region.contains(15, 25));
        assert!(!region.contains(5, 5));
    }
    
    #[test]
    fn test_sprite_sheet() {
        let mut sheet = SpriteSheet::new(vec![0u8; 256 * 256 * 4], 256, 256);
        sheet.add_sprite("icon", SpriteRegion::new(0, 0, 32, 32));
        
        assert!(sheet.get_sprite("icon").is_some());
        assert!(sheet.get_sprite("missing").is_none());
    }
    
    #[test]
    fn test_grid_creation() {
        let mut sheet = SpriteSheet::new(vec![0u8; 128 * 128 * 4], 128, 128);
        sheet.create_grid(32, 32, "tile");
        
        assert_eq!(sheet.sprites.len(), 16); // 4x4 grid
    }
}
