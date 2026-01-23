//! DXVA2 / D3D11VA Hardware Acceleration (Windows)
//!
//! DirectX Video Acceleration for hardware video decoding on Windows.

use super::HwCodec;
use crate::decoders::{VideoFrame, EncodedPacket, DecoderCaps, DecoderResult, DecoderError, PixelFormat, Plane};
use std::time::Duration;

/// DXVA2 decoder  
#[derive(Debug)]
pub struct Dxva2Decoder {
    codec: HwCodec,
    width: u32,
    height: u32,
    initialized: bool,
    use_d3d11: bool,
    // In real impl: ID3D11Device, ID3D11VideoDecoder, ID3D11VideoDecoderOutputView[], etc.
}

impl Dxva2Decoder {
    /// Check if DXVA2/D3D11VA is available for the given codec
    pub fn is_available(codec: HwCodec) -> bool {
        #[cfg(target_os = "windows")]
        {
            // Real impl would:
            // 1. Create D3D11 device
            // 2. Query ID3D11VideoDevice for decoder profile support
            // 3. Check for specific codec GUIDs
            match codec {
                HwCodec::H264 | HwCodec::H265 | HwCodec::Vp9 | HwCodec::Av1 => true,
                _ => false,
            }
        }
        #[cfg(not(target_os = "windows"))]
        { false }
    }
    
    /// Create a new DXVA2 decoder (prefer D3D11VA when available)
    pub fn new(codec: HwCodec) -> DecoderResult<Self> {
        if !Self::is_available(codec) {
            return Err(DecoderError::HardwareError("DXVA2 not available".into()));
        }
        
        Ok(Self { codec, width: 0, height: 0, initialized: false, use_d3d11: true })
    }
    
    /// Initialize with video dimensions
    fn init(&mut self, width: u32, height: u32) -> DecoderResult<()> {
        self.width = width;
        self.height = height;
        
        // Real impl would:
        // D3D11VA path:
        // 1. Create ID3D11VideoDevice
        // 2. Create ID3D11VideoDecoder with profile GUID
        // 3. Create output textures array
        // 4. Create ID3D11VideoDecoderOutputView for each texture
        //
        // DXVA2 path (legacy):
        // 1. Create IDirectXVideoDecoderService
        // 2. CreateVideoDecoder with DXVA2 profile GUID
        // 3. Create D3D9 surfaces
        
        self.initialized = true;
        Ok(())
    }
    
    /// Decode a packet
    pub fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<Vec<VideoFrame>> {
        if !self.initialized {
            self.init(1920, 1080)?;
        }
        
        // Real impl would:
        // D3D11VA:
        // 1. ID3D11VideoContext::DecoderBeginFrame
        // 2. GetDecoderBuffer for each buffer type (picture params, bitstream, etc.)
        // 3. ReleaseDecoderBuffer
        // 4. SubmitDecoderBuffers
        // 5. DecoderEndFrame
        // 6. Copy output texture to staging texture
        // 7. Map staging texture to read pixels
        
        let y_size = (self.width * self.height) as usize;
        let uv_size = y_size / 4;
        
        let frame = VideoFrame {
            pts: packet.pts,
            dts: packet.dts,
            duration: Duration::from_millis(33),
            width: self.width,
            height: self.height,
            format: PixelFormat::Nv12, // DXVA typically uses NV12
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
        Vec::new()
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

/// DXVA2 profile GUIDs (for reference)
pub mod profile_guids {
    // H.264
    pub const DXVA2_MODEH264_VLD_NOFC: &str = "1b81be68-a0c7-11d3-b984-00c04f2e73c5";
    pub const DXVA2_MODEH264_VLD_FGT: &str = "1b81be69-a0c7-11d3-b984-00c04f2e73c5";
    
    // HEVC
    pub const DXVA_MODEHEVC_VLD_MAIN: &str = "5b11d51b-2f4c-4452-bcc3-09f2a1160cc0";
    pub const DXVA_MODEHEVC_VLD_MAIN10: &str = "107af0e0-ef1a-4d19-aba8-67a163073d13";
    
    // VP9
    pub const DXVA_MODEVP9_VLD_PROFILE0: &str = "463707f8-a1d0-4585-876d-83aa6d60b89e";
    pub const DXVA_MODEVP9_VLD_10BIT_PROFILE2: &str = "a4c749ef-6ecf-48aa-8448-50a7a1165ff7";
    
    // AV1
    pub const DXVA_MODEAV1_VLD_PROFILE0: &str = "b8be4ccb-cf53-46ba-8d59-d6b8a6da5d2a";
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dxva2() {
        #[cfg(not(target_os = "windows"))]
        assert!(!Dxva2Decoder::is_available(HwCodec::H264));
    }
}
