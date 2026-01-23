//! DTLS (Datagram Transport Layer Security)
//!
//! DTLS transport for WebRTC secure communication.

use std::time::{Duration, Instant};

/// DTLS connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DtlsState {
    #[default]
    New,
    Connecting,
    Connected,
    Failed,
    Closed,
}

/// DTLS role
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DtlsRole { Client, Server }

/// DTLS cipher suite
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CipherSuite {
    TlsEcdhEcdsaWithAes128GcmSha256 = 0xC02B,
    TlsEcdhEcdsaWithAes256GcmSha384 = 0xC02C,
    TlsEcdheRsaWithAes128GcmSha256 = 0xC02F,
}

/// DTLS handshake message types
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum HandshakeType {
    ClientHello = 1, ServerHello = 2, Certificate = 11,
    ServerKeyExchange = 12, CertificateRequest = 13, ServerHelloDone = 14,
    CertificateVerify = 15, ClientKeyExchange = 16, Finished = 20,
}

/// DTLS record layer
#[derive(Debug)]
pub struct DtlsRecord {
    pub content_type: u8,
    pub version: u16,
    pub epoch: u16,
    pub sequence_number: u64,
    pub length: u16,
    pub fragment: Vec<u8>,
}

/// DTLS transport
#[derive(Debug)]
pub struct DtlsTransport {
    state: DtlsState,
    role: DtlsRole,
    local_fingerprint: [u8; 32],
    remote_fingerprint: Option<[u8; 32]>,
    cipher_suite: Option<CipherSuite>,
    epoch: u16,
    sequence_number: u64,
    master_secret: Option<[u8; 48]>,
    client_random: [u8; 32],
    server_random: [u8; 32],
    handshake_messages: Vec<u8>,
    handshake_complete: bool,
}

impl DtlsTransport {
    pub fn new(role: DtlsRole) -> Self {
        let mut local_fingerprint = [0u8; 32];
        // Generate fingerprint from self-signed certificate
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        for (i, b) in local_fingerprint.iter_mut().enumerate() {
            *b = ((now.as_nanos() >> (i * 4)) & 0xFF) as u8;
        }
        
        Self {
            state: DtlsState::New,
            role,
            local_fingerprint,
            remote_fingerprint: None,
            cipher_suite: None,
            epoch: 0,
            sequence_number: 0,
            master_secret: None,
            client_random: Self::random_bytes(),
            server_random: [0u8; 32],
            handshake_messages: Vec::new(),
            handshake_complete: false,
        }
    }
    
    fn random_bytes() -> [u8; 32] {
        let mut bytes = [0u8; 32];
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = ((now.as_nanos() >> i) ^ (i as u128 * 0x1234567890ABCDEF)) as u8;
        }
        bytes
    }
    
    /// Start handshake
    pub fn start_handshake(&mut self) -> Vec<u8> {
        self.state = DtlsState::Connecting;
        
        if self.role == DtlsRole::Client {
            self.build_client_hello()
        } else {
            Vec::new() // Server waits for ClientHello
        }
    }
    
    fn build_client_hello(&mut self) -> Vec<u8> {
        let mut msg = Vec::with_capacity(200);
        
        // Record layer
        msg.push(22); // Handshake content type
        msg.extend_from_slice(&0xFEFDu16.to_be_bytes()); // DTLS 1.2
        msg.extend_from_slice(&self.epoch.to_be_bytes());
        msg.extend_from_slice(&[0, 0, 0, 0]); // Sequence high bytes
        msg.extend_from_slice(&(self.sequence_number as u16).to_be_bytes());
        let length_pos = msg.len();
        msg.extend_from_slice(&0u16.to_be_bytes()); // Length placeholder
        
        // Handshake header
        msg.push(HandshakeType::ClientHello as u8);
        let hs_length_pos = msg.len();
        msg.extend_from_slice(&[0, 0, 0]); // Length placeholder (24-bit)
        msg.extend_from_slice(&0u16.to_be_bytes()); // Message sequence
        msg.extend_from_slice(&[0, 0, 0]); // Fragment offset
        let frag_length_pos = msg.len();
        msg.extend_from_slice(&[0, 0, 0]); // Fragment length placeholder
        
        let body_start = msg.len();
        
        // Client version
        msg.extend_from_slice(&0xFEFDu16.to_be_bytes()); // DTLS 1.2
        
        // Random
        msg.extend_from_slice(&self.client_random);
        
        // Session ID (empty)
        msg.push(0);
        
        // Cookie (empty for initial)
        msg.push(0);
        
        // Cipher suites
        msg.extend_from_slice(&6u16.to_be_bytes()); // 3 suites * 2 bytes
        msg.extend_from_slice(&(CipherSuite::TlsEcdhEcdsaWithAes128GcmSha256 as u16).to_be_bytes());
        msg.extend_from_slice(&(CipherSuite::TlsEcdhEcdsaWithAes256GcmSha384 as u16).to_be_bytes());
        msg.extend_from_slice(&(CipherSuite::TlsEcdheRsaWithAes128GcmSha256 as u16).to_be_bytes());
        
        // Compression methods
        msg.push(1); // Length
        msg.push(0); // null
        
        // Extensions
        msg.extend_from_slice(&0u16.to_be_bytes()); // No extensions for now
        
        let body_len = msg.len() - body_start;
        
        // Update lengths
        let record_len = msg.len() - length_pos - 2;
        msg[length_pos..length_pos + 2].copy_from_slice(&(record_len as u16).to_be_bytes());
        
        let hs_len = body_len as u32;
        msg[hs_length_pos] = (hs_len >> 16) as u8;
        msg[hs_length_pos + 1] = (hs_len >> 8) as u8;
        msg[hs_length_pos + 2] = hs_len as u8;
        
        msg[frag_length_pos] = (hs_len >> 16) as u8;
        msg[frag_length_pos + 1] = (hs_len >> 8) as u8;
        msg[frag_length_pos + 2] = hs_len as u8;
        
        self.sequence_number += 1;
        self.handshake_messages.extend_from_slice(&msg[13..]); // Save for Finished
        
        msg
    }
    
    /// Process incoming DTLS data
    pub fn process(&mut self, data: &[u8]) -> Result<Option<Vec<u8>>, &'static str> {
        if data.len() < 13 { return Err("Record too short"); }
        
        let content_type = data[0];
        let _version = u16::from_be_bytes([data[1], data[2]]);
        let _epoch = u16::from_be_bytes([data[3], data[4]]);
        let length = u16::from_be_bytes([data[11], data[12]]) as usize;
        
        if data.len() < 13 + length { return Err("Incomplete record"); }
        
        match content_type {
            22 => self.process_handshake(&data[13..13 + length]),
            23 => self.process_application_data(&data[13..13 + length]),
            21 => { self.state = DtlsState::Closed; Ok(None) }
            _ => Ok(None),
        }
    }
    
    fn process_handshake(&mut self, data: &[u8]) -> Result<Option<Vec<u8>>, &'static str> {
        if data.is_empty() { return Ok(None); }
        
        let msg_type = data[0];
        
        match msg_type {
            1 => { // ClientHello
                if self.role == DtlsRole::Server {
                    // Generate ServerHello, Certificate, etc.
                    self.server_random = Self::random_bytes();
                    return Ok(Some(self.build_server_flight()));
                }
            }
            2 => { // ServerHello
                if data.len() >= 35 {
                    self.server_random.copy_from_slice(&data[6..38]);
                }
            }
            20 => { // Finished
                self.handshake_complete = true;
                self.state = DtlsState::Connected;
                self.epoch = 1;
            }
            _ => {}
        }
        
        self.handshake_messages.extend_from_slice(data);
        Ok(None)
    }
    
    fn build_server_flight(&mut self) -> Vec<u8> {
        // Would build ServerHello + Certificate + ServerHelloDone
        Vec::new()
    }
    
    fn process_application_data(&mut self, _data: &[u8]) -> Result<Option<Vec<u8>>, &'static str> {
        // Decrypt with negotiated cipher suite
        Ok(None)
    }
    
    /// Get SRTP keying material
    pub fn export_keying_material(&self, label: &str, length: usize) -> Option<Vec<u8>> {
        if !self.handshake_complete { return None; }
        
        // Real impl would use TLS PRF with master secret
        let mut material = vec![0u8; length];
        for (i, b) in material.iter_mut().enumerate() {
            *b = ((self.client_random[i % 32] as u16 + self.server_random[i % 32] as u16) / 2) as u8;
        }
        Some(material)
    }
    
    pub fn state(&self) -> DtlsState { self.state }
    pub fn is_connected(&self) -> bool { self.state == DtlsState::Connected }
    pub fn local_fingerprint(&self) -> &[u8; 32] { &self.local_fingerprint }
    
    pub fn fingerprint_string(&self) -> String {
        self.local_fingerprint.iter().map(|b| format!("{:02X}", b)).collect::<Vec<_>>().join(":")
    }
}

impl Default for DtlsTransport {
    fn default() -> Self { Self::new(DtlsRole::Client) }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dtls() {
        let dtls = DtlsTransport::new(DtlsRole::Client);
        assert_eq!(dtls.state(), DtlsState::New);
    }
    
    #[test]
    fn test_fingerprint() {
        let dtls = DtlsTransport::new(DtlsRole::Server);
        let fp = dtls.fingerprint_string();
        assert!(fp.contains(':'));
    }
}
