//! QUIC Version Negotiation
//!
//! Version negotiation and 0-RTT support per RFC 9000.

/// Supported QUIC versions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuicVersion {
    /// QUIC v1 (RFC 9000)
    V1 = 0x00000001,
    /// QUIC v2 (RFC 9369)
    V2 = 0x6b3343cf,
}

impl QuicVersion {
    /// All supported versions in preference order
    pub const SUPPORTED: &'static [QuicVersion] = &[QuicVersion::V1, QuicVersion::V2];
    
    /// Parse version from wire format
    pub fn from_wire(version: u32) -> Option<Self> {
        match version {
            0x00000001 => Some(QuicVersion::V1),
            0x6b3343cf => Some(QuicVersion::V2),
            _ => None,
        }
    }
    
    /// Convert to wire format
    pub fn to_wire(self) -> u32 {
        self as u32
    }
    
    /// Check if version is supported
    pub fn is_supported(version: u32) -> bool {
        Self::from_wire(version).is_some()
    }
    
    /// Get the initial salt for this version (used in initial packet protection)
    pub fn initial_salt(&self) -> &'static [u8] {
        match self {
            // RFC 9001 Section 5.2
            QuicVersion::V1 => &[
                0x38, 0x76, 0x2c, 0xf7, 0xf5, 0x59, 0x34, 0xb3, 0x4d, 0x17,
                0x9a, 0xe6, 0xa4, 0xc8, 0x0c, 0xad, 0xcc, 0xbb, 0x7f, 0x0a,
            ],
            // RFC 9369 Section 5.2
            QuicVersion::V2 => &[
                0x0d, 0xed, 0xe3, 0xde, 0xf7, 0x00, 0xa6, 0xdb, 0x81, 0x93,
                0x81, 0xbe, 0x6e, 0x26, 0x9d, 0xcb, 0xf9, 0xbd, 0x2e, 0xd9,
            ],
        }
    }
}

/// Version negotiation packet
#[derive(Debug, Clone)]
pub struct VersionNegotiation {
    /// Destination connection ID (echoed from client)
    pub dcid: Vec<u8>,
    /// Source connection ID (echoed from client)
    pub scid: Vec<u8>,
    /// Supported versions
    pub versions: Vec<u32>,
}

impl VersionNegotiation {
    /// Create a version negotiation packet
    pub fn new(dcid: Vec<u8>, scid: Vec<u8>) -> Self {
        let versions = QuicVersion::SUPPORTED
            .iter()
            .map(|v| v.to_wire())
            .collect();
        
        Self { dcid, scid, versions }
    }
    
    /// Encode to bytes
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        
        // First byte with Long Header form bit set, random other bits
        buf.push(0x80);
        
        // Version (0 for version negotiation)
        buf.extend_from_slice(&0u32.to_be_bytes());
        
        // DCID length and DCID
        buf.push(self.dcid.len() as u8);
        buf.extend_from_slice(&self.dcid);
        
        // SCID length and SCID
        buf.push(self.scid.len() as u8);
        buf.extend_from_slice(&self.scid);
        
        // Supported versions
        for version in &self.versions {
            buf.extend_from_slice(&version.to_be_bytes());
        }
        
        buf
    }
    
    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 7 {
            return None;
        }
        
        // Check long header form
        if data[0] & 0x80 == 0 {
            return None;
        }
        
        // Version must be 0
        let version = u32::from_be_bytes([data[1], data[2], data[3], data[4]]);
        if version != 0 {
            return None;
        }
        
        let mut pos = 5;
        
        // DCID
        let dcid_len = data[pos] as usize;
        pos += 1;
        if data.len() < pos + dcid_len {
            return None;
        }
        let dcid = data[pos..pos + dcid_len].to_vec();
        pos += dcid_len;
        
        // SCID
        if data.len() <= pos {
            return None;
        }
        let scid_len = data[pos] as usize;
        pos += 1;
        if data.len() < pos + scid_len {
            return None;
        }
        let scid = data[pos..pos + scid_len].to_vec();
        pos += scid_len;
        
        // Parse versions
        let mut versions = Vec::new();
        while pos + 4 <= data.len() {
            let v = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
            versions.push(v);
            pos += 4;
        }
        
        Some(Self { dcid, scid, versions })
    }
    
    /// Select best version from server's list
    pub fn select_version(server_versions: &[u32]) -> Option<QuicVersion> {
        for &preferred in QuicVersion::SUPPORTED {
            if server_versions.contains(&preferred.to_wire()) {
                return Some(preferred);
            }
        }
        None
    }
}

/// 0-RTT state
#[derive(Debug, Clone)]
pub struct ZeroRttState {
    /// Whether 0-RTT is available
    pub available: bool,
    /// Maximum 0-RTT data size (from ticket)
    pub max_early_data: u64,
    /// Bytes of 0-RTT data sent
    pub bytes_sent: u64,
    /// Whether 0-RTT was accepted by server
    pub accepted: Option<bool>,
    /// Application protocol from previous connection
    pub alpn: Option<String>,
    /// Resumption secret
    resumption_secret: Option<Vec<u8>>,
}

impl ZeroRttState {
    /// Create new 0-RTT state (no resumption available)
    pub fn new() -> Self {
        Self {
            available: false,
            max_early_data: 0,
            bytes_sent: 0,
            accepted: None,
            alpn: None,
            resumption_secret: None,
        }
    }
    
    /// Create 0-RTT state from a session ticket
    pub fn from_ticket(
        max_early_data: u64,
        alpn: String,
        resumption_secret: Vec<u8>,
    ) -> Self {
        Self {
            available: max_early_data > 0,
            max_early_data,
            bytes_sent: 0,
            accepted: None,
            alpn: Some(alpn),
            resumption_secret: Some(resumption_secret),
        }
    }
    
    /// Check if we can send more 0-RTT data
    pub fn can_send(&self, bytes: u64) -> bool {
        self.available && self.accepted != Some(false) && 
        self.bytes_sent + bytes <= self.max_early_data
    }
    
    /// Record 0-RTT bytes sent
    pub fn record_sent(&mut self, bytes: u64) {
        self.bytes_sent += bytes;
    }
    
    /// Server accepted 0-RTT
    pub fn accept(&mut self) {
        self.accepted = Some(true);
    }
    
    /// Server rejected 0-RTT
    pub fn reject(&mut self) {
        self.accepted = Some(false);
    }
    
    /// Check if 0-RTT was accepted
    pub fn was_accepted(&self) -> Option<bool> {
        self.accepted
    }
    
    /// Get resumption secret for deriving 0-RTT keys
    pub fn resumption_secret(&self) -> Option<&[u8]> {
        self.resumption_secret.as_deref()
    }
}

impl Default for ZeroRttState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_version_from_wire() {
        assert_eq!(QuicVersion::from_wire(0x00000001), Some(QuicVersion::V1));
        assert_eq!(QuicVersion::from_wire(0x6b3343cf), Some(QuicVersion::V2));
        assert_eq!(QuicVersion::from_wire(0xdeadbeef), None);
    }
    
    #[test]
    fn test_version_negotiation_encode_decode() {
        let vn = VersionNegotiation::new(vec![1, 2, 3, 4], vec![5, 6, 7, 8]);
        let encoded = vn.encode();
        let decoded = VersionNegotiation::decode(&encoded).unwrap();
        
        assert_eq!(decoded.dcid, vec![1, 2, 3, 4]);
        assert_eq!(decoded.scid, vec![5, 6, 7, 8]);
        assert!(!decoded.versions.is_empty());
    }
    
    #[test]
    fn test_select_version() {
        let server_versions = vec![0x00000001, 0xff000020];
        assert_eq!(
            VersionNegotiation::select_version(&server_versions),
            Some(QuicVersion::V1)
        );
        
        let server_versions = vec![0xdeadbeef];
        assert_eq!(VersionNegotiation::select_version(&server_versions), None);
    }
    
    #[test]
    fn test_zero_rtt_state() {
        let mut state = ZeroRttState::new();
        assert!(!state.available);
        
        let mut state = ZeroRttState::from_ticket(1024, "h3".to_string(), vec![0; 32]);
        assert!(state.available);
        assert!(state.can_send(100));
        
        state.record_sent(500);
        assert!(state.can_send(500));
        assert!(!state.can_send(600));
        
        state.accept();
        assert_eq!(state.was_accepted(), Some(true));
    }
    
    #[test]
    fn test_initial_salt() {
        let salt = QuicVersion::V1.initial_salt();
        assert_eq!(salt.len(), 20);
        assert_eq!(salt[0], 0x38);
    }
}
