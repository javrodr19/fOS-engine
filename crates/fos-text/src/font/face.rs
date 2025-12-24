//! Font face with parsed metrics
//!
//! Uses custom parser instead of ttf-parser.

use super::parser::{FontParser, GlyphId};
use super::FontId;

/// Parsed font face with metrics
pub struct FontFace<'a> {
    /// The underlying custom parser
    parser: FontParser<'a>,
    /// Font ID in database
    pub id: FontId,
}

impl<'a> FontFace<'a> {
    /// Parse a font face from data
    pub fn parse(data: &'a [u8], index: u32, id: FontId) -> Option<Self> {
        FontParser::parse_index(data, index)
            .ok()
            .map(|parser| Self { parser, id })
    }
    
    /// Units per em
    pub fn units_per_em(&self) -> u16 {
        self.parser.units_per_em()
    }
    
    /// Ascender (above baseline)
    pub fn ascender(&self) -> i16 {
        self.parser.ascender()
    }
    
    /// Descender (below baseline, usually negative)
    pub fn descender(&self) -> i16 {
        self.parser.descender()
    }
    
    /// Line gap
    pub fn line_gap(&self) -> i16 {
        self.parser.line_gap()
    }
    
    /// Line height (ascender - descender + line_gap)
    pub fn line_height(&self) -> i16 {
        self.ascender() - self.descender() + self.line_gap()
    }
    
    /// Get glyph ID for a character
    pub fn glyph_index(&self, c: char) -> Option<GlyphId> {
        self.parser.glyph_index(c)
    }
    
    /// Get glyph horizontal advance
    pub fn glyph_hor_advance(&self, glyph_id: GlyphId) -> Option<u16> {
        self.parser.glyph_hor_advance(glyph_id)
    }
    
    /// Number of glyphs in font
    pub fn number_of_glyphs(&self) -> u16 {
        self.parser.number_of_glyphs()
    }
    
    /// Check if font has glyph for character
    pub fn has_char(&self, c: char) -> bool {
        self.glyph_index(c).is_some()
    }
    
    /// Get underlying parser (for advanced usage)
    pub fn parser(&self) -> &FontParser<'a> {
        &self.parser
    }
    
    /// Get glyph bounding box
    pub fn glyph_bounding_box(&self, glyph_id: GlyphId) -> Option<super::parser::BoundingBox> {
        self.parser.glyph_bounding_box(glyph_id)
    }
    
    /// Outline a glyph
    pub fn outline_glyph<B: super::parser::OutlineBuilder>(&self, glyph_id: GlyphId, builder: &mut B) -> Option<()> {
        self.parser.outline_glyph(glyph_id, builder)
    }
}
