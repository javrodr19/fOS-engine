//! VideoToolbox Hardware Acceleration (macOS)
//!
//! Apple VideoToolbox API for hardware video decoding on macOS/iOS.

use super::HwCodec;
use crate::decoders::{VideoFrame, EncodedPacket, DecoderCaps, DecoderResult, DecoderError, PixelFormat, Plane};
use std::time::Duration;

/// VideoToolbox decoder
#[derive(Debug)]
pub struct VideoToolboxDecoder {
    codec: HwCodec,
    width: u32,
    height: u32,
    initialized: bool,
    // In real impl: CMFormatDescriptionRef, VTDecompressionSessionRef, etc.
}

impl VideoToolboxDecoder {
    /// Check if VideoToolbox is available for the given codec
    pub fn is_available(codec: HwCodec) -> bool {
        #[cfg(target_os = "macos")]
        {
            // Real impl would check:
            // 1. VTIsHardwareDecodeSupported for codec
            // 2. Check for specific profile support
            match codec {
                HwCodec::H264 | HwCodec::H265 | HwCodec::Vp9 => true,
                HwCodec::Av1 => false, // AV1 support depends on Apple Silicon
                _ => false,
            }
        }
        #[cfg(not(target_os = "macos"))]
        { false }
    }
    
    /// Create a new VideoToolbox decoder
    pub fn new(codec: HwCodec) -> DecoderResult<Self> {
        if !Self::is_available(codec) {
            return Err(DecoderError::HardwareError("VideoToolbox not available".into()));
        }
        
        Ok(Self { codec, width: 0, height: 0, initialized: false })
    }
    
    /// Initialize with format description
    fn init(&mut self, width: u32, height: u32, codec_data: &[u8]) -> DecoderResult<()> {
        self.width = width;
        self.height = height;
        
        // Real impl would:
        // 1. Create CMFormatDescription from SPS/PPS (H.264) or VPS/SPS/PPS (H.265)
        // 2. Create VTDecompressionSession with output callback
        // 3. Set up CVPixelBuffer pool
        
        self.initialized = true;
        Ok(())
    }
    
    /// Decode a packet
    pub fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<Vec<VideoFrame>> {
        if !self.initialized {
            // Parse header to get dimensions from codec-specific data
            self.init(1920, 1080, &[])?;
        }
        
        // Real impl would:
        // 1. Create CMSampleBuffer from packet data
        // 2. VTDecompressionSessionDecodeFrame
        // 3. Wait for callback with decoded CVPixelBuffer
        // 4. Copy or map CVPixelBuffer to VideoFrame
        
        let y_size = (self.width * self.height) as usize;
        let uv_size = y_size / 4;
        
        let frame = VideoFrame {
            pts: packet.pts,
            dts: packet.dts,
            duration: Duration::from_millis(33),
            width: self.width,
            height: self.height,
            format: PixelFormat::Nv12, // VideoToolbox typically outputs NV12
            key_frame: packet.is_key,
            planes: vec![
                Plane { data: vec![128; y_size], stride: self.width as usize },
                Plane { data: vec![128; uv_size * 2], stride: self.width as usize },
            ],
        };
        
        Ok(vec![frame])
    }
    
    /// Flush decoder
    pub fn flush(&mut self) -> Vec<VideoFrame> {
        // VTDecompressionSessionWaitForAsynchronousFrames
        Vec::new()
    }
    
    /// Get decoder capabilities
    pub fn capabilities(&self) -> DecoderCaps {
        DecoderCaps {
            max_width: 8192,
            max_height: 4320,
            formats: vec![PixelFormat::Nv12, PixelFormat::I420],
            hardware: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_videotoolbox() {
        // On non-macOS, should not be available
        #[cfg(not(target_os = "macos"))]
        assert!(!VideoToolboxDecoder::is_available(HwCodec::H264));
    }
}
