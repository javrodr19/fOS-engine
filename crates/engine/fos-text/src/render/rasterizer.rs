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
        self.rasterize_synthesized(parser, glyph_id, font_size, 0)
    }

    /// Rasterize a glyph, slanted and/or emboldened when the face lacks
    /// the requested style (`synthesis` flags from the atlas module)
    pub fn rasterize_synthesized(
        &self,
        parser: &FontParser,
        glyph_id: u16,
        font_size: f32,
        synthesis: u8,
    ) -> Option<RasterizedGlyph> {
        let glyph = GlyphId(glyph_id);
        
        // Get glyph bounding box
        let bbox = parser.glyph_bounding_box(glyph)?;
        
        // Scale factor
        let scale = font_size / parser.units_per_em().max(1) as f32;
        // Oblique: x shifts right by a fifth of the height above the
        // baseline (about 11 degrees); bold: an outline stroke
        let skew = if synthesis & super::atlas::SYNTH_OBLIQUE != 0 { 0.2 } else { 0.0 };
        let stroke = if synthesis & super::atlas::SYNTH_BOLD != 0 { (font_size / 28.0).max(0.6) } else { 0.0 };
        if skew != 0.0 || stroke != 0.0 {
            return self.rasterize_transformed(parser, glyph, font_size, scale, bbox, skew, stroke);
        }

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

impl GlyphRasterizer {
    #[allow(clippy::too_many_arguments)]
    fn rasterize_transformed(
        &self,
        parser: &FontParser,
        glyph: GlyphId,
        font_size: f32,
        scale: f32,
        bbox: crate::font::parser::BoundingBox,
        skew: f32,
        stroke: f32,
    ) -> Option<RasterizedGlyph> {
        let half = stroke / 2.0;
        let (y0, y1) = (bbox.y_min as f32 * scale, bbox.y_max as f32 * scale);
        let left = (bbox.x_min as f32 * scale + skew * y0.min(0.0) - half).floor();
        let right = (bbox.x_max as f32 * scale + skew * y1.max(0.0) + half).ceil();
        let top = (y1 + half).ceil();
        let bottom = (y0 - half).floor();
        let width = (right - left).max(0.0) as u32;
        let height = (top - bottom).max(0.0) as u32;
        if width == 0 || height == 0 || width > MAX_GLYPH_DIMENSION || height > MAX_GLYPH_DIMENSION || font_size <= 0.0 {
            return Some(RasterizedGlyph::empty(glyph.0));
        }
        // The outline at the origin (y down), then slanted and moved so
        // (left, top) is the bitmap's corner
        let mut builder = PathBuilder::new(scale, 0.0, 0.0);
        parser.outline_glyph(glyph, &mut builder)?;
        let path = builder.finish()?;
        let transform = tiny_skia::Transform::from_row(1.0, 0.0, -skew, 1.0, -left, top);
        let mut pixmap = tiny_skia::Pixmap::new(width, height)?;
        let mut paint = tiny_skia::Paint::default();
        paint.set_color(tiny_skia::Color::WHITE);
        paint.anti_alias = true;
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, transform, None);
        if stroke > 0.0 {
            let s = tiny_skia::Stroke { width: stroke, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
            pixmap.stroke_path(&path, &paint, &s, transform, None);
        }
        let bitmap: Vec<u8> = pixmap.pixels().iter().map(|p| p.alpha()).collect();
        Some(RasterizedGlyph { glyph_id: glyph.0, width, height, bearing_x: left as i32, bearing_y: top as i32, bitmap })
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
