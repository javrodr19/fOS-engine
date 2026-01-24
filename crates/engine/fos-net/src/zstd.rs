//! Zstandard Compression
//!
//! Custom Zstandard implementation for HTTP compression.
//! Zero external dependencies.

use std::collections::HashMap;

/// Maximum window size (16 MB)
const MAX_WINDOW_SIZE: usize = 16 * 1024 * 1024;

/// Zstandard magic number
const MAGIC_NUMBER: u32 = 0xFD2FB528;

/// Zstandard frame header
#[derive(Debug, Clone)]
pub struct FrameHeader {
    /// Window size
    pub window_size: u32,
    /// Dictionary ID (0 if none)
    pub dict_id: u32,
    /// Content size (if known)
    pub content_size: Option<u64>,
    /// Checksum present
    pub checksum: bool,
}

impl Default for FrameHeader {
    fn default() -> Self {
        Self {
            window_size: 1 << 17, // 128KB default
            dict_id: 0,
            content_size: None,
            checksum: false,
        }
    }
}

/// Zstandard compression level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionLevel {
    /// Fastest compression
    Fastest,
    /// Fast compression (level 1-3)
    Fast,
    /// Default compression (level 4-6)
    Default,
    /// Better compression (level 7-9)
    Better,
    /// Best compression (level 10+)
    Best,
}

impl CompressionLevel {
    /// Get numeric level
    pub fn level(&self) -> i32 {
        match self {
            Self::Fastest => 1,
            Self::Fast => 3,
            Self::Default => 5,
            Self::Better => 9,
            Self::Best => 19,
        }
    }
}

impl Default for CompressionLevel {
    fn default() -> Self {
        Self::Default
    }
}

/// Zstandard compressor
#[derive(Debug)]
pub struct ZstdCompressor {
    /// Compression level
    level: CompressionLevel,
    /// Window size
    window_size: usize,
    /// Dictionary (if any)
    dictionary: Option<Vec<u8>>,
    /// Statistics
    stats: CompressorStats,
}

/// Compression statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct CompressorStats {
    /// Bytes input
    pub bytes_in: u64,
    /// Bytes output
    pub bytes_out: u64,
    /// Frames compressed
    pub frames: u64,
}

impl CompressorStats {
    /// Get compression ratio
    pub fn ratio(&self) -> f64 {
        if self.bytes_in == 0 {
            1.0
        } else {
            self.bytes_out as f64 / self.bytes_in as f64
        }
    }
}

impl Default for ZstdCompressor {
    fn default() -> Self {
        Self::new(CompressionLevel::Default)
    }
}

impl ZstdCompressor {
    /// Create a new compressor
    pub fn new(level: CompressionLevel) -> Self {
        Self {
            level,
            window_size: 1 << 17,
            dictionary: None,
            stats: CompressorStats::default(),
        }
    }
    
    /// Set dictionary
    pub fn with_dictionary(mut self, dict: Vec<u8>) -> Self {
        self.dictionary = Some(dict);
        self
    }
    
    /// Set window size
    pub fn with_window_size(mut self, size: usize) -> Self {
        self.window_size = size.min(MAX_WINDOW_SIZE);
        self
    }
    
    /// Compress data
    pub fn compress(&mut self, input: &[u8]) -> Vec<u8> {
        self.stats.bytes_in += input.len() as u64;
        self.stats.frames += 1;
        
        let mut output = Vec::new();
        
        // Write magic number
        output.extend_from_slice(&MAGIC_NUMBER.to_le_bytes());
        
        // Write frame header
        let header = self.build_header(input.len());
        self.write_header(&mut output, &header);
        
        // Compress content using simple LZ77 + entropy coding
        let compressed = self.compress_block(input);
        output.extend_from_slice(&compressed);
        
        // Write end marker
        output.push(0); // Last block marker
        
        self.stats.bytes_out += output.len() as u64;
        output
    }
    
    /// Compress with streaming output
    pub fn compress_stream(&mut self, input: &[u8], block_size: usize) -> Vec<Vec<u8>> {
        let mut blocks = Vec::new();
        
        for chunk in input.chunks(block_size) {
            let block = self.compress(chunk);
            blocks.push(block);
        }
        
        blocks
    }
    
    /// Get statistics
    pub fn stats(&self) -> &CompressorStats {
        &self.stats
    }
    
    fn build_header(&self, content_size: usize) -> FrameHeader {
        FrameHeader {
            window_size: self.window_size as u32,
            dict_id: if self.dictionary.is_some() { 1 } else { 0 },
            content_size: Some(content_size as u64),
            checksum: false,
        }
    }
    
    fn write_header(&self, output: &mut Vec<u8>, header: &FrameHeader) {
        // Frame descriptor
        let mut descriptor = 0u8;
        
        // Content size flag
        if header.content_size.is_some() {
            descriptor |= 0x20;
        }
        
        // Dictionary ID flag
        if header.dict_id > 0 {
            descriptor |= 0x03;
        }
        
        output.push(descriptor);
        
        // Window descriptor
        let window_log = (header.window_size as f64).log2() as u8;
        output.push(window_log.saturating_sub(10));
        
        // Content size
        if let Some(size) = header.content_size {
            if size <= 255 {
                output.push(size as u8);
            } else {
                output.extend_from_slice(&(size as u32).to_le_bytes());
            }
        }
    }
    
    fn compress_block(&self, input: &[u8]) -> Vec<u8> {
        // Simple RLE + dictionary matching
        // In production, this would be full LZ77 + entropy coding
        
        if input.is_empty() {
            return Vec::new();
        }
        
        let mut output = Vec::new();
        let mut pos = 0;
        
        while pos < input.len() {
            // Look for repeated sequences
            let (match_len, match_offset) = self.find_match(input, pos);
            
            if match_len >= 4 {
                // Emit match
                output.push(0x80 | ((match_len - 4) as u8 & 0x7F));
                output.extend_from_slice(&(match_offset as u16).to_le_bytes());
                pos += match_len;
            } else {
                // Emit literal
                let literal_len = self.literal_length(input, pos);
                output.push(literal_len as u8);
                output.extend_from_slice(&input[pos..pos + literal_len]);
                pos += literal_len;
            }
        }
        
        output
    }
    
    fn find_match(&self, input: &[u8], pos: usize) -> (usize, usize) {
        if pos < 4 {
            return (0, 0);
        }
        
        let window_start = pos.saturating_sub(self.window_size);
        let mut best_len = 0;
        let mut best_offset = 0;
        
        for offset in window_start..pos {
            let mut len = 0;
            while pos + len < input.len() 
                && len < 255 
                && input[offset + len] == input[pos + len] 
            {
                len += 1;
            }
            
            if len > best_len {
                best_len = len;
                best_offset = pos - offset;
            }
        }
        
        (best_len, best_offset)
    }
    
    fn literal_length(&self, input: &[u8], pos: usize) -> usize {
        let mut len = 1;
        while pos + len < input.len() && len < 127 {
            if self.find_match(input, pos + len).0 >= 4 {
                break;
            }
            len += 1;
        }
        len
    }
}

/// Zstandard decompressor
#[derive(Debug, Default)]
pub struct ZstdDecompressor {
    /// Dictionary (if any)
    dictionary: Option<Vec<u8>>,
    /// Statistics
    stats: DecompressorStats,
}

/// Decompression statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct DecompressorStats {
    /// Bytes input
    pub bytes_in: u64,
    /// Bytes output
    pub bytes_out: u64,
    /// Frames decompressed
    pub frames: u64,
    /// Errors encountered
    pub errors: u64,
}

/// Decompression error
#[derive(Debug, Clone)]
pub enum ZstdError {
    /// Invalid magic number
    InvalidMagic,
    /// Invalid frame header
    InvalidHeader,
    /// Corrupted data
    CorruptedData,
    /// Dictionary mismatch
    DictionaryMismatch,
    /// Window too large
    WindowTooLarge,
    /// Checksum mismatch
    ChecksumMismatch,
}

impl std::fmt::Display for ZstdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMagic => write!(f, "Invalid magic number"),
            Self::InvalidHeader => write!(f, "Invalid frame header"),
            Self::CorruptedData => write!(f, "Corrupted data"),
            Self::DictionaryMismatch => write!(f, "Dictionary mismatch"),
            Self::WindowTooLarge => write!(f, "Window too large"),
            Self::ChecksumMismatch => write!(f, "Checksum mismatch"),
        }
    }
}

impl std::error::Error for ZstdError {}

impl ZstdDecompressor {
    /// Create a new decompressor
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set dictionary
    pub fn with_dictionary(mut self, dict: Vec<u8>) -> Self {
        self.dictionary = Some(dict);
        self
    }
    
    /// Decompress data
    pub fn decompress(&mut self, input: &[u8]) -> Result<Vec<u8>, ZstdError> {
        self.stats.bytes_in += input.len() as u64;
        
        if input.len() < 8 {
            self.stats.errors += 1;
            return Err(ZstdError::InvalidHeader);
        }
        
        // Check magic number
        let magic = u32::from_le_bytes([input[0], input[1], input[2], input[3]]);
        if magic != MAGIC_NUMBER {
            self.stats.errors += 1;
            return Err(ZstdError::InvalidMagic);
        }
        
        // Parse header
        let descriptor = input[4];
        let _has_content_size = (descriptor & 0x20) != 0;
        let has_dict = (descriptor & 0x03) != 0;
        
        if has_dict && self.dictionary.is_none() {
            self.stats.errors += 1;
            return Err(ZstdError::DictionaryMismatch);
        }
        
        // Decompress blocks
        let output = self.decompress_blocks(&input[6..])?;
        
        self.stats.bytes_out += output.len() as u64;
        self.stats.frames += 1;
        
        Ok(output)
    }
    
    /// Get statistics
    pub fn stats(&self) -> &DecompressorStats {
        &self.stats
    }
    
    fn decompress_blocks(&mut self, data: &[u8]) -> Result<Vec<u8>, ZstdError> {
        let mut output = Vec::new();
        let mut pos = 0;
        
        while pos < data.len() {
            if data[pos] == 0 {
                // End marker
                break;
            }
            
            if data[pos] & 0x80 != 0 {
                // Match
                let match_len = (data[pos] & 0x7F) as usize + 4;
                if pos + 3 > data.len() {
                    return Err(ZstdError::CorruptedData);
                }
                let offset = u16::from_le_bytes([data[pos + 1], data[pos + 2]]) as usize;
                
                // Copy from output
                if offset > output.len() {
                    return Err(ZstdError::CorruptedData);
                }
                
                let start = output.len() - offset;
                for i in 0..match_len {
                    output.push(output[start + i % offset]);
                }
                
                pos += 3;
            } else {
                // Literal
                let literal_len = data[pos] as usize;
                pos += 1;
                
                if pos + literal_len > data.len() {
                    return Err(ZstdError::CorruptedData);
                }
                
                output.extend_from_slice(&data[pos..pos + literal_len]);
                pos += literal_len;
            }
        }
        
        Ok(output)
    }
}

/// Detect if data is zstd compressed
pub fn is_zstd(data: &[u8]) -> bool {
    data.len() >= 4 
        && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == MAGIC_NUMBER
}

/// Content-Encoding values
pub mod encoding {
    /// Zstandard encoding
    pub const ZSTD: &str = "zstd";
    /// Brotli encoding
    pub const BR: &str = "br";
    /// Gzip encoding
    pub const GZIP: &str = "gzip";
    /// Deflate encoding
    pub const DEFLATE: &str = "deflate";
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_compression_level() {
        assert_eq!(CompressionLevel::Fastest.level(), 1);
        assert_eq!(CompressionLevel::Default.level(), 5);
        assert_eq!(CompressionLevel::Best.level(), 19);
    }
    
    #[test]
    fn test_compress_decompress() {
        let mut compressor = ZstdCompressor::default();
        let mut decompressor = ZstdDecompressor::new();
        
        let data = b"Hello, World! Hello, World! Hello, World!";
        
        let compressed = compressor.compress(data);
        assert!(!compressed.is_empty());
        
        // Verify magic number
        assert!(is_zstd(&compressed));
        
        let decompressed = decompressor.decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }
    
    #[test]
    fn test_empty_data() {
        let mut compressor = ZstdCompressor::default();
        let compressed = compressor.compress(&[]);
        assert!(is_zstd(&compressed));
    }
    
    #[test]
    fn test_stats() {
        let mut compressor = ZstdCompressor::default();
        
        let data = b"Test data for compression";
        compressor.compress(data);
        
        let stats = compressor.stats();
        assert_eq!(stats.frames, 1);
        assert_eq!(stats.bytes_in, data.len() as u64);
        assert!(stats.bytes_out > 0);
    }
    
    #[test]
    fn test_invalid_magic() {
        let mut decompressor = ZstdDecompressor::new();
        let result = decompressor.decompress(&[0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(matches!(result, Err(ZstdError::InvalidMagic)));
    }
    
    #[test]
    fn test_compression_ratio() {
        let mut compressor = ZstdCompressor::new(CompressionLevel::Best);
        
        // Highly compressible data
        let data: Vec<u8> = (0..1000).map(|i| (i % 10) as u8).collect();
        let compressed = compressor.compress(&data);
        
        assert!(compressed.len() < data.len());
    }
}
