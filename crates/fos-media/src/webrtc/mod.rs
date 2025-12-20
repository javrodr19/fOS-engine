//! WebRTC Module
//!
//! Real-time communication.

pub mod connection;
pub mod datachannel;
pub mod screen;

pub use connection::{
    RTCPeerConnection, RTCPeerConnectionState, RTCConfiguration,
    RTCSessionDescription, RTCSdpType, RTCIceCandidate,
    MediaStream, MediaStreamTrack, MediaStreamTrackKind, MediaStreamTrackState,
};
pub use datachannel::{RTCDataChannel, RTCDataChannelState, RTCDataChannelInit};
pub use screen::{ScreenCapture, DisplayMediaStreamOptions};
