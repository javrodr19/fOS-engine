//! Emoji and Color Font Support
//!
//! Support for color emoji fonts (COLR, CBDT, sbix formats).

use std::collections::HashMap;

/// Color font format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorFontFormat {
    /// COLR/CPAL (layered color glyphs)
    Colr,
    /// CBDT/CBLC (color bitmap data)
    Cbdt,
    /// sbix (Apple color bitmap)
    Sbix,
    /// SVG (SVG outlines in font)
    Svg,
}

/// Color glyph data
#[derive(Debug, Clone)]
pub enum ColorGlyph {
    /// Layered color glyph (COLR format)
    Layered(Vec<ColorLayer>),
    /// Bitmap glyph (CBDT/sbix)
    Bitmap(ColorBitmap),
    /// SVG glyph
    Svg(String),
}

/// Color layer for COLR format
#[derive(Debug, Clone)]
pub struct ColorLayer {
    /// Glyph ID for this layer
    pub glyph_id: u16,
    /// Palette index for this layer
    pub palette_index: u16,
}

/// Color bitmap for emoji
#[derive(Debug, Clone)]
pub struct ColorBitmap {
    /// Width in pixels
    pub width: u16,
    /// Height in pixels
    pub height: u16,
    /// Horizontal bearing X
    pub bearing_x: i16,
    /// Horizontal bearing Y
    pub bearing_y: i16,
    /// Advance width
    pub advance: u16,
    /// Bitmap data (RGBA or PNG)
    pub data: Vec<u8>,
    /// Data format
    pub format: BitmapFormat,
}

/// Bitmap data format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BitmapFormat {
    /// Raw RGBA
    Rgba,
    /// PNG compressed
    Png,
    /// JPEG compressed
    Jpeg,
}

/// Color palette entry (CPAL format)
#[derive(Debug, Clone, Copy, Default)]
pub struct PaletteColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl PaletteColor {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    
    pub fn to_rgba(&self) -> (u8, u8, u8, u8) {
        (self.r, self.g, self.b, self.a)
    }
}

/// Color palette (CPAL table)
#[derive(Debug, Clone, Default)]
pub struct ColorPalette {
    pub colors: Vec<PaletteColor>,
}

impl ColorPalette {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_color(&mut self, color: PaletteColor) {
        self.colors.push(color);
    }
    
    pub fn get(&self, index: usize) -> Option<&PaletteColor> {
        self.colors.get(index)
    }
}

/// Emoji renderer
#[derive(Debug, Default)]
pub struct EmojiRenderer {
    /// Cached color glyphs
    color_glyphs: HashMap<u16, ColorGlyph>,
    /// Current palette
    palette: ColorPalette,
    /// Preferred format
    preferred_format: Option<ColorFontFormat>,
}

impl EmojiRenderer {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set preferred color font format
    pub fn set_preferred_format(&mut self, format: ColorFontFormat) {
        self.preferred_format = Some(format);
    }
    
    /// Set color palette
    pub fn set_palette(&mut self, palette: ColorPalette) {
        self.palette = palette;
    }
    
    /// Cache a color glyph
    pub fn cache_glyph(&mut self, glyph_id: u16, glyph: ColorGlyph) {
        self.color_glyphs.insert(glyph_id, glyph);
    }
    
    /// Get cached color glyph
    pub fn get_glyph(&self, glyph_id: u16) -> Option<&ColorGlyph> {
        self.color_glyphs.get(&glyph_id)
    }
    
    /// Check if glyph has color data
    pub fn has_color_glyph(&self, glyph_id: u16) -> bool {
        self.color_glyphs.contains_key(&glyph_id)
    }
    
    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.color_glyphs.clear();
    }
}

/// Check if character is an emoji
pub fn is_emoji(c: char) -> bool {
    // Common emoji ranges
    matches!(c as u32,
        0x1F600..=0x1F64F |  // Emoticons
        0x1F300..=0x1F5FF |  // Misc Symbols and Pictographs
        0x1F680..=0x1F6FF |  // Transport and Map
        0x1F1E0..=0x1F1FF |  // Flags
        0x2600..=0x26FF   |  // Misc symbols
        0x2700..=0x27BF   |  // Dingbats
        0xFE00..=0xFE0F   |  // Variation Selectors
        0x1F900..=0x1F9FF |  // Supplemental Symbols
        0x1FA00..=0x1FA6F |  // Chess symbols
        0x1FA70..=0x1FAFF |  // Symbols and Pictographs Extended-A
        0x231A..=0x231B   |  // Watch, Hourglass
        0x23E9..=0x23F3   |  // Media control
        0x23F8..=0x23FA   |  // Misc
        0x25AA..=0x25AB   |  // Squares
        0x25B6            |  // Play button
        0x25C0            |  // Reverse button
        0x25FB..=0x25FE   |  // Squares
        0x2614..=0x2615   |  // Umbrella, Hot Beverage
        0x2648..=0x2653   |  // Zodiac
        0x267F            |  // Wheelchair
        0x2693            |  // Anchor
        0x26A1            |  // High Voltage
        0x26AA..=0x26AB   |  // Circles
        0x26BD..=0x26BE   |  // Sports
        0x26C4..=0x26C5   |  // Weather
        0x26CE            |  // Ophiuchus
        0x26D4            |  // No Entry
        0x26EA            |  // Church
        0x26F2..=0x26F3   |  // Fountain, Golf
        0x26F5            |  // Sailboat
        0x26FA            |  // Tent
        0x26FD            |  // Fuel Pump
        0x2702            |  // Scissors
        0x2705            |  // Check Mark
        0x2708..=0x270D   |  // Misc
        0x270F            |  // Pencil
        0x2712            |  // Black Nib
        0x2714            |  // Check Mark
        0x2716            |  // Cross Mark
        0x271D            |  // Latin Cross
        0x2721            |  // Star of David
        0x2728            |  // Sparkles
        0x2733..=0x2734   |  // Eight Spoked Asterisk
        0x2744            |  // Snowflake
        0x2747            |  // Sparkle
        0x274C            |  // Cross Mark
        0x274E            |  // Cross Mark Outline
        0x2753..=0x2755   |  // Question Mark
        0x2757            |  // Exclamation Mark
        0x2763..=0x2764   |  // Heart
        0x2795..=0x2797   |  // Math operators
        0x27A1            |  // Right Arrow
        0x27B0            |  // Curly Loop
        0x27BF            |  // Double Curly Loop
        0x2934..=0x2935   |  // Arrows
        0x2B05..=0x2B07   |  // Arrows
        0x2B1B..=0x2B1C   |  // Squares
        0x2B50            |  // Star
        0x2B55            |  // Circle
        0x3030            |  // Wavy Dash
        0x303D            |  // Part Alternation Mark
        0x3297            |  // Circled Ideograph Congratulation
        0x3299               // Circled Ideograph Secret
    )
}

/// Check if character is emoji variation selector
pub fn is_emoji_variation_selector(c: char) -> bool {
    c == '\u{FE0F}' || c == '\u{FE0E}'
}

/// Check if character is skin tone modifier
pub fn is_skin_tone_modifier(c: char) -> bool {
    matches!(c, '\u{1F3FB}'..='\u{1F3FF}')
}

/// Check if character is ZWJ (Zero Width Joiner)
pub fn is_zwj(c: char) -> bool {
    c == '\u{200D}'
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_is_emoji() {
        assert!(is_emoji('😀'));
        assert!(is_emoji('🚀'));
        assert!(!is_emoji('A'));
        assert!(!is_emoji('中'));
    }
    
    #[test]
    fn test_skin_tone() {
        assert!(is_skin_tone_modifier('\u{1F3FB}'));  // Light skin tone
        assert!(!is_skin_tone_modifier('A'));
    }
    
    #[test]
    fn test_color_palette() {
        let mut palette = ColorPalette::new();
        palette.add_color(PaletteColor::new(255, 0, 0, 255));
        palette.add_color(PaletteColor::new(0, 255, 0, 255));
        
        assert_eq!(palette.colors.len(), 2);
        assert_eq!(palette.get(0).unwrap().r, 255);
    }
    
    #[test]
    fn test_emoji_renderer() {
        let mut renderer = EmojiRenderer::new();
        renderer.cache_glyph(100, ColorGlyph::Layered(vec![
            ColorLayer { glyph_id: 1, palette_index: 0 },
            ColorLayer { glyph_id: 2, palette_index: 1 },
        ]));
        
        assert!(renderer.has_color_glyph(100));
        assert!(!renderer.has_color_glyph(200));
    }
}
