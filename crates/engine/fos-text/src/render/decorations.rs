//! Text Decorations
//!
//! Support for text-decoration CSS properties including
//! underline, overline, line-through, and styling options.

/// Text decoration line type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDecorationLine {
    /// No decoration
    None,
    /// Underline
    Underline,
    /// Overline (above text)
    Overline,
    /// Line through text (strikethrough)
    LineThrough,
}

/// Text decoration style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDecorationStyle {
    /// Solid line
    #[default]
    Solid,
    /// Double line
    Double,
    /// Dotted line
    Dotted,
    /// Dashed line
    Dashed,
    /// Wavy line
    Wavy,
}

impl TextDecorationStyle {
    pub fn from_css(value: &str) -> Self {
        match value {
            "solid" => Self::Solid,
            "double" => Self::Double,
            "dotted" => Self::Dotted,
            "dashed" => Self::Dashed,
            "wavy" => Self::Wavy,
            _ => Self::Solid,
        }
    }
    
    pub fn to_css(&self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Double => "double",
            Self::Dotted => "dotted",
            Self::Dashed => "dashed",
            Self::Wavy => "wavy",
        }
    }
}

/// Text decoration color
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDecorationColor {
    /// Current text color
    CurrentColor,
    /// Specific color (RGBA)
    Rgba(u8, u8, u8, u8),
}

impl Default for TextDecorationColor {
    fn default() -> Self {
        Self::CurrentColor
    }
}

/// Complete text decoration specification
#[derive(Debug, Clone, Default)]
pub struct TextDecoration {
    /// Which lines to draw
    pub lines: Vec<TextDecorationLine>,
    /// Line style
    pub style: TextDecorationStyle,
    /// Line color
    pub color: TextDecorationColor,
    /// Line thickness (in pixels, 0 = auto)
    pub thickness: f32,
    /// Skip ink (skip over glyph descenders)
    pub skip_ink: TextDecorationSkipInk,
    /// Underline position
    pub underline_position: UnderlinePosition,
    /// Underline offset from baseline
    pub underline_offset: f32,
}

impl TextDecoration {
    /// Create a new text decoration
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add an underline
    pub fn underline(mut self) -> Self {
        if !self.lines.contains(&TextDecorationLine::Underline) {
            self.lines.push(TextDecorationLine::Underline);
        }
        self
    }
    
    /// Add an overline
    pub fn overline(mut self) -> Self {
        if !self.lines.contains(&TextDecorationLine::Overline) {
            self.lines.push(TextDecorationLine::Overline);
        }
        self
    }
    
    /// Add strikethrough
    pub fn line_through(mut self) -> Self {
        if !self.lines.contains(&TextDecorationLine::LineThrough) {
            self.lines.push(TextDecorationLine::LineThrough);
        }
        self
    }
    
    /// Set style
    pub fn with_style(mut self, style: TextDecorationStyle) -> Self {
        self.style = style;
        self
    }
    
    /// Set color
    pub fn with_color(mut self, r: u8, g: u8, b: u8, a: u8) -> Self {
        self.color = TextDecorationColor::Rgba(r, g, b, a);
        self
    }
    
    /// Set thickness
    pub fn with_thickness(mut self, thickness: f32) -> Self {
        self.thickness = thickness;
        self
    }
    
    /// Check if has underline
    pub fn has_underline(&self) -> bool {
        self.lines.contains(&TextDecorationLine::Underline)
    }
    
    /// Check if has overline
    pub fn has_overline(&self) -> bool {
        self.lines.contains(&TextDecorationLine::Overline)
    }
    
    /// Check if has line-through
    pub fn has_line_through(&self) -> bool {
        self.lines.contains(&TextDecorationLine::LineThrough)
    }
    
    /// Check if has any decoration
    pub fn has_any(&self) -> bool {
        !self.lines.is_empty() && 
        !self.lines.iter().all(|l| *l == TextDecorationLine::None)
    }
}

/// Skip ink behavior for decorations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextDecorationSkipInk {
    /// Skip ink (skip descenders)
    #[default]
    Auto,
    /// Always skip
    All,
    /// Never skip
    None,
}

/// Underline position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UnderlinePosition {
    /// Automatic (font metrics)
    #[default]
    Auto,
    /// Under the text baseline
    Under,
    /// Left for vertical text
    Left,
    /// Right for vertical text
    Right,
}

/// Decoration geometry for rendering
#[derive(Debug, Clone)]
pub struct DecorationGeometry {
    /// X position
    pub x: f32,
    /// Y position
    pub y: f32,
    /// Width (length of line)
    pub width: f32,
    /// Thickness
    pub thickness: f32,
    /// Decoration type
    pub line_type: TextDecorationLine,
    /// Decoration style
    pub style: TextDecorationStyle,
    /// Color (r, g, b, a)
    pub color: (u8, u8, u8, u8),
    /// Skip regions (for skip-ink)
    pub skip_regions: Vec<(f32, f32)>,
}

impl DecorationGeometry {
    /// Create new decoration geometry
    pub fn new(
        x: f32, 
        y: f32, 
        width: f32, 
        thickness: f32, 
        line_type: TextDecorationLine
    ) -> Self {
        Self {
            x,
            y,
            width,
            thickness,
            line_type,
            style: TextDecorationStyle::Solid,
            color: (0, 0, 0, 255),
            skip_regions: Vec::new(),
        }
    }
}

/// Calculate decoration positions for a text run
pub fn calculate_decorations(
    decoration: &TextDecoration,
    x: f32,
    baseline: f32,
    width: f32,
    font_size: f32,
    ascender: f32,
    descender: f32,
) -> Vec<DecorationGeometry> {
    let mut geometries = Vec::new();
    
    let thickness = if decoration.thickness > 0.0 {
        decoration.thickness
    } else {
        (font_size / 14.0).max(1.0)
    };
    
    let color = match decoration.color {
        TextDecorationColor::CurrentColor => (0, 0, 0, 255),
        TextDecorationColor::Rgba(r, g, b, a) => (r, g, b, a),
    };
    
    for line_type in &decoration.lines {
        let y = match line_type {
            TextDecorationLine::None => continue,
            TextDecorationLine::Underline => {
                // Position below baseline
                baseline + decoration.underline_offset + thickness
            }
            TextDecorationLine::Overline => {
                // Position above ascender
                baseline - ascender - thickness
            }
            TextDecorationLine::LineThrough => {
                // Position at middle of x-height (approximately)
                baseline - (ascender * 0.35)
            }
        };
        
        let mut geom = DecorationGeometry::new(x, y, width, thickness, *line_type);
        geom.style = decoration.style;
        geom.color = color;
        geometries.push(geom);
    }
    
    geometries
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_text_decoration() {
        let deco = TextDecoration::new()
            .underline()
            .with_style(TextDecorationStyle::Wavy)
            .with_color(255, 0, 0, 255);
        
        assert!(deco.has_underline());
        assert!(!deco.has_overline());
        assert_eq!(deco.style, TextDecorationStyle::Wavy);
    }
    
    #[test]
    fn test_decoration_geometry() {
        let deco = TextDecoration::new()
            .underline()
            .line_through();
        
        let geometries = calculate_decorations(
            &deco,
            0.0,    // x
            100.0,  // baseline
            200.0,  // width
            16.0,   // font_size
            12.0,   // ascender
            4.0,    // descender
        );
        
        assert_eq!(geometries.len(), 2);
    }
    
    #[test]
    fn test_decoration_style_parse() {
        assert_eq!(TextDecorationStyle::from_css("wavy"), TextDecorationStyle::Wavy);
        assert_eq!(TextDecorationStyle::from_css("solid"), TextDecorationStyle::Solid);
    }
}
