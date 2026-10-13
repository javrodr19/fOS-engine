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
    /// The height of lowercase letters (what `vertical-align: middle`
    /// centers on)
    pub x_height: f32,
    /// The x-height was measured from the "x" glyph, whose hinted outline
    /// has it in whole pixels
    pub x_height_measured: bool,
    /// The OS/2 table's average character width (0 when absent) and the
    /// width of the face's bounding box: what text fields are sized by
    pub avg_char_width: f32,
    pub max_char_width: f32,
    /// Every glyph is as wide (the post table's isFixedPitch)
    pub fixed_pitch: bool,
}

impl FontMetrics {
    /// When no font is available
    pub const FALLBACK: FontMetrics = FontMetrics { ascent: 0.8, descent: 0.2, line_gap: 0.0, x_height: 0.448, x_height_measured: false, avg_char_width: 0.0, max_char_width: 0.0, fixed_pitch: false };

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
    /// Glyph ranges drawn from fallback faces (characters `font` lacks):
    /// each entry's face applies from its glyph index until the next
    /// entry; `None` returns to `font`
    pub fallback: Box<[(u32, Option<FontId>)]>,
}

impl ShapedWord {
    /// The face of each glyph
    pub fn glyph_fonts(&self) -> impl Iterator<Item = (Option<FontId>, &Glyph)> {
        let mut runs = self.fallback.iter().peekable();
        let mut current = self.font;
        self.glyphs.iter().enumerate().map(move |(i, g)| {
            while let Some(&&(start, face)) = runs.peek() {
                if start as usize > i {
                    break;
                }
                current = face.or(self.font);
                runs.next();
            }
            (current, g)
        })
    }
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
    /// Above the baseline, in whole pixels as browsers round font metrics
    pub fn ascent(&self) -> f32 {
        (self.metrics.ascent * self.size).round()
    }

    /// Below the baseline, in whole pixels
    pub fn descent(&self) -> f32 {
        (self.metrics.descent * self.size).round()
    }

    /// The x-height in pixels
    pub fn x_height(&self) -> f32 {
        let x = self.metrics.x_height * self.size;
        if self.metrics.x_height_measured {
            x.round()
        } else {
            x
        }
    }

    /// `line-height` in pixels: `normal` is the font's own line spacing
    /// (ascent, descent and line gap, each rounded)
    pub fn line_height(&self, lh: fos_css::style::LineHeight) -> f32 {
        match lh {
            fos_css::style::LineHeight::Normal => self.ascent() + self.descent() + (self.metrics.line_gap * self.size).round(),
            other => other.resolve(self.size),
        }
    }
}

/// Shaped words kept before the cache starts over
const MAX_WORDS: usize = 50_000;

/// What `ch` and `ex` are for styling, from the faces text is drawn with
pub struct FontUnits {
    fonts: std::sync::Mutex<FontContext>,
    /// (families, weight, italic, size) -> (ch, ex) in pixels
    units: std::sync::Mutex<HashMap<(Arc<[Arc<str>]>, u16, bool, u32), (f32, f32)>>,
}

impl FontUnits {
    pub fn new(db: Arc<FontDatabase>) -> Self {
        FontUnits { fonts: std::sync::Mutex::new(FontContext::new(db)), units: Default::default() }
    }
}

impl fos_css::style::FontUnits for FontUnits {
    fn ch_ex(&self, font: &fos_css::style::InheritedStyle, size: f32) -> (f32, f32) {
        let key = (font.font_family.clone(), font.font_weight, font.font_style != FontStyle::Normal, size.to_bits());
        if let Some(units) = self.units.lock().map_or(None, |u| u.get(&key).copied()) {
            return units;
        }
        let Ok(mut fonts) = self.fonts.lock() else { return (size * 0.5, size * 0.5) };
        let mut style = Style::default();
        let i = Arc::make_mut(&mut style.inherited);
        i.font_family = font.font_family.clone();
        i.font_weight = font.font_weight;
        i.font_style = font.font_style;
        i.font_size = size;
        // At the size asked for: a variable face's optical size changes
        // its advances. "0" is half an em without a face; the x-height is
        // as text is laid out with it (a measured one in whole pixels)
        let f = fonts.resolve(&style);
        let ch = if f.id.is_some() { fonts.shape(&f, "0").width } else { 0.0 };
        let units = (if ch > 0.0 { ch } else { size * 0.5 }, f.x_height());
        if let Ok(mut u) = self.units.lock() {
            u.insert(key, units);
        }
        units
    }
}

pub struct FontContext {
    db: Arc<FontDatabase>,
    shaper: TextShaper,
    /// (families, weight, italic) -> face
    selection: HashMap<(Arc<[Arc<str>]>, u16, bool), (Option<FontId>, u8)>,
    metrics: HashMap<FontId, FontMetrics>,
    /// Shaped words by (face, size bits), then text
    words: HashMap<(Option<FontId>, u32), HashMap<Box<str>, Arc<ShapedWord>>>,
    word_count: usize,
    /// The face to draw a character with when the chosen one lacks it
    fallback: HashMap<char, Option<FontId>>,
    /// Faces to try for fallback, best first (built on first need)
    fallback_order: Option<Vec<FontId>>,
    /// (face, weight, size) -> the face to draw (a variable face's instance)
    instances: HashMap<(FontId, u16, u16), FontId>,
}

impl Default for FontContext {
    fn default() -> Self {
        Self::new(FontDatabase::shared())
    }
}

impl FontContext {
    pub fn new(db: Arc<FontDatabase>) -> Self {
        FontContext { db, shaper: TextShaper::new(), selection: HashMap::new(), metrics: HashMap::new(), words: HashMap::new(), word_count: 0, fallback: HashMap::new(), fallback_order: None, instances: HashMap::new() }
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
                // As in Chromium on Linux: names of fonts that are not
                // installed (and of Apple's system fonts) match nothing, and
                // when nothing in the list does, the standard font, serif,
                // is used
                let mut families: Vec<&str> = i.font_family.iter().map(|f| &**f).collect();
                if families.last() != Some(&"serif") {
                    families.push("serif");
                }
                let style = if key.2 { fos_text::FontStyle::Italic } else { fos_text::FontStyle::Normal };
                let query = FontQuery::new(&families).weight(fos_text::FontWeight(i.font_weight)).style(style);
                let id = self.db.query(&query);
                // Browsers slant and thicken faces that lack the style
                let mut synthesis = 0;
                if let Some(face) = id.and_then(|id| self.db.font(id)) {
                    if key.2 && face.style == fos_text::FontStyle::Normal {
                        synthesis |= fos_text::SYNTH_OBLIQUE;
                    }
                    if key.1 >= 600 && face.weights.1 < 600 {
                        synthesis |= fos_text::SYNTH_BOLD;
                    }
                }
                self.selection.insert(key, (id, synthesis));
                (id, synthesis)
            }
        };
        // A variable face is drawn through its instance at the weight and
        // (font-optical-sizing: auto) the size asked for
        let id = id.map(|id| {
            let key = (id, i.font_weight, i.font_size.round().clamp(1.0, 1000.0) as u16);
            *self.instances.entry(key).or_insert_with(|| self.db.instance(id, key.1, key.2))
        });
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
                // As Skia: OS/2's sxHeight (table version 2 and up), else
                // the height of the "x" glyph, else (Blink) 56% of the ascent
                let os2 = p.table_data(b"OS/2").filter(|t| t.len() >= 88 && u16::from_be_bytes([t[0], t[1]]) >= 2).map(|t| i16::from_be_bytes([t[86], t[87]]));
                let glyph = || p.glyph_index('x').and_then(|g| p.glyph_bounding_box(g)).map(|b| b.y_max);
                let os2 = os2.filter(|&h| h > 0);
                let measured = if os2.is_none() { glyph() } else { None };
                let x = os2.or(measured).map(|h| h as f32 / upem).filter(|&x| x > 0.0 && x < a);
                // Faces with broken metrics get sane ones
                (a > 0.0 && d >= 0.0 && a + d < 4.0).then_some(FontMetrics {
                    ascent: a,
                    descent: d,
                    line_gap: g.max(0.0),
                    x_height: x.unwrap_or(a * 0.56),
                    x_height_measured: x.is_some() && measured.is_some(),
                    avg_char_width: p.table_data(b"OS/2").filter(|t| t.len() >= 4).map_or(0.0, |t| i16::from_be_bytes([t[2], t[3]]).max(0) as f32 / upem),
                    max_char_width: p.table_data(b"head").filter(|t| t.len() >= 44).map_or(0.0, |t| (i16::from_be_bytes([t[40], t[41]]) as f32 - i16::from_be_bytes([t[36], t[37]]) as f32).max(0.0) / upem),
                    fixed_pitch: p.table_data(b"post").filter(|t| t.len() >= 16).is_some_and(|t| t[12..16] != [0, 0, 0, 0]),
                })
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
        if text.chars().any(is_complex) {
            if let Some(w) = self.shape_complex(font, text) {
                return w;
            }
        }
        let size = font.size;
        let shaped = font.id.and_then(|id| self.shaper.shape(&self.db, id, text, size).ok());
        match shaped {
            Some(run) if run.glyphs.iter().any(|g| g.glyph_id == 0) && !text.trim().is_empty() => self.shape_with_fallback(font, text, run),
            Some(run) => {
                let (glyphs, width) = positioned(&run, 0.0, self.whole_pixels(font.id));
                ShapedWord { font: font.id, size, width, glyphs: glyphs.into(), fallback: Box::new([]) }
            }
            // No font: an estimate, so layout still works
            None => {
                let width = text.chars().map(|c| if c == ' ' { 0.3 } else if c.is_ascii() { 0.55 } else { 1.0 }).sum::<f32>() * size;
                ShapedWord { font: None, size, width, glyphs: Box::new([]), fallback: Box::new([]) }
            }
        }
    }

    /// Shape a word in a complex script (joining, reordering, right to
    /// left) with HarfBuzz's algorithm, in the font when it has the
    /// script's characters or else a fallback face; glyphs come out in
    /// visual order
    fn shape_complex(&mut self, font: &ResolvedFont, text: &str) -> Option<ShapedWord> {
        let first = text.chars().find(|&c| is_complex(c))?;
        let covers = |db: &FontDatabase, id: FontId| {
            db.with_face_data(id, |data, index| rustybuzz::Face::from_slice(data, index).is_some_and(|f| text.chars().filter(|&c| is_complex(c)).all(|c| f.glyph_index(c).is_some())))
                .unwrap_or(false)
        };
        let face_id = match font.id {
            Some(id) if covers(&self.db, id) => id,
            _ => self.fallback_face(first)?,
        };
        let size = font.size;
        let (glyphs, width) = self.db.with_face_data(face_id, |data, index| {
            let face = rustybuzz::Face::from_slice(data, index)?;
            let mut buf = rustybuzz::UnicodeBuffer::new();
            buf.push_str(text);
            buf.guess_segment_properties();
            let out = rustybuzz::shape(&face, &[], buf);
            let scale = size / face.units_per_em() as f32;
            let mut x = 0.0;
            let glyphs: Vec<Glyph> = out
                .glyph_infos()
                .iter()
                .zip(out.glyph_positions())
                .map(|(info, pos)| {
                    let g = Glyph { id: info.glyph_id as u16, x: x + pos.x_offset as f32 * scale, y: pos.y_offset as f32 * scale, advance: pos.x_advance as f32 * scale };
                    x += pos.x_advance as f32 * scale;
                    g
                })
                .collect();
            Some((glyphs, x))
        })??;
        let fallback: Box<[(u32, Option<FontId>)]> = if Some(face_id) == font.id { Box::new([]) } else { Box::new([(0, Some(face_id))]) };
        Some(ShapedWord { font: font.id, size, width, glyphs: glyphs.into(), fallback })
    }

    /// Shape `text` in runs: characters the face lacks in a fallback face
    /// Whether glyphs of face `id` advance by whole pixels: web fonts do,
    /// as in Chromium on Linux, which positions them without subpixel
    /// precision (system fonts keep fractional advances)
    fn whole_pixels(&self, id: Option<FontId>) -> bool {
        id.and_then(|id| self.db.font(id)).is_some_and(|f| f.web)
    }

    fn shape_with_fallback(&mut self, font: &ResolvedFont, text: &str, first: fos_text::ShapedRun) -> ShapedWord {
        let missing: std::collections::HashSet<u32> = first.glyphs.iter().filter(|g| g.glyph_id == 0).map(|g| g.cluster).collect();
        // Runs of characters by face
        let mut runs: Vec<(Option<FontId>, String)> = Vec::new();
        for (i, c) in text.chars().enumerate() {
            let face = if missing.contains(&(i as u32)) && !c.is_whitespace() { self.fallback_face(c).or(font.id) } else { font.id };
            match runs.last_mut() {
                Some((f, s)) if *f == face => s.push(c),
                _ => runs.push((face, c.to_string())),
            }
        }
        let mut glyphs: Vec<Glyph> = Vec::new();
        let mut fallback: Vec<(u32, Option<FontId>)> = Vec::new();
        let mut x = 0.0;
        for (face, run_text) in runs {
            let Some(run) = face.and_then(|id| self.shaper.shape(&self.db, id, &run_text, font.size).ok()) else { continue };
            let last_face = fallback.last().map_or(font.id, |l| l.1.or(font.id));
            if face != last_face {
                fallback.push((glyphs.len() as u32, if face == font.id { None } else { face }));
            }
            let (g, w) = positioned(&run, x, self.whole_pixels(face));
            glyphs.extend(g);
            x = w;
        }
        ShapedWord { font: font.id, size: font.size, width: x, glyphs: glyphs.into(), fallback: fallback.into() }
    }

    /// A face that has character `c` (cached), from the faces the
    /// rasterizer can draw, general-purpose families first
    fn fallback_face(&mut self, c: char) -> Option<FontId> {
        if let Some(f) = self.fallback.get(&c) {
            return *f;
        }
        let db = self.db.clone();
        let order = self.fallback_order.get_or_insert_with(|| {
            let mut faces: Vec<(u32, FontId)> = (0..db.len() as u32)
                .filter_map(|i| db.font(FontId(i)))
                .filter(|f| f.has_glyf)
                .map(|f| {
                    let family = f.family.to_lowercase();
                    let mut score = 0;
                    if f.style != fos_text::FontStyle::Normal {
                        score += 1000;
                    }
                    score += (f.weight.0 as i32 - 400).unsigned_abs() as u32;
                    if family.contains("mono") {
                        score += 500;
                    }
                    if !family.contains("sans") {
                        score += 200;
                    }
                    (score + family.len() as u32, f.id)
                })
                .collect();
            faces.sort_by_key(|f| f.0);
            faces.into_iter().map(|f| f.1).collect()
        });
        let found = order.iter().copied().find(|&id| db.with_face_data(id, |data, index| FontParser::parse_index(data, index).ok().and_then(|p| p.glyph_index(c)).is_some_and(|g| g.0 != 0)).unwrap_or(false));
        self.fallback.insert(c, found);
        found
    }
}

/// A run's glyphs positioned from `x0`, and the pen position after them
/// Characters of scripts that need complex shaping: Hebrew, Arabic and
/// the other right-to-left scripts, Indic scripts, Thai, Lao, Tibetan,
/// Myanmar and Khmer
pub fn is_complex(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0x0900..=0x0DFF | 0x0E00..=0x0FFF | 0x1000..=0x109F | 0x1780..=0x17FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF)
}

/// Glyphs of `run` placed from `x0`, and where the run ends; with
/// `whole_pixels`, each glyph's advance (kerning included) is rounded
fn positioned(run: &fos_text::ShapedRun, x0: f32, whole_pixels: bool) -> (Vec<Glyph>, f32) {
    let scale = run.scale();
    let mut x = x0;
    let glyphs = run
        .glyphs
        .iter()
        .map(|g| {
            // (whole pixels: the glyph's own advance and its kerning are
            // each rounded, as Chromium's hinted advances and kerning are)
            let advance = if whole_pixels {
                ((g.x_advance - g.x_kerning) as f32 * scale).round() + (g.x_kerning as f32 * scale).round()
            } else {
                g.x_advance as f32 * scale
            };
            let glyph = Glyph { id: g.glyph_id, x: x + g.x_offset as f32 * scale, y: g.y_offset as f32 * scale, advance };
            x += advance;
            glyph
        })
        .collect();
    (glyphs, x)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_families_fall_back_to_serif() {
        let mut fonts = FontContext::default();
        let mut face = |families: &[&str]| {
            let mut style = Style::default();
            std::sync::Arc::make_mut(&mut style.inherited).font_family = families.iter().map(|f| Arc::from(*f)).collect();
            fonts.resolve(&style).id
        };
        let serif = face(&["serif"]);
        assert_eq!(face(&["no such font"]), serif);
        assert_eq!(face(&["-apple-system", "BlinkMacSystemFont"]), serif);
        // Apple's names are skipped, not taken for sans-serif
        assert_eq!(face(&["-apple-system", "monospace"]), face(&["monospace"]));
    }

    #[test]
    fn missing_characters_come_from_fallback_faces() {
        let mut fonts = FontContext::default();
        let mut style = Style::default();
        std::sync::Arc::make_mut(&mut style.inherited).font_family = Arc::from([Arc::from("sans-serif")]);
        let font = fonts.resolve(&style);
        let Some(primary) = font.id else { return };
        let latin = fonts.shape(&font, "hello");
        assert!(latin.fallback.is_empty());
        // Chinese is rarely in the default sans face; when some face has it,
        // no glyph is left as .notdef
        let mixed = fonts.shape(&font, "a中文b");
        if mixed.fallback.is_empty() {
            return; // the default face covers it, or nothing does
        }
        let faces: Vec<Option<FontId>> = mixed.glyph_fonts().map(|(f, _)| f).collect();
        assert_eq!(faces.first().copied().flatten(), Some(primary));
        assert!(faces.iter().any(|f| *f != Some(primary)));
        assert!(mixed.glyphs.iter().all(|g| g.id != 0), "{mixed:?}");
        assert!(mixed.width > latin.width / 5.0 * 2.0);
    }
}
