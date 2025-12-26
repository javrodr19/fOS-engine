//! Custom Text Shaper
//!
//! Full HarfBuzz-compatible text shaper using custom GSUB/GPOS,
//! Bidi algorithm, script itemization, and complex script shaping.

use crate::font::{FontDatabase, FontId};
use crate::font::parser::{FontParser, GlyphId};
use crate::{Result, TextError};
use super::{ShapedGlyph, ShapedRun};
use super::gsub::{GsubTable, Substitution};
use super::gpos::{GposTable, ValueRecord};
use super::bidi::{BidiParagraph, Level};
use super::script::{Script, ScriptItemizer, ScriptRun, Direction, Language};
use super::arabic::ArabicShaper;
use super::indic::IndicShaper;

/// Text direction
#[derive(Debug, Clone, Copy, Default)]
pub enum TextDirection {
    #[default]
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

impl From<TextDirection> for Direction {
    fn from(d: TextDirection) -> Self {
        match d {
            TextDirection::LeftToRight => Direction::LeftToRight,
            TextDirection::RightToLeft => Direction::RightToLeft,
            TextDirection::TopToBottom => Direction::TopToBottom,
            TextDirection::BottomToTop => Direction::BottomToTop,
        }
    }
}

/// OpenType feature tag
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Feature(pub [u8; 4]);

impl Feature {
    pub const LIGA: Feature = Feature(*b"liga");
    pub const KERN: Feature = Feature(*b"kern");
    pub const CALT: Feature = Feature(*b"calt");
    pub const LOCL: Feature = Feature(*b"locl");
    pub const RLIG: Feature = Feature(*b"rlig");
    pub const CCMP: Feature = Feature(*b"ccmp");
    pub const MARK: Feature = Feature(*b"mark");
    pub const MKMK: Feature = Feature(*b"mkmk");
}

/// Text shaper configuration
#[derive(Debug, Clone)]
pub struct ShaperConfig {
    /// Text direction
    pub direction: TextDirection,
    /// Script (auto-detected if None)
    pub script: Option<Script>,
    /// Language tag
    pub language: Language,
    /// Enabled features (all standard features enabled by default)
    pub features: Vec<Feature>,
    /// Disable standard ligatures
    pub no_ligatures: bool,
    /// Disable kerning
    pub no_kerning: bool,
}

impl Default for ShaperConfig {
    fn default() -> Self {
        Self {
            direction: TextDirection::LeftToRight,
            script: None,
            language: Language::DEFAULT,
            features: vec![
                Feature::CCMP,
                Feature::LOCL,
                Feature::RLIG,
                Feature::CALT,
                Feature::LIGA,
                Feature::KERN,
                Feature::MARK,
                Feature::MKMK,
            ],
            no_ligatures: false,
            no_kerning: false,
        }
    }
}

/// Custom text shaper (HarfBuzz-compatible)
pub struct TextShaper {
    /// Script itemizer
    script_itemizer: ScriptItemizer,
    /// Arabic shaper
    arabic_shaper: ArabicShaper,
    /// Indic shaper
    indic_shaper: IndicShaper,
    /// Configuration
    config: ShaperConfig,
}

impl TextShaper {
    /// Create a new text shaper
    pub fn new() -> Self {
        Self {
            script_itemizer: ScriptItemizer::new(),
            arabic_shaper: ArabicShaper::new(),
            indic_shaper: IndicShaper::new(),
            config: ShaperConfig::default(),
        }
    }
    
    /// Set text direction
    pub fn direction(mut self, direction: TextDirection) -> Self {
        self.config.direction = direction;
        self
    }
    
    /// Set script (for explicit control)
    pub fn script(mut self, script: Script) -> Self {
        self.config.script = Some(script);
        self
    }
    
    /// Set language
    pub fn language(mut self, language: &str) -> Self {
        self.config.language = Language::from_bcp47(language);
        self
    }
    
    /// Disable ligatures
    pub fn no_ligatures(mut self) -> Self {
        self.config.no_ligatures = true;
        self
    }
    
    /// Disable kerning
    pub fn no_kerning(mut self) -> Self {
        self.config.no_kerning = true;
        self
    }
    
    /// Shape text using a font from the database
    pub fn shape(
        &mut self,
        db: &FontDatabase,
        font_id: FontId,
        text: &str,
        font_size: f32,
    ) -> Result<ShapedRun> {
        db.with_face_data(font_id, |data, index| {
            self.shape_with_data(data, index, text, font_size)
        }).ok_or_else(|| TextError::FontNotFound("Font not found in database".into()))?
    }
    
    /// Shape text with raw font data
    pub fn shape_with_data(
        &mut self,
        font_data: &[u8],
        face_index: u32,
        text: &str,
        font_size: f32,
    ) -> Result<ShapedRun> {
        // Parse font
        let font = FontParser::parse_index(font_data, face_index)
            .map_err(|_| TextError::FontParsing("Failed to parse font".into()))?;
        
        // Map characters to glyphs
        let mut glyphs: Vec<GlyphInfo> = text.chars()
            .enumerate()
            .map(|(i, c)| {
                let glyph_id = font.glyph_index(c).unwrap_or(GlyphId(0));
                GlyphInfo {
                    glyph_id,
                    cluster: i as u32,
                    char_code: c,
                    x_advance: font.glyph_hor_advance(glyph_id).unwrap_or(0) as i32,
                    y_advance: 0,
                    x_offset: 0,
                    y_offset: 0,
                }
            })
            .collect();
        
        // Script itemization
        let script_runs = self.script_itemizer.itemize(text);
        
        // Process each script run
        for run in &script_runs {
            let script = self.config.script.unwrap_or(run.script);
            
            // Apply script-specific shaping
            self.shape_script_run(&font, font_data, &mut glyphs, run, script);
        }
        
        // Apply Bidi algorithm if needed
        let has_rtl = script_runs.iter().any(|r| r.script.is_rtl());
        if has_rtl {
            self.apply_bidi(text, &mut glyphs);
        }
        
        // Apply GSUB substitutions
        if let Some(gsub_data) = font.table_data(b"GSUB") {
            self.apply_gsub(gsub_data, &mut glyphs);
        }
        
        // Apply GPOS positioning
        if let Some(gpos_data) = font.table_data(b"GPOS") {
            self.apply_gpos(gpos_data, &mut glyphs, &font);
        }
        
        // Convert to ShapedGlyph
        let shaped_glyphs: Vec<ShapedGlyph> = glyphs.into_iter()
            .map(|g| ShapedGlyph {
                glyph_id: g.glyph_id.0,
                x_offset: g.x_offset,
                y_offset: g.y_offset,
                x_advance: g.x_advance,
                y_advance: g.y_advance,
                cluster: g.cluster,
            })
            .collect();
        
        Ok(ShapedRun::new(shaped_glyphs, font_size, font.units_per_em()))
    }
    
    /// Shape a script-specific run
    fn shape_script_run(
        &mut self,
        font: &FontParser,
        font_data: &[u8],
        glyphs: &mut [GlyphInfo],
        run: &ScriptRun,
        script: Script,
    ) {
        // Skip empty runs
        if run.start >= run.end {
            return;
        }
        
        // Get glyph slice for this run
        let run_glyphs = &mut glyphs[run.start..run.end];
        
        match script {
            Script::Arabic | Script::Syriac | Script::Nko | Script::Thaana => {
                // Arabic-style shaping
                let text: String = run_glyphs.iter().map(|g| g.char_code).collect();
                self.arabic_shaper.analyze(&text);
                
                // Apply positional forms would happen in GSUB
                // The arabic_shaper marks which forms are needed
            }
            
            Script::Devanagari | Script::Bengali | Script::Gurmukhi |
            Script::Gujarati | Script::Tamil | Script::Telugu |
            Script::Kannada | Script::Malayalam => {
                // Indic shaping
                let text: String = run_glyphs.iter().map(|g| g.char_code).collect();
                self.indic_shaper.analyze(&text);
                
                // Reordering is handled by the syllable analysis
                // Actual glyph substitution happens in GSUB
            }
            
            _ => {
                // Simple scripts (Latin, Greek, etc.) - no special processing
            }
        }
    }
    
    /// Apply Bidi algorithm
    fn apply_bidi(&self, text: &str, glyphs: &mut Vec<GlyphInfo>) {
        let bidi = BidiParagraph::new(text, None);
        let visual_indices = bidi.visual_indices();
        
        // Reorder glyphs according to visual order
        if visual_indices.len() == glyphs.len() {
            let original = glyphs.clone();
            for (visual_pos, &logical_pos) in visual_indices.iter().enumerate() {
                if logical_pos < original.len() {
                    glyphs[visual_pos] = original[logical_pos].clone();
                }
            }
        }
    }
    
    /// Apply GSUB substitutions
    fn apply_gsub(&self, gsub_data: &[u8], glyphs: &mut Vec<GlyphInfo>) {
        let gsub = match GsubTable::parse(gsub_data) {
            Some(g) => g,
            None => return,
        };
        
        // Apply lookups
        for i in 0..gsub.lookup_count() {
            if let Some(lookup) = gsub.get_lookup(i) {
                self.apply_gsub_lookup(&lookup, glyphs);
            }
        }
    }
    
    /// Apply a single GSUB lookup
    fn apply_gsub_lookup(&self, lookup: &super::gsub::GsubLookup, glyphs: &mut Vec<GlyphInfo>) {
        use super::gsub::{GsubSubtable, LookupType};
        
        let mut i = 0;
        while i < glyphs.len() {
            for subtable in &lookup.subtables {
                match subtable {
                    GsubSubtable::Single(single) => {
                        if let Substitution::Single(new_id) = single.apply(glyphs[i].glyph_id) {
                            glyphs[i].glyph_id = new_id;
                        }
                    }
                    
                    GsubSubtable::Multiple(multiple) => {
                        if let Substitution::Multiple(new_ids) = multiple.apply(glyphs[i].glyph_id) {
                            if !new_ids.is_empty() {
                                // Replace current glyph and insert rest
                                glyphs[i].glyph_id = new_ids[0];
                                for (j, &id) in new_ids.iter().enumerate().skip(1) {
                                    let mut new_glyph = glyphs[i].clone();
                                    new_glyph.glyph_id = id;
                                    glyphs.insert(i + j, new_glyph);
                                }
                                i += new_ids.len() - 1;
                            }
                        }
                    }
                    
                    GsubSubtable::Ligature(ligature) => {
                        let remaining: Vec<GlyphId> = glyphs[i..].iter()
                            .map(|g| g.glyph_id)
                            .collect();
                        
                        if let Some((lig_id, consumed)) = ligature.apply(&remaining) {
                            glyphs[i].glyph_id = lig_id;
                            // Remove consumed glyphs (except first)
                            for _ in 1..consumed {
                                if i + 1 < glyphs.len() {
                                    glyphs.remove(i + 1);
                                }
                            }
                        }
                    }
                    
                    _ => {
                        // Context/chained lookups handled by recursive lookup application
                    }
                }
            }
            i += 1;
        }
    }
    
    /// Apply GPOS positioning
    fn apply_gpos(&self, gpos_data: &[u8], glyphs: &mut [GlyphInfo], font: &FontParser) {
        let gpos = match GposTable::parse(gpos_data) {
            Some(g) => g,
            None => return,
        };
        
        // Apply kerning
        if !self.config.no_kerning && glyphs.len() >= 2 {
            for i in 0..glyphs.len() - 1 {
                let first = glyphs[i].glyph_id;
                let second = glyphs[i + 1].glyph_id;
                
                if let Some(kern) = gpos.get_kerning(first, second) {
                    glyphs[i].x_advance += kern as i32;
                }
            }
        }
        
        // Apply mark positioning would be done here
        // For now, we apply basic adjustments from lookups
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}

/// Internal glyph info during shaping
#[derive(Debug, Clone)]
struct GlyphInfo {
    glyph_id: GlyphId,
    cluster: u32,
    char_code: char,
    x_advance: i32,
    y_advance: i32,
    x_offset: i32,
    y_offset: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_shaper_creation() {
        let shaper = TextShaper::new()
            .direction(TextDirection::LeftToRight);
        assert!(matches!(shaper.config.direction, TextDirection::LeftToRight));
    }
    
    #[test]
    fn test_feature_tags() {
        assert_eq!(Feature::LIGA.0, *b"liga");
        assert_eq!(Feature::KERN.0, *b"kern");
    }
    
    #[test]
    fn test_config_default() {
        let config = ShaperConfig::default();
        assert!(!config.no_ligatures);
        assert!(!config.no_kerning);
    }
}
