//! Glyph rasterization
//!
//! Uses custom font parser for outline extraction.

use crate::font::parser::{FontParser, GlyphId, OutlineBuilder};

/// Largest glyph bitmap side we rasterize, in pixels
const MAX_GLYPH_DIMENSION: u32 = 2048;

/// A rasterized glyph
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    /// Glyph ID
    pub glyph_id: u16,
    /// Bitmap width
    pub width: u32,
    /// Bitmap height
    pub height: u32,
    /// X bearing (offset from origin)
    pub bearing_x: i32,
    /// Y bearing (offset from baseline)
    pub bearing_y: i32,
    /// Grayscale bitmap (1 byte per pixel)
    pub bitmap: Vec<u8>,
}

impl RasterizedGlyph {
    /// Create an empty glyph
    pub fn empty(glyph_id: u16) -> Self {
        Self {
            glyph_id,
            width: 0,
            height: 0,
            bearing_x: 0,
            bearing_y: 0,
            bitmap: Vec::new(),
        }
    }
}

/// Glyph rasterizer using tiny-skia
pub struct GlyphRasterizer {
    /// Anti-aliasing quality (1-4, higher = better but slower)
    pub aa_quality: u8,
}

impl GlyphRasterizer {
    /// Create a new rasterizer
    pub fn new() -> Self {
        Self { aa_quality: 2 }
    }
    
    /// Rasterize a glyph from font data
    pub fn rasterize(
        &self,
        font_data: &[u8],
        face_index: u32,
        glyph_id: u16,
        font_size: f32,
    ) -> Option<RasterizedGlyph> {
        let parser = FontParser::parse_index(font_data, face_index).ok()?;
        self.rasterize_from_parser(&parser, glyph_id, font_size)
    }
    
    /// Rasterize a glyph from a parsed font
    pub fn rasterize_from_parser(
        &self,
        parser: &FontParser,
        glyph_id: u16,
        font_size: f32,
    ) -> Option<RasterizedGlyph> {
        let glyph = GlyphId(glyph_id);
        
        // Get glyph bounding box
        let bbox = parser.glyph_bounding_box(glyph)?;
        
        // Scale factor
        let scale = font_size / parser.units_per_em().max(1) as f32;

        // Pixel-aligned bounds that fully contain the anti-aliased outline
        let left = (bbox.x_min as f32 * scale).floor();
        let right = (bbox.x_max as f32 * scale).ceil();
        let top = (bbox.y_max as f32 * scale).ceil();
        let bottom = (bbox.y_min as f32 * scale).floor();

        let width = (right - left).max(0.0) as u32;
        let height = (top - bottom).max(0.0) as u32;

        // Also rejects absurd bounding boxes from malformed fonts
        if width == 0 || height == 0 || width > MAX_GLYPH_DIMENSION || height > MAX_GLYPH_DIMENSION {
            return Some(RasterizedGlyph::empty(glyph_id));
        }

        // Create outline builder for tiny-skia; the offsets place the
        // pixel-aligned origin (left, top) at (0, 0)
        let mut builder = PathBuilder::new(scale, left / scale, top / scale);
        parser.outline_glyph(glyph, &mut builder)?;
        let path = builder.finish()?;
        
        // Create pixmap
        let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
        
        // Fill path
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::WHITE);
        paint.anti_alias = true;
        
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
        
        // Extract alpha channel as grayscale
        let bitmap: Vec<u8> = pixmap.pixels()
            .iter()
            .map(|p| p.alpha())
            .collect();
        
        Some(RasterizedGlyph {
            glyph_id,
            width,
            height,
            bearing_x: left as i32,
            bearing_y: top as i32,
            bitmap,
        })
    }
}

impl Default for GlyphRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Path builder that converts custom parser outlines to tiny-skia paths
struct PathBuilder {
    builder: tiny_skia::PathBuilder,
    scale: f32,
    offset_x: f32,
    offset_y: f32,
}

impl PathBuilder {
    fn new(scale: f32, offset_x: f32, offset_y: f32) -> Self {
        Self {
            builder: tiny_skia::PathBuilder::new(),
            scale,
            offset_x,
            offset_y,
        }
    }
    
    fn transform_x(&self, x: f32) -> f32 {
        (x - self.offset_x) * self.scale
    }
    
    fn transform_y(&self, y: f32) -> f32 {
        (self.offset_y - y) * self.scale  // Flip Y axis
    }
    
    fn finish(self) -> Option<tiny_skia::Path> {
        self.builder.finish()
    }
}

impl OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(self.transform_x(x), self.transform_y(y));
    }
    
    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(self.transform_x(x), self.transform_y(y));
    }
    
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(
            self.transform_x(x1), self.transform_y(y1),
            self.transform_x(x), self.transform_y(y),
        );
    }
    
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_to(
            self.transform_x(x1), self.transform_y(y1),
            self.transform_x(x2), self.transform_y(y2),
            self.transform_x(x), self.transform_y(y),
        );
    }
    
    fn close(&mut self) {
        self.builder.close();
    }
}
