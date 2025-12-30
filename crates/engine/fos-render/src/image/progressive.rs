//! Progressive Image Decoding
//!
//! Support for progressive JPEG/PNG decoding to show low-quality previews first.

use std::sync::Arc;

/// Progressive decode state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeProgress {
    /// Not started
    NotStarted,
    /// Header parsed, dimensions known
    Header,
    /// Low-quality preview available
    Preview,
    /// Partial decode (percentage complete)
    Partial(u8),
    /// Fully decoded
    Complete,
    /// Decode failed
    Error,
}

/// Progressive image decoder
#[derive(Debug)]
pub struct ProgressiveDecoder {
    /// Image format
    format: ProgressiveFormat,
    /// Current decode state
    state: DecodeProgress,
    /// Image width (from header)
    width: u32,
    /// Image height (from header)
    height: u32,
    /// Current preview/partial data (RGBA)
    current_data: Vec<u8>,
    /// Received bytes
    received_bytes: Vec<u8>,
    /// Total expected bytes (if known)
    total_bytes: Option<usize>,
}

/// Formats that support progressive decoding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressiveFormat {
    /// Progressive JPEG
    Jpeg,
    /// Interlaced PNG
    Png,
    /// Animated GIF (frame-progressive)
    Gif,
    /// Other (non-progressive)
    Other,
}

impl ProgressiveDecoder {
    pub fn new(format: ProgressiveFormat) -> Self {
        Self {
            format,
            state: DecodeProgress::NotStarted,
            width: 0,
            height: 0,
            current_data: Vec::new(),
            received_bytes: Vec::new(),
            total_bytes: None,
        }
    }
    
    /// Set total expected size
    pub fn set_total_bytes(&mut self, total: usize) {
        self.total_bytes = Some(total);
    }
    
    /// Feed more data to decoder
    pub fn feed(&mut self, data: &[u8]) -> DecodeProgress {
        self.received_bytes.extend_from_slice(data);
        
        match self.state {
            DecodeProgress::NotStarted => {
                // Try to parse header
                if self.try_parse_header() {
                    self.state = DecodeProgress::Header;
                }
            }
            DecodeProgress::Header | DecodeProgress::Preview | DecodeProgress::Partial(_) => {
                // Try progressive decode
                self.try_decode_progressive();
            }
            _ => {}
        }
        
        self.state
    }
    
    /// Get current best available image
    pub fn current_image(&self) -> Option<&[u8]> {
        if self.current_data.is_empty() {
            None
        } else {
            Some(&self.current_data)
        }
    }
    
    /// Get image dimensions (may be 0 if not yet parsed)
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    
    /// Get current progress
    pub fn progress(&self) -> DecodeProgress {
        self.state
    }
    
    /// Check if decoding is complete
    pub fn is_complete(&self) -> bool {
        self.state == DecodeProgress::Complete
    }
    
    fn try_parse_header(&mut self) -> bool {
        match self.format {
            ProgressiveFormat::Jpeg => self.parse_jpeg_header(),
            ProgressiveFormat::Png => self.parse_png_header(),
            ProgressiveFormat::Gif => self.parse_gif_header(),
            ProgressiveFormat::Other => false,
        }
    }
    
    fn parse_jpeg_header(&mut self) -> bool {
        // JPEG SOI marker + find SOF marker
        if self.received_bytes.len() < 10 {
            return false;
        }
        
        // Check SOI
        if self.received_bytes[0..2] != [0xFF, 0xD8] {
            return false;
        }
        
        // Find SOF0 or SOF2 (progressive) marker
        let mut i = 2;
        while i + 4 < self.received_bytes.len() {
            if self.received_bytes[i] == 0xFF {
                let marker = self.received_bytes[i + 1];
                // SOF0, SOF1, SOF2 (progressive)
                if marker >= 0xC0 && marker <= 0xC2 {
                    if i + 9 < self.received_bytes.len() {
                        self.height = u16::from_be_bytes([
                            self.received_bytes[i + 5],
                            self.received_bytes[i + 6],
                        ]) as u32;
                        self.width = u16::from_be_bytes([
                            self.received_bytes[i + 7],
                            self.received_bytes[i + 8],
                        ]) as u32;
                        return true;
                    }
                }
                
                if marker == 0xD9 { // EOI
                    break;
                }
                
                // Skip segment
                if i + 4 < self.received_bytes.len() {
                    let length = u16::from_be_bytes([
                        self.received_bytes[i + 2],
                        self.received_bytes[i + 3],
                    ]) as usize;
                    i += 2 + length;
                } else {
                    break;
                }
            } else {
                i += 1;
            }
        }
        
        false
    }
    
    fn parse_png_header(&mut self) -> bool {
        // PNG signature + IHDR
        if self.received_bytes.len() < 24 {
            return false;
        }
        
        // Check signature
        if &self.received_bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
            return false;
        }
        
        // IHDR should be first chunk
        if &self.received_bytes[12..16] == b"IHDR" {
            self.width = u32::from_be_bytes([
                self.received_bytes[16],
                self.received_bytes[17],
                self.received_bytes[18],
                self.received_bytes[19],
            ]);
            self.height = u32::from_be_bytes([
                self.received_bytes[20],
                self.received_bytes[21],
                self.received_bytes[22],
                self.received_bytes[23],
            ]);
            return true;
        }
        
        false
    }
    
    fn parse_gif_header(&mut self) -> bool {
        if self.received_bytes.len() < 10 {
            return false;
        }
        
        // Check signature
        if &self.received_bytes[0..3] != b"GIF" {
            return false;
        }
        
        self.width = u16::from_le_bytes([
            self.received_bytes[6],
            self.received_bytes[7],
        ]) as u32;
        self.height = u16::from_le_bytes([
            self.received_bytes[8],
            self.received_bytes[9],
        ]) as u32;
        
        true
    }
    
    fn try_decode_progressive(&mut self) {
        // Calculate progress
        let progress = if let Some(total) = self.total_bytes {
            if total > 0 {
                ((self.received_bytes.len() * 100) / total).min(100) as u8
            } else {
                0
            }
        } else {
            0
        };
        
        // For progressive JPEG, we could decode each scan
        // For interlaced PNG, we decode each pass
        // For now, just update progress
        
        if progress < 30 {
            self.state = DecodeProgress::Header;
        } else if progress < 60 {
            self.state = DecodeProgress::Preview;
            // Create low-res preview
            self.create_preview();
        } else if progress < 100 {
            self.state = DecodeProgress::Partial(progress);
        } else {
            self.state = DecodeProgress::Complete;
            // Full decode
            self.decode_full();
        }
    }
    
    fn create_preview(&mut self) {
        // Create placeholder/blurry preview
        let size = (self.width * self.height * 4) as usize;
        if self.current_data.len() != size {
            self.current_data = vec![128u8; size]; // Gray placeholder
        }
    }
    
    fn decode_full(&mut self) {
        // In real implementation, use image crate
        let size = (self.width * self.height * 4) as usize;
        if self.current_data.len() != size {
            self.current_data = vec![255u8; size]; // White placeholder
        }
    }
}

/// Memory-mapped image loading
#[derive(Debug)]
pub struct MmapImage {
    /// Path to image file
    path: std::path::PathBuf,
    /// File size
    size: usize,
    /// Decoded dimensions
    width: u32,
    height: u32,
}

impl MmapImage {
    /// Open image file with memory mapping
    pub fn open(path: std::path::PathBuf) -> Result<Self, std::io::Error> {
        let metadata = std::fs::metadata(&path)?;
        Ok(Self {
            path,
            size: metadata.len() as usize,
            width: 0,
            height: 0,
        })
    }
    
    /// Get file size
    pub fn file_size(&self) -> usize {
        self.size
    }
    
    /// Decode just the header to get dimensions
    pub fn decode_header(&mut self) -> Result<(u32, u32), std::io::Error> {
        // In real implementation, memory-map and read just header
        Ok((self.width, self.height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_progressive_decoder() {
        let mut decoder = ProgressiveDecoder::new(ProgressiveFormat::Png);
        
        // Feed PNG header
        let png_header = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x64\x00\x00\x00\x64";
        decoder.feed(png_header);
        
        assert!(matches!(decoder.progress(), DecodeProgress::Header));
        assert_eq!(decoder.dimensions(), (100, 100));
    }
    
    #[test]
    fn test_progress_states() {
        assert_ne!(DecodeProgress::NotStarted, DecodeProgress::Header);
        assert_eq!(DecodeProgress::Partial(50), DecodeProgress::Partial(50));
    }
}
