//! OffscreenCanvas
//!
//! Canvas that can be used in workers.

use crate::context2d::CanvasRenderingContext2D;

/// OffscreenCanvas - canvas for off-main-thread rendering
#[derive(Debug)]
pub struct OffscreenCanvas {
    width: u32,
    height: u32,
    context: Option<OffscreenContext>,
}

/// Offscreen context type
#[derive(Debug)]
pub enum OffscreenContext {
    Canvas2D(CanvasRenderingContext2D),
    // WebGL would go here
}

/// ImageBitmap - transferable image
#[derive(Debug, Clone)]
pub struct ImageBitmap {
    data: Vec<u8>,
    width: u32,
    height: u32,
    premultiplied_alpha: bool,
    color_space_conversion: ColorSpaceConversion,
    resize_quality: ResizeQuality,
}

/// Color space conversion
#[derive(Debug, Clone, Copy, Default)]
pub enum ColorSpaceConversion {
    #[default]
    Default,
    None,
}

/// Resize quality
#[derive(Debug, Clone, Copy, Default)]
pub enum ResizeQuality {
    Pixelated,
    Low,
    #[default]
    Medium,
    High,
}

impl OffscreenCanvas {
    /// Create new offscreen canvas
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            context: None,
        }
    }
    
    /// Get width
    pub fn width(&self) -> u32 {
        self.width
    }
    
    /// Get height
    pub fn height(&self) -> u32 {
        self.height
    }
    
    /// Set width
    pub fn set_width(&mut self, width: u32) {
        self.width = width;
        // Would recreate context
    }
    
    /// Set height
    pub fn set_height(&mut self, height: u32) {
        self.height = height;
        // Would recreate context
    }
    
    /// Get 2D context
    pub fn get_context_2d(&mut self) -> Option<&mut CanvasRenderingContext2D> {
        if self.context.is_none() {
            self.context = Some(OffscreenContext::Canvas2D(
                CanvasRenderingContext2D::new(self.width, self.height)
            ));
        }
        
        match &mut self.context {
            Some(OffscreenContext::Canvas2D(ctx)) => Some(ctx),
            _ => None,
        }
    }
    
    /// Transfer to ImageBitmap
    pub fn transfer_to_image_bitmap(&self) -> ImageBitmap {
        let data = match &self.context {
            Some(OffscreenContext::Canvas2D(ctx)) => ctx.data().to_vec(),
            None => vec![0u8; (self.width * self.height * 4) as usize],
        };
        
        ImageBitmap {
            data,
            width: self.width,
            height: self.height,
            premultiplied_alpha: false,
            color_space_conversion: ColorSpaceConversion::default(),
            resize_quality: ResizeQuality::default(),
        }
    }
    
    /// Convert to blob
    pub fn convert_to_blob(&self, _options: BlobOptions) -> Vec<u8> {
        // Would encode to PNG/JPEG
        Vec::new()
    }
}

/// Blob options
#[derive(Debug, Clone, Default)]
pub struct BlobOptions {
    pub mime_type: Option<String>,
    pub quality: Option<f64>,
}

impl ImageBitmap {
    /// Get width
    pub fn width(&self) -> u32 {
        self.width
    }
    
    /// Get height
    pub fn height(&self) -> u32 {
        self.height
    }
    
    /// Get data
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    
    /// Close and release resources
    pub fn close(self) {
        // Resources dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_offscreen_canvas() {
        let mut canvas = OffscreenCanvas::new(100, 100);
        assert_eq!(canvas.width(), 100);
        
        let ctx = canvas.get_context_2d().unwrap();
        ctx.fill_rect(0.0, 0.0, 50.0, 50.0);
    }
    
    #[test]
    fn test_transfer_to_bitmap() {
        let canvas = OffscreenCanvas::new(10, 10);
        let bitmap = canvas.transfer_to_image_bitmap();
        
        assert_eq!(bitmap.width(), 10);
        assert_eq!(bitmap.height(), 10);
    }
}
