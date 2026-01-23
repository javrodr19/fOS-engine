//! Fragmented MP4 Parser
//!
//! Parser for fragmented MP4 (fMP4) used in DASH and MSE.

use super::{Demuxer, DemuxerResult, DemuxerError, TrackInfo, Packet};
use super::mp4::{Mp4Demuxer, BoxType};
use std::time::Duration;

/// Fragmented MP4 Demuxer
#[derive(Debug)]
pub struct Fmp4Demuxer {
    init_segment: Option<Mp4Demuxer>,
    media_data: Vec<u8>,
    pos: usize,
    base_decode_time: u64,
    timescale: u32,
    eof: bool,
}

impl Fmp4Demuxer {
    pub fn new() -> Self {
        Self { init_segment: None, media_data: Vec::new(), pos: 0, base_decode_time: 0, timescale: 1000, eof: false }
    }
    
    /// Set initialization segment (moov box)
    pub fn set_init_segment(&mut self, data: Vec<u8>) -> DemuxerResult<()> {
        self.init_segment = Some(Mp4Demuxer::new(data)?);
        if let Some(vt) = self.init_segment.as_ref().and_then(|d| d.video_track()) {
            self.timescale = vt.timescale;
        }
        Ok(())
    }
    
    /// Append media segment (moof + mdat)
    pub fn append_segment(&mut self, data: Vec<u8>) -> DemuxerResult<()> {
        self.media_data.extend(data);
        self.eof = false;
        Ok(())
    }
    
    fn read_u32(&self, pos: usize) -> u32 {
        if pos + 4 > self.media_data.len() { return 0; }
        u32::from_be_bytes([self.media_data[pos], self.media_data[pos+1], self.media_data[pos+2], self.media_data[pos+3]])
    }
    
    fn read_u64(&self, pos: usize) -> u64 {
        if pos + 8 > self.media_data.len() { return 0; }
        u64::from_be_bytes([self.media_data[pos], self.media_data[pos+1], self.media_data[pos+2], self.media_data[pos+3], self.media_data[pos+4], self.media_data[pos+5], self.media_data[pos+6], self.media_data[pos+7]])
    }
    
    fn parse_moof(&mut self, start: usize, _len: usize) -> Vec<(u64, u32, Duration, bool)> {
        let mut samples = Vec::new();
        let mut pos = start;
        
        while pos + 8 <= self.media_data.len() {
            let size = self.read_u32(pos) as usize;
            if size < 8 { break; }
            let box_type = &self.media_data[pos+4..pos+8];
            
            if box_type == b"traf" {
                // Parse traf
                let mut traf_pos = pos + 8;
                let traf_end = pos + size;
                
                while traf_pos + 8 <= traf_end {
                    let traf_size = self.read_u32(traf_pos) as usize;
                    if traf_size < 8 { break; }
                    let traf_box = &self.media_data[traf_pos+4..traf_pos+8];
                    
                    if traf_box == b"tfdt" && traf_pos + 16 <= self.media_data.len() {
                        let version = self.media_data[traf_pos + 8];
                        self.base_decode_time = if version == 1 { self.read_u64(traf_pos + 12) } else { self.read_u32(traf_pos + 12) as u64 };
                    }
                    
                    if traf_box == b"trun" && traf_pos + 12 <= self.media_data.len() {
                        let flags = self.read_u32(traf_pos + 8) & 0xFFFFFF;
                        let sample_count = self.read_u32(traf_pos + 12);
                        
                        let mut offset = traf_pos + 16;
                        if flags & 0x001 != 0 { offset += 4; } // data offset
                        if flags & 0x004 != 0 { offset += 4; } // first sample flags
                        
                        let has_duration = flags & 0x100 != 0;
                        let has_size = flags & 0x200 != 0;
                        let has_flags = flags & 0x400 != 0;
                        let has_cts = flags & 0x800 != 0;
                        
                        let mut current_time = self.base_decode_time;
                        
                        for _ in 0..sample_count {
                            let duration = if has_duration { let d = self.read_u32(offset); offset += 4; d } else { 1024 };
                            let size = if has_size { let s = self.read_u32(offset); offset += 4; s } else { 0 };
                            let flags = if has_flags { let f = self.read_u32(offset); offset += 4; f } else { 0 };
                            if has_cts { offset += 4; }
                            
                            let is_key = (flags >> 16) & 0xFF == 0;
                            let pts = Duration::from_secs_f64(current_time as f64 / self.timescale as f64);
                            
                            samples.push((0, size, pts, is_key));
                            current_time += duration as u64;
                        }
                    }
                    
                    traf_pos += traf_size;
                }
            }
            
            if box_type == b"mdat" { break; }
            pos += size;
        }
        
        samples
    }
}

impl Default for Fmp4Demuxer { fn default() -> Self { Self::new() } }

impl Demuxer for Fmp4Demuxer {
    fn duration(&self) -> Option<Duration> { self.init_segment.as_ref().and_then(|d| d.duration()) }
    fn video_track(&self) -> Option<&TrackInfo> { self.init_segment.as_ref().and_then(|d| d.video_track()) }
    fn audio_track(&self) -> Option<&TrackInfo> { self.init_segment.as_ref().and_then(|d| d.audio_track()) }
    
    fn read_packet(&mut self) -> DemuxerResult<Packet> {
        // Parse moof to get sample info, then read from mdat
        let samples = self.parse_moof(0, self.media_data.len());
        if samples.is_empty() { self.eof = true; return Err(DemuxerError::EndOfStream); }
        
        // Find mdat
        let mut mdat_start = 0;
        let mut pos = 0;
        while pos + 8 <= self.media_data.len() {
            let size = self.read_u32(pos) as usize;
            if size < 8 { break; }
            if &self.media_data[pos+4..pos+8] == b"mdat" { mdat_start = pos + 8; break; }
            pos += size;
        }
        
        if mdat_start == 0 || self.pos >= samples.len() { self.eof = true; return Err(DemuxerError::EndOfStream); }
        
        let (_, size, pts, is_key) = samples[self.pos];
        let mut offset = mdat_start;
        for i in 0..self.pos { offset += samples[i].1 as usize; }
        
        if offset + size as usize > self.media_data.len() { return Err(DemuxerError::IoError("Out of bounds".into())); }
        
        let data = self.media_data[offset..offset + size as usize].to_vec();
        self.pos += 1;
        
        Ok(Packet { track_id: 1, pts, dts: pts, duration: Duration::from_millis(33), is_key, data })
    }
    
    fn seek(&mut self, _position: Duration) -> DemuxerResult<()> { self.pos = 0; Ok(()) }
    fn is_eof(&self) -> bool { self.eof }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_fmp4() { let d = Fmp4Demuxer::new(); assert!(d.init_segment.is_none()); }
}
