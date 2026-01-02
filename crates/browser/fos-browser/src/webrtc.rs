//! WebRTC API
//!
//! Real-Time Communications for peer-to-peer audio, video, and data.
//!
//! ## Core Components
//! - RTCPeerConnection: Main peer connection
//! - RTCDataChannel: Arbitrary data transfer
//! - RTCSessionDescription: SDP offer/answer
//! - RTCIceCandidate: ICE candidates for connectivity

use std::collections::HashMap;

/// RTC Peer Connection State
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RTCPeerConnectionState {
    #[default]
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

/// ICE Connection State
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RTCIceConnectionState {
    #[default]
    New,
    Checking,
    Connected,
    Completed,
    Disconnected,
    Failed,
    Closed,
}

/// ICE Gathering State
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RTCIceGatheringState {
    #[default]
    New,
    Gathering,
    Complete,
}

/// Signaling State
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RTCSignalingState {
    #[default]
    Stable,
    HaveLocalOffer,
    HaveRemoteOffer,
    HaveLocalPranswer,
    HaveRemotePranswer,
    Closed,
}

/// RTC Configuration
#[derive(Debug, Clone, Default)]
pub struct RTCConfiguration {
    pub ice_servers: Vec<RTCIceServer>,
    pub ice_transport_policy: IceTransportPolicy,
    pub bundle_policy: BundlePolicy,
    pub rtcp_mux_policy: RtcpMuxPolicy,
}

/// ICE Server configuration
#[derive(Debug, Clone)]
pub struct RTCIceServer {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IceTransportPolicy {
    Relay,
    #[default]
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BundlePolicy {
    Balanced,
    #[default]
    MaxCompat,
    MaxBundle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RtcpMuxPolicy {
    #[default]
    Require,
}

/// RTC Peer Connection
#[derive(Debug)]
pub struct RTCPeerConnection {
    pub id: u64,
    pub configuration: RTCConfiguration,
    pub connection_state: RTCPeerConnectionState,
    pub ice_connection_state: RTCIceConnectionState,
    pub ice_gathering_state: RTCIceGatheringState,
    pub signaling_state: RTCSignalingState,
    pub local_description: Option<RTCSessionDescription>,
    pub remote_description: Option<RTCSessionDescription>,
    pub pending_local_description: Option<RTCSessionDescription>,
    pub pending_remote_description: Option<RTCSessionDescription>,
    data_channels: HashMap<u64, RTCDataChannel>,
    senders: Vec<RTCRtpSender>,
    receivers: Vec<RTCRtpReceiver>,
    ice_candidates: Vec<RTCIceCandidate>,
    next_channel_id: u64,
}

static mut NEXT_PC_ID: u64 = 1;

impl RTCPeerConnection {
    pub fn new(configuration: RTCConfiguration) -> Self {
        let id = unsafe {
            let id = NEXT_PC_ID;
            NEXT_PC_ID += 1;
            id
        };
        Self {
            id,
            configuration,
            connection_state: RTCPeerConnectionState::New,
            ice_connection_state: RTCIceConnectionState::New,
            ice_gathering_state: RTCIceGatheringState::New,
            signaling_state: RTCSignalingState::Stable,
            local_description: None,
            remote_description: None,
            pending_local_description: None,
            pending_remote_description: None,
            data_channels: HashMap::new(),
            senders: Vec::new(),
            receivers: Vec::new(),
            ice_candidates: Vec::new(),
            next_channel_id: 1,
        }
    }

    /// Create an offer SDP
    pub fn create_offer(&mut self) -> Result<RTCSessionDescription, RTCError> {
        if self.signaling_state == RTCSignalingState::Closed {
            return Err(RTCError::InvalidState);
        }

        Ok(RTCSessionDescription {
            sdp_type: RTCSdpType::Offer,
            sdp: self.generate_sdp(),
        })
    }

    /// Create an answer SDP
    pub fn create_answer(&mut self) -> Result<RTCSessionDescription, RTCError> {
        if self.signaling_state != RTCSignalingState::HaveRemoteOffer {
            return Err(RTCError::InvalidState);
        }

        Ok(RTCSessionDescription {
            sdp_type: RTCSdpType::Answer,
            sdp: self.generate_sdp(),
        })
    }

    /// Set local description
    pub fn set_local_description(&mut self, desc: RTCSessionDescription) -> Result<(), RTCError> {
        match desc.sdp_type {
            RTCSdpType::Offer => {
                self.signaling_state = RTCSignalingState::HaveLocalOffer;
            }
            RTCSdpType::Answer | RTCSdpType::Pranswer => {
                self.signaling_state = RTCSignalingState::Stable;
            }
            RTCSdpType::Rollback => {
                self.signaling_state = RTCSignalingState::Stable;
                return Ok(());
            }
        }
        self.local_description = Some(desc);
        self.ice_gathering_state = RTCIceGatheringState::Gathering;
        Ok(())
    }

    /// Set remote description
    pub fn set_remote_description(&mut self, desc: RTCSessionDescription) -> Result<(), RTCError> {
        match desc.sdp_type {
            RTCSdpType::Offer => {
                self.signaling_state = RTCSignalingState::HaveRemoteOffer;
            }
            RTCSdpType::Answer | RTCSdpType::Pranswer => {
                self.signaling_state = RTCSignalingState::Stable;
                self.connection_state = RTCPeerConnectionState::Connecting;
            }
            RTCSdpType::Rollback => {
                return Ok(());
            }
        }
        self.remote_description = Some(desc);
        Ok(())
    }

    /// Add ICE candidate
    pub fn add_ice_candidate(&mut self, candidate: RTCIceCandidate) -> Result<(), RTCError> {
        if self.remote_description.is_none() {
            return Err(RTCError::InvalidState);
        }
        self.ice_candidates.push(candidate);
        Ok(())
    }

    /// Create data channel
    pub fn create_data_channel(&mut self, label: &str, options: Option<RTCDataChannelInit>) -> &RTCDataChannel {
        let id = self.next_channel_id;
        self.next_channel_id += 1;

        let channel = RTCDataChannel {
            id,
            label: label.to_string(),
            ordered: options.as_ref().map(|o| o.ordered).unwrap_or(true),
            max_packet_life_time: options.as_ref().and_then(|o| o.max_packet_life_time),
            max_retransmits: options.as_ref().and_then(|o| o.max_retransmits),
            protocol: options.as_ref().map(|o| o.protocol.clone()).unwrap_or_default(),
            negotiated: options.as_ref().map(|o| o.negotiated).unwrap_or(false),
            ready_state: RTCDataChannelState::Connecting,
            buffered_amount: 0,
            buffered_amount_low_threshold: 0,
            binary_type: BinaryType::ArrayBuffer,
            messages: Vec::new(),
        };

        self.data_channels.insert(id, channel);
        self.data_channels.get(&id).unwrap()
    }

    /// Get senders
    pub fn get_senders(&self) -> &[RTCRtpSender] {
        &self.senders
    }

    /// Get receivers
    pub fn get_receivers(&self) -> &[RTCRtpReceiver] {
        &self.receivers
    }

    /// Close connection
    pub fn close(&mut self) {
        self.connection_state = RTCPeerConnectionState::Closed;
        self.ice_connection_state = RTCIceConnectionState::Closed;
        self.signaling_state = RTCSignalingState::Closed;

        for channel in self.data_channels.values_mut() {
            channel.ready_state = RTCDataChannelState::Closed;
        }
    }

    fn generate_sdp(&self) -> String {
        // Generate minimal SDP
        format!(
            "v=0\r\n\
             o=- {} 1 IN IP4 0.0.0.0\r\n\
             s=-\r\n\
             t=0 0\r\n",
            self.id
        )
    }
}

/// Session description
#[derive(Debug, Clone)]
pub struct RTCSessionDescription {
    pub sdp_type: RTCSdpType,
    pub sdp: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RTCSdpType {
    Offer,
    Answer,
    Pranswer,
    Rollback,
}

/// ICE Candidate
#[derive(Debug, Clone)]
pub struct RTCIceCandidate {
    pub candidate: String,
    pub sdp_mid: Option<String>,
    pub sdp_m_line_index: Option<u16>,
    pub username_fragment: Option<String>,
}

/// Data channel initialization options
#[derive(Debug, Clone, Default)]
pub struct RTCDataChannelInit {
    pub ordered: bool,
    pub max_packet_life_time: Option<u16>,
    pub max_retransmits: Option<u16>,
    pub protocol: String,
    pub negotiated: bool,
    pub id: Option<u16>,
}

/// Data channel state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RTCDataChannelState {
    #[default]
    Connecting,
    Open,
    Closing,
    Closed,
}

/// Binary type for data channels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BinaryType {
    Blob,
    #[default]
    ArrayBuffer,
}

/// RTC Data Channel
#[derive(Debug)]
pub struct RTCDataChannel {
    pub id: u64,
    pub label: String,
    pub ordered: bool,
    pub max_packet_life_time: Option<u16>,
    pub max_retransmits: Option<u16>,
    pub protocol: String,
    pub negotiated: bool,
    pub ready_state: RTCDataChannelState,
    pub buffered_amount: u64,
    pub buffered_amount_low_threshold: u64,
    pub binary_type: BinaryType,
    messages: Vec<DataChannelMessage>,
}

#[derive(Debug, Clone)]
pub enum DataChannelMessage {
    Text(String),
    Binary(Vec<u8>),
}

impl RTCDataChannel {
    /// Send string data
    pub fn send(&mut self, data: &str) -> Result<(), RTCError> {
        if self.ready_state != RTCDataChannelState::Open {
            return Err(RTCError::InvalidState);
        }
        self.messages.push(DataChannelMessage::Text(data.to_string()));
        Ok(())
    }

    /// Send binary data
    pub fn send_binary(&mut self, data: Vec<u8>) -> Result<(), RTCError> {
        if self.ready_state != RTCDataChannelState::Open {
            return Err(RTCError::InvalidState);
        }
        self.buffered_amount += data.len() as u64;
        self.messages.push(DataChannelMessage::Binary(data));
        Ok(())
    }

    /// Close channel
    pub fn close(&mut self) {
        self.ready_state = RTCDataChannelState::Closing;
    }
}

/// RTP Sender
#[derive(Debug)]
pub struct RTCRtpSender {
    pub track: Option<MediaStreamTrack>,
}

/// RTP Receiver
#[derive(Debug)]
pub struct RTCRtpReceiver {
    pub track: MediaStreamTrack,
}

/// Media stream track
#[derive(Debug, Clone)]
pub struct MediaStreamTrack {
    pub id: String,
    pub kind: TrackKind,
    pub label: String,
    pub enabled: bool,
    pub muted: bool,
    pub ready_state: MediaStreamTrackState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Audio,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MediaStreamTrackState {
    #[default]
    Live,
    Ended,
}

/// RTC Error
#[derive(Debug)]
pub enum RTCError {
    InvalidState,
    InvalidParameter,
    NetworkError,
    NotSupported,
}

impl std::fmt::Display for RTCError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RTCError::InvalidState => write!(f, "Invalid state"),
            RTCError::InvalidParameter => write!(f, "Invalid parameter"),
            RTCError::NetworkError => write!(f, "Network error"),
            RTCError::NotSupported => write!(f, "Not supported"),
        }
    }
}

impl std::error::Error for RTCError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_connection() {
        let config = RTCConfiguration::default();
        let mut pc = RTCPeerConnection::new(config);

        assert_eq!(pc.connection_state, RTCPeerConnectionState::New);
        assert_eq!(pc.signaling_state, RTCSignalingState::Stable);
    }

    #[test]
    fn test_create_offer() {
        let mut pc = RTCPeerConnection::new(RTCConfiguration::default());
        let offer = pc.create_offer().unwrap();

        assert_eq!(offer.sdp_type, RTCSdpType::Offer);
        assert!(!offer.sdp.is_empty());
    }

    #[test]
    fn test_data_channel() {
        let mut pc = RTCPeerConnection::new(RTCConfiguration::default());
        let channel = pc.create_data_channel("test", None);

        assert_eq!(channel.label, "test");
        assert_eq!(channel.ready_state, RTCDataChannelState::Connecting);
    }

    #[test]
    fn test_signaling_flow() {
        let mut pc1 = RTCPeerConnection::new(RTCConfiguration::default());
        let mut pc2 = RTCPeerConnection::new(RTCConfiguration::default());

        // PC1 creates offer
        let offer = pc1.create_offer().unwrap();
        pc1.set_local_description(offer.clone()).unwrap();

        // PC2 receives offer
        pc2.set_remote_description(offer).unwrap();
        assert_eq!(pc2.signaling_state, RTCSignalingState::HaveRemoteOffer);

        // PC2 creates answer
        let answer = pc2.create_answer().unwrap();
        pc2.set_local_description(answer.clone()).unwrap();

        // PC1 receives answer
        pc1.set_remote_description(answer).unwrap();
        assert_eq!(pc1.signaling_state, RTCSignalingState::Stable);
    }
}
