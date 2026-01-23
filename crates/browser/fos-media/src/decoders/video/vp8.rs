//! VP8 Decoder
//!
//! Pure-Rust implementation of VP8 video decoding.

use super::{BitReader, DecodedPictureBuffer, ReferenceFrame, VideoDecoderState};
use crate::decoders::{
    VideoFrame, EncodedPacket, DecoderCaps, DecoderResult, DecoderError,
    VideoDecoderTrait, PixelFormat, Plane,
};
use std::time::Duration;

/// VP8 Frame Header
#[derive(Debug, Clone)]
pub struct Vp8FrameHeader {
    pub key_frame: bool,
    pub version: u8,
    pub show_frame: bool,
    pub first_part_size: u32,
    pub width: u32,
    pub height: u32,
    pub horizontal_scale: u8,
    pub vertical_scale: u8,
}

/// VP8 Decoder
#[derive(Debug)]
pub struct Vp8Decoder {
    dpb: DecodedPictureBuffer,
    state: VideoDecoderState,
    last_frame: Option<VideoFrame>,
    golden_frame: Option<VideoFrame>,
    alt_ref_frame: Option<VideoFrame>,
    pending_output: Vec<VideoFrame>,
}

impl Vp8Decoder {
    pub fn new() -> Self {
        Self {
            dpb: DecodedPictureBuffer::new(4),
            state: VideoDecoderState::default(),
            last_frame: None, golden_frame: None, alt_ref_frame: None,
            pending_output: Vec::new(),
        }
    }
    
    fn parse_frame_header(&self, data: &[u8]) -> DecoderResult<Vp8FrameHeader> {
        if data.len() < 10 { return Err(DecoderError::NeedMoreData); }
        let byte0 = data[0];
        let key_frame = (byte0 & 1) == 0;
        let version = (byte0 >> 1) & 7;
        let show_frame = (byte0 >> 4) & 1 == 1;
        let first_part_size = ((data[0] as u32) >> 5) | ((data[1] as u32) << 3) | ((data[2] as u32) << 11);
        
        if !key_frame {
            return Ok(Vp8FrameHeader {
                key_frame, version, show_frame, first_part_size,
                width: self.state.width, height: self.state.height,
                horizontal_scale: 0, vertical_scale: 0,
            });
        }
        
        // Key frame has start code and dimensions
        if data.len() < 10 || data[3] != 0x9D || data[4] != 0x01 || data[5] != 0x2A {
            return Err(DecoderError::InvalidBitstream("Invalid VP8 start code".into()));
        }
        
        let width = (data[6] as u32) | ((data[7] as u32 & 0x3F) << 8);
        let horizontal_scale = data[7] >> 6;
        let height = (data[8] as u32) | ((data[9] as u32 & 0x3F) << 8);
        let vertical_scale = data[9] >> 6;
        
        Ok(Vp8FrameHeader {
            key_frame, version, show_frame, first_part_size,
            width, height, horizontal_scale, vertical_scale,
        })
    }
    
    fn decode_frame(&mut self, data: &[u8], pts: Duration, dts: Duration) -> DecoderResult<Option<VideoFrame>> {
        let header = self.parse_frame_header(data)?;
        
        if header.key_frame {
            self.state.width = header.width;
            self.state.height = header.height;
        }
        
        let (w, h) = (self.state.width, self.state.height);
        if w == 0 || h == 0 { return Err(DecoderError::InvalidBitstream("No dimensions".into())); }
        
        let y_size = (w * h) as usize;
        let uv_size = y_size / 4;
        
        // Simulated decode - real impl would decode macroblocks
        let frame = VideoFrame {
            pts, dts, duration: Duration::from_millis(33), width: w, height: h,
            format: PixelFormat::I420, key_frame: header.key_frame,
            planes: vec![
                Plane { data: vec![128; y_size], stride: w as usize },
                Plane { data: vec![128; uv_size], stride: (w/2) as usize },
                Plane { data: vec![128; uv_size], stride: (w/2) as usize },
            ],
        };
        
        // Update reference frames
        if header.key_frame {
            self.last_frame = Some(frame.clone());
            self.golden_frame = Some(frame.clone());
            self.alt_ref_frame = Some(frame.clone());
        } else {
            self.last_frame = Some(frame.clone());
        }
        
        if header.show_frame { Ok(Some(frame)) } else { Ok(None) }
    }
}

impl Default for Vp8Decoder { fn default() -> Self { Self::new() } }

impl VideoDecoderTrait for Vp8Decoder {
    fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<Vec<VideoFrame>> {
        let mut frames = Vec::new();
        if let Some(f) = self.decode_frame(&packet.data, packet.pts, packet.dts)? {
            frames.push(f);
        }
        Ok(frames)
    }
    fn flush(&mut self) -> Vec<VideoFrame> { std::mem::take(&mut self.pending_output) }
    fn reset(&mut self) { self.last_frame = None; self.golden_frame = None; self.alt_ref_frame = None; self.state = VideoDecoderState::default(); }
    fn capabilities(&self) -> DecoderCaps { DecoderCaps { max_width: 4096, max_height: 2160, formats: vec![PixelFormat::I420], hardware: false } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_decoder() { let d = Vp8Decoder::new(); assert_eq!(d.capabilities().max_width, 4096); }
}
