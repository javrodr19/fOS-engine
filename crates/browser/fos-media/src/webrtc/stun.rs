//! STUN (Session Traversal Utilities for NAT)
//!
//! STUN client for server-reflexive candidate gathering.

use std::net::SocketAddr;

/// STUN message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum StunMessageType { BindingRequest = 0x0001, BindingSuccess = 0x0101, BindingError = 0x0111 }

/// STUN attribute types
#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum StunAttrType { MappedAddress = 0x0001, XorMappedAddress = 0x0020, Username = 0x0006, MessageIntegrity = 0x0008, Fingerprint = 0x8028 }

/// STUN message
#[derive(Debug)]
pub struct StunMessage {
    pub msg_type: StunMessageType,
    pub transaction_id: [u8; 12],
    pub attributes: Vec<StunAttribute>,
}

/// STUN attribute
#[derive(Debug, Clone)]
pub enum StunAttribute {
    MappedAddress(SocketAddr),
    XorMappedAddress(SocketAddr),
    Username(String),
    MessageIntegrity([u8; 20]),
    Fingerprint(u32),
    Unknown(u16, Vec<u8>),
}

impl StunMessage {
    pub fn binding_request() -> Self {
        let mut tid = [0u8; 12];
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        tid[0..8].copy_from_slice(&now.as_nanos().to_le_bytes()[..8]);
        Self { msg_type: StunMessageType::BindingRequest, transaction_id: tid, attributes: Vec::new() }
    }
    
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(28);
        buf.extend_from_slice(&(self.msg_type as u16).to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes()); // Length placeholder
        buf.extend_from_slice(&0x2112A442u32.to_be_bytes()); // Magic cookie
        buf.extend_from_slice(&self.transaction_id);
        
        for attr in &self.attributes {
            let (atype, data) = match attr {
                StunAttribute::Username(s) => (StunAttrType::Username as u16, s.as_bytes().to_vec()),
                _ => continue,
            };
            buf.extend_from_slice(&atype.to_be_bytes());
            buf.extend_from_slice(&(data.len() as u16).to_be_bytes());
            buf.extend_from_slice(&data);
            // Padding
            while buf.len() % 4 != 0 { buf.push(0); }
        }
        
        let len = (buf.len() - 20) as u16;
        buf[2..4].copy_from_slice(&len.to_be_bytes());
        buf
    }
    
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() < 20 { return None; }
        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        let _len = u16::from_be_bytes([data[2], data[3]]);
        let _magic = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let mut tid = [0u8; 12];
        tid.copy_from_slice(&data[8..20]);
        
        let msg_type = match msg_type { 0x0001 => StunMessageType::BindingRequest, 0x0101 => StunMessageType::BindingSuccess, _ => StunMessageType::BindingError };
        
        let mut attributes = Vec::new();
        let mut pos = 20;
        while pos + 4 <= data.len() {
            let atype = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let alen = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
            pos += 4;
            if pos + alen > data.len() { break; }
            
            if atype == 0x0020 && alen >= 8 { // XOR-MAPPED-ADDRESS
                let xport = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) ^ 0x2112;
                let xip = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) ^ 0x2112A442;
                let addr = SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::from(xip)), xport);
                attributes.push(StunAttribute::XorMappedAddress(addr));
            }
            
            pos += alen;
            pos = (pos + 3) & !3; // Align
        }
        
        Some(Self { msg_type, transaction_id: tid, attributes })
    }
    
    pub fn get_mapped_address(&self) -> Option<SocketAddr> {
        for attr in &self.attributes {
            match attr {
                StunAttribute::XorMappedAddress(a) | StunAttribute::MappedAddress(a) => return Some(*a),
                _ => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_stun() { let msg = StunMessage::binding_request(); let encoded = msg.encode(); assert!(encoded.len() >= 20); }
}
