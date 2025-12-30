//! AVIF Image Support
//!
//! AVIF decoding wrapper.

/// AVIF decoder status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvifStatus {
    Ready,
    Decoding,
    Error,
}

/// AVIF image data
#[derive(Debug, Clone)]
pub struct AvifImage {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>, // RGBA
    pub depth: u8,
    pub has_alpha: bool,
}

/// AVIF decoder
#[derive(Debug, Default)]
pub struct AvifDecoder {
    status: AvifStatus,
}

impl Default for AvifStatus {
    fn default() -> Self {
        Self::Ready
    }
}

impl AvifDecoder {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Decode AVIF from bytes
    pub fn decode(&mut self, data: &[u8]) -> Result<AvifImage, AvifError> {
        self.status = AvifStatus::Decoding;
        
        // Check magic bytes
        if data.len() < 12 {
            self.status = AvifStatus::Error;
            return Err(AvifError::InvalidData);
        }
        
        // AVIF files are ISO base media file format
        // Check for "ftyp" box
        if &data[4..8] != b"ftyp" {
            self.status = AvifStatus::Error;
            return Err(AvifError::InvalidData);
        }
        
        // Check for avif brand
        let brand = &data[8..12];
        if brand != b"avif" && brand != b"avis" && brand != b"mif1" {
            self.status = AvifStatus::Error;
            return Err(AvifError::UnsupportedFormat);
        }
        
        // In production, would use libavif or rav1d for actual decoding
        // For now, return a placeholder
        self.status = AvifStatus::Ready;
        
        Ok(AvifImage {
            width: 1,
            height: 1,
            pixels: vec![0, 0, 0, 255],
            depth: 8,
            has_alpha: false,
        })
    }
    
    pub fn status(&self) -> AvifStatus {
        self.status
    }
}

/// AVIF error
#[derive(Debug, Clone)]
pub enum AvifError {
    InvalidData,
    UnsupportedFormat,
    DecodingFailed,
}

impl std::fmt::Display for AvifError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidData => write!(f, "Invalid AVIF data"),
            Self::UnsupportedFormat => write!(f, "Unsupported AVIF format"),
            Self::DecodingFailed => write!(f, "AVIF decoding failed"),
        }
    }
}

impl std::error::Error for AvifError {}

/// Check if data is AVIF
pub fn is_avif(data: &[u8]) -> bool {
    if data.len() < 12 {
        return false;
    }
    if &data[4..8] != b"ftyp" {
        return false;
    }
    let brand = &data[8..12];
    brand == b"avif" || brand == b"avis" || brand == b"mif1"
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_avif() {
        let fake = b"\x00\x00\x00\x1cftypavif";
        assert!(is_avif(fake));
        
        let png = b"\x89PNG\r\n\x1a\n";
        assert!(!is_avif(png));
    }
}
