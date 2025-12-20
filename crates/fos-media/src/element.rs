//! Media Elements
//!
//! HTMLVideoElement and HTMLAudioElement.

use std::time::Duration;

/// Network state
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NetworkState {
    #[default]
    Empty = 0,
    Idle = 1,
    Loading = 2,
    NoSource = 3,
}

/// Ready state
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReadyState {
    #[default]
    HaveNothing = 0,
    HaveMetadata = 1,
    HaveCurrentData = 2,
    HaveFutureData = 3,
    HaveEnoughData = 4,
}

/// Media error
#[derive(Debug, Clone)]
pub struct MediaError {
    pub code: MediaErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaErrorCode {
    Aborted = 1,
    Network = 2,
    Decode = 3,
    SrcNotSupported = 4,
}

/// Time ranges
#[derive(Debug, Clone, Default)]
pub struct TimeRanges {
    ranges: Vec<(f64, f64)>,
}

impl TimeRanges {
    pub fn new() -> Self { Self::default() }
    
    pub fn add(&mut self, start: f64, end: f64) {
        self.ranges.push((start, end));
    }
    
    pub fn length(&self) -> usize {
        self.ranges.len()
    }
    
    pub fn start(&self, index: usize) -> Option<f64> {
        self.ranges.get(index).map(|(s, _)| *s)
    }
    
    pub fn end(&self, index: usize) -> Option<f64> {
        self.ranges.get(index).map(|(_, e)| *e)
    }
}

/// Base media element (shared between video/audio)
#[derive(Debug)]
pub struct HTMLMediaElement {
    // Source
    pub src: String,
    pub current_src: String,
    pub cross_origin: Option<String>,
    
    // State
    pub network_state: NetworkState,
    pub ready_state: ReadyState,
    pub error: Option<MediaError>,
    
    // Playback
    pub current_time: f64,
    pub duration: f64,
    pub paused: bool,
    pub ended: bool,
    pub seeking: bool,
    pub autoplay: bool,
    pub loop_: bool,
    
    // Volume
    pub volume: f64,
    pub muted: bool,
    pub default_muted: bool,
    
    // Buffering
    pub buffered: TimeRanges,
    pub seekable: TimeRanges,
    pub played: TimeRanges,
    
    // Playback rate
    pub playback_rate: f64,
    pub default_playback_rate: f64,
    
    // Controls
    pub controls: bool,
    pub preload: PreloadHint,
}

/// Preload hint
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PreloadHint {
    None,
    #[default]
    Metadata,
    Auto,
}

impl HTMLMediaElement {
    pub fn new() -> Self {
        Self {
            src: String::new(),
            current_src: String::new(),
            cross_origin: None,
            network_state: NetworkState::Empty,
            ready_state: ReadyState::HaveNothing,
            error: None,
            current_time: 0.0,
            duration: f64::NAN,
            paused: true,
            ended: false,
            seeking: false,
            autoplay: false,
            loop_: false,
            volume: 1.0,
            muted: false,
            default_muted: false,
            buffered: TimeRanges::new(),
            seekable: TimeRanges::new(),
            played: TimeRanges::new(),
            playback_rate: 1.0,
            default_playback_rate: 1.0,
            controls: false,
            preload: PreloadHint::Metadata,
        }
    }
    
    /// Play media
    pub fn play(&mut self) -> Result<(), MediaError> {
        if self.ready_state < ReadyState::HaveFutureData {
            return Err(MediaError {
                code: MediaErrorCode::Aborted,
                message: "Not enough data".into(),
            });
        }
        self.paused = false;
        self.ended = false;
        Ok(())
    }
    
    /// Pause media
    pub fn pause(&mut self) {
        self.paused = true;
    }
    
    /// Load media
    pub fn load(&mut self) {
        self.network_state = NetworkState::Loading;
        self.ready_state = ReadyState::HaveNothing;
        self.current_time = 0.0;
        self.paused = true;
        self.ended = false;
    }
    
    /// Seek to time
    pub fn seek(&mut self, time: f64) {
        self.seeking = true;
        self.current_time = time.clamp(0.0, self.duration);
        self.seeking = false;
    }
    
    /// Check if can play type
    pub fn can_play_type(&self, mime_type: &str) -> CanPlayType {
        match mime_type {
            "video/mp4" | "video/webm" | "audio/mp3" | "audio/mpeg" 
            | "audio/ogg" | "audio/wav" => CanPlayType::Probably,
            "video/ogg" | "audio/aac" => CanPlayType::Maybe,
            _ => CanPlayType::Empty,
        }
    }
    
    /// Fast seek
    pub fn fast_seek(&mut self, time: f64) {
        self.current_time = time.clamp(0.0, self.duration);
    }
}

/// Can play type result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanPlayType {
    Empty,
    Maybe,
    Probably,
}

impl Default for HTMLMediaElement {
    fn default() -> Self {
        Self::new()
    }
}

/// HTML Video Element
#[derive(Debug, Default)]
pub struct HTMLVideoElement {
    pub base: HTMLMediaElement,
    pub width: u32,
    pub height: u32,
    pub video_width: u32,
    pub video_height: u32,
    pub poster: String,
    pub plays_inline: bool,
}

impl HTMLVideoElement {
    pub fn new() -> Self {
        Self {
            base: HTMLMediaElement::new(),
            width: 0,
            height: 0,
            video_width: 0,
            video_height: 0,
            poster: String::new(),
            plays_inline: false,
        }
    }
    
    /// Request Picture-in-Picture
    pub fn request_picture_in_picture(&self) -> Result<(), &'static str> {
        // Would request PiP
        Ok(())
    }
    
    /// Get video playback quality
    pub fn get_video_playback_quality(&self) -> VideoPlaybackQuality {
        VideoPlaybackQuality::default()
    }
}

/// Video playback quality
#[derive(Debug, Clone, Default)]
pub struct VideoPlaybackQuality {
    pub creation_time: f64,
    pub total_video_frames: u32,
    pub dropped_video_frames: u32,
    pub corrupted_video_frames: u32,
}

/// HTML Audio Element
#[derive(Debug, Default)]
pub struct HTMLAudioElement {
    pub base: HTMLMediaElement,
}

impl HTMLAudioElement {
    pub fn new() -> Self {
        Self {
            base: HTMLMediaElement::new(),
        }
    }
    
    /// Create from URL
    pub fn from_url(src: &str) -> Self {
        let mut audio = Self::new();
        audio.base.src = src.to_string();
        audio
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_video_element() {
        let video = HTMLVideoElement::new();
        assert!(video.base.paused);
        assert_eq!(video.base.volume, 1.0);
    }
    
    #[test]
    fn test_audio_element() {
        let audio = HTMLAudioElement::from_url("test.mp3");
        assert_eq!(audio.base.src, "test.mp3");
    }
    
    #[test]
    fn test_can_play_type() {
        let media = HTMLMediaElement::new();
        assert_eq!(media.can_play_type("video/mp4"), CanPlayType::Probably);
        assert_eq!(media.can_play_type("video/unknown"), CanPlayType::Empty);
    }
}
