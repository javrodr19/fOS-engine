//! WebM Parser
//!
//! EBML-based WebM container parser.

use super::{Demuxer, DemuxerResult, DemuxerError, TrackInfo, TrackType, CodecId, Packet};
use std::time::Duration;

/// EBML Element IDs
const EBML_ID: u32 = 0x1A45DFA3;
const SEGMENT_ID: u32 = 0x18538067;
const INFO_ID: u32 = 0x1549A966;
const TRACKS_ID: u32 = 0x1654AE6B;
const TRACK_ENTRY_ID: u32 = 0xAE;
const CLUSTER_ID: u32 = 0x1F43B675;
const SIMPLE_BLOCK_ID: u32 = 0xA3;
const BLOCK_ID: u32 = 0xA1;
const TIMECODE_SCALE_ID: u32 = 0x2AD7B1;
const DURATION_ID: u32 = 0x4489;
const TRACK_NUMBER_ID: u32 = 0xD7;
const TRACK_TYPE_ID: u32 = 0x83;
const CODEC_ID: u32 = 0x86;
const VIDEO_ID: u32 = 0xE0;
const AUDIO_ID: u32 = 0xE1;
const PIXEL_WIDTH_ID: u32 = 0xB0;
const PIXEL_HEIGHT_ID: u32 = 0xBA;
const SAMPLING_FREQ_ID: u32 = 0xB5;
const CHANNELS_ID: u32 = 0x9F;
const CODEC_PRIVATE_ID: u32 = 0x63A2;

/// WebM Demuxer
#[derive(Debug)]
pub struct WebMDemuxer {
    data: Vec<u8>,
    pos: usize,
    timecode_scale: u64,
    duration: Duration,
    video_track: Option<TrackInfo>,
    audio_track: Option<TrackInfo>,
    cluster_positions: Vec<(usize, u64)>, // (offset, timecode)
    current_cluster: usize,
    cluster_pos: usize,
    cluster_timecode: u64,
    eof: bool,
}

impl WebMDemuxer {
    pub fn new(data: Vec<u8>) -> DemuxerResult<Self> {
        let mut demuxer = Self {
            data, pos: 0, timecode_scale: 1_000_000, duration: Duration::ZERO,
            video_track: None, audio_track: None, cluster_positions: Vec::new(),
            current_cluster: 0, cluster_pos: 0, cluster_timecode: 0, eof: false,
        };
        demuxer.parse()?;
        Ok(demuxer)
    }
    
    fn read_vint(&self, pos: &mut usize) -> Option<(u32, u64)> {
        if *pos >= self.data.len() { return None; }
        let first = self.data[*pos];
        let len = first.leading_zeros() as usize + 1;
        if *pos + len > self.data.len() { return None; }
        
        let mut value = (first & ((1 << (8 - len)) - 1)) as u64;
        for i in 1..len {
            value = (value << 8) | self.data[*pos + i] as u64;
        }
        let id_bytes = len;
        *pos += len;
        Some((id_bytes as u32, value))
    }
    
    fn read_element_id(&self, pos: &mut usize) -> Option<u32> {
        if *pos >= self.data.len() { return None; }
        let first = self.data[*pos];
        let len = first.leading_zeros() as usize + 1;
        if *pos + len > self.data.len() { return None; }
        
        let mut id = 0u32;
        for i in 0..len { id = (id << 8) | self.data[*pos + i] as u32; }
        *pos += len;
        Some(id)
    }
    
    fn read_uint(&self, pos: usize, len: usize) -> u64 {
        let mut val = 0u64;
        for i in 0..len.min(8) {
            if pos + i < self.data.len() { val = (val << 8) | self.data[pos + i] as u64; }
        }
        val
    }
    
    fn read_float(&self, pos: usize, len: usize) -> f64 {
        if len == 4 {
            let bits = self.read_uint(pos, 4) as u32;
            f32::from_bits(bits) as f64
        } else {
            let bits = self.read_uint(pos, 8);
            f64::from_bits(bits)
        }
    }
    
    fn parse(&mut self) -> DemuxerResult<()> {
        let mut pos = 0;
        
        // Parse EBML header
        if let Some(id) = self.read_element_id(&mut pos) {
            if id != EBML_ID { return Err(DemuxerError::InvalidContainer("Not EBML".into())); }
        }
        let (_, ebml_size) = self.read_vint(&mut pos).ok_or(DemuxerError::NeedMoreData)?;
        pos += ebml_size as usize;
        
        // Parse Segment
        while pos < self.data.len() {
            let elem_start = pos;
            let id = self.read_element_id(&mut pos).ok_or(DemuxerError::NeedMoreData)?;
            let (_, size) = self.read_vint(&mut pos).ok_or(DemuxerError::NeedMoreData)?;
            
            match id {
                INFO_ID => self.parse_info(pos, size as usize)?,
                TRACKS_ID => self.parse_tracks(pos, size as usize)?,
                CLUSTER_ID => { self.cluster_positions.push((elem_start, 0)); }
                _ => {}
            }
            
            if size == 0xFFFFFFFFFFFFFF { break; } // Unknown size
            pos += size as usize;
        }
        
        Ok(())
    }
    
    fn parse_info(&mut self, start: usize, len: usize) -> DemuxerResult<()> {
        let mut pos = start;
        let end = start + len;
        
        while pos < end {
            let id = self.read_element_id(&mut pos).ok_or(DemuxerError::NeedMoreData)?;
            let (_, size) = self.read_vint(&mut pos).ok_or(DemuxerError::NeedMoreData)?;
            
            match id {
                TIMECODE_SCALE_ID => { self.timecode_scale = self.read_uint(pos, size as usize); }
                DURATION_ID => {
                    let dur_float = self.read_float(pos, size as usize);
                    self.duration = Duration::from_nanos((dur_float * self.timecode_scale as f64) as u64);
                }
                _ => {}
            }
            pos += size as usize;
        }
        Ok(())
    }
    
    fn parse_tracks(&mut self, start: usize, len: usize) -> DemuxerResult<()> {
        let mut pos = start;
        let end = start + len;
        
        while pos < end {
            let id = self.read_element_id(&mut pos).ok_or(DemuxerError::NeedMoreData)?;
            let (_, size) = self.read_vint(&mut pos).ok_or(DemuxerError::NeedMoreData)?;
            
            if id == TRACK_ENTRY_ID { self.parse_track_entry(pos, size as usize)?; }
            pos += size as usize;
        }
        Ok(())
    }
    
    fn parse_track_entry(&mut self, start: usize, len: usize) -> DemuxerResult<()> {
        let mut pos = start;
        let end = start + len;
        let mut track_num = 0u32;
        let mut track_type = TrackType::Data;
        let mut codec = CodecId::Unknown;
        let mut width = 0u32;
        let mut height = 0u32;
        let mut sample_rate = 0u32;
        let mut channels = 0u32;
        let mut codec_private = Vec::new();
        
        while pos < end {
            let id = self.read_element_id(&mut pos).ok_or(DemuxerError::NeedMoreData)?;
            let (_, size) = self.read_vint(&mut pos).ok_or(DemuxerError::NeedMoreData)?;
            
            match id {
                TRACK_NUMBER_ID => { track_num = self.read_uint(pos, size as usize) as u32; }
                TRACK_TYPE_ID => {
                    let t = self.read_uint(pos, size as usize);
                    track_type = match t { 1 => TrackType::Video, 2 => TrackType::Audio, 17 => TrackType::Subtitle, _ => TrackType::Data };
                }
                CODEC_ID => {
                    let codec_str = String::from_utf8_lossy(&self.data[pos..pos + size as usize]);
                    codec = match codec_str.as_ref() {
                        "V_VP8" => CodecId::Vp8, "V_VP9" => CodecId::Vp9, "V_AV1" => CodecId::Av1,
                        "A_OPUS" => CodecId::Opus, "A_VORBIS" => CodecId::Vorbis, _ => CodecId::Unknown,
                    };
                }
                VIDEO_ID => {
                    let mut vpos = pos;
                    let vend = pos + size as usize;
                    while vpos < vend {
                        let vid = self.read_element_id(&mut vpos).unwrap_or(0);
                        let (_, vsize) = self.read_vint(&mut vpos).unwrap_or((0, 0));
                        if vid == PIXEL_WIDTH_ID { width = self.read_uint(vpos, vsize as usize) as u32; }
                        if vid == PIXEL_HEIGHT_ID { height = self.read_uint(vpos, vsize as usize) as u32; }
                        vpos += vsize as usize;
                    }
                }
                AUDIO_ID => {
                    let mut apos = pos;
                    let aend = pos + size as usize;
                    while apos < aend {
                        let aid = self.read_element_id(&mut apos).unwrap_or(0);
                        let (_, asize) = self.read_vint(&mut apos).unwrap_or((0, 0));
                        if aid == SAMPLING_FREQ_ID { sample_rate = self.read_float(apos, asize as usize) as u32; }
                        if aid == CHANNELS_ID { channels = self.read_uint(apos, asize as usize) as u32; }
                        apos += asize as usize;
                    }
                }
                CODEC_PRIVATE_ID => { codec_private = self.data[pos..pos + size as usize].to_vec(); }
                _ => {}
            }
            pos += size as usize;
        }
        
        let track = TrackInfo {
            track_id: track_num, track_type, codec, duration: self.duration, timescale: 1_000_000_000,
            width: if width > 0 { Some(width) } else { None }, height: if height > 0 { Some(height) } else { None },
            frame_rate: None, sample_rate: if sample_rate > 0 { Some(sample_rate) } else { None },
            channels: if channels > 0 { Some(channels) } else { None }, codec_private,
        };
        
        if track_type == TrackType::Video && self.video_track.is_none() { self.video_track = Some(track); }
        else if track_type == TrackType::Audio && self.audio_track.is_none() { self.audio_track = Some(track); }
        
        Ok(())
    }
}

impl Demuxer for WebMDemuxer {
    fn duration(&self) -> Option<Duration> { Some(self.duration) }
    fn video_track(&self) -> Option<&TrackInfo> { self.video_track.as_ref() }
    fn audio_track(&self) -> Option<&TrackInfo> { self.audio_track.as_ref() }
    fn read_packet(&mut self) -> DemuxerResult<Packet> { self.eof = true; Err(DemuxerError::EndOfStream) }
    fn seek(&mut self, _position: Duration) -> DemuxerResult<()> { Ok(()) }
    fn is_eof(&self) -> bool { self.eof }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_ebml_id() { assert_eq!(EBML_ID, 0x1A45DFA3); }
}
