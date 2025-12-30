//! Paint operations and styles

use crate::Color;

/// Fill style for painting
#[derive(Debug, Clone)]
pub enum FillStyle {
    /// Solid color fill
    Solid(Color),
    /// No fill
    None,
}

impl Default for FillStyle {
    fn default() -> Self {
        FillStyle::None
    }
}

impl From<Color> for FillStyle {
    fn from(color: Color) -> Self {
        FillStyle::Solid(color)
    }
}

/// Stroke style for borders and lines
#[derive(Debug, Clone)]
pub struct StrokeStyle {
    /// Stroke color
    pub color: Color,
    /// Stroke width
    pub width: f32,
    /// Dash pattern (None for solid)
    pub dash: Option<DashPattern>,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            color: Color::BLACK,
            width: 1.0,
            dash: None,
        }
    }
}

impl StrokeStyle {
    /// Create solid stroke
    pub fn solid(color: Color, width: f32) -> Self {
        Self { color, width, dash: None }
    }
    
    /// Create dashed stroke
    pub fn dashed(color: Color, width: f32, dash_length: f32, gap_length: f32) -> Self {
        Self {
            color,
            width,
            dash: Some(DashPattern { dash: dash_length, gap: gap_length }),
        }
    }
    
    /// Create dotted stroke
    pub fn dotted(color: Color, width: f32) -> Self {
        Self::dashed(color, width, width, width)
    }
}

/// Dash pattern
#[derive(Debug, Clone, Copy)]
pub struct DashPattern {
    pub dash: f32,
    pub gap: f32,
}

/// Border style (CSS-like)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BorderStyle {
    #[default]
    None,
    Solid,
    Dashed,
    Dotted,
    Double,
    Hidden,
}

/// Four-sided border specification
#[derive(Debug, Clone, Default)]
pub struct Border {
    pub top: BorderSide,
    pub right: BorderSide,
    pub bottom: BorderSide,
    pub left: BorderSide,
}

impl Border {
    /// Create uniform border on all sides
    pub fn all(width: f32, style: BorderStyle, color: Color) -> Self {
        let side = BorderSide { width, style, color };
        Self {
            top: side.clone(),
            right: side.clone(),
            bottom: side.clone(),
            left: side,
        }
    }
    
    /// Check if border has any visible sides
    pub fn has_visible(&self) -> bool {
        self.top.is_visible() || self.right.is_visible() ||
        self.bottom.is_visible() || self.left.is_visible()
    }
}

/// Single border side
#[derive(Debug, Clone, Default)]
pub struct BorderSide {
    pub width: f32,
    pub style: BorderStyle,
    pub color: Color,
}

impl BorderSide {
    pub fn is_visible(&self) -> bool {
        self.width > 0.0 && self.style != BorderStyle::None && self.style != BorderStyle::Hidden
    }
}

/// Border radius (CSS-like)
#[derive(Debug, Clone, Copy, Default)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_right: f32,
    pub bottom_left: f32,
}

impl BorderRadius {
    /// Uniform radius on all corners
    pub fn all(radius: f32) -> Self {
        Self {
            top_left: radius,
            top_right: radius,
            bottom_right: radius,
            bottom_left: radius,
        }
    }
    
    /// Check if any radius is non-zero
    pub fn has_radius(&self) -> bool {
        self.top_left > 0.0 || self.top_right > 0.0 ||
        self.bottom_right > 0.0 || self.bottom_left > 0.0
    }
    
    /// Get maximum radius
    pub fn max(&self) -> f32 {
        self.top_left.max(self.top_right).max(self.bottom_right).max(self.bottom_left)
    }
}
