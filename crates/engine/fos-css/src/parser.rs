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
        parse_rules(css, &self.media, &mut out.rules);
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

// ---- rules ----

fn parse_rules(css: &str, media: &MediaContext, out: &mut Vec<Rule>) {
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
                "media" if media_matches(prelude, media) => parse_rules(block, media, out),
                "supports" if supports(prelude) => parse_rules(block, media, out),
                "layer" | "container" | "scope" | "document" | "-moz-document" | "starting-style" => parse_rules(block, media, out),
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
        let selectors: Vec<Selector> = split_top(&prelude, b',')
            .into_iter()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .filter_map(|text| {
                let parsed = fos_dom::SelectorList::parse(text)?;
                let (a, b, c) = *parsed.specificities().first()?;
                Some(Selector { text: text.to_string(), specificity: Specificity(a, b, c), parts: Vec::new(), parsed: Some(parsed) })
            })
            .collect();
        // Rules are kept even when none of their declarations is modeled
        // (the stylesheet mirrors the source); matching skips them
        if !selectors.is_empty() {
            let mut declarations = parse_declarations(block);
            declarations.shrink_to_fit();
            let mut selectors = selectors;
            selectors.shrink_to_fit();
            out.push(Rule { selectors, declarations });
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
    let q = strip_comments(query).to_ascii_lowercase();
    let q = q.trim();
    if q.is_empty() {
        return true;
    }
    split_top(q, b',').into_iter().any(|one| single_query(one.trim(), ctx))
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
    let len = |v: Option<&str>| v.and_then(media_length);
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
    if lower.contains("var(") || ["calc", "min", "max", "clamp"].iter().any(|f| crate::values::has_function(&lower, f)) {
        let id = if name == "font-size" || name == "font" { PropertyId::FontSize } else { PropertyId::Custom };
        out.push(decl(id, PropertyValue::Unresolved(Box::new((name.to_string(), value.to_string()))), important));
        return;
    }
    if lower.contains("env(") || lower.contains("attr(") {
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
    if let Some(open) = lower.find('(') {
        let func = &lower[..open];
        let args = lower[open + 1..].strip_suffix(')')?;
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
                let h = nums[0].trim_end_matches("deg").parse::<f32>().ok()?;
                let pct = |s: &str| s.trim_end_matches('%').parse::<f32>().ok().map(|x| (x / 100.0).clamp(0.0, 1.0));
                let (s, l) = (pct(nums[1])?, pct(nums[2])?);
                let (r, g, b) = hsl_to_rgb(h, s, l);
                Some(Color::rgba(r, g, b, alpha))
            }
            _ => None,
        };
    }
    named_color(&lower)
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
    fn media_queries_with_non_ascii_text() {
        let ctx = MediaContext { width: 800.0, height: 600.0 };
        // Malformed, but must not panic
        for q in ["\u{fffd}", "é and (min-width: 1px)", "screen and (wïdth: 5px)", "(min-width: 1px) or ✓"] {
            let _ = media_matches(q, &ctx);
        }
        assert!(media_matches("screen and (min-width: 100px)", &ctx));
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
