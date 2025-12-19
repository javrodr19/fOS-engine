//! Font face with parsed metrics

use ttf_parser::{Face, GlyphId};
use super::FontId;

/// Parsed font face with metrics
pub struct FontFace<'a> {
    /// The underlying ttf-parser face
    face: Face<'a>,
    /// Font ID in database
    pub id: FontId,
}

impl<'a> FontFace<'a> {
    /// Parse a font face from data
    pub fn parse(data: &'a [u8], index: u32, id: FontId) -> Option<Self> {
        Face::parse(data, index).ok().map(|face| Self { face, id })
    }
    
    /// Units per em
    pub fn units_per_em(&self) -> u16 {
        self.face.units_per_em()
    }
    
    /// Ascender (above baseline)
    pub fn ascender(&self) -> i16 {
        self.face.ascender()
    }
    
    /// Descender (below baseline, usually negative)
    pub fn descender(&self) -> i16 {
        self.face.descender()
    }
    
    /// Line gap
    pub fn line_gap(&self) -> i16 {
        self.face.line_gap()
    }
    
    /// Line height (ascender - descender + line_gap)
    pub fn line_height(&self) -> i16 {
        self.ascender() - self.descender() + self.line_gap()
    }
    
    /// Get glyph ID for a character
    pub fn glyph_index(&self, c: char) -> Option<GlyphId> {
        self.face.glyph_index(c)
    }
    
    /// Get glyph horizontal advance
    pub fn glyph_hor_advance(&self, glyph_id: GlyphId) -> Option<u16> {
        self.face.glyph_hor_advance(glyph_id)
    }
    
    /// Number of glyphs in font
    pub fn number_of_glyphs(&self) -> u16 {
        self.face.number_of_glyphs()
    }
    
    /// Check if font has glyph for character
    pub fn has_char(&self, c: char) -> bool {
        self.glyph_index(c).is_some()
    }
    
    /// Get underlying ttf-parser face
    pub fn ttf_face(&self) -> &Face<'a> {
        &self.face
    }
}
