//! Canvas - pixel buffer

use crate::Color;

/// Pixel canvas
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<Color>,
}

impl Canvas {
    /// Create a new canvas
    pub fn new(width: u32, height: u32) -> Self {
        let size = (width * height) as usize;
        Self {
            width,
            height,
            pixels: vec![Color::WHITE; size],
        }
    }
    
    /// Set a pixel color
    pub fn set_pixel(&mut self, x: u32, y: u32, color: Color) {
        if x < self.width && y < self.height {
            let idx = (y * self.width + x) as usize;
            self.pixels[idx] = color;
        }
    }
    
    /// Fill a rectangle
    pub fn fill_rect(&mut self, x: u32, y: u32, w: u32, h: u32, color: Color) {
        for dy in 0..h {
            for dx in 0..w {
                self.set_pixel(x + dx, y + dy, color);
            }
        }
    }
    
    /// Get pixel data as raw bytes (RGBA)
    pub fn as_bytes(&self) -> Vec<u8> {
        self.pixels.iter().flat_map(|c| [c.r, c.g, c.b, c.a]).collect()
    }
}
