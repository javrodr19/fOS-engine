//! Custom Text Shaper
//!
//! Full HarfBuzz-compatible text shaper using custom GSUB/GPOS,
//! Bidi algorithm, script itemization, and complex script shaping.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHasher};

use crate::font::{FontDatabase, FontId};
use crate::font::parser::{FontParser, GlyphId};
use crate::{Result, TextError};
use super::{ShapedGlyph, ShapedRun};
use super::gsub::{GsubLookup, GsubTable, Substitution};
use super::gpos::{GposSubtable, GposTable, PairPos};
use super::bidi::BidiParagraph;
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

/// GSUB features applied by default. Contextual (`calt`), localized
/// (`locl`) and composition (`ccmp`) features are left out: their lookup
/// types, or the mark positioning they depend on, are not implemented yet,
/// and applying them partially produces wrong glyphs.
const DEFAULT_GSUB_FEATURES: &[[u8; 4]] = &[*b"liga", *b"clig", *b"rlig"];

/// GPOS features used for kerning
const KERNING_FEATURES: &[[u8; 4]] = &[*b"kern"];

/// Shaping plans kept per shaper (one per font face in use)
const MAX_CACHED_PLANS: usize = 16;

/// Identifies a font face (and the options that affect its plan)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PlanKey {
    data_ptr: usize,
    data_len: usize,
    face_index: u32,
    no_ligatures: bool,
    no_kerning: bool,
}

/// Everything shaping needs from a font face, parsed once and reused for
/// every run: the lookups of the enabled features and ASCII glyph metrics.
///
/// Parsing GSUB/GPOS lookups is far more expensive than applying them;
/// previously every lookup was re-parsed for every glyph pair.
struct ShapingPlan {
    /// Guards against a different font later occupying the same buffer
    fingerprint: u64,
    /// GSUB lookups for the default features, in LookupList order
    gsub: Vec<GsubLookup>,
    /// Pair adjustment subtables of the `kern` feature
    kerning: Vec<PairPos>,
    /// Glyph ID and advance for each ASCII character
    ascii: [(GlyphId, i32); 128],
}

impl ShapingPlan {
    fn build(font: &FontParser, font_data: &[u8], no_ligatures: bool, no_kerning: bool) -> Self {
        let mut ascii = [(GlyphId(0), 0); 128];
        for (c, slot) in ascii.iter_mut().enumerate() {
            let glyph = font.glyph_index(c as u8 as char).unwrap_or(GlyphId(0));
            *slot = (glyph, font.glyph_hor_advance(glyph).unwrap_or(0) as i32);
        }
        
        let gsub = if no_ligatures {
            Vec::new()
        } else {
            font.table_data(b"GSUB")
                .and_then(GsubTable::parse)
                .map(|gsub| {
                    gsub.feature_lookup_indices(DEFAULT_GSUB_FEATURES)
                        .into_iter()
                        .filter_map(|index| gsub.get_lookup(index))
                        .collect()
                })
                .unwrap_or_default()
        };
        
        let mut kerning = Vec::new();
        if !no_kerning {
            if let Some(gpos) = font.table_data(b"GPOS").and_then(GposTable::parse) {
                for index in gpos.feature_lookup_indices(KERNING_FEATURES) {
                    if let Some(lookup) = gpos.get_lookup(index) {
                        for subtable in lookup.subtables {
                            if let GposSubtable::PairAdjustment(pair) = subtable {
                                kerning.push(pair);
                            }
                        }
                    }
                }
            }
        }
        
        Self {
            fingerprint: font_fingerprint(font_data),
            gsub,
            kerning,
            ascii,
        }
    }
}

/// Cheap fingerprint of a font buffer (length plus leading bytes)
fn font_fingerprint(data: &[u8]) -> u64 {
    // Computed on every shaping call, so a fast non-cryptographic hash
    let mut hasher = FxHasher::default();
    data.len().hash(&mut hasher);
    data[..data.len().min(256)].hash(&mut hasher);
    hasher.finish()
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
    /// Per-face shaping plans (shared, so shaping can hold one while
    /// using `self` mutably)
    plans: FxHashMap<PlanKey, Arc<ShapingPlan>>,
}

impl TextShaper {
    /// Create a new text shaper
    pub fn new() -> Self {
        Self {
            script_itemizer: ScriptItemizer::new(),
            arabic_shaper: ArabicShaper::new(),
            indic_shaper: IndicShaper::new(),
            config: ShaperConfig::default(),
            plans: FxHashMap::default(),
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
        
        // Get (or build) the plan for this face
        let key = PlanKey {
            data_ptr: font_data.as_ptr() as usize,
            data_len: font_data.len(),
            face_index,
            no_ligatures: self.config.no_ligatures,
            no_kerning: self.config.no_kerning,
        };
        let fingerprint = font_fingerprint(font_data);
        if self.plans.get(&key).is_none_or(|plan| plan.fingerprint != fingerprint) {
            if self.plans.len() >= MAX_CACHED_PLANS {
                self.plans.clear();
            }
            let plan = ShapingPlan::build(&font, font_data, key.no_ligatures, key.no_kerning);
            self.plans.insert(key, Arc::new(plan));
        }
        let plan = Arc::clone(&self.plans[&key]);
        
        // Map characters to glyphs
        let mut glyphs: Vec<GlyphInfo> = text.chars()
            .enumerate()
            .map(|(i, c)| {
                let (glyph_id, x_advance) = match plan.ascii.get(c as usize) {
                    Some(&metrics) => metrics,
                    None => {
                        let glyph_id = font.glyph_index(c).unwrap_or(GlyphId(0));
                        (glyph_id, font.glyph_hor_advance(glyph_id).unwrap_or(0) as i32)
                    }
                };
                GlyphInfo {
                    glyph_id,
                    cluster: i as u32,
                    char_code: c,
                    x_advance,
                    y_advance: 0,
                    x_offset: 0,
                    y_offset: 0,
                }
            })
            .collect();
        
        // Script itemization
        let script_runs = self.script_itemizer.itemize(text);
        
        // Script runs are byte ranges, while glyphs are indexed by character
        // (the same thing for ASCII, the common case)
        let char_starts: Vec<usize> = if text.is_ascii() {
            Vec::new()
        } else {
            text.char_indices().map(|(i, _)| i).collect()
        };
        let to_char_index = |byte: usize| {
            if char_starts.is_empty() { byte } else { char_starts.partition_point(|&start| start < byte) }
        };
        
        // Process each script run
        for run in &script_runs {
            let script = self.config.script.unwrap_or(run.script);
            let char_run = ScriptRun {
                start: to_char_index(run.start),
                end: to_char_index(run.end),
                script: run.script,
            };
            
            // Apply script-specific shaping
            self.shape_script_run(&font, font_data, &mut glyphs, &char_run, script);
        }
        
        // Apply Bidi algorithm if needed
        let has_rtl = script_runs.iter().any(|r| r.script.is_rtl());
        if has_rtl {
            self.apply_bidi(text, &mut glyphs);
        }
        
        // Apply GSUB substitutions (enabled features only)
        if !plan.gsub.is_empty() {
            let before: Vec<GlyphId> = glyphs.iter().map(|g| g.glyph_id).collect();
            for lookup in &plan.gsub {
                Self::apply_gsub_lookup(lookup, &mut glyphs);
            }
            // Substituted glyphs (ligatures, alternates) advance by their
            // own width, not the replaced character's
            let changed = glyphs.len() != before.len() || glyphs.iter().zip(&before).any(|(g, b)| g.glyph_id != *b);
            if changed {
                for g in &mut glyphs {
                    g.x_advance = font.glyph_hor_advance(g.glyph_id).unwrap_or(0) as i32;
                }
            }
        }
        
        // Apply GPOS kerning
        Self::apply_kerning(&plan.kerning, &mut glyphs);
        
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
        // Skip empty runs (and never index past the glyph buffer)
        let end = run.end.min(glyphs.len());
        if run.start >= end {
            return;
        }
        
        // Get glyph slice for this run
        let run_glyphs = &mut glyphs[run.start..end];
        
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
    
    /// Apply a single GSUB lookup
    fn apply_gsub_lookup(lookup: &GsubLookup, glyphs: &mut Vec<GlyphInfo>) {
        use super::gsub::GsubSubtable;
        
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
                        let matched = ligature.apply_with(glyphs.len() - i, |k| glyphs[i + k].glyph_id);
                        if let Some((lig_id, consumed)) = matched {
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
    
    /// Apply pair kerning: the first subtable that covers a pair wins
    fn apply_kerning(kerning: &[PairPos], glyphs: &mut [GlyphInfo]) {
        if kerning.is_empty() {
            return;
        }
        for i in 1..glyphs.len() {
            let (first, second) = (glyphs[i - 1].glyph_id, glyphs[i].glyph_id);
            let adjustment = kerning.iter().find_map(|pair| pair.apply(first, second));
            if let Some((value, _)) = adjustment {
                glyphs[i - 1].x_advance += value.x_advance as i32;
            }
        }
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
    
    #[test]
    fn test_shape_multibyte_text() {
        // Regression: script runs are byte ranges but glyphs are per
        // character, so multi-byte text used to index out of bounds
        let db = FontDatabase::shared();
        let Some(font) = db.query(&crate::FontQuery::new(&["sans-serif"])) else {
            return; // No fonts installed
        };
        
        let mut shaper = TextShaper::new();
        for text in ["• item", "café — “quoted”", "Привет, мир", "日本語 text", "a"] {
            let run = shaper.shape(&db, font, text, 16.0).unwrap();
            assert_eq!(run.glyphs.len(), text.chars().count(), "{text}");
        }
    }

    #[test]
    fn test_ligatures_are_formed() {
        let db = FontDatabase::shared();
        let Some(font) = db.query(&crate::FontQuery::new(&["DejaVu Sans"])) else {
            return; // Font not installed
        };
        if !db.font(font).is_some_and(|f| f.family.contains("DejaVu Sans")) {
            return;
        }

        // DejaVu Sans has "ffi" and "fl" ligatures under `liga`
        let text = "official flight";
        let with = TextShaper::new().shape(&db, font, text, 16.0).unwrap();
        let without = TextShaper::new().no_ligatures().shape(&db, font, text, 16.0).unwrap();
        assert_eq!(without.glyphs.len(), text.chars().count());
        assert_eq!(with.glyphs.len(), without.glyphs.len() - 3);
        // A ligature is about as wide as the letters it replaces
        let width = |r: &ShapedRun| r.glyphs.iter().map(|g| g.x_advance).sum::<i32>() as f32;
        let (w, wo) = (width(&with), width(&without));
        assert!((w - wo).abs() < wo * 0.05, "{w} vs {wo}");
    }
}
