//! MPEG-TS Parser
//!
//! MPEG Transport Stream parser for HLS segments.

use super::{Demuxer, DemuxerResult, DemuxerError, TrackInfo, TrackType, CodecId, Packet};
use std::time::Duration;

const TS_PACKET_SIZE: usize = 188;
const SYNC_BYTE: u8 = 0x47;

/// MPEG-TS Demuxer
#[derive(Debug)]
pub struct TsDemuxer {
    data: Vec<u8>,
    pos: usize,
    video_pid: Option<u16>,
    audio_pid: Option<u16>,
    video_track: Option<TrackInfo>,
    audio_track: Option<TrackInfo>,
    video_buffer: Vec<u8>,
    audio_buffer: Vec<u8>,
    video_pts: Duration,
    audio_pts: Duration,
    eof: bool,
}

/// TS Packet header
#[derive(Debug)]
struct TsPacket<'a> {
    pid: u16,
    payload_unit_start: bool,
    adaptation_field_control: u8,
    continuity_counter: u8,
    payload: &'a [u8],
}

impl TsDemuxer {
    pub fn new(data: Vec<u8>) -> DemuxerResult<Self> {
        let mut demuxer = Self {
            data, pos: 0, video_pid: None, audio_pid: None,
            video_track: None, audio_track: None,
            video_buffer: Vec::new(), audio_buffer: Vec::new(),
            video_pts: Duration::ZERO, audio_pts: Duration::ZERO, eof: false,
        };
        demuxer.find_pids()?;
        Ok(demuxer)
    }
    
    fn parse_packet(&self, offset: usize) -> DemuxerResult<TsPacket> {
        if offset + TS_PACKET_SIZE > self.data.len() { return Err(DemuxerError::NeedMoreData); }
        if self.data[offset] != SYNC_BYTE { return Err(DemuxerError::InvalidContainer("Invalid sync".into())); }
        
        let pid = (((self.data[offset + 1] & 0x1F) as u16) << 8) | self.data[offset + 2] as u16;
        let payload_unit_start = (self.data[offset + 1] & 0x40) != 0;
        let adaptation_field_control = (self.data[offset + 3] >> 4) & 0x03;
        let continuity_counter = self.data[offset + 3] & 0x0F;
        
        let mut payload_start = offset + 4;
        if adaptation_field_control & 0x02 != 0 {
            let af_len = self.data[offset + 4] as usize;
            payload_start += 1 + af_len;
        }
        
        let payload = if adaptation_field_control & 0x01 != 0 && payload_start < offset + TS_PACKET_SIZE {
            &self.data[payload_start..offset + TS_PACKET_SIZE]
        } else {
            &[]
        };
        
        Ok(TsPacket { pid, payload_unit_start, adaptation_field_control, continuity_counter, payload })
    }
    
    fn find_pids(&mut self) -> DemuxerResult<()> {
        let mut pos = 0;
        while pos + TS_PACKET_SIZE <= self.data.len() {
            if self.data[pos] == SYNC_BYTE {
                if let Ok(pkt) = self.parse_packet(pos) {
                    // PAT is PID 0
                    if pkt.pid == 0 && !pkt.payload.is_empty() { self.parse_pat(pkt.payload)?; }
                    // PMT
                    if pkt.pid > 0 && pkt.pid < 0x1FFF && self.video_pid.is_none() {
                        self.parse_pmt(pkt.payload);
                    }
                }
                pos += TS_PACKET_SIZE;
            } else {
                pos += 1;
            }
            if self.video_pid.is_some() || self.audio_pid.is_some() { break; }
        }
        Ok(())
    }
    
    fn parse_pat(&mut self, data: &[u8]) -> DemuxerResult<()> {
        if data.len() < 8 { return Ok(()); }
        let pointer = data[0] as usize;
        let start = 1 + pointer;
        if start + 8 > data.len() { return Ok(()); }
        
        let _table_id = data[start];
        let section_len = (((data[start + 1] & 0x0F) as usize) << 8) | data[start + 2] as usize;
        
        let entries_start = start + 8;
        let entries_end = (start + 3 + section_len).min(data.len()).saturating_sub(4);
        
        let mut i = entries_start;
        while i + 4 <= entries_end {
            let _program_num = ((data[i] as u16) << 8) | data[i + 1] as u16;
            let _pmt_pid = (((data[i + 2] & 0x1F) as u16) << 8) | data[i + 3] as u16;
            i += 4;
        }
        Ok(())
    }
    
    fn parse_pmt(&mut self, data: &[u8]) {
        if data.len() < 12 { return; }
        
        // Look for stream types
        for i in 0..data.len().saturating_sub(5) {
            let stream_type = data[i];
            let pid = (((data[i + 1] & 0x1F) as u16) << 8) | data[i + 2] as u16;
            
            match stream_type {
                0x1B => { // H.264
                    self.video_pid = Some(pid);
                    self.video_track = Some(TrackInfo {
                        track_id: pid as u32, track_type: TrackType::Video, codec: CodecId::H264,
                        duration: Duration::ZERO, timescale: 90000,
                        width: Some(1920), height: Some(1080), frame_rate: Some(30.0),
                        sample_rate: None, channels: None, codec_private: Vec::new(),
                    });
                }
                0x24 => { // H.265
                    self.video_pid = Some(pid);
                    self.video_track = Some(TrackInfo {
                        track_id: pid as u32, track_type: TrackType::Video, codec: CodecId::H265,
                        duration: Duration::ZERO, timescale: 90000,
                        width: Some(1920), height: Some(1080), frame_rate: Some(30.0),
                        sample_rate: None, channels: None, codec_private: Vec::new(),
                    });
                }
                0x0F | 0x11 => { // AAC
                    self.audio_pid = Some(pid);
                    self.audio_track = Some(TrackInfo {
                        track_id: pid as u32, track_type: TrackType::Audio, codec: CodecId::Aac,
                        duration: Duration::ZERO, timescale: 90000,
                        width: None, height: None, frame_rate: None,
                        sample_rate: Some(48000), channels: Some(2), codec_private: Vec::new(),
                    });
                }
                _ => {}
            }
        }
    }
    
    fn parse_pes_header(&self, data: &[u8]) -> (Duration, usize) {
        if data.len() < 9 || data[0] != 0 || data[1] != 0 || data[2] != 1 { return (Duration::ZERO, 0); }
        
        let header_len = data[8] as usize;
        let pts_dts_flags = (data[7] >> 6) & 0x03;
        
        let pts = if pts_dts_flags >= 2 && data.len() >= 14 {
            let pts = (((data[9] >> 1) & 0x07) as u64) << 30
                    | ((data[10] as u64) << 22)
                    | (((data[11] >> 1) as u64) << 15)
                    | ((data[12] as u64) << 7)
                    | ((data[13] >> 1) as u64);
            Duration::from_nanos(pts * 1_000_000_000 / 90_000)
        } else {
            Duration::ZERO
        };
        
        (pts, 9 + header_len)
    }
}

impl Demuxer for TsDemuxer {
    fn duration(&self) -> Option<Duration> { None }
    fn video_track(&self) -> Option<&TrackInfo> { self.video_track.as_ref() }
    fn audio_track(&self) -> Option<&TrackInfo> { self.audio_track.as_ref() }
    
    fn read_packet(&mut self) -> DemuxerResult<Packet> {
        while self.pos + TS_PACKET_SIZE <= self.data.len() {
            let pkt = self.parse_packet(self.pos)?;
            self.pos += TS_PACKET_SIZE;
            
            if Some(pkt.pid) == self.video_pid && !pkt.payload.is_empty() {
                if pkt.payload_unit_start && !self.video_buffer.is_empty() {
                    let data = std::mem::take(&mut self.video_buffer);
                    return Ok(Packet {
                        track_id: pkt.pid as u32, pts: self.video_pts, dts: self.video_pts,
                        duration: Duration::from_millis(33), is_key: data.get(4).map(|b| b & 0x1F == 5).unwrap_or(false),
                        data,
                    });
                }
                if pkt.payload_unit_start {
                    let (pts, skip) = self.parse_pes_header(pkt.payload);
                    self.video_pts = pts;
                    self.video_buffer.extend_from_slice(&pkt.payload[skip.min(pkt.payload.len())..]);
                } else {
                    self.video_buffer.extend_from_slice(pkt.payload);
                }
            }
            
            if Some(pkt.pid) == self.audio_pid && !pkt.payload.is_empty() {
                if pkt.payload_unit_start && !self.audio_buffer.is_empty() {
                    let data = std::mem::take(&mut self.audio_buffer);
                    return Ok(Packet {
                        track_id: pkt.pid as u32, pts: self.audio_pts, dts: self.audio_pts,
                        duration: Duration::from_millis(21), is_key: true, data,
                    });
                }
                if pkt.payload_unit_start {
                    let (pts, skip) = self.parse_pes_header(pkt.payload);
                    self.audio_pts = pts;
                    self.audio_buffer.extend_from_slice(&pkt.payload[skip.min(pkt.payload.len())..]);
                } else {
                    self.audio_buffer.extend_from_slice(pkt.payload);
                }
            }
        }
        
        // Flush remaining
        if !self.video_buffer.is_empty() {
            let data = std::mem::take(&mut self.video_buffer);
            self.eof = true;
            return Ok(Packet { track_id: self.video_pid.unwrap_or(0) as u32, pts: self.video_pts, dts: self.video_pts, duration: Duration::from_millis(33), is_key: false, data });
        }
        
        self.eof = true;
        Err(DemuxerError::EndOfStream)
    }
    
    fn seek(&mut self, _position: Duration) -> DemuxerResult<()> { self.pos = 0; Ok(()) }
    fn is_eof(&self) -> bool { self.eof }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_constants() { assert_eq!(TS_PACKET_SIZE, 188); assert_eq!(SYNC_BYTE, 0x47); }
}
