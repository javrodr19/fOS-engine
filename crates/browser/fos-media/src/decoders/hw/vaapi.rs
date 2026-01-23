//! VA-API Hardware Acceleration (Linux)
//!
//! Video Acceleration API for hardware video decoding on Linux.

use super::HwCodec;
use crate::decoders::{VideoFrame, EncodedPacket, DecoderCaps, DecoderResult, DecoderError, PixelFormat, Plane};
use std::time::Duration;

/// VA-API decoder
#[derive(Debug)]
pub struct VaApiDecoder {
    codec: HwCodec,
    width: u32,
    height: u32,
    initialized: bool,
    // In real impl: VADisplay, VAConfigID, VAContextID, VASurfaceID[], etc.
}

impl VaApiDecoder {
    /// Check if VA-API is available for the given codec
    pub fn is_available(_codec: HwCodec) -> bool {
        // Real impl would check:
        // 1. Open DRM device (/dev/dri/renderD128)
        // 2. vaGetDisplay, vaInitialize
        // 3. vaQueryConfigProfiles for codec support
        false // Disabled by default - enable when VA-API libs are available
    }
    
    /// Create a new VA-API decoder
    pub fn new(codec: HwCodec) -> DecoderResult<Self> {
        if !Self::is_available(codec) {
            return Err(DecoderError::HardwareError("VA-API not available".into()));
        }
        
        Ok(Self { codec, width: 0, height: 0, initialized: false })
    }
    
    /// Initialize with dimensions
    fn init(&mut self, width: u32, height: u32) -> DecoderResult<()> {
        self.width = width;
        self.height = height;
        
        // Real impl would:
        // 1. vaCreateConfig with profile/entrypoint
        // 2. vaCreateContext
        // 3. vaCreateSurfaces
        
        self.initialized = true;
        Ok(())
    }
    
    /// Decode a packet
    pub fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<Vec<VideoFrame>> {
        if !self.initialized {
            // Parse header to get dimensions, then init
            // For now, use default
            self.init(1920, 1080)?;
        }
        
        // Real impl would:
        // 1. vaBeginPicture
        // 2. vaRenderPicture with buffers (slice params, slice data, IQ matrix, etc.)
        // 3. vaEndPicture
        // 4. vaSyncSurface
        // 5. vaGetImage or use surface directly
        
        let y_size = (self.width * self.height) as usize;
        let uv_size = y_size / 4;
        
        let frame = VideoFrame {
            pts: packet.pts,
            dts: packet.dts,
            duration: Duration::from_millis(33),
            width: self.width,
            height: self.height,
            format: PixelFormat::Nv12, // VA-API typically outputs NV12
            key_frame: packet.is_key,
            planes: vec![
                Plane { data: vec![128; y_size], stride: self.width as usize },
                Plane { data: vec![128; uv_size * 2], stride: self.width as usize }, // NV12 interleaved UV
            ],
        };
        
        Ok(vec![frame])
    }
    
    /// Get decoder capabilities
    pub fn capabilities(&self) -> DecoderCaps {
        DecoderCaps {
            max_width: 8192,
            max_height: 4320,
            formats: vec![PixelFormat::Nv12],
            hardware: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vaapi_not_available() {
        // VA-API is typically not available in test environments
        assert!(!VaApiDecoder::is_available(HwCodec::H264));
    }
}
