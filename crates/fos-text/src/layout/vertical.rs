//! Vertical Text Layout
//!
//! Support for vertical writing modes (CJK and other vertical scripts).

/// Writing mode for text layout
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritingMode {
    /// Horizontal left-to-right, top-to-bottom (default)
    #[default]
    HorizontalTb,
    /// Vertical right-to-left (traditional CJK)
    VerticalRl,
    /// Vertical left-to-right
    VerticalLr,
    /// Sideways right-to-left
    SidewaysRl,
    /// Sideways left-to-right
    SidewaysLr,
}

impl WritingMode {
    /// Parse from CSS value
    pub fn from_css(value: &str) -> Self {
        match value {
            "horizontal-tb" => Self::HorizontalTb,
            "vertical-rl" => Self::VerticalRl,
            "vertical-lr" => Self::VerticalLr,
            "sideways-rl" => Self::SidewaysRl,
            "sideways-lr" => Self::SidewaysLr,
            _ => Self::HorizontalTb,
        }
    }
    
    /// Convert to CSS value
    pub fn to_css(&self) -> &'static str {
        match self {
            Self::HorizontalTb => "horizontal-tb",
            Self::VerticalRl => "vertical-rl",
            Self::VerticalLr => "vertical-lr",
            Self::SidewaysRl => "sideways-rl",
            Self::SidewaysLr => "sideways-lr",
        }
    }
    
    /// Check if vertical
    pub fn is_vertical(&self) -> bool {
        !matches!(self, Self::HorizontalTb)
    }
    
    /// Check if sideways
    pub fn is_sideways(&self) -> bool {
        matches!(self, Self::SidewaysRl | Self::SidewaysLr)
    }
    
    /// Get block flow direction
    pub fn block_direction(&self) -> Direction {
        match self {
            Self::HorizontalTb => Direction::TopToBottom,
            Self::VerticalRl => Direction::RightToLeft,
            Self::VerticalLr => Direction::LeftToRight,
            Self::SidewaysRl => Direction::RightToLeft,
            Self::SidewaysLr => Direction::LeftToRight,
        }
    }
    
    /// Get inline flow direction
    pub fn inline_direction(&self) -> Direction {
        match self {
            Self::HorizontalTb => Direction::LeftToRight, // Can be RTL too
            Self::VerticalRl | Self::VerticalLr => Direction::TopToBottom,
            Self::SidewaysRl => Direction::BottomToTop,
            Self::SidewaysLr => Direction::TopToBottom,
        }
    }
}

/// Text direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

/// Text orientation for vertical text
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextOrientation {
    /// Characters are upright (rotated 90° for scripts that need it)
    #[default]
    Mixed,
    /// Characters are upright (no rotation)
    Upright,
    /// Characters are rotated 90° clockwise
    Sideways,
}

impl TextOrientation {
    pub fn from_css(value: &str) -> Self {
        match value {
            "mixed" => Self::Mixed,
            "upright" => Self::Upright,
            "sideways" => Self::Sideways,
            _ => Self::Mixed,
        }
    }
    
    pub fn to_css(&self) -> &'static str {
        match self {
            Self::Mixed => "mixed",
            Self::Upright => "upright",
            Self::Sideways => "sideways",
        }
    }
}

/// Glyph rotation for vertical text
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphRotation {
    /// No rotation (upright)
    None,
    /// Rotate 90° clockwise
    Cw90,
    /// Rotate 180°
    Rotate180,
    /// Rotate 90° counter-clockwise
    Ccw90,
}

impl GlyphRotation {
    /// Get rotation angle in degrees
    pub fn degrees(&self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Cw90 => 90.0,
            Self::Rotate180 => 180.0,
            Self::Ccw90 => -90.0,
        }
    }
    
    /// Get rotation angle in radians
    pub fn radians(&self) -> f32 {
        self.degrees().to_radians()
    }
}

/// Determine if a character should be rotated in vertical text
pub fn should_rotate_in_vertical(c: char, orientation: TextOrientation) -> GlyphRotation {
    match orientation {
        TextOrientation::Sideways => GlyphRotation::Cw90,
        TextOrientation::Upright => GlyphRotation::None,
        TextOrientation::Mixed => {
            // In mixed mode, most characters are upright, but
            // Latin, Greek, Cyrillic characters are rotated
            if is_upright_in_mixed(c) {
                GlyphRotation::None
            } else {
                GlyphRotation::Cw90
            }
        }
    }
}

/// Check if character should be upright in mixed orientation
fn is_upright_in_mixed(c: char) -> bool {
    // CJK characters are upright
    if is_cjk(c) {
        return true;
    }
    
    // CJK punctuation is upright
    if is_cjk_punctuation(c) {
        return true;
    }
    
    // Most other scripts are rotated
    false
}

/// Check if character is CJK
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{4E00}'..='\u{9FFF}' |   // CJK Unified Ideographs
        '\u{3400}'..='\u{4DBF}' |   // CJK Extension A
        '\u{20000}'..='\u{2A6DF}' | // CJK Extension B
        '\u{2A700}'..='\u{2B73F}' | // CJK Extension C
        '\u{2B740}'..='\u{2B81F}' | // CJK Extension D
        '\u{F900}'..='\u{FAFF}' |   // CJK Compatibility Ideographs
        '\u{3040}'..='\u{309F}' |   // Hiragana
        '\u{30A0}'..='\u{30FF}' |   // Katakana
        '\u{AC00}'..='\u{D7AF}'     // Hangul Syllables
    )
}

/// Check if character is CJK punctuation
fn is_cjk_punctuation(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{303F}' |   // CJK Punctuation
        '\u{FF00}'..='\u{FFEF}'     // Fullwidth Forms
    )
}

/// Vertical text layout context
#[derive(Debug, Clone)]
pub struct VerticalTextContext {
    pub writing_mode: WritingMode,
    pub text_orientation: TextOrientation,
    pub direction: Direction,
}

impl Default for VerticalTextContext {
    fn default() -> Self {
        Self {
            writing_mode: WritingMode::HorizontalTb,
            text_orientation: TextOrientation::Mixed,
            direction: Direction::LeftToRight,
        }
    }
}

impl VerticalTextContext {
    /// Create context for vertical right-to-left
    pub fn vertical_rl() -> Self {
        Self {
            writing_mode: WritingMode::VerticalRl,
            text_orientation: TextOrientation::Mixed,
            direction: Direction::TopToBottom,
        }
    }
    
    /// Transform x,y coordinates for vertical layout
    pub fn transform_position(&self, x: f32, y: f32, container_width: f32, container_height: f32) -> (f32, f32) {
        match self.writing_mode {
            WritingMode::HorizontalTb => (x, y),
            WritingMode::VerticalRl => (container_width - y, x),
            WritingMode::VerticalLr => (y, x),
            WritingMode::SidewaysRl => (container_width - y, x),
            WritingMode::SidewaysLr => (y, container_height - x),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_writing_mode() {
        assert!(!WritingMode::HorizontalTb.is_vertical());
        assert!(WritingMode::VerticalRl.is_vertical());
        assert!(WritingMode::SidewaysRl.is_sideways());
    }
    
    #[test]
    fn test_writing_mode_parse() {
        assert_eq!(WritingMode::from_css("vertical-rl"), WritingMode::VerticalRl);
        assert_eq!(WritingMode::from_css("horizontal-tb"), WritingMode::HorizontalTb);
    }
    
    #[test]
    fn test_cjk_detection() {
        assert!(is_cjk('中'));
        assert!(is_cjk('あ'));
        assert!(!is_cjk('A'));
    }
    
    #[test]
    fn test_glyph_rotation() {
        assert_eq!(should_rotate_in_vertical('中', TextOrientation::Mixed), GlyphRotation::None);
        assert_eq!(should_rotate_in_vertical('A', TextOrientation::Mixed), GlyphRotation::Cw90);
    }
}
