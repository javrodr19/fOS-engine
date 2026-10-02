//! Canvas text: the `font` shorthand, shaping and glyph outlines
//!
//! Text is shaped with fos-text and drawn as paths built from the glyph
//! outlines, so it is transformed, filled, stroked and clipped like any
//! other path. Laid-out text is cached per (font, text), since pages tend
//! to draw the same labels every frame.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use fos_text::font::parser::OutlineBuilder;
use fos_text::{FontDatabase, FontParser, FontQuery, FontStyle, FontWeight, TextShaper};
use tiny_skia as sk;

use crate::canvas2d::{TextAlign, TextBaseline};

/// A parsed CSS `font` value
#[derive(Clone, Debug, PartialEq)]
pub struct FontSpec {
    pub style: FontStyle,
    pub weight: u16,
    /// In CSS pixels
    pub size: f32,
    pub families: Vec<String>,
    /// The value as the context reports it back
    pub serialized: String,
}

impl Default for FontSpec {
    fn default() -> Self {
        parse_font("10px sans-serif").expect("default font")
    }
}

/// Parse the CSS `font` shorthand: `[style] [variant] [weight] [stretch]
/// size[/line-height] family[, family]*`. Relative sizes resolve against
/// the 10px default (the canvas has no element to inherit from).
pub fn parse_font(value: &str) -> Option<FontSpec> {
    let value = value.trim();
    let mut style = FontStyle::Normal;
    let mut weight = 400u16;
    let mut rest = value;
    let mut style_word = "";
    let mut weight_word = String::new();
    let mut prefix = Vec::new();
    // Keywords before the size
    loop {
        let (word, tail) = match rest.find(char::is_whitespace) {
            Some(i) => (&rest[..i], rest[i..].trim_start()),
            None => return None,
        };
        let lower = word.to_ascii_lowercase();
        match lower.as_str() {
            "normal" => {}
            "italic" => {
                style = FontStyle::Italic;
                style_word = "italic";
            }
            "oblique" => {
                style = FontStyle::Oblique;
                style_word = "oblique";
            }
            "small-caps" => prefix.push("small-caps"),
            "bold" => weight = 700,
            "bolder" => weight = 700,
            "lighter" => weight = 100,
            "ultra-condensed" | "extra-condensed" | "condensed" | "semi-condensed" | "semi-expanded" | "expanded" | "extra-expanded" | "ultra-expanded" => {}
            w if w.len() == 3 && w.parse::<u16>().is_ok_and(|n| (1..=1000).contains(&n)) => weight = w.parse().unwrap(),
            _ => break,
        }
        if matches!(lower.as_str(), "bold" | "bolder" | "lighter") || lower.parse::<u16>().is_ok() {
            weight_word = lower.clone();
        }
        rest = tail;
    }
    // The size, with an optional line height
    let (size_word, families) = match rest.find(char::is_whitespace) {
        Some(i) => (&rest[..i], rest[i..].trim()),
        None => return None,
    };
    let size_word = size_word.split('/').next()?;
    let size = parse_size(size_word)?;
    if families.is_empty() {
        return None;
    }
    let mut family_list = Vec::new();
    for f in families.split(',') {
        let f = f.trim().trim_matches(|c| c == '"' || c == '\'').trim();
        if f.is_empty() {
            return None;
        }
        family_list.push(f.to_string());
    }
    // Serialized as browsers do: keywords that differ from normal, then
    // the size in px and the families as given
    let mut parts: Vec<String> = Vec::new();
    if !style_word.is_empty() {
        parts.push(style_word.to_string());
    }
    parts.extend(prefix.iter().map(|s| s.to_string()));
    if weight != 400 {
        parts.push(if weight_word == "bold" || weight == 700 { "bold".to_string() } else { weight.to_string() });
    }
    parts.push(format!("{}px", trim_float(size)));
    parts.push(families.split(',').map(str::trim).collect::<Vec<_>>().join(", "));
    Some(FontSpec { style, weight, size, families: family_list, serialized: parts.join(" ") })
}

fn trim_float(v: f32) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

fn parse_size(word: &str) -> Option<f32> {
    let w = word.to_ascii_lowercase();
    let keyword = match w.as_str() {
        "xx-small" => Some(9.0),
        "x-small" => Some(10.0),
        "small" => Some(13.0),
        "medium" => Some(16.0),
        "large" => Some(18.0),
        "x-large" => Some(24.0),
        "xx-large" => Some(32.0),
        "xxx-large" => Some(48.0),
        "smaller" => Some(10.0 / 1.2),
        "larger" => Some(12.0),
        _ => None,
    };
    if keyword.is_some() {
        return keyword;
    }
    let b = w.as_bytes();
    let mut num_end = usize::from(matches!(b.first(), Some(b'-' | b'+')));
    while num_end < b.len() && (b[num_end].is_ascii_digit() || b[num_end] == b'.') {
        num_end += 1;
    }
    // An exponent only when digits follow (`2em` is 2 em, `2e1px` is 20px)
    if num_end < b.len() && b[num_end] == b'e' {
        let mut j = num_end + 1;
        if j < b.len() && matches!(b[j], b'-' | b'+') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            num_end = j;
        }
    }
    let n: f32 = w[..num_end].parse().ok()?;
    let px = match &w[num_end..] {
        "px" => n,
        "pt" => n * 4.0 / 3.0,
        "pc" => n * 16.0,
        "in" => n * 96.0,
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 96.0 / 25.4,
        "q" => n * 96.0 / 101.6,
        "em" | "rem" => n * 10.0,
        "%" => n * 10.0 / 100.0,
        "" if n == 0.0 => 0.0,
        _ => return None,
    };
    (px >= 0.0 && px.is_finite()).then_some(px)
}

/// Text laid out at the origin: a path in CSS pixels (y down, baseline at
/// 0), its advance, and the font's ascent and descent
#[derive(Clone)]
pub struct TextLayout {
    pub path: Option<sk::Path>,
    pub advance: f32,
    pub ascent: f32,
    pub descent: f32,
    /// Ink bounds: (left, top, right, bottom) relative to the origin
    pub ink: (f32, f32, f32, f32),
}

/// What `measureText` returns
#[derive(Clone, Debug, Default)]
pub struct TextMetrics2D {
    pub width: f32,
    pub actual_left: f32,
    pub actual_right: f32,
    pub actual_ascent: f32,
    pub actual_descent: f32,
    pub font_ascent: f32,
    pub font_descent: f32,
    pub em_ascent: f32,
    pub em_descent: f32,
    pub hanging: f32,
    pub alphabetic: f32,
    pub ideographic: f32,
}

struct TextContext {
    fonts: Arc<FontDatabase>,
    shaper: TextShaper,
    cache: HashMap<(String, String, u32), TextLayout>,
}

thread_local! {
    static TEXT: RefCell<Option<TextContext>> = const { RefCell::new(None) };
}

/// Glyph outlines to a tiny-skia path: font units scaled to pixels,
/// y flipped, placed at the pen position
struct GlyphPath<'a> {
    builder: &'a mut sk::PathBuilder,
    scale: f32,
    x: f32,
    y: f32,
}

impl OutlineBuilder for GlyphPath<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(self.x + x * self.scale, self.y - y * self.scale);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(self.x + x * self.scale, self.y - y * self.scale);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(self.x + x1 * self.scale, self.y - y1 * self.scale, self.x + x * self.scale, self.y - y * self.scale);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let s = self.scale;
        self.builder.cubic_to(self.x + x1 * s, self.y - y1 * s, self.x + x2 * s, self.y - y2 * s, self.x + x * s, self.y - y * s);
    }
    fn close(&mut self) {
        self.builder.close();
    }
}

/// Lay out `text` in `font` (whitespace becomes spaces, as the standard
/// requires)
pub fn layout(font: &FontSpec, text: &str, letter_spacing: f32) -> Option<TextLayout> {
    let text: String = text.chars().map(|c| if matches!(c, '\t' | '\n' | '\x0c' | '\r') { ' ' } else { c }).collect();
    let key = (font.serialized.clone(), text.clone(), letter_spacing.to_bits());
    TEXT.with(|cell| {
        let mut slot = cell.borrow_mut();
        let ctx = slot.get_or_insert_with(|| TextContext { fonts: FontDatabase::shared(), shaper: TextShaper::new(), cache: HashMap::new() });
        if let Some(l) = ctx.cache.get(&key) {
            return Some(l.clone());
        }
        let l = layout_uncached(ctx, font, &text, letter_spacing);
        if ctx.cache.len() > 512 {
            ctx.cache.clear();
        }
        if let Some(l) = &l {
            ctx.cache.insert(key, l.clone());
        }
        l
    })
}

fn layout_uncached(ctx: &mut TextContext, font: &FontSpec, text: &str, letter_spacing: f32) -> Option<TextLayout> {
    let families: Vec<&str> = font.families.iter().map(String::as_str).collect();
    let query = FontQuery::new(&families).weight(FontWeight(font.weight)).style(font.style);
    let id = ctx.fonts.query(&query).or_else(|| ctx.fonts.query(&FontQuery::new(&["sans-serif"])))?;
    let data = ctx.fonts.face_data(id)?;
    let index = ctx.fonts.font(id).map_or(0, |f| f.index);
    let parser = FontParser::parse_index(&data, index).ok()?;
    let upem = parser.units_per_em().max(1) as f32;
    let scale = font.size / upem;
    let ascent = parser.ascender() as f32 * scale;
    let descent = -(parser.descender() as f32) * scale;
    if text.is_empty() || font.size == 0.0 {
        return Some(TextLayout { path: None, advance: 0.0, ascent, descent, ink: (0.0, 0.0, 0.0, 0.0) });
    }
    let run = ctx.shaper.shape_with_data(&data, index, text, font.size).ok()?;
    let mut builder = sk::PathBuilder::new();
    let mut pen = 0.0f32;
    for g in &run.glyphs {
        let mut gp = GlyphPath { builder: &mut builder, scale, x: pen + g.x_offset as f32 * scale, y: -(g.y_offset as f32) * scale };
        parser.outline_glyph(fos_text::font::parser::GlyphId(g.glyph_id), &mut gp);
        pen += g.x_advance as f32 * scale + letter_spacing;
    }
    let path = builder.finish();
    let ink = path.as_ref().map_or((0.0, 0.0, 0.0, 0.0), |p| {
        let b = p.bounds();
        (b.left(), b.top(), b.right(), b.bottom())
    });
    Some(TextLayout { path, advance: pen, ascent, descent, ink })
}

/// `measureText`, relative to the current alignment and baseline
pub fn measure(font: &FontSpec, text: &str, letter_spacing: f32, align: TextAlign, baseline: TextBaseline) -> TextMetrics2D {
    let Some(l) = layout(font, text, letter_spacing) else {
        // No fonts at all: estimate, so layout code still gets sizes
        let width = text.chars().count() as f32 * font.size * 0.5;
        return TextMetrics2D { width, actual_right: width, actual_ascent: font.size * 0.8, actual_descent: font.size * 0.2, font_ascent: font.size * 0.8, font_descent: font.size * 0.2, em_ascent: font.size * 0.8, em_descent: font.size * 0.2, ..Default::default() };
    };
    let x_offset = match align {
        TextAlign::Left | TextAlign::Start => 0.0,
        TextAlign::Right | TextAlign::End => l.advance,
        TextAlign::Center => l.advance / 2.0,
    };
    let base = match baseline {
        TextBaseline::Alphabetic => 0.0,
        TextBaseline::Top => l.ascent,
        TextBaseline::Hanging => l.ascent * 0.8,
        TextBaseline::Middle => (l.ascent - l.descent) / 2.0,
        TextBaseline::Ideographic | TextBaseline::Bottom => -l.descent,
    };
    let (left, top, right, bottom) = l.ink;
    let em_total = l.ascent + l.descent;
    let em_ascent = if em_total > 0.0 { font.size * l.ascent / em_total } else { font.size * 0.8 };
    TextMetrics2D {
        width: l.advance,
        actual_left: x_offset - left,
        actual_right: right - x_offset,
        actual_ascent: base - top,
        actual_descent: bottom - base,
        font_ascent: l.ascent + base,
        font_descent: l.descent - base,
        em_ascent: em_ascent + base,
        em_descent: font.size - em_ascent - base,
        hanging: l.ascent * 0.8 + base,
        alphabetic: base,
        ideographic: -l.descent + base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_shorthand() {
        let f = parse_font("italic bold 12pt/1.5 \"Helvetica Neue\", Arial, sans-serif").unwrap();
        assert_eq!((f.style, f.weight, f.size), (FontStyle::Italic, 700, 16.0));
        assert_eq!(f.families, ["Helvetica Neue", "Arial", "sans-serif"]);
        assert_eq!(f.serialized, "italic bold 16px \"Helvetica Neue\", Arial, sans-serif");
        assert_eq!(parse_font("10px sans-serif").unwrap().serialized, "10px sans-serif");
        assert_eq!(parse_font("300 2em serif").unwrap().serialized, "300 20px serif");
        for bad in ["", "bold", "12px", "12 sans-serif", "12px ,", "foo 12px serif"] {
            assert!(parse_font(bad).is_none(), "{bad}");
        }
    }
}
