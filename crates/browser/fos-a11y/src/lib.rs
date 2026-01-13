//! fOS Accessibility
//!
//! Accessibility APIs for the fOS browser engine.
//! Zero external dependencies beyond thiserror.
//!
//! Features:
//! - Complete ARIA (82 roles, states, properties, relationships)
//! - Accessibility tree with incremental updates
//! - Focus management with skip links and roving tabindex
//! - Keyboard navigation with spatial support
//! - Screen reader integration (virtual buffer, announcements)
//! - Live region change detection
//! - Platform APIs (AT-SPI2, NSAccessibility, UIA) 
//! - High contrast mode and contrast checking
//! - Reduced motion support
//! - Text scaling and zoom
//! - Alternative input (switch access, voice control)
//! - Media preferences (reduced-data, reduced-transparency)
//! - Auto-fix suggestions
//! - Reading mode

pub mod aria;
pub mod tree;
pub mod focus;
pub mod screen_reader;
pub mod high_contrast;
pub mod reduced_motion;
pub mod text_scaling;
pub mod keyboard_nav;
pub mod live_region;
pub mod platform;
pub mod alternative_input;
pub mod media_preferences;
pub mod auto_fix;
pub mod reading_mode;

// Core exports
pub use aria::{AriaRole, AriaState, AriaAttributes, LiveRegionMode, LiveRelevant, DropEffect};
pub use tree::{AccessibilityTree, AccessibilityNode, NodeBounds, compute_text_alternative};
pub use focus::{FocusManager, TabIndex, SkipLink, FocusIndicator, FocusStyle};
pub use screen_reader::{
    ScreenReaderBridge, VirtualBuffer, VirtualBufferItem, BufferItemType,
    AnnouncementQueue, Announcement, AnnouncePriority, Politeness,
};
pub use high_contrast::{
    HighContrastManager, HighContrastSettings, ContrastChecker, SystemColors,
    ContrastPreference, ColorScheme, ForcedColorsMode,
};
pub use reduced_motion::{MotionManager, ReducedMotionSettings, MotionPreference, AnimationOverride};
pub use text_scaling::{ScalingManager, TextScalingSettings, ZoomLevel, FontSizePreset};
pub use keyboard_nav::{
    KeyboardNavManager, NavigationMode, SpatialNavigator, ShortcutRegistry,
    KeyboardShortcut, NavAction, Direction, ElementRect,
};
pub use live_region::{LiveRegionTracker, LiveRegionConfig, LiveRegionChange, ChangeType, RelevantFlags};
pub use platform::{PlatformAccessibility, PlatformError, create_platform_bridge, NullBridge};
pub use alternative_input::{SwitchAccessManager, VoiceControlManager, ScanMode, VoiceAction};
pub use media_preferences::{MediaPreferences, TransparencyPreference, DataPreference};
pub use auto_fix::{AccessibilityAudit, A11yIssue, IssueSeverity, SuggestedFix};
pub use reading_mode::{ReadingMode, ReadingModeSettings};

/// Accessibility error
#[derive(Debug, thiserror::Error)]
pub enum A11yError {
    #[error("Missing accessible name for {0}")]
    MissingName(String),
    
    #[error("Invalid ARIA role: {0}")]
    InvalidRole(String),
    
    #[error("Contrast ratio too low: {0}")]
    LowContrast(String),
    
    #[error("Platform error: {0}")]
    Platform(#[from] PlatformError),
}
