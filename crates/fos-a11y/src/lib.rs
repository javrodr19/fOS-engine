//! fOS Accessibility
//!
//! Accessibility APIs for the fOS browser engine.
//!
//! Features:
//! - ARIA roles, states, properties
//! - Accessibility tree
//! - Focus management
//! - Keyboard navigation

pub mod aria;
pub mod tree;
pub mod focus;

pub use aria::{AriaRole, AriaState, AriaAttributes, LiveRegionMode};
pub use tree::{AccessibilityTree, AccessibilityNode, compute_text_alternative};
pub use focus::{FocusManager, TabIndex, SkipLink, FocusIndicator};

/// Accessibility error
#[derive(Debug, thiserror::Error)]
pub enum A11yError {
    #[error("Missing accessible name for {0}")]
    MissingName(String),
    
    #[error("Invalid ARIA role: {0}")]
    InvalidRole(String),
}
