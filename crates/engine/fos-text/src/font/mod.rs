//! Font loading and matching module
//!
//! Custom font parser implementation - replaces ttf-parser and fontdb.

// Custom parser (replaces ttf-parser)
pub mod parser;
// WOFF decoder
pub mod woff;
// String interning for font names
mod intern;
// Arena allocator for efficient parsing
pub mod arena;
// Fixed-point arithmetic for variable font axes
pub mod fixed_point;
// Custom database (replaces fontdb)
mod custom_database;

// Original modules
mod face;
mod matching;
pub mod variable;
pub mod emoji;
pub mod optimization;

// Re-export from custom implementations
pub use custom_database::{CustomFontDatabase as FontDatabase, FontId, FontEntry, FontSource};
pub use face::FontFace;
pub use matching::{FontQuery, resolve_generic_family};
pub use variable::{FontAxis, VariableFont, VariableFontInstance, NamedInstance, axis_tags};
pub use emoji::{EmojiRenderer, ColorGlyph, ColorFontFormat, is_emoji};
pub use optimization::{FontSubsetter, GlyphStreamer, SharedFontCache, MmapFont, GlyphMetricsCache};
pub use parser::{GlyphId, OutlineBuilder, BoundingBox, FontParser};

/// Font weight (100-900)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FontWeight(pub u16);

impl FontWeight {
    pub const THIN: FontWeight = FontWeight(100);
    pub const EXTRA_LIGHT: FontWeight = FontWeight(200);
    pub const LIGHT: FontWeight = FontWeight(300);
    pub const NORMAL: FontWeight = FontWeight(400);
    pub const MEDIUM: FontWeight = FontWeight(500);
    pub const SEMI_BOLD: FontWeight = FontWeight(600);
    pub const BOLD: FontWeight = FontWeight(700);
    pub const EXTRA_BOLD: FontWeight = FontWeight(800);
    pub const BLACK: FontWeight = FontWeight(900);
}

impl From<u16> for FontWeight {
    fn from(value: u16) -> Self {
        FontWeight(value.clamp(100, 900))
    }
}

/// Font style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}
