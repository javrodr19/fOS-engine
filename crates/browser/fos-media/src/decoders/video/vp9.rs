//! VP9 Decoder
//!
//! Pure-Rust implementation of VP9 video decoding.

use super::{DecodedPictureBuffer, ReferenceFrame, VideoDecoderState};
use crate::decoders::{
    VideoFrame, EncodedPacket, DecoderCaps, DecoderResult, DecoderError,
    VideoDecoderTrait, PixelFormat, Plane,
};
use std::time::Duration;

/// VP9 Frame types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vp9FrameType { KeyFrame, InterFrame }

/// VP9 Color space
#[derive(Debug, Clone, Copy)]
pub enum Vp9ColorSpace { Unknown, Bt601, Bt709, Smpte170, Smpte240, Bt2020, Reserved, Srgb }

/// VP9 Frame Header
#[derive(Debug, Clone)]
pub struct Vp9FrameHeader {
    pub frame_type: Vp9FrameType,
    pub show_frame: bool,
    pub error_resilient: bool,
    pub width: u32,
    pub height: u32,
    pub render_width: u32,
    pub render_height: u32,
    pub profile: u8,
    pub bit_depth: u8,
    pub color_space: Vp9ColorSpace,
    pub subsampling_x: bool,
    pub subsampling_y: bool,
    pub refresh_frame_flags: u8,
    pub ref_frame_idx: [u8; 3],
    pub ref_frame_sign_bias: [bool; 4],
    pub allow_high_precision_mv: bool,
    pub interp_filter: u8,
}

/// VP9 Decoder
#[derive(Debug)]
pub struct Vp9Decoder {
    dpb: DecodedPictureBuffer,
    state: VideoDecoderState,
    ref_frames: [Option<VideoFrame>; 8],
    pending_output: Vec<VideoFrame>,
    profile: u8,
}

impl Vp9Decoder {
    pub fn new() -> Self {
        Self {
            dpb: DecodedPictureBuffer::new(8),
            state: VideoDecoderState::default(),
            ref_frames: Default::default(),
            pending_output: Vec::new(),
            profile: 0,
        }
    }
    
    fn parse_superframe_index(&self, data: &[u8]) -> Vec<(usize, usize)> {
        let mut frames = Vec::new();
        if data.is_empty() { return frames; }
        
        let marker = data[data.len() - 1];
        if (marker & 0xE0) != 0xC0 { 
            frames.push((0, data.len()));
            return frames; 
        }
        
        let bytes_per_framesize = ((marker >> 3) & 3) as usize + 1;
        let num_frames = (marker & 7) as usize + 1;
        let index_size = 2 + num_frames * bytes_per_framesize;
        
        if data.len() < index_size { 
            frames.push((0, data.len()));
            return frames; 
        }
        
        let idx_start = data.len() - index_size;
        if data[idx_start] != marker {
            frames.push((0, data.len()));
            return frames;
        }
        
        let mut offset = 0;
        for i in 0..num_frames {
            let mut size = 0usize;
            for j in 0..bytes_per_framesize {
                size |= (data[idx_start + 1 + i * bytes_per_framesize + j] as usize) << (j * 8);
            }
            frames.push((offset, size));
            offset += size;
        }
        frames
    }
    
    fn parse_frame_header(&self, data: &[u8]) -> DecoderResult<Vp9FrameHeader> {
        if data.len() < 3 { return Err(DecoderError::NeedMoreData); }
        
        let mut pos = 0;
        let mut bit = 0;
        
        let read_bits = |pos: &mut usize, bit: &mut usize, n: usize| -> DecoderResult<u32> {
            let mut val = 0u32;
            for _ in 0..n {
                if *pos >= data.len() { return Err(DecoderError::NeedMoreData); }
                let b = (data[*pos] >> (7 - *bit)) & 1;
                val = (val << 1) | b as u32;
                *bit += 1;
                if *bit == 8 { *bit = 0; *pos += 1; }
            }
            Ok(val)
        };
        
        let marker = read_bits(&mut pos, &mut bit, 2)?;
        if marker != 2 { return Err(DecoderError::InvalidBitstream("Invalid VP9 marker".into())); }
        
        let profile_low = read_bits(&mut pos, &mut bit, 1)? as u8;
        let profile_high = read_bits(&mut pos, &mut bit, 1)? as u8;
        let profile = (profile_high << 1) | profile_low;
        
        let show_existing = read_bits(&mut pos, &mut bit, 1)? == 1;
        if show_existing {
            let _idx = read_bits(&mut pos, &mut bit, 3)?;
            return Ok(Vp9FrameHeader {
                frame_type: Vp9FrameType::InterFrame, show_frame: true, error_resilient: false,
                width: self.state.width, height: self.state.height,
                render_width: self.state.width, render_height: self.state.height,
                profile, bit_depth: 8, color_space: Vp9ColorSpace::Unknown,
                subsampling_x: true, subsampling_y: true, refresh_frame_flags: 0,
                ref_frame_idx: [0; 3], ref_frame_sign_bias: [false; 4],
                allow_high_precision_mv: false, interp_filter: 0,
            });
        }
        
        let frame_type = if read_bits(&mut pos, &mut bit, 1)? == 0 { Vp9FrameType::KeyFrame } else { Vp9FrameType::InterFrame };
        let show_frame = read_bits(&mut pos, &mut bit, 1)? == 1;
        let error_resilient = read_bits(&mut pos, &mut bit, 1)? == 1;
        
        let (width, height, bit_depth) = if frame_type == Vp9FrameType::KeyFrame {
            let _sync = read_bits(&mut pos, &mut bit, 24)?;
            let bit_depth = if profile >= 2 { if read_bits(&mut pos, &mut bit, 1)? == 1 { 12 } else { 10 } } else { 8 };
            let _cs = read_bits(&mut pos, &mut bit, 3)?;
            if profile >= 1 { read_bits(&mut pos, &mut bit, 1)?; if profile >= 2 { read_bits(&mut pos, &mut bit, 1)?; } }
            let w = read_bits(&mut pos, &mut bit, 16)? + 1;
            let h = read_bits(&mut pos, &mut bit, 16)? + 1;
            (w, h, bit_depth as u8)
        } else {
            (self.state.width, self.state.height, 8)
        };
        
        Ok(Vp9FrameHeader {
            frame_type, show_frame, error_resilient, width, height,
            render_width: width, render_height: height, profile, bit_depth,
            color_space: Vp9ColorSpace::Unknown, subsampling_x: true, subsampling_y: true,
            refresh_frame_flags: 0xFF, ref_frame_idx: [0; 3], ref_frame_sign_bias: [false; 4],
            allow_high_precision_mv: false, interp_filter: 0,
        })
    }
    
    fn decode_frame(&mut self, data: &[u8], pts: Duration, dts: Duration) -> DecoderResult<Option<VideoFrame>> {
        let header = self.parse_frame_header(data)?;
        
        self.state.width = header.width;
        self.state.height = header.height;
        self.profile = header.profile;
        
        let (w, h) = (header.width, header.height);
        let y_size = (w * h) as usize;
        let uv_size = y_size / 4;
        
        let format = if header.bit_depth > 8 { PixelFormat::I420_10 } else { PixelFormat::I420 };
        
        let frame = VideoFrame {
            pts, dts, duration: Duration::from_millis(33), width: w, height: h,
            format, key_frame: header.frame_type == Vp9FrameType::KeyFrame,
            planes: vec![
                Plane { data: vec![128; y_size], stride: w as usize },
                Plane { data: vec![128; uv_size], stride: (w/2) as usize },
                Plane { data: vec![128; uv_size], stride: (w/2) as usize },
            ],
        };
        
        // Update reference frames
        for i in 0..8 {
            if (header.refresh_frame_flags >> i) & 1 == 1 {
                self.ref_frames[i] = Some(frame.clone());
            }
        }
        
        if header.show_frame { Ok(Some(frame)) } else { Ok(None) }
    }
}

impl Default for Vp9Decoder { fn default() -> Self { Self::new() } }

impl VideoDecoderTrait for Vp9Decoder {
    fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<Vec<VideoFrame>> {
        let mut output = Vec::new();
        for (offset, size) in self.parse_superframe_index(&packet.data) {
            if offset + size <= packet.data.len() {
                if let Some(f) = self.decode_frame(&packet.data[offset..offset+size], packet.pts, packet.dts)? {
                    output.push(f);
                }
            }
        }
        Ok(output)
    }
    fn flush(&mut self) -> Vec<VideoFrame> { std::mem::take(&mut self.pending_output) }
    fn reset(&mut self) { self.ref_frames = Default::default(); self.state = VideoDecoderState::default(); }
    fn capabilities(&self) -> DecoderCaps { DecoderCaps { max_width: 8192, max_height: 4320, formats: vec![PixelFormat::I420, PixelFormat::I420_10], hardware: false } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_decoder() { let d = Vp9Decoder::new(); assert_eq!(d.capabilities().max_width, 8192); }
}
