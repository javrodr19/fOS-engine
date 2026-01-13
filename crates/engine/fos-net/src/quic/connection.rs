//! QUIC Connection
//!
//! Connection state machine and management per RFC 9000.

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::Instant;

use super::cid::{ConnectionId, CidManager};
use super::crypto::QuicCrypto;
use super::flow::FlowController;
use super::congestion::CubicController;
use super::loss::{LossDetection, PacketSpace, SentPacket};
use super::stream::{StreamManager, QuicStream, StreamError};
use super::frame::Frame;
use super::udp::AmplificationLimit;

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// Initial state, no packets sent
    Idle,
    /// Handshake in progress
    Handshaking,
    /// Connection established
    Connected,
    /// Draining (received CONNECTION_CLOSE, waiting)
    Draining,
    /// Closing (sent CONNECTION_CLOSE, waiting)
    Closing,
    /// Connection fully closed
    Closed,
}

impl Default for ConnectionState {
    fn default() -> Self {
        ConnectionState::Idle
    }
}

/// Connection error codes (RFC 9000 §20)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    NoError = 0x00,
    InternalError = 0x01,
    ConnectionRefused = 0x02,
    FlowControlError = 0x03,
    StreamLimitError = 0x04,
    StreamStateError = 0x05,
    FinalSizeError = 0x06,
    FrameEncodingError = 0x07,
    TransportParameterError = 0x08,
    ConnectionIdLimitError = 0x09,
    ProtocolViolation = 0x0a,
    InvalidToken = 0x0b,
    ApplicationError = 0x0c,
    CryptoBufferExceeded = 0x0d,
    KeyUpdateError = 0x0e,
    AeadLimitReached = 0x0f,
    NoViablePath = 0x10,
}

/// QUIC connection
#[derive(Debug)]
pub struct QuicConnection {
    /// Connection state
    state: ConnectionState,
    /// Local address
    local_addr: SocketAddr,
    /// Remote address
    remote_addr: SocketAddr,
    /// Connection ID manager
    cids: CidManager,
    /// Crypto state
    crypto: QuicCrypto,
    /// Stream manager
    streams: StreamManager,
    /// Connection-level flow control
    flow: FlowController,
    /// Congestion controller
    congestion: CubicController,
    /// Loss detection
    loss: LossDetection,
    /// Anti-amplification limit
    amplification: AmplificationLimit,
    /// Whether we are the client
    is_client: bool,
    /// Next packet number to send (by space)
    next_packet_number: [u64; 3],
    /// Pending frames to send
    pending_frames: VecDeque<Frame>,
    /// Close reason (if closing)
    close_reason: Option<(TransportError, String)>,
    /// Connection creation time
    created_at: Instant,
    /// Last activity time
    last_activity: Instant,
}

impl QuicConnection {
    /// Create a new client connection
    pub fn new_client(local_addr: SocketAddr, remote_addr: SocketAddr) -> Self {
        // Generate initial destination CID
        let dcid = ConnectionId::generate(8);
        
        Self {
            state: ConnectionState::Idle,
            local_addr,
            remote_addr,
            cids: CidManager::new(),
            crypto: QuicCrypto::new_client(&dcid),
            streams: StreamManager::new(true),
            flow: FlowController::new(),
            congestion: CubicController::new(),
            loss: LossDetection::new(),
            amplification: AmplificationLimit::new(),
            is_client: true,
            next_packet_number: [0, 0, 0],
            pending_frames: VecDeque::new(),
            close_reason: None,
            created_at: Instant::now(),
            last_activity: Instant::now(),
        }
    }
    
    /// Create a new server connection
    pub fn new_server(
        local_addr: SocketAddr,
        remote_addr: SocketAddr,
        dcid: &ConnectionId,
    ) -> Self {
        let mut cids = CidManager::new();
        cids.add_local_cid(8);
        
        Self {
            state: ConnectionState::Idle,
            local_addr,
            remote_addr,
            cids,
            crypto: QuicCrypto::new_server(dcid),
            streams: StreamManager::new(false),
            flow: FlowController::new(),
            congestion: CubicController::new(),
            loss: LossDetection::new(),
            amplification: AmplificationLimit::new(),
            is_client: false,
            next_packet_number: [0, 0, 0],
            pending_frames: VecDeque::new(),
            close_reason: None,
            created_at: Instant::now(),
            last_activity: Instant::now(),
        }
    }
    
    /// Get connection state
    pub fn state(&self) -> ConnectionState {
        self.state
    }
    
    /// Check if connection is established
    pub fn is_established(&self) -> bool {
        self.state == ConnectionState::Connected
    }
    
    /// Check if connection is closed
    pub fn is_closed(&self) -> bool {
        matches!(self.state, ConnectionState::Closed | ConnectionState::Closing | ConnectionState::Draining)
    }
    
    /// Get remote address
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }
    
    /// Get local address
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
    
    /// Start handshake (client)
    pub fn connect(&mut self) -> Result<(), ConnectionError> {
        if !self.is_client {
            return Err(ConnectionError::InvalidState);
        }
        
        self.state = ConnectionState::Handshaking;
        // Queue initial crypto data (ClientHello would go here)
        // For now, just transition state
        
        Ok(())
    }
    
    /// Open a new bidirectional stream
    pub fn open_stream(&mut self) -> Result<u64, ConnectionError> {
        if self.state != ConnectionState::Connected {
            return Err(ConnectionError::NotConnected);
        }
        
        Ok(self.streams.open_bidi())
    }
    
    /// Open a new unidirectional stream
    pub fn open_uni_stream(&mut self) -> Result<u64, ConnectionError> {
        if self.state != ConnectionState::Connected {
            return Err(ConnectionError::NotConnected);
        }
        
        Ok(self.streams.open_uni())
    }
    
    /// Get a stream
    pub fn stream(&self, id: u64) -> Option<&QuicStream> {
        self.streams.get(id)
    }
    
    /// Get a stream mutably
    pub fn stream_mut(&mut self, id: u64) -> Option<&mut QuicStream> {
        self.streams.get_mut(id)
    }
    
    /// Write data to a stream
    pub fn write(&mut self, stream_id: u64, data: &[u8]) -> Result<usize, ConnectionError> {
        let stream = self.streams.get_mut(stream_id)
            .ok_or(ConnectionError::InvalidStream)?;
        
        stream.write(data).map_err(|_| ConnectionError::StreamBlocked)
    }
    
    /// Read data from a stream
    pub fn read(&mut self, stream_id: u64, buf: &mut [u8]) -> Result<usize, ConnectionError> {
        let stream = self.streams.get_mut(stream_id)
            .ok_or(ConnectionError::InvalidStream)?;
        
        stream.read(buf).map_err(|e| match e {
            StreamError::Finished => ConnectionError::StreamFinished,
            _ => ConnectionError::StreamBlocked,
        })
    }
    
    /// Close a stream
    pub fn close_stream(&mut self, stream_id: u64) -> Result<(), ConnectionError> {
        self.streams.close(stream_id);
        Ok(())
    }
    
    /// Process received frames
    pub fn process_frames(&mut self, frames: Vec<Frame>, now: Instant) -> Result<(), ConnectionError> {
        self.last_activity = now;
        
        for frame in frames {
            self.process_frame(frame, now)?;
        }
        
        Ok(())
    }
    
    /// Process a single frame
    fn process_frame(&mut self, frame: Frame, now: Instant) -> Result<(), ConnectionError> {
        match frame {
            Frame::Padding | Frame::Ping => {
                // No action needed
            }
            
            Frame::Ack { largest_acked, ack_delay, .. } => {
                let ack_info = super::loss::AckInfo {
                    largest_acked,
                    ack_delay: std::time::Duration::from_micros(ack_delay),
                    ack_time: now,
                };
                
                let (acked, lost) = self.loss.on_ack_received(
                    PacketSpace::Application,
                    &ack_info,
                    self.congestion.smoothed_rtt(),
                );
                
                for packet in acked {
                    self.congestion.on_ack(packet.size as u64, now);
                }
                
                if !lost.is_empty() {
                    self.congestion.on_loss(now);
                }
            }
            
            Frame::Crypto { offset, data } => {
                // Process TLS handshake data
                // In a full implementation, this would feed data to the TLS layer
                let _ = (offset, data);
            }
            
            Frame::Stream { stream_id, offset, data, fin } => {
                let stream = self.streams.get_or_create(stream_id)
                    .map_err(|_| ConnectionError::InvalidStream)?;
                
                stream.receive(offset, data, fin)
                    .map_err(|_| ConnectionError::FlowControl)?;
            }
            
            Frame::MaxData { max_data } => {
                self.flow.update_send_max(max_data);
            }
            
            Frame::MaxStreamData { stream_id, max_data } => {
                if let Some(stream) = self.streams.get_mut(stream_id) {
                    stream.update_send_max(max_data);
                }
            }
            
            Frame::MaxStreams { max_streams: _, bidirectional: _ } => {
                // Update stream limits
            }
            
            Frame::NewConnectionId { sequence, retire_prior_to, connection_id, stateless_reset_token } => {
                self.cids.add_remote_cid(connection_id, sequence, Some(stateless_reset_token));
                self.cids.retire_local_prior_to(retire_prior_to);
            }
            
            Frame::ConnectionClose { error_code, reason, .. } => {
                self.close_reason = Some((
                    TransportError::NoError, // Would map error_code
                    reason,
                ));
                self.state = ConnectionState::Draining;
                let _ = error_code;
            }
            
            Frame::HandshakeDone => {
                if !self.is_client {
                    return Err(ConnectionError::ProtocolError);
                }
                self.state = ConnectionState::Connected;
                self.amplification.validate_address();
            }
            
            _ => {
                // Handle other frames
            }
        }
        
        Ok(())
    }
    
    /// Queue a frame to send
    pub fn queue_frame(&mut self, frame: Frame) {
        self.pending_frames.push_back(frame);
    }
    
    /// Get pending frames to send
    pub fn get_pending_frames(&mut self) -> Vec<Frame> {
        self.pending_frames.drain(..).collect()
    }
    
    /// Get next packet number for a space
    pub fn next_packet_number(&mut self, space: PacketSpace) -> u64 {
        let idx = match space {
            PacketSpace::Initial => 0,
            PacketSpace::Handshake => 1,
            PacketSpace::Application => 2,
        };
        
        let pn = self.next_packet_number[idx];
        self.next_packet_number[idx] += 1;
        pn
    }
    
    /// Record a sent packet
    pub fn on_packet_sent(&mut self, packet: SentPacket) {
        self.loss.on_packet_sent(packet);
    }
    
    /// Close the connection
    pub fn close(&mut self, error: TransportError, reason: &str) {
        if self.is_closed() {
            return;
        }
        
        self.close_reason = Some((error, reason.to_string()));
        self.state = ConnectionState::Closing;
        
        // Queue CONNECTION_CLOSE frame
        self.queue_frame(Frame::ConnectionClose {
            error_code: error as u64,
            frame_type: None,
            reason: reason.to_string(),
        });
    }
    
    /// Check if we can send (congestion and flow control)
    pub fn can_send(&self, bytes: u64) -> bool {
        self.congestion.can_send(bytes) && self.flow.can_send(bytes)
    }
    
    /// Get current congestion window
    pub fn cwnd(&self) -> u64 {
        self.congestion.cwnd()
    }
    
    /// Get RTT estimate
    pub fn rtt(&self) -> std::time::Duration {
        self.congestion.smoothed_rtt()
    }
    
    /// Get connection age
    pub fn age(&self) -> std::time::Duration {
        self.created_at.elapsed()
    }
    
    /// Get time since last activity
    pub fn idle_time(&self) -> std::time::Duration {
        self.last_activity.elapsed()
    }
}

/// Connection error
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConnectionError {
    #[error("Invalid connection state")]
    InvalidState,
    
    #[error("Connection not established")]
    NotConnected,
    
    #[error("Invalid stream ID")]
    InvalidStream,
    
    #[error("Stream is blocked")]
    StreamBlocked,
    
    #[error("Stream is finished")]
    StreamFinished,
    
    #[error("Flow control error")]
    FlowControl,
    
    #[error("Protocol error")]
    ProtocolError,
    
    #[error("Connection refused")]
    ConnectionRefused,
    
    #[error("Handshake failed")]
    HandshakeFailed,
    
    #[error("Timed out")]
    TimedOut,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_client_connection_creation() {
        let local = "127.0.0.1:12345".parse().unwrap();
        let remote = "127.0.0.1:443".parse().unwrap();
        
        let conn = QuicConnection::new_client(local, remote);
        
        assert_eq!(conn.state(), ConnectionState::Idle);
        assert!(conn.is_client);
    }
    
    #[test]
    fn test_connection_connect() {
        let local = "127.0.0.1:12345".parse().unwrap();
        let remote = "127.0.0.1:443".parse().unwrap();
        
        let mut conn = QuicConnection::new_client(local, remote);
        conn.connect().unwrap();
        
        assert_eq!(conn.state(), ConnectionState::Handshaking);
    }
    
    #[test]
    fn test_packet_number_increment() {
        let local = "127.0.0.1:12345".parse().unwrap();
        let remote = "127.0.0.1:443".parse().unwrap();
        
        let mut conn = QuicConnection::new_client(local, remote);
        
        assert_eq!(conn.next_packet_number(PacketSpace::Initial), 0);
        assert_eq!(conn.next_packet_number(PacketSpace::Initial), 1);
        assert_eq!(conn.next_packet_number(PacketSpace::Initial), 2);
    }
    
    #[test]
    fn test_connection_close() {
        let local = "127.0.0.1:12345".parse().unwrap();
        let remote = "127.0.0.1:443".parse().unwrap();
        
        let mut conn = QuicConnection::new_client(local, remote);
        conn.close(TransportError::NoError, "test close");
        
        assert!(conn.is_closed());
        assert_eq!(conn.state(), ConnectionState::Closing);
    }
}
