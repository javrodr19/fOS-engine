//! CSS View Transitions
//!
//! Implementation of CSS View Transitions specification.
//! Enables smooth transitions between DOM states with snapshot pseudo-elements.

use std::collections::HashMap;

// ============================================================================
// View Transition Types
// ============================================================================

/// View transition state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransitionState {
    #[default]
    Idle,
    /// Capturing old state
    Capturing,
    /// Animating between states
    Animating,
    /// Transition finished
    Finished,
    /// Transition was skipped
    Skipped,
}

/// A view transition group for an element
#[derive(Debug, Clone)]
pub struct ViewTransitionGroup {
    /// Transition name (from view-transition-name)
    pub name: Box<str>,
    /// Element ID
    pub element_id: u32,
    /// Old snapshot (captured before DOM change)
    pub old_snapshot: Option<TransitionSnapshot>,
    /// New snapshot (captured after DOM change)
    pub new_snapshot: Option<TransitionSnapshot>,
    /// Animation progress (0.0 to 1.0)
    pub progress: f32,
}

/// Snapshot of an element's state
#[derive(Debug, Clone)]
pub struct TransitionSnapshot {
    /// Bounding box
    pub rect: SnapshotRect,
    /// Transform matrix (flattened 4x4)
    pub transform: [f32; 16],
    /// Writing mode
    pub writing_mode: WritingMode,
    /// Direction
    pub direction: Direction,
    /// Captured image data (optional - for raster snapshots)
    pub image_data: Option<Box<[u8]>>,
}

/// Snapshot rectangle
#[derive(Debug, Clone, Copy, Default)]
pub struct SnapshotRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Writing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
    VerticalLr,
}

/// Direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

// ============================================================================
// View Transition Manager
// ============================================================================

/// Manages view transitions
#[derive(Debug)]
pub struct ViewTransitionManager {
    /// Current transition state
    state: TransitionState,
    /// Active transition groups
    groups: HashMap<Box<str>, ViewTransitionGroup>,
    /// Default transition duration (ms)
    default_duration: f32,
    /// Transition started callback ID
    on_start: Option<u32>,
    /// Transition finished callback ID
    on_finish: Option<u32>,
}

impl Default for ViewTransitionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewTransitionManager {
    pub fn new() -> Self {
        Self {
            state: TransitionState::Idle,
            groups: HashMap::new(),
            default_duration: 250.0,
            on_start: None,
            on_finish: None,
        }
    }
    
    /// Start a new view transition
    pub fn start_transition(&mut self) -> Result<(), TransitionError> {
        if self.state != TransitionState::Idle {
            return Err(TransitionError::AlreadyActive);
        }
        
        self.state = TransitionState::Capturing;
        Ok(())
    }
    
    /// Register an element with view-transition-name
    pub fn register_element(&mut self, name: &str, element_id: u32) {
        self.groups.insert(name.into(), ViewTransitionGroup {
            name: name.into(),
            element_id,
            old_snapshot: None,
            new_snapshot: None,
            progress: 0.0,
        });
    }
    
    /// Capture old state snapshot for an element
    pub fn capture_old_state(
        &mut self,
        name: &str,
        rect: SnapshotRect,
        transform: [f32; 16],
        writing_mode: WritingMode,
        direction: Direction,
    ) {
        if let Some(group) = self.groups.get_mut(name) {
            group.old_snapshot = Some(TransitionSnapshot {
                rect,
                transform,
                writing_mode,
                direction,
                image_data: None,
            });
        }
    }
    
    /// Capture new state snapshot for an element
    pub fn capture_new_state(
        &mut self,
        name: &str,
        rect: SnapshotRect,
        transform: [f32; 16],
        writing_mode: WritingMode,
        direction: Direction,
    ) {
        if let Some(group) = self.groups.get_mut(name) {
            group.new_snapshot = Some(TransitionSnapshot {
                rect,
                transform,
                writing_mode,
                direction,
                image_data: None,
            });
        }
    }
    
    /// Start animating after capturing
    pub fn start_animating(&mut self) {
        if self.state == TransitionState::Capturing {
            self.state = TransitionState::Animating;
        }
    }
    
    /// Update animation progress
    pub fn update(&mut self, delta_ms: f32) {
        if self.state != TransitionState::Animating {
            return;
        }
        
        let progress_delta = delta_ms / self.default_duration;
        let mut all_finished = true;
        
        for group in self.groups.values_mut() {
            group.progress += progress_delta;
            if group.progress < 1.0 {
                all_finished = false;
            } else {
                group.progress = 1.0;
            }
        }
        
        if all_finished {
            self.state = TransitionState::Finished;
        }
    }
    
    /// Get interpolated rect for a transition group
    pub fn get_interpolated_rect(&self, name: &str) -> Option<SnapshotRect> {
        let group = self.groups.get(name)?;
        let old = group.old_snapshot.as_ref()?;
        let new = group.new_snapshot.as_ref()?;
        
        let t = group.progress;
        
        Some(SnapshotRect {
            x: lerp(old.rect.x, new.rect.x, t),
            y: lerp(old.rect.y, new.rect.y, t),
            width: lerp(old.rect.width, new.rect.width, t),
            height: lerp(old.rect.height, new.rect.height, t),
        })
    }
    
    /// Skip the transition
    pub fn skip(&mut self) {
        self.state = TransitionState::Skipped;
        for group in self.groups.values_mut() {
            group.progress = 1.0;
        }
    }
    
    /// Finish the transition
    pub fn finish(&mut self) {
        self.state = TransitionState::Finished;
        for group in self.groups.values_mut() {
            group.progress = 1.0;
        }
    }
    
    /// Reset to idle state
    pub fn reset(&mut self) {
        self.state = TransitionState::Idle;
        self.groups.clear();
    }
    
    /// Get current state
    pub fn state(&self) -> TransitionState {
        self.state
    }
    
    /// Get all transition group names
    pub fn group_names(&self) -> impl Iterator<Item = &str> {
        self.groups.keys().map(|s| s.as_ref())
    }
    
    /// Get a transition group
    pub fn get_group(&self, name: &str) -> Option<&ViewTransitionGroup> {
        self.groups.get(name)
    }
    
    /// Set default duration
    pub fn set_duration(&mut self, duration_ms: f32) {
        self.default_duration = duration_ms;
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// View transition error
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionError {
    AlreadyActive,
    NotActive,
    InvalidState,
}

// ============================================================================
// View Transition Pseudo-Elements
// ============================================================================

/// View transition pseudo-element types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewTransitionPseudo {
    /// ::view-transition - root pseudo
    Root,
    /// ::view-transition-group(name)
    Group,
    /// ::view-transition-image-pair(name)
    ImagePair,
    /// ::view-transition-old(name)
    Old,
    /// ::view-transition-new(name)
    New,
}

impl ViewTransitionPseudo {
    /// Parse from pseudo-element string
    pub fn parse(s: &str) -> Option<(Self, Option<Box<str>>)> {
        if s == "view-transition" {
            return Some((Self::Root, None));
        }
        
        if let Some(rest) = s.strip_prefix("view-transition-group(") {
            let name = rest.strip_suffix(')')?.trim();
            return Some((Self::Group, Some(name.into())));
        }
        
        if let Some(rest) = s.strip_prefix("view-transition-image-pair(") {
            let name = rest.strip_suffix(')')?.trim();
            return Some((Self::ImagePair, Some(name.into())));
        }
        
        if let Some(rest) = s.strip_prefix("view-transition-old(") {
            let name = rest.strip_suffix(')')?.trim();
            return Some((Self::Old, Some(name.into())));
        }
        
        if let Some(rest) = s.strip_prefix("view-transition-new(") {
            let name = rest.strip_suffix(')')?.trim();
            return Some((Self::New, Some(name.into())));
        }
        
        None
    }
    
    /// Get default styles for this pseudo-element
    pub fn default_styles(&self) -> &'static str {
        match self {
            Self::Root => "position: fixed; inset: 0; contain: layout;",
            Self::Group => "position: absolute; transform-origin: 0 0;",
            Self::ImagePair => "position: absolute; inset: 0;",
            Self::Old => "position: absolute; inset: 0; mix-blend-mode: plus-lighter; animation: -ua-view-transition-fade-out 0.25s;",
            Self::New => "position: absolute; inset: 0; mix-blend-mode: plus-lighter; animation: -ua-view-transition-fade-in 0.25s;",
        }
    }
}

// ============================================================================
// Cross-Document View Transitions
// ============================================================================

/// Cross-document transition state
#[derive(Debug, Clone)]
pub struct CrossDocumentTransition {
    /// Source document URL
    pub source_url: Box<str>,
    /// Target document URL
    pub target_url: Box<str>,
    /// Serialized snapshots from source document
    pub snapshots: HashMap<Box<str>, SerializedSnapshot>,
    /// Navigation type
    pub navigation_type: NavigationType,
}

/// Serialized snapshot for cross-document transitions
#[derive(Debug, Clone)]
pub struct SerializedSnapshot {
    pub name: Box<str>,
    pub rect: SnapshotRect,
    pub image_data: Option<Box<[u8]>>,
}

/// Navigation type for cross-document transitions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavigationType {
    #[default]
    Push,
    Replace,
    Back,
    Forward,
    Reload,
}

/// Parse view-transition-name property
pub fn parse_view_transition_name(value: &str) -> Option<Box<str>> {
    let value = value.trim();
    
    if value == "none" || value.is_empty() {
        None
    } else if value.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        Some(value.into())
    } else {
        None
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_transition_lifecycle() {
        let mut manager = ViewTransitionManager::new();
        
        assert_eq!(manager.state(), TransitionState::Idle);
        
        manager.start_transition().unwrap();
        assert_eq!(manager.state(), TransitionState::Capturing);
        
        manager.register_element("main", 1);
        manager.capture_old_state(
            "main",
            SnapshotRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            WritingMode::HorizontalTb,
            Direction::Ltr,
        );
        
        manager.capture_new_state(
            "main",
            SnapshotRect { x: 50.0, y: 50.0, width: 200.0, height: 200.0 },
            [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            WritingMode::HorizontalTb,
            Direction::Ltr,
        );
        
        manager.start_animating();
        assert_eq!(manager.state(), TransitionState::Animating);
    }
    
    #[test]
    fn test_interpolation() {
        let mut manager = ViewTransitionManager::new();
        manager.start_transition().unwrap();
        manager.register_element("test", 1);
        
        manager.capture_old_state(
            "test",
            SnapshotRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 },
            [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            WritingMode::HorizontalTb,
            Direction::Ltr,
        );
        
        manager.capture_new_state(
            "test",
            SnapshotRect { x: 100.0, y: 100.0, width: 200.0, height: 200.0 },
            [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
            WritingMode::HorizontalTb,
            Direction::Ltr,
        );
        
        manager.start_animating();
        
        // At 50% progress
        if let Some(group) = manager.groups.get_mut("test") {
            group.progress = 0.5;
        }
        
        let rect = manager.get_interpolated_rect("test").unwrap();
        assert_eq!(rect.x, 50.0);
        assert_eq!(rect.y, 50.0);
        assert_eq!(rect.width, 150.0);
        assert_eq!(rect.height, 150.0);
    }
    
    #[test]
    fn test_parse_pseudo() {
        assert!(matches!(
            ViewTransitionPseudo::parse("view-transition"),
            Some((ViewTransitionPseudo::Root, None))
        ));
        
        assert!(matches!(
            ViewTransitionPseudo::parse("view-transition-group(main)"),
            Some((ViewTransitionPseudo::Group, Some(_)))
        ));
        
        assert!(matches!(
            ViewTransitionPseudo::parse("view-transition-old(header)"),
            Some((ViewTransitionPseudo::Old, Some(_)))
        ));
    }
    
    #[test]
    fn test_parse_view_transition_name() {
        assert!(parse_view_transition_name("main-content").is_some());
        assert!(parse_view_transition_name("none").is_none());
        assert!(parse_view_transition_name("").is_none());
    }
}
