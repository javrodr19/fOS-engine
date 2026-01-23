//! Media Decoders
//!
//! Pure-Rust video and audio decoders with hardware acceleration support.

pub mod video;
pub mod audio;
pub mod hw;

use std::time::Duration;

/// Decoded video frame
#[derive(Debug, Clone)]
pub struct VideoFrame {
    /// Presentation timestamp
    pub pts: Duration,
    /// Decode timestamp
    pub dts: Duration,
    /// Duration of this frame
    pub duration: Duration,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Pixel format
    pub format: PixelFormat,
    /// Plane data (Y, U, V or RGBA)
    pub planes: Vec<Plane>,
    /// Is this a key frame
    pub key_frame: bool,
}

/// Video plane data
#[derive(Debug, Clone)]
pub struct Plane {
    /// Raw pixel data
    pub data: Vec<u8>,
    /// Stride (bytes per row)
    pub stride: usize,
}

/// Pixel format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// YUV 4:2:0 planar (most common for video)
    I420,
    /// YUV 4:2:0 semi-planar (NV12)
    Nv12,
    /// YUV 4:2:2 planar
    I422,
    /// YUV 4:4:4 planar
    I444,
    /// 32-bit RGBA
    Rgba,
    /// 32-bit BGRA
    Bgra,
    /// 10-bit YUV 4:2:0
    I420_10,
}

impl Default for PixelFormat {
    fn default() -> Self {
        Self::I420
    }
}

/// Encoded packet input to decoder
#[derive(Debug, Clone)]
pub struct EncodedPacket {
    /// Compressed data
    pub data: Vec<u8>,
    /// Presentation timestamp
    pub pts: Duration,
    /// Decode timestamp
    pub dts: Duration,
    /// Is this a keyframe/IDR
    pub is_key: bool,
}

/// Decoder capabilities
#[derive(Debug, Clone)]
pub struct DecoderCaps {
    /// Maximum supported width
    pub max_width: u32,
    /// Maximum supported height
    pub max_height: u32,
    /// Supported pixel formats
    pub formats: Vec<PixelFormat>,
    /// Hardware accelerated
    pub hardware: bool,
}

/// Result type for decoder operations
pub type DecoderResult<T> = Result<T, DecoderError>;

/// Decoder error
#[derive(Debug, Clone, thiserror::Error)]
pub enum DecoderError {
    #[error("Invalid bitstream: {0}")]
    InvalidBitstream(String),
    
    #[error("Unsupported feature: {0}")]
    Unsupported(String),
    
    #[error("Need more data")]
    NeedMoreData,
    
    #[error("End of stream")]
    EndOfStream,
    
    #[error("Hardware acceleration error: {0}")]
    HardwareError(String),
    
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Video decoder trait
pub trait VideoDecoderTrait: Send {
    /// Decode a single packet
    fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<Vec<VideoFrame>>;
    
    /// Flush decoder and get remaining frames
    fn flush(&mut self) -> Vec<VideoFrame>;
    
    /// Reset decoder state
    fn reset(&mut self);
    
    /// Get decoder capabilities
    fn capabilities(&self) -> DecoderCaps;
}

/// Decoded audio samples
#[derive(Debug, Clone)]
pub struct AudioSamples {
    /// Presentation timestamp
    pub pts: Duration,
    /// Duration of these samples  
    pub duration: Duration,
    /// Sample rate in Hz
    pub sample_rate: u32,
    /// Number of channels
    pub channels: u32,
    /// Interleaved f32 samples
    pub data: Vec<f32>,
}

/// Audio decoder trait
pub trait AudioDecoderTrait: Send {
    /// Decode a single packet
    fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<AudioSamples>;
    
    /// Flush decoder and get remaining samples
    fn flush(&mut self) -> Option<AudioSamples>;
    
    /// Reset decoder state
    fn reset(&mut self);
    
    /// Get sample rate
    fn sample_rate(&self) -> u32;
    
    /// Get channel count
    fn channels(&self) -> u32;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pixel_format_default() {
        assert_eq!(PixelFormat::default(), PixelFormat::I420);
    }
    
    #[test]
    fn test_video_frame() {
        let frame = VideoFrame {
            pts: Duration::from_millis(0),
            dts: Duration::from_millis(0),
            duration: Duration::from_millis(33),
            width: 1920,
            height: 1080,
            format: PixelFormat::I420,
            planes: vec![],
            key_frame: true,
        };
        assert_eq!(frame.width, 1920);
    }
}
