//! UDP Socket Layer
//!
//! Non-blocking UDP socket handling for QUIC transport.
//! Uses smol async runtime for I/O operations.

use std::io;
use std::net::SocketAddr;
use std::collections::VecDeque;

/// Explicit Congestion Notification marking
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EcnMark {
    /// Not ECN-Capable Transport
    #[default]
    NonEct,
    /// ECN Capable Transport (0)
    Ect0,
    /// ECN Capable Transport (1)
    Ect1,
    /// Congestion Experienced
    Ce,
}

impl EcnMark {
    /// Convert from IP TOS/Traffic Class field bits
    pub fn from_tos(tos: u8) -> Self {
        match tos & 0x03 {
            0b00 => EcnMark::NonEct,
            0b01 => EcnMark::Ect1,
            0b10 => EcnMark::Ect0,
            0b11 => EcnMark::Ce,
            _ => unreachable!(),
        }
    }
    
    /// Convert to IP TOS/Traffic Class field bits
    pub fn to_tos(self) -> u8 {
        match self {
            EcnMark::NonEct => 0b00,
            EcnMark::Ect1 => 0b01,
            EcnMark::Ect0 => 0b10,
            EcnMark::Ce => 0b11,
        }
    }
}

/// UDP datagram with metadata
#[derive(Debug, Clone)]
pub struct Datagram {
    /// Packet data
    pub data: Vec<u8>,
    /// Source/destination address
    pub addr: SocketAddr,
    /// ECN marking
    pub ecn: EcnMark,
    /// Receive timestamp (monotonic)
    pub timestamp: u64,
}

impl Datagram {
    /// Create a new datagram
    pub fn new(data: Vec<u8>, addr: SocketAddr) -> Self {
        Self {
            data,
            addr,
            ecn: EcnMark::NonEct,
            timestamp: 0,
        }
    }
    
    /// Create with ECN marking
    pub fn with_ecn(mut self, ecn: EcnMark) -> Self {
        self.ecn = ecn;
        self
    }
    
    /// Get data length
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Buffer pool for datagrams to reduce allocations
pub struct DatagramPool {
    /// Free buffers
    pool: VecDeque<Vec<u8>>,
    /// Buffer size
    buffer_size: usize,
    /// Maximum pool size
    max_pool: usize,
}

impl DatagramPool {
    /// Create a new datagram pool
    pub fn new(buffer_size: usize, initial_count: usize) -> Self {
        let mut pool = VecDeque::with_capacity(initial_count);
        for _ in 0..initial_count {
            pool.push_back(vec![0u8; buffer_size]);
        }
        Self {
            pool,
            buffer_size,
            max_pool: initial_count * 2,
        }
    }
    
    /// Acquire a buffer from the pool
    pub fn acquire(&mut self) -> Vec<u8> {
        self.pool.pop_front().unwrap_or_else(|| vec![0u8; self.buffer_size])
    }
    
    /// Release a buffer back to the pool
    pub fn release(&mut self, mut buffer: Vec<u8>) {
        if self.pool.len() < self.max_pool {
            buffer.clear();
            buffer.resize(self.buffer_size, 0);
            self.pool.push_back(buffer);
        }
    }
    
    /// Number of available buffers
    pub fn available(&self) -> usize {
        self.pool.len()
    }
}

impl Default for DatagramPool {
    fn default() -> Self {
        // QUIC max UDP payload is 1200 bytes minimum, 1500 typical
        Self::new(1500, 16)
    }
}

/// Anti-amplification tracking for QUIC
#[derive(Debug)]
pub struct AmplificationLimit {
    /// Bytes received from peer
    bytes_received: u64,
    /// Bytes sent to peer
    bytes_sent: u64,
    /// Whether address is validated
    address_validated: bool,
}

impl AmplificationLimit {
    /// Create new amplification limiter
    pub fn new() -> Self {
        Self {
            bytes_received: 0,
            bytes_sent: 0,
            address_validated: false,
        }
    }
    
    /// Record bytes received
    pub fn record_received(&mut self, bytes: usize) {
        self.bytes_received = self.bytes_received.saturating_add(bytes as u64);
    }
    
    /// Check if we can send `bytes` without exceeding 3x amplification
    pub fn can_send(&self, bytes: usize) -> bool {
        if self.address_validated {
            return true;
        }
        // RFC 9000 §8.1: Anti-amplification limit of 3x
        let limit = self.bytes_received.saturating_mul(3);
        self.bytes_sent.saturating_add(bytes as u64) <= limit
    }
    
    /// Record bytes sent
    pub fn record_sent(&mut self, bytes: usize) {
        self.bytes_sent = self.bytes_sent.saturating_add(bytes as u64);
    }
    
    /// Mark address as validated (handshake complete)
    pub fn validate_address(&mut self) {
        self.address_validated = true;
    }
    
    /// Check if address is validated
    pub fn is_validated(&self) -> bool {
        self.address_validated
    }
    
    /// Reset limits (for connection migration)
    pub fn reset(&mut self) {
        self.bytes_received = 0;
        self.bytes_sent = 0;
        self.address_validated = false;
    }
}

impl Default for AmplificationLimit {
    fn default() -> Self {
        Self::new()
    }
}

/// UDP socket wrapper for QUIC
/// 
/// Uses smol's async UDP socket with buffer pooling.
pub struct UdpSocket {
    /// Underlying async UDP socket
    inner: smol::net::UdpSocket,
    /// Send buffer pool
    send_pool: DatagramPool,
    /// Receive buffer pool  
    recv_pool: DatagramPool,
    /// Local address
    local_addr: SocketAddr,
    /// Monotonic time counter for timestamps
    time_base: std::time::Instant,
}

impl UdpSocket {
    /// Bind to a local address
    pub async fn bind(addr: SocketAddr) -> io::Result<Self> {
        let inner = smol::net::UdpSocket::bind(addr).await?;
        let local_addr = inner.local_addr()?;
        
        Ok(Self {
            inner,
            send_pool: DatagramPool::default(),
            recv_pool: DatagramPool::default(),
            local_addr,
            time_base: std::time::Instant::now(),
        })
    }
    
    /// Bind to any available port
    pub async fn bind_any() -> io::Result<Self> {
        Self::bind("0.0.0.0:0".parse().unwrap()).await
    }
    
    /// Get local address
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }
    
    /// Send a datagram
    pub async fn send(&self, datagram: &Datagram) -> io::Result<usize> {
        self.inner.send_to(&datagram.data, datagram.addr).await
    }
    
    /// Receive a datagram
    pub async fn recv(&mut self) -> io::Result<Datagram> {
        let mut buffer = self.recv_pool.acquire();
        
        let (len, addr) = self.inner.recv_from(&mut buffer).await?;
        buffer.truncate(len);
        
        let timestamp = self.time_base.elapsed().as_micros() as u64;
        
        Ok(Datagram {
            data: buffer,
            addr,
            ecn: EcnMark::NonEct, // ECN requires platform-specific socket options
            timestamp,
        })
    }
    
    /// Return a buffer to the pool
    pub fn recycle(&mut self, datagram: Datagram) {
        self.recv_pool.release(datagram.data);
    }
    
    /// Acquire a send buffer from the pool
    pub fn acquire_send_buffer(&mut self) -> Vec<u8> {
        self.send_pool.acquire()
    }
    
    /// Return a send buffer to the pool
    pub fn release_send_buffer(&mut self, buffer: Vec<u8>) {
        self.send_pool.release(buffer);
    }
    
    /// Get current monotonic timestamp in microseconds
    pub fn timestamp(&self) -> u64 {
        self.time_base.elapsed().as_micros() as u64
    }
}

impl std::fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UdpSocket")
            .field("local_addr", &self.local_addr)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ecn_mark_roundtrip() {
        for mark in [EcnMark::NonEct, EcnMark::Ect0, EcnMark::Ect1, EcnMark::Ce] {
            let tos = mark.to_tos();
            let recovered = EcnMark::from_tos(tos);
            assert_eq!(mark, recovered);
        }
    }
    
    #[test]
    fn test_datagram_pool() {
        let mut pool = DatagramPool::new(1500, 4);
        assert_eq!(pool.available(), 4);
        
        let b1 = pool.acquire();
        assert_eq!(pool.available(), 3);
        assert_eq!(b1.len(), 1500);
        
        pool.release(b1);
        assert_eq!(pool.available(), 4);
    }
    
    #[test]
    fn test_amplification_limit() {
        let mut limit = AmplificationLimit::new();
        
        // Can't send anything before receiving
        assert!(!limit.can_send(1));
        
        // Receive 100 bytes, can send up to 300
        limit.record_received(100);
        assert!(limit.can_send(300));
        assert!(!limit.can_send(301));
        
        // After validation, no limit
        limit.validate_address();
        assert!(limit.can_send(10000));
    }
    
    #[test]
    fn test_datagram_creation() {
        let addr: SocketAddr = "127.0.0.1:8080".parse().unwrap();
        let dg = Datagram::new(vec![1, 2, 3], addr)
            .with_ecn(EcnMark::Ect0);
        
        assert_eq!(dg.len(), 3);
        assert_eq!(dg.addr, addr);
        assert_eq!(dg.ecn, EcnMark::Ect0);
    }
}
