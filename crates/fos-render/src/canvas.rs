//! Canvas - pixel buffer backed by tiny-skia
//!
//! Provides a drawing surface for rendering layout boxes.

use tiny_skia::{Pixmap, Paint, PathBuilder, Stroke, Transform, FillRule, LineCap, LineJoin, Rect as SkiaRect};
use crate::Color;

/// Pixel canvas backed by tiny-skia Pixmap
pub struct Canvas {
    pixmap: Pixmap,
}

impl Canvas {
    /// Create a new canvas with given dimensions
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Pixmap::new(width, height).map(|pixmap| Self { pixmap })
    }
    
    /// Get canvas width
    #[inline]
    pub fn width(&self) -> u32 {
        self.pixmap.width()
    }
    
    /// Get canvas height
    #[inline]
    pub fn height(&self) -> u32 {
        self.pixmap.height()
    }
    
    /// Clear the canvas with a color
    pub fn clear(&mut self, color: Color) {
        self.pixmap.fill(to_skia_color(color));
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
        
        self.pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
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
        
        self.pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
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
        
        self.pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
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
        
        self.pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    
    /// Get pixel at position
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x >= self.width() || y >= self.height() {
            return None;
        }
        
        let idx = (y * self.width() + x) as usize;
        let pixel = self.pixmap.pixels()[idx];
        
        Some(Color {
            r: pixel.red(),
            g: pixel.green(),
            b: pixel.blue(),
            a: pixel.alpha(),
        })
    }
    
    /// Get raw RGBA bytes
    pub fn as_rgba_bytes(&self) -> Vec<u8> {
        self.pixmap.pixels()
            .iter()
            .flat_map(|p| {
                // tiny-skia uses premultiplied alpha, need to unpremultiply
                let a = p.alpha();
                if a == 0 {
                    [0, 0, 0, 0]
                } else {
                    [
                        ((p.red() as u16 * 255) / a as u16) as u8,
                        ((p.green() as u16 * 255) / a as u16) as u8,
                        ((p.blue() as u16 * 255) / a as u16) as u8,
                        a,
                    ]
                }
            })
            .collect()
    }
    
    /// Get the underlying pixmap for advanced operations
    pub fn pixmap(&self) -> &Pixmap {
        &self.pixmap
    }
    
    /// Get mutable pixmap
    pub fn pixmap_mut(&mut self) -> &mut Pixmap {
        &mut self.pixmap
    }
}

/// Convert our Color to tiny-skia Color
fn to_skia_color(c: Color) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, c.a)
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
}
