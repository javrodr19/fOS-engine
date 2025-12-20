//! fOS Media
//!
//! Media APIs for the fOS browser engine.
//!
//! Features:
//! - HTMLVideoElement, HTMLAudioElement
//! - Media Source Extensions
//! - Web Audio API (with spatial audio)
//! - WebRTC (with data channels)

pub mod element;
pub mod tracks;
pub mod mse;
pub mod fullscreen;
pub mod audio;
pub mod webrtc;

pub use element::{
    HTMLVideoElement, HTMLAudioElement, HTMLMediaElement,
    NetworkState, ReadyState, CanPlayType, TimeRanges,
};
pub use tracks::{TextTrack, AudioTrack, VideoTrack, TextTrackKind, TextTrackMode};
pub use mse::{MediaSource, SourceBuffer, MediaSourceReadyState};
pub use fullscreen::{FullscreenManager, PipManager};
pub use audio::{
    AudioContext, AudioContextState, OscillatorNode, GainNode, AudioBuffer,
    PannerNode, StereoPannerNode, AudioWorkletNode,
};
pub use webrtc::{
    RTCPeerConnection, MediaStream, MediaStreamTrack,
    RTCDataChannel, ScreenCapture,
};

/// Media error
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("Not supported: {0}")]
    NotSupported(String),
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
}
