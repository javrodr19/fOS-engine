//! CSS Grid properties (CSS Grid Layout 1/2): track lists with line
//! names, `repeat()` (including auto-fill/auto-fit), `minmax()` and
//! `fit-content()`; template areas; placement lines; auto flow.
//!
//! Values are validated when the stylesheet is parsed and kept as text;
//! the cascade parses them with the element's lengths context (`em`).

use std::sync::Arc;

use crate::style::Lp;

/// One side of a track's sizing function
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Breadth {
    Length(Lp),
    Fr(f32),
    Auto,
    MinContent,
    MaxContent,
}

impl Breadth {
    pub fn is_intrinsic(self) -> bool {
        matches!(self, Breadth::Auto | Breadth::MinContent | Breadth::MaxContent)
    }
}

/// A track sizing function
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackSize {
    pub min: Breadth,
    pub max: Breadth,
    /// `fit-content(limit)`: max-content, but at most the limit
    pub fit_content: Option<Lp>,
}

impl TrackSize {
    pub const AUTO: TrackSize = TrackSize { min: Breadth::Auto, max: Breadth::Auto, fit_content: None };
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RepeatCount {
    Count(u32),
    AutoFill,
    AutoFit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TemplateItem {
    Track(TrackSize),
    /// `[name ...]`: names of the line at this point
    Names(Arc<[Arc<str>]>),
    Repeat(RepeatCount, Arc<[TemplateItem]>),
}

/// `grid-template-areas`: each named area's rows and columns (end
/// exclusive, 0-based)
#[derive(Clone, Debug, PartialEq)]
pub struct Areas {
    pub rows: u32,
    pub columns: u32,
    pub areas: Arc<[(Arc<str>, u32, u32, u32, u32)]>,
}

/// One end of an item's placement in an axis
#[derive(Clone, Debug, Default, PartialEq)]
pub enum GridLine {
    #[default]
    Auto,
    /// A line number (negative counts from the end), optionally the nth
    /// line with a name
    Line(i32, Option<Arc<str>>),
    Span(u32, Option<Arc<str>>),
    /// An area or line name
    Ident(Arc<str>),
}

/// Grid properties of an element
#[derive(Clone, Debug, PartialEq)]
pub struct GridStyle {
    pub template_columns: Arc<[TemplateItem]>,
    pub template_rows: Arc<[TemplateItem]>,
    pub areas: Option<Arc<Areas>>,
    pub auto_columns: Arc<[TrackSize]>,
    pub auto_rows: Arc<[TrackSize]>,
    /// Auto placement fills columns first
    pub flow_column: bool,
    pub flow_dense: bool,
    pub column_start: GridLine,
    pub column_end: GridLine,
    pub row_start: GridLine,
    pub row_end: GridLine,
    /// `justify-items` / `justify-self` as `AlignItems` / `AlignSelf`
    /// discriminants
    pub justify_items: u8,
    pub justify_self: u8,
}

impl Default for GridStyle {
    fn default() -> Self {
        GridStyle {
            template_columns: Arc::from([]),
            template_rows: Arc::from([]),
            areas: None,
            auto_columns: Arc::from([TrackSize::AUTO]),
            auto_rows: Arc::from([TrackSize::AUTO]),
            flow_column: false,
            flow_dense: false,
            column_start: GridLine::Auto,
            column_end: GridLine::Auto,
            row_start: GridLine::Auto,
            row_end: GridLine::Auto,
            justify_items: crate::style::AlignItems::Normal as u8,
            justify_self: crate::style::AlignSelf::Auto as u8,
        }
    }
}

/// Tokens of a grid value: `[names]`, functions with their arguments,
/// strings, `/`, and plain words
fn tokens(v: &str) -> Vec<&str> {
    let b = v.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        match b[i] {
            b'[' => {
                while i < b.len() && b[i] != b']' {
                    i += 1;
                }
                i = (i + 1).min(b.len());
            }
            b'"' | b'\'' => {
                let q = b[i];
                i += 1;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                i = (i + 1).min(b.len());
            }
            b'/' => i += 1,
            _ => {
                let mut depth = 0;
                while i < b.len() && (depth > 0 || !(b[i].is_ascii_whitespace() || b[i] == b'/' || b[i] == b'[')) {
                    match b[i] {
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
            }
        }
        out.push(&v[start..i]);
    }
    out
}

/// Split `f(a, b)` arguments at top-level commas
fn args(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0, 0);
    for (i, c) in inner.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(inner[start..].trim());
    out
}

fn function<'a>(t: &'a str, name: &str) -> Option<&'a str> {
    let rest = t.strip_prefix(name)?.strip_prefix('(')?;
    rest.strip_suffix(')')
}

/// Lengths to pixels, given the element's context
pub type LengthFn<'a> = &'a dyn Fn(&str) -> Option<Lp>;

fn breadth(t: &str, len: LengthFn) -> Option<Breadth> {
    Some(match t {
        "auto" => Breadth::Auto,
        "min-content" => Breadth::MinContent,
        "max-content" => Breadth::MaxContent,
        _ => {
            if let Some(n) = t.strip_suffix("fr") {
                let f = n.parse::<f32>().ok().filter(|f| *f >= 0.0)?;
                return Some(Breadth::Fr(f));
            }
            let l = len(t)?;
            if l.pct < 0.0 || (l.pct == 0.0 && l.px < 0.0) {
                return None;
            }
            Breadth::Length(l)
        }
    })
}

fn track_size(t: &str, len: LengthFn) -> Option<TrackSize> {
    if let Some(inner) = function(t, "minmax") {
        let a = args(inner);
        let [min, max] = a.as_slice() else { return None };
        let min = breadth(min, len)?;
        if matches!(min, Breadth::Fr(_)) {
            return None;
        }
        return Some(TrackSize { min, max: breadth(max, len)?, fit_content: None });
    }
    if let Some(inner) = function(t, "fit-content") {
        let l = len(inner.trim())?;
        return Some(TrackSize { min: Breadth::Auto, max: Breadth::MaxContent, fit_content: Some(l) });
    }
    let b = breadth(t, len)?;
    Some(match b {
        // A flexible track's minimum is auto
        Breadth::Fr(_) => TrackSize { min: Breadth::Auto, max: b, fit_content: None },
        _ => TrackSize { min: b, max: b, fit_content: None },
    })
}

fn names(t: &str) -> Option<Arc<[Arc<str>]>> {
    let inner = t.strip_prefix('[')?.strip_suffix(']')?;
    Some(inner.split_whitespace().map(Arc::from).collect())
}

/// A track list (`grid-template-columns`/`-rows`); `none` is empty
pub fn parse_track_list(v: &str, len: LengthFn) -> Option<Arc<[TemplateItem]>> {
    let v = v.trim();
    if v == "none" {
        return Some(Arc::from([]));
    }
    let items = track_items(v, len, true)?;
    items.iter().any(|i| !matches!(i, TemplateItem::Names(_))).then(|| Arc::from(items))
}

fn track_items(v: &str, len: LengthFn, allow_repeat: bool) -> Option<Vec<TemplateItem>> {
    let mut out = Vec::new();
    for t in tokens(v) {
        if t.starts_with('[') {
            out.push(TemplateItem::Names(names(t)?));
        } else if let Some(inner) = function(t, "repeat") {
            if !allow_repeat {
                return None;
            }
            let (count, rest) = inner.split_once(',')?;
            let count = match count.trim() {
                "auto-fill" => RepeatCount::AutoFill,
                "auto-fit" => RepeatCount::AutoFit,
                n => RepeatCount::Count(n.parse::<u32>().ok().filter(|n| *n >= 1)?.min(10_000)),
            };
            let inner_items = track_items(rest, len, false)?;
            if !inner_items.iter().any(|i| matches!(i, TemplateItem::Track(_))) {
                return None;
            }
            out.push(TemplateItem::Repeat(count, Arc::from(inner_items)));
        } else {
            out.push(TemplateItem::Track(track_size(t, len)?));
        }
    }
    Some(out)
}

/// Implicit track sizes (`grid-auto-columns`/`-rows`)
pub fn parse_auto_tracks(v: &str, len: LengthFn) -> Option<Arc<[TrackSize]>> {
    let sizes: Option<Vec<TrackSize>> = tokens(v).into_iter().map(|t| track_size(t, len)).collect();
    sizes.filter(|s| !s.is_empty()).map(Arc::from)
}

/// `grid-template-areas` strings (rectangular areas only)
pub fn parse_areas(v: &str) -> Option<Option<Arc<Areas>>> {
    let v = v.trim();
    if v == "none" {
        return Some(None);
    }
    let rows: Vec<Vec<&str>> = tokens(v)
        .into_iter()
        .map(|t| t.strip_prefix('"').and_then(|t| t.strip_suffix('"')).or_else(|| t.strip_prefix('\'').and_then(|t| t.strip_suffix('\''))).map(|s| s.split_whitespace().collect()))
        .collect::<Option<_>>()?;
    let columns = rows.first()?.len();
    if columns == 0 || rows.iter().any(|r| r.len() != columns) {
        return None;
    }
    let mut areas: Vec<(Arc<str>, u32, u32, u32, u32)> = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, &cell) in row.iter().enumerate() {
            if cell.chars().all(|ch| ch == '.') {
                continue;
            }
            match areas.iter_mut().find(|a| &*a.0 == cell) {
                Some(a) => {
                    a.1 = a.1.min(r as u32);
                    a.2 = a.2.min(c as u32);
                    a.3 = a.3.max(r as u32 + 1);
                    a.4 = a.4.max(c as u32 + 1);
                }
                None => areas.push((Arc::from(cell), r as u32, c as u32, r as u32 + 1, c as u32 + 1)),
            }
        }
    }
    // Every area must be a filled rectangle
    for a in &areas {
        for r in a.1..a.3 {
            for c in a.2..a.4 {
                if rows[r as usize][c as usize] != &*a.0 {
                    return None;
                }
            }
        }
    }
    Some(Some(Arc::new(Areas { rows: rows.len() as u32, columns: columns as u32, areas: Arc::from(areas) })))
}

/// A placement line (`grid-column-start`, ...)
pub fn parse_line(v: &str) -> Option<GridLine> {
    let parts: Vec<&str> = v.split_whitespace().collect();
    match parts.as_slice() {
        ["auto"] => Some(GridLine::Auto),
        [p] => match p.parse::<i32>() {
            Ok(0) => None,
            Ok(n) => Some(GridLine::Line(n, None)),
            Err(_) if is_ident(p) => Some(GridLine::Ident(Arc::from(*p))),
            Err(_) => None,
        },
        _ => {
            let span = parts.contains(&"span");
            let num = parts.iter().find_map(|p| p.parse::<i32>().ok());
            let name = parts.iter().find(|p| **p != "span" && p.parse::<i32>().is_err()).filter(|p| is_ident(p)).map(|p| Arc::from(*p));
            if parts.len() > 3 || num == Some(0) {
                return None;
            }
            if span {
                let n = num.unwrap_or(1);
                (n > 0).then(|| GridLine::Span(n as u32, name))
            } else {
                num.map(|n| GridLine::Line(n, name))
            }
        }
    }
}

fn is_ident(s: &str) -> bool {
    !s.is_empty() && !matches!(s, "span" | "auto") && s.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '-') && s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-')
}

/// `grid-auto-flow`: (column, dense)
pub fn parse_flow(v: &str) -> Option<(bool, bool)> {
    let mut column = false;
    let mut dense = false;
    let mut any = false;
    for p in v.split_whitespace() {
        match p {
            "row" => {}
            "column" => column = true,
            "dense" => dense = true,
            _ => return None,
        }
        any = true;
    }
    any.then_some((column, dense))
}

/// Lengths for validation at parse time (any context will do)
pub(crate) fn validation_length(t: &str) -> Option<Lp> {
    if t.contains('(') || t.trim_start_matches(['+', '-']).trim_start_matches('0').trim_start_matches('.').trim_start_matches('0').is_empty() {
        // calc() and friends (resolved later), or a unitless zero
        return Some(Lp::ZERO);
    }
    let l = crate::parser::parse_length(t)?;
    Some(match l.unit {
        crate::properties::LengthUnit::Percent => Lp::pct(l.value),
        _ => Lp::px(l.value),
    })
}

/// Expand a shorthand (`grid-area`, `grid-row`, `grid-column`,
/// `grid-template`, `grid`) into `(longhand, value)` texts
pub(crate) fn expand_shorthand(name: &str, v: &str) -> Option<Vec<(&'static str, String)>> {
    let parts: Vec<String> = v.split('/').map(|p| p.trim().to_string()).collect();
    let other = |p: &str| if is_ident(p) { p.to_string() } else { "auto".to_string() };
    match name {
        "grid-row" | "grid-column" => {
            let (s, e) = if name == "grid-row" { ("grid-row-start", "grid-row-end") } else { ("grid-column-start", "grid-column-end") };
            let start = parts.first()?.clone();
            let end = parts.get(1).cloned().unwrap_or_else(|| other(&start));
            if parts.len() > 2 || parse_line(&start).is_none() || parse_line(&end).is_none() {
                return None;
            }
            Some(vec![(s, start), (e, end)])
        }
        "grid-area" => {
            if parts.len() > 4 || parts.iter().any(|p| parse_line(p).is_none()) {
                return None;
            }
            let rs = parts[0].clone();
            let cs = parts.get(1).cloned().unwrap_or_else(|| other(&rs));
            let re = parts.get(2).cloned().unwrap_or_else(|| other(&rs));
            let ce = parts.get(3).cloned().unwrap_or_else(|| other(&cs));
            Some(vec![("grid-row-start", rs), ("grid-column-start", cs), ("grid-row-end", re), ("grid-column-end", ce)])
        }
        "grid-template" | "grid" => {
            let v = v.trim();
            if v == "none" {
                let mut out = vec![("grid-template-rows", "none".into()), ("grid-template-columns", "none".into()), ("grid-template-areas", "none".into())];
                if name == "grid" {
                    out.extend([("grid-auto-flow", "row".into()), ("grid-auto-rows", "auto".into()), ("grid-auto-columns", "auto".into())]);
                }
                return Some(out);
            }
            if name == "grid" && v.contains("auto-flow") {
                // grid: auto-flow [dense] <auto-rows>? / <columns>
                //  or grid: <rows> / auto-flow [dense] <auto-columns>?
                let [a, b] = parts.as_slice() else { return None };
                let flow_side = |s: &str| -> Option<(bool, String)> {
                    let words: Vec<&str> = s.split_whitespace().collect();
                    if !words.contains(&"auto-flow") {
                        return None;
                    }
                    let dense = words.contains(&"dense");
                    let rest: Vec<&str> = words.into_iter().filter(|w| *w != "auto-flow" && *w != "dense").collect();
                    Some((dense, if rest.is_empty() { "auto".into() } else { rest.join(" ") }))
                };
                if let Some((dense, auto)) = flow_side(a) {
                    return Some(vec![
                        ("grid-auto-flow", if dense { "row dense" } else { "row" }.into()),
                        ("grid-auto-rows", auto),
                        ("grid-template-columns", b.clone()),
                        ("grid-template-rows", "none".into()),
                        ("grid-template-areas", "none".into()),
                    ]);
                }
                let (dense, auto) = flow_side(b)?;
                return Some(vec![
                    ("grid-auto-flow", if dense { "column dense" } else { "column" }.into()),
                    ("grid-auto-columns", auto),
                    ("grid-template-rows", a.clone()),
                    ("grid-template-columns", "none".into()),
                    ("grid-template-areas", "none".into()),
                ]);
            }
            if v.contains('"') || v.contains('\'') {
                // Areas, each string optionally followed by its row size
                let (rows_part, cols) = match parts.as_slice() {
                    [r] => (r.clone(), "none".to_string()),
                    [r, c] => (r.clone(), c.clone()),
                    _ => return None,
                };
                let mut areas = Vec::new();
                let mut sizes = Vec::new();
                let mut pending_size = false;
                for t in tokens(&rows_part) {
                    if t.starts_with('"') || t.starts_with('\'') {
                        if pending_size {
                            sizes.push("auto".to_string());
                        }
                        areas.push(t.to_string());
                        pending_size = true;
                    } else if t.starts_with('[') {
                        continue;
                    } else if pending_size {
                        sizes.push(t.to_string());
                        pending_size = false;
                    } else {
                        return None;
                    }
                }
                if pending_size {
                    sizes.push("auto".to_string());
                }
                let mut out = vec![("grid-template-areas", areas.join(" ")), ("grid-template-rows", sizes.join(" ")), ("grid-template-columns", cols)];
                if name == "grid" {
                    out.extend([("grid-auto-flow", "row".into()), ("grid-auto-rows", "auto".into()), ("grid-auto-columns", "auto".into())]);
                }
                return Some(out);
            }
            let [r, c] = parts.as_slice() else { return None };
            let mut out = vec![("grid-template-rows", r.clone()), ("grid-template-columns", c.clone()), ("grid-template-areas", "none".into())];
            if name == "grid" {
                out.extend([("grid-auto-flow", "row".into()), ("grid-auto-rows", "auto".into()), ("grid-auto-columns", "auto".into())]);
            }
            Some(out)
        }
        _ => None,
    }
}

/// Validate a grid longhand's text at parse time
pub(crate) fn valid(name: &str, v: &str) -> bool {
    let len: LengthFn = &validation_length;
    match name {
        "grid-template-columns" | "grid-template-rows" => parse_track_list(v, len).is_some(),
        "grid-auto-columns" | "grid-auto-rows" => parse_auto_tracks(v, len).is_some(),
        "grid-template-areas" => parse_areas(v).is_some(),
        "grid-auto-flow" => parse_flow(v).is_some(),
        "grid-row-start" | "grid-row-end" | "grid-column-start" | "grid-column-end" => parse_line(v).is_some(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn len(t: &str) -> Option<Lp> {
        validation_length(t)
    }

    #[test]
    fn track_lists() {
        let l = parse_track_list("[full-start] minmax(1em, 1fr) [main] repeat(3, 100px 2fr) fit-content(40%) auto", &len).unwrap();
        assert_eq!(l.len(), 6);
        assert!(matches!(&l[0], TemplateItem::Names(n) if &*n[0] == "full-start"));
        assert!(matches!(l[1], TemplateItem::Track(TrackSize { min: Breadth::Length(_), max: Breadth::Fr(f), .. }) if f == 1.0));
        assert!(matches!(&l[3], TemplateItem::Repeat(RepeatCount::Count(3), inner) if inner.len() == 2));
        assert!(matches!(l[4], TemplateItem::Track(TrackSize { fit_content: Some(_), .. })));
        assert!(parse_track_list("repeat(auto-fill, minmax(200px, 1fr))", &len).is_some());
        assert!(valid("grid-template-columns", "12.25rem minmax(0,1fr)"));
        assert!(expand_shorthand("grid-template", "min-content 1fr min-content / 12.25rem minmax(0,1fr)").unwrap().iter().all(|(n, v)| valid(n, v)));
        assert!(parse_track_list("minmax(1fr, 2fr)", &len).is_none());
        assert!(parse_track_list("-5px", &len).is_none());
    }

    #[test]
    fn areas_lines_and_shorthands() {
        let a = parse_areas(r#""head head" "nav main" ". foot""#).unwrap().unwrap();
        assert_eq!((a.rows, a.columns), (3, 2));
        assert!(a.areas.iter().any(|x| &*x.0 == "head" && (x.1, x.2, x.3, x.4) == (0, 0, 1, 2)));
        assert!(parse_areas(r#""a b a""#).is_none());
        assert_eq!(parse_line("span 2"), Some(GridLine::Span(2, None)));
        assert_eq!(parse_line("-1"), Some(GridLine::Line(-1, None)));
        assert_eq!(parse_line("main"), Some(GridLine::Ident(Arc::from("main"))));
        assert_eq!(parse_line("0"), None);
        let e = expand_shorthand("grid-area", "main").unwrap();
        assert!(e.iter().all(|(_, v)| v == "main"));
        let e = expand_shorthand("grid-column", "1 / -1").unwrap();
        assert_eq!(e, vec![("grid-column-start", "1".to_string()), ("grid-column-end", "-1".to_string())]);
        let e = expand_shorthand("grid-template", r#""a a" 40px "b c" 1fr / 1fr 2fr"#).unwrap();
        assert!(e.contains(&("grid-template-rows", "40px 1fr".to_string())));
        let e = expand_shorthand("grid", "auto-flow dense / 1fr 1fr").unwrap();
        assert!(e.contains(&("grid-auto-flow", "row dense".to_string())));
    }
}
