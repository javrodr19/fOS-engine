//! Glyph Atlas Module
//!
//! Pre-renders common ASCII glyphs to a texture atlas for fast rendering.

use std::collections::HashMap;

/// Glyph atlas for pre-rendered glyphs
pub struct GlyphAtlas {
    /// Atlas texture (RGBA)
    pub texture: Vec<u8>,
    /// Atlas width
    pub width: u32,
    /// Atlas height
    pub height: u32,
    /// Glyph locations: char -> (x, y, width, height, advance)
    glyphs: HashMap<GlyphKey, GlyphInfo>,
    /// Font size this atlas was rendered for
    pub font_size: f32,
}

/// Key for glyph lookup
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub codepoint: u32,
    pub font_id: u16,
}

impl GlyphKey {
    pub fn new(c: char, font_id: u16) -> Self {
        Self {
            codepoint: c as u32,
            font_id,
        }
    }
}

/// Information about a rendered glyph
#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    /// X position in atlas
    pub atlas_x: u32,
    /// Y position in atlas
    pub atlas_y: u32,
    /// Glyph width
    pub width: u32,
    /// Glyph height
    pub height: u32,
    /// Horizontal bearing (left side bearing)
    pub bearing_x: i32,
    /// Vertical bearing (distance from baseline to top)
    pub bearing_y: i32,
    /// Horizontal advance
    pub advance: f32,
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new(256, 256, 16.0)
    }
}

impl GlyphAtlas {
    /// Create a new glyph atlas
    pub fn new(width: u32, height: u32, font_size: f32) -> Self {
        Self {
            texture: vec![0; (width * height * 4) as usize],
            width,
            height,
            glyphs: HashMap::new(),
            font_size,
        }
    }
    
    /// Pre-render ASCII glyphs (32-126)
    pub fn prerender_ascii(&mut self, font_id: u16) {
        let glyph_w = (self.font_size * 0.6).ceil() as u32;
        let glyph_h = self.font_size.ceil() as u32;
        let cols = self.width / glyph_w;
        
        for c in 32u8..=126 {
            let idx = (c - 32) as u32;
            let col = idx % cols;
            let row = idx / cols;
            
            let x = col * glyph_w;
            let y = row * glyph_h;
            
            if y + glyph_h > self.height {
                break; // Atlas full
            }
            
            let key = GlyphKey::new(c as char, font_id);
            let info = GlyphInfo {
                atlas_x: x,
                atlas_y: y,
                width: glyph_w,
                height: glyph_h,
                bearing_x: 0,
                bearing_y: glyph_h as i32,
                advance: glyph_w as f32,
            };
            
            self.glyphs.insert(key, info);
        }
    }
    
    /// Get glyph info
    pub fn get(&self, key: &GlyphKey) -> Option<&GlyphInfo> {
        self.glyphs.get(key)
    }
    
    /// Get glyph info by char
    pub fn get_char(&self, c: char, font_id: u16) -> Option<&GlyphInfo> {
        self.get(&GlyphKey::new(c, font_id))
    }
    
    /// Insert a glyph at specified position
    pub fn insert(&mut self, key: GlyphKey, info: GlyphInfo) {
        self.glyphs.insert(key, info);
    }
    
    /// Write glyph data to atlas texture
    pub fn write_glyph(&mut self, x: u32, y: u32, bitmap: &[u8], w: u32, h: u32) {
        for py in 0..h {
            for px in 0..w {
                let src_idx = (py * w + px) as usize;
                let dst_x = x + px;
                let dst_y = y + py;
                
                if dst_x < self.width && dst_y < self.height && src_idx < bitmap.len() {
                    let dst_idx = ((dst_y * self.width + dst_x) * 4) as usize;
                    let alpha = bitmap[src_idx];
                    
                    if dst_idx + 3 < self.texture.len() {
                        self.texture[dst_idx] = 255;     // R
                        self.texture[dst_idx + 1] = 255; // G
                        self.texture[dst_idx + 2] = 255; // B
                        self.texture[dst_idx + 3] = alpha; // A
                    }
                }
            }
        }
    }
    
    /// Get atlas hit rate (glyphs in atlas / lookups)
    pub fn coverage(&self) -> usize {
        self.glyphs.len()
    }
    
    /// Clear the atlas
    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.texture.fill(0);
    }
}

/// Global glyph atlas cache
pub struct GlyphAtlasCache {
    atlases: HashMap<AtlasKey, GlyphAtlas>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct AtlasKey {
    font_id: u16,
    size: u32, // font size * 10 for sub-pixel precision
}

impl Default for GlyphAtlasCache {
    fn default() -> Self {
        Self::new()
    }
}

impl GlyphAtlasCache {
    pub fn new() -> Self {
        Self {
            atlases: HashMap::new(),
        }
    }
    
    /// Get or create atlas for font/size combination
    pub fn get_or_create(&mut self, font_id: u16, font_size: f32) -> &mut GlyphAtlas {
        let key = AtlasKey {
            font_id,
            size: (font_size * 10.0) as u32,
        };
        
        self.atlases.entry(key).or_insert_with(|| {
            let mut atlas = GlyphAtlas::new(512, 512, font_size);
            atlas.prerender_ascii(font_id);
            atlas
        })
    }
    
    /// Clear all atlases
    pub fn clear(&mut self) {
        self.atlases.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_atlas_creation() {
        let atlas = GlyphAtlas::new(256, 256, 16.0);
        assert_eq!(atlas.width, 256);
        assert_eq!(atlas.height, 256);
    }
    
    #[test]
    fn test_prerender_ascii() {
        let mut atlas = GlyphAtlas::new(256, 256, 16.0);
        atlas.prerender_ascii(0);
        
        // Should have ASCII 32-126 = 95 glyphs
        assert!(atlas.coverage() > 90);
    }
    
    #[test]
    fn test_glyph_lookup() {
        let mut atlas = GlyphAtlas::new(256, 256, 16.0);
        atlas.prerender_ascii(0);
        
        let info = atlas.get_char('A', 0);
        assert!(info.is_some());
    }
    
    #[test]
    fn test_atlas_cache() {
        let mut cache = GlyphAtlasCache::new();
        
        let atlas1 = cache.get_or_create(0, 16.0);
        assert!(atlas1.get_char('A', 0).is_some());
    }
}
