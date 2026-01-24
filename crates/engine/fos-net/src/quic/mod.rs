//! QUIC Protocol Implementation
//!
//! Custom QUIC transport layer for fOS-engine.
//! Zero external dependencies - integrates with smol async runtime.

pub mod udp;
pub mod packet;
pub mod frame;
pub mod cid;
pub mod crypto;
pub mod connection;
pub mod stream;
pub mod flow;
pub mod congestion;
pub mod loss;
pub mod qpack;
pub mod version;
pub mod migration;
pub mod h3_frame;
pub mod altsvc;
pub mod push;
pub mod bbrv2;
pub mod session_cache;

// Core UDP layer
pub use udp::{UdpSocket, Datagram, EcnMark};

// Packet layer
pub use packet::{QuicPacket, PacketType, LongHeader, ShortHeader, PacketHeader};
pub use frame::Frame;
pub use cid::ConnectionId;

// Crypto
pub use crypto::QuicCrypto;

// Transport
pub use connection::{QuicConnection, ConnectionState};
pub use stream::{QuicStream, StreamState};
pub use flow::FlowController;
pub use congestion::CubicController;
pub use loss::LossDetection;

// Version and 0-RTT
pub use version::{QuicVersion, VersionNegotiation, ZeroRttState};

// Connection migration
pub use migration::{PathManager, NetworkPath, PathState, PathChangeResult};

// HTTP/3
pub use qpack::{QpackEncoder, QpackDecoder};
pub use h3_frame::{Http3Frame, Http3Setting, Http3SettingId, UniStreamType, default_settings};
pub use altsvc::{AltSvc, AltSvcEntry, AltSvcCache};
pub use push::{PushManager, ServerPush, PushState, PushError};

// BBRv2 congestion control
pub use bbrv2::{Bbrv2Controller, BbrState, CongestionAlgorithm};

// Session cache for 0-RTT
pub use session_cache::{SessionCache, SessionTicket, SessionCacheKey, SessionCacheStats, EarlyDataBuffer};
