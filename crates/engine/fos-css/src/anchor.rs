//! CSS Anchor Positioning
//!
//! Implementation of CSS Anchor Positioning specification.
//! Allows positioning elements relative to named anchor elements.

use std::collections::HashMap;

// ============================================================================
// Anchor Types
// ============================================================================

/// Unique anchor name
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AnchorName(pub Box<str>);

impl AnchorName {
    pub fn new(name: &str) -> Self {
        Self(name.into())
    }
    
    /// Check if this is the implicit (default) anchor
    pub fn is_implicit(&self) -> bool {
        self.0.as_ref() == "--implicit"
    }
}

/// Anchor side for positioning
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSide {
    Top,
    Right,
    Bottom,
    Left,
    Start,
    End,
    SelfStart,
    SelfEnd,
    Center,
    /// Percentage along the anchor's axis
    Percentage(i32), // Fixed-point percentage * 100
}

/// Anchor size function
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorSize {
    Width,
    Height,
    Block,
    Inline,
    SelfBlock,
    SelfInline,
}

// ============================================================================
// Anchor Registry
// ============================================================================

/// Information about a registered anchor element
#[derive(Debug, Clone)]
pub struct AnchorInfo {
    /// Element ID
    pub element_id: u32,
    /// Bounding box (x, y, width, height)
    pub rect: AnchorRect,
    /// Writing mode (for logical properties)
    pub writing_mode: WritingMode,
}

/// Anchor bounding rectangle
#[derive(Debug, Clone, Copy, Default)]
pub struct AnchorRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl AnchorRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    pub fn top(&self) -> f32 { self.y }
    pub fn right(&self) -> f32 { self.x + self.width }
    pub fn bottom(&self) -> f32 { self.y + self.height }
    pub fn left(&self) -> f32 { self.x }
    pub fn center_x(&self) -> f32 { self.x + self.width / 2.0 }
    pub fn center_y(&self) -> f32 { self.y + self.height / 2.0 }
}

/// Writing mode for logical properties
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
    VerticalLr,
}

/// Registry of anchor elements
#[derive(Debug, Default)]
pub struct AnchorRegistry {
    /// Named anchors
    anchors: HashMap<AnchorName, AnchorInfo>,
    /// Element to anchor name mapping
    element_anchors: HashMap<u32, AnchorName>,
}

impl AnchorRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register an anchor element
    pub fn register(
        &mut self,
        name: AnchorName,
        element_id: u32,
        rect: AnchorRect,
        writing_mode: WritingMode,
    ) {
        self.anchors.insert(name.clone(), AnchorInfo {
            element_id,
            rect,
            writing_mode,
        });
        self.element_anchors.insert(element_id, name);
    }
    
    /// Get anchor by name
    pub fn get(&self, name: &AnchorName) -> Option<&AnchorInfo> {
        self.anchors.get(name)
    }
    
    /// Get anchor name for an element
    pub fn get_anchor_name(&self, element_id: u32) -> Option<&AnchorName> {
        self.element_anchors.get(&element_id)
    }
    
    /// Update anchor rectangle (e.g., after layout)
    pub fn update_rect(&mut self, name: &AnchorName, rect: AnchorRect) {
        if let Some(info) = self.anchors.get_mut(name) {
            info.rect = rect;
        }
    }
    
    /// Remove an anchor
    pub fn remove(&mut self, name: &AnchorName) {
        if let Some(info) = self.anchors.remove(name) {
            self.element_anchors.remove(&info.element_id);
        }
    }
    
    /// Clear all anchors
    pub fn clear(&mut self) {
        self.anchors.clear();
        self.element_anchors.clear();
    }
    
    /// Number of anchors
    pub fn len(&self) -> usize {
        self.anchors.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.anchors.is_empty()
    }
}

// ============================================================================
// Anchor Functions
// ============================================================================

/// Evaluate anchor() function
pub fn evaluate_anchor(
    registry: &AnchorRegistry,
    anchor_name: &AnchorName,
    side: AnchorSide,
    fallback: Option<f32>,
) -> Option<f32> {
    let info = registry.get(anchor_name)?;
    
    let value = match side {
        AnchorSide::Top => info.rect.top(),
        AnchorSide::Right => info.rect.right(),
        AnchorSide::Bottom => info.rect.bottom(),
        AnchorSide::Left => info.rect.left(),
        AnchorSide::Center => {
            // Default to horizontal center
            info.rect.center_x()
        }
        AnchorSide::Start => {
            match info.writing_mode {
                WritingMode::HorizontalTb => info.rect.left(),
                WritingMode::VerticalRl => info.rect.top(),
                WritingMode::VerticalLr => info.rect.top(),
            }
        }
        AnchorSide::End => {
            match info.writing_mode {
                WritingMode::HorizontalTb => info.rect.right(),
                WritingMode::VerticalRl => info.rect.bottom(),
                WritingMode::VerticalLr => info.rect.bottom(),
            }
        }
        AnchorSide::SelfStart | AnchorSide::SelfEnd => {
            // These depend on the positioned element's writing mode
            // For now, treat as start/end
            if side == AnchorSide::SelfStart {
                info.rect.left()
            } else {
                info.rect.right()
            }
        }
        AnchorSide::Percentage(pct) => {
            // Interpolate along the inline axis
            let t = pct as f32 / 100.0;
            info.rect.left() + t * info.rect.width
        }
    };
    
    Some(value).or(fallback)
}

/// Evaluate anchor-size() function
pub fn evaluate_anchor_size(
    registry: &AnchorRegistry,
    anchor_name: &AnchorName,
    size: AnchorSize,
    fallback: Option<f32>,
) -> Option<f32> {
    let info = registry.get(anchor_name)?;
    
    let value = match size {
        AnchorSize::Width => info.rect.width,
        AnchorSize::Height => info.rect.height,
        AnchorSize::Block => {
            match info.writing_mode {
                WritingMode::HorizontalTb => info.rect.height,
                WritingMode::VerticalRl | WritingMode::VerticalLr => info.rect.width,
            }
        }
        AnchorSize::Inline => {
            match info.writing_mode {
                WritingMode::HorizontalTb => info.rect.width,
                WritingMode::VerticalRl | WritingMode::VerticalLr => info.rect.height,
            }
        }
        AnchorSize::SelfBlock | AnchorSize::SelfInline => {
            // These depend on the positioned element's writing mode
            if size == AnchorSize::SelfBlock {
                info.rect.height
            } else {
                info.rect.width
            }
        }
    };
    
    Some(value).or(fallback)
}

// ============================================================================
// Position Fallback
// ============================================================================

/// A position fallback option
#[derive(Debug, Clone)]
pub struct PositionFallback {
    /// Fallback name
    pub name: Box<str>,
    /// Fallback options in order
    pub options: Vec<FallbackOption>,
}

/// A single fallback option
#[derive(Debug, Clone)]
pub struct FallbackOption {
    /// Property-value pairs
    pub properties: Vec<(Box<str>, Box<str>)>,
}

/// Registry of position fallbacks
#[derive(Debug, Default)]
pub struct PositionFallbackRegistry {
    fallbacks: HashMap<Box<str>, PositionFallback>,
}

impl PositionFallbackRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a position fallback
    pub fn register(&mut self, name: &str, options: Vec<FallbackOption>) {
        self.fallbacks.insert(name.into(), PositionFallback {
            name: name.into(),
            options,
        });
    }
    
    /// Get a fallback by name
    pub fn get(&self, name: &str) -> Option<&PositionFallback> {
        self.fallbacks.get(name)
    }
    
    /// Try fallback options until one doesn't overflow
    pub fn try_fallbacks(
        &self,
        name: &str,
        check_overflow: impl Fn(&FallbackOption) -> bool,
    ) -> Option<&FallbackOption> {
        let fallback = self.fallbacks.get(name)?;
        
        for option in &fallback.options {
            if !check_overflow(option) {
                return Some(option);
            }
        }
        
        // All overflow, return last option
        fallback.options.last()
    }
}

// ============================================================================
// Anchor Position Parser
// ============================================================================

/// Parse anchor() function from CSS value
pub fn parse_anchor_function(value: &str) -> Option<(AnchorName, AnchorSide, Option<f32>)> {
    let value = value.trim();
    
    if !value.starts_with("anchor(") || !value.ends_with(')') {
        return None;
    }
    
    let inner = &value[7..value.len() - 1];
    let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
    
    if parts.is_empty() {
        return None;
    }
    
    // Parse anchor name
    let (name_part, side_part) = if parts[0].starts_with("--") {
        (parts.get(0).copied(), parts.get(1).copied())
    } else {
        (None, parts.get(0).copied())
    };
    
    let anchor_name = name_part
        .map(|n| AnchorName::new(n))
        .unwrap_or_else(|| AnchorName::new("--implicit"));
    
    let side = side_part.and_then(parse_anchor_side).unwrap_or(AnchorSide::Center);
    
    // Parse fallback
    let fallback = parts.last()
        .and_then(|s| s.strip_suffix("px"))
        .and_then(|s| s.parse::<f32>().ok());
    
    Some((anchor_name, side, fallback))
}

fn parse_anchor_side(s: &str) -> Option<AnchorSide> {
    match s.trim() {
        "top" => Some(AnchorSide::Top),
        "right" => Some(AnchorSide::Right),
        "bottom" => Some(AnchorSide::Bottom),
        "left" => Some(AnchorSide::Left),
        "start" => Some(AnchorSide::Start),
        "end" => Some(AnchorSide::End),
        "self-start" => Some(AnchorSide::SelfStart),
        "self-end" => Some(AnchorSide::SelfEnd),
        "center" => Some(AnchorSide::Center),
        s if s.ends_with('%') => {
            s[..s.len() - 1].parse::<i32>().ok().map(AnchorSide::Percentage)
        }
        _ => None,
    }
}

/// Parse anchor-name property
pub fn parse_anchor_name(value: &str) -> Option<AnchorName> {
    let value = value.trim();
    
    if value == "none" || value.is_empty() {
        None
    } else if value.starts_with("--") {
        Some(AnchorName::new(value))
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
    fn test_anchor_registry() {
        let mut registry = AnchorRegistry::new();
        let name = AnchorName::new("--tooltip-anchor");
        
        registry.register(
            name.clone(),
            1,
            AnchorRect::new(100.0, 200.0, 50.0, 30.0),
            WritingMode::HorizontalTb,
        );
        
        let info = registry.get(&name).unwrap();
        assert_eq!(info.element_id, 1);
        assert_eq!(info.rect.width, 50.0);
    }
    
    #[test]
    fn test_evaluate_anchor() {
        let mut registry = AnchorRegistry::new();
        let name = AnchorName::new("--btn");
        
        registry.register(
            name.clone(),
            1,
            AnchorRect::new(100.0, 200.0, 80.0, 40.0),
            WritingMode::HorizontalTb,
        );
        
        assert_eq!(evaluate_anchor(&registry, &name, AnchorSide::Top, None), Some(200.0));
        assert_eq!(evaluate_anchor(&registry, &name, AnchorSide::Right, None), Some(180.0));
        assert_eq!(evaluate_anchor(&registry, &name, AnchorSide::Bottom, None), Some(240.0));
        assert_eq!(evaluate_anchor(&registry, &name, AnchorSide::Left, None), Some(100.0));
    }
    
    #[test]
    fn test_evaluate_anchor_size() {
        let mut registry = AnchorRegistry::new();
        let name = AnchorName::new("--btn");
        
        registry.register(
            name.clone(),
            1,
            AnchorRect::new(100.0, 200.0, 80.0, 40.0),
            WritingMode::HorizontalTb,
        );
        
        assert_eq!(evaluate_anchor_size(&registry, &name, AnchorSize::Width, None), Some(80.0));
        assert_eq!(evaluate_anchor_size(&registry, &name, AnchorSize::Height, None), Some(40.0));
    }
    
    #[test]
    fn test_parse_anchor_function() {
        let result = parse_anchor_function("anchor(--tooltip, bottom)");
        assert!(result.is_some());
        
        let (name, side, fallback) = result.unwrap();
        assert_eq!(name.0.as_ref(), "--tooltip");
        assert_eq!(side, AnchorSide::Bottom);
        assert!(fallback.is_none());
    }
    
    #[test]
    fn test_parse_anchor_name() {
        assert!(parse_anchor_name("--my-anchor").is_some());
        assert!(parse_anchor_name("none").is_none());
        assert!(parse_anchor_name("").is_none());
    }
    
    #[test]
    fn test_position_fallback() {
        let mut registry = PositionFallbackRegistry::new();
        
        registry.register("--flip", vec![
            FallbackOption {
                properties: vec![("top".into(), "anchor(bottom)".into())],
            },
            FallbackOption {
                properties: vec![("bottom".into(), "anchor(top)".into())],
            },
        ]);
        
        let fallback = registry.get("--flip").unwrap();
        assert_eq!(fallback.options.len(), 2);
    }
}
