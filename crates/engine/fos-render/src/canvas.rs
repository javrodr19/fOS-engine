//! Canvas - pixel buffer drawn with tiny-skia
//!
//! Provides a drawing surface for rendering layout boxes.

use tiny_skia::{PixmapMut, PixmapRef, Paint, PathBuilder, Stroke, Transform, FillRule, LineCap, LineJoin, Rect as SkiaRect};
use crate::Color;

/// Pixel canvas.
///
/// Pixels are premultiplied RGBA, stored one per `u32` word so that
/// [`Canvas::into_argb32`] can convert them for a window framebuffer in
/// place, without a second full-size buffer. Drawing goes through
/// tiny-skia views of the same memory.
pub struct Canvas {
    /// Premultiplied RGBA bytes, one pixel per word
    data: Vec<u32>,
    width: u32,
    height: u32,
}

impl Canvas {
    /// Create a new (transparent) canvas with given dimensions
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Self::from_words(width, height, 0)
    }

    /// Create a canvas filled with `color`. Equivalent to `new` followed by
    /// `clear`, but writes the buffer once instead of zeroing it first.
    pub fn filled(width: u32, height: u32, color: Color) -> Option<Self> {
        let px = to_skia_color(color).premultiply().to_color_u8();
        Self::from_words(width, height, u32::from_ne_bytes([px.red(), px.green(), px.blue(), px.alpha()]))
    }

    fn from_words(width: u32, height: u32, word: u32) -> Option<Self> {
        // Same limits as a tiny-skia pixmap (non-zero, row size fits in i32)
        tiny_skia::IntSize::from_wh(width, height)?;
        if (width as usize).checked_mul(4)? > i32::MAX as usize {
            return None;
        }
        let pixels = (width as usize).checked_mul(height as usize)?;
        let mut canvas = Self { data: vec![word; pixels], width, height };
        // Views are created per drawing call; make sure they always succeed
        canvas.pixmap_mut()?;
        Some(canvas)
    }

    /// Get canvas width
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Get canvas height
    #[inline]
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Premultiplied RGBA bytes, row by row
    #[inline]
    pub fn data(&self) -> &[u8] {
        bytemuck::cast_slice(&self.data)
    }

    /// Mutable premultiplied RGBA bytes, row by row
    #[inline]
    pub fn data_mut(&mut self) -> &mut [u8] {
        bytemuck::cast_slice_mut(&mut self.data)
    }

    /// Drawing view of the pixels
    fn skia(&mut self) -> PixmapMut<'_> {
        let (width, height) = (self.width, self.height);
        PixmapMut::from_bytes(self.data_mut(), width, height).expect("size validated at creation")
    }

    /// Clear the canvas with a color
    pub fn clear(&mut self, color: Color) {
        let px = to_skia_color(color).premultiply().to_color_u8();
        self.data.fill(u32::from_ne_bytes([px.red(), px.green(), px.blue(), px.alpha()]));
    }
    
    /// Fill a rectangle with a solid color
    pub fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: Color) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        
        let path = {
            let mut pb = PathBuilder::new();
            if let Some(rect) = SkiaRect::from_xywh(x, y, width, height) {
                pb.push_rect(rect);
            }
            match pb.finish() {
                Some(p) => p,
                None => return,
            }
        };
        
        let mut paint = Paint::default();
        paint.set_color(to_skia_color(color));
        paint.anti_alias = true;
        
        self.skia().fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
    
    /// Fill a rounded rectangle
    pub fn fill_rounded_rect(
        &mut self,
        x: f32, y: f32,
        width: f32, height: f32,
        radius: f32,
        color: Color,
    ) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        
        let r = radius.min(width / 2.0).min(height / 2.0);
        
        if r <= 0.0 {
            self.fill_rect(x, y, width, height, color);
            return;
        }
        
        // Build rounded rectangle path
        let path = {
            let mut pb = PathBuilder::new();
            
            // Top-left corner
            pb.move_to(x + r, y);
            // Top edge
            pb.line_to(x + width - r, y);
            // Top-right corner
            pb.quad_to(x + width, y, x + width, y + r);
            // Right edge
            pb.line_to(x + width, y + height - r);
            // Bottom-right corner
            pb.quad_to(x + width, y + height, x + width - r, y + height);
            // Bottom edge
            pb.line_to(x + r, y + height);
            // Bottom-left corner
            pb.quad_to(x, y + height, x, y + height - r);
            // Left edge
            pb.line_to(x, y + r);
            // Top-left corner
            pb.quad_to(x, y, x + r, y);
            pb.close();
            
            match pb.finish() {
                Some(p) => p,
                None => return,
            }
        };
        
        let mut paint = Paint::default();
        paint.set_color(to_skia_color(color));
        paint.anti_alias = true;
        
        self.skia().fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
    
    /// Stroke a rectangle (draw border)
    pub fn stroke_rect(
        &mut self,
        x: f32, y: f32,
        width: f32, height: f32,
        stroke_width: f32,
        color: Color,
    ) {
        if width <= 0.0 || height <= 0.0 || stroke_width <= 0.0 {
            return;
        }
        
        let path = {
            let mut pb = PathBuilder::new();
            if let Some(rect) = SkiaRect::from_xywh(x, y, width, height) {
                pb.push_rect(rect);
            }
            match pb.finish() {
                Some(p) => p,
                None => return,
            }
        };
        
        let mut paint = Paint::default();
        paint.set_color(to_skia_color(color));
        paint.anti_alias = true;
        
        let stroke = Stroke {
            width: stroke_width,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: 4.0,
            dash: None,
        };
        
        self.skia().stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    
    /// Draw a line
    pub fn draw_line(
        &mut self,
        x1: f32, y1: f32,
        x2: f32, y2: f32,
        stroke_width: f32,
        color: Color,
    ) {
        let path = {
            let mut pb = PathBuilder::new();
            pb.move_to(x1, y1);
            pb.line_to(x2, y2);
            match pb.finish() {
                Some(p) => p,
                None => return,
            }
        };
        
        let mut paint = Paint::default();
        paint.set_color(to_skia_color(color));
        paint.anti_alias = true;
        
        let stroke = Stroke {
            width: stroke_width,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: 4.0,
            dash: None,
        };
        
        self.skia().stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    
    /// Get pixel at position
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x >= self.width() || y >= self.height() {
            return None;
        }
        
        let idx = y as usize * self.width as usize + x as usize;
        let [r, g, b, a] = self.data[idx].to_ne_bytes();

        // Premultiplied components
        Some(Color { r, g, b, a })
    }
    
    /// Set pixel at position
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x >= self.width() || y >= self.height() {
            return;
        }
        
        let idx = y as usize * self.width as usize + x as usize;
        // Use premultiplied alpha
        let a = color.a as f32 / 255.0;
        let premultiplied = [
            (color.r as f32 * a) as u8,
            (color.g as f32 * a) as u8,
            (color.b as f32 * a) as u8,
            color.a,
        ];
        self.data[idx] = u32::from_ne_bytes(premultiplied);
    }
    
    /// Get raw RGBA bytes
    pub fn as_rgba_bytes(&self) -> Vec<u8> {
        self.data.iter()
            .flat_map(|&word| {
                // Pixels are premultiplied, so unpremultiply
                let [r, g, b, a] = word.to_ne_bytes();
                if a == 0 {
                    [0, 0, 0, 0]
                } else {
                    [
                        ((r as u16 * 255) / a as u16) as u8,
                        ((g as u16 * 255) / a as u16) as u8,
                        ((b as u16 * 255) / a as u16) as u8,
                        a,
                    ]
                }
            })
            .collect()
    }
    
    /// Get pixels as `0xAARRGGBB` words (unpremultiplied), the layout used
    /// by window framebuffers such as softbuffer, so presenting a frame is
    /// a plain row copy.
    pub fn to_argb32(&self) -> Vec<u32> {
        let mut out = self.data.clone();
        rgba_words_to_argb(&mut out);
        out
    }

    /// Like [`Self::to_argb32`], converting in place: no second buffer is
    /// allocated
    pub fn into_argb32(mut self) -> Vec<u32> {
        rgba_words_to_argb(&mut self.data);
        self.data
    }

    /// Read-only tiny-skia view, for advanced operations
    pub fn pixmap(&self) -> PixmapRef<'_> {
        PixmapRef::from_bytes(self.data(), self.width, self.height).expect("size validated at creation")
    }

    /// Mutable tiny-skia view, for advanced operations
    pub fn pixmap_mut(&mut self) -> Option<PixmapMut<'_>> {
        let (width, height) = (self.width, self.height);
        PixmapMut::from_bytes(self.data_mut(), width, height)
    }
}

/// Convert premultiplied RGBA words to straight-alpha `0xAARRGGBB`
fn rgba_words_to_argb(words: &mut [u32]) {
    // Swapping the red and blue bytes of each little-endian word gives
    // 0xAARRGGBB; the loop is branch-free, so it vectorizes
    for word in words.iter_mut() {
        let rgba = u32::from_le(*word);
        *word = (rgba & 0xFF00_FF00) | ((rgba & 0xFF) << 16) | ((rgba >> 16) & 0xFF);
    }

    // Opaque pixels are already final. Pages are painted on an opaque
    // background, so translucent pixels needing unpremultiplying are rare.
    // (A branch-free AND reduction: its top byte is 0xFF iff all are opaque)
    let alpha_and = words.iter().fold(u32::MAX, |acc, &px| acc & px);
    if alpha_and < 0xFF00_0000 {
        for px in words.iter_mut().filter(|px| **px < 0xFF00_0000) {
            *px = unpremultiply_argb(*px);
        }
    }
}

/// Convert our Color to tiny-skia Color
fn to_skia_color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// Convert a premultiplied 0xAARRGGBB pixel to straight alpha
fn unpremultiply_argb(px: u32) -> u32 {
    let a = px >> 24;
    if a == 0 {
        return 0;
    }
    let channel = |shift: u32| (((px >> shift) & 0xFF) * 255 + a / 2) / a;
    (a << 24) | (channel(16).min(255) << 16) | (channel(8).min(255) << 8) | channel(0).min(255)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_canvas() {
        let canvas = Canvas::new(100, 100);
        assert!(canvas.is_some());
        
        let canvas = canvas.unwrap();
        assert_eq!(canvas.width(), 100);
        assert_eq!(canvas.height(), 100);
    }
    
    #[test]
    fn test_fill_rect() {
        let mut canvas = Canvas::new(100, 100).unwrap();
        canvas.clear(Color::WHITE);
        canvas.fill_rect(10.0, 10.0, 20.0, 20.0, Color::rgb(255, 0, 0));
        
        // Check center of filled rect
        let pixel = canvas.get_pixel(20, 20).unwrap();
        assert_eq!(pixel.r, 255);
        assert_eq!(pixel.g, 0);
        assert_eq!(pixel.b, 0);
    }
    
    #[test]
    fn test_rounded_rect() {
        let mut canvas = Canvas::new(100, 100).unwrap();
        canvas.clear(Color::WHITE);
        canvas.fill_rounded_rect(10.0, 10.0, 50.0, 30.0, 5.0, Color::rgb(0, 0, 255));
        
        // Corner should still be white (rounded away)
        // Note: Due to anti-aliasing, we just ensure no crash
    }
    
    #[test]
    fn test_stroke_rect() {
        let mut canvas = Canvas::new(100, 100).unwrap();
        canvas.clear(Color::WHITE);
        canvas.stroke_rect(10.0, 10.0, 50.0, 50.0, 2.0, Color::BLACK);
        
        // Border should be drawn
    }

    #[test]
    fn test_filled_matches_clear() {
        for color in [Color::WHITE, Color::rgb(10, 20, 30), Color::rgba(200, 100, 50, 128)] {
            let filled = Canvas::filled(3, 2, color).unwrap();
            let mut cleared = Canvas::new(3, 2).unwrap();
            cleared.clear(color);
            assert_eq!(filled.data(), cleared.data());
        }
        assert!(Canvas::filled(0, 5, Color::WHITE).is_none());
    }

    #[test]
    fn test_into_argb32_matches_to_argb32() {
        let mut canvas = Canvas::filled(5, 3, Color::WHITE).unwrap();
        canvas.fill_rect(1.0, 1.0, 2.5, 1.5, Color::rgba(0, 128, 255, 200));
        canvas.set_pixel(4, 2, Color::rgba(10, 20, 30, 0));
        let copied = canvas.to_argb32();
        assert_eq!(canvas.into_argb32(), copied);
    }

    #[test]
    fn test_to_argb32() {
        let mut canvas = Canvas::new(4, 2).unwrap();
        canvas.clear(Color::WHITE);
        canvas.fill_rect(0.0, 0.0, 1.0, 1.0, Color::rgb(0x12, 0x34, 0x56));
        let px = canvas.to_argb32();
        assert_eq!(px.len(), 8);
        assert_eq!(px[0], 0xFF12_3456);
        assert_eq!(px[1], 0xFFFF_FFFF);

        // Translucent pixels are unpremultiplied
        let mut canvas = Canvas::new(2, 1).unwrap();
        canvas.fill_rect(0.0, 0.0, 1.0, 1.0, Color::rgba(200, 100, 50, 128));
        let px = canvas.to_argb32();
        let (a, r, g, b) = (px[0] >> 24, (px[0] >> 16) & 0xFF, (px[0] >> 8) & 0xFF, px[0] & 0xFF);
        assert_eq!(a, 128);
        assert!(r.abs_diff(200) <= 1 && g.abs_diff(100) <= 1 && b.abs_diff(50) <= 1, "{:08x}", px[0]);
        assert_eq!(px[1], 0);
    }
}
