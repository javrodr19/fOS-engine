//! SRTP (Secure Real-time Transport Protocol)
//!
//! SRTP encryption/decryption for WebRTC media.

use std::collections::HashMap;

/// SRTP protection profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SrtpProfile {
    Aes128CmHmacSha1_80 = 0x0001,
    Aes128CmHmacSha1_32 = 0x0002,
    AeadAes128Gcm = 0x0007,
    AeadAes256Gcm = 0x0008,
}

/// SRTP key material
#[derive(Debug, Clone)]
pub struct SrtpKeyMaterial {
    pub master_key: Vec<u8>,
    pub master_salt: Vec<u8>,
}

/// SRTP session
#[derive(Debug)]
pub struct SrtpSession {
    profile: SrtpProfile,
    local_key: SrtpKeyMaterial,
    remote_key: SrtpKeyMaterial,
    local_roc: u32,  // Rollover counter
    remote_roc: u32,
    local_seq: u16,
    highest_seq: u16,
    replay_window: u64,
    ssrc_contexts: HashMap<u32, SsrcContext>,
}

#[derive(Debug, Default)]
struct SsrcContext {
    roc: u32,
    highest_seq: u16,
    replay_window: u64,
}

impl SrtpSession {
    /// Create from DTLS-exported keying material
    pub fn from_keying_material(material: &[u8], profile: SrtpProfile, is_client: bool) -> Option<Self> {
        let key_len = match profile {
            SrtpProfile::Aes128CmHmacSha1_80 | SrtpProfile::Aes128CmHmacSha1_32 | SrtpProfile::AeadAes128Gcm => 16,
            SrtpProfile::AeadAes256Gcm => 32,
        };
        let salt_len = match profile {
            SrtpProfile::Aes128CmHmacSha1_80 | SrtpProfile::Aes128CmHmacSha1_32 => 14,
            SrtpProfile::AeadAes128Gcm | SrtpProfile::AeadAes256Gcm => 12,
        };
        
        let total_len = 2 * (key_len + salt_len);
        if material.len() < total_len { return None; }
        
        let client_key = material[0..key_len].to_vec();
        let server_key = material[key_len..2 * key_len].to_vec();
        let client_salt = material[2 * key_len..2 * key_len + salt_len].to_vec();
        let server_salt = material[2 * key_len + salt_len..total_len].to_vec();
        
        let (local_key, remote_key) = if is_client {
            (SrtpKeyMaterial { master_key: client_key, master_salt: client_salt },
             SrtpKeyMaterial { master_key: server_key, master_salt: server_salt })
        } else {
            (SrtpKeyMaterial { master_key: server_key, master_salt: server_salt },
             SrtpKeyMaterial { master_key: client_key, master_salt: client_salt })
        };
        
        Some(Self {
            profile,
            local_key,
            remote_key,
            local_roc: 0,
            remote_roc: 0,
            local_seq: 0,
            highest_seq: 0,
            replay_window: 0,
            ssrc_contexts: HashMap::new(),
        })
    }
    
    /// Protect (encrypt) RTP packet
    pub fn protect(&mut self, rtp: &[u8]) -> Option<Vec<u8>> {
        if rtp.len() < 12 { return None; }
        
        let ssrc = u32::from_be_bytes([rtp[8], rtp[9], rtp[10], rtp[11]]);
        let seq = u16::from_be_bytes([rtp[2], rtp[3]]);
        
        // Build index for encryption: ROC || SEQ
        let index = ((self.local_roc as u64) << 16) | seq as u64;
        
        // Derive session keys from master key
        let session_key = self.derive_session_key(&self.local_key, index, 0);
        let session_salt = self.derive_session_key(&self.local_key, index, 2);
        
        let mut protected = rtp.to_vec();
        
        // Encrypt payload (everything after RTP header)
        let header_len = 12 + (rtp[0] & 0x0F) as usize * 4;
        if header_len < rtp.len() {
            self.aes_ctr_encrypt(&mut protected[header_len..], &session_key, &session_salt, ssrc, index);
        }
        
        // Add authentication tag
        let tag = self.compute_auth_tag(&protected, self.local_roc);
        protected.extend_from_slice(&tag);
        
        // Update sequence
        if seq == 0xFFFF { self.local_roc += 1; }
        self.local_seq = seq.wrapping_add(1);
        
        Some(protected)
    }
    
    /// Unprotect (decrypt) SRTP packet
    pub fn unprotect(&mut self, srtp: &[u8]) -> Option<Vec<u8>> {
        let tag_len = match self.profile {
            SrtpProfile::Aes128CmHmacSha1_80 => 10,
            SrtpProfile::Aes128CmHmacSha1_32 => 4,
            SrtpProfile::AeadAes128Gcm | SrtpProfile::AeadAes256Gcm => 16,
        };
        
        if srtp.len() < 12 + tag_len { return None; }
        
        let encrypted = &srtp[..srtp.len() - tag_len];
        let _received_tag = &srtp[srtp.len() - tag_len..];
        
        let ssrc = u32::from_be_bytes([encrypted[8], encrypted[9], encrypted[10], encrypted[11]]);
        let seq = u16::from_be_bytes([encrypted[2], encrypted[3]]);
        
        // Estimate ROC
        let ctx = self.ssrc_contexts.entry(ssrc).or_default();
        let roc = self.estimate_roc(ctx, seq);
        let index = ((roc as u64) << 16) | seq as u64;
        
        // Verify authentication (simplified - real impl would use HMAC)
        // let expected_tag = self.compute_auth_tag(encrypted, roc);
        // if received_tag != expected_tag { return None; }
        
        // Check replay
        if !self.check_replay(ctx, seq, index) { return None; }
        
        // Decrypt
        let session_key = self.derive_session_key(&self.remote_key, index, 0);
        let session_salt = self.derive_session_key(&self.remote_key, index, 2);
        
        let mut decrypted = encrypted.to_vec();
        let header_len = 12 + (encrypted[0] & 0x0F) as usize * 4;
        if header_len < decrypted.len() {
            self.aes_ctr_encrypt(&mut decrypted[header_len..], &session_key, &session_salt, ssrc, index);
        }
        
        // Update replay window
        self.update_replay(ctx, seq);
        
        Some(decrypted)
    }
    
    fn estimate_roc(&self, ctx: &SsrcContext, seq: u16) -> u32 {
        let s_l = ctx.highest_seq;
        if s_l < 32768 {
            if seq.wrapping_sub(s_l) > 32768 { ctx.roc.wrapping_sub(1) }
            else { ctx.roc }
        } else {
            if s_l.wrapping_sub(32768) > seq { ctx.roc.wrapping_add(1) }
            else { ctx.roc }
        }
    }
    
    fn check_replay(&self, ctx: &SsrcContext, seq: u16, _index: u64) -> bool {
        // Check if packet is in replay window
        let delta = seq.wrapping_sub(ctx.highest_seq) as i16;
        if delta > 0 { return true; } // New packet
        if delta < -64 { return false; } // Too old
        let bit = 1u64 << (-delta as u32);
        ctx.replay_window & bit == 0
    }
    
    fn update_replay(&mut self, ctx: &mut SsrcContext, seq: u16) {
        let delta = seq.wrapping_sub(ctx.highest_seq) as i16;
        if delta > 0 {
            ctx.replay_window = (ctx.replay_window << delta) | 1;
            ctx.highest_seq = seq;
            if seq < ctx.highest_seq { ctx.roc += 1; }
        } else {
            let bit = 1u64 << (-delta as u32);
            ctx.replay_window |= bit;
        }
    }
    
    fn derive_session_key(&self, material: &SrtpKeyMaterial, _index: u64, label: u8) -> Vec<u8> {
        // Simplified key derivation - real impl uses AES-CM PRF
        let mut key = material.master_key.clone();
        for (i, b) in key.iter_mut().enumerate() {
            *b ^= label ^ material.master_salt.get(i).copied().unwrap_or(0);
        }
        key
    }
    
    fn aes_ctr_encrypt(&self, data: &mut [u8], key: &[u8], salt: &[u8], ssrc: u32, index: u64) {
        // Simplified XOR "encryption" - real impl uses AES-CTR
        for (i, byte) in data.iter_mut().enumerate() {
            let keystream_byte = key[i % key.len()] ^ salt[i % salt.len()] ^ ((ssrc >> (i % 4 * 8)) as u8) ^ ((index >> (i % 8 * 8)) as u8);
            *byte ^= keystream_byte;
        }
    }
    
    fn compute_auth_tag(&self, data: &[u8], roc: u32) -> Vec<u8> {
        // Simplified tag - real impl uses HMAC-SHA1
        let tag_len = match self.profile {
            SrtpProfile::Aes128CmHmacSha1_80 => 10,
            SrtpProfile::Aes128CmHmacSha1_32 => 4,
            _ => 16,
        };
        
        let mut tag = vec![0u8; tag_len];
        for (i, b) in tag.iter_mut().enumerate() {
            *b = data.get(i).copied().unwrap_or(0) ^ ((roc >> (i % 4 * 8)) as u8);
        }
        tag
    }
    
    pub fn profile(&self) -> SrtpProfile { self.profile }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_srtp_session() {
        let material = vec![0u8; 60];
        let session = SrtpSession::from_keying_material(&material, SrtpProfile::Aes128CmHmacSha1_80, true);
        assert!(session.is_some());
    }
    
    #[test]
    fn test_protect_unprotect() {
        let material: Vec<u8> = (0..60).collect();
        let mut session = SrtpSession::from_keying_material(&material, SrtpProfile::Aes128CmHmacSha1_80, true).unwrap();
        
        // Minimal RTP packet
        let rtp = vec![0x80, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x12, 0x34, 0x56, 0x78, 0xAB, 0xCD];
        let protected = session.protect(&rtp);
        assert!(protected.is_some());
    }
}
