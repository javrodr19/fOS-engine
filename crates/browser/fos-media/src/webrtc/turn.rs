//! TURN (Traversal Using Relays around NAT)
//!
//! TURN client for relay candidate allocation.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// TURN message types (extends STUN)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum TurnMessageType {
    Allocate = 0x0003,
    AllocateSuccess = 0x0103,
    AllocateError = 0x0113,
    Refresh = 0x0004,
    RefreshSuccess = 0x0104,
    Send = 0x0006,
    Data = 0x0007,
    CreatePermission = 0x0008,
    CreatePermissionSuccess = 0x0108,
    ChannelBind = 0x0009,
    ChannelBindSuccess = 0x0109,
}

/// TURN attribute types
#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum TurnAttrType {
    ChannelNumber = 0x000C,
    Lifetime = 0x000D,
    XorPeerAddress = 0x0012,
    Data = 0x0013,
    XorRelayedAddress = 0x0016,
    RequestedTransport = 0x0019,
    DontFragment = 0x001A,
    ReservationToken = 0x0022,
}

/// TURN allocation state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocationState {
    None,
    Allocating,
    Allocated,
    Expired,
    Failed,
}

/// TURN channel binding
#[derive(Debug, Clone)]
pub struct ChannelBinding {
    pub channel_number: u16,
    pub peer_address: SocketAddr,
    pub expires_at: Instant,
}

/// TURN permission
#[derive(Debug, Clone)]
pub struct Permission {
    pub peer_address: SocketAddr,
    pub expires_at: Instant,
}

/// TURN client
#[derive(Debug)]
pub struct TurnClient {
    server_addr: SocketAddr,
    username: String,
    password: String,
    realm: Option<String>,
    nonce: Option<String>,
    state: AllocationState,
    relayed_address: Option<SocketAddr>,
    mapped_address: Option<SocketAddr>,
    lifetime: Duration,
    allocated_at: Option<Instant>,
    channels: Vec<ChannelBinding>,
    permissions: Vec<Permission>,
    next_channel: u16,
    transaction_id: [u8; 12],
}

impl TurnClient {
    pub fn new(server: SocketAddr, username: String, password: String) -> Self {
        Self {
            server_addr: server,
            username,
            password,
            realm: None,
            nonce: None,
            state: AllocationState::None,
            relayed_address: None,
            mapped_address: None,
            lifetime: Duration::from_secs(600),
            allocated_at: None,
            channels: Vec::new(),
            permissions: Vec::new(),
            next_channel: 0x4000, // Channel numbers start at 0x4000
            transaction_id: Self::new_transaction_id(),
        }
    }
    
    fn new_transaction_id() -> [u8; 12] {
        let mut tid = [0u8; 12];
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        tid[0..8].copy_from_slice(&now.as_nanos().to_le_bytes()[..8]);
        tid[8..12].copy_from_slice(&(now.subsec_nanos() ^ 0xDEADBEEF).to_le_bytes());
        tid
    }
    
    /// Build Allocate request
    pub fn build_allocate_request(&mut self) -> Vec<u8> {
        self.transaction_id = Self::new_transaction_id();
        let mut msg = Vec::with_capacity(100);
        
        // STUN header
        msg.extend_from_slice(&(TurnMessageType::Allocate as u16).to_be_bytes());
        msg.extend_from_slice(&0u16.to_be_bytes()); // Length placeholder
        msg.extend_from_slice(&0x2112A442u32.to_be_bytes()); // Magic cookie
        msg.extend_from_slice(&self.transaction_id);
        
        // REQUESTED-TRANSPORT (UDP = 17)
        msg.extend_from_slice(&(TurnAttrType::RequestedTransport as u16).to_be_bytes());
        msg.extend_from_slice(&4u16.to_be_bytes());
        msg.push(17); // UDP protocol number
        msg.extend_from_slice(&[0, 0, 0]); // RFFU
        
        // LIFETIME
        msg.extend_from_slice(&(TurnAttrType::Lifetime as u16).to_be_bytes());
        msg.extend_from_slice(&4u16.to_be_bytes());
        msg.extend_from_slice(&(self.lifetime.as_secs() as u32).to_be_bytes());
        
        // Add MESSAGE-INTEGRITY and FINGERPRINT if we have realm/nonce
        if let (Some(ref realm), Some(ref nonce)) = (&self.realm, &self.nonce) {
            // USERNAME
            self.add_string_attr(&mut msg, 0x0006, &self.username);
            // REALM  
            self.add_string_attr(&mut msg, 0x0014, realm);
            // NONCE
            self.add_string_attr(&mut msg, 0x0015, nonce);
            // MESSAGE-INTEGRITY would be computed here with HMAC-SHA1
        }
        
        // Update length
        let len = (msg.len() - 20) as u16;
        msg[2..4].copy_from_slice(&len.to_be_bytes());
        
        self.state = AllocationState::Allocating;
        msg
    }
    
    fn add_string_attr(&self, msg: &mut Vec<u8>, attr_type: u16, value: &str) {
        msg.extend_from_slice(&attr_type.to_be_bytes());
        let padded_len = (value.len() + 3) & !3;
        msg.extend_from_slice(&(value.len() as u16).to_be_bytes());
        msg.extend_from_slice(value.as_bytes());
        // Padding
        for _ in value.len()..padded_len {
            msg.push(0);
        }
    }
    
    /// Process Allocate response
    pub fn process_allocate_response(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.len() < 20 { return Err("Response too short"); }
        
        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        
        if msg_type == TurnMessageType::AllocateError as u16 {
            // Parse 401 Unauthorized to get realm/nonce
            let mut pos = 20;
            while pos + 4 <= data.len() {
                let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
                let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
                pos += 4;
                
                if pos + attr_len > data.len() { break; }
                
                match attr_type {
                    0x0014 => { // REALM
                        self.realm = Some(String::from_utf8_lossy(&data[pos..pos + attr_len]).trim_end_matches('\0').to_string());
                    }
                    0x0015 => { // NONCE
                        self.nonce = Some(String::from_utf8_lossy(&data[pos..pos + attr_len]).trim_end_matches('\0').to_string());
                    }
                    _ => {}
                }
                pos += (attr_len + 3) & !3;
            }
            return Err("Need authentication");
        }
        
        if msg_type != TurnMessageType::AllocateSuccess as u16 {
            self.state = AllocationState::Failed;
            return Err("Allocation failed");
        }
        
        // Parse success response
        let mut pos = 20;
        while pos + 4 <= data.len() {
            let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;
            pos += 4;
            
            if pos + attr_len > data.len() { break; }
            
            match attr_type {
                0x0016 => { // XOR-RELAYED-ADDRESS
                    if attr_len >= 8 {
                        let xport = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) ^ 0x2112;
                        let xip = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]) ^ 0x2112A442;
                        self.relayed_address = Some(SocketAddr::new(
                            std::net::IpAddr::V4(std::net::Ipv4Addr::from(xip)), xport));
                    }
                }
                0x000D => { // LIFETIME
                    if attr_len >= 4 {
                        let secs = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
                        self.lifetime = Duration::from_secs(secs as u64);
                    }
                }
                _ => {}
            }
            pos += (attr_len + 3) & !3;
        }
        
        self.state = AllocationState::Allocated;
        self.allocated_at = Some(Instant::now());
        Ok(())
    }
    
    /// Build CreatePermission request
    pub fn build_create_permission(&mut self, peer: SocketAddr) -> Vec<u8> {
        self.transaction_id = Self::new_transaction_id();
        let mut msg = Vec::with_capacity(60);
        
        msg.extend_from_slice(&(TurnMessageType::CreatePermission as u16).to_be_bytes());
        msg.extend_from_slice(&0u16.to_be_bytes());
        msg.extend_from_slice(&0x2112A442u32.to_be_bytes());
        msg.extend_from_slice(&self.transaction_id);
        
        // XOR-PEER-ADDRESS
        msg.extend_from_slice(&(TurnAttrType::XorPeerAddress as u16).to_be_bytes());
        msg.extend_from_slice(&8u16.to_be_bytes());
        msg.push(0); // Reserved
        msg.push(1); // IPv4
        let port = peer.port() ^ 0x2112;
        msg.extend_from_slice(&port.to_be_bytes());
        if let std::net::IpAddr::V4(ip) = peer.ip() {
            let xip = u32::from(ip) ^ 0x2112A442;
            msg.extend_from_slice(&xip.to_be_bytes());
        }
        
        let len = (msg.len() - 20) as u16;
        msg[2..4].copy_from_slice(&len.to_be_bytes());
        msg
    }
    
    /// Build ChannelBind request
    pub fn build_channel_bind(&mut self, peer: SocketAddr) -> (u16, Vec<u8>) {
        let channel = self.next_channel;
        self.next_channel += 1;
        self.transaction_id = Self::new_transaction_id();
        
        let mut msg = Vec::with_capacity(60);
        msg.extend_from_slice(&(TurnMessageType::ChannelBind as u16).to_be_bytes());
        msg.extend_from_slice(&0u16.to_be_bytes());
        msg.extend_from_slice(&0x2112A442u32.to_be_bytes());
        msg.extend_from_slice(&self.transaction_id);
        
        // CHANNEL-NUMBER
        msg.extend_from_slice(&(TurnAttrType::ChannelNumber as u16).to_be_bytes());
        msg.extend_from_slice(&4u16.to_be_bytes());
        msg.extend_from_slice(&channel.to_be_bytes());
        msg.extend_from_slice(&[0, 0]); // RFFU
        
        // XOR-PEER-ADDRESS
        msg.extend_from_slice(&(TurnAttrType::XorPeerAddress as u16).to_be_bytes());
        msg.extend_from_slice(&8u16.to_be_bytes());
        msg.push(0);
        msg.push(1);
        let port = peer.port() ^ 0x2112;
        msg.extend_from_slice(&port.to_be_bytes());
        if let std::net::IpAddr::V4(ip) = peer.ip() {
            let xip = u32::from(ip) ^ 0x2112A442;
            msg.extend_from_slice(&xip.to_be_bytes());
        }
        
        let len = (msg.len() - 20) as u16;
        msg[2..4].copy_from_slice(&len.to_be_bytes());
        
        (channel, msg)
    }
    
    /// Build ChannelData message
    pub fn build_channel_data(&self, channel: u16, data: &[u8]) -> Vec<u8> {
        let mut msg = Vec::with_capacity(4 + data.len());
        msg.extend_from_slice(&channel.to_be_bytes());
        msg.extend_from_slice(&(data.len() as u16).to_be_bytes());
        msg.extend_from_slice(data);
        // Padding to 4-byte boundary
        while msg.len() % 4 != 0 { msg.push(0); }
        msg
    }
    
    pub fn state(&self) -> AllocationState { self.state }
    pub fn relayed_address(&self) -> Option<SocketAddr> { self.relayed_address }
    pub fn is_expired(&self) -> bool {
        self.allocated_at.map(|t| t.elapsed() > self.lifetime).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    
    #[test]
    fn test_turn_client() {
        let server = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 3478);
        let client = TurnClient::new(server, "user".into(), "pass".into());
        assert_eq!(client.state(), AllocationState::None);
    }
    
    #[test]
    fn test_allocate_request() {
        let server = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4)), 3478);
        let mut client = TurnClient::new(server, "user".into(), "pass".into());
        let req = client.build_allocate_request();
        assert!(req.len() >= 20);
        assert_eq!(client.state(), AllocationState::Allocating);
    }
}
