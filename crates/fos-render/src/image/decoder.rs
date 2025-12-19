//! Image decoder for various formats
//!
//! Supports PNG, JPEG, GIF, WebP via the image crate.

use image::{DynamicImage, ImageFormat as ImgFormat, GenericImageView};
use std::io::Cursor;

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
    
    /// Convert to image crate format
    fn to_image_format(self) -> Option<ImgFormat> {
        match self {
            Self::Png => Some(ImgFormat::Png),
            Self::Jpeg => Some(ImgFormat::Jpeg),
            Self::Gif => Some(ImgFormat::Gif),
            Self::WebP => Some(ImgFormat::WebP),
            Self::Unknown => None,
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
    /// Decode image from bytes
    pub fn decode(data: &[u8]) -> Result<DecodedImage, ImageError> {
        let format = ImageFormat::from_bytes(data);
        Self::decode_with_format(data, format)
    }
    
    /// Decode with known format
    pub fn decode_with_format(data: &[u8], format: ImageFormat) -> Result<DecodedImage, ImageError> {
        let img_format = format.to_image_format()
            .ok_or(ImageError::UnsupportedFormat)?;
        
        let img = image::load(Cursor::new(data), img_format)
            .map_err(|e| ImageError::DecodeFailed(e.to_string()))?;
        
        Ok(Self::image_to_decoded(img, format))
    }
    
    /// Decode and resize to target dimensions
    pub fn decode_resized(
        data: &[u8],
        target_width: u32,
        target_height: u32,
    ) -> Result<DecodedImage, ImageError> {
        let format = ImageFormat::from_bytes(data);
        let img_format = format.to_image_format()
            .ok_or(ImageError::UnsupportedFormat)?;
        
        let img = image::load(Cursor::new(data), img_format)
            .map_err(|e| ImageError::DecodeFailed(e.to_string()))?;
        
        // Resize using fast algorithm
        let resized = img.resize_exact(
            target_width,
            target_height,
            image::imageops::FilterType::Triangle,
        );
        
        Ok(Self::image_to_decoded(resized, format))
    }
    
    /// Convert DynamicImage to DecodedImage
    fn image_to_decoded(img: DynamicImage, format: ImageFormat) -> DecodedImage {
        let (width, height) = img.dimensions();
        let rgba = img.into_rgba8();
        
        DecodedImage {
            pixels: rgba.into_raw(),
            width,
            height,
            format,
        }
    }
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
