//! MP4/MOV Parser
//!
//! ISO Base Media File Format (ISOBMFF) parser for MP4 and MOV files.

use super::{Demuxer, DemuxerResult, DemuxerError, TrackInfo, TrackType, CodecId, Packet};
use std::time::Duration;

/// MP4 Box types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoxType([u8; 4]);

impl BoxType {
    pub const FTYP: Self = Self(*b"ftyp");
    pub const MOOV: Self = Self(*b"moov");
    pub const MVHD: Self = Self(*b"mvhd");
    pub const TRAK: Self = Self(*b"trak");
    pub const TKHD: Self = Self(*b"tkhd");
    pub const MDIA: Self = Self(*b"mdia");
    pub const MDHD: Self = Self(*b"mdhd");
    pub const HDLR: Self = Self(*b"hdlr");
    pub const MINF: Self = Self(*b"minf");
    pub const STBL: Self = Self(*b"stbl");
    pub const STSD: Self = Self(*b"stsd");
    pub const STTS: Self = Self(*b"stts");
    pub const STSS: Self = Self(*b"stss");
    pub const STSC: Self = Self(*b"stsc");
    pub const STSZ: Self = Self(*b"stsz");
    pub const STCO: Self = Self(*b"stco");
    pub const CO64: Self = Self(*b"co64");
    pub const CTTS: Self = Self(*b"ctts");
    pub const MDAT: Self = Self(*b"mdat");
    pub const AVCC: Self = Self(*b"avcC");
    pub const HVCC: Self = Self(*b"hvcC");
    pub const VP09: Self = Self(*b"vp09");
    pub const AV1C: Self = Self(*b"av1C");
    pub const ESDS: Self = Self(*b"esds");
}

/// MP4 Box header
#[derive(Debug, Clone)]
pub struct Box { pub box_type: BoxType, pub size: u64, pub offset: u64 }

/// MP4 Demuxer
#[derive(Debug)]
pub struct Mp4Demuxer {
    data: Vec<u8>,
    pos: usize,
    duration: Duration,
    timescale: u32,
    video_track: Option<TrackInfo>,
    audio_track: Option<TrackInfo>,
    samples: Vec<SampleInfo>,
    current_sample: usize,
    eof: bool,
}

#[derive(Debug, Clone)]
struct SampleInfo { track_id: u32, offset: u64, size: u32, pts: Duration, dts: Duration, duration: Duration, is_key: bool }

impl Mp4Demuxer {
    pub fn new(data: Vec<u8>) -> DemuxerResult<Self> {
        let mut demuxer = Self {
            data, pos: 0, duration: Duration::ZERO, timescale: 1000,
            video_track: None, audio_track: None, samples: Vec::new(), current_sample: 0, eof: false,
        };
        demuxer.parse()?;
        Ok(demuxer)
    }
    
    fn read_u32(&self, pos: usize) -> u32 {
        if pos + 4 > self.data.len() { return 0; }
        u32::from_be_bytes([self.data[pos], self.data[pos+1], self.data[pos+2], self.data[pos+3]])
    }
    
    fn read_u64(&self, pos: usize) -> u64 {
        if pos + 8 > self.data.len() { return 0; }
        u64::from_be_bytes([self.data[pos], self.data[pos+1], self.data[pos+2], self.data[pos+3], self.data[pos+4], self.data[pos+5], self.data[pos+6], self.data[pos+7]])
    }
    
    fn parse(&mut self) -> DemuxerResult<()> {
        let mut pos = 0;
        while pos + 8 <= self.data.len() {
            let size = self.read_u32(pos) as u64;
            let box_type = BoxType([self.data[pos+4], self.data[pos+5], self.data[pos+6], self.data[pos+7]]);
            
            let real_size = if size == 1 { self.read_u64(pos + 8) } else if size == 0 { (self.data.len() - pos) as u64 } else { size };
            
            if box_type == BoxType::MOOV { self.parse_moov(pos + 8, real_size as usize - 8)?; }
            
            pos += real_size as usize;
        }
        
        // Sort samples by DTS
        self.samples.sort_by(|a, b| a.dts.cmp(&b.dts));
        Ok(())
    }
    
    fn parse_moov(&mut self, start: usize, len: usize) -> DemuxerResult<()> {
        let mut pos = start;
        let end = start + len;
        let mut track_id = 0u32;
        
        while pos + 8 <= end {
            let size = self.read_u32(pos) as usize;
            if size < 8 { break; }
            let box_type = BoxType([self.data[pos+4], self.data[pos+5], self.data[pos+6], self.data[pos+7]]);
            
            match box_type {
                BoxType::MVHD => {
                    let version = self.data.get(pos + 8).copied().unwrap_or(0);
                    if version == 1 {
                        self.timescale = self.read_u32(pos + 28);
                        let dur = self.read_u64(pos + 32);
                        self.duration = Duration::from_secs_f64(dur as f64 / self.timescale as f64);
                    } else {
                        self.timescale = self.read_u32(pos + 20);
                        let dur = self.read_u32(pos + 24) as u64;
                        self.duration = Duration::from_secs_f64(dur as f64 / self.timescale as f64);
                    }
                }
                BoxType::TRAK => {
                    track_id += 1;
                    self.parse_trak(pos + 8, size - 8, track_id)?;
                }
                _ => {}
            }
            pos += size;
        }
        Ok(())
    }
    
    fn parse_trak(&mut self, start: usize, len: usize, track_id: u32) -> DemuxerResult<()> {
        let mut pos = start;
        let end = start + len;
        let mut track_type = TrackType::Data;
        let mut codec = CodecId::Unknown;
        let mut timescale = 1000u32;
        let mut dur = 0u64;
        let mut width = 0u32;
        let mut height = 0u32;
        let mut sample_rate = 0u32;
        let mut channels = 0u32;
        let mut codec_private = Vec::new();
        
        while pos + 8 <= end {
            let size = self.read_u32(pos) as usize;
            if size < 8 { break; }
            let box_type = BoxType([self.data[pos+4], self.data[pos+5], self.data[pos+6], self.data[pos+7]]);
            
            match box_type {
                BoxType::MDIA => { self.parse_mdia(pos + 8, size - 8, &mut track_type, &mut codec, &mut timescale, &mut dur, &mut width, &mut height, &mut sample_rate, &mut channels, &mut codec_private)?; }
                _ => {}
            }
            pos += size;
        }
        
        let track = TrackInfo {
            track_id, track_type, codec, duration: Duration::from_secs_f64(dur as f64 / timescale as f64),
            timescale, width: if width > 0 { Some(width) } else { None }, height: if height > 0 { Some(height) } else { None },
            frame_rate: None, sample_rate: if sample_rate > 0 { Some(sample_rate) } else { None },
            channels: if channels > 0 { Some(channels) } else { None }, codec_private,
        };
        
        if track_type == TrackType::Video && self.video_track.is_none() { self.video_track = Some(track); }
        else if track_type == TrackType::Audio && self.audio_track.is_none() { self.audio_track = Some(track); }
        
        Ok(())
    }
    
    fn parse_mdia(&mut self, start: usize, len: usize, track_type: &mut TrackType, codec: &mut CodecId, timescale: &mut u32, dur: &mut u64, width: &mut u32, height: &mut u32, sample_rate: &mut u32, channels: &mut u32, codec_private: &mut Vec<u8>) -> DemuxerResult<()> {
        let mut pos = start;
        let end = start + len;
        
        while pos + 8 <= end {
            let size = self.read_u32(pos) as usize;
            if size < 8 { break; }
            let box_type = BoxType([self.data[pos+4], self.data[pos+5], self.data[pos+6], self.data[pos+7]]);
            
            match box_type {
                BoxType::MDHD => {
                    let ver = self.data.get(pos + 8).copied().unwrap_or(0);
                    if ver == 1 { *timescale = self.read_u32(pos + 28); *dur = self.read_u64(pos + 32); }
                    else { *timescale = self.read_u32(pos + 20); *dur = self.read_u32(pos + 24) as u64; }
                }
                BoxType::HDLR => {
                    if pos + 16 <= end {
                        let ht = &self.data[pos+16..pos+20];
                        if ht == b"vide" { *track_type = TrackType::Video; }
                        else if ht == b"soun" { *track_type = TrackType::Audio; }
                        else if ht == b"subt" || ht == b"text" { *track_type = TrackType::Subtitle; }
                    }
                }
                BoxType::MINF => { self.parse_minf(pos + 8, size - 8, codec, width, height, sample_rate, channels, codec_private)?; }
                _ => {}
            }
            pos += size;
        }
        Ok(())
    }
    
    fn parse_minf(&mut self, start: usize, len: usize, codec: &mut CodecId, width: &mut u32, height: &mut u32, sample_rate: &mut u32, channels: &mut u32, codec_private: &mut Vec<u8>) -> DemuxerResult<()> {
        // Simplified - in real impl would parse stbl/stsd fully
        let mut pos = start;
        let end = start + len;
        
        while pos + 8 <= end {
            let size = self.read_u32(pos) as usize;
            if size < 8 { break; }
            let box_type = BoxType([self.data[pos+4], self.data[pos+5], self.data[pos+6], self.data[pos+7]]);
            
            if box_type == BoxType::STBL {
                // Look for codec info in stsd
                let stsd_start = pos + 8;
                let stsd_end = pos + size;
                for i in stsd_start..stsd_end.saturating_sub(8) {
                    if &self.data[i..i+4] == b"avc1" { *codec = CodecId::H264; *width = self.read_u32(i + 32) >> 16; *height = self.read_u32(i + 36) >> 16; }
                    if &self.data[i..i+4] == b"hvc1" || &self.data[i..i+4] == b"hev1" { *codec = CodecId::H265; }
                    if &self.data[i..i+4] == b"vp08" { *codec = CodecId::Vp8; }
                    if &self.data[i..i+4] == b"vp09" { *codec = CodecId::Vp9; }
                    if &self.data[i..i+4] == b"av01" { *codec = CodecId::Av1; }
                    if &self.data[i..i+4] == b"mp4a" { *codec = CodecId::Aac; *sample_rate = 44100; *channels = 2; }
                    if &self.data[i..i+4] == b"Opus" { *codec = CodecId::Opus; }
                }
            }
            pos += size;
        }
        Ok(())
    }
}

impl Demuxer for Mp4Demuxer {
    fn duration(&self) -> Option<Duration> { Some(self.duration) }
    fn video_track(&self) -> Option<&TrackInfo> { self.video_track.as_ref() }
    fn audio_track(&self) -> Option<&TrackInfo> { self.audio_track.as_ref() }
    
    fn read_packet(&mut self) -> DemuxerResult<Packet> {
        if self.current_sample >= self.samples.len() { self.eof = true; return Err(DemuxerError::EndOfStream); }
        let s = &self.samples[self.current_sample];
        let offset = s.offset as usize;
        let size = s.size as usize;
        if offset + size > self.data.len() { return Err(DemuxerError::IoError("Out of bounds".into())); }
        let packet = Packet { track_id: s.track_id, pts: s.pts, dts: s.dts, duration: s.duration, is_key: s.is_key, data: self.data[offset..offset+size].to_vec() };
        self.current_sample += 1;
        Ok(packet)
    }
    
    fn seek(&mut self, position: Duration) -> DemuxerResult<()> {
        for (i, s) in self.samples.iter().enumerate() {
            if s.is_key && s.pts >= position { self.current_sample = i; return Ok(()); }
        }
        self.current_sample = 0;
        Ok(())
    }
    
    fn is_eof(&self) -> bool { self.eof }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_box_type() { assert_eq!(BoxType::FTYP.0, *b"ftyp"); }
}
