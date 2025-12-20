//! CSS Mask Properties
//!
//! Implementation of CSS masking (mask, mask-image, etc.)

/// Mask layer definition
#[derive(Debug, Clone)]
pub struct MaskLayer {
    /// Mask image source
    pub image: MaskImage,
    /// Mask position
    pub position: MaskPosition,
    /// Mask size
    pub size: MaskSize,
    /// Mask repeat
    pub repeat: MaskRepeat,
    /// Mask origin
    pub origin: MaskOrigin,
    /// Mask clip
    pub clip: MaskClip,
    /// Mask composite operation
    pub composite: MaskComposite,
    /// Mask mode
    pub mode: MaskMode,
}

impl Default for MaskLayer {
    fn default() -> Self {
        Self {
            image: MaskImage::None,
            position: MaskPosition::default(),
            size: MaskSize::Auto,
            repeat: MaskRepeat::default(),
            origin: MaskOrigin::BorderBox,
            clip: MaskClip::BorderBox,
            composite: MaskComposite::Add,
            mode: MaskMode::MatchSource,
        }
    }
}

/// Mask image type
#[derive(Debug, Clone)]
pub enum MaskImage {
    /// No mask
    None,
    /// URL reference
    Url(String),
    /// Linear gradient
    LinearGradient {
        angle: f32,
        stops: Vec<GradientStop>,
    },
    /// Radial gradient
    RadialGradient {
        shape: RadialShape,
        stops: Vec<GradientStop>,
    },
}

/// Gradient color stop
#[derive(Debug, Clone)]
pub struct GradientStop {
    pub color: (u8, u8, u8, u8),
    pub position: Option<f32>,
}

/// Radial gradient shape
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RadialShape {
    #[default]
    Ellipse,
    Circle,
}

/// Mask position
#[derive(Debug, Clone, Default)]
pub struct MaskPosition {
    pub x: PositionValue,
    pub y: PositionValue,
}

/// Position value
#[derive(Debug, Clone)]
pub enum PositionValue {
    Length(f32),
    Percent(f32),
    Keyword(PositionKeyword),
}

impl Default for PositionValue {
    fn default() -> Self {
        Self::Percent(0.0)
    }
}

/// Position keyword
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionKeyword {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

/// Mask size
#[derive(Debug, Clone, Default)]
pub enum MaskSize {
    #[default]
    Auto,
    Cover,
    Contain,
    Size(f32, f32),
}

/// Mask repeat
#[derive(Debug, Clone, Default)]
pub struct MaskRepeat {
    pub x: RepeatStyle,
    pub y: RepeatStyle,
}

/// Repeat style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatStyle {
    #[default]
    Repeat,
    NoRepeat,
    Space,
    Round,
}

/// Mask origin
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaskOrigin {
    ContentBox,
    PaddingBox,
    #[default]
    BorderBox,
    FillBox,
    StrokeBox,
    ViewBox,
}

/// Mask clip
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaskClip {
    ContentBox,
    PaddingBox,
    #[default]
    BorderBox,
    FillBox,
    StrokeBox,
    ViewBox,
    NoClip,
}

/// Mask composite operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaskComposite {
    #[default]
    Add,
    Subtract,
    Intersect,
    Exclude,
}

/// Mask mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaskMode {
    Alpha,
    Luminance,
    #[default]
    MatchSource,
}

/// CSS isolation property
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Isolation {
    #[default]
    Auto,
    Isolate,
}

impl Isolation {
    pub fn from_css(value: &str) -> Self {
        match value.trim() {
            "isolate" => Self::Isolate,
            _ => Self::Auto,
        }
    }
    
    pub fn to_css(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Isolate => "isolate",
        }
    }
    
    /// Check if this creates a new stacking context
    pub fn creates_stacking_context(&self) -> bool {
        *self == Self::Isolate
    }
}

/// Complete mask definition for an element
#[derive(Debug, Clone, Default)]
pub struct Mask {
    pub layers: Vec<MaskLayer>,
    pub border_mode: MaskBorderMode,
}

impl Mask {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_layer(&mut self, layer: MaskLayer) {
        self.layers.push(layer);
    }
    
    pub fn has_mask(&self) -> bool {
        self.layers.iter().any(|l| !matches!(l.image, MaskImage::None))
    }
}

/// Mask border mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MaskBorderMode {
    #[default]
    Alpha,
    Luminance,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_isolation() {
        assert_eq!(Isolation::from_css("isolate"), Isolation::Isolate);
        assert_eq!(Isolation::from_css("auto"), Isolation::Auto);
        assert!(Isolation::Isolate.creates_stacking_context());
    }
    
    #[test]
    fn test_mask_layer() {
        let layer = MaskLayer::default();
        assert!(matches!(layer.image, MaskImage::None));
    }
    
    #[test]
    fn test_mask() {
        let mut mask = Mask::new();
        assert!(!mask.has_mask());
        
        mask.add_layer(MaskLayer {
            image: MaskImage::Url("mask.svg".to_string()),
            ..Default::default()
        });
        assert!(mask.has_mask());
    }
}
