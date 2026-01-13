//! Platform Accessibility Integration
//!
//! Abstraction layer for platform-specific accessibility APIs.
//! All implementations are pure Rust with platform-specific paths.
//!
//! Supported platforms:
//! - Linux: AT-SPI2 (Orca screen reader)
//! - macOS: NSAccessibility (VoiceOver)
//! - Windows: UIA (NVDA, JAWS)

mod linux;
mod macos;
mod windows;

pub use linux::AtSpi2Bridge;
pub use macos::NsAccessibilityBridge;
pub use windows::UiaBridge;

use crate::tree::AccessibilityTree;
use crate::screen_reader::AnnouncePriority;

/// Platform accessibility bridge trait
pub trait PlatformAccessibility: Send + Sync {
    /// Initialize the platform bridge
    fn init(&mut self) -> Result<(), PlatformError>;
    
    /// Shutdown the bridge
    fn shutdown(&mut self);
    
    /// Check if a screen reader is active
    fn is_screen_reader_active(&self) -> bool;
    
    /// Announce text to screen reader
    fn announce(&self, text: &str, priority: AnnouncePriority);
    
    /// Update the entire accessibility tree
    fn update_tree(&self, tree: &AccessibilityTree);
    
    /// Notify that focus changed to a node
    fn focus_changed(&self, node_id: u64);
    
    /// Notify that a node's properties changed
    fn node_changed(&self, node_id: u64);
    
    /// Notify that a node was added
    fn node_added(&self, node_id: u64, parent_id: u64);
    
    /// Notify that a node was removed
    fn node_removed(&self, node_id: u64);
    
    /// Get platform name
    fn platform_name(&self) -> &'static str;
}

/// Platform accessibility error
#[derive(Debug, Clone)]
pub enum PlatformError {
    /// Platform not supported
    NotSupported,
    /// Failed to connect to accessibility service
    ConnectionFailed(String),
    /// Screen reader not running
    NoScreenReader,
    /// Other error
    Other(String),
}

impl std::fmt::Display for PlatformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSupported => write!(f, "Platform not supported"),
            Self::ConnectionFailed(s) => write!(f, "Connection failed: {}", s),
            Self::NoScreenReader => write!(f, "No screen reader running"),
            Self::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for PlatformError {}

/// Detect current platform and create appropriate bridge
pub fn create_platform_bridge() -> Box<dyn PlatformAccessibility> {
    #[cfg(target_os = "linux")]
    {
        Box::new(AtSpi2Bridge::new())
    }
    #[cfg(target_os = "macos")]
    {
        Box::new(NsAccessibilityBridge::new())
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(UiaBridge::new())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Box::new(NullBridge::new())
    }
}

/// Null bridge for unsupported platforms
#[derive(Debug, Default)]
pub struct NullBridge;

impl NullBridge {
    pub fn new() -> Self { Self }
}

impl PlatformAccessibility for NullBridge {
    fn init(&mut self) -> Result<(), PlatformError> { Ok(()) }
    fn shutdown(&mut self) {}
    fn is_screen_reader_active(&self) -> bool { false }
    fn announce(&self, _text: &str, _priority: AnnouncePriority) {}
    fn update_tree(&self, _tree: &AccessibilityTree) {}
    fn focus_changed(&self, _node_id: u64) {}
    fn node_changed(&self, _node_id: u64) {}
    fn node_added(&self, _node_id: u64, _parent_id: u64) {}
    fn node_removed(&self, _node_id: u64) {}
    fn platform_name(&self) -> &'static str { "null" }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_null_bridge() {
        let mut bridge = NullBridge::new();
        assert!(bridge.init().is_ok());
        assert!(!bridge.is_screen_reader_active());
        assert_eq!(bridge.platform_name(), "null");
    }
}
