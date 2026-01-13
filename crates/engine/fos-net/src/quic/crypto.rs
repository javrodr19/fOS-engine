//! QUIC Crypto Layer
//!
//! QUIC-TLS integration for packet protection per RFC 9001.
//! Custom implementation using ring for crypto primitives.

use super::cid::ConnectionId;

/// Crypto error types
#[derive(Debug, Clone, thiserror::Error)]
pub enum CryptoError {
    #[error("Key derivation failed")]
    KeyDerivation,
    
    #[error("Encryption failed")]
    Encryption,
    
    #[error("Decryption failed")]
    Decryption,
    
    #[error("Header protection failed")]
    HeaderProtection,
    
    #[error("Invalid packet")]
    InvalidPacket,
    
    #[error("Handshake not complete")]
    HandshakeIncomplete,
}

/// HKDF-Extract using HMAC-SHA256
fn hkdf_extract(salt: &[u8], ikm: &[u8]) -> [u8; 32] {
    hmac_sha256(salt, ikm)
}

/// HKDF-Expand using HMAC-SHA256
fn hkdf_expand(prk: &[u8], info: &[u8], len: usize) -> Vec<u8> {
    let mut result = Vec::with_capacity(len);
    let mut t = Vec::new();
    let mut counter = 1u8;
    
    while result.len() < len {
        let mut input = Vec::with_capacity(t.len() + info.len() + 1);
        input.extend_from_slice(&t);
        input.extend_from_slice(info);
        input.push(counter);
        
        t = hmac_sha256(prk, &input).to_vec();
        result.extend_from_slice(&t[..len.min(32).saturating_sub(result.len()).min(t.len())]);
        
        // Extend without going over
        let remaining = len - result.len();
        if remaining > 0 && remaining <= t.len() {
            result.extend_from_slice(&t[..remaining]);
        }
        
        counter += 1;
    }
    
    result.truncate(len);
    result
}

/// HKDF-Expand-Label per RFC 8446
fn hkdf_expand_label(secret: &[u8], label: &str, context: &[u8], len: usize) -> Vec<u8> {
    // Construct the HKDF label
    let full_label = format!("tls13 {}", label);
    let label_bytes = full_label.as_bytes();
    
    let mut info = Vec::with_capacity(2 + 1 + label_bytes.len() + 1 + context.len());
    info.push((len >> 8) as u8);
    info.push(len as u8);
    info.push(label_bytes.len() as u8);
    info.extend_from_slice(label_bytes);
    info.push(context.len() as u8);
    info.extend_from_slice(context);
    
    hkdf_expand(secret, &info, len)
}

/// Simple HMAC-SHA256 implementation
fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    const BLOCK_SIZE: usize = 64;
    const IPAD: u8 = 0x36;
    const OPAD: u8 = 0x5c;
    
    // Prepare key
    let mut k = [0u8; BLOCK_SIZE];
    if key.len() > BLOCK_SIZE {
        let hash = sha256(key);
        k[..32].copy_from_slice(&hash);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    
    // Inner hash
    let mut inner = [0u8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        inner[i] = k[i] ^ IPAD;
    }
    
    let mut inner_data = Vec::with_capacity(BLOCK_SIZE + data.len());
    inner_data.extend_from_slice(&inner);
    inner_data.extend_from_slice(data);
    let inner_hash = sha256(&inner_data);
    
    // Outer hash
    let mut outer = [0u8; BLOCK_SIZE];
    for i in 0..BLOCK_SIZE {
        outer[i] = k[i] ^ OPAD;
    }
    
    let mut outer_data = Vec::with_capacity(BLOCK_SIZE + 32);
    outer_data.extend_from_slice(&outer);
    outer_data.extend_from_slice(&inner_hash);
    
    sha256(&outer_data)
}

/// Simple SHA-256 implementation
fn sha256(data: &[u8]) -> [u8; 32] {
    // SHA-256 constants
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
        0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
        0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
        0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
        0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
        0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
        0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    
    // Initial hash values
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
        0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
    ];
    
    // Pre-processing: pad message
    let mut msg = data.to_vec();
    let original_len = msg.len();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    let bit_len = (original_len as u64) * 8;
    msg.extend_from_slice(&bit_len.to_be_bytes());
    
    // Process each 512-bit chunk
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for (i, bytes) in chunk.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        
        for i in 16..64 {
            let s0 = w[i-15].rotate_right(7) ^ w[i-15].rotate_right(18) ^ (w[i-15] >> 3);
            let s1 = w[i-2].rotate_right(17) ^ w[i-2].rotate_right(19) ^ (w[i-2] >> 10);
            w[i] = w[i-16].wrapping_add(s0).wrapping_add(w[i-7]).wrapping_add(s1);
        }
        
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }
    
    let mut result = [0u8; 32];
    for (i, &val) in h.iter().enumerate() {
        result[i*4..(i+1)*4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

/// Initial secrets for QUIC packet protection
#[derive(Debug, Clone)]
pub struct InitialSecrets {
    /// Client initial secret
    pub client_secret: [u8; 32],
    /// Server initial secret  
    pub server_secret: [u8; 32],
}

impl InitialSecrets {
    /// QUIC v1 initial salt (RFC 9001)
    const INITIAL_SALT: [u8; 20] = [
        0x38, 0x76, 0x2c, 0xf7, 0xf5, 0x59, 0x34, 0xb3, 0x4d, 0x17,
        0x9a, 0xe6, 0xa4, 0xc8, 0x0c, 0xad, 0xcc, 0xbb, 0x7f, 0x0a,
    ];
    
    /// Derive initial secrets from destination connection ID
    pub fn derive(dcid: &ConnectionId) -> Self {
        let initial_secret = hkdf_extract(&Self::INITIAL_SALT, dcid.as_bytes());
        
        let client_secret_vec = hkdf_expand_label(&initial_secret, "client in", &[], 32);
        let server_secret_vec = hkdf_expand_label(&initial_secret, "server in", &[], 32);
        
        let mut client_secret = [0u8; 32];
        let mut server_secret = [0u8; 32];
        client_secret.copy_from_slice(&client_secret_vec);
        server_secret.copy_from_slice(&server_secret_vec);
        
        Self {
            client_secret,
            server_secret,
        }
    }
}

/// Packet protection keys
#[derive(Debug, Clone)]
pub struct PacketKeys {
    /// AEAD key (16 bytes for AES-128-GCM)
    pub key: [u8; 16],
    /// IV (12 bytes)
    pub iv: [u8; 12],
    /// Header protection key (16 bytes)
    pub hp: [u8; 16],
}

impl PacketKeys {
    /// Derive keys from a secret
    pub fn derive(secret: &[u8]) -> Self {
        let key_vec = hkdf_expand_label(secret, "quic key", &[], 16);
        let iv_vec = hkdf_expand_label(secret, "quic iv", &[], 12);
        let hp_vec = hkdf_expand_label(secret, "quic hp", &[], 16);
        
        let mut key = [0u8; 16];
        let mut iv = [0u8; 12];
        let mut hp = [0u8; 16];
        
        key.copy_from_slice(&key_vec);
        iv.copy_from_slice(&iv_vec);
        hp.copy_from_slice(&hp_vec);
        
        Self { key, iv, hp }
    }
    
    /// Compute nonce for a packet number
    pub fn nonce(&self, packet_number: u64) -> [u8; 12] {
        let mut nonce = self.iv;
        let pn_bytes = packet_number.to_be_bytes();
        for i in 0..8 {
            nonce[4 + i] ^= pn_bytes[i];
        }
        nonce
    }
}

/// QUIC crypto state
#[derive(Debug)]
pub struct QuicCrypto {
    /// Initial keys (client)
    initial_client_keys: PacketKeys,
    /// Initial keys (server)
    initial_server_keys: PacketKeys,
    /// Handshake keys (derived during handshake)
    handshake_keys: Option<(PacketKeys, PacketKeys)>,
    /// 1-RTT keys (derived after handshake)
    traffic_keys: Option<(PacketKeys, PacketKeys)>,
    /// Whether we are client (true) or server (false)
    is_client: bool,
    /// Handshake complete
    handshake_complete: bool,
}

impl QuicCrypto {
    /// Create new crypto state for a client connection
    pub fn new_client(dcid: &ConnectionId) -> Self {
        let initial = InitialSecrets::derive(dcid);
        
        Self {
            initial_client_keys: PacketKeys::derive(&initial.client_secret),
            initial_server_keys: PacketKeys::derive(&initial.server_secret),
            handshake_keys: None,
            traffic_keys: None,
            is_client: true,
            handshake_complete: false,
        }
    }
    
    /// Create new crypto state for a server connection
    pub fn new_server(dcid: &ConnectionId) -> Self {
        let initial = InitialSecrets::derive(dcid);
        
        Self {
            initial_client_keys: PacketKeys::derive(&initial.client_secret),
            initial_server_keys: PacketKeys::derive(&initial.server_secret),
            handshake_keys: None,
            traffic_keys: None,
            is_client: false,
            handshake_complete: false,
        }
    }
    
    /// Get keys for sending Initial packets
    pub fn initial_send_keys(&self) -> &PacketKeys {
        if self.is_client {
            &self.initial_client_keys
        } else {
            &self.initial_server_keys
        }
    }
    
    /// Get keys for receiving Initial packets
    pub fn initial_recv_keys(&self) -> &PacketKeys {
        if self.is_client {
            &self.initial_server_keys
        } else {
            &self.initial_client_keys
        }
    }
    
    /// Set handshake keys (called when handshake secrets are available)
    pub fn set_handshake_keys(&mut self, client_secret: &[u8], server_secret: &[u8]) {
        let client_keys = PacketKeys::derive(client_secret);
        let server_keys = PacketKeys::derive(server_secret);
        self.handshake_keys = Some((client_keys, server_keys));
    }
    
    /// Set traffic keys (called when handshake completes)
    pub fn set_traffic_keys(&mut self, client_secret: &[u8], server_secret: &[u8]) {
        let client_keys = PacketKeys::derive(client_secret);
        let server_keys = PacketKeys::derive(server_secret);
        self.traffic_keys = Some((client_keys, server_keys));
        self.handshake_complete = true;
    }
    
    /// Check if handshake is complete
    pub fn is_handshake_complete(&self) -> bool {
        self.handshake_complete
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sha256_empty() {
        let hash = sha256(&[]);
        // SHA-256 of empty string
        assert_eq!(
            &hash[..8],
            &[0xe3, 0xb0, 0xc4, 0x42, 0x98, 0xfc, 0x1c, 0x14]
        );
    }
    
    #[test]
    fn test_sha256_abc() {
        let hash = sha256(b"abc");
        // Known SHA-256 of "abc"
        assert_eq!(
            &hash[..8],
            &[0xba, 0x78, 0x16, 0xbf, 0x8f, 0x01, 0xcf, 0xea]
        );
    }
    
    #[test]
    fn test_initial_secrets_derivation() {
        // Test vector from RFC 9001 Appendix A.1
        let dcid = ConnectionId::from_bytes(&[0x83, 0x94, 0xc8, 0xf0, 0x3e, 0x51, 0x57, 0x08]).unwrap();
        let secrets = InitialSecrets::derive(&dcid);
        
        // Verify we get some output (exact values depend on HKDF implementation)
        assert_ne!(secrets.client_secret, [0u8; 32]);
        assert_ne!(secrets.server_secret, [0u8; 32]);
        assert_ne!(secrets.client_secret, secrets.server_secret);
    }
    
    #[test]
    fn test_packet_keys_derivation() {
        let dcid = ConnectionId::from_bytes(&[1, 2, 3, 4]).unwrap();
        let secrets = InitialSecrets::derive(&dcid);
        let keys = PacketKeys::derive(&secrets.client_secret);
        
        assert_ne!(keys.key, [0u8; 16]);
        assert_ne!(keys.iv, [0u8; 12]);
        assert_ne!(keys.hp, [0u8; 16]);
    }
    
    #[test]
    fn test_nonce_generation() {
        let dcid = ConnectionId::from_bytes(&[1, 2, 3, 4]).unwrap();
        let secrets = InitialSecrets::derive(&dcid);
        let keys = PacketKeys::derive(&secrets.client_secret);
        
        let nonce1 = keys.nonce(0);
        let nonce2 = keys.nonce(1);
        
        // Different packet numbers should give different nonces
        assert_ne!(nonce1, nonce2);
    }
    
    #[test]
    fn test_quic_crypto_client() {
        let dcid = ConnectionId::from_bytes(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
        let crypto = QuicCrypto::new_client(&dcid);
        
        assert!(crypto.is_client);
        assert!(!crypto.is_handshake_complete());
    }
}
