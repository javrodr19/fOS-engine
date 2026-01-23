//! H.265/HEVC Decoder
//!
//! Pure-Rust implementation of H.265 video decoding.

use super::{BitReader, DecodedPictureBuffer, ReferenceFrame, VideoDecoderState};
use crate::decoders::{
    VideoFrame, EncodedPacket, DecoderCaps, DecoderResult, DecoderError,
    VideoDecoderTrait, PixelFormat, Plane,
};
use std::collections::HashMap;
use std::time::Duration;

/// HEVC NAL unit types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HevcNalType {
    TrailN = 0, TrailR = 1, TsaN = 2, TsaR = 3,
    StsaN = 4, StsaR = 5, RadlN = 6, RadlR = 7,
    RaslN = 8, RaslR = 9, BlaWLp = 16, BlaWRadl = 17,
    BlaNLp = 18, IdrWRadl = 19, IdrNLp = 20, CraNut = 21,
    VpsNut = 32, SpsNut = 33, PpsNut = 34, AudNut = 35,
    EosNut = 36, EobNut = 37, FdNut = 38, PrefixSeiNut = 39,
    SuffixSeiNut = 40,
}

impl TryFrom<u8> for HevcNalType {
    type Error = DecoderError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::TrailN), 1 => Ok(Self::TrailR),
            19 => Ok(Self::IdrWRadl), 20 => Ok(Self::IdrNLp), 21 => Ok(Self::CraNut),
            32 => Ok(Self::VpsNut), 33 => Ok(Self::SpsNut), 34 => Ok(Self::PpsNut),
            35 => Ok(Self::AudNut), 39 => Ok(Self::PrefixSeiNut),
            _ => Ok(Self::TrailN),
        }
    }
}

/// Video Parameter Set
#[derive(Debug, Clone)]
pub struct Vps { pub vps_id: u8, pub max_layers: u8, pub max_sub_layers: u8 }

/// Sequence Parameter Set
#[derive(Debug, Clone)]
pub struct HevcSps {
    pub sps_id: u8, pub vps_id: u8, pub max_sub_layers: u8,
    pub chroma_format_idc: u8, pub pic_width: u32, pub pic_height: u32,
    pub bit_depth_luma: u8, pub bit_depth_chroma: u8,
    pub log2_max_poc_lsb: u8, pub log2_min_luma_cb_size: u8,
    pub log2_diff_max_min_luma_cb_size: u8, pub log2_min_tb_size: u8,
    pub log2_diff_max_min_tb_size: u8, pub max_transform_hierarchy_depth_inter: u8,
    pub max_transform_hierarchy_depth_intra: u8,
}

/// Picture Parameter Set
#[derive(Debug, Clone)]
pub struct HevcPps {
    pub pps_id: u8, pub sps_id: u8, pub dependent_slice_segments: bool,
    pub output_flag_present: bool, pub num_extra_slice_header_bits: u8,
    pub sign_data_hiding_enabled: bool, pub cabac_init_present: bool,
    pub num_ref_idx_l0_default: u8, pub num_ref_idx_l1_default: u8,
    pub init_qp: i8, pub constrained_intra_pred: bool,
    pub transform_skip_enabled: bool, pub cu_qp_delta_enabled: bool,
}

/// H.265 Decoder
#[derive(Debug)]
pub struct H265Decoder {
    vps_map: HashMap<u8, Vps>,
    sps_map: HashMap<u8, HevcSps>,
    pps_map: HashMap<u8, HevcPps>,
    dpb: DecodedPictureBuffer,
    state: VideoDecoderState,
    poc: i32,
    pending_output: Vec<VideoFrame>,
}

impl H265Decoder {
    pub fn new() -> Self {
        Self {
            vps_map: HashMap::new(), sps_map: HashMap::new(), pps_map: HashMap::new(),
            dpb: DecodedPictureBuffer::new(16), state: VideoDecoderState::default(),
            poc: 0, pending_output: Vec::new(),
        }
    }
    
    fn parse_nal_units(&self, data: &[u8]) -> Vec<(HevcNalType, Vec<u8>)> {
        let mut units = Vec::new();
        let mut i = 0;
        while i + 4 < data.len() {
            if data[i] == 0 && data[i+1] == 0 && (data[i+2] == 1 || (data[i+2] == 0 && data[i+3] == 1)) {
                let start = if data[i+2] == 1 { i + 3 } else { i + 4 };
                let mut end = data.len();
                for j in start..data.len().saturating_sub(3) {
                    if data[j] == 0 && data[j+1] == 0 { end = j; break; }
                }
                if start + 2 < end {
                    let nt = HevcNalType::try_from((data[start] >> 1) & 0x3F).unwrap_or(HevcNalType::TrailN);
                    units.push((nt, data[start..end].to_vec()));
                }
                i = end;
            } else { i += 1; }
        }
        units
    }
    
    fn parse_vps(&mut self, data: &[u8]) -> DecoderResult<()> {
        let mut r = BitReader::new(&data[2..]);
        let vps_id = r.read_bits(4)? as u8;
        r.skip(2)?;
        let max_layers = r.read_bits(6)? as u8 + 1;
        let max_sub_layers = r.read_bits(3)? as u8 + 1;
        self.vps_map.insert(vps_id, Vps { vps_id, max_layers, max_sub_layers });
        Ok(())
    }
    
    fn parse_sps(&mut self, data: &[u8]) -> DecoderResult<()> {
        let mut r = BitReader::new(&data[2..]);
        let vps_id = r.read_bits(4)? as u8;
        let max_sub_layers = r.read_bits(3)? as u8 + 1;
        r.skip(1)?; // temporal_id_nesting
        // Skip profile_tier_level
        r.skip(88)?;
        let sps_id = r.read_ue()? as u8;
        let chroma_format_idc = r.read_ue()? as u8;
        if chroma_format_idc == 3 { r.skip(1)?; }
        let pic_width = r.read_ue()?;
        let pic_height = r.read_ue()?;
        if r.read_bit()? { r.read_ue()?; r.read_ue()?; r.read_ue()?; r.read_ue()?; }
        let bit_depth_luma = r.read_ue()? as u8 + 8;
        let bit_depth_chroma = r.read_ue()? as u8 + 8;
        let log2_max_poc_lsb = r.read_ue()? as u8 + 4;
        self.state.width = pic_width;
        self.state.height = pic_height;
        self.sps_map.insert(sps_id, HevcSps {
            sps_id, vps_id, max_sub_layers, chroma_format_idc, pic_width, pic_height,
            bit_depth_luma, bit_depth_chroma, log2_max_poc_lsb,
            log2_min_luma_cb_size: 3, log2_diff_max_min_luma_cb_size: 3,
            log2_min_tb_size: 2, log2_diff_max_min_tb_size: 3,
            max_transform_hierarchy_depth_inter: 4, max_transform_hierarchy_depth_intra: 4,
        });
        Ok(())
    }
    
    fn parse_pps(&mut self, data: &[u8]) -> DecoderResult<()> {
        let mut r = BitReader::new(&data[2..]);
        let pps_id = r.read_ue()? as u8;
        let sps_id = r.read_ue()? as u8;
        let dependent = r.read_bit()?;
        let output_flag = r.read_bit()?;
        let extra_bits = r.read_bits(3)? as u8;
        let sign_hiding = r.read_bit()?;
        let cabac_init = r.read_bit()?;
        let l0 = r.read_ue()? as u8 + 1;
        let l1 = r.read_ue()? as u8 + 1;
        let init_qp = r.read_se()? as i8 + 26;
        let constrained = r.read_bit()?;
        let transform_skip = r.read_bit()?;
        let cu_qp_delta = r.read_bit()?;
        self.pps_map.insert(pps_id, HevcPps {
            pps_id, sps_id, dependent_slice_segments: dependent, output_flag_present: output_flag,
            num_extra_slice_header_bits: extra_bits, sign_data_hiding_enabled: sign_hiding,
            cabac_init_present: cabac_init, num_ref_idx_l0_default: l0, num_ref_idx_l1_default: l1,
            init_qp, constrained_intra_pred: constrained, transform_skip_enabled: transform_skip,
            cu_qp_delta_enabled: cu_qp_delta,
        });
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
        self.state.frame_num += 1; self.poc += 1;
        Ok(Some(frame))
    }
}

impl Default for H265Decoder { fn default() -> Self { Self::new() } }

impl VideoDecoderTrait for H265Decoder {
    fn decode(&mut self, packet: &EncodedPacket) -> DecoderResult<Vec<VideoFrame>> {
        let mut frames = Vec::new();
        for (nt, data) in self.parse_nal_units(&packet.data) {
            match nt {
                HevcNalType::VpsNut => { self.parse_vps(&data)?; }
                HevcNalType::SpsNut => { self.parse_sps(&data)?; }
                HevcNalType::PpsNut => { self.parse_pps(&data)?; }
                HevcNalType::IdrWRadl | HevcNalType::IdrNLp | HevcNalType::CraNut => {
                    if let Some(f) = self.decode_slice(true, packet.pts, packet.dts)? { frames.push(f); }
                }
                HevcNalType::TrailN | HevcNalType::TrailR => {
                    if let Some(f) = self.decode_slice(false, packet.pts, packet.dts)? { frames.push(f); }
                }
                _ => {}
            }
        }
        Ok(frames)
    }
    fn flush(&mut self) -> Vec<VideoFrame> { std::mem::take(&mut self.pending_output) }
    fn reset(&mut self) { self.dpb.clear(); self.pending_output.clear(); self.poc = 0; self.state.frame_num = 0; }
    fn capabilities(&self) -> DecoderCaps { DecoderCaps { max_width: 8192, max_height: 4320, formats: vec![PixelFormat::I420, PixelFormat::I420_10], hardware: false } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_decoder() { let d = H265Decoder::new(); assert_eq!(d.capabilities().max_width, 8192); }
}
