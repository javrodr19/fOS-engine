//! Fonts for layout: matching a style's font to a face, the face's
//! metrics, and shaped words (cached, since words repeat across a page and
//! across relayouts)

use std::collections::HashMap;
use std::sync::Arc;

use fos_css::style::{FontStyle, Style};
use fos_text::{FontDatabase, FontId, FontParser, FontQuery, TextShaper};

/// Vertical metrics of a face, per em
#[derive(Clone, Copy, Debug)]
pub struct FontMetrics {
    pub ascent: f32,
    /// Below the baseline (positive)
    pub descent: f32,
    pub line_gap: f32,
}

impl FontMetrics {
    /// When no font is available
    pub const FALLBACK: FontMetrics = FontMetrics { ascent: 0.8, descent: 0.2, line_gap: 0.0 };

    /// The content-area height of `normal` line height, per em
    pub fn normal_line_height(&self) -> f32 {
        self.ascent + self.descent + self.line_gap
    }
}

/// A positioned glyph of a shaped word
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub id: u16,
    /// From the word's start, in px
    pub x: f32,
    /// Up from the baseline, in px
    pub y: f32,
    pub advance: f32,
}

/// A word shaped in one font at one size
#[derive(Debug)]
pub struct ShapedWord {
    pub font: Option<FontId>,
    pub size: f32,
    pub width: f32,
    pub glyphs: Box<[Glyph]>,
}

/// A style's font, resolved
#[derive(Clone, Copy, Debug)]
pub struct ResolvedFont {
    pub id: Option<FontId>,
    pub size: f32,
    pub metrics: FontMetrics,
    /// Styles the face lacks, to synthesize when drawing
    /// (`fos_text::SYNTH_OBLIQUE`, `SYNTH_BOLD`)
    pub synthesis: u8,
}

impl ResolvedFont {
    pub fn ascent(&self) -> f32 {
        self.metrics.ascent * self.size
    }

    pub fn descent(&self) -> f32 {
        self.metrics.descent * self.size
    }
}

/// Shaped words kept before the cache starts over
const MAX_WORDS: usize = 50_000;

pub struct FontContext {
    db: Arc<FontDatabase>,
    shaper: TextShaper,
    /// (families, weight, italic) -> face
    selection: HashMap<(Arc<[Arc<str>]>, u16, bool), (Option<FontId>, u8)>,
    metrics: HashMap<FontId, FontMetrics>,
    /// Shaped words by (face, size bits), then text
    words: HashMap<(Option<FontId>, u32), HashMap<Box<str>, Arc<ShapedWord>>>,
    word_count: usize,
}

impl Default for FontContext {
    fn default() -> Self {
        Self::new(FontDatabase::shared())
    }
}

impl FontContext {
    pub fn new(db: Arc<FontDatabase>) -> Self {
        FontContext { db, shaper: TextShaper::new(), selection: HashMap::new(), metrics: HashMap::new(), words: HashMap::new(), word_count: 0 }
    }

    pub fn database(&self) -> &Arc<FontDatabase> {
        &self.db
    }

    /// The face and size for a style's text
    pub fn resolve(&mut self, style: &Style) -> ResolvedFont {
        let i = &style.inherited;
        let key = (i.font_family.clone(), i.font_weight, i.font_style != FontStyle::Normal);
        let (id, synthesis) = match self.selection.get(&key) {
            Some(sel) => *sel,
            None => {
                let families: Vec<&str> = i.font_family.iter().map(|f| match &**f {
                    // Generic names the font database knows by other names
                    "system-ui" | "-apple-system" | "blinkmacsystemfont" | "ui-sans-serif" => "sans-serif",
                    "ui-serif" => "serif",
                    "ui-monospace" => "monospace",
                    other => other,
                }).collect();
                let style = if key.2 { fos_text::FontStyle::Italic } else { fos_text::FontStyle::Normal };
                let query = FontQuery::new(&families).weight(fos_text::FontWeight(i.font_weight)).style(style);
                let id = self.db.query(&query);
                // Browsers slant and thicken faces that lack the style
                let mut synthesis = 0;
                if let Some(face) = id.and_then(|id| self.db.font(id)) {
                    if key.2 && face.style == fos_text::FontStyle::Normal {
                        synthesis |= fos_text::SYNTH_OBLIQUE;
                    }
                    if key.1 >= 600 && face.weight.0 < 600 {
                        synthesis |= fos_text::SYNTH_BOLD;
                    }
                }
                self.selection.insert(key, (id, synthesis));
                (id, synthesis)
            }
        };
        let metrics = id.map_or(FontMetrics::FALLBACK, |id| self.metrics_of(id));
        ResolvedFont { id, size: i.font_size, metrics, synthesis }
    }

    fn metrics_of(&mut self, id: FontId) -> FontMetrics {
        if let Some(m) = self.metrics.get(&id) {
            return *m;
        }
        let m = self
            .db
            .with_face_data(id, |data, index| {
                let p = FontParser::parse_index(data, index).ok()?;
                let upem = p.units_per_em().max(1) as f32;
                let (a, d, g) = (p.ascender() as f32 / upem, -(p.descender() as f32) / upem, p.line_gap() as f32 / upem);
                // Faces with broken metrics get sane ones
                (a > 0.0 && d >= 0.0 && a + d < 4.0).then_some(FontMetrics { ascent: a, descent: d, line_gap: g.max(0.0) })
            })
            .flatten()
            .unwrap_or(FontMetrics::FALLBACK);
        self.metrics.insert(id, m);
        m
    }

    /// Shape `text` (one word or space run) in a resolved font
    pub fn shape(&mut self, font: &ResolvedFont, text: &str) -> Arc<ShapedWord> {
        let key = (font.id, font.size.to_bits());
        if let Some(w) = self.words.get(&key).and_then(|m| m.get(text)) {
            return w.clone();
        }
        let word = Arc::new(self.shape_uncached(font, text));
        if self.word_count >= MAX_WORDS {
            self.words.clear();
            self.word_count = 0;
        }
        self.word_count += 1;
        self.words.entry(key).or_default().insert(text.into(), word.clone());
        word
    }

    fn shape_uncached(&mut self, font: &ResolvedFont, text: &str) -> ShapedWord {
        let size = font.size;
        let shaped = font.id.and_then(|id| self.shaper.shape(&self.db, id, text, size).ok());
        match shaped {
            Some(run) => {
                let scale = run.scale();
                let mut x = 0.0;
                let glyphs: Box<[Glyph]> = run
                    .glyphs
                    .iter()
                    .map(|g| {
                        let advance = g.x_advance as f32 * scale;
                        let glyph = Glyph { id: g.glyph_id, x: x + g.x_offset as f32 * scale, y: g.y_offset as f32 * scale, advance };
                        x += advance;
                        glyph
                    })
                    .collect();
                ShapedWord { font: font.id, size, width: x, glyphs }
            }
            // No font: an estimate, so layout still works
            None => {
                let width = text.chars().map(|c| if c == ' ' { 0.3 } else if c.is_ascii() { 0.55 } else { 1.0 }).sum::<f32>() * size;
                ShapedWord { font: None, size, width, glyphs: Box::new([]) }
            }
        }
    }
}
