//! QUIC Packet Layer
//!
//! QUIC packet parsing and serialization.
//! Implements Long and Short header packet formats per RFC 9000.

use super::cid::ConnectionId;

/// QUIC version
pub const QUIC_VERSION_1: u32 = 0x00000001;
/// Version for version negotiation
pub const VERSION_NEGOTIATION: u32 = 0x00000000;

/// Packet type (for long headers)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketType {
    /// Initial packet (type 0x00)
    Initial,
    /// 0-RTT packet (type 0x01)
    ZeroRtt,
    /// Handshake packet (type 0x02)
    Handshake,
    /// Retry packet (type 0x03)
    Retry,
    /// Short header packet (1-RTT)
    Short,
}

impl PacketType {
    /// Convert from long header type bits
    pub fn from_long_header_type(bits: u8) -> Option<Self> {
        match bits & 0x30 >> 4 {
            0x00 => Some(PacketType::Initial),
            0x01 => Some(PacketType::ZeroRtt),
            0x02 => Some(PacketType::Handshake),
            0x03 => Some(PacketType::Retry),
            _ => None,
        }
    }
    
    /// Convert to long header type bits
    pub fn to_long_header_type(self) -> u8 {
        match self {
            PacketType::Initial => 0x00,
            PacketType::ZeroRtt => 0x01,
            PacketType::Handshake => 0x02,
            PacketType::Retry => 0x03,
            PacketType::Short => panic!("Short packets don't have long header type"),
        }
    }
}

/// Long header (Initial, Handshake, 0-RTT, Retry)
#[derive(Debug, Clone)]
pub struct LongHeader {
    /// Packet type
    pub packet_type: PacketType,
    /// QUIC version
    pub version: u32,
    /// Destination connection ID
    pub dcid: ConnectionId,
    /// Source connection ID
    pub scid: ConnectionId,
    /// Token (Initial packets only)
    pub token: Vec<u8>,
    /// Packet number (decoded, variable length 1-4 bytes)
    pub packet_number: u64,
    /// Packet number length (1-4)
    pub packet_number_len: u8,
}

impl LongHeader {
    /// Create a new Initial packet header
    pub fn initial(dcid: ConnectionId, scid: ConnectionId, pn: u64) -> Self {
        Self {
            packet_type: PacketType::Initial,
            version: QUIC_VERSION_1,
            dcid,
            scid,
            token: Vec::new(),
            packet_number: pn,
            packet_number_len: Self::packet_number_length(pn),
        }
    }
    
    /// Create a new Handshake packet header
    pub fn handshake(dcid: ConnectionId, scid: ConnectionId, pn: u64) -> Self {
        Self {
            packet_type: PacketType::Handshake,
            version: QUIC_VERSION_1,
            dcid,
            scid,
            token: Vec::new(),
            packet_number: pn,
            packet_number_len: Self::packet_number_length(pn),
        }
    }
    
    /// Calculate minimum bytes needed for packet number
    fn packet_number_length(pn: u64) -> u8 {
        if pn <= 0xFF { 1 }
        else if pn <= 0xFFFF { 2 }
        else if pn <= 0xFFFFFF { 3 }
        else { 4 }
    }
}

/// Short header (1-RTT packets)
#[derive(Debug, Clone)]
pub struct ShortHeader {
    /// Destination connection ID
    pub dcid: ConnectionId,
    /// Packet number (decoded)
    pub packet_number: u64,
    /// Packet number length (1-4)
    pub packet_number_len: u8,
    /// Key phase bit
    pub key_phase: bool,
    /// Spin bit
    pub spin_bit: bool,
}

impl ShortHeader {
    /// Create a new short header
    pub fn new(dcid: ConnectionId, pn: u64, key_phase: bool) -> Self {
        let pn_len = if pn <= 0xFF { 1 }
            else if pn <= 0xFFFF { 2 }
            else if pn <= 0xFFFFFF { 3 }
            else { 4 };
        
        Self {
            dcid,
            packet_number: pn,
            packet_number_len: pn_len,
            key_phase,
            spin_bit: false,
        }
    }
}

/// Packet header (either long or short)
#[derive(Debug, Clone)]
pub enum PacketHeader {
    Long(LongHeader),
    Short(ShortHeader),
}

impl PacketHeader {
    /// Get packet type
    pub fn packet_type(&self) -> PacketType {
        match self {
            PacketHeader::Long(h) => h.packet_type,
            PacketHeader::Short(_) => PacketType::Short,
        }
    }
    
    /// Get destination CID
    pub fn dcid(&self) -> &ConnectionId {
        match self {
            PacketHeader::Long(h) => &h.dcid,
            PacketHeader::Short(h) => &h.dcid,
        }
    }
    
    /// Get packet number
    pub fn packet_number(&self) -> u64 {
        match self {
            PacketHeader::Long(h) => h.packet_number,
            PacketHeader::Short(h) => h.packet_number,
        }
    }
    
    /// Is this a long header packet?
    pub fn is_long(&self) -> bool {
        matches!(self, PacketHeader::Long(_))
    }
}

/// A complete QUIC packet
#[derive(Debug, Clone)]
pub struct QuicPacket {
    /// Packet header
    pub header: PacketHeader,
    /// Encrypted/protected payload (frames)
    pub payload: Vec<u8>,
}

impl QuicPacket {
    /// Create a new packet
    pub fn new(header: PacketHeader, payload: Vec<u8>) -> Self {
        Self { header, payload }
    }
    
    /// Get the packet type
    pub fn packet_type(&self) -> PacketType {
        self.header.packet_type()
    }
}

/// Variable-length integer encoding (RFC 9000 §16)
pub mod varint {
    /// Maximum value for a varint
    pub const MAX_VALUE: u64 = (1 << 62) - 1;
    
    /// Encode a varint, returns number of bytes written
    pub fn encode(value: u64, buf: &mut [u8]) -> Option<usize> {
        if value <= 0x3F {
            if buf.is_empty() { return None; }
            buf[0] = value as u8;
            Some(1)
        } else if value <= 0x3FFF {
            if buf.len() < 2 { return None; }
            buf[0] = ((value >> 8) as u8) | 0x40;
            buf[1] = value as u8;
            Some(2)
        } else if value <= 0x3FFFFFFF {
            if buf.len() < 4 { return None; }
            buf[0] = ((value >> 24) as u8) | 0x80;
            buf[1] = (value >> 16) as u8;
            buf[2] = (value >> 8) as u8;
            buf[3] = value as u8;
            Some(4)
        } else if value <= MAX_VALUE {
            if buf.len() < 8 { return None; }
            buf[0] = ((value >> 56) as u8) | 0xC0;
            buf[1] = (value >> 48) as u8;
            buf[2] = (value >> 40) as u8;
            buf[3] = (value >> 32) as u8;
            buf[4] = (value >> 24) as u8;
            buf[5] = (value >> 16) as u8;
            buf[6] = (value >> 8) as u8;
            buf[7] = value as u8;
            Some(8)
        } else {
            None
        }
    }
    
    /// Decode a varint, returns (value, bytes_read)
    pub fn decode(buf: &[u8]) -> Option<(u64, usize)> {
        if buf.is_empty() {
            return None;
        }
        
        let first = buf[0];
        let len = 1 << (first >> 6);
        
        if buf.len() < len {
            return None;
        }
        
        let value = match len {
            1 => (first & 0x3F) as u64,
            2 => {
                ((first & 0x3F) as u64) << 8 | buf[1] as u64
            }
            4 => {
                ((first & 0x3F) as u64) << 24
                    | (buf[1] as u64) << 16
                    | (buf[2] as u64) << 8
                    | buf[3] as u64
            }
            8 => {
                ((first & 0x3F) as u64) << 56
                    | (buf[1] as u64) << 48
                    | (buf[2] as u64) << 40
                    | (buf[3] as u64) << 32
                    | (buf[4] as u64) << 24
                    | (buf[5] as u64) << 16
                    | (buf[6] as u64) << 8
                    | buf[7] as u64
            }
            _ => unreachable!(),
        };
        
        Some((value, len))
    }
    
    /// Get encoded length for a value
    pub fn encoded_len(value: u64) -> usize {
        if value <= 0x3F { 1 }
        else if value <= 0x3FFF { 2 }
        else if value <= 0x3FFFFFFF { 4 }
        else { 8 }
    }
}

/// Packet number decoding
pub mod packet_number {
    /// Decode a truncated packet number
    /// 
    /// RFC 9000 Appendix A
    pub fn decode(truncated: u64, truncated_len: u8, largest_pn: u64) -> u64 {
        let expected = largest_pn.wrapping_add(1);
        let window = 1u64 << (truncated_len * 8);
        let half_window = window / 2;
        
        let candidate = (expected & !(window - 1)) | truncated;
        
        if candidate <= expected.wrapping_sub(half_window) && candidate < (1u64 << 62) - window {
            candidate.wrapping_add(window)
        } else if candidate > expected.wrapping_add(half_window) && candidate >= window {
            candidate.wrapping_sub(window)
        } else {
            candidate
        }
    }
    
    /// Encode a packet number, returns (truncated, length)
    pub fn encode(full_pn: u64, largest_acked: u64) -> (u64, u8) {
        let num_unacked = full_pn.saturating_sub(largest_acked);
        
        let len = if num_unacked < (1 << 7) { 1 }
            else if num_unacked < (1 << 15) { 2 }
            else if num_unacked < (1 << 23) { 3 }
            else { 4 };
        
        let mask = (1u64 << (len * 8)) - 1;
        (full_pn & mask, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_varint_encode_decode() {
        let test_values = [0, 63, 64, 16383, 16384, 1073741823, 1073741824];
        
        for &val in &test_values {
            let mut buf = [0u8; 8];
            let len = varint::encode(val, &mut buf).unwrap();
            let (decoded, decoded_len) = varint::decode(&buf).unwrap();
            
            assert_eq!(len, decoded_len);
            assert_eq!(val, decoded);
        }
    }
    
    #[test]
    fn test_varint_encoded_len() {
        assert_eq!(varint::encoded_len(0), 1);
        assert_eq!(varint::encoded_len(63), 1);
        assert_eq!(varint::encoded_len(64), 2);
        assert_eq!(varint::encoded_len(16383), 2);
        assert_eq!(varint::encoded_len(16384), 4);
    }
    
    #[test]
    fn test_packet_number_decode() {
        // RFC 9000 Appendix A examples
        assert_eq!(packet_number::decode(0x9b32, 2, 0xa82f30ea), 0xa82f9b32);
    }
    
    #[test]
    fn test_packet_number_encode() {
        let (truncated, len) = packet_number::encode(100, 0);
        assert_eq!(len, 1);
        assert_eq!(truncated, 100);
        
        let (truncated, len) = packet_number::encode(0x1234, 0);
        assert_eq!(len, 2);
        assert_eq!(truncated, 0x1234);
    }
    
    #[test]
    fn test_long_header() {
        let dcid = ConnectionId::from_bytes(&[1, 2, 3, 4]).unwrap();
        let scid = ConnectionId::from_bytes(&[5, 6, 7, 8]).unwrap();
        
        let header = LongHeader::initial(dcid, scid, 0);
        assert_eq!(header.packet_type, PacketType::Initial);
        assert_eq!(header.version, QUIC_VERSION_1);
    }
    
    #[test]
    fn test_short_header() {
        let dcid = ConnectionId::from_bytes(&[1, 2, 3, 4]).unwrap();
        let header = ShortHeader::new(dcid, 42, false);
        
        assert_eq!(header.packet_number, 42);
        assert!(!header.key_phase);
    }
}
