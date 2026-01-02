//! Text rendering module
//!
//! Integrates fos-text for rendering text content on the canvas.

use crate::{Canvas, Color};
use fos_text::{
    FontDatabase, FontId, FontQuery, 
    TextShaper,
    GlyphRasterizer, GlyphAtlas, GlyphKey,
};

/// Text renderer that integrates with the canvas
pub struct TextRenderer {
    /// Font database
    pub fonts: FontDatabase,
    /// Text shaper
    shaper: TextShaper,
    /// Glyph rasterizer
    rasterizer: GlyphRasterizer,
    /// Glyph cache
    atlas: GlyphAtlas,
}

impl TextRenderer {
    /// Create a new text renderer with system fonts
    pub fn new() -> Self {
        Self {
            fonts: FontDatabase::with_system_fonts(),
            shaper: TextShaper::new(),
            rasterizer: GlyphRasterizer::new(),
            atlas: GlyphAtlas::default(),
        }
    }
    
    /// Create without system fonts (for testing)
    pub fn new_empty() -> Self {
        Self {
            fonts: FontDatabase::new(),
            shaper: TextShaper::new(),
            rasterizer: GlyphRasterizer::new(),
            atlas: GlyphAtlas::default(),
        }
    }
    
    /// Find a font by family name
    pub fn find_font(&self, families: &[&str]) -> Option<FontId> {
        let query = FontQuery::new(families);
        self.fonts.query(&query)
    }
    
    /// Render text to the canvas
    pub fn draw_text(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        x: f32,
        y: f32,
        font_id: FontId,
        font_size: f32,
        color: Color,
    ) {
        // Shape the text
        let shaped = match self.shaper.shape(&self.fonts, font_id, text, font_size) {
            Ok(s) => s,
            Err(_) => return,
        };
        
        // Get font data for rasterization
        let font_data = self.fonts.with_face_data(font_id, |data, _| data.to_vec());
        let font_data = match font_data {
            Some(d) => d,
            None => return,
        };
        
        // Render each glyph
        let mut cursor_x = x;
        let scale = shaped.scale();
        
        // Compute font hash once outside the loop
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        font_id.0.hash(&mut hasher);
        let font_idx = hasher.finish() as u32;
        
        for glyph in &shaped.glyphs {
            let key = GlyphKey::new(font_idx, glyph.glyph_id, font_size);
            
            // Get or rasterize glyph, clone to avoid borrow conflict
            let rasterized = self.atlas.get_or_insert_with(key, || {
                self.rasterizer.rasterize(&font_data, 0, glyph.glyph_id, font_size)
                    .unwrap_or_else(|| fos_text::RasterizedGlyph::empty(glyph.glyph_id))
            }).clone();
            
            if rasterized.width > 0 && rasterized.height > 0 {
                // Draw glyph bitmap
                let gx = cursor_x + glyph.x_offset as f32 * scale + rasterized.bearing_x as f32;
                let gy = y + glyph.y_offset as f32 * scale - rasterized.bearing_y as f32;
                
                draw_glyph_bitmap(
                    canvas,
                    &rasterized.bitmap,
                    rasterized.width,
                    rasterized.height,
                    gx,
                    gy,
                    color,
                );
            }
            
            cursor_x += glyph.x_advance as f32 * scale;
        }
    }
    
    /// Measure text width
    pub fn measure_text(&mut self, text: &str, font_id: FontId, font_size: f32) -> f32 {
        self.shaper.shape(&self.fonts, font_id, text, font_size)
            .map(|run| run.width())
            .unwrap_or(0.0)
    }
    
    /// Get cache statistics
    pub fn cache_stats(&self) -> (u64, u64, f64) {
        (self.atlas.hits, self.atlas.misses, self.atlas.hit_rate())
    }
}

/// Draw a glyph bitmap onto the canvas
fn draw_glyph_bitmap(
    canvas: &mut Canvas,
    bitmap: &[u8],
    width: u32,
    height: u32,
    x: f32,
    y: f32,
    color: Color,
) {
    let ix = x as i32;
    let iy = y as i32;
    
    for py in 0..height as i32 {
        for px in 0..width as i32 {
            let alpha = bitmap[(py as u32 * width + px as u32) as usize];
            if alpha > 0 {
                let canvas_x = ix + px;
                let canvas_y = iy + py;
                
                if canvas_x >= 0 && canvas_y >= 0 {
                    // Blend with existing pixel
                    if let Some(existing) = canvas.get_pixel(canvas_x as u32, canvas_y as u32) {
                        let blended = blend_pixel(existing, color, alpha);
                        canvas.set_pixel(canvas_x as u32, canvas_y as u32, blended);
                    }
                }
            }
        }
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Blend a foreground color onto a background using alpha
fn blend_pixel(bg: Color, fg: Color, alpha: u8) -> Color {
    let a = alpha as f32 / 255.0;
    let inv_a = 1.0 - a;
    
    Color::rgba(
        (fg.r as f32 * a + bg.r as f32 * inv_a) as u8,
        (fg.g as f32 * a + bg.g as f32 * inv_a) as u8,
        (fg.b as f32 * a + bg.b as f32 * inv_a) as u8,
        255,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_text_renderer_creation() {
        let renderer = TextRenderer::new();
        // Should have loaded some fonts
        assert!(renderer.fonts.len() > 0);
    }
    
    #[test]
    fn test_blend_pixel() {
        let bg = Color::WHITE;
        let fg = Color::BLACK;
        let result = blend_pixel(bg, fg, 128);
        // Should be roughly gray
        assert!(result.r > 100 && result.r < 160);
    }
}
