//! ICO/Favicon Support
//!
//! Decoder for Windows ICO format and web favicons.

/// ICO image container
#[derive(Debug, Clone)]
pub struct IcoImage {
    /// Individual images in the ICO file
    pub images: Vec<IcoEntry>,
}

/// Individual image entry in an ICO file
#[derive(Debug, Clone)]
pub struct IcoEntry {
    /// Width (0 means 256)
    pub width: u8,
    /// Height (0 means 256)
    pub height: u8,
    /// Number of colors in palette (0 = no palette)
    pub color_count: u8,
    /// Color planes
    pub planes: u16,
    /// Bits per pixel
    pub bit_count: u16,
    /// Image data (decoded RGBA)
    pub data: Vec<u8>,
    /// Image format
    pub format: IcoFormat,
}

impl IcoEntry {
    /// Get actual width (256 if stored as 0)
    pub fn actual_width(&self) -> u32 {
        if self.width == 0 { 256 } else { self.width as u32 }
    }
    
    /// Get actual height (256 if stored as 0)
    pub fn actual_height(&self) -> u32 {
        if self.height == 0 { 256 } else { self.height as u32 }
    }
    
    /// Get pixel count
    pub fn pixel_count(&self) -> u32 {
        self.actual_width() * self.actual_height()
    }
}

/// ICO embedded image format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IcoFormat {
    /// BMP (DIB) format
    Bmp,
    /// PNG format
    Png,
}

/// ICO decoder
#[derive(Debug, Default)]
pub struct IcoDecoder;

impl IcoDecoder {
    pub fn new() -> Self {
        Self
    }
    
    /// Decode ICO file
    pub fn decode(&self, data: &[u8]) -> Result<IcoImage, IcoError> {
        if data.len() < 6 {
            return Err(IcoError::TooSmall);
        }
        
        // Check header
        if data[0] != 0 || data[1] != 0 {
            return Err(IcoError::InvalidHeader);
        }
        
        let image_type = u16::from_le_bytes([data[2], data[3]]);
        if image_type != 1 && image_type != 2 {
            return Err(IcoError::InvalidType);
        }
        
        let count = u16::from_le_bytes([data[4], data[5]]) as usize;
        if count == 0 {
            return Err(IcoError::NoImages);
        }
        
        let mut images = Vec::with_capacity(count);
        let mut offset = 6;
        
        for _ in 0..count {
            if offset + 16 > data.len() {
                break;
            }
            
            let width = data[offset];
            let height = data[offset + 1];
            let color_count = data[offset + 2];
            // reserved = data[offset + 3]
            let planes = u16::from_le_bytes([data[offset + 4], data[offset + 5]]);
            let bit_count = u16::from_le_bytes([data[offset + 6], data[offset + 7]]);
            let size = u32::from_le_bytes([data[offset + 8], data[offset + 9], data[offset + 10], data[offset + 11]]) as usize;
            let img_offset = u32::from_le_bytes([data[offset + 12], data[offset + 13], data[offset + 14], data[offset + 15]]) as usize;
            
            offset += 16;
            
            // Get image data
            if img_offset + size <= data.len() {
                let img_data = &data[img_offset..img_offset + size];
                
                // Determine format
                let format = if is_png(img_data) {
                    IcoFormat::Png
                } else {
                    IcoFormat::Bmp
                };
                
                // Decode to RGBA
                let decoded = self.decode_entry(img_data, format, width, height)?;
                
                images.push(IcoEntry {
                    width,
                    height,
                    color_count,
                    planes,
                    bit_count,
                    data: decoded,
                    format,
                });
            }
        }
        
        Ok(IcoImage { images })
    }
    
    fn decode_entry(&self, data: &[u8], format: IcoFormat, width: u8, height: u8) -> Result<Vec<u8>, IcoError> {
        match format {
            IcoFormat::Png => {
                // Use image crate to decode PNG
                // For now, placeholder
                let w = if width == 0 { 256 } else { width as u32 };
                let h = if height == 0 { 256 } else { height as u32 };
                Ok(vec![0u8; (w * h * 4) as usize])
            }
            IcoFormat::Bmp => {
                // Decode BMP DIB format
                // For now, placeholder
                let w = if width == 0 { 256 } else { width as u32 };
                let h = if height == 0 { 256 } else { height as u32 };
                Ok(vec![0u8; (w * h * 4) as usize])
            }
        }
    }
    
    /// Get best icon for size
    pub fn best_for_size(ico: &IcoImage, target_width: u32) -> Option<&IcoEntry> {
        // Find exact match first
        if let Some(entry) = ico.images.iter().find(|e| e.actual_width() == target_width) {
            return Some(entry);
        }
        
        // Find next larger
        if let Some(entry) = ico.images.iter()
            .filter(|e| e.actual_width() > target_width)
            .min_by_key(|e| e.actual_width())
        {
            return Some(entry);
        }
        
        // Fallback to largest
        ico.images.iter().max_by_key(|e| e.actual_width())
    }
}

/// Check if data is PNG
fn is_png(data: &[u8]) -> bool {
    data.len() >= 8 && &data[0..8] == b"\x89PNG\r\n\x1a\n"
}

/// Check if data is ICO
pub fn is_ico(data: &[u8]) -> bool {
    data.len() >= 4 && data[0] == 0 && data[1] == 0 && 
    (data[2] == 1 || data[2] == 2) && data[3] == 0
}

/// ICO errors
#[derive(Debug, thiserror::Error)]
pub enum IcoError {
    #[error("File too small")]
    TooSmall,
    #[error("Invalid header")]
    InvalidHeader,
    #[error("Invalid image type")]
    InvalidType,
    #[error("No images in file")]
    NoImages,
    #[error("Invalid image data")]
    InvalidData,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_ico() {
        let ico_header = [0, 0, 1, 0, 1, 0]; // Valid ICO
        assert!(is_ico(&ico_header));
        
        let not_ico = [0x89, b'P', b'N', b'G'];
        assert!(!is_ico(&not_ico));
    }
    
    #[test]
    fn test_ico_entry() {
        let entry = IcoEntry {
            width: 0,
            height: 0,
            color_count: 0,
            planes: 1,
            bit_count: 32,
            data: vec![],
            format: IcoFormat::Png,
        };
        
        assert_eq!(entry.actual_width(), 256);
        assert_eq!(entry.actual_height(), 256);
    }
}
