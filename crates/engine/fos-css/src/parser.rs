//! CSS parser
//!
//! A small, forgiving parser following CSS Syntax Level 3's error recovery:
//! a malformed rule or declaration is skipped and parsing carries on after
//! it, so one bad line never costs a page its whole stylesheet.
//!
//! - Style rules keep each selector's source text and its specificity
//!   (computed by the selector engine that matches them).
//! - `@media` blocks are included when their query matches the viewport
//!   (level 4 range syntax included); `@supports`, `@layer`, `@container`
//!   and `@scope` blocks are included; other at-rules are skipped.
//! - Declarations are converted for the properties the engine models, with
//!   shorthands (`font`, `margin`, `padding`, `background`) expanded.

use crate::properties::{Color, Keyword, Length, LengthUnit, PropertyId, PropertyValue};
use crate::{CssError, Declaration, Rule, Selector, Specificity, Stylesheet};

/// The viewport media queries are evaluated against
#[derive(Debug, Clone, Copy)]
pub struct MediaContext {
    /// Viewport width in CSS pixels
    pub width: f32,
    /// Viewport height in CSS pixels
    pub height: f32,
}

impl Default for MediaContext {
    fn default() -> Self {
        Self { width: 1024.0, height: 768.0 }
    }
}

/// CSS Parser
pub struct CssParser {
    media: MediaContext,
}

impl CssParser {
    pub fn new() -> Self {
        Self { media: MediaContext::default() }
    }

    /// A parser evaluating media queries against `media`
    pub fn with_media(media: MediaContext) -> Self {
        Self { media }
    }

    /// Parse a CSS stylesheet (never fails: errors are skipped)
    pub fn parse(&self, css: &str) -> Result<Stylesheet, CssError> {
        let mut out = Stylesheet::new();
        parse_rules(css, &self.media, &mut out.rules, &mut out.keyframes);
        Ok(out)
    }
}

impl Default for CssParser {
    fn default() -> Self {
        Self::new()
    }
}

// ---- scanning ----

/// Position just past a comment, string or balanced block starting at `i`
/// (or `i + 1` for any other byte)
#[inline]
fn skip_token(b: &[u8], i: usize) -> usize {
    // Most bytes start nothing: no call for them
    if !matches!(b[i], b'/' | b'"' | b'\'' | b'\\' | b'(' | b'[' | b'{') {
        return i + 1;
    }
    skip_special(b, i)
}

#[inline(never)]
fn skip_special(b: &[u8], i: usize) -> usize {
    match b[i] {
        b'/' if b.get(i + 1) == Some(&b'*') => {
            let mut j = i + 2;
            while j + 1 < b.len() && !(b[j] == b'*' && b[j + 1] == b'/') {
                j += 1;
            }
            (j + 2).min(b.len())
        }
        q @ (b'"' | b'\'') => {
            let mut j = i + 1;
            while j < b.len() && b[j] != q && b[j] != b'\n' {
                if b[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            (j + 1).min(b.len())
        }
        b'\\' => (i + 2).min(b.len()),
        open @ (b'(' | b'[' | b'{') => {
            let close = match open {
                b'(' => b')',
                b'[' => b']',
                _ => b'}',
            };
            let mut j = i + 1;
            while j < b.len() && b[j] != close {
                j = skip_token(b, j);
            }
            (j + 1).min(b.len())
        }
        _ => i + 1,
    }
}

/// Index of the first of `stops` at nesting depth 0 from `i` (or the end)
fn find_top(b: &[u8], mut i: usize, stops: &[u8]) -> usize {
    while i < b.len() {
        if stops.contains(&b[i]) {
            return i;
        }
        i = skip_token(b, i);
    }
    b.len()
}

/// Split at top-level `sep` bytes
pub(crate) fn split_top(s: &str, sep: u8) -> Vec<&str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == sep {
            out.push(&s[start..i]);
            start = i + 1;
            i += 1;
        } else {
            i = skip_token(b, i);
        }
    }
    out.push(&s[start..]);
    out
}

/// `s` without comments
fn strip_comments(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains("/*") {
        return std::borrow::Cow::Borrowed(s);
    }
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            i = skip_token(b, i);
            out.push(' ');
        } else {
            let j = if matches!(b[i], b'"' | b'\'') { skip_token(b, i) } else { i + 1 };
            // Copy whole UTF-8 sequences
            let mut end = j;
            while end < b.len() && (b[end] & 0xC0) == 0x80 {
                end += 1;
            }
            out.push_str(&s[i..end]);
            i = end;
        }
    }
    std::borrow::Cow::Owned(out)
}

fn skip_ws_and_comments(b: &[u8], mut i: usize) -> usize {
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if b[i..].starts_with(b"/*") {
            i = skip_token(b, i);
        } else if b[i..].starts_with(b"<!--") {
            i += 4;
        } else if b[i..].starts_with(b"-->") {
            i += 3;
        } else {
            return i;
        }
    }
}

// ---- @font-face ----

/// A web font an `@font-face` rule declares
#[derive(Debug, Clone, PartialEq)]
pub struct FontFace {
    pub family: String,
    /// Sources in preference order: URL and `format()` hint (lowercase)
    pub src: Vec<(String, Option<String>)>,
    /// The weights it covers (a range for variable fonts)
    pub weight: (u16, u16),
    pub italic: bool,
    /// Code point ranges it covers (empty: all)
    pub unicode_range: Vec<(u32, u32)>,
}

impl FontFace {
    /// Whether the face has code point `c` (by its unicode-range)
    pub fn covers(&self, c: char) -> bool {
        self.unicode_range.is_empty() || self.unicode_range.iter().any(|&(a, b)| (a..=b).contains(&(c as u32)))
    }
}

/// `U+0025-00FF`, `U+4??`, `U+1F600` ranges
fn unicode_ranges(v: &str) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for part in v.split(',') {
        let p = part.trim();
        let Some(r) = p.strip_prefix("U+").or_else(|| p.strip_prefix("u+")) else { continue };
        let range = if let Some((a, b)) = r.split_once('-') {
            u32::from_str_radix(a, 16).ok().zip(u32::from_str_radix(b, 16).ok())
        } else if r.contains('?') {
            u32::from_str_radix(&r.replace('?', "0"), 16).ok().zip(u32::from_str_radix(&r.replace('?', "F"), 16).ok())
        } else {
            u32::from_str_radix(r, 16).ok().map(|a| (a, a))
        };
        if let Some(r) = range {
            out.push(r);
        }
    }
    out
}

/// The `@font-face` rules of a stylesheet (in matching `@media`,
/// `@supports` and `@layer` blocks too)
pub fn font_faces(css: &str, media: &MediaContext) -> Vec<FontFace> {
    let mut out = Vec::new();
    collect_font_faces(css, media, &mut out);
    out
}

fn collect_font_faces(css: &str, media: &MediaContext, out: &mut Vec<FontFace>) {
    let b = css.as_bytes();
    let mut i = 0;
    while i < b.len() {
        i = skip_ws_and_comments(b, i);
        if i >= b.len() {
            break;
        }
        if b[i] != b'@' {
            // A style rule: skip its block
            let end = find_top(b, i, b"{;}");
            i = if end < b.len() && b[end] == b'{' { skip_token(b, end) } else { end + 1 };
            continue;
        }
        let name_end = b[i + 1..].iter().position(|c| !(c.is_ascii_alphanumeric() || *c == b'-')).map_or(b.len(), |p| i + 1 + p);
        let name = css[i + 1..name_end].to_ascii_lowercase();
        let end = find_top(b, name_end, b";{}");
        if end >= b.len() || b[end] != b'{' {
            i = end + 1;
            continue;
        }
        let prelude = &css[name_end..end];
        let block_end = skip_token(b, end);
        let block = &css[end + 1..block_end.saturating_sub(1).max(end + 1)];
        match name.as_str() {
            "font-face" => out.extend(font_face(block)),
            "media" if media_matches(prelude, media) => collect_font_faces(block, media, out),
            "supports" if supports(prelude) => collect_font_faces(block, media, out),
            "layer" => collect_font_faces(block, media, out),
            _ => {}
        }
        i = block_end;
    }
}

fn unquote(s: &str) -> &str {
    let s = s.trim();
    s.strip_prefix('"').and_then(|r| r.strip_suffix('"')).or_else(|| s.strip_prefix('\'').and_then(|r| r.strip_suffix('\''))).unwrap_or(s)
}

fn font_face(block: &str) -> Option<FontFace> {
    let block = strip_comments(block);
    let (mut family, mut src, mut weight, mut italic) = (None, Vec::new(), (400, 400), false);
    let mut unicode_range = Vec::new();
    for decl in split_top(&block, b';') {
        let Some((name, value)) = decl.split_once(':') else { continue };
        let value = value.trim();
        match name.trim().to_ascii_lowercase().as_str() {
            "font-family" => family = Some(unquote(value).to_string()),
            "src" => {
                for one in split_top(value, b',') {
                    let one = one.trim();
                    let lower = one.to_ascii_lowercase();
                    let Some(start) = lower.find("url(") else { continue };
                    let rest = &one[start + 4..];
                    let Some(close) = rest.find(')') else { continue };
                    let url = unquote(&rest[..close]).to_string();
                    let format = lower.find("format(").and_then(|f| {
                        let r = &lower[f + 7..];
                        r.find(')').map(|c| unquote(&r[..c]).to_string())
                    });
                    if !url.is_empty() {
                        src.push((url, format));
                    }
                }
            }
            "font-weight" => {
                let w = |v: &str| match v {
                    "normal" => Some(400),
                    "bold" => Some(700),
                    n => n.parse::<f32>().ok().map(|n| n.clamp(1.0, 1000.0) as u16),
                };
                let parts: Vec<&str> = value.split_whitespace().collect();
                if let [a, rest @ ..] = parts.as_slice() {
                    if let Some(a) = w(&a.to_ascii_lowercase()) {
                        let b = rest.first().and_then(|b| w(&b.to_ascii_lowercase())).unwrap_or(a);
                        weight = (a.min(b), a.max(b));
                    }
                }
            }
            "font-style" => {
                let v = value.to_ascii_lowercase();
                italic = v.starts_with("italic") || v.starts_with("oblique");
            }
            "unicode-range" => unicode_range = unicode_ranges(value),
            _ => {}
        }
    }
    let family = family.filter(|f| !f.is_empty())?;
    (!src.is_empty()).then_some(FontFace { family, src, weight, italic, unicode_range })
}

// ---- rules ----

fn parse_rules(css: &str, media: &MediaContext, out: &mut Vec<Rule>, keyframes: &mut Vec<crate::Keyframes>) {
    let b = css.as_bytes();
    let mut i = 0;
    while i < b.len() {
        i = skip_ws_and_comments(b, i);
        if i >= b.len() {
            break;
        }
        if b[i] == b'@' {
            let name_end = b[i + 1..].iter().position(|c| !(c.is_ascii_alphanumeric() || *c == b'-')).map_or(b.len(), |p| i + 1 + p);
            let name = css[i + 1..name_end].to_ascii_lowercase();
            let end = find_top(b, name_end, b";{}");
            let prelude = &css[name_end..end];
            if end >= b.len() || b[end] != b'{' {
                // A statement at-rule (@import, @charset, @namespace)
                i = end + 1;
                continue;
            }
            let block_end = skip_token(b, end);
            let block = &css[end + 1..block_end.saturating_sub(1).max(end + 1)];
            match name.as_str() {
                "media" if media_matches(prelude, media) => parse_rules(block, media, out, keyframes),
                "supports" if supports(prelude) => parse_rules(block, media, out, keyframes),
                "layer" | "container" | "scope" | "document" | "-moz-document" | "starting-style" => parse_rules(block, media, out, keyframes),
                "keyframes" | "-webkit-keyframes" | "-moz-keyframes" => keyframes.extend(keyframes_rule(prelude, block)),
                _ => {}
            }
            i = block_end;
            continue;
        }
        let end = find_top(b, i, b"{;}");
        if end >= b.len() {
            break;
        }
        if b[end] != b'{' {
            // Junk before a `;` or `}`: skip it
            i = end + 1;
            continue;
        }
        let prelude = &css[i..end];
        let block_end = skip_token(b, end);
        let block = &css[end + 1..block_end.saturating_sub(1).max(end + 1)];
        i = block_end;

        let prelude = strip_comments(prelude);
        let texts: Vec<String> = split_top(&prelude, b',').into_iter().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string).collect();
        style_rule(&texts, block, media, out, 0);
    }
}

/// A style rule's selectors (texts) and block: its declarations become a
/// rule, and rules nested in it (CSS nesting) follow as rules of their
/// own, with `&` standing for each parent selector (or the parent as an
/// ancestor when there is none); nested `@media`/`@supports` blocks apply
/// to the parent's selectors
/// An `@keyframes` rule's name and keyframes (`from`, `to`, percentages,
/// comma lists of them)
fn keyframes_rule(prelude: &str, block: &str) -> Option<crate::Keyframes> {
    let name = prelude.trim().trim_matches(|c| c == '"' || c == '\'').to_string();
    if name.is_empty() {
        return None;
    }
    let b = block.as_bytes();
    let mut frames: Vec<(f32, Vec<Declaration>)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        i = skip_ws_and_comments(b, i);
        if i >= b.len() {
            break;
        }
        let end = find_top(b, i, b"{}");
        if end >= b.len() || b[end] != b'{' {
            break;
        }
        let selectors = &block[i..end];
        let block_end = skip_token(b, end);
        let body = &block[end + 1..block_end.saturating_sub(1).max(end + 1)];
        let declarations = parse_declarations(body);
        for sel in selectors.split(',') {
            let sel = sel.trim().to_ascii_lowercase();
            let offset = match sel.as_str() {
                "from" => Some(0.0),
                "to" => Some(1.0),
                p => p.strip_suffix('%').and_then(|n| n.trim().parse::<f32>().ok()).map(|n| n / 100.0),
            };
            if let Some(offset) = offset.filter(|o| (0.0..=1.0).contains(o)) {
                frames.push((offset, declarations.clone()));
            }
        }
        i = block_end;
    }
    frames.sort_by(|a, b| a.0.total_cmp(&b.0));
    Some(crate::Keyframes { name, frames })
}

fn style_rule(texts: &[String], block: &str, media: &MediaContext, out: &mut Vec<Rule>, depth: u32) {
    let selectors: Vec<Selector> = texts
        .iter()
        .filter_map(|text| {
            let parsed = fos_dom::SelectorList::parse(text)?;
            let (a, b, c) = *parsed.specificities().first()?;
            Some(Selector { text: text.clone(), specificity: Specificity(a, b, c), parts: Vec::new(), parsed: Some(parsed) })
        })
        .collect();
    if selectors.is_empty() {
        return;
    }
    // Split the block: declarations, nested rules, nested at-rules
    let b = block.as_bytes();
    let mut decls = String::new();
    let mut nested: Vec<(&str, &str)> = Vec::new();
    let mut nested_at: Vec<(String, &str, &str)> = Vec::new();
    let mut i = 0;
    if block.contains('{') {
        while i < b.len() {
            i = skip_ws_and_comments(b, i);
            if i >= b.len() {
                break;
            }
            let end = find_top(b, i, b";{}");
            if end < b.len() && b[end] == b'{' {
                let block_end = skip_token(b, end);
                let inner = &block[end + 1..block_end.saturating_sub(1).max(end + 1)];
                let prelude = block[i..end].trim();
                if let Some(at) = prelude.strip_prefix('@') {
                    let name_end = at.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).unwrap_or(at.len());
                    nested_at.push((at[..name_end].to_ascii_lowercase(), &at[name_end..], inner));
                } else {
                    nested.push((prelude, inner));
                }
                i = block_end;
            } else {
                decls.push_str(&block[i..end.min(b.len())]);
                decls.push(';');
                i = end + 1;
            }
        }
    }
    // Rules are kept even when none of their declarations is modeled
    // (the stylesheet mirrors the source); matching skips them
    let mut declarations = if block.contains('{') { parse_declarations(&decls) } else { parse_declarations(block) };
    declarations.shrink_to_fit();
    let mut selectors = selectors;
    selectors.shrink_to_fit();
    if !declarations.is_empty() || (nested.is_empty() && nested_at.is_empty()) {
        out.push(Rule { selectors, declarations });
    }
    if depth > 16 {
        return;
    }
    for (prelude, inner) in nested {
        let mut combined = Vec::new();
        for n in split_top(&strip_comments(prelude), b',').into_iter().map(str::trim).filter(|s| !s.is_empty()) {
            for parent in texts {
                combined.push(if n.contains('&') {
                    n.replace('&', parent)
                } else {
                    format!("{parent} {n}")
                });
            }
        }
        style_rule(&combined, inner, media, out, depth + 1);
    }
    for (name, prelude, inner) in nested_at {
        let applies = match name.as_str() {
            "media" => media_matches(prelude, media),
            "supports" => supports(prelude),
            "layer" | "container" | "scope" | "starting-style" => true,
            _ => false,
        };
        if applies {
            style_rule(texts, inner, media, out, depth + 1);
        }
    }
}

/// `@supports` conditions: assume support except for explicit negations of
/// features that are in fact supported
fn supports(prelude: &str) -> bool {
    !prelude.trim_start().to_ascii_lowercase().starts_with("not ")
}

// ---- media queries ----

/// Whether a media query list matches (an empty list matches everything)
pub fn media_matches(query: &str, ctx: &MediaContext) -> bool {
    let q = spaced_keywords(&strip_comments(query).to_ascii_lowercase());
    let q = q.trim();
    if q.is_empty() {
        return true;
    }
    split_top(q, b',').into_iter().any(|one| single_query(one.trim(), ctx))
}

/// `q` with the keywords spaced from the parentheses they touch:
/// minifiers write `(min-width:1069px)and (min-height:776px)` and
/// `screen and(color)`
fn spaced_keywords(q: &str) -> String {
    let b = q.as_bytes();
    let word_at = |i: usize| -> Option<usize> {
        if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'-') {
            return None;
        }
        ["and", "or", "not", "only"]
            .iter()
            .find(|w| b[i..].starts_with(w.as_bytes()) && b.get(i + w.len()).is_none_or(|&c| !(c.is_ascii_alphanumeric() || c == b'-')))
            .map(|w| w.len())
    };
    let mut out = String::with_capacity(q.len() + 8);
    let mut i = 0;
    while i < b.len() {
        match word_at(i) {
            Some(n) => {
                if out.ends_with(')') {
                    out.push(' ');
                }
                out.push_str(&q[i..i + n]);
                if b.get(i + n) == Some(&b'(') {
                    out.push(' ');
                }
                i += n;
            }
            None => {
                let c = q[i..].chars().next().unwrap_or(' ');
                out.push(c);
                i += c.len_utf8();
            }
        }
    }
    out
}

fn single_query(q: &str, ctx: &MediaContext) -> bool {
    let mut q = q;
    let mut negate = false;
    if let Some(rest) = q.strip_prefix("not ") {
        negate = true;
        q = rest.trim_start();
    } else if let Some(rest) = q.strip_prefix("only ") {
        q = rest.trim_start();
    }
    // Conditions joined by `and`; a condition list may also use `or`
    let result = if q.contains(" or ") && !q.contains(" and ") {
        split_words(q, " or ").into_iter().any(|c| condition(c.trim(), ctx))
    } else {
        split_words(q, " and ").into_iter().all(|c| condition(c.trim(), ctx))
    };
    result != negate
}

/// Split at a top-level word separator (not inside parentheses)
fn split_words<'a>(s: &'a str, sep: &str) -> Vec<&'a str> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let (mut start, mut i, mut depth) = (0, 0, 0i32);
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            // Bytes, not str slices: `i` may be inside a multi-byte
            // character (the ASCII separator never is)
            _ if depth == 0 && b[i..].starts_with(sep.as_bytes()) => {
                out.push(&s[start..i]);
                i += sep.len();
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&s[start..]);
    out
}

fn condition(c: &str, ctx: &MediaContext) -> bool {
    if let Some(inner) = c.strip_prefix("not ") {
        return !condition(inner.trim(), ctx);
    }
    if let Some(inner) = c.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        let inner = inner.trim();
        // A nested condition list: ((a) or (b))
        if inner.starts_with('(') || inner.starts_with("not ") {
            return single_query(inner, ctx);
        }
        return feature(inner, ctx);
    }
    // A media type
    matches!(c, "all" | "screen" | "")
}

/// A length in a media query, in CSS pixels
fn media_length(v: &str) -> Option<f32> {
    let v = v.trim();
    let num_end = v.find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+')).unwrap_or(v.len());
    let n: f32 = v[..num_end].parse().ok()?;
    Some(match v[num_end..].trim() {
        "px" | "" => n,
        "em" | "rem" => n * 16.0,
        "pt" => n * 4.0 / 3.0,
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 96.0 / 25.4,
        "in" => n * 96.0,
        _ => return None,
    })
}

fn ratio(v: &str) -> Option<f32> {
    let (a, b) = v.split_once('/').unwrap_or((v, "1"));
    let (a, b): (f32, f32) = (a.trim().parse().ok()?, b.trim().parse().ok()?);
    (b != 0.0).then(|| a / b)
}

fn feature(f: &str, ctx: &MediaContext) -> bool {
    // Range syntax: (width >= 600px), (400px <= width < 800px)
    if f.contains('<') || f.contains('>') || (f.contains('=') && !f.contains(':')) {
        return range_feature(f, ctx);
    }
    let (name, value) = match f.split_once(':') {
        Some((n, v)) => (n.trim(), Some(v.trim())),
        None => (f.trim(), None),
    };
    let name = name.strip_prefix("-webkit-").unwrap_or(name);
    let (w, h) = (ctx.width, ctx.height);
    let len = |v: Option<&str>| {
        v.and_then(|v| if v.contains('(') { crate::values::math_length(v, (w, h)) } else { media_length(v) })
    };
    match (name, value) {
        ("width", Some(v)) => len(Some(v)).is_some_and(|x| (w - x).abs() < 0.5),
        ("min-width", v) => len(v).is_some_and(|x| w >= x),
        ("max-width", v) => len(v).is_some_and(|x| w <= x),
        ("height", Some(v)) => len(Some(v)).is_some_and(|x| (h - x).abs() < 0.5),
        ("min-height", v) => len(v).is_some_and(|x| h >= x),
        ("max-height", v) => len(v).is_some_and(|x| h <= x),
        ("min-device-width", v) => len(v).is_some_and(|x| w >= x),
        ("max-device-width", v) => len(v).is_some_and(|x| w <= x),
        ("orientation", Some(v)) => (v == "landscape") == (w >= h),
        ("aspect-ratio", Some(v)) => ratio(v).is_some_and(|r| (w / h - r).abs() < 0.01),
        ("min-aspect-ratio", Some(v)) => ratio(v).is_some_and(|r| w / h >= r),
        ("max-aspect-ratio", Some(v)) => ratio(v).is_some_and(|r| w / h <= r),
        ("prefers-color-scheme", Some(v)) => v == "light",
        ("prefers-reduced-motion", Some(v)) => v == "no-preference",
        ("prefers-contrast", Some(v)) => v == "no-preference",
        ("prefers-reduced-transparency", Some(v)) => v == "no-preference",
        ("forced-colors", Some(v)) => v == "none",
        ("inverted-colors", Some(v)) => v == "none",
        ("hover" | "any-hover", Some(v)) => v == "hover",
        ("pointer" | "any-pointer", Some(v)) => v == "fine",
        ("scripting", Some(v)) => v == "enabled",
        ("update", Some(v)) => v == "fast",
        ("display-mode", Some(v)) => v == "browser",
        ("color", None) | ("hover" | "any-hover" | "pointer" | "any-pointer", None) => true,
        ("min-color", Some(v)) => v.parse::<u32>().is_ok_and(|n| n <= 8),
        ("monochrome", None) | ("grid", None) => false,
        ("min-resolution", Some(v)) => resolution(v).is_some_and(|r| r <= 1.0),
        ("max-resolution", Some(v)) => resolution(v).is_some_and(|r| r >= 1.0),
        ("min-device-pixel-ratio", Some(v)) => v.parse::<f32>().is_ok_and(|r| r <= 1.0),
        ("max-device-pixel-ratio", Some(v)) => v.parse::<f32>().is_ok_and(|r| r >= 1.0),
        ("width" | "height", None) => true,
        _ => false,
    }
}

/// Resolution in dppx
fn resolution(v: &str) -> Option<f32> {
    let v = v.trim();
    if let Some(n) = v.strip_suffix("dppx").or_else(|| v.strip_suffix('x')) {
        return n.trim().parse().ok();
    }
    if let Some(n) = v.strip_suffix("dpi") {
        return n.trim().parse::<f32>().ok().map(|d| d / 96.0);
    }
    if let Some(n) = v.strip_suffix("dpcm") {
        return n.trim().parse::<f32>().ok().map(|d| d * 2.54 / 96.0);
    }
    None
}

fn range_feature(f: &str, ctx: &MediaContext) -> bool {
    // Tokens: values and comparison operators
    let mut parts: Vec<String> = Vec::new();
    let mut cur = String::new();
    let chars: Vec<char> = f.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<' || c == '>' || c == '=' {
            if !cur.trim().is_empty() {
                parts.push(cur.trim().to_string());
            }
            cur.clear();
            let mut op = c.to_string();
            if chars.get(i + 1) == Some(&'=') && c != '=' {
                op.push('=');
                i += 1;
            }
            parts.push(op);
        } else {
            cur.push(c);
        }
        i += 1;
    }
    if !cur.trim().is_empty() {
        parts.push(cur.trim().to_string());
    }
    let value_of = |t: &str| -> Option<f32> {
        match t {
            "width" => Some(ctx.width),
            "height" => Some(ctx.height),
            "aspect-ratio" => Some(ctx.width / ctx.height),
            _ if t.contains('(') => crate::values::math_length(t, (ctx.width, ctx.height)),
            _ => media_length(t).or_else(|| ratio(t)),
        }
    };
    let cmp = |a: f32, op: &str, b: f32| match op {
        "<" => a < b,
        "<=" => a <= b,
        ">" => a > b,
        ">=" => a >= b,
        "=" => (a - b).abs() < 0.5,
        _ => false,
    };
    match parts.as_slice() {
        [a, op, b] => match (value_of(a), value_of(b)) {
            (Some(x), Some(y)) => cmp(x, op, y),
            _ => false,
        },
        [a, op1, m, op2, b] => match (value_of(a), value_of(m), value_of(b)) {
            (Some(x), Some(y), Some(z)) => cmp(x, op1, y) && cmp(y, op2, z),
            _ => false,
        },
        _ => false,
    }
}

// ---- declarations ----

/// Parse a declaration block (the inside of `{...}`, or a `style`
/// attribute)
pub fn parse_declarations(block: &str) -> Vec<Declaration> {
    let block = strip_comments(block);
    let mut out = Vec::new();
    for chunk in split_top(&block, b';') {
        // Nested rules (CSS nesting) are not supported: skip them
        if chunk.contains('{') {
            continue;
        }
        let Some((name, value)) = chunk.split_once(':') else { continue };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        if name.starts_with("--") {
            // Custom properties: case-sensitive names, values kept as text
            let mut value = value.trim();
            let mut important = false;
            if let Some(bang) = value.rfind('!') {
                if value[bang + 1..].trim().eq_ignore_ascii_case("important") {
                    important = true;
                    value = value[..bang].trim_end();
                }
            }
            out.push(decl(PropertyId::Custom, PropertyValue::Custom(Box::new((name.to_string(), value.to_string()))), important));
            continue;
        }
        let name = name.to_ascii_lowercase();
        let mut value = value.trim();
        let mut important = false;
        if let Some(bang) = value.rfind('!') {
            if value[bang + 1..].trim().eq_ignore_ascii_case("important") {
                important = true;
                value = value[..bang].trim_end();
            }
        }
        if value.is_empty() {
            continue;
        }
        convert(&name, value, important, &mut out);
    }
    out
}

fn decl(property: PropertyId, value: PropertyValue, important: bool) -> Declaration {
    Declaration { property, value, important }
}

/// Whitespace-separated components (keeping functions like rgb(...) whole)
pub(crate) fn components(v: &str) -> Vec<&str> {
    let b = v.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() {
            i = skip_token(b, i);
        }
        if i > start {
            out.push(&v[start..i]);
        }
    }
    out
}

fn global_keyword(v: &str) -> Option<Keyword> {
    match v.to_ascii_lowercase().as_str() {
        "inherit" => Some(Keyword::Inherit),
        "initial" | "revert" | "revert-layer" => Some(Keyword::Initial),
        "unset" => Some(Keyword::Unset),
        _ => None,
    }
}

fn convert(name: &str, value: &str, important: bool, out: &mut Vec<Declaration>) {
    if !crate::longhand::is_known(name) {
        return;
    }
    // Values depending on custom properties or math functions are computed
    // per element, during the cascade
    let lower = value.to_ascii_lowercase();
    let has_var = lower.contains("var(");
    let has_math = ["calc", "min", "max", "clamp"].iter().any(|f| crate::values::has_function(&lower, f));
    // Math over a relative color's channels (`hsl(from red calc(h + 120)
    // s l)`) is the color parser's: such values need no per-element
    // resolution
    if has_math && !has_var && lower.contains("(from ") {
        let before = out.len();
        crate::longhand::expand(name, value, important, out);
        if out.len() > before {
            return;
        }
    }
    if has_var || has_math {
        let id = if name == "font-size" || name == "font" { PropertyId::FontSize } else { PropertyId::Custom };
        out.push(decl(id, PropertyValue::Unresolved(Box::new((name.to_string(), value.to_string()))), important));
        return;
    }
    // `content` takes attr() itself
    if lower.contains("env(") || (lower.contains("attr(") && name != "content") {
        return;
    }
    crate::longhand::expand(name, value, important, out);
}

/// A length, `0`, or (if allowed) `auto`
fn length_or_auto(v: &str, allow_auto: bool) -> Option<PropertyValue> {
    let v = v.trim();
    if allow_auto && v == "auto" {
        return Some(PropertyValue::Keyword(Keyword::Auto));
    }
    if v == "0" || v == "-0" || v == "+0" {
        return Some(PropertyValue::Length(Length::px(0.0)));
    }
    parse_length(v).map(PropertyValue::Length)
}

/// A CSS length with a unit (absolute units become px)
pub fn parse_length(v: &str) -> Option<Length> {
    let v = v.trim();
    let split = v
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0) || ((c == 'e' || c == 'E') && v[i + 1..].starts_with(|d: char| d.is_ascii_digit()))))
        .map_or(v.len(), |(i, _)| i);
    let n: f32 = v[..split].parse().ok()?;
    let (value, unit) = match v[split..].to_ascii_lowercase().as_str() {
        "px" => (n, LengthUnit::Px),
        "em" => (n, LengthUnit::Em),
        "rem" => (n, LengthUnit::Rem),
        "%" => (n, LengthUnit::Percent),
        "vw" => (n, LengthUnit::Vw),
        "vh" => (n, LengthUnit::Vh),
        "vmin" => (n, LengthUnit::Vmin),
        "vmax" => (n, LengthUnit::Vmax),
        "ch" => (n, LengthUnit::Ch),
        "ex" => (n, LengthUnit::Ex),
        "pt" => (n * 4.0 / 3.0, LengthUnit::Px),
        "pc" => (n * 16.0, LengthUnit::Px),
        "in" => (n * 96.0, LengthUnit::Px),
        "cm" => (n * 96.0 / 2.54, LengthUnit::Px),
        "mm" => (n * 96.0 / 25.4, LengthUnit::Px),
        "q" => (n * 96.0 / 101.6, LengthUnit::Px),
        _ => return None,
    };
    Some(Length { value, unit })
}

pub(crate) fn font_size(v: &str) -> Option<PropertyValue> {
    let px = |n: f32| Some(PropertyValue::Length(Length::px(n)));
    match v.trim() {
        "xx-small" => px(9.0),
        "x-small" => px(10.0),
        "small" => px(13.0),
        "medium" => px(16.0),
        "large" => px(18.0),
        "x-large" => px(24.0),
        "xx-large" => px(32.0),
        "xxx-large" => px(48.0),
        "smaller" => Some(PropertyValue::Length(Length { value: 0.833, unit: LengthUnit::Em })),
        "larger" => Some(PropertyValue::Length(Length { value: 1.2, unit: LengthUnit::Em })),
        // A unitless zero is a length (font-size: 0 hides icon buttons' text)
        other if other.parse::<f32>() == Ok(0.0) => px(0.0),
        other => parse_length(other).map(PropertyValue::Length),
    }
}

pub(crate) fn font_weight(v: &str) -> Option<i32> {
    match v.trim() {
        "normal" => Some(400),
        "bold" | "bolder" => Some(700),
        "lighter" => Some(300),
        n => n.parse::<f32>().ok().filter(|w| (1.0..=1000.0).contains(w)).map(|w| w as i32),
    }
}

/// `font: [style] [variant] [weight] [stretch] size[/line-height] family`
fn font_shorthand(v: &str, important: bool, out: &mut Vec<Declaration>) {
    let parts = components(v);
    // System fonts (caption, menu, ...) have no size of their own here
    let Some(size_at) = parts.iter().position(|p| font_size(p.split('/').next().unwrap_or("")).is_some()) else { return };
    let mut weight = 400;
    for p in &parts[..size_at] {
        if let Some(w) = font_weight(p) {
            weight = w;
        }
    }
    let size = parts[size_at].split('/').next().unwrap_or("");
    if let Some(s) = font_size(size) {
        out.push(decl(PropertyId::FontSize, s, important));
    }
    out.push(decl(PropertyId::FontWeight, PropertyValue::Integer(weight), important));
}

// ---- colors ----

/// Parse a CSS color: hex, `rgb()`/`rgba()`, `hsl()`/`hsla()`, or a name
pub fn parse_color(v: &str) -> Option<Color> {
    let v = v.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return match hex.len() {
            4 => {
                let d = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|x| x * 17);
                Some(Color::rgba(d(0)?, d(1)?, d(2)?, d(3)?))
            }
            3 | 6 | 8 => Color::from_hex(hex),
            _ => None,
        };
    }
    let lower = v.to_ascii_lowercase();
    // Pages are shown in their light scheme
    if let Some(args) = lower.strip_prefix("light-dark(").and_then(|r| r.strip_suffix(')')) {
        let parts = split_top(args, b',');
        return match parts.as_slice() {
            [light, _] => parse_color(light),
            _ => None,
        };
    }
    if let Some(args) = lower.strip_prefix("color-mix(").and_then(|r| r.strip_suffix(')')) {
        return color_mix(args);
    }
    if let Some(open) = lower.find('(') {
        let func = &lower[..open];
        let args = lower[open + 1..].strip_suffix(')')?;
        if let Some(rest) = args.trim_start().strip_prefix("from ") {
            return relative_color(func, rest);
        }
        // Comma or space syntax, alpha after `/` or as a 4th value
        let args = args.replace('/', " ").replace(',', " ");
        let nums: Vec<&str> = args.split_whitespace().collect();
        if nums.len() < 3 {
            return None;
        }
        let alpha = match nums.get(3) {
            Some(a) => {
                let x = match a.strip_suffix('%') {
                    Some(p) => p.parse::<f32>().ok()? / 100.0,
                    None => a.parse::<f32>().ok()?,
                };
                (x.clamp(0.0, 1.0) * 255.0).round() as u8
            }
            None => 255,
        };
        return match func {
            "rgb" | "rgba" => {
                let ch = |s: &str| -> Option<u8> {
                    let x = match s.strip_suffix('%') {
                        Some(p) => p.parse::<f32>().ok()? * 2.55,
                        None => s.parse::<f32>().ok()?,
                    };
                    Some(x.clamp(0.0, 255.0).round() as u8)
                };
                Some(Color::rgba(ch(nums[0])?, ch(nums[1])?, ch(nums[2])?, alpha))
            }
            "hsl" | "hsla" => {
                let h = hue(nums[0])?;
                let pct = |s: &str| s.trim_end_matches('%').parse::<f32>().ok().map(|x| (x / 100.0).clamp(0.0, 1.0));
                let (s, l) = (pct(nums[1])?, pct(nums[2])?);
                let (r, g, b) = hsl_to_rgb(h, s, l);
                Some(Color::rgba(r, g, b, alpha))
            }
            "hwb" => {
                let h = hue(nums[0])?;
                let (w, bl) = (component(nums[1], 100.0)? / 100.0, component(nums[2], 100.0)? / 100.0);
                let (r, g, b) = if w + bl >= 1.0 {
                    let gray = (w / (w + bl) * 255.0).round() as u8;
                    (gray, gray, gray)
                } else {
                    let (r, g, b) = hsl_to_rgb(h, 1.0, 0.5);
                    let mix = |c: u8| ((c as f32 / 255.0 * (1.0 - w - bl) + w) * 255.0).round() as u8;
                    (mix(r), mix(g), mix(b))
                };
                Some(Color::rgba(r, g, b, alpha))
            }
            // Perceptual spaces (CSS Color 4), converted to sRGB (clamped)
            "oklab" | "oklch" | "lab" | "lch" => {
                let (lab_l, a, b) = match func {
                    "oklab" => (component(nums[0], 1.0)?, component(nums[1], 0.4)?, component(nums[2], 0.4)?),
                    "oklch" => {
                        let (c, h) = (component(nums[1], 0.4)?, hue(nums[2])?.to_radians());
                        (component(nums[0], 1.0)?, c * h.cos(), c * h.sin())
                    }
                    "lab" => (component(nums[0], 100.0)?, component(nums[1], 125.0)?, component(nums[2], 125.0)?),
                    _ => {
                        let (c, h) = (component(nums[1], 150.0)?, hue(nums[2])?.to_radians());
                        (component(nums[0], 100.0)?, c * h.cos(), c * h.sin())
                    }
                };
                let linear = if func.starts_with("ok") { oklab_to_linear_srgb(lab_l, a, b) } else { lab_to_linear_srgb(lab_l, a, b) };
                let enc = |c: f32| {
                    let c = c.clamp(0.0, 1.0);
                    let v = if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
                    (v * 255.0).round().clamp(0.0, 255.0) as u8
                };
                Some(Color::rgba(enc(linear[0]), enc(linear[1]), enc(linear[2]), alpha))
            }
            _ => None,
        };
    }
    named_color(&lower)
}

/// Relative color syntax (CSS Color 5): `rgb(from <color> r g b / alpha)`
/// and the like for `hsl()` and `hwb()`. Each channel is a number, a
/// percentage, one of the origin's channel keywords, or a `calc()` over
/// them.
fn relative_color(func: &str, rest: &str) -> Option<Color> {
    let tokens: Vec<&str> = split_top(rest, b' ').into_iter().map(str::trim).filter(|t| !t.is_empty()).collect();
    let (origin, rest) = tokens.split_first()?;
    let origin = parse_color(origin)?;
    let (r, g, b) = (origin.r as f32, origin.g as f32, origin.b as f32);
    let alpha = origin.a as f32 / 255.0;
    let (channels, normal): ([(&str, f32); 3], fn(f32, f32, f32, f32) -> String) = match func {
        "rgb" | "rgba" => ([("r", r), ("g", g), ("b", b)], |a, b, c, al| format!("rgb({a} {b} {c} / {al})")),
        "hsl" | "hsla" => {
            let (h, s, l) = rgb_to_hsl(r / 255.0, g / 255.0, b / 255.0);
            ([("h", h), ("s", s * 100.0), ("l", l * 100.0)], |a, b, c, al| format!("hsl({a} {b}% {c}% / {al})"))
        }
        "hwb" => {
            let (h, _, _) = rgb_to_hsl(r / 255.0, g / 255.0, b / 255.0);
            let w = r.min(g).min(b) / 255.0 * 100.0;
            let bl = (1.0 - r.max(g).max(b) / 255.0) * 100.0;
            ([("h", h), ("w", w), ("b", bl)], |a, b, c, al| format!("hwb({a} {b}% {c}% / {al})"))
        }
        _ => return None,
    };
    let vars: Vec<(&str, f32)> = channels.iter().copied().chain(std::iter::once(("alpha", alpha))).collect();
    let value = |t: &str, full: f32| -> Option<f32> {
        if let Some(&(_, v)) = vars.iter().find(|(k, _)| *k == t) {
            return Some(v);
        }
        if let Some(p) = t.strip_suffix('%') {
            return p.parse::<f32>().ok().map(|x| x / 100.0 * full);
        }
        if t == "none" {
            return Some(0.0);
        }
        if let Some(expr) = t.strip_prefix("calc(").and_then(|e| e.strip_suffix(')')) {
            return eval_channel_expr(expr, &vars);
        }
        hue(t)
    };
    let (main, alpha_part) = match rest.iter().position(|t| *t == "/") {
        Some(i) => (&rest[..i], rest.get(i + 1)),
        None => (rest, None),
    };
    let [c1, c2, c3] = main else { return None };
    let a = match alpha_part {
        Some(t) => value(t, 1.0)?,
        None => alpha,
    };
    // hsl/hwb's s, l, w, b resolve to numbers on a 0-100 scale
    let full = if matches!(func, "rgb" | "rgba") { 255.0 } else { 100.0 };
    parse_color(&normal(value(c1, if full == 255.0 { 255.0 } else { 360.0 })?, value(c2, full)?, value(c3, full)?, a.clamp(0.0, 1.0)))
}

/// A `calc()` expression over numbers and channel keywords: `+ - * /`
/// and parentheses
fn eval_channel_expr(expr: &str, vars: &[(&str, f32)]) -> Option<f32> {
    fn atom(s: &[u8], i: &mut usize, vars: &[(&str, f32)]) -> Option<f32> {
        while *i < s.len() && s[*i] == b' ' {
            *i += 1;
        }
        if *i < s.len() && s[*i] == b'(' {
            *i += 1;
            let v = sum(s, i, vars)?;
            while *i < s.len() && s[*i] == b' ' {
                *i += 1;
            }
            if *i < s.len() && s[*i] == b')' {
                *i += 1;
            }
            return Some(v);
        }
        let start = *i;
        while *i < s.len() && (s[*i].is_ascii_alphanumeric() || s[*i] == b'.' || s[*i] == b'%' || (*i == start && s[*i] == b'-')) {
            *i += 1;
        }
        let word = std::str::from_utf8(&s[start..*i]).ok()?;
        vars.iter().find(|(k, _)| *k == word).map(|&(_, v)| v).or_else(|| word.trim_end_matches('%').parse().ok())
    }
    fn product(s: &[u8], i: &mut usize, vars: &[(&str, f32)]) -> Option<f32> {
        let mut v = atom(s, i, vars)?;
        loop {
            while *i < s.len() && s[*i] == b' ' {
                *i += 1;
            }
            match s.get(*i) {
                Some(b'*') => {
                    *i += 1;
                    v *= atom(s, i, vars)?;
                }
                Some(b'/') => {
                    *i += 1;
                    v /= atom(s, i, vars)?;
                }
                _ => return Some(v),
            }
        }
    }
    fn sum(s: &[u8], i: &mut usize, vars: &[(&str, f32)]) -> Option<f32> {
        let mut v = product(s, i, vars)?;
        loop {
            while *i < s.len() && s[*i] == b' ' {
                *i += 1;
            }
            match s.get(*i) {
                Some(b'+') => {
                    *i += 1;
                    v += product(s, i, vars)?;
                }
                Some(b'-') => {
                    *i += 1;
                    v -= product(s, i, vars)?;
                }
                _ => return Some(v),
            }
        }
    }
    let mut i = 0;
    let v = sum(expr.as_bytes(), &mut i, vars)?;
    (i >= expr.trim_end().len()).then_some(v)
}

/// `color-mix(in <space>, <color> [<pct>], <color> [<pct>])`, mixed in
/// OKLab for the OK spaces and sRGB otherwise (premultiplied alpha)
fn color_mix(args: &str) -> Option<Color> {
    let parts = split_top(args, b',');
    let [space, a, b] = parts.as_slice() else { return None };
    let space = space.trim().strip_prefix("in ")?.trim();
    let item = |s: &str| -> Option<(Color, Option<f32>)> {
        let tokens: Vec<&str> = split_top(s.trim(), b' ').into_iter().map(str::trim).filter(|t| !t.is_empty()).collect();
        let pct = tokens.iter().find_map(|t| t.strip_suffix('%').and_then(|p| p.parse::<f32>().ok()).map(|p| p / 100.0));
        let color = tokens.iter().find(|t| !(t.ends_with('%') && t[..t.len() - 1].parse::<f32>().is_ok()))?;
        Some((parse_color(color)?, pct))
    };
    let ((ca, pa), (cb, pb)) = (item(a)?, item(b)?);
    let (pa, pb) = match (pa, pb) {
        (Some(x), Some(y)) => (x, y),
        (Some(x), None) => (x, 1.0 - x),
        (None, Some(y)) => (1.0 - y, y),
        (None, None) => (0.5, 0.5),
    };
    let total = pa + pb;
    if total <= 0.0 {
        return None;
    }
    // Fewer than 100% in all leaves the result that much transparent
    let (wa, wb, scale) = (pa / total, pb / total, total.min(1.0));
    let (aa, ab) = (ca.a as f32 / 255.0, cb.a as f32 / 255.0);
    let alpha = aa * wa + ab * wb;
    if alpha <= 0.0 {
        return Some(Color::rgba(0, 0, 0, 0));
    }
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let enc = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        let v = if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
        (v * 255.0).round().clamp(0.0, 255.0) as u8
    };
    let rgb = if space.starts_with("oklab") || space.starts_with("oklch") {
        let to_lab = |c: Color| linear_srgb_to_oklab(lin(c.r), lin(c.g), lin(c.b));
        let (la, lb) = (to_lab(ca), to_lab(cb));
        let mixed: Vec<f32> = (0..3).map(|i| (la[i] * aa * wa + lb[i] * ab * wb) / alpha).collect();
        let l = oklab_to_linear_srgb(mixed[0], mixed[1], mixed[2]);
        [enc(l[0]), enc(l[1]), enc(l[2])]
    } else {
        let ch = |x: u8, y: u8| ((x as f32 * aa * wa + y as f32 * ab * wb) / alpha).round().clamp(0.0, 255.0) as u8;
        [ch(ca.r, cb.r), ch(ca.g, cb.g), ch(ca.b, cb.b)]
    };
    Some(Color::rgba(rgb[0], rgb[1], rgb[2], (alpha * scale * 255.0).round() as u8))
}

/// sRGB channels (0-1) to hue (degrees), saturation and lightness (0-1)
fn rgb_to_hsl(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h * 60.0, s, l)
}

/// Linear-light sRGB to OKLab
fn linear_srgb_to_oklab(r: f32, g: f32, b: f32) -> [f32; 3] {
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

/// A color component: a number, a percentage of `full`, or `none` (0)
fn component(s: &str, full: f32) -> Option<f32> {
    if s == "none" {
        return Some(0.0);
    }
    match s.strip_suffix('%') {
        Some(p) => p.parse::<f32>().ok().map(|x| x / 100.0 * full),
        None => s.parse().ok(),
    }
}

/// A hue in degrees (deg, rad, grad, turn or a bare number)
fn hue(s: &str) -> Option<f32> {
    if s == "none" {
        return Some(0.0);
    }
    let unit = |suffix: &str, k: f32| s.strip_suffix(suffix).and_then(|n| n.parse::<f32>().ok()).map(|n| n * k);
    unit("grad", 0.9).or_else(|| unit("deg", 1.0)).or_else(|| unit("rad", 180.0 / std::f32::consts::PI)).or_else(|| unit("turn", 360.0)).or_else(|| s.parse().ok())
}

/// OKLab to linear-light sRGB
fn oklab_to_linear_srgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [
        4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_,
        -1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_,
        -0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_,
    ]
}

/// CIE Lab (D50) to linear-light sRGB (D65, Bradford adaptation)
fn lab_to_linear_srgb(l: f32, a: f32, b: f32) -> [f32; 3] {
    const KAPPA: f32 = 24389.0 / 27.0;
    const EPS: f32 = 216.0 / 24389.0;
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 500.0, fy - b / 200.0);
    let inv = |f: f32| if f.powi(3) > EPS { f.powi(3) } else { (116.0 * f - 16.0) / KAPPA };
    let y = if l > KAPPA * EPS { fy.powi(3) } else { l / KAPPA };
    let (x, z) = (inv(fx) * 0.964_22, inv(fz) * 0.825_21);
    let d65 = [
        0.955_473_45 * x - 0.023_098_537 * y + 0.063_259_31 * z,
        -0.028_369_706 * x + 1.009_995_5 * y + 0.021_041_4 * z,
        0.012_314_002 * x - 0.020_507_697 * y + 1.330_366 * z,
    ];
    [
        3.240_97 * d65[0] - 1.537_383_2 * d65[1] - 0.498_610_76 * d65[2],
        -0.969_243_65 * d65[0] + 1.875_967_5 * d65[1] + 0.041_555_06 * d65[2],
        0.055_630_08 * d65[0] - 0.203_976_96 * d65[1] + 1.056_971_5 * d65[2],
    ]
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = h.rem_euclid(360.0) / 360.0;
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let hue = |mut t: f32| {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let c = |x: f32| (x * 255.0).round().clamp(0.0, 255.0) as u8;
    (c(hue(h + 1.0 / 3.0)), c(hue(h)), c(hue(h - 1.0 / 3.0)))
}

/// The CSS named colors
fn named_color(name: &str) -> Option<Color> {
    const NAMED: &[(&str, u32)] = &[
        ("aliceblue", 0xf0f8ff), ("antiquewhite", 0xfaebd7), ("aqua", 0x00ffff), ("aquamarine", 0x7fffd4),
        ("azure", 0xf0ffff), ("beige", 0xf5f5dc), ("bisque", 0xffe4c4), ("black", 0x000000),
        ("blanchedalmond", 0xffebcd), ("blue", 0x0000ff), ("blueviolet", 0x8a2be2), ("brown", 0xa52a2a),
        ("burlywood", 0xdeb887), ("cadetblue", 0x5f9ea0), ("chartreuse", 0x7fff00), ("chocolate", 0xd2691e),
        ("coral", 0xff7f50), ("cornflowerblue", 0x6495ed), ("cornsilk", 0xfff8dc), ("crimson", 0xdc143c),
        ("cyan", 0x00ffff), ("darkblue", 0x00008b), ("darkcyan", 0x008b8b), ("darkgoldenrod", 0xb8860b),
        ("darkgray", 0xa9a9a9), ("darkgreen", 0x006400), ("darkgrey", 0xa9a9a9), ("darkkhaki", 0xbdb76b),
        ("darkmagenta", 0x8b008b), ("darkolivegreen", 0x556b2f), ("darkorange", 0xff8c00), ("darkorchid", 0x9932cc),
        ("darkred", 0x8b0000), ("darksalmon", 0xe9967a), ("darkseagreen", 0x8fbc8f), ("darkslateblue", 0x483d8b),
        ("darkslategray", 0x2f4f4f), ("darkslategrey", 0x2f4f4f), ("darkturquoise", 0x00ced1), ("darkviolet", 0x9400d3),
        ("deeppink", 0xff1493), ("deepskyblue", 0x00bfff), ("dimgray", 0x696969), ("dimgrey", 0x696969),
        ("dodgerblue", 0x1e90ff), ("firebrick", 0xb22222), ("floralwhite", 0xfffaf0), ("forestgreen", 0x228b22),
        ("fuchsia", 0xff00ff), ("gainsboro", 0xdcdcdc), ("ghostwhite", 0xf8f8ff), ("gold", 0xffd700),
        ("goldenrod", 0xdaa520), ("gray", 0x808080), ("green", 0x008000), ("greenyellow", 0xadff2f),
        ("grey", 0x808080), ("honeydew", 0xf0fff0), ("hotpink", 0xff69b4), ("indianred", 0xcd5c5c),
        ("indigo", 0x4b0082), ("ivory", 0xfffff0), ("khaki", 0xf0e68c), ("lavender", 0xe6e6fa),
        ("lavenderblush", 0xfff0f5), ("lawngreen", 0x7cfc00), ("lemonchiffon", 0xfffacd), ("lightblue", 0xadd8e6),
        ("lightcoral", 0xf08080), ("lightcyan", 0xe0ffff), ("lightgoldenrodyellow", 0xfafad2), ("lightgray", 0xd3d3d3),
        ("lightgreen", 0x90ee90), ("lightgrey", 0xd3d3d3), ("lightpink", 0xffb6c1), ("lightsalmon", 0xffa07a),
        ("lightseagreen", 0x20b2aa), ("lightskyblue", 0x87cefa), ("lightslategray", 0x778899), ("lightslategrey", 0x778899),
        ("lightsteelblue", 0xb0c4de), ("lightyellow", 0xffffe0), ("lime", 0x00ff00), ("limegreen", 0x32cd32),
        ("linen", 0xfaf0e6), ("magenta", 0xff00ff), ("maroon", 0x800000), ("mediumaquamarine", 0x66cdaa),
        ("mediumblue", 0x0000cd), ("mediumorchid", 0xba55d3), ("mediumpurple", 0x9370db), ("mediumseagreen", 0x3cb371),
        ("mediumslateblue", 0x7b68ee), ("mediumspringgreen", 0x00fa9a), ("mediumturquoise", 0x48d1cc), ("mediumvioletred", 0xc71585),
        ("midnightblue", 0x191970), ("mintcream", 0xf5fffa), ("mistyrose", 0xffe4e1), ("moccasin", 0xffe4b5),
        ("navajowhite", 0xffdead), ("navy", 0x000080), ("oldlace", 0xfdf5e6), ("olive", 0x808000),
        ("olivedrab", 0x6b8e23), ("orange", 0xffa500), ("orangered", 0xff4500), ("orchid", 0xda70d6),
        ("palegoldenrod", 0xeee8aa), ("palegreen", 0x98fb98), ("paleturquoise", 0xafeeee), ("palevioletred", 0xdb7093),
        ("papayawhip", 0xffefd5), ("peachpuff", 0xffdab9), ("peru", 0xcd853f), ("pink", 0xffc0cb),
        ("plum", 0xdda0dd), ("powderblue", 0xb0e0e6), ("purple", 0x800080), ("rebeccapurple", 0x663399),
        ("red", 0xff0000), ("rosybrown", 0xbc8f8f), ("royalblue", 0x4169e1), ("saddlebrown", 0x8b4513),
        ("salmon", 0xfa8072), ("sandybrown", 0xf4a460), ("seagreen", 0x2e8b57), ("seashell", 0xfff5ee),
        ("sienna", 0xa0522d), ("silver", 0xc0c0c0), ("skyblue", 0x87ceeb), ("slateblue", 0x6a5acd),
        ("slategray", 0x708090), ("slategrey", 0x708090), ("snow", 0xfffafa), ("springgreen", 0x00ff7f),
        ("steelblue", 0x4682b4), ("tan", 0xd2b48c), ("teal", 0x008080), ("thistle", 0xd8bfd8),
        ("tomato", 0xff6347), ("turquoise", 0x40e0d0), ("violet", 0xee82ee), ("wheat", 0xf5deb3),
        ("white", 0xffffff), ("whitesmoke", 0xf5f5f5), ("yellow", 0xffff00), ("yellowgreen", 0x9acd32),
    ];
    if name == "transparent" {
        return Some(Color::TRANSPARENT);
    }
    let &(_, rgb) = NAMED.iter().find(|(n, _)| *n == name)?;
    Some(Color::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(css: &str) -> Stylesheet {
        CssParser::new().parse(css).unwrap()
    }

    fn color_of(css: &str) -> (u8, u8, u8, u8) {
        let ss = parse(css);
        let d = ss.rules[0].declarations.iter().find(|d| d.property == PropertyId::Color).expect("color");
        match d.value {
            PropertyValue::Color(c) => (c.r, c.g, c.b, c.a),
            _ => panic!("not a color"),
        }
    }

    #[test]
    fn font_faces_are_collected() {
        let ctx = MediaContext { width: 800.0, height: 600.0 };
        let css = r#"a { color: red } @font-face { font-family: "Inter"; src: local(Inter), url(/f/inter.woff2) format("woff2"), url('/f/inter.woff') format('woff'); font-weight: 100 900; font-style: normal }
            @media (min-width: 100px) { @font-face { font-family: Icons; src: url(i.ttf); font-style: italic; font-weight: bold } }
            @media (max-width: 10px) { @font-face { font-family: Hidden; src: url(h.ttf) } }
            @font-face { font-family: NoSrc }"#;
        let faces = font_faces(css, &ctx);
        assert_eq!(faces.len(), 2);
        assert_eq!(faces[0].family, "Inter");
        assert_eq!(faces[0].src, vec![("/f/inter.woff2".to_string(), Some("woff2".to_string())), ("/f/inter.woff".to_string(), Some("woff".to_string()))]);
        assert_eq!(faces[0].weight, (100, 900));
        assert_eq!((faces[1].family.as_str(), faces[1].italic, faces[1].weight), ("Icons", true, (700, 700)));
        let f = &font_faces("@font-face { font-family: L; src: url(l.woff2); unicode-range: U+0000-00FF, U+0131, U+20?? }", &ctx)[0];
        assert!(f.covers('a') && f.covers('\u{0131}') && f.covers('\u{20AC}') && !f.covers('\u{0400}'));
    }

    #[test]
    fn light_dark_picks_the_light_color() {
        assert_eq!(parse_color("light-dark(#fff, #000)").map(|c| (c.r, c.a)), Some((255, 255)));
        assert_eq!(parse_color("light-dark(rgb(1, 2, 3), black)").map(|c| c.b), Some(3));
        assert!(parse_color("light-dark(red)").is_none());
    }

    #[test]
    fn media_queries_with_non_ascii_text() {
        let ctx = MediaContext { width: 800.0, height: 600.0 };
        // Malformed, but must not panic
        for q in ["\u{fffd}", "é and (min-width: 1px)", "screen and (wïdth: 5px)", "(min-width: 1px) or ✓"] {
            let _ = media_matches(q, &ctx);
        }
        assert!(media_matches("screen and (min-width: 100px)", &ctx));
    }

    #[test]
    fn minified_media_queries() {
        // Keywords may touch the parentheses around them
        let ctx = MediaContext { width: 1280.0, height: 800.0 };
        let cases = [
            ("(min-width:1069px)and (min-height:776px)", true),
            ("(min-width:1069px)and(min-height:900px)", false),
            ("screen and(min-width:1000px)", true),
            ("not all and(max-width:1000px)", true),
            ("(max-width:100px)or (orientation:landscape)", true),
            ("(orientation:landscape)", true),
        ];
        for (q, want) in cases {
            assert_eq!(media_matches(q, &ctx), want, "{q}");
        }
    }

    #[test]
    fn test_parse_simple() {
        let ss = parse(".foo { display: block; }\n#bar { color: red; }");
        assert_eq!(ss.len(), 2);
        assert_eq!(ss.rules[0].selectors[0].text, ".foo");
        assert_eq!(ss.rules[1].selectors[0].specificity, Specificity(1, 0, 0));
    }

    #[test]
    fn selectors_and_specificity() {
        let ss = parse("div.a > p:first-child, ul li a:hover, #x::before { color: red }");
        let texts: Vec<_> = ss.rules[0].selectors.iter().map(|s| (s.text.as_str(), s.specificity)).collect();
        assert_eq!(
            texts,
            vec![("div.a > p:first-child", Specificity(0, 2, 2)), ("ul li a:hover", Specificity(0, 1, 3)), ("#x::before", Specificity(1, 0, 1))]
        );
    }

    #[test]
    fn colors() {
        assert_eq!(color_of("a { color: #f00 }"), (255, 0, 0, 255));
        assert_eq!(color_of("a { color: #11223344 }"), (0x11, 0x22, 0x33, 0x44));
        assert_eq!(color_of("a { color: rgb(1, 2, 3) }"), (1, 2, 3, 255));
        assert_eq!(color_of("a { color: rgba(10,20,30,.5) }"), (10, 20, 30, 128));
        assert_eq!(color_of("a { color: rgb(100% 0% 0% / 25%) }"), (255, 0, 0, 64));
        assert_eq!(color_of("a { color: hsl(120, 100%, 50%) }"), (0, 255, 0, 255));
        assert_eq!(color_of("a { color: RebeccaPurple }"), (0x66, 0x33, 0x99, 255));
        // CSS Color 4 spaces: sRGB red in each, within rounding
        let near = |css: &str, want: (u8, u8, u8, u8)| {
            let got = color_of(&format!("a {{ color: {css} }}"));
            let d = |a: u8, b: u8| (a as i32 - b as i32).abs();
            assert!(d(got.0, want.0) <= 2 && d(got.1, want.1) <= 2 && d(got.2, want.2) <= 2 && got.3 == want.3, "{css}: {got:?}");
        };
        near("oklch(62.8% 0.2577 29.23deg)", (255, 0, 0, 255));
        near("oklab(0.628 0.2249 0.1258)", (255, 0, 0, 255));
        near("lab(54.29% 80.8 69.89)", (255, 0, 0, 255));
        near("lch(54.29 106.84 40.85)", (255, 0, 0, 255));
        near("hwb(0 0% 0%)", (255, 0, 0, 255));
        near("hwb(0 50% 50%)", (128, 128, 128, 255));
        near("oklch(100% 0 0)", (255, 255, 255, 255));
        // MDN's translucent overlay
        near("oklch(0% 0 0deg/6%)", (0, 0, 0, 15));
        // Relative colors (CSS Color 5): channels from an origin color
        near("hsl(from #000000 h s l / 0.8)", (0, 0, 0, 204));
        near("rgb(from #ff8000 r g b / 50%)", (255, 128, 0, 128));
        near("rgb(from red b g r)", (0, 0, 255, 255));
        // (calc() over channels: resolved per element, see values.rs)
        let rgb = |c: &str| parse_color(c).map(|c| (c.r, c.g, c.b, c.a));
        assert_eq!(rgb("hsl(from #ff0000 calc(h + 120) s l)"), Some((0, 255, 0, 255)));
        assert_eq!(rgb("hsl(from rgb(0 0 255) h s calc(l / 2))"), Some((0, 0, 128, 255)));
        near("hwb(from #808080 h w b / alpha)", (128, 128, 128, 255));
        // color-mix()
        near("color-mix(in srgb, #ff0000, #0000ff)", (128, 0, 128, 255));
        near("color-mix(in srgb, red 25%, blue)", (64, 0, 191, 255));
        near("color-mix(in srgb, red 20%, blue 30%)", (102, 0, 153, 128));
        near("color-mix(in oklab, white, black 0%)", (255, 255, 255, 255));
        near("color-mix(in oklab, #cd491c, black 5%)", (191, 68, 25, 255));
    }

    #[test]
    fn nested_rules() {
        let css = "#nav { color: red; #logo { float: left; & a { display: block } } & ul, ol { margin: 0 } &:hover { color: blue } @media (min-width: 10px) { width: 5px } padding: 1px; } a, b { & > i { top: 0 } }";
        let ss = parse(css);
        let rules: Vec<(String, usize)> = ss.rules.iter().map(|r| (r.selectors.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(", "), r.declarations.len())).collect();
        assert_eq!(
            rules,
            vec![
                // The parent keeps declarations after nested rules too
                // (color, and padding's four longhands)
                ("#nav".to_string(), 5),
                ("#nav #logo".to_string(), 1),
                ("#nav #logo a".to_string(), 1),
                ("#nav ul, #nav ol".to_string(), 4),
                ("#nav:hover".to_string(), 1),
                ("#nav".to_string(), 1),
                ("a > i, b > i".to_string(), 1),
            ]
        );
    }

    #[test]
    fn declarations_and_shorthands() {
        let ss = parse("p { font: italic bold 12pt/1.5 Georgia, serif; margin: 1em auto; padding: 0 !important; background: url(x.png) no-repeat #fff }");
        let d = &ss.rules[0].declarations;
        let size = d.iter().find(|d| d.property == PropertyId::FontSize).unwrap();
        assert!(matches!(size.value, PropertyValue::Length(Length { value, unit: LengthUnit::Px }) if (value - 16.0).abs() < 0.01));
        assert!(d.iter().any(|d| d.property == PropertyId::FontWeight && matches!(d.value, PropertyValue::Integer(700))));
        // Shorthands expand into longhands
        assert!(d.iter().any(|d| d.property == PropertyId::MarginTop && matches!(d.value, PropertyValue::Length(Length { value, unit: LengthUnit::Em }) if value == 1.0)));
        assert!(d.iter().any(|d| d.property == PropertyId::MarginRight && matches!(d.value, PropertyValue::Keyword(Keyword::Auto))));
        assert_eq!(d.iter().filter(|d| d.property == PropertyId::PaddingLeft && d.important).count(), 1);
        assert!(d.iter().any(|d| d.property == PropertyId::BackgroundColor && matches!(d.value, PropertyValue::Color(c) if c.r == 255)));
        // Logical inset shorthands (the full-bleed trick: inset-inline: 50%)
        let ss = parse("p { inset-inline: 50%; inset-block: auto -3px }");
        let d = &ss.rules[0].declarations;
        let pct = |id: PropertyId| d.iter().any(|d| d.property == id && matches!(d.value, PropertyValue::Length(Length { value, unit: LengthUnit::Percent }) if value == 50.0));
        assert!(pct(PropertyId::Left) && pct(PropertyId::Right));
        assert!(d.iter().any(|d| d.property == PropertyId::Top && matches!(d.value, PropertyValue::Keyword(Keyword::Auto))));
        assert!(d.iter().any(|d| d.property == PropertyId::Bottom && matches!(d.value, PropertyValue::Length(Length { value, .. }) if value == -3.0)));
    }

    #[test]
    fn error_recovery() {
        let ss = parse("a { color: red; ; : bad; color } }} b { color: blue } @import 'x.css'; @font-face { font-family: X } c..x { color: red } d { color: green }");
        let texts: Vec<_> = ss.rules.iter().map(|r| r.selectors[0].text.as_str()).collect();
        assert_eq!(texts, vec!["a", "b", "d"]);
    }

    #[test]
    fn media_queries() {
        let ctx = MediaContext { width: 800.0, height: 600.0 };
        let on = |q: &str| media_matches(q, &ctx);
        assert!(on("screen"));
        assert!(on("all and (min-width: 600px)"));
        assert!(!on("(max-width: 600px)"));
        // Math functions (Wikipedia's breakpoints)
        assert!(on("screen and (max-width: calc(1120px - 1px))"));
        assert!(!on("(max-width: calc(640px - 1px))"));
        assert!(on("(min-width: calc(40em + 1px))"));
        assert!(on("(width < calc(1120px - 1px))"));
        assert!(on("print, (orientation: landscape)"));
        assert!(!on("print"));
        assert!(on("not print"));
        assert!(on("(width >= 700px)"));
        assert!(on("(600px < width <= 800px)"));
        assert!(!on("(width < 50em)"));
        assert!(on("only screen and (min-width: 40em) and (prefers-color-scheme: light)"));
        assert!(!on("(prefers-color-scheme: dark)"));
        assert!(on("(min-resolution: 1dppx)"));
        let css = "a { color: red } @media (max-width: 500px) { a { color: blue } } @media (min-width: 500px) { b { color: green } }";
        let ss = CssParser::with_media(ctx).parse(css).unwrap();
        let texts: Vec<_> = ss.rules.iter().map(|r| r.selectors[0].text.as_str()).collect();
        assert_eq!(texts, vec!["a", "b"]);
    }

    #[test]
    fn inline_style() {
        let d = parse_declarations("color: blue; font-size: 2em; display: none");
        assert_eq!(d.len(), 3);
        let d = parse_declarations("width: calc(100% - 2px); color: var(--x); --Brand: #fff !important");
        assert!(matches!(&d[0].value, PropertyValue::Unresolved(b) if b.0 == "width"));
        assert!(matches!(&d[1].value, PropertyValue::Unresolved(b) if b.1 == "var(--x)"));
        assert!(matches!(&d[2].value, PropertyValue::Custom(b) if b.0 == "--Brand" && b.1 == "#fff") && d[2].important);
    }
}
