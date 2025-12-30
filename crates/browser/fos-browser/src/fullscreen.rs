//! Fullscreen API
//!
//! Enter and exit fullscreen mode for elements.

use fos_dom::NodeId;

/// Fullscreen state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FullscreenState {
    #[default]
    Normal,
    Fullscreen,
    FullscreenRequested,
    ExitRequested,
}

/// Fullscreen error
#[derive(Debug)]
pub enum FullscreenError {
    NotSupported,
    ElementNotFound,
    NotAllowed,
    NotInDocument,
}

impl std::fmt::Display for FullscreenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSupported => write!(f, "Fullscreen not supported"),
            Self::ElementNotFound => write!(f, "Element not found"),
            Self::NotAllowed => write!(f, "Fullscreen not allowed"),
            Self::NotInDocument => write!(f, "Element not in document"),
        }
    }
}

impl std::error::Error for FullscreenError {}

/// Fullscreen manager
#[derive(Debug, Default)]
pub struct FullscreenManager {
    state: FullscreenState,
    fullscreen_element: Option<NodeId>,
    allowed_origins: Vec<String>,
}

impl FullscreenManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get current fullscreen state
    pub fn state(&self) -> FullscreenState {
        self.state
    }
    
    /// Get current fullscreen element
    pub fn fullscreen_element(&self) -> Option<NodeId> {
        self.fullscreen_element
    }
    
    /// Check if currently fullscreen
    pub fn is_fullscreen(&self) -> bool {
        self.state == FullscreenState::Fullscreen
    }
    
    /// Request fullscreen for an element
    pub fn request_fullscreen(&mut self, element: NodeId) -> Result<(), FullscreenError> {
        if self.state == FullscreenState::Fullscreen {
            // Already fullscreen, just update element
            self.fullscreen_element = Some(element);
            return Ok(());
        }
        
        self.state = FullscreenState::FullscreenRequested;
        self.fullscreen_element = Some(element);
        
        // The actual fullscreen transition would be handled by the window system
        // For now, transition immediately
        self.state = FullscreenState::Fullscreen;
        
        Ok(())
    }
    
    /// Exit fullscreen
    pub fn exit_fullscreen(&mut self) -> Result<(), FullscreenError> {
        if self.state != FullscreenState::Fullscreen {
            return Ok(()); // Already not fullscreen
        }
        
        self.state = FullscreenState::ExitRequested;
        self.fullscreen_element = None;
        
        // The actual exit would be handled by the window system
        self.state = FullscreenState::Normal;
        
        Ok(())
    }
    
    /// Check if fullscreen is enabled for origin
    pub fn fullscreen_enabled(&self, _origin: &str) -> bool {
        // In real implementation, check iframe permissions
        true
    }
    
    /// Process keyboard escape
    pub fn handle_escape(&mut self) -> bool {
        if self.is_fullscreen() {
            let _ = self.exit_fullscreen();
            true
        } else {
            false
        }
    }
}

/// Screen wake lock to prevent device sleep
#[derive(Debug, Default)]
pub struct WakeLockManager {
    active_locks: Vec<WakeLock>,
    next_id: u64,
}

#[derive(Debug, Clone)]
pub struct WakeLock {
    pub id: u64,
    pub wake_type: WakeLockType,
    pub origin: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeLockType {
    Screen,
}

impl WakeLockManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Request a wake lock
    pub fn request(&mut self, lock_type: WakeLockType, origin: &str) -> Result<u64, WakeLockError> {
        // In real implementation, call system API
        let id = self.next_id;
        self.next_id += 1;
        
        self.active_locks.push(WakeLock {
            id,
            wake_type: lock_type,
            origin: origin.to_string(),
        });
        
        log::debug!("Wake lock acquired: id={}, type={:?}", id, lock_type);
        Ok(id)
    }
    
    /// Release a wake lock
    pub fn release(&mut self, id: u64) -> bool {
        let len_before = self.active_locks.len();
        self.active_locks.retain(|l| l.id != id);
        self.active_locks.len() < len_before
    }
    
    /// Release all locks for origin
    pub fn release_for_origin(&mut self, origin: &str) {
        self.active_locks.retain(|l| l.origin != origin);
    }
    
    /// Check if wake lock is active
    pub fn is_active(&self) -> bool {
        !self.active_locks.is_empty()
    }
}

#[derive(Debug)]
pub enum WakeLockError {
    NotSupported,
    NotAllowed,
}

impl std::fmt::Display for WakeLockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSupported => write!(f, "Wake lock not supported"),
            Self::NotAllowed => write!(f, "Wake lock not allowed"),
        }
    }
}

impl std::error::Error for WakeLockError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fullscreen() {
        let mut mgr = FullscreenManager::new();
        
        assert!(!mgr.is_fullscreen());
        
        mgr.request_fullscreen(NodeId::from_raw_parts(1, 0)).unwrap();
        assert!(mgr.is_fullscreen());
        
        mgr.exit_fullscreen().unwrap();
        assert!(!mgr.is_fullscreen());
    }
    
    #[test]
    fn test_wake_lock() {
        let mut mgr = WakeLockManager::new();
        
        let id = mgr.request(WakeLockType::Screen, "https://example.com").unwrap();
        assert!(mgr.is_active());
        
        mgr.release(id);
        assert!(!mgr.is_active());
    }
}
