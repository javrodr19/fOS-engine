//! fOS Accessibility
//!
//! Accessibility APIs for the fOS browser engine.
//!
//! Features:
//! - ARIA roles, states, properties
//! - Accessibility tree
//! - Focus management
//! - Keyboard navigation
//! - Screen reader integration
//! - High contrast mode
//! - Reduced motion support
//! - Text scaling

pub mod aria;
pub mod tree;
pub mod focus;
pub mod screen_reader;
pub mod high_contrast;
pub mod reduced_motion;
pub mod text_scaling;
pub mod keyboard_nav;

pub use aria::{AriaRole, AriaState, AriaAttributes, LiveRegionMode};
pub use tree::{AccessibilityTree, AccessibilityNode, compute_text_alternative};
pub use focus::{FocusManager, TabIndex, SkipLink, FocusIndicator};
pub use screen_reader::{ScreenReaderBridge, VirtualBuffer, AnnouncementQueue, Announcement};
pub use high_contrast::{HighContrastManager, HighContrastSettings, ContrastChecker, SystemColors};
pub use reduced_motion::{MotionManager, ReducedMotionSettings, MotionPreference};
pub use text_scaling::{ScalingManager, TextScalingSettings, ZoomLevel};
pub use keyboard_nav::{KeyboardNavManager, NavigationMode, SpatialNavigator, ShortcutRegistry};

/// Accessibility error
#[derive(Debug, thiserror::Error)]
pub enum A11yError {
    #[error("Missing accessible name for {0}")]
    MissingName(String),
    
    #[error("Invalid ARIA role: {0}")]
    InvalidRole(String),
    
    #[error("Contrast ratio too low: {0}")]
    LowContrast(String),
}
