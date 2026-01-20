//! Wake Lock Management
//!
//! Manages wake locks with automatic release for power efficiency.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Reason for holding a wake lock
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WakeLockReason {
    /// User input activity
    UserInput,
    /// Active animation
    Animation,
    /// Media playback
    MediaPlayback,
    /// Active download
    Download,
    /// WebRTC call
    WebRTC,
    /// JavaScript timer
    Timer,
}

impl WakeLockReason {
    /// Get default timeout for this lock type
    pub fn default_timeout(&self) -> Duration {
        match self {
            Self::UserInput => Duration::from_millis(100),
            Self::Animation => Duration::from_secs(30),
            Self::MediaPlayback => Duration::MAX, // Until stopped
            Self::Download => Duration::MAX,       // Until complete
            Self::WebRTC => Duration::MAX,         // Until call ends
            Self::Timer => Duration::from_secs(1),
        }
    }
    
    /// Get priority (higher = more important)
    pub fn priority(&self) -> u8 {
        match self {
            Self::MediaPlayback => 5,
            Self::WebRTC => 5,
            Self::Download => 4,
            Self::Animation => 3,
            Self::UserInput => 2,
            Self::Timer => 1,
        }
    }
}

/// A single wake lock instance
#[derive(Debug)]
struct WakeLock {
    /// Lock ID
    id: u64,
    /// Lock reason
    reason: WakeLockReason,
    /// Creation time
    created: Instant,
    /// Expiry time (if set)
    expires: Option<Instant>,
    /// Associated tab (if any)
    tab_id: Option<u32>,
}

impl WakeLock {
    fn is_expired(&self) -> bool {
        self.expires.map(|e| Instant::now() >= e).unwrap_or(false)
    }
}

/// Wake lock guard for RAII-style management
#[derive(Debug)]
pub struct WakeLockGuard {
    /// Lock ID (used for release)
    pub id: u64,
    /// Lock reason
    pub reason: WakeLockReason,
}

/// Wake lock manager
#[derive(Debug)]
pub struct WakeLockManager {
    /// Active locks
    locks: HashMap<u64, WakeLock>,
    /// Next lock ID
    next_id: u64,
    /// Total locks acquired
    total_acquired: u64,
    /// Total locks released
    total_released: u64,
    /// Enable coalescing of similar locks
    coalesce: bool,
}

impl Default for WakeLockManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WakeLockManager {
    /// Create a new wake lock manager
    pub fn new() -> Self {
        Self {
            locks: HashMap::new(),
            next_id: 1,
            total_acquired: 0,
            total_released: 0,
            coalesce: true,
        }
    }
    
    /// Request a wake lock
    pub fn request(&mut self, reason: WakeLockReason) -> WakeLockGuard {
        self.request_with_timeout(reason, reason.default_timeout())
    }
    
    /// Request a wake lock with custom timeout
    pub fn request_with_timeout(&mut self, reason: WakeLockReason, timeout: Duration) -> WakeLockGuard {
        self.request_for_tab(reason, timeout, None)
    }
    
    /// Request a wake lock for a specific tab
    pub fn request_for_tab(&mut self, reason: WakeLockReason, timeout: Duration, tab_id: Option<u32>) -> WakeLockGuard {
        // Coalesce if enabled and similar lock exists
        if self.coalesce {
            if let Some(existing) = self.find_similar(reason, tab_id) {
                // Extend existing lock
                if let Some(lock) = self.locks.get_mut(&existing) {
                    let new_expiry = Instant::now() + timeout;
                    if lock.expires.map(|e| new_expiry > e).unwrap_or(true) {
                        lock.expires = if timeout == Duration::MAX { None } else { Some(new_expiry) };
                    }
                    return WakeLockGuard { id: existing, reason };
                }
            }
        }
        
        let id = self.next_id;
        self.next_id += 1;
        
        let expires = if timeout == Duration::MAX {
            None
        } else {
            Some(Instant::now() + timeout)
        };
        
        let lock = WakeLock {
            id,
            reason,
            created: Instant::now(),
            expires,
            tab_id,
        };
        
        self.locks.insert(id, lock);
        self.total_acquired += 1;
        
        WakeLockGuard { id, reason }
    }
    
    /// Find similar lock for coalescing
    fn find_similar(&self, reason: WakeLockReason, tab_id: Option<u32>) -> Option<u64> {
        self.locks.iter()
            .find(|(_, lock)| lock.reason == reason && lock.tab_id == tab_id && !lock.is_expired())
            .map(|(&id, _)| id)
    }
    
    /// Release a wake lock
    pub fn release(&mut self, id: u64) {
        if self.locks.remove(&id).is_some() {
            self.total_released += 1;
        }
    }
    
    /// Release all locks for a tab
    pub fn release_for_tab(&mut self, tab_id: u32) {
        let to_remove: Vec<_> = self.locks.iter()
            .filter(|(_, lock)| lock.tab_id == Some(tab_id))
            .map(|(&id, _)| id)
            .collect();
        
        for id in to_remove {
            self.locks.remove(&id);
            self.total_released += 1;
        }
    }
    
    /// Release all locks of a reason type
    pub fn release_reason(&mut self, reason: WakeLockReason) {
        let to_remove: Vec<_> = self.locks.iter()
            .filter(|(_, lock)| lock.reason == reason)
            .map(|(&id, _)| id)
            .collect();
        
        for id in to_remove {
            self.locks.remove(&id);
            self.total_released += 1;
        }
    }
    
    /// Clean up expired locks
    pub fn cleanup_expired(&mut self) {
        let expired: Vec<_> = self.locks.iter()
            .filter(|(_, lock)| lock.is_expired())
            .map(|(&id, _)| id)
            .collect();
        
        for id in expired {
            self.locks.remove(&id);
            self.total_released += 1;
        }
    }
    
    /// Check if any locks are active
    pub fn has_active_locks(&self) -> bool {
        self.locks.values().any(|l| !l.is_expired())
    }
    
    /// Check if specific reason has active lock
    pub fn has_lock(&self, reason: WakeLockReason) -> bool {
        self.locks.values().any(|l| l.reason == reason && !l.is_expired())
    }
    
    /// Get highest priority active lock reason
    pub fn highest_priority(&self) -> Option<WakeLockReason> {
        self.locks.values()
            .filter(|l| !l.is_expired())
            .max_by_key(|l| l.reason.priority())
            .map(|l| l.reason)
    }
    
    /// Get active lock count
    pub fn active_count(&self) -> usize {
        self.locks.values().filter(|l| !l.is_expired()).count()
    }
    
    /// Get statistics
    pub fn stats(&self) -> WakeLockStats {
        WakeLockStats {
            active_locks: self.active_count(),
            total_acquired: self.total_acquired,
            total_released: self.total_released,
        }
    }
    
    /// Enable/disable lock coalescing
    pub fn set_coalesce(&mut self, enabled: bool) {
        self.coalesce = enabled;
    }
}

/// Wake lock statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct WakeLockStats {
    pub active_locks: usize,
    pub total_acquired: u64,
    pub total_released: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_request_release() {
        let mut manager = WakeLockManager::new();
        
        let guard = manager.request(WakeLockReason::UserInput);
        assert_eq!(manager.active_count(), 1);
        
        manager.release(guard.id);
        assert_eq!(manager.active_count(), 0);
    }
    
    #[test]
    fn test_timeout() {
        let mut manager = WakeLockManager::new();
        
        manager.request_with_timeout(WakeLockReason::Timer, Duration::ZERO);
        
        // Should be expired immediately
        manager.cleanup_expired();
        assert_eq!(manager.active_count(), 0);
    }
    
    #[test]
    fn test_coalescing() {
        let mut manager = WakeLockManager::new();
        manager.set_coalesce(true);
        
        let guard1 = manager.request(WakeLockReason::Animation);
        let guard2 = manager.request(WakeLockReason::Animation);
        
        // Should coalesce to single lock
        assert_eq!(guard1.id, guard2.id);
        assert_eq!(manager.active_count(), 1);
    }
    
    #[test]
    fn test_priority() {
        assert!(WakeLockReason::MediaPlayback.priority() > WakeLockReason::UserInput.priority());
    }
    
    #[test]
    fn test_highest_priority() {
        let mut manager = WakeLockManager::new();
        manager.set_coalesce(false);
        
        manager.request(WakeLockReason::Timer);
        manager.request(WakeLockReason::MediaPlayback);
        manager.request(WakeLockReason::Animation);
        
        assert_eq!(manager.highest_priority(), Some(WakeLockReason::MediaPlayback));
    }
}
