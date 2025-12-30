//! Image decoder for various formats
//!
//! Supports PNG, JPEG, GIF, WebP via custom from-scratch decoders.

use super::decoders::{self, DecodeError};

/// Supported image formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    WebP,
    Unknown,
}

impl ImageFormat {
    /// Detect format from magic bytes
    pub fn from_bytes(data: &[u8]) -> Self {
        if data.len() < 8 {
            return Self::Unknown;
        }
        
        // PNG: 89 50 4E 47 0D 0A 1A 0A
        if data.starts_with(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]) {
            return Self::Png;
        }
        
        // JPEG: FF D8 FF
        if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
            return Self::Jpeg;
        }
        
        // GIF: GIF87a or GIF89a
        if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            return Self::Gif;
        }
        
        // WebP: RIFF....WEBP
        if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
            return Self::WebP;
        }
        
        Self::Unknown
    }
    
    /// Get format from file extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "png" => Self::Png,
            "jpg" | "jpeg" => Self::Jpeg,
            "gif" => Self::Gif,
            "webp" => Self::WebP,
            _ => Self::Unknown,
        }
    }
}

/// A decoded image ready for rendering
#[derive(Debug, Clone)]
pub struct DecodedImage {
    /// RGBA pixel data
    pub pixels: Vec<u8>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Original format
    pub format: ImageFormat,
}

impl DecodedImage {
    /// Create from raw RGBA data
    pub fn from_rgba(pixels: Vec<u8>, width: u32, height: u32) -> Self {
        Self {
            pixels,
            width,
            height,
            format: ImageFormat::Unknown,
        }
    }
    
    /// Memory size in bytes
    pub fn memory_size(&self) -> usize {
        self.pixels.len()
    }
    
    /// Get pixel at (x, y)
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let idx = ((y * self.width + x) * 4) as usize;
        if idx + 4 <= self.pixels.len() {
            Some([
                self.pixels[idx],
                self.pixels[idx + 1],
                self.pixels[idx + 2],
                self.pixels[idx + 3],
            ])
        } else {
            None
        }
    }
}

/// Image decoder
pub struct ImageDecoder;

impl ImageDecoder {
    /// Decode image from bytes using custom decoders
    pub fn decode(data: &[u8]) -> Result<DecodedImage, ImageError> {
        decoders::decode(data).map_err(|e| ImageError::DecodeFailed(e.to_string()))
    }
    
    /// Decode with known format
    pub fn decode_with_format(data: &[u8], format: ImageFormat) -> Result<DecodedImage, ImageError> {
        decoders::decode_format(data, format).map_err(|e| ImageError::DecodeFailed(e.to_string()))
    }
    
    /// Decode and resize to target dimensions
    pub fn decode_resized(
        data: &[u8],
        target_width: u32,
        target_height: u32,
    ) -> Result<DecodedImage, ImageError> {
        let mut img = Self::decode(data)?;
        
        // Simple nearest-neighbor resize
        if img.width != target_width || img.height != target_height {
            let new_pixels = resize_nearest(&img.pixels, img.width, img.height, target_width, target_height);
            img.pixels = new_pixels;
            img.width = target_width;
            img.height = target_height;
        }
        
        Ok(img)
    }
}

/// Nearest-neighbor resize
fn resize_nearest(
    src: &[u8],
    src_width: u32,
    src_height: u32,
    dst_width: u32,
    dst_height: u32,
) -> Vec<u8> {
    let mut dst = vec![0u8; (dst_width * dst_height * 4) as usize];
    
    let x_ratio = src_width as f32 / dst_width as f32;
    let y_ratio = src_height as f32 / dst_height as f32;
    
    for y in 0..dst_height {
        for x in 0..dst_width {
            let src_x = ((x as f32 + 0.5) * x_ratio) as u32;
            let src_y = ((y as f32 + 0.5) * y_ratio) as u32;
            
            let src_idx = ((src_y * src_width + src_x) * 4) as usize;
            let dst_idx = ((y * dst_width + x) * 4) as usize;
            
            if src_idx + 4 <= src.len() && dst_idx + 4 <= dst.len() {
                dst[dst_idx..dst_idx + 4].copy_from_slice(&src[src_idx..src_idx + 4]);
            }
        }
    }
    
    dst
}

/// Image decoding errors
#[derive(Debug, Clone)]
pub enum ImageError {
    UnsupportedFormat,
    DecodeFailed(String),
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat => write!(f, "Unsupported image format"),
            Self::DecodeFailed(e) => write!(f, "Decode failed: {}", e),
        }
    }
}

impl std::error::Error for ImageError {}

impl From<DecodeError> for ImageError {
    fn from(e: DecodeError) -> Self {
        ImageError::DecodeFailed(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_format_detection_png() {
        let png_header = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
        assert_eq!(ImageFormat::from_bytes(&png_header), ImageFormat::Png);
    }
    
    #[test]
    fn test_format_detection_jpeg() {
        let jpeg_header = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
        assert_eq!(ImageFormat::from_bytes(&jpeg_header), ImageFormat::Jpeg);
    }
    
    #[test]
    fn test_format_from_extension() {
        assert_eq!(ImageFormat::from_extension("png"), ImageFormat::Png);
        assert_eq!(ImageFormat::from_extension("JPG"), ImageFormat::Jpeg);
        assert_eq!(ImageFormat::from_extension("webp"), ImageFormat::WebP);
    }
}
