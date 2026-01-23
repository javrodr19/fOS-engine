//! Video Decoders
//!
//! Pure-Rust implementations of H.264, H.265, VP8, VP9, and AV1 decoders.

pub mod h264;
pub mod h265;
pub mod vp8;
pub mod vp9;
pub mod av1;

use super::{
    VideoFrame, EncodedPacket, DecoderCaps, DecoderResult, DecoderError,
    VideoDecoderTrait, PixelFormat, Plane,
};
use std::time::Duration;

/// Decoded Picture Buffer for reference frame management
#[derive(Debug)]
pub struct DecodedPictureBuffer {
    /// Maximum number of reference frames
    max_refs: usize,
    /// Stored reference frames
    frames: Vec<ReferenceFrame>,
}

/// Reference frame for inter prediction
#[derive(Debug, Clone)]
pub struct ReferenceFrame {
    /// Frame data
    pub frame: VideoFrame,
    /// Picture order count
    pub poc: i32,
    /// Frame number
    pub frame_num: u32,
    /// Long-term reference flag
    pub long_term: bool,
}

impl DecodedPictureBuffer {
    pub fn new(max_refs: usize) -> Self {
        Self {
            max_refs,
            frames: Vec::with_capacity(max_refs),
        }
    }
    
    /// Add a reference frame
    pub fn add(&mut self, frame: ReferenceFrame) {
        if self.frames.len() >= self.max_refs {
            // Remove oldest short-term reference
            if let Some(pos) = self.frames.iter().position(|f| !f.long_term) {
                self.frames.remove(pos);
            }
        }
        self.frames.push(frame);
    }
    
    /// Get reference by POC
    pub fn get_by_poc(&self, poc: i32) -> Option<&ReferenceFrame> {
        self.frames.iter().find(|f| f.poc == poc)
    }
    
    /// Get reference by frame number
    pub fn get_by_frame_num(&self, frame_num: u32) -> Option<&ReferenceFrame> {
        self.frames.iter().find(|f| f.frame_num == frame_num)
    }
    
    /// Clear all references
    pub fn clear(&mut self) {
        self.frames.clear();
    }
    
    /// Number of stored references
    pub fn len(&self) -> usize {
        self.frames.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

/// Bitstream reader for parsing NAL units and other bitstreams
#[derive(Debug)]
pub struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }
    
    /// Read n bits (up to 32)
    pub fn read_bits(&mut self, n: u8) -> DecoderResult<u32> {
        if n > 32 {
            return Err(DecoderError::Internal("Cannot read more than 32 bits".into()));
        }
        
        let mut result = 0u32;
        let mut bits_remaining = n;
        
        while bits_remaining > 0 {
            if self.byte_pos >= self.data.len() {
                return Err(DecoderError::NeedMoreData);
            }
            
            let bits_in_byte = 8 - self.bit_pos;
            let bits_to_read = bits_remaining.min(bits_in_byte);
            
            let mask = (1u8 << bits_to_read) - 1;
            let shift = bits_in_byte - bits_to_read;
            let bits = (self.data[self.byte_pos] >> shift) & mask;
            
            result = (result << bits_to_read) | bits as u32;
            
            self.bit_pos += bits_to_read;
            bits_remaining -= bits_to_read;
            
            if self.bit_pos >= 8 {
                self.bit_pos = 0;
                self.byte_pos += 1;
            }
        }
        
        Ok(result)
    }
    
    /// Read single bit
    pub fn read_bit(&mut self) -> DecoderResult<bool> {
        Ok(self.read_bits(1)? != 0)
    }
    
    /// Read unsigned Exp-Golomb coded value
    pub fn read_ue(&mut self) -> DecoderResult<u32> {
        let mut leading_zeros = 0u8;
        while !self.read_bit()? {
            leading_zeros += 1;
            if leading_zeros > 31 {
                return Err(DecoderError::InvalidBitstream("Exp-Golomb overflow".into()));
            }
        }
        
        if leading_zeros == 0 {
            return Ok(0);
        }
        
        let suffix = self.read_bits(leading_zeros)?;
        Ok((1 << leading_zeros) - 1 + suffix)
    }
    
    /// Read signed Exp-Golomb coded value
    pub fn read_se(&mut self) -> DecoderResult<i32> {
        let ue = self.read_ue()?;
        let sign = if ue & 1 == 1 { 1 } else { -1 };
        Ok(sign * ((ue + 1) / 2) as i32)
    }
    
    /// Skip n bits
    pub fn skip(&mut self, n: usize) -> DecoderResult<()> {
        for _ in 0..n {
            self.read_bit()?;
        }
        Ok(())
    }
    
    /// Check if aligned to byte boundary
    pub fn is_byte_aligned(&self) -> bool {
        self.bit_pos == 0
    }
    
    /// Align to next byte boundary
    pub fn byte_align(&mut self) {
        if self.bit_pos != 0 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
    }
    
    /// Remaining bytes
    pub fn remaining_bytes(&self) -> usize {
        if self.byte_pos >= self.data.len() {
            0
        } else {
            self.data.len() - self.byte_pos - if self.bit_pos > 0 { 1 } else { 0 }
        }
    }
    
    /// Check if more data available
    pub fn has_more_data(&self) -> bool {
        self.byte_pos < self.data.len()
    }
}

/// Common video decoder state
#[derive(Debug, Default)]
pub struct VideoDecoderState {
    /// Current width
    pub width: u32,
    /// Current height
    pub height: u32,
    /// Current frame number  
    pub frame_num: u64,
    /// Output pixel format
    pub format: PixelFormat,
}

/// YUV color space conversion utilities
pub mod yuv {
    /// Convert I420 to RGBA
    pub fn i420_to_rgba(
        y: &[u8], y_stride: usize,
        u: &[u8], u_stride: usize,
        v: &[u8], v_stride: usize,
        width: usize, height: usize,
        rgba: &mut [u8],
    ) {
        for row in 0..height {
            for col in 0..width {
                let y_idx = row * y_stride + col;
                let uv_row = row / 2;
                let uv_col = col / 2;
                let u_idx = uv_row * u_stride + uv_col;
                let v_idx = uv_row * v_stride + uv_col;
                
                let y_val = y.get(y_idx).copied().unwrap_or(0) as i32;
                let u_val = u.get(u_idx).copied().unwrap_or(128) as i32 - 128;
                let v_val = v.get(v_idx).copied().unwrap_or(128) as i32 - 128;
                
                // BT.601 conversion
                let r = (y_val + ((351 * v_val) >> 8)).clamp(0, 255) as u8;
                let g = (y_val - ((179 * v_val + 86 * u_val) >> 8)).clamp(0, 255) as u8;
                let b = (y_val + ((443 * u_val) >> 8)).clamp(0, 255) as u8;
                
                let rgba_idx = (row * width + col) * 4;
                if rgba_idx + 3 < rgba.len() {
                    rgba[rgba_idx] = r;
                    rgba[rgba_idx + 1] = g;
                    rgba[rgba_idx + 2] = b;
                    rgba[rgba_idx + 3] = 255;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bit_reader() {
        let data = [0b10110100, 0b11001010];
        let mut reader = BitReader::new(&data);
        
        assert_eq!(reader.read_bits(4).unwrap(), 0b1011);
        assert_eq!(reader.read_bits(4).unwrap(), 0b0100);
        assert_eq!(reader.read_bits(8).unwrap(), 0b11001010);
    }
    
    #[test]
    fn test_exp_golomb() {
        // ue(0) = 1, ue(1) = 010, ue(2) = 011, etc.
        let data = [0b10100110]; // ue: 0, 1, 2
        let mut reader = BitReader::new(&data);
        
        assert_eq!(reader.read_ue().unwrap(), 0);
        assert_eq!(reader.read_ue().unwrap(), 1);
        assert_eq!(reader.read_ue().unwrap(), 2);
    }
    
    #[test]
    fn test_dpb() {
        let mut dpb = DecodedPictureBuffer::new(4);
        assert!(dpb.is_empty());
        
        let frame = VideoFrame {
            pts: Duration::ZERO,
            dts: Duration::ZERO,
            duration: Duration::from_millis(33),
            width: 1920,
            height: 1080,
            format: PixelFormat::I420,
            planes: vec![],
            key_frame: true,
        };
        
        dpb.add(ReferenceFrame {
            frame,
            poc: 0,
            frame_num: 0,
            long_term: false,
        });
        
        assert_eq!(dpb.len(), 1);
        assert!(dpb.get_by_poc(0).is_some());
    }
}
