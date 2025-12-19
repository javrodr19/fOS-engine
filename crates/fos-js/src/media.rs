//! Media types for fos-js
//!
//! Media element types for use in bindings.

/// Media element with all properties
#[derive(Debug, Default)]
pub struct MediaElement {
    pub element_type: MediaType,
    pub state: PlaybackState,
    pub ready_state: ReadyState,
    pub properties: MediaProperties,
}

/// Media element type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaType {
    #[default]
    Video,
    Audio,
}

/// Playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackState {
    #[default]
    Idle,
    Loading,
    Ready,
    Playing,
    Paused,
    Ended,
    Error,
}

/// Ready state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadyState {
    #[default]
    HaveNothing,
    HaveMetadata,
    HaveCurrentData,
    HaveFutureData,
    HaveEnoughData,
}

/// Media properties
#[derive(Debug, Clone, Default)]
pub struct MediaProperties {
    pub src: String,
    pub duration: f64,
    pub current_time: f64,
    pub volume: f64,
    pub muted: bool,
    pub playback_rate: f64,
    pub loop_playback: bool,
    pub autoplay: bool,
    pub width: u32,
    pub height: u32,
}

impl MediaElement {
    pub fn video() -> Self {
        Self {
            element_type: MediaType::Video,
            properties: MediaProperties {
                volume: 1.0,
                playback_rate: 1.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    
    pub fn audio() -> Self {
        Self {
            element_type: MediaType::Audio,
            properties: MediaProperties {
                volume: 1.0,
                playback_rate: 1.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }
    
    pub fn set_src(&mut self, src: &str) {
        self.properties.src = src.to_string();
        self.state = PlaybackState::Loading;
        self.ready_state = ReadyState::HaveNothing;
    }
    
    pub fn play(&mut self) -> Result<(), &'static str> {
        match self.state {
            PlaybackState::Ready | PlaybackState::Paused => {
                self.state = PlaybackState::Playing;
                Ok(())
            }
            PlaybackState::Playing => Ok(()),
            PlaybackState::Ended => {
                self.properties.current_time = 0.0;
                self.state = PlaybackState::Playing;
                Ok(())
            }
            _ => Err("Cannot play"),
        }
    }
    
    pub fn pause(&mut self) {
        if self.state == PlaybackState::Playing {
            self.state = PlaybackState::Paused;
        }
    }
    
    pub fn seek(&mut self, time: f64) {
        self.properties.current_time = time.max(0.0).min(self.properties.duration);
    }
    
    pub fn set_volume(&mut self, volume: f64) {
        self.properties.volume = volume.max(0.0).min(1.0);
    }
    
    pub fn on_metadata_loaded(&mut self, duration: f64, width: u32, height: u32) {
        self.properties.duration = duration;
        self.properties.width = width;
        self.properties.height = height;
        self.ready_state = ReadyState::HaveMetadata;
        if self.state == PlaybackState::Loading {
            self.state = PlaybackState::Ready;
        }
    }
    
    pub fn on_can_play(&mut self) {
        self.ready_state = ReadyState::HaveEnoughData;
        if self.state == PlaybackState::Loading {
            self.state = PlaybackState::Ready;
        }
    }
    
    pub fn is_playing(&self) -> bool {
        self.state == PlaybackState::Playing
    }
    
    pub fn is_paused(&self) -> bool {
        self.state == PlaybackState::Paused
    }
}
