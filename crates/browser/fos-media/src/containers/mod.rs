//! Container Parsers (Demuxers)
//!
//! Parsers for media container formats: MP4, WebM, MKV, MPEG-TS, fMP4.

pub mod mp4;
pub mod webm;
pub mod mkv;
pub mod ts;
pub mod fmp4;

use std::time::Duration;
use crate::decoders::EncodedPacket;

/// Demuxer trait
pub trait Demuxer: Send {
    /// Get duration of media
    fn duration(&self) -> Option<Duration>;
    
    /// Get video track info
    fn video_track(&self) -> Option<&TrackInfo>;
    
    /// Get audio track info
    fn audio_track(&self) -> Option<&TrackInfo>;
    
    /// Read next packet
    fn read_packet(&mut self) -> DemuxerResult<Packet>;
    
    /// Seek to position
    fn seek(&mut self, position: Duration) -> DemuxerResult<()>;
    
    /// Check if at end of stream
    fn is_eof(&self) -> bool;
}

/// Track information
#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub track_id: u32,
    pub track_type: TrackType,
    pub codec: CodecId,
    pub duration: Duration,
    pub timescale: u32,
    /// For video
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<f64>,
    /// For audio  
    pub sample_rate: Option<u32>,
    pub channels: Option<u32>,
    /// Codec-specific data (SPS/PPS, AudioSpecificConfig, etc.)
    pub codec_private: Vec<u8>,
}

/// Track type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackType { Video, Audio, Subtitle, Data }

/// Codec identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecId {
    H264, H265, Vp8, Vp9, Av1,
    Aac, Mp3, Opus, Vorbis, Flac, Pcm,
    WebVtt, Subrip,
    Unknown,
}

/// Demuxed packet
#[derive(Debug)]
pub struct Packet {
    pub track_id: u32,
    pub pts: Duration,
    pub dts: Duration,
    pub duration: Duration,
    pub is_key: bool,
    pub data: Vec<u8>,
}

impl Packet {
    pub fn to_encoded_packet(&self) -> EncodedPacket {
        EncodedPacket { data: self.data.clone(), pts: self.pts, dts: self.dts, is_key: self.is_key }
    }
}

/// Demuxer result type
pub type DemuxerResult<T> = Result<T, DemuxerError>;

/// Demuxer error
#[derive(Debug, Clone, thiserror::Error)]
pub enum DemuxerError {
    #[error("Invalid container: {0}")]
    InvalidContainer(String),
    #[error("Unsupported feature: {0}")]
    Unsupported(String),
    #[error("Need more data")]
    NeedMoreData,
    #[error("End of stream")]
    EndOfStream,
    #[error("IO error: {0}")]
    IoError(String),
}

/// Container format detection
pub fn detect_format(data: &[u8]) -> Option<ContainerFormat> {
    if data.len() < 12 { return None; }
    
    // MP4/MOV: ftyp box or moov box
    if &data[4..8] == b"ftyp" || &data[4..8] == b"moov" || &data[4..8] == b"mdat" {
        return Some(ContainerFormat::Mp4);
    }
    
    // WebM/MKV: EBML header
    if data[0..4] == [0x1A, 0x45, 0xDF, 0xA3] {
        // Check DocType for webm vs matroska
        if data.len() > 30 {
            let s = String::from_utf8_lossy(&data[0..40]);
            if s.contains("webm") { return Some(ContainerFormat::WebM); }
            if s.contains("matroska") { return Some(ContainerFormat::Mkv); }
        }
        return Some(ContainerFormat::Mkv);
    }
    
    // MPEG-TS: sync byte 0x47 every 188 bytes
    if data[0] == 0x47 && (data.len() < 188 || data[188] == 0x47) {
        return Some(ContainerFormat::MpegTs);
    }
    
    None
}

/// Container format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerFormat { Mp4, WebM, Mkv, MpegTs, FragmentedMp4 }

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_mp4() {
        let data = [0, 0, 0, 20, b'f', b't', b'y', b'p', b'm', b'p', b'4', b'2'];
        assert_eq!(detect_format(&data), Some(ContainerFormat::Mp4));
    }
    
    #[test]    
    fn test_detect_ebml() {
        let data = [0x1A, 0x45, 0xDF, 0xA3, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(detect_format(&data), Some(ContainerFormat::Mkv));
    }
}
