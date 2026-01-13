//! QUIC Connection Migration
//!
//! Path validation and connection migration per RFC 9000 §9.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

/// Path state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathState {
    /// Path is being validated
    Validating,
    /// Path is validated and active
    Active,
    /// Path validation failed
    Failed,
    /// Path was abandoned
    Abandoned,
}

/// Challenge/response data for path validation
#[derive(Debug, Clone)]
pub struct PathChallenge {
    /// Challenge data (8 random bytes)
    pub data: [u8; 8],
    /// When the challenge was sent
    pub sent_at: Instant,
    /// Number of times sent
    pub attempts: u32,
}

impl PathChallenge {
    /// Create a new path challenge with random data
    pub fn new() -> Self {
        let mut data = [0u8; 8];
        // Simple PRNG for challenge data
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        
        for (i, byte) in data.iter_mut().enumerate() {
            *byte = ((seed >> (i * 8)) & 0xFF) as u8;
        }
        
        Self {
            data,
            sent_at: Instant::now(),
            attempts: 1,
        }
    }
    
    /// Check if challenge has timed out
    pub fn is_expired(&self, timeout: Duration) -> bool {
        self.sent_at.elapsed() > timeout
    }
}

impl Default for PathChallenge {
    fn default() -> Self {
        Self::new()
    }
}

/// Network path information
#[derive(Debug, Clone)]
pub struct NetworkPath {
    /// Local address
    pub local_addr: SocketAddr,
    /// Remote address
    pub remote_addr: SocketAddr,
    /// Path state
    pub state: PathState,
    /// Pending challenge
    pub challenge: Option<PathChallenge>,
    /// RTT estimate for this path (if known)
    pub rtt: Option<Duration>,
    /// Congestion window for this path
    pub cwnd: u64,
    /// When this path became active
    pub active_since: Option<Instant>,
    /// Bytes sent on this path
    pub bytes_sent: u64,
    /// Bytes received on this path
    pub bytes_received: u64,
}

impl NetworkPath {
    /// Create a new unvalidated path
    pub fn new(local_addr: SocketAddr, remote_addr: SocketAddr) -> Self {
        Self {
            local_addr,
            remote_addr,
            state: PathState::Validating,
            challenge: None,
            rtt: None,
            cwnd: 10 * 1200, // Initial cwnd
            active_since: None,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }
    
    /// Start path validation
    pub fn start_validation(&mut self) -> [u8; 8] {
        let challenge = PathChallenge::new();
        let data = challenge.data;
        self.challenge = Some(challenge);
        self.state = PathState::Validating;
        data
    }
    
    /// Complete path validation (received PATH_RESPONSE)
    pub fn complete_validation(&mut self, response_data: &[u8; 8]) -> bool {
        if let Some(challenge) = &self.challenge {
            if challenge.data == *response_data {
                // Calculate RTT from challenge
                self.rtt = Some(challenge.sent_at.elapsed());
                self.state = PathState::Active;
                self.active_since = Some(Instant::now());
                self.challenge = None;
                return true;
            }
        }
        false
    }
    
    /// Mark path validation as failed
    pub fn fail_validation(&mut self) {
        self.state = PathState::Failed;
        self.challenge = None;
    }
    
    /// Check if path is active (validated)
    pub fn is_active(&self) -> bool {
        self.state == PathState::Active
    }
    
    /// Record bytes sent
    pub fn record_sent(&mut self, bytes: u64) {
        self.bytes_sent += bytes;
    }
    
    /// Record bytes received
    pub fn record_received(&mut self, bytes: u64) {
        self.bytes_received += bytes;
    }
}

/// Path manager for connection migration
#[derive(Debug)]
pub struct PathManager {
    /// All known paths
    paths: HashMap<(SocketAddr, SocketAddr), NetworkPath>,
    /// Currently active path
    active_path: Option<(SocketAddr, SocketAddr)>,
    /// Path validation timeout
    validation_timeout: Duration,
    /// Maximum validation attempts
    max_validation_attempts: u32,
    /// Whether migration is enabled
    migration_enabled: bool,
}

impl PathManager {
    /// Create a new path manager
    pub fn new() -> Self {
        Self {
            paths: HashMap::new(),
            active_path: None,
            validation_timeout: Duration::from_secs(3),
            max_validation_attempts: 3,
            migration_enabled: true,
        }
    }
    
    /// Set initial path
    pub fn set_initial_path(&mut self, local: SocketAddr, remote: SocketAddr) {
        let mut path = NetworkPath::new(local, remote);
        path.state = PathState::Active;
        path.active_since = Some(Instant::now());
        
        let key = (local, remote);
        self.paths.insert(key, path);
        self.active_path = Some(key);
    }
    
    /// Get the active path
    pub fn active_path(&self) -> Option<&NetworkPath> {
        self.active_path.and_then(|key| self.paths.get(&key))
    }
    
    /// Get the active path mutably
    pub fn active_path_mut(&mut self) -> Option<&mut NetworkPath> {
        self.active_path.and_then(|key| self.paths.get_mut(&key))
    }
    
    /// Detect path change from received packet
    pub fn on_packet_received(&mut self, local: SocketAddr, remote: SocketAddr) -> PathChangeResult {
        let key = (local, remote);
        
        // Same as active path
        if self.active_path == Some(key) {
            if let Some(path) = self.paths.get_mut(&key) {
                path.record_received(1);
            }
            return PathChangeResult::NoChange;
        }
        
        // Check if this is a known path
        if let Some(path) = self.paths.get_mut(&key) {
            path.record_received(1);
            if path.is_active() {
                // Switch to this path (NAT rebinding or intentional migration)
                self.active_path = Some(key);
                return PathChangeResult::Switched(key);
            } else {
                return PathChangeResult::Validating;
            }
        }
        
        // New path - needs validation
        if self.migration_enabled {
            let path = NetworkPath::new(local, remote);
            self.paths.insert(key, path);
            return PathChangeResult::NewPath(key);
        }
        
        PathChangeResult::NoChange
    }
    
    /// Initiate migration to a new path
    pub fn initiate_migration(&mut self, local: SocketAddr, remote: SocketAddr) -> Option<[u8; 8]> {
        if !self.migration_enabled {
            return None;
        }
        
        let key = (local, remote);
        let path = self.paths.entry(key).or_insert_with(|| NetworkPath::new(local, remote));
        
        Some(path.start_validation())
    }
    
    /// Handle PATH_RESPONSE frame
    pub fn on_path_response(&mut self, response_data: &[u8; 8]) -> bool {
        // Check all validating paths
        for (key, path) in self.paths.iter_mut() {
            if path.state == PathState::Validating {
                if path.complete_validation(response_data) {
                    // Make this the active path
                    let key = *key;
                    self.active_path = Some(key);
                    return true;
                }
            }
        }
        false
    }
    
    /// Handle PATH_CHALLENGE frame - returns PATH_RESPONSE data
    pub fn on_path_challenge(&mut self, challenge_data: [u8; 8]) -> [u8; 8] {
        // Simply echo back the challenge data
        challenge_data
    }
    
    /// Check for timed-out validations
    pub fn check_timeouts(&mut self) {
        for path in self.paths.values_mut() {
            if path.state == PathState::Validating {
                if let Some(challenge) = &path.challenge {
                    if challenge.is_expired(self.validation_timeout) {
                        if challenge.attempts >= self.max_validation_attempts {
                            path.fail_validation();
                        }
                    }
                }
            }
        }
    }
    
    /// Retry failed validation
    pub fn retry_validation(&mut self, local: SocketAddr, remote: SocketAddr) -> Option<[u8; 8]> {
        let key = (local, remote);
        let path = self.paths.get_mut(&key)?;
        
        if path.state == PathState::Validating {
            if let Some(challenge) = &mut path.challenge {
                if challenge.attempts < self.max_validation_attempts {
                    challenge.attempts += 1;
                    challenge.sent_at = Instant::now();
                    return Some(challenge.data);
                }
            }
        }
        None
    }
    
    /// Get all paths
    pub fn paths(&self) -> impl Iterator<Item = &NetworkPath> {
        self.paths.values()
    }
    
    /// Enable/disable migration
    pub fn set_migration_enabled(&mut self, enabled: bool) {
        self.migration_enabled = enabled;
    }
    
    /// Check if migration is enabled
    pub fn is_migration_enabled(&self) -> bool {
        self.migration_enabled
    }
}

impl Default for PathManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of path change detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathChangeResult {
    /// No path change
    NoChange,
    /// Switched to an existing validated path
    Switched((SocketAddr, SocketAddr)),
    /// New path detected, needs validation
    NewPath((SocketAddr, SocketAddr)),
    /// Path is currently being validated
    Validating,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn addr(port: u16) -> SocketAddr {
        format!("127.0.0.1:{}", port).parse().unwrap()
    }
    
    #[test]
    fn test_path_creation() {
        let path = NetworkPath::new(addr(1234), addr(443));
        assert_eq!(path.state, PathState::Validating);
        assert!(path.rtt.is_none());
    }
    
    #[test]
    fn test_path_validation() {
        let mut path = NetworkPath::new(addr(1234), addr(443));
        let challenge = path.start_validation();
        
        // Wrong response
        assert!(!path.complete_validation(&[0; 8]));
        assert_eq!(path.state, PathState::Validating);
        
        // Correct response
        assert!(path.complete_validation(&challenge));
        assert_eq!(path.state, PathState::Active);
        assert!(path.rtt.is_some());
    }
    
    #[test]
    fn test_path_manager_initial() {
        let mut mgr = PathManager::new();
        mgr.set_initial_path(addr(1234), addr(443));
        
        let path = mgr.active_path().unwrap();
        assert!(path.is_active());
    }
    
    #[test]
    fn test_path_migration() {
        let mut mgr = PathManager::new();
        mgr.set_initial_path(addr(1234), addr(443));
        
        // Initiate migration to new path
        let challenge = mgr.initiate_migration(addr(5678), addr(443)).unwrap();
        
        // Complete validation
        assert!(mgr.on_path_response(&challenge));
        
        // Check new active path
        let active = mgr.active_path().unwrap();
        assert_eq!(active.local_addr, addr(5678));
    }
    
    #[test]
    fn test_path_challenge_echo() {
        let mut mgr = PathManager::new();
        let challenge = [1, 2, 3, 4, 5, 6, 7, 8];
        let response = mgr.on_path_challenge(challenge);
        assert_eq!(response, challenge);
    }
}
