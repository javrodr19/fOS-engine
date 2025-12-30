//! Media Codecs
//!
//! Codec support detection and media decoding/encoding.

use std::collections::HashMap;

/// Supported codec
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CodecType {
    // Video codecs
    H264,
    H265,
    VP8,
    VP9,
    AV1,
    
    // Audio codecs
    AAC,
    MP3,
    Opus,
    Vorbis,
    FLAC,
    PCM,
}

impl CodecType {
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::H264 => "video/avc",
            Self::H265 => "video/hevc",
            Self::VP8 => "video/vp8",
            Self::VP9 => "video/vp9",
            Self::AV1 => "video/av1",
            Self::AAC => "audio/aac",
            Self::MP3 => "audio/mpeg",
            Self::Opus => "audio/opus",
            Self::Vorbis => "audio/vorbis",
            Self::FLAC => "audio/flac",
            Self::PCM => "audio/pcm",
        }
    }
    
    pub fn is_video(&self) -> bool {
        matches!(self, Self::H264 | Self::H265 | Self::VP8 | Self::VP9 | Self::AV1)
    }
    
    pub fn is_audio(&self) -> bool {
        !self.is_video()
    }
}

/// Codec configuration
#[derive(Debug, Clone)]
pub struct CodecConfig {
    pub codec: CodecType,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
    pub bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    pub profile: Option<String>,
    pub level: Option<String>,
}

impl CodecConfig {
    pub fn video(codec: CodecType, width: u32, height: u32) -> Self {
        Self {
            codec,
            width: Some(width),
            height: Some(height),
            frame_rate: Some(30.0),
            bitrate: None,
            sample_rate: None,
            channels: None,
            profile: None,
            level: None,
        }
    }
    
    pub fn audio(codec: CodecType, sample_rate: u32, channels: u32) -> Self {
        Self {
            codec,
            width: None,
            height: None,
            frame_rate: None,
            bitrate: None,
            sample_rate: Some(sample_rate),
            channels: Some(channels),
            profile: None,
            level: None,
        }
    }
}

/// Codec support check result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecSupport {
    Supported,
    Unsupported,
    MaybeSupported,
}

/// Codec registry
#[derive(Debug, Default)]
pub struct CodecRegistry {
    /// Registered decoders
    decoders: HashMap<CodecType, CodecInfo>,
    /// Registered encoders
    encoders: HashMap<CodecType, CodecInfo>,
}

/// Codec info
#[derive(Debug, Clone)]
pub struct CodecInfo {
    pub codec: CodecType,
    pub hardware_accelerated: bool,
    pub max_width: u32,
    pub max_height: u32,
    pub max_frame_rate: f64,
}

impl CodecRegistry {
    pub fn new() -> Self {
        let mut registry = Self::default();
        registry.register_defaults();
        registry
    }
    
    fn register_defaults(&mut self) {
        // Register common decoders
        let video_codecs = [CodecType::H264, CodecType::VP8, CodecType::VP9];
        for codec in video_codecs {
            self.decoders.insert(codec, CodecInfo {
                codec,
                hardware_accelerated: false,
                max_width: 4096,
                max_height: 2160,
                max_frame_rate: 60.0,
            });
        }
        
        let audio_codecs = [CodecType::AAC, CodecType::MP3, CodecType::Opus, CodecType::Vorbis];
        for codec in audio_codecs {
            self.decoders.insert(codec, CodecInfo {
                codec,
                hardware_accelerated: false,
                max_width: 0,
                max_height: 0,
                max_frame_rate: 0.0,
            });
        }
    }
    
    /// Check if codec is supported for decoding
    pub fn is_decode_supported(&self, codec: CodecType) -> CodecSupport {
        if self.decoders.contains_key(&codec) {
            CodecSupport::Supported
        } else {
            CodecSupport::Unsupported
        }
    }
    
    /// Check if codec is supported for encoding
    pub fn is_encode_supported(&self, codec: CodecType) -> CodecSupport {
        if self.encoders.contains_key(&codec) {
            CodecSupport::Supported
        } else {
            CodecSupport::Unsupported
        }
    }
    
    /// Check if configuration is supported
    pub fn is_config_supported(&self, config: &CodecConfig) -> CodecSupport {
        if let Some(info) = self.decoders.get(&config.codec) {
            if let (Some(w), Some(h)) = (config.width, config.height) {
                if w > info.max_width || h > info.max_height {
                    return CodecSupport::Unsupported;
                }
            }
            CodecSupport::Supported
        } else {
            CodecSupport::Unsupported
        }
    }
    
    /// Register a decoder
    pub fn register_decoder(&mut self, info: CodecInfo) {
        self.decoders.insert(info.codec, info);
    }
    
    /// Register an encoder
    pub fn register_encoder(&mut self, info: CodecInfo) {
        self.encoders.insert(info.codec, info);
    }
}

/// Video decoder
#[derive(Debug)]
pub struct VideoDecoder {
    pub config: CodecConfig,
    pub state: DecoderState,
    /// Decoded frames queue
    frames: Vec<DecodedFrame>,
}

/// Decoder state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DecoderState {
    #[default]
    Unconfigured,
    Configured,
    Closed,
}

/// Decoded video frame
#[derive(Debug)]
pub struct DecodedFrame {
    pub timestamp: f64,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    pub format: PixelFormat,
}

/// Pixel format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    I420,
    NV12,
    RGBA,
    BGRA,
}

impl VideoDecoder {
    pub fn new(config: CodecConfig) -> Self {
        Self {
            config,
            state: DecoderState::Unconfigured,
            frames: Vec::new(),
        }
    }
    
    pub fn configure(&mut self) -> Result<(), CodecError> {
        self.state = DecoderState::Configured;
        Ok(())
    }
    
    pub fn decode(&mut self, _chunk: &EncodedChunk) -> Result<(), CodecError> {
        if self.state != DecoderState::Configured {
            return Err(CodecError::InvalidState);
        }
        
        // Placeholder - actual decoding would use FFmpeg/GStreamer
        let frame = DecodedFrame {
            timestamp: 0.0,
            duration: 1.0 / 30.0,
            width: self.config.width.unwrap_or(1920),
            height: self.config.height.unwrap_or(1080),
            data: Vec::new(),
            format: PixelFormat::I420,
        };
        self.frames.push(frame);
        
        Ok(())
    }
    
    pub fn flush(&mut self) -> Vec<DecodedFrame> {
        std::mem::take(&mut self.frames)
    }
    
    pub fn close(&mut self) {
        self.state = DecoderState::Closed;
        self.frames.clear();
    }
}

/// Encoded chunk
#[derive(Debug)]
pub struct EncodedChunk {
    pub data: Vec<u8>,
    pub timestamp: f64,
    pub duration: Option<f64>,
    pub is_key: bool,
}

/// Audio decoder
#[derive(Debug)]
pub struct AudioDecoder {
    pub config: CodecConfig,
    pub state: DecoderState,
    samples: Vec<DecodedAudio>,
}

/// Decoded audio
#[derive(Debug)]
pub struct DecodedAudio {
    pub timestamp: f64,
    pub duration: f64,
    pub sample_rate: u32,
    pub channels: u32,
    pub data: Vec<f32>,
}

impl AudioDecoder {
    pub fn new(config: CodecConfig) -> Self {
        Self {
            config,
            state: DecoderState::Unconfigured,
            samples: Vec::new(),
        }
    }
    
    pub fn configure(&mut self) -> Result<(), CodecError> {
        self.state = DecoderState::Configured;
        Ok(())
    }
    
    pub fn decode(&mut self, _chunk: &EncodedChunk) -> Result<(), CodecError> {
        if self.state != DecoderState::Configured {
            return Err(CodecError::InvalidState);
        }
        Ok(())
    }
    
    pub fn flush(&mut self) -> Vec<DecodedAudio> {
        std::mem::take(&mut self.samples)
    }
    
    pub fn close(&mut self) {
        self.state = DecoderState::Closed;
    }
}

/// Codec error
#[derive(Debug, Clone, thiserror::Error)]
pub enum CodecError {
    #[error("Invalid state")]
    InvalidState,
    
    #[error("Unsupported codec")]
    Unsupported,
    
    #[error("Decode error: {0}")]
    DecodeError(String),
    
    #[error("Encode error: {0}")]
    EncodeError(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_codec_registry() {
        let registry = CodecRegistry::new();
        
        assert_eq!(registry.is_decode_supported(CodecType::H264), CodecSupport::Supported);
        assert_eq!(registry.is_decode_supported(CodecType::AV1), CodecSupport::Unsupported);
    }
    
    #[test]
    fn test_video_decoder() {
        let config = CodecConfig::video(CodecType::H264, 1920, 1080);
        let mut decoder = VideoDecoder::new(config);
        
        decoder.configure().unwrap();
        assert_eq!(decoder.state, DecoderState::Configured);
    }
    
    #[test]
    fn test_codec_types() {
        assert!(CodecType::H264.is_video());
        assert!(CodecType::AAC.is_audio());
    }
}
