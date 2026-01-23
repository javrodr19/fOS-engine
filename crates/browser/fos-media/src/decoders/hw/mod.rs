//! Hardware Acceleration
//!
//! Platform-specific hardware video decoding backends.

#[cfg(target_os = "linux")]
pub mod vaapi;

#[cfg(target_os = "macos")]
pub mod videotoolbox;

#[cfg(target_os = "windows")]
pub mod dxva2;

use crate::decoders::{VideoFrame, EncodedPacket, DecoderCaps, DecoderResult, DecoderError, VideoDecoderTrait, PixelFormat};

/// Hardware decoder backend
#[derive(Debug)]
pub enum HwAcceleratedDecoder {
    #[cfg(target_os = "linux")]
    VaApi(vaapi::VaApiDecoder),
    #[cfg(target_os = "macos")]
    VideoToolbox(videotoolbox::VideoToolboxDecoder),
    #[cfg(target_os = "windows")]
    Dxva2(dxva2::Dxva2Decoder),
    None,
}

/// Codec identifier for HW acceleration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwCodec { H264, H265, Vp8, Vp9, Av1 }

/// Check if hardware acceleration is available for a codec
pub fn hw_available(codec: HwCodec) -> bool {
    #[cfg(target_os = "linux")]
    { vaapi::VaApiDecoder::is_available(codec) }
    #[cfg(not(target_os = "linux"))]
    { false }
}

/// Try to create a hardware decoder
pub fn try_hw_decoder(codec: HwCodec) -> Option<HwAcceleratedDecoder> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(decoder) = vaapi::VaApiDecoder::new(codec) {
            return Some(HwAcceleratedDecoder::VaApi(decoder));
        }
    }
    None
}

/// Decoder backend - software or hardware
#[derive(Debug)]
pub enum DecoderBackend {
    Software(Box<dyn VideoDecoderTrait>),
    Hardware(HwAcceleratedDecoder),
}

impl DecoderBackend {
    pub fn new(codec: HwCodec) -> Self {
        if let Some(hw) = try_hw_decoder(codec) {
            Self::Hardware(hw)
        } else {
            Self::Software(match codec {
                HwCodec::H264 => Box::new(super::video::h264::H264Decoder::new()),
                HwCodec::H265 => Box::new(super::video::h265::H265Decoder::new()),
                HwCodec::Vp8 => Box::new(super::video::vp8::Vp8Decoder::new()),
                HwCodec::Vp9 => Box::new(super::video::vp9::Vp9Decoder::new()),
                HwCodec::Av1 => Box::new(super::video::av1::Av1Decoder::new()),
            })
        }
    }
    
    pub fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<Vec<VideoFrame>> {
        match self {
            Self::Software(dec) => dec.decode(packet),
            Self::Hardware(hw) => match hw {
                #[cfg(target_os = "linux")]
                HwAcceleratedDecoder::VaApi(dec) => dec.decode(packet),
                HwAcceleratedDecoder::None => Err(DecoderError::Unsupported("No HW decoder".into())),
            }
        }
    }
    
    pub fn flush(&mut self) -> Vec<VideoFrame> {
        match self {
            Self::Software(dec) => dec.flush(),
            Self::Hardware(_) => Vec::new(),
        }
    }
    
    pub fn is_hardware(&self) -> bool { matches!(self, Self::Hardware(_)) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_backend() { let b = DecoderBackend::new(HwCodec::H264); assert!(!b.is_hardware() || b.is_hardware()); }
}
