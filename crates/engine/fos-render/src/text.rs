//! Text rendering module
//!
//! Integrates fos-text for rendering text content on the canvas.

use std::sync::Arc;

use crate::{Canvas, Color};
use fos_text::{
    FontDatabase, FontId, FontQuery, FontParser,
    TextShaper,
    GlyphRasterizer, GlyphAtlas, GlyphKey, RasterizedGlyph,
};

/// Text renderer that integrates with the canvas
pub struct TextRenderer {
    /// Font database (shared, so system fonts are scanned once per process)
    pub fonts: Arc<FontDatabase>,
    /// Text shaper
    shaper: TextShaper,
    /// Glyph rasterizer
    rasterizer: GlyphRasterizer,
    /// Glyph cache
    atlas: GlyphAtlas,
}

impl TextRenderer {
    /// Create a new text renderer using the process-wide system font database
    pub fn new() -> Self {
        Self::with_database(FontDatabase::shared())
    }

    /// Create without system fonts (for testing)
    pub fn new_empty() -> Self {
        Self::with_database(Arc::new(FontDatabase::new()))
    }

    /// Create a renderer using a specific font database
    pub fn with_database(fonts: Arc<FontDatabase>) -> Self {
        Self {
            fonts,
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

    /// Render text to the canvas with its baseline at `y`.
    ///
    /// Returns the advance width of the rendered text, so callers do not
    /// need to shape the same text again to measure it.
    pub fn draw_text(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        x: f32,
        y: f32,
        font_id: FontId,
        font_size: f32,
        color: Color,
    ) -> f32 {
        let Some(font_data) = self.fonts.face_data(font_id) else { return 0.0 };
        let face_index = self.fonts.font(font_id).map_or(0, |f| f.index);

        // Shape the text
        let shaped = match self.shaper.shape_with_data(&font_data, face_index, text, font_size) {
            Ok(s) => s,
            Err(_) => return 0.0,
        };
        let scale = shaped.scale();
        let advance = shaped.width();

        // Nothing to draw if the whole line is outside the canvas vertically
        let (top, bottom) = (y - font_size * 2.0, y + font_size);
        if bottom < 0.0 || top > canvas.height() as f32 || color.a == 0 {
            return advance;
        }

        // Parsed lazily: only needed when a glyph is not cached yet
        let mut parser: Option<Option<FontParser>> = None;
        let mut cursor_x = x;

        for glyph in &shaped.glyphs {
            let key = GlyphKey::new(font_id.0, glyph.glyph_id, font_size);
            let rasterizer = &self.rasterizer;
            let rasterized = self.atlas.get_or_insert_with(key, || {
                parser
                    .get_or_insert_with(|| FontParser::parse_index(&font_data, face_index).ok())
                    .as_ref()
                    .and_then(|p| rasterizer.rasterize_from_parser(p, glyph.glyph_id, font_size))
                    .unwrap_or_else(|| RasterizedGlyph::empty(glyph.glyph_id))
            });

            if rasterized.width > 0 && rasterized.height > 0 {
                let gx = cursor_x + glyph.x_offset as f32 * scale + rasterized.bearing_x as f32;
                let gy = y - glyph.y_offset as f32 * scale - rasterized.bearing_y as f32;
                draw_glyph_bitmap(canvas, rasterized, gx.round() as i32, gy.round() as i32, color);
            }

            cursor_x += glyph.x_advance as f32 * scale;
        }

        advance
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

    /// Drop all cached glyph bitmaps (e.g. under memory pressure)
    pub fn clear_glyph_cache(&mut self) {
        self.atlas.clear();
    }
}

/// Blend a coverage bitmap onto the canvas at integer position (x, y).
///
/// Works directly on tiny-skia's premultiplied pixels with integer math and
/// clips once up front instead of bounds-checking every pixel.
fn draw_glyph_bitmap(canvas: &mut Canvas, glyph: &RasterizedGlyph, x: i32, y: i32, color: Color) {
    let canvas_w = canvas.width() as i32;
    let canvas_h = canvas.height() as i32;
    let glyph_w = glyph.width as i32;
    let glyph_h = glyph.height as i32;

    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + glyph_w).min(canvas_w);
    let y1 = (y + glyph_h).min(canvas_h);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    // Row slices of the glyph and the canvas, so the inner loop has no
    // bounds checks; pixels are premultiplied RGBA bytes
    let canvas_w = canvas_w as usize;
    let glyph_w = glyph_w as usize;
    let (sx0, sx1) = ((x0 - x) as usize, (x1 - x) as usize);
    let (pixels, _) = canvas.data_mut().as_chunks_mut::<4>();
    for cy in y0..y1 {
        let src_row = (cy - y) as usize * glyph_w;
        let src = &glyph.bitmap[src_row + sx0..src_row + sx1];
        let dst_row = cy as usize * canvas_w;
        let dst = &mut pixels[dst_row + x0 as usize..dst_row + x1 as usize];
        for (pixel, &coverage) in dst.iter_mut().zip(src) {
            if coverage != 0 {
                *pixel = blend_premultiplied(*pixel, color, coverage);
            }
        }
    }
}

/// Source-over blend of `color` (straight alpha) at `coverage` onto a
/// premultiplied RGBA pixel. The result stays premultiplied (no channel
/// exceeds alpha), as the rounding of each term is bounded by alpha's.
#[inline]
fn blend_premultiplied(dst: [u8; 4], color: Color, coverage: u8) -> [u8; 4] {
    let sa = (coverage as u32 * color.a as u32 + 127) / 255;
    match sa {
        0 => dst,
        // Fully covered by an opaque color (glyph interiors)
        255 => [color.r, color.g, color.b, 255],
        _ => {
            let inv = 255 - sa;
            let mix = |src: u8, dst: u8| ((src as u32 * sa + dst as u32 * inv + 127) / 255) as u8;
            [
                mix(color.r, dst[0]),
                mix(color.g, dst[1]),
                mix(color.b, dst[2]),
                (sa + (dst[3] as u32 * inv + 127) / 255) as u8,
            ]
        }
    }
}

impl Default for TextRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Blend a foreground color onto a background using alpha
#[cfg(test)]
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
        // Environments without any installed fonts are valid; the shared
        // database must simply be usable
        let _ = renderer.fonts.len();
    }

    #[test]
    fn test_renderers_share_font_database() {
        let a = TextRenderer::new();
        let b = TextRenderer::new();
        assert!(Arc::ptr_eq(&a.fonts, &b.fonts));
    }

    #[test]
    fn test_blend_pixel() {
        let bg = Color::WHITE;
        let fg = Color::BLACK;
        let result = blend_pixel(bg, fg, 128);
        // Should be roughly gray
        assert!(result.r > 100 && result.r < 160);
    }

    #[test]
    fn test_blend_premultiplied() {
        let white = [255; 4];

        let full = blend_premultiplied(white, Color::BLACK, 255);
        assert_eq!(full, [0, 0, 0, 255]);

        let half = blend_premultiplied(white, Color::BLACK, 128);
        assert!(half[0] > 120 && half[0] < 135);
        assert_eq!(half[3], 255);

        let none = blend_premultiplied(white, Color::BLACK, 0);
        assert_eq!(none, white);

        // Onto transparent pixels, channels never exceed alpha
        for coverage in [1, 7, 128, 200, 254] {
            for color in [Color::rgba(255, 128, 0, 200), Color::rgba(10, 250, 90, 255)] {
                let [r, g, b, a] = blend_premultiplied([0; 4], color, coverage);
                assert!(r <= a && g <= a && b <= a, "{coverage}: {:?}", [r, g, b, a]);
                let again = blend_premultiplied([r, g, b, a], color, coverage);
                assert!(again[..3].iter().all(|&c| c <= again[3]));
            }
        }
    }

    #[test]
    fn test_glyph_bitmap_is_clipped() {
        let mut canvas = Canvas::new(4, 4).unwrap();
        canvas.clear(Color::WHITE);
        let glyph = RasterizedGlyph {
            glyph_id: 1,
            width: 3,
            height: 3,
            bearing_x: 0,
            bearing_y: 0,
            bitmap: vec![255; 9],
        };

        // Partially off every edge; must not panic
        draw_glyph_bitmap(&mut canvas, &glyph, -1, -1, Color::BLACK);
        draw_glyph_bitmap(&mut canvas, &glyph, 3, 3, Color::BLACK);
        draw_glyph_bitmap(&mut canvas, &glyph, 100, 100, Color::BLACK);

        assert_eq!(canvas.get_pixel(0, 0).unwrap().r, 0);
        assert_eq!(canvas.get_pixel(1, 1).unwrap().r, 0);
        assert_eq!(canvas.get_pixel(2, 2).unwrap().r, 255);
        assert_eq!(canvas.get_pixel(3, 3).unwrap().r, 0);
    }
}
