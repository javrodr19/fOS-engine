//! fOS Media
//!
//! Media APIs for the fOS browser engine.
//!
//! Features:
//! - HTMLVideoElement, HTMLAudioElement
//! - Media Source Extensions
//! - Web Audio API (with spatial audio)
//! - WebRTC (with data channels, ICE, STUN)
//! - Media codecs (H.264, H.265, VP8/VP9, AV1, AAC, Opus, Vorbis)
//! - Encrypted Media Extensions (EME)
//! - Container formats (MP4, WebM, MKV, MPEG-TS, fMP4)
//! - Streaming protocols (HLS, DASH, ABR)
//! - SIMD optimizations

pub mod element;
pub mod tracks;
pub mod mse;
pub mod fullscreen;
pub mod audio;
pub mod webrtc;
pub mod codecs;
pub mod eme;
pub mod buffer_pool;
pub mod decoders;
pub mod containers;
pub mod pipeline;
pub mod streaming;
pub mod simd;

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
    ice::{IceAgent, IceCandidate, IceState},
    stun::StunMessage,
    sdp::SessionDescription,
};
pub use codecs::{CodecType, CodecRegistry, VideoDecoder, AudioDecoder, CodecConfig};
pub use eme::{KeySystem, MediaKeys, MediaKeySession, ClearKey};
pub use decoders::{VideoFrame, AudioSamples, EncodedPacket, VideoDecoderTrait, AudioDecoderTrait};
pub use containers::{Demuxer, TrackInfo, Packet, CodecId, ContainerFormat, detect_format};
pub use pipeline::{MediaPipeline, PipelineState};
pub use streaming::{Manifest, Variant, Segment, QualityLevel};

/// Media error
#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("Not supported: {0}")]
    NotSupported(String),
    
    #[error("Invalid state: {0}")]
    InvalidState(String),
}
