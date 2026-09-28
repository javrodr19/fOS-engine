//! Zstandard Compression
//!
//! Zstandard (RFC 8878) for the `zstd` HTTP content coding. Decoding is
//! done by `ruzstd`, a pure-Rust implementation of the full format (FSE and
//! Huffman entropy coding, dictionaries, checksums, skippable frames), so
//! data from real servers decodes correctly.

use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
use ruzstd::decoding::{BlockDecodingStrategy, Dictionary, FrameDecoder};

/// Zstandard frame magic number
const MAGIC_NUMBER: u32 = 0xFD2FB528;

/// Largest window accepted by default. RFC 9659 caps the window of the
/// `zstd` content coding at 8 MB, which also bounds decoder memory.
pub const DEFAULT_MAX_WINDOW_SIZE: u64 = 8 * 1024 * 1024;

/// Default cap on the decompressed size, against decompression bombs
pub const DEFAULT_MAX_OUTPUT_SIZE: usize = 256 * 1024 * 1024;

/// Output decoded per step before it is drained and checked against the cap
const DECODE_STEP: usize = 1024 * 1024;

/// Zstandard compression level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompressionLevel {
    /// Fastest compression
    Fastest,
    /// Fast compression (level 1-3)
    Fast,
    /// Default compression (level 4-6)
    #[default]
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

/// Zstandard compressor
#[derive(Debug, Default)]
pub struct ZstdCompressor {
    /// Compression level
    level: CompressionLevel,
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

impl ZstdCompressor {
    /// Create a new compressor
    pub fn new(level: CompressionLevel) -> Self {
        Self { level, stats: CompressorStats::default() }
    }

    /// The requested compression level
    pub fn level(&self) -> CompressionLevel {
        self.level
    }

    /// Compress `input` into a single standard Zstandard frame.
    ///
    /// The encoder implements the fast strategy (about zstd level 1) for
    /// every level; higher levels trade too much CPU for a browser, which
    /// compresses little besides the occasional upload.
    pub fn compress(&mut self, input: &[u8]) -> Vec<u8> {
        let output = ruzstd::encoding::compress_to_vec(input, ruzstd::encoding::CompressionLevel::Fastest);
        self.stats.bytes_in += input.len() as u64;
        self.stats.bytes_out += output.len() as u64;
        self.stats.frames += 1;
        output
    }

    /// Compress each `block_size` chunk of `input` into its own frame
    pub fn compress_stream(&mut self, input: &[u8], block_size: usize) -> Vec<Vec<u8>> {
        input.chunks(block_size.max(1)).map(|chunk| self.compress(chunk)).collect()
    }

    /// Get statistics
    pub fn stats(&self) -> &CompressorStats {
        &self.stats
    }
}

/// Zstandard decompressor
#[derive(Debug)]
pub struct ZstdDecompressor {
    /// Dictionary in the Zstandard dictionary format (if any)
    dictionary: Option<Vec<u8>>,
    /// Largest window size accepted
    max_window_size: u64,
    /// Cap on the decompressed size
    max_output_size: usize,
    /// Statistics
    stats: DecompressorStats,
}

impl Default for ZstdDecompressor {
    fn default() -> Self {
        Self {
            dictionary: None,
            max_window_size: DEFAULT_MAX_WINDOW_SIZE,
            max_output_size: DEFAULT_MAX_OUTPUT_SIZE,
            stats: DecompressorStats::default(),
        }
    }
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZstdError {
    /// Invalid magic number
    InvalidMagic,
    /// Invalid frame header
    InvalidHeader,
    /// Corrupted data
    CorruptedData,
    /// Dictionary missing, malformed or not the one the frame needs
    DictionaryMismatch,
    /// Window too large
    WindowTooLarge,
    /// Checksum mismatch
    ChecksumMismatch,
    /// Decompressed output exceeds the size cap
    OutputTooLarge,
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
            Self::OutputTooLarge => write!(f, "Decompressed output too large"),
        }
    }
}

impl std::error::Error for ZstdError {}

impl From<FrameDecoderError> for ZstdError {
    fn from(e: FrameDecoderError) -> Self {
        match e {
            FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::BadMagicNumber(_)) => Self::InvalidMagic,
            FrameDecoderError::ReadFrameHeaderError(_)
            | FrameDecoderError::FrameHeaderError(_)
            | FrameDecoderError::FailedToInitialize(_) => Self::InvalidHeader,
            FrameDecoderError::WindowSizeTooBig { .. } => Self::WindowTooLarge,
            FrameDecoderError::DictionaryDecodeError(_) | FrameDecoderError::DictNotProvided { .. } => {
                Self::DictionaryMismatch
            }
            _ => Self::CorruptedData,
        }
    }
}

impl ZstdDecompressor {
    /// Create a new decompressor
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a dictionary (Zstandard dictionary format, as made by `zstd --train`)
    pub fn with_dictionary(mut self, dict: Vec<u8>) -> Self {
        self.dictionary = Some(dict);
        self
    }

    /// Set the largest window size accepted
    pub fn with_max_window_size(mut self, size: u64) -> Self {
        self.max_window_size = size;
        self
    }

    /// Set the cap on the decompressed size
    pub fn with_max_output_size(mut self, size: usize) -> Self {
        self.max_output_size = size;
        self
    }

    /// Decompress a complete Zstandard stream: one or more frames,
    /// skippable frames included
    pub fn decompress(&mut self, input: &[u8]) -> Result<Vec<u8>, ZstdError> {
        self.stats.bytes_in += input.len() as u64;
        let result = self.decode_frames(input);
        match &result {
            Ok((output, frames)) => {
                self.stats.bytes_out += output.len() as u64;
                self.stats.frames += frames;
            }
            Err(_) => self.stats.errors += 1,
        }
        result.map(|(output, _)| output)
    }

    /// Get statistics
    pub fn stats(&self) -> &DecompressorStats {
        &self.stats
    }

    fn decode_frames(&self, mut input: &[u8]) -> Result<(Vec<u8>, u64), ZstdError> {
        if !is_zstd(input) && !is_skippable_frame(input) {
            return Err(if input.len() < 4 { ZstdError::InvalidHeader } else { ZstdError::InvalidMagic });
        }

        let mut decoder = FrameDecoder::new();
        decoder.set_max_window_size(self.max_window_size);
        if let Some(raw) = &self.dictionary {
            let dict = Dictionary::decode_dict(raw).map_err(|_| ZstdError::DictionaryMismatch)?;
            decoder.add_dict(dict)?;
        }

        let limit = self.max_output_size;
        let mut output = Vec::new();
        let mut frames = 0;
        while !input.is_empty() {
            match decoder.reset(&mut input) {
                Ok(()) => {}
                Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame { length, .. })) => {
                    input = input.get(length as usize..).ok_or(ZstdError::CorruptedData)?;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
            if decoder.content_size() > (limit - output.len()) as u64 {
                return Err(ZstdError::OutputTooLarge);
            }

            loop {
                decoder.decode_blocks(&mut input, BlockDecodingStrategy::UptoBytes(DECODE_STEP))?;
                decoder.collect_to_writer(&mut output).map_err(|_| ZstdError::CorruptedData)?;
                if output.len() > limit {
                    return Err(ZstdError::OutputTooLarge);
                }
                if decoder.is_finished() && decoder.can_collect() == 0 {
                    break;
                }
            }

            if let (Some(expected), Some(actual)) = (decoder.get_checksum_from_data(), decoder.get_calculated_checksum()) {
                if expected != actual {
                    return Err(ZstdError::ChecksumMismatch);
                }
            }
            frames += 1;
        }

        Ok((output, frames))
    }
}

/// Decode a Zstandard stream with the default window and output limits,
/// capping the output at `max_output_size`
pub fn decompress(input: &[u8], max_output_size: usize) -> Result<Vec<u8>, ZstdError> {
    ZstdDecompressor::new().with_max_output_size(max_output_size).decompress(input)
}

/// Detect if data starts with a Zstandard frame
pub fn is_zstd(data: &[u8]) -> bool {
    data.len() >= 4
        && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == MAGIC_NUMBER
}

/// Skippable frames use magic numbers 0x184D2A50..=0x184D2A5F
fn is_skippable_frame(data: &[u8]) -> bool {
    data.len() >= 4
        && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) & 0xFFFF_FFF0 == 0x184D_2A50
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

    /// `printf 'Hello, zstd!' | zstd -19 --check`: a frame from the reference
    /// encoder, with a content checksum
    const REFERENCE_FRAME: &[u8] = &[
        0x28, 0xb5, 0x2f, 0xfd, 0x24, 0x0c, 0x61, 0x00, 0x00, 0x48, 0x65, 0x6c, 0x6c, 0x6f, 0x2c, 0x20,
        0x7a, 0x73, 0x74, 0x64, 0x21, 0x6d, 0xd9, 0x67, 0x0a,
    ];

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
        assert_eq!(decompressor.stats().frames, 1);
    }

    #[test]
    fn test_decodes_reference_encoder_output() {
        assert_eq!(decompress(REFERENCE_FRAME, 1024).unwrap(), b"Hello, zstd!");
    }

    #[test]
    fn test_checksum_mismatch_is_detected() {
        let mut frame = REFERENCE_FRAME.to_vec();
        *frame.last_mut().unwrap() ^= 0xFF;
        assert_eq!(decompress(&frame, 1024), Err(ZstdError::ChecksumMismatch));
    }

    #[test]
    fn test_multiple_and_skippable_frames() {
        let mut stream = REFERENCE_FRAME.to_vec();
        // Skippable frame: magic, 4-byte length, payload
        stream.extend_from_slice(&[0x50, 0x2a, 0x4d, 0x18, 3, 0, 0, 0, 1, 2, 3]);
        stream.extend_from_slice(REFERENCE_FRAME);
        assert_eq!(decompress(&stream, 1024).unwrap(), b"Hello, zstd!Hello, zstd!");
    }

    #[test]
    fn test_output_cap() {
        let data = vec![b'a'; 100_000];
        let compressed = ZstdCompressor::default().compress(&data);
        assert!(compressed.len() < 1000);
        assert_eq!(decompress(&compressed, 100_000).unwrap(), data);
        assert_eq!(decompress(&compressed, 99_999), Err(ZstdError::OutputTooLarge));
    }

    #[test]
    fn test_truncated_frame_is_an_error() {
        let data: Vec<u8> = (0..10_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let compressed = ZstdCompressor::default().compress(&data);
        assert!(decompress(&compressed[..compressed.len() / 2], usize::MAX).is_err());
    }

    #[test]
    fn test_empty_data() {
        let mut compressor = ZstdCompressor::default();
        let compressed = compressor.compress(&[]);
        assert!(is_zstd(&compressed));
        assert_eq!(decompress(&compressed, 0).unwrap(), b"");
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
        assert_eq!(decompressor.stats().errors, 1);
    }

    #[test]
    fn test_compression_ratio() {
        let mut compressor = ZstdCompressor::new(CompressionLevel::Best);

        // Highly compressible data
        let data: Vec<u8> = (0..1000).map(|i| (i % 10) as u8).collect();
        let compressed = compressor.compress(&data);

        assert!(compressed.len() < data.len());
        assert_eq!(ZstdDecompressor::new().decompress(&compressed).unwrap(), data);
    }
}
