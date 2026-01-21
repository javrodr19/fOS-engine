//! Anchor Positioning (Phase 3.3)
//!
//! CSS Anchor Positioning support for positioning elements
//! relative to named anchor elements.

use std::collections::HashMap;

// ============================================================================
// Anchor Position
// ============================================================================

/// Anchor element rectangle
#[derive(Debug, Clone, Copy, Default)]
pub struct AnchorRect {
    /// X position
    pub x: f32,
    /// Y position
    pub y: f32,
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
}

impl AnchorRect {
    /// Create new anchor rect
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Left edge
    pub fn left(&self) -> f32 {
        self.x
    }
    
    /// Right edge
    pub fn right(&self) -> f32 {
        self.x + self.width
    }
    
    /// Top edge
    pub fn top(&self) -> f32 {
        self.y
    }
    
    /// Bottom edge
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }
    
    /// Center X
    pub fn center_x(&self) -> f32 {
        self.x + self.width / 2.0
    }
    
    /// Center Y
    pub fn center_y(&self) -> f32 {
        self.y + self.height / 2.0
    }
}

// ============================================================================
// Anchor Side
// ============================================================================

/// Side of an anchor element
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSide {
    /// Top edge
    Top,
    /// Right edge
    Right,
    /// Bottom edge
    Bottom,
    /// Left edge
    Left,
    /// Horizontal center
    Center,
    /// Start (inline)
    Start,
    /// End (inline)
    End,
    /// Self-start (block)
    SelfStart,
    /// Self-end (block)
    SelfEnd,
}

impl AnchorSide {
    /// Get coordinate from anchor rect
    pub fn get_position(&self, rect: &AnchorRect) -> f32 {
        match self {
            Self::Top | Self::Start | Self::SelfStart => rect.top(),
            Self::Right | Self::End | Self::SelfEnd => rect.right(),
            Self::Bottom => rect.bottom(),
            Self::Left => rect.left(),
            Self::Center => rect.center_y(), // Context dependent
        }
    }
    
    /// Is this a horizontal edge?
    pub fn is_horizontal(&self) -> bool {
        matches!(self, Self::Left | Self::Right | Self::Center | Self::Start | Self::End)
    }
}

// ============================================================================
// Anchor Reference
// ============================================================================

/// Reference to an anchor element
#[derive(Debug, Clone)]
pub struct AnchorReference {
    /// Anchor name (from anchor-name property)
    pub name: String,
    /// Side of the anchor to use
    pub side: AnchorSide,
    /// Fallback value if anchor not found
    pub fallback: Option<f32>,
}

impl AnchorReference {
    /// Create a new anchor reference
    pub fn new(name: impl Into<String>, side: AnchorSide) -> Self {
        Self {
            name: name.into(),
            side,
            fallback: None,
        }
    }
    
    /// Add fallback value
    pub fn with_fallback(mut self, fallback: f32) -> Self {
        self.fallback = Some(fallback);
        self
    }
}

// ============================================================================
// Positioning Area
// ============================================================================

/// Inset values for positioning-area
#[derive(Debug, Clone, Copy, Default)]
pub struct PositioningArea {
    /// Top inset
    pub top: Option<f32>,
    /// Right inset
    pub right: Option<f32>,
    /// Bottom inset
    pub bottom: Option<f32>,
    /// Left inset
    pub left: Option<f32>,
}

impl PositioningArea {
    /// Create from anchor references
    pub fn from_anchors(
        top: Option<AnchorReference>,
        right: Option<AnchorReference>,
        bottom: Option<AnchorReference>,
        left: Option<AnchorReference>,
        registry: &AnchorRegistry,
    ) -> Self {
        Self {
            top: top.and_then(|r| registry.resolve(&r)),
            right: right.and_then(|r| registry.resolve(&r)),
            bottom: bottom.and_then(|r| registry.resolve(&r)),
            left: left.and_then(|r| registry.resolve(&r)),
        }
    }
    
    /// Compute final rect within containing block
    pub fn compute_rect(
        &self,
        containing_block: &AnchorRect,
        element_width: Option<f32>,
        element_height: Option<f32>,
    ) -> AnchorRect {
        let left = self.left.unwrap_or(0.0);
        let right = self.right.unwrap_or(containing_block.width);
        let top = self.top.unwrap_or(0.0);
        let bottom = self.bottom.unwrap_or(containing_block.height);
        
        let width = element_width.unwrap_or((right - left).max(0.0));
        let height = element_height.unwrap_or((bottom - top).max(0.0));
        
        AnchorRect::new(
            containing_block.x + left,
            containing_block.y + top,
            width,
            height,
        )
    }
}

// ============================================================================
// Anchor Registry
// ============================================================================

/// Registry of named anchor elements
#[derive(Debug, Default)]
pub struct AnchorRegistry {
    /// Anchors by name
    anchors: HashMap<String, AnchorRect>,
}

impl AnchorRegistry {
    /// Create new registry
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register an anchor element
    pub fn register(&mut self, name: impl Into<String>, rect: AnchorRect) {
        self.anchors.insert(name.into(), rect);
    }
    
    /// Unregister an anchor
    pub fn unregister(&mut self, name: &str) {
        self.anchors.remove(name);
    }
    
    /// Get anchor rect by name
    pub fn get(&self, name: &str) -> Option<&AnchorRect> {
        self.anchors.get(name)
    }
    
    /// Resolve an anchor reference to a position
    pub fn resolve(&self, reference: &AnchorReference) -> Option<f32> {
        self.anchors.get(&reference.name)
            .map(|rect| reference.side.get_position(rect))
            .or(reference.fallback)
    }
    
    /// Clear all anchors
    pub fn clear(&mut self) {
        self.anchors.clear();
    }
    
    /// Number of registered anchors
    pub fn len(&self) -> usize {
        self.anchors.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }
}

// ============================================================================
// Anchored Element
// ============================================================================

/// Element positioned relative to anchors
#[derive(Debug)]
pub struct AnchoredElement {
    /// Target anchor name
    pub anchor_name: String,
    /// Top positioning
    pub top: Option<AnchorReference>,
    /// Right positioning
    pub right: Option<AnchorReference>,
    /// Bottom positioning
    pub bottom: Option<AnchorReference>,
    /// Left positioning  
    pub left: Option<AnchorReference>,
    /// Element width
    pub width: Option<f32>,
    /// Element height
    pub height: Option<f32>,
    /// Fallback positions (for position-fallback)
    pub fallbacks: Vec<PositioningArea>,
}

impl AnchoredElement {
    /// Create a new anchored element
    pub fn new(anchor_name: impl Into<String>) -> Self {
        Self {
            anchor_name: anchor_name.into(),
            top: None,
            right: None,
            bottom: None,
            left: None,
            width: None,
            height: None,
            fallbacks: Vec::new(),
        }
    }
    
    /// Set top position to anchor bottom
    pub fn below_anchor(mut self) -> Self {
        self.top = Some(AnchorReference::new(self.anchor_name.clone(), AnchorSide::Bottom));
        self
    }
    
    /// Set bottom position to anchor top  
    pub fn above_anchor(mut self) -> Self {
        self.bottom = Some(AnchorReference::new(self.anchor_name.clone(), AnchorSide::Top));
        self
    }
    
    /// Set left position to anchor right
    pub fn after_anchor(mut self) -> Self {
        self.left = Some(AnchorReference::new(self.anchor_name.clone(), AnchorSide::Right));
        self
    }
    
    /// Set right position to anchor left
    pub fn before_anchor(mut self) -> Self {
        self.right = Some(AnchorReference::new(self.anchor_name.clone(), AnchorSide::Left));
        self
    }
    
    /// Compute position
    pub fn compute_position(
        &self,
        registry: &AnchorRegistry,
        containing_block: &AnchorRect,
    ) -> Option<AnchorRect> {
        let area = PositioningArea::from_anchors(
            self.top.clone(),
            self.right.clone(),
            self.bottom.clone(),
            self.left.clone(),
            registry,
        );
        
        Some(area.compute_rect(containing_block, self.width, self.height))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_anchor_rect() {
        let rect = AnchorRect::new(100.0, 50.0, 200.0, 100.0);
        assert_eq!(rect.left(), 100.0);
        assert_eq!(rect.right(), 300.0);
        assert_eq!(rect.top(), 50.0);
        assert_eq!(rect.bottom(), 150.0);
        assert_eq!(rect.center_x(), 200.0);
        assert_eq!(rect.center_y(), 100.0);
    }
    
    #[test]
    fn test_anchor_registry() {
        let mut registry = AnchorRegistry::new();
        
        registry.register("button", AnchorRect::new(100.0, 100.0, 80.0, 30.0));
        
        assert!(registry.get("button").is_some());
        assert!(registry.get("unknown").is_none());
        
        let reference = AnchorReference::new("button", AnchorSide::Bottom);
        let pos = registry.resolve(&reference);
        assert_eq!(pos, Some(130.0)); // 100 + 30
    }
    
    #[test]
    fn test_positioned_element() {
        let mut registry = AnchorRegistry::new();
        registry.register("trigger", AnchorRect::new(50.0, 50.0, 100.0, 40.0));
        
        let element = AnchoredElement::new("trigger")
            .below_anchor();
        
        let containing = AnchorRect::new(0.0, 0.0, 800.0, 600.0);
        let result = element.compute_position(&registry, &containing);
        
        assert!(result.is_some());
        let rect = result.unwrap();
        // Top should be at anchor bottom (50 + 40 = 90)
        assert_eq!(rect.y, 90.0);
    }
    
    #[test]
    fn test_fallback_value() {
        let registry = AnchorRegistry::new();
        
        let reference = AnchorReference::new("missing", AnchorSide::Top)
            .with_fallback(50.0);
        
        let pos = registry.resolve(&reference);
        assert_eq!(pos, Some(50.0));
    }
}
