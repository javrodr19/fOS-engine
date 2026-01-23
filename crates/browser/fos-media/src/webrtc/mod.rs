//! WebRTC Module
//!
//! Real-time communication with full ICE, DTLS-SRTP, and Simulcast support.

pub mod connection;
pub mod datachannel;
pub mod screen;
pub mod ice;
pub mod stun;
pub mod turn;
pub mod sdp;
pub mod dtls;
pub mod srtp;
pub mod simulcast;

pub use connection::{
    RTCPeerConnection, RTCPeerConnectionState, RTCConfiguration,
    RTCSessionDescription, RTCSdpType, RTCIceCandidate,
    MediaStream, MediaStreamTrack, MediaStreamTrackKind, MediaStreamTrackState,
};
pub use datachannel::{RTCDataChannel, RTCDataChannelState, RTCDataChannelInit};
pub use screen::{ScreenCapture, DisplayMediaStreamOptions};
pub use ice::{IceAgent, IceCandidate, IceState};
pub use stun::StunMessage;
pub use turn::TurnClient;
pub use sdp::SessionDescription;
pub use dtls::{DtlsTransport, DtlsState};
pub use srtp::SrtpSession;
pub use simulcast::{SimulcastConfig, SimulcastLayer, Rid};
