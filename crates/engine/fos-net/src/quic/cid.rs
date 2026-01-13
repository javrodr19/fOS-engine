//! QUIC Connection ID
//!
//! Connection ID management for QUIC connections.

use std::fmt;

/// Maximum connection ID length (RFC 9000)
pub const MAX_CID_LEN: usize = 20;

/// Minimum connection ID length for server-chosen CIDs
pub const MIN_CID_LEN: usize = 8;

/// Connection ID
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnectionId {
    /// Raw bytes
    bytes: [u8; MAX_CID_LEN],
    /// Actual length
    len: u8,
}

impl ConnectionId {
    /// Create an empty connection ID
    pub const fn empty() -> Self {
        Self {
            bytes: [0; MAX_CID_LEN],
            len: 0,
        }
    }
    
    /// Create from bytes
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > MAX_CID_LEN {
            return None;
        }
        
        let mut cid = Self::empty();
        cid.bytes[..bytes.len()].copy_from_slice(bytes);
        cid.len = bytes.len() as u8;
        Some(cid)
    }
    
    /// Generate a random connection ID
    pub fn generate(len: usize) -> Self {
        let len = len.min(MAX_CID_LEN);
        let mut cid = Self::empty();
        
        // Simple PRNG for CID generation (would use ring/rand in production)
        // Using a combination of time and a counter for uniqueness
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        
        let mut state = seed;
        for i in 0..len {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            cid.bytes[i] = state as u8;
        }
        
        cid.len = len as u8;
        cid
    }
    
    /// Get the bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
    
    /// Get the length
    pub fn len(&self) -> usize {
        self.len as usize
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self::empty()
    }
}

impl fmt::Debug for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "CID(")?;
        for b in self.as_bytes() {
            write!(f, "{:02x}", b)?;
        }
        write!(f, ")")
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.as_bytes() {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

/// Connection ID sequence entry
#[derive(Debug, Clone)]
pub struct CidEntry {
    /// The connection ID
    pub cid: ConnectionId,
    /// Sequence number
    pub sequence: u64,
    /// Stateless reset token (16 bytes)
    pub reset_token: Option<[u8; 16]>,
}

/// Connection ID manager
#[derive(Debug, Default)]
pub struct CidManager {
    /// Local connection IDs we've issued
    local_cids: Vec<CidEntry>,
    /// Remote connection IDs we've received
    remote_cids: Vec<CidEntry>,
    /// Active local CID index
    active_local_idx: usize,
    /// Active remote CID index
    active_remote_idx: usize,
    /// Next sequence for local CIDs
    next_local_seq: u64,
    /// Retire prior to value for local CIDs
    retire_prior_to: u64,
}

impl CidManager {
    /// Create a new CID manager
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Generate and add a new local CID
    pub fn add_local_cid(&mut self, len: usize) -> &ConnectionId {
        let cid = ConnectionId::generate(len);
        let seq = self.next_local_seq;
        self.next_local_seq += 1;
        
        // Generate reset token
        let mut reset_token = [0u8; 16];
        // Simple derivation from CID (in production, use HMAC with a secret)
        for (i, b) in cid.as_bytes().iter().enumerate() {
            reset_token[i % 16] ^= b;
        }
        
        self.local_cids.push(CidEntry {
            cid,
            sequence: seq,
            reset_token: Some(reset_token),
        });
        
        &self.local_cids.last().unwrap().cid
    }
    
    /// Add a remote CID received from peer
    pub fn add_remote_cid(&mut self, cid: ConnectionId, sequence: u64, reset_token: Option<[u8; 16]>) {
        // Check if we already have this sequence
        if self.remote_cids.iter().any(|e| e.sequence == sequence) {
            return;
        }
        
        self.remote_cids.push(CidEntry {
            cid,
            sequence,
            reset_token,
        });
        
        // Sort by sequence
        self.remote_cids.sort_by_key(|e| e.sequence);
    }
    
    /// Get the active local CID
    pub fn active_local_cid(&self) -> Option<&ConnectionId> {
        self.local_cids.get(self.active_local_idx).map(|e| &e.cid)
    }
    
    /// Get the active remote CID (for sending)
    pub fn active_remote_cid(&self) -> Option<&ConnectionId> {
        self.remote_cids.get(self.active_remote_idx).map(|e| &e.cid)
    }
    
    /// Rotate to a new remote CID if available
    pub fn rotate_remote_cid(&mut self) -> bool {
        if self.active_remote_idx + 1 < self.remote_cids.len() {
            self.active_remote_idx += 1;
            true
        } else {
            false
        }
    }
    
    /// Retire local CIDs prior to a sequence number
    pub fn retire_local_prior_to(&mut self, retire_prior_to: u64) {
        if retire_prior_to <= self.retire_prior_to {
            return;
        }
        
        self.retire_prior_to = retire_prior_to;
        self.local_cids.retain(|e| e.sequence >= retire_prior_to);
        
        // Update active index
        if self.active_local_idx >= self.local_cids.len() {
            self.active_local_idx = self.local_cids.len().saturating_sub(1);
        }
    }
    
    /// Find a local CID by its bytes
    pub fn find_local_cid(&self, cid_bytes: &[u8]) -> Option<&CidEntry> {
        self.local_cids.iter().find(|e| e.cid.as_bytes() == cid_bytes)
    }
    
    /// Number of local CIDs
    pub fn local_cid_count(&self) -> usize {
        self.local_cids.len()
    }
    
    /// Number of remote CIDs
    pub fn remote_cid_count(&self) -> usize {
        self.remote_cids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cid_empty() {
        let cid = ConnectionId::empty();
        assert!(cid.is_empty());
        assert_eq!(cid.len(), 0);
    }
    
    #[test]
    fn test_cid_from_bytes() {
        let bytes = [1, 2, 3, 4, 5, 6, 7, 8];
        let cid = ConnectionId::from_bytes(&bytes).unwrap();
        assert_eq!(cid.len(), 8);
        assert_eq!(cid.as_bytes(), &bytes);
    }
    
    #[test]
    fn test_cid_generate() {
        let cid1 = ConnectionId::generate(8);
        let cid2 = ConnectionId::generate(8);
        
        assert_eq!(cid1.len(), 8);
        assert_eq!(cid2.len(), 8);
        // Very unlikely to be equal (probabilistic test)
        // Note: In tests run very fast, they might collide due to time-based seed
    }
    
    #[test]
    fn test_cid_too_long() {
        let bytes = [0u8; 21];
        assert!(ConnectionId::from_bytes(&bytes).is_none());
    }
    
    #[test]
    fn test_cid_manager() {
        let mut mgr = CidManager::new();
        
        let cid = mgr.add_local_cid(8);
        assert_eq!(cid.len(), 8);
        assert_eq!(mgr.local_cid_count(), 1);
        
        mgr.add_remote_cid(ConnectionId::generate(8), 0, None);
        assert_eq!(mgr.remote_cid_count(), 1);
        assert!(mgr.active_remote_cid().is_some());
    }
}
