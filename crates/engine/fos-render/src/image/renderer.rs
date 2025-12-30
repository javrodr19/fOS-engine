//! Image renderer for drawing images to canvas
//!
//! Handles scaling, aspect ratio, and object-fit/object-position.

use crate::{Canvas, Color};
use super::{DecodedImage, ImageCache, ImageKey, ImageDecoder};

/// Scaling mode for images
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ScaleMode {
    /// Stretch to fill (may distort)
    Fill,
    /// Scale to fit, maintain aspect ratio (may letterbox)
    #[default]
    Contain,
    /// Scale to cover, maintain aspect ratio (may crop)
    Cover,
    /// No scaling
    None,
}

/// Image position anchor
#[derive(Debug, Clone, Copy, Default)]
pub struct ImagePosition {
    /// Horizontal position (0.0 = left, 0.5 = center, 1.0 = right)
    pub x: f32,
    /// Vertical position (0.0 = top, 0.5 = center, 1.0 = bottom)
    pub y: f32,
}

impl ImagePosition {
    pub const CENTER: Self = Self { x: 0.5, y: 0.5 };
    pub const TOP_LEFT: Self = Self { x: 0.0, y: 0.0 };
    pub const TOP_RIGHT: Self = Self { x: 1.0, y: 0.0 };
    pub const BOTTOM_LEFT: Self = Self { x: 0.0, y: 1.0 };
    pub const BOTTOM_RIGHT: Self = Self { x: 1.0, y: 1.0 };
}

/// Image renderer with caching
pub struct ImageRenderer {
    /// Image cache
    pub cache: ImageCache,
}

impl ImageRenderer {
    /// Create a new image renderer
    pub fn new() -> Self {
        Self {
            cache: ImageCache::default(),
        }
    }
    
    /// Create with custom cache size
    pub fn with_cache_size(max_bytes: usize) -> Self {
        Self {
            cache: ImageCache::new(max_bytes),
        }
    }
    
    /// Draw an image from bytes
    pub fn draw_image(
        &mut self,
        canvas: &mut Canvas,
        data: &[u8],
        source: &str,
        dest_x: f32,
        dest_y: f32,
        dest_width: f32,
        dest_height: f32,
        mode: ScaleMode,
        position: ImagePosition,
    ) {
        // Get or decode image
        let key = ImageKey::original(source);
        let image = self.cache.get_or_insert_with(key, || {
            ImageDecoder::decode(data).ok()
        }).cloned();  // Clone to avoid borrow conflict
        
        if let Some(ref img) = image {
            draw_image_to_canvas(canvas, img, dest_x, dest_y, dest_width, dest_height, mode, position);
        }
    }
    
    /// Draw an already decoded image
    pub fn draw_decoded(
        &self,
        canvas: &mut Canvas,
        image: &DecodedImage,
        dest_x: f32,
        dest_y: f32,
        dest_width: f32,
        dest_height: f32,
        mode: ScaleMode,
        position: ImagePosition,
    ) {
        draw_image_to_canvas(canvas, image, dest_x, dest_y, dest_width, dest_height, mode, position);
    }
}

/// Draw image to canvas (free function to avoid borrow conflicts)
fn draw_image_to_canvas(
    canvas: &mut Canvas,
    image: &DecodedImage,
    dest_x: f32,
    dest_y: f32,
    dest_width: f32,
    dest_height: f32,
    mode: ScaleMode,
    position: ImagePosition,
) {
    let (src_x, src_y, src_w, src_h, dst_x, dst_y, dst_w, dst_h) = 
        calculate_draw_params(
            image.width as f32, image.height as f32,
            dest_x, dest_y, dest_width, dest_height,
            mode, position,
        );
    
    // Draw pixels with scaling
    blit_scaled(
        canvas, image,
        src_x, src_y, src_w, src_h,
        dst_x, dst_y, dst_w, dst_h,
    );
}

/// Blit image with nearest-neighbor scaling
fn blit_scaled(
    canvas: &mut Canvas,
    image: &DecodedImage,
    src_x: f32, src_y: f32, src_w: f32, src_h: f32,
    dst_x: f32, dst_y: f32, dst_w: f32, dst_h: f32,
) {
    let scale_x = src_w / dst_w;
    let scale_y = src_h / dst_h;
    
    for py in 0..(dst_h as i32) {
        for px in 0..(dst_w as i32) {
            // Map to source coordinates
            let sx = src_x + (px as f32 + 0.5) * scale_x;
            let sy = src_y + (py as f32 + 0.5) * scale_y;
            
            // Sample from source (nearest neighbor for speed)
            let sx_i = sx as u32;
            let sy_i = sy as u32;
            
            if let Some([r, g, b, a]) = image.get_pixel(sx_i, sy_i) {
                if a > 0 {
                    let canvas_x = (dst_x as i32 + px) as u32;
                    let canvas_y = (dst_y as i32 + py) as u32;
                    
                    if a == 255 {
                        canvas.set_pixel(canvas_x, canvas_y, Color::rgba(r, g, b, a));
                    } else {
                        // Alpha blend
                        if let Some(bg) = canvas.get_pixel(canvas_x, canvas_y) {
                            let blended = blend_alpha(bg, Color::rgba(r, g, b, a));
                            canvas.set_pixel(canvas_x, canvas_y, blended);
                        }
                    }
                }
            }
        }
    }
}

impl Default for ImageRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate source and destination rectangles based on scaling mode
fn calculate_draw_params(
    img_w: f32, img_h: f32,
    dest_x: f32, dest_y: f32, dest_w: f32, dest_h: f32,
    mode: ScaleMode,
    position: ImagePosition,
) -> (f32, f32, f32, f32, f32, f32, f32, f32) {
    match mode {
        ScaleMode::Fill => {
            // Use full source and destination
            (0.0, 0.0, img_w, img_h, dest_x, dest_y, dest_w, dest_h)
        }
        ScaleMode::Contain => {
            // Scale to fit, preserve aspect ratio
            let scale = (dest_w / img_w).min(dest_h / img_h);
            let scaled_w = img_w * scale;
            let scaled_h = img_h * scale;
            let offset_x = (dest_w - scaled_w) * position.x;
            let offset_y = (dest_h - scaled_h) * position.y;
            (0.0, 0.0, img_w, img_h, dest_x + offset_x, dest_y + offset_y, scaled_w, scaled_h)
        }
        ScaleMode::Cover => {
            // Scale to cover, preserve aspect ratio (crop excess)
            let scale = (dest_w / img_w).max(dest_h / img_h);
            let scaled_w = img_w * scale;
            let scaled_h = img_h * scale;
            
            // Calculate source crop
            let crop_x = (scaled_w - dest_w) * position.x / scale;
            let crop_y = (scaled_h - dest_h) * position.y / scale;
            let src_w = dest_w / scale;
            let src_h = dest_h / scale;
            
            (crop_x, crop_y, src_w, src_h, dest_x, dest_y, dest_w, dest_h)
        }
        ScaleMode::None => {
            // No scaling, use natural size
            let offset_x = (dest_w - img_w) * position.x;
            let offset_y = (dest_h - img_h) * position.y;
            (0.0, 0.0, img_w, img_h, dest_x + offset_x, dest_y + offset_y, img_w, img_h)
        }
    }
}

/// Alpha blend foreground onto background
fn blend_alpha(bg: Color, fg: Color) -> Color {
    let a = fg.a as f32 / 255.0;
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
    fn test_contain_scaling() {
        // 100x50 image into 200x200 box
        let (_, _, _, _, dx, dy, dw, dh) = calculate_draw_params(
            100.0, 50.0,
            0.0, 0.0, 200.0, 200.0,
            ScaleMode::Contain,
            ImagePosition::CENTER,
        );
        
        // Should scale to 200x100, centered vertically
        assert_eq!(dw, 200.0);
        assert_eq!(dh, 100.0);
        assert_eq!(dy, 50.0); // Centered in 200px height
    }
    
    #[test]
    fn test_cover_scaling() {
        // 100x50 image into 100x100 box
        let (sx, sy, sw, sh, _, _, _, _) = calculate_draw_params(
            100.0, 50.0,
            0.0, 0.0, 100.0, 100.0,
            ScaleMode::Cover,
            ImagePosition::CENTER,
        );
        
        // Should use center 50x50 of source
        assert_eq!(sw, 50.0);
        assert_eq!(sh, 50.0);
        assert_eq!(sx, 25.0); // Centered
    }
}
