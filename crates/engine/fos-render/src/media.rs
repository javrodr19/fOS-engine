//! Media Elements
//!
//! Video and audio element support.

use std::time::Duration;

/// Media playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    /// Not started or reset
    Idle,
    /// Loading media
    Loading,
    /// Ready to play
    Ready,
    /// Currently playing
    Playing,
    /// Paused
    Paused,
    /// Playback ended
    Ended,
    /// Error occurred
    Error,
}

impl Default for PlaybackState {
    fn default() -> Self {
        PlaybackState::Idle
    }
}

/// Media ready state (HTML5 spec)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReadyState {
    /// No data
    HaveNothing = 0,
    /// Metadata loaded
    HaveMetadata = 1,
    /// Current frame data available
    HaveCurrentData = 2,
    /// Future data available
    HaveFutureData = 3,
    /// Enough data to play through
    HaveEnoughData = 4,
}

impl Default for ReadyState {
    fn default() -> Self {
        ReadyState::HaveNothing
    }
}

/// Media element properties
#[derive(Debug, Clone)]
pub struct MediaProperties {
    /// Source URL
    pub src: String,
    /// Duration in seconds
    pub duration: f64,
    /// Current playback position in seconds
    pub current_time: f64,
    /// Volume (0.0 to 1.0)
    pub volume: f64,
    /// Muted state
    pub muted: bool,
    /// Playback rate (1.0 = normal)
    pub playback_rate: f64,
    /// Loop playback
    pub loop_playback: bool,
    /// Autoplay
    pub autoplay: bool,
    /// Width (for video)
    pub width: u32,
    /// Height (for video)
    pub height: u32,
}

impl Default for MediaProperties {
    fn default() -> Self {
        Self {
            src: String::new(),
            duration: 0.0,
            current_time: 0.0,
            volume: 1.0,
            muted: false,
            playback_rate: 1.0,
            loop_playback: false,
            autoplay: false,
            width: 0,
            height: 0,
        }
    }
}

/// Media element
#[derive(Debug, Default)]
pub struct MediaElement {
    /// Element type
    pub element_type: MediaType,
    /// Playback state
    pub state: PlaybackState,
    /// Ready state
    pub ready_state: ReadyState,
    /// Properties
    pub properties: MediaProperties,
    /// Buffered ranges (start, end) in seconds
    pub buffered: Vec<(f64, f64)>,
    /// Error message if any
    pub error: Option<String>,
}

/// Media element type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaType {
    #[default]
    Video,
    Audio,
}

impl MediaElement {
    /// Create a new video element
    pub fn video() -> Self {
        Self {
            element_type: MediaType::Video,
            ..Default::default()
        }
    }
    
    /// Create a new audio element
    pub fn audio() -> Self {
        Self {
            element_type: MediaType::Audio,
            ..Default::default()
        }
    }
    
    /// Set the source URL
    pub fn set_src(&mut self, src: &str) {
        self.properties.src = src.to_string();
        self.state = PlaybackState::Loading;
        self.ready_state = ReadyState::HaveNothing;
    }
    
    /// Play the media
    pub fn play(&mut self) -> Result<(), &'static str> {
        match self.state {
            PlaybackState::Ready | PlaybackState::Paused => {
                self.state = PlaybackState::Playing;
                Ok(())
            }
            PlaybackState::Playing => Ok(()), // Already playing
            PlaybackState::Ended => {
                // Restart from beginning
                self.properties.current_time = 0.0;
                self.state = PlaybackState::Playing;
                Ok(())
            }
            PlaybackState::Loading => Err("Media is still loading"),
            PlaybackState::Error => Err("Media has an error"),
            PlaybackState::Idle => Err("No media source set"),
        }
    }
    
    /// Pause the media
    pub fn pause(&mut self) {
        if self.state == PlaybackState::Playing {
            self.state = PlaybackState::Paused;
        }
    }
    
    /// Seek to a position
    pub fn seek(&mut self, time: f64) {
        self.properties.current_time = time.max(0.0).min(self.properties.duration);
    }
    
    /// Set volume (0.0 to 1.0)
    pub fn set_volume(&mut self, volume: f64) {
        self.properties.volume = volume.max(0.0).min(1.0);
    }
    
    /// Toggle mute
    pub fn set_muted(&mut self, muted: bool) {
        self.properties.muted = muted;
    }
    
    /// Set playback rate
    pub fn set_playback_rate(&mut self, rate: f64) {
        self.properties.playback_rate = rate.max(0.1).min(16.0);
    }
    
    /// Called when metadata is loaded
    pub fn on_metadata_loaded(&mut self, duration: f64, width: u32, height: u32) {
        self.properties.duration = duration;
        self.properties.width = width;
        self.properties.height = height;
        self.ready_state = ReadyState::HaveMetadata;
        if self.state == PlaybackState::Loading {
            self.state = PlaybackState::Ready;
        }
    }
    
    /// Called when enough data is buffered
    pub fn on_can_play(&mut self) {
        self.ready_state = ReadyState::HaveEnoughData;
        if self.state == PlaybackState::Loading {
            self.state = PlaybackState::Ready;
        }
    }
    
    /// Called on playback error
    pub fn on_error(&mut self, error: String) {
        self.state = PlaybackState::Error;
        self.error = Some(error);
    }
    
    /// Update current time (called during playback)
    pub fn update_time(&mut self, delta: f64) {
        if self.state == PlaybackState::Playing {
            self.properties.current_time += delta * self.properties.playback_rate;
            
            if self.properties.current_time >= self.properties.duration {
                if self.properties.loop_playback {
                    self.properties.current_time = 0.0;
                } else {
                    self.properties.current_time = self.properties.duration;
                    self.state = PlaybackState::Ended;
                }
            }
        }
    }
    
    /// Check if media is playing
    pub fn is_playing(&self) -> bool {
        self.state == PlaybackState::Playing
    }
    
    /// Check if media is paused
    pub fn is_paused(&self) -> bool {
        self.state == PlaybackState::Paused
    }
    
    /// Check if media has ended
    pub fn is_ended(&self) -> bool {
        self.state == PlaybackState::Ended
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_video_element() {
        let video = MediaElement::video();
        assert_eq!(video.element_type, MediaType::Video);
        assert_eq!(video.state, PlaybackState::Idle);
    }
    
    #[test]
    fn test_audio_element() {
        let audio = MediaElement::audio();
        assert_eq!(audio.element_type, MediaType::Audio);
    }
    
    #[test]
    fn test_set_src() {
        let mut video = MediaElement::video();
        video.set_src("movie.mp4");
        
        assert_eq!(video.properties.src, "movie.mp4");
        assert_eq!(video.state, PlaybackState::Loading);
    }
    
    #[test]
    fn test_play_pause() {
        let mut video = MediaElement::video();
        video.set_src("movie.mp4");
        video.on_can_play();
        
        video.play().unwrap();
        assert!(video.is_playing());
        
        video.pause();
        assert!(video.is_paused());
    }
    
    #[test]
    fn test_seek() {
        let mut video = MediaElement::video();
        video.on_metadata_loaded(100.0, 1920, 1080);
        
        video.seek(50.0);
        assert_eq!(video.properties.current_time, 50.0);
        
        // Clamp to bounds
        video.seek(200.0);
        assert_eq!(video.properties.current_time, 100.0);
        
        video.seek(-10.0);
        assert_eq!(video.properties.current_time, 0.0);
    }
    
    #[test]
    fn test_volume() {
        let mut video = MediaElement::video();
        
        video.set_volume(0.5);
        assert_eq!(video.properties.volume, 0.5);
        
        video.set_volume(2.0);
        assert_eq!(video.properties.volume, 1.0);
        
        video.set_volume(-1.0);
        assert_eq!(video.properties.volume, 0.0);
    }
    
    #[test]
    fn test_playback_rate() {
        let mut video = MediaElement::video();
        
        video.set_playback_rate(2.0);
        assert_eq!(video.properties.playback_rate, 2.0);
        
        video.set_playback_rate(0.01);
        assert_eq!(video.properties.playback_rate, 0.1);
    }
    
    #[test]
    fn test_update_time() {
        let mut video = MediaElement::video();
        video.set_src("movie.mp4");
        video.on_metadata_loaded(10.0, 1920, 1080);
        video.play().unwrap();
        
        video.update_time(1.0);
        assert_eq!(video.properties.current_time, 1.0);
        
        video.update_time(1.0);
        assert_eq!(video.properties.current_time, 2.0);
    }
    
    #[test]
    fn test_playback_end() {
        let mut video = MediaElement::video();
        video.set_src("movie.mp4");
        video.on_metadata_loaded(5.0, 1920, 1080);
        video.play().unwrap();
        
        video.update_time(6.0);
        assert!(video.is_ended());
        assert_eq!(video.properties.current_time, 5.0);
    }
    
    #[test]
    fn test_loop_playback() {
        let mut video = MediaElement::video();
        video.set_src("movie.mp4");
        video.on_metadata_loaded(5.0, 1920, 1080);
        video.properties.loop_playback = true;
        video.play().unwrap();
        
        video.update_time(6.0);
        assert!(video.is_playing());
        assert_eq!(video.properties.current_time, 0.0);
    }
    
    #[test]
    fn test_mute() {
        let mut video = MediaElement::video();
        
        assert!(!video.properties.muted);
        video.set_muted(true);
        assert!(video.properties.muted);
    }
}
