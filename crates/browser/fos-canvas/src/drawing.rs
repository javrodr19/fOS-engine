//! Image Drawing
//!
//! Canvas 2D image methods.

use crate::image_data::ImageData;

/// Image source for drawImage
#[derive(Debug, Clone)]
pub enum CanvasImageSource {
    ImageData(ImageData),
    ImageBitmap { data: Vec<u8>, width: u32, height: u32 },
    Canvas { data: Vec<u8>, width: u32, height: u32 },
    Video { frame: Vec<u8>, width: u32, height: u32 },
}

impl CanvasImageSource {
    pub fn width(&self) -> u32 {
        match self {
            Self::ImageData(img) => img.width(),
            Self::ImageBitmap { width, .. } => *width,
            Self::Canvas { width, .. } => *width,
            Self::Video { width, .. } => *width,
        }
    }
    
    pub fn height(&self) -> u32 {
        match self {
            Self::ImageData(img) => img.height(),
            Self::ImageBitmap { height, .. } => *height,
            Self::Canvas { height, .. } => *height,
            Self::Video { height, .. } => *height,
        }
    }
    
    pub fn data(&self) -> &[u8] {
        match self {
            Self::ImageData(img) => img.data(),
            Self::ImageBitmap { data, .. } => data,
            Self::Canvas { data, .. } => data,
            Self::Video { frame, .. } => frame,
        }
    }
}

/// Image drawing trait for CanvasRenderingContext2D
pub trait ImageDrawing {
    /// Draw image at position
    fn draw_image(&mut self, image: &CanvasImageSource, dx: f64, dy: f64);
    
    /// Draw image with size
    fn draw_image_scaled(&mut self, image: &CanvasImageSource, dx: f64, dy: f64, dwidth: f64, dheight: f64);
    
    /// Draw image with source and destination rectangles
    fn draw_image_full(
        &mut self,
        image: &CanvasImageSource,
        sx: f64, sy: f64, swidth: f64, sheight: f64,
        dx: f64, dy: f64, dwidth: f64, dheight: f64,
    );
}

impl ImageDrawing for super::context2d::CanvasRenderingContext2D {
    fn draw_image(&mut self, image: &CanvasImageSource, dx: f64, dy: f64) {
        self.draw_image_scaled(image, dx, dy, image.width() as f64, image.height() as f64);
    }
    
    fn draw_image_scaled(&mut self, image: &CanvasImageSource, dx: f64, dy: f64, dwidth: f64, dheight: f64) {
        self.draw_image_full(
            image,
            0.0, 0.0, image.width() as f64, image.height() as f64,
            dx, dy, dwidth, dheight,
        );
    }
    
    fn draw_image_full(
        &mut self,
        image: &CanvasImageSource,
        sx: f64, sy: f64, swidth: f64, sheight: f64,
        dx: f64, dy: f64, dwidth: f64, dheight: f64,
    ) {
        let src_data = image.data();
        let src_width = image.width() as usize;
        
        let scale_x = swidth / dwidth;
        let scale_y = sheight / dheight;
        
        // Get dimensions before mutable borrow
        let dest_width = self.width() as usize;
        let dest_height = self.height() as usize;
        let dest_data = self.data_mut();
        
        for py in 0..(dheight as usize) {
            for px in 0..(dwidth as usize) {
                let dest_x = (dx as usize) + px;
                let dest_y = (dy as usize) + py;
                
                if dest_x >= dest_width || dest_y >= dest_height {
                    continue;
                }
                
                let src_x = (sx + (px as f64 * scale_x)) as usize;
                let src_y = (sy + (py as f64 * scale_y)) as usize;
                
                if src_x < src_width && src_y < (sheight as usize) {
                    let src_idx = (src_y * src_width + src_x) * 4;
                    let dst_idx = (dest_y * dest_width + dest_x) * 4;
                    
                    if src_idx + 3 < src_data.len() && dst_idx + 3 < dest_data.len() {
                        dest_data[dst_idx] = src_data[src_idx];
                        dest_data[dst_idx + 1] = src_data[src_idx + 1];
                        dest_data[dst_idx + 2] = src_data[src_idx + 2];
                        dest_data[dst_idx + 3] = src_data[src_idx + 3];
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CanvasRenderingContext2D;
    
    #[test]
    fn test_draw_image() {
        let mut ctx = CanvasRenderingContext2D::new(100, 100);
        let img = ImageData::new(10, 10);
        let src = CanvasImageSource::ImageData(img);
        
        ctx.draw_image(&src, 0.0, 0.0);
    }
}
