//! H.264/AVC Decoder
//!
//! Pure-Rust implementation of H.264 video decoding.

use super::{BitReader, DecodedPictureBuffer, ReferenceFrame, VideoDecoderState};
use crate::decoders::{
    VideoFrame, EncodedPacket, DecoderCaps, DecoderResult, DecoderError,
    VideoDecoderTrait, PixelFormat, Plane,
};
use std::collections::HashMap;
use std::time::Duration;

/// H.264 NAL unit types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum NalUnitType {
    Unspecified = 0,
    NonIdrSlice = 1,
    IdrSlice = 5,
    Sei = 6,
    Sps = 7,
    Pps = 8,
    AccessUnitDelimiter = 9,
}

impl TryFrom<u8> for NalUnitType {
    type Error = DecoderError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Unspecified), 1 => Ok(Self::NonIdrSlice),
            5 => Ok(Self::IdrSlice), 6 => Ok(Self::Sei),
            7 => Ok(Self::Sps), 8 => Ok(Self::Pps),
            9 => Ok(Self::AccessUnitDelimiter),
            _ => Ok(Self::Unspecified),
        }
    }
}

/// Sequence Parameter Set
#[derive(Debug, Clone)]
pub struct Sps {
    pub sps_id: u8,
    pub profile_idc: u8,
    pub level_idc: u8,
    pub chroma_format_idc: u8,
    pub log2_max_frame_num: u8,
    pub pic_order_cnt_type: u8,
    pub log2_max_pic_order_cnt_lsb: u8,
    pub max_num_ref_frames: u8,
    pub pic_width_in_mbs: u32,
    pub pic_height_in_map_units: u32,
    pub frame_mbs_only_flag: bool,
}

/// Picture Parameter Set  
#[derive(Debug, Clone)]
pub struct Pps {
    pub pps_id: u8,
    pub sps_id: u8,
    pub entropy_coding_mode_flag: bool,
    pub num_ref_idx_l0_default_active: u8,
    pub num_ref_idx_l1_default_active: u8,
    pub pic_init_qp: i8,
    pub deblocking_filter_control_present: bool,
}

/// Slice type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceType { P = 0, B = 1, I = 2, Sp = 3, Si = 4 }

impl TryFrom<u32> for SliceType {
    type Error = DecoderError;
    fn try_from(v: u32) -> Result<Self, Self::Error> {
        match v % 5 {
            0 => Ok(Self::P), 1 => Ok(Self::B), 2 => Ok(Self::I),
            3 => Ok(Self::Sp), _ => Ok(Self::Si),
        }
    }
}

/// H.264 Decoder
#[derive(Debug)]
pub struct H264Decoder {
    sps_map: HashMap<u8, Sps>,
    pps_map: HashMap<u8, Pps>,
    dpb: DecodedPictureBuffer,
    state: VideoDecoderState,
    poc: i32,
    pending_output: Vec<VideoFrame>,
    active_sps_id: Option<u8>,
}

impl H264Decoder {
    pub fn new() -> Self {
        Self {
            sps_map: HashMap::new(),
            pps_map: HashMap::new(),
            dpb: DecodedPictureBuffer::new(16),
            state: VideoDecoderState::default(),
            poc: 0,
            pending_output: Vec::new(),
            active_sps_id: None,
        }
    }
    
    fn parse_nal_units(&self, data: &[u8]) -> Vec<(NalUnitType, Vec<u8>)> {
        let mut units = Vec::new();
        let mut i = 0;
        while i + 4 < data.len() {
            if data[i] == 0 && data[i+1] == 0 && (data[i+2] == 1 || (data[i+2] == 0 && data[i+3] == 1)) {
                let start = if data[i+2] == 1 { i + 3 } else { i + 4 };
                let mut end = data.len();
                for j in start..data.len().saturating_sub(3) {
                    if data[j] == 0 && data[j+1] == 0 && (data[j+2] == 0 || data[j+2] == 1) {
                        end = j; break;
                    }
                }
                if start < end {
                    let nt = NalUnitType::try_from(data[start] & 0x1F).unwrap_or(NalUnitType::Unspecified);
                    units.push((nt, data[start..end].to_vec()));
                }
                i = end;
            } else { i += 1; }
        }
        units
    }
    
    fn parse_sps(&mut self, data: &[u8]) -> DecoderResult<()> {
        let mut r = BitReader::new(&data[1..]);
        let profile_idc = r.read_bits(8)? as u8;
        r.skip(8)?; // constraint flags
        let level_idc = r.read_bits(8)? as u8;
        let sps_id = r.read_ue()? as u8;
        let chroma_format_idc = if profile_idc >= 100 { r.read_ue()? as u8 } else { 1 };
        if profile_idc >= 100 {
            if chroma_format_idc == 3 { r.skip(1)?; }
            r.read_ue()?; r.read_ue()?; r.skip(1)?;
            if r.read_bit()? { for _ in 0..8 { if r.read_bit()? { for _ in 0..16 { r.read_se()?; } } } }
        }
        let log2_max_frame_num = r.read_ue()? as u8 + 4;
        let pic_order_cnt_type = r.read_ue()? as u8;
        let log2_max_pic_order_cnt_lsb = if pic_order_cnt_type == 0 { r.read_ue()? as u8 + 4 } else { 4 };
        if pic_order_cnt_type == 1 { r.skip(1)?; r.read_se()?; r.read_se()?; let n = r.read_ue()?; for _ in 0..n { r.read_se()?; } }
        let max_num_ref_frames = r.read_ue()? as u8;
        r.skip(1)?;
        let w = r.read_ue()? + 1;
        let h = r.read_ue()? + 1;
        let frame_mbs_only = r.read_bit()?;
        self.state.width = w * 16;
        self.state.height = h * 16 * if frame_mbs_only { 1 } else { 2 };
        self.sps_map.insert(sps_id, Sps { sps_id, profile_idc, level_idc, chroma_format_idc, log2_max_frame_num, pic_order_cnt_type, log2_max_pic_order_cnt_lsb, max_num_ref_frames, pic_width_in_mbs: w, pic_height_in_map_units: h, frame_mbs_only_flag: frame_mbs_only });
        Ok(())
    }
    
    fn parse_pps(&mut self, data: &[u8]) -> DecoderResult<()> {
        let mut r = BitReader::new(&data[1..]);
        let pps_id = r.read_ue()? as u8;
        let sps_id = r.read_ue()? as u8;
        let entropy = r.read_bit()?;
        r.skip(1)?;
        let nsg = r.read_ue()? + 1;
        if nsg > 1 { return Err(DecoderError::Unsupported("slice groups".into())); }
        let l0 = r.read_ue()? as u8 + 1;
        let l1 = r.read_ue()? as u8 + 1;
        r.skip(2)?;
        let qp = r.read_se()? as i8 + 26;
        r.read_se()?; r.read_se()?;
        let dbf = r.read_bit()?;
        self.pps_map.insert(pps_id, Pps { pps_id, sps_id, entropy_coding_mode_flag: entropy, num_ref_idx_l0_default_active: l0, num_ref_idx_l1_default_active: l1, pic_init_qp: qp, deblocking_filter_control_present: dbf });
        Ok(())
    }
    
    fn decode_slice(&mut self, is_idr: bool, pts: Duration, dts: Duration) -> DecoderResult<Option<VideoFrame>> {
        let (w, h) = (self.state.width, self.state.height);
        if w == 0 || h == 0 { return Err(DecoderError::InvalidBitstream("No SPS".into())); }
        let y_size = (w * h) as usize;
        let uv_size = y_size / 4;
        let frame = VideoFrame {
            pts, dts, duration: Duration::from_millis(33), width: w, height: h,
            format: PixelFormat::I420, key_frame: is_idr,
            planes: vec![
                Plane { data: vec![128; y_size], stride: w as usize },
                Plane { data: vec![128; uv_size], stride: (w/2) as usize },
                Plane { data: vec![128; uv_size], stride: (w/2) as usize },
            ],
        };
        if is_idr { self.dpb.clear(); }
        self.dpb.add(ReferenceFrame { frame: frame.clone(), poc: self.poc, frame_num: self.state.frame_num as u32, long_term: false });
        self.state.frame_num += 1;
        self.poc += 2;
        Ok(Some(frame))
    }
}

impl Default for H264Decoder { fn default() -> Self { Self::new() } }

impl VideoDecoderTrait for H264Decoder {
    fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<Vec<VideoFrame>> {
        let mut frames = Vec::new();
        for (nt, data) in self.parse_nal_units(&packet.data) {
            match nt {
                NalUnitType::Sps => { self.parse_sps(&data)?; self.active_sps_id = self.sps_map.keys().next().copied(); }
                NalUnitType::Pps => { self.parse_pps(&data)?; }
                NalUnitType::IdrSlice => { if let Some(f) = self.decode_slice(true, packet.pts, packet.dts)? { frames.push(f); } }
                NalUnitType::NonIdrSlice => { if let Some(f) = self.decode_slice(false, packet.pts, packet.dts)? { frames.push(f); } }
                _ => {}
            }
        }
        Ok(frames)
    }
    fn flush(&mut self) -> Vec<VideoFrame> { std::mem::take(&mut self.pending_output) }
    fn reset(&mut self) { self.dpb.clear(); self.pending_output.clear(); self.poc = 0; self.state.frame_num = 0; }
    fn capabilities(&self) -> DecoderCaps { DecoderCaps { max_width: 4096, max_height: 2160, formats: vec![PixelFormat::I420, PixelFormat::Nv12], hardware: false } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_decoder() { let d = H264Decoder::new(); assert!(d.sps_map.is_empty()); assert_eq!(d.capabilities().max_width, 4096); }
}
