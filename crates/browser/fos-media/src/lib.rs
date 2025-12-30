//! fOS Media
//!
//! Media APIs for the fOS browser engine.
//!
//! Features:
//! - HTMLVideoElement, HTMLAudioElement
//! - Media Source Extensions
//! - Web Audio API (with spatial audio)
//! - WebRTC (with data channels)
//! - Media codecs (H.264, H.265, VP8/VP9, AV1, AAC, MP3, Opus)
//! - Encrypted Media Extensions (EME)

pub mod element;
pub mod tracks;
pub mod mse;
pub mod fullscreen;
pub mod audio;
pub mod webrtc;
pub mod codecs;
pub mod eme;

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
pub use codecs::{CodecType, CodecRegistry, VideoDecoder, AudioDecoder, CodecConfig};
pub use eme::{KeySystem, MediaKeys, MediaKeySession, ClearKey};

/// Media error
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("Not supported: {0}")]
    NotSupported(String),
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
}
