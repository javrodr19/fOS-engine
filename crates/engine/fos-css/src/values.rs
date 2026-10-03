//! Values computed per element: `var()` substitution and math functions
//!
//! Declarations using custom properties or `calc()`/`min()`/`max()`/
//! `clamp()` cannot be converted when the stylesheet is parsed: their
//! value depends on the element (its custom properties and font size) and
//! on the viewport. The parser keeps them as text
//! (`PropertyValue::Unresolved`) and [`resolve_declaration`] finishes the
//! job during the cascade.

use std::collections::HashMap;

use crate::Declaration;

/// Most nested `var()` substitutions followed (guards against blow-up)
const MAX_VAR_DEPTH: u32 = 32;

/// Where `var()` looks names up
pub trait VarSource: std::fmt::Debug {
    fn var(&self, name: &str) -> Option<&str>;
}

impl VarSource for HashMap<String, String> {
    fn var(&self, name: &str) -> Option<&str> {
        self.get(name).map(String::as_str)
    }
}

/// An element's computed custom properties: the ones it declares, over its
/// parent's (shared, never copied: pages define thousands of variables
/// at the root and override a few per component)
#[derive(Debug, Default, PartialEq)]
pub struct CustomProperties {
    /// Declared here; `None` marks a property made invalid (a cycle),
    /// which hides the inherited one
    own: HashMap<String, Option<String>>,
    parent: Option<std::sync::Arc<CustomProperties>>,
    depth: u16,
}

/// Chains longer than this are flattened (lookups stay short)
const MAX_CUSTOM_CHAIN: u16 = 12;

impl CustomProperties {
    /// The computed value of property `name`
    pub fn get(&self, name: &str) -> Option<&str> {
        let mut at = Some(self);
        while let Some(c) = at {
            if let Some(v) = c.own.get(name) {
                return v.as_deref();
            }
            at = c.parent.as_deref();
        }
        None
    }

    /// Every defined property (nearest definition wins)
    pub fn to_map(&self) -> HashMap<String, String> {
        let mut out: HashMap<String, Option<String>> = HashMap::new();
        let mut at = Some(self);
        while let Some(c) = at {
            for (k, v) in &c.own {
                out.entry(k.clone()).or_insert_with(|| v.clone());
            }
            at = c.parent.as_deref();
        }
        out.into_iter().filter_map(|(k, v)| v.map(|v| (k, v))).collect()
    }
}

impl VarSource for CustomProperties {
    fn var(&self, name: &str) -> Option<&str> {
        self.get(name)
    }
}

/// What values are computed against
#[derive(Debug, Clone, Copy)]
pub struct ResolveContext<'a> {
    /// The element's custom properties (already computed)
    pub custom: Option<&'a dyn VarSource>,
    /// The element's font size (what `em` means, except in `font-size`)
    pub font_size: f32,
    /// The parent's font size (what `em` and `%` mean in `font-size`)
    pub parent_font_size: f32,
    /// The root element's font size (`rem`)
    pub root_font_size: f32,
    /// Viewport size (`vw`, `vh`)
    pub viewport: (f32, f32),
}

/// Find `name(` at a token boundary (case-insensitive), from `from`
fn find_function(s: &str, name: &str, from: usize) -> Option<usize> {
    let b = s.as_bytes();
    let n = name.as_bytes();
    if b.len() < n.len() + 1 {
        return None;
    }
    let mut i = from;
    // Each candidate is an opening parenthesis preceded by the name
    while let Some(p) = b.get(i..).and_then(|rest| rest.iter().position(|&c| c == b'(')) {
        let open = i + p;
        if open >= n.len() {
            let start = open - n.len();
            let boundary = start == 0 || !matches!(b[start - 1].to_ascii_lowercase(), b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_');
            if boundary && b[start..open].eq_ignore_ascii_case(n) && start >= from {
                return Some(start);
            }
        }
        i = open + 1;
    }
    None
}

/// Whether `s` calls function `name` (case-insensitive)
pub(crate) fn has_function(s: &str, name: &str) -> bool {
    find_function(s, name, 0).is_some()
}

/// Index of the `)` closing the `(` at `open`
fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Replace every `var(--name[, fallback])` in `value`; `None` if a
/// reference has neither a value nor a fallback (the declaration is then
/// "invalid at computed-value time")
pub fn substitute_vars(value: &str, custom: Option<&dyn VarSource>) -> Option<String> {
    substitute(value, custom, 0)
}

fn substitute(value: &str, custom: Option<&dyn VarSource>, depth: u32) -> Option<String> {
    if depth > MAX_VAR_DEPTH {
        return None;
    }
    let mut out = String::with_capacity(value.len());
    let mut at = 0;
    while let Some(start) = find_function(value, "var", at) {
        out.push_str(&value[at..start]);
        let open = start + 3;
        let close = matching_paren(value, open)?;
        let inner = &value[open + 1..close];
        let (name, fallback) = match inner.find(',') {
            Some(c) => (inner[..c].trim(), Some(&inner[c + 1..])),
            None => (inner.trim(), None),
        };
        let replacement = match custom.and_then(|m| m.var(name)) {
            Some(v) => substitute(v, custom, depth + 1)?,
            None => substitute(fallback?.trim(), custom, depth + 1)?,
        };
        out.push_str(&replacement);
        at = close + 1;
    }
    out.push_str(&value[at..]);
    Some(out)
}

/// Compute the custom properties an element declares (`own`, in cascade
/// order) on top of the inherited ones: references between them are
/// resolved, and cycles make the properties involved invalid
pub fn compute_custom_properties(inherited: Option<&std::sync::Arc<CustomProperties>>, own: &[(&str, &str)]) -> CustomProperties {
    let mut raw: HashMap<&str, &str> = HashMap::new();
    for &(name, value) in own {
        raw.insert(name, value);
    }
    let parent = inherited.map(|p| &**p);
    let mut done: HashMap<String, String> = HashMap::new();
    let names: Vec<&str> = raw.keys().copied().collect();
    let mut resolving = Vec::new();
    for &name in &names {
        resolve_custom(name, &raw, &mut done, parent, &mut resolving);
    }
    let mut own: HashMap<String, Option<String>> = names.iter().map(|&n| (n.to_string(), done.remove(n))).collect();
    let depth = inherited.map_or(0, |p| p.depth + 1);
    if depth > MAX_CUSTOM_CHAIN {
        // Flatten: copy the inherited properties under this level's
        for (k, v) in inherited.map(|p| p.to_map()).unwrap_or_default() {
            own.entry(k).or_insert(Some(v));
        }
        return CustomProperties { own, parent: None, depth: 0 };
    }
    CustomProperties { own, parent: inherited.cloned(), depth }
}

fn resolve_custom<'a>(
    name: &'a str,
    raw: &HashMap<&'a str, &'a str>,
    done: &mut HashMap<String, String>,
    inherited: Option<&CustomProperties>,
    resolving: &mut Vec<&'a str>,
) -> Option<String> {
    if let Some(v) = done.get(name) {
        return Some(v.clone());
    }
    let Some(&value) = raw.get(name) else {
        return inherited.and_then(|m| m.get(name)).map(str::to_string);
    };
    if resolving.contains(&name) {
        // A cycle: every property in it is invalid
        return None;
    }
    resolving.push(name);
    // Resolve the references this value makes first
    let mut at = 0;
    let mut ok = true;
    let mut local: HashMap<String, String> = HashMap::new();
    while let Some(start) = find_function(value, "var", at) {
        let Some(close) = matching_paren(value, start + 3) else {
            ok = false;
            break;
        };
        let inner = &value[start + 4..close];
        let referenced = inner.split(',').next().unwrap_or("").trim();
        // Every key of `raw` is borrowed from `own`, so look it up there
        if let Some((&key, _)) = raw.get_key_value(referenced) {
            if let Some(v) = resolve_custom(key, raw, done, inherited, resolving) {
                local.insert(referenced.to_string(), v);
            }
        } else if let Some(v) = done.get(referenced).cloned().or_else(|| inherited.and_then(|m| m.get(referenced)).map(str::to_string)) {
            local.insert(referenced.to_string(), v);
        }
        at = close + 1;
    }
    resolving.pop();
    let result = if ok { substitute(value, Some(&local), 0) } else { None };
    if let Some(v) = &result {
        done.insert(name.to_string(), v.trim().to_string());
    }
    result
}

// ---- math functions ----

/// A length being computed: px plus a percentage (of a basis that may only
/// be known later), or a plain number
#[derive(Debug, Clone, Copy, PartialEq)]
struct Quantity {
    px: f32,
    percent: f32,
    number: f32,
    /// Whether this is a plain number (no unit)
    is_number: bool,
}

impl Quantity {
    fn number(n: f32) -> Self {
        Self { px: 0.0, percent: 0.0, number: n, is_number: true }
    }

    fn length(px: f32, percent: f32) -> Self {
        Self { px, percent, number: 0.0, is_number: false }
    }

    fn add(self, other: Self, sign: f32) -> Option<Self> {
        match (self.is_number, other.is_number) {
            (true, true) => Some(Self::number(self.number + sign * other.number)),
            (false, false) => Some(Self::length(self.px + sign * other.px, self.percent + sign * other.percent)),
            // `calc(0 + 10px)` is invalid in CSS, but 0 is harmless
            (true, false) if self.number == 0.0 => Some(Self::length(sign * other.px, sign * other.percent)),
            (false, true) if other.number == 0.0 => Some(self),
            _ => None,
        }
    }

    fn scale(self, k: f32) -> Self {
        if self.is_number {
            Self::number(self.number * k)
        } else {
            Self::length(self.px * k, self.percent * k)
        }
    }

    /// A single comparable value (for min/max/clamp), if there is one
    fn comparable(self) -> Option<f32> {
        if self.is_number {
            Some(self.number)
        } else if self.percent == 0.0 {
            Some(self.px)
        } else if self.px == 0.0 {
            Some(self.percent)
        } else {
            None
        }
    }
}

struct MathParser<'a, 'c> {
    s: &'a [u8],
    i: usize,
    ctx: &'c ResolveContext<'c>,
    /// What `%` is a percentage of, when known (font-size: the parent's)
    percent_basis: Option<f32>,
}

impl MathParser<'_, '_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn sum(&mut self) -> Option<Quantity> {
        let mut acc = self.product()?;
        loop {
            self.ws();
            let sign = match self.s.get(self.i) {
                Some(b'+') => 1.0,
                Some(b'-') => -1.0,
                _ => return Some(acc),
            };
            self.i += 1;
            let rhs = self.product()?;
            acc = acc.add(rhs, sign)?;
        }
    }

    fn product(&mut self) -> Option<Quantity> {
        let mut acc = self.value()?;
        loop {
            self.ws();
            match self.s.get(self.i) {
                Some(b'*') => {
                    self.i += 1;
                    let rhs = self.value()?;
                    acc = match (acc.is_number, rhs.is_number) {
                        (_, true) => acc.scale(rhs.number),
                        (true, false) => rhs.scale(acc.number),
                        _ => return None,
                    };
                }
                Some(b'/') => {
                    self.i += 1;
                    let rhs = self.value()?;
                    if !rhs.is_number || rhs.number == 0.0 {
                        return None;
                    }
                    acc = acc.scale(1.0 / rhs.number);
                }
                _ => return Some(acc),
            }
        }
    }

    fn args(&mut self) -> Option<Vec<Quantity>> {
        // After the `(`
        let mut out = vec![self.sum()?];
        loop {
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => {
                    self.i += 1;
                    out.push(self.sum()?);
                }
                Some(b')') => {
                    self.i += 1;
                    return Some(out);
                }
                _ => return None,
            }
        }
    }

    fn value(&mut self) -> Option<Quantity> {
        self.ws();
        let rest = std::str::from_utf8(&self.s[self.i..]).ok()?;
        let lower = rest.to_ascii_lowercase();
        for f in ["calc(", "min(", "max(", "clamp("] {
            if lower.starts_with(f) {
                self.i += f.len();
                let args = self.args()?;
                return match (f, args.as_slice()) {
                    ("calc(", [one]) => Some(*one),
                    ("min(", list) | ("max(", list) if !list.is_empty() => {
                        let mut best = list[0];
                        for &q in &list[1..] {
                            let (a, b) = (self.resolve_percent(best)?.comparable()?, self.resolve_percent(q)?.comparable()?);
                            if (f == "min(" && b < a) || (f == "max(" && b > a) {
                                best = q;
                            }
                        }
                        Some(best)
                    }
                    ("clamp(", [lo, v, hi]) => {
                        let (l, x, h) = (
                            self.resolve_percent(*lo)?.comparable()?,
                            self.resolve_percent(*v)?.comparable()?,
                            self.resolve_percent(*hi)?.comparable()?,
                        );
                        Some(if x < l { *lo } else if x > h.max(l) { *hi } else { *v })
                    }
                    _ => None,
                };
            }
        }
        if self.s.get(self.i) == Some(&b'(') {
            self.i += 1;
            let v = self.sum()?;
            self.ws();
            if self.s.get(self.i) != Some(&b')') {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        // A number with an optional unit
        let start = self.i;
        if matches!(self.s.get(self.i), Some(b'+' | b'-')) {
            self.i += 1;
        }
        while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.') {
            self.i += 1;
        }
        if self.i < self.s.len() && matches!(self.s[self.i], b'e' | b'E') && self.s.get(self.i + 1).is_some_and(|c| c.is_ascii_digit() || *c == b'-') {
            self.i += 2;
            while self.i < self.s.len() && self.s[self.i].is_ascii_digit() {
                self.i += 1;
            }
        }
        let n: f32 = std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()?;
        let unit_start = self.i;
        while self.i < self.s.len() && (self.s[self.i].is_ascii_alphabetic() || self.s[self.i] == b'%') {
            self.i += 1;
        }
        let unit = std::str::from_utf8(&self.s[unit_start..self.i]).ok()?.to_ascii_lowercase();
        let c = self.ctx;
        let (vw, vh) = c.viewport;
        Some(match unit.as_str() {
            "" => Quantity::number(n),
            "px" => Quantity::length(n, 0.0),
            "%" => Quantity::length(0.0, n),
            "em" => Quantity::length(n * c.font_size, 0.0),
            "rem" => Quantity::length(n * c.root_font_size, 0.0),
            "ex" | "ch" => Quantity::length(n * c.font_size * 0.5, 0.0),
            "vw" => Quantity::length(n * vw / 100.0, 0.0),
            "vh" => Quantity::length(n * vh / 100.0, 0.0),
            "vmin" => Quantity::length(n * vw.min(vh) / 100.0, 0.0),
            "vmax" => Quantity::length(n * vw.max(vh) / 100.0, 0.0),
            "pt" => Quantity::length(n * 4.0 / 3.0, 0.0),
            "pc" => Quantity::length(n * 16.0, 0.0),
            "in" => Quantity::length(n * 96.0, 0.0),
            "cm" => Quantity::length(n * 96.0 / 2.54, 0.0),
            "mm" => Quantity::length(n * 96.0 / 25.4, 0.0),
            "q" => Quantity::length(n * 96.0 / 101.6, 0.0),
            _ => return None,
        })
    }

    /// Fold the percentage into px when its basis is known
    fn resolve_percent(&self, q: Quantity) -> Option<Quantity> {
        match self.percent_basis {
            Some(basis) if !q.is_number && q.percent != 0.0 => Some(Quantity::length(q.px + q.percent * basis / 100.0, 0.0)),
            _ => Some(q),
        }
    }
}

/// Evaluate a math function (`calc(...)`, `min(...)`, ...) to CSS text:
/// `"12px"`, `"50%"`, `"1.5"`, or `"-fos-mix(12px 50%)"` for a length
/// plus a percentage of a basis known only at layout; `None` if invalid
fn evaluate_math(expr: &str, ctx: &ResolveContext, percent_basis: Option<f32>) -> Option<String> {
    let mut p = MathParser { s: expr.as_bytes(), i: 0, ctx, percent_basis };
    let q = p.value()?;
    p.ws();
    if p.i != expr.len() {
        return None;
    }
    let q = p.resolve_percent(q)?;
    if q.is_number {
        Some(format!("{}", q.number))
    } else if q.percent == 0.0 {
        Some(format!("{}px", q.px))
    } else if q.px == 0.0 {
        Some(format!("{}%", q.percent))
    } else {
        // Both parts, for layout to resolve once the basis is known
        Some(format!("-fos-mix({}px {}%)", q.px, q.percent))
    }
}

/// Replace the math functions in `value` with their results
fn evaluate_math_functions(value: &str, ctx: &ResolveContext, percent_basis: Option<f32>) -> Option<String> {
    let mut out = String::with_capacity(value.len());
    let mut at = 0;
    loop {
        let next = ["calc", "min", "max", "clamp"]
            .iter()
            .filter_map(|f| find_function(value, f, at))
            .min();
        let Some(start) = next else { break };
        let open = start + value[start..].find('(')?;
        let close = matching_paren(value, open)?;
        out.push_str(&value[at..start]);
        out.push_str(&evaluate_math(&value[start..=close], ctx, percent_basis)?);
        at = close + 1;
    }
    out.push_str(&value[at..]);
    Some(out)
}

/// Results of [`resolve_declaration`] for one layout pass: elements mostly
/// share their custom properties (inherited from the root) and font size,
/// so the same `var()` declarations would otherwise be substituted and
/// parsed again for each of them
#[derive(Default)]
pub struct ResolveCache {
    entries: HashMap<CacheKey, Vec<Declaration>>,
    /// The custom property maps keyed by address, kept alive so an address
    /// is never reused by another map while the cache exists
    pinned: HashMap<usize, std::sync::Arc<CustomProperties>>,
}

#[derive(PartialEq, Eq, Hash)]
struct CacheKey {
    name: String,
    value: String,
    important: bool,
    custom: usize,
    font_size: u32,
    parent_font_size: u32,
    root_font_size: u32,
}

impl ResolveCache {
    /// Resolve with [`resolve_declaration`], reusing earlier results
    pub fn resolve(
        &mut self,
        name: &str,
        value: &str,
        important: bool,
        custom: Option<&std::sync::Arc<CustomProperties>>,
        ctx: &ResolveContext,
    ) -> &[Declaration] {
        let addr = custom.map_or(0, |m| std::sync::Arc::as_ptr(m) as usize);
        if let Some(m) = custom {
            self.pinned.entry(addr).or_insert_with(|| m.clone());
        }
        let key = CacheKey {
            name: name.to_string(),
            value: value.to_string(),
            important,
            custom: addr,
            font_size: ctx.font_size.to_bits(),
            parent_font_size: ctx.parent_font_size.to_bits(),
            root_font_size: ctx.root_font_size.to_bits(),
        };
        self.entries.entry(key).or_insert_with(|| resolve_declaration(name, value, important, ctx))
    }
}

/// Turn an unresolved declaration (`name: value` using `var()` or math
/// functions) into concrete declarations for this element; none if the
/// value turns out invalid
pub fn resolve_declaration(name: &str, value: &str, important: bool, ctx: &ResolveContext) -> Vec<Declaration> {
    let Some(substituted) = substitute_vars(value, ctx.custom) else { return Vec::new() };
    // In font-size, `em` and `%` refer to the parent's font size
    let (math_ctx, basis) = if name == "font-size" || name == "font" {
        (ResolveContext { font_size: ctx.parent_font_size, ..*ctx }, Some(ctx.parent_font_size))
    } else {
        (*ctx, None)
    };
    let Some(computed) = evaluate_math_functions(&substituted, &math_ctx, basis) else { return Vec::new() };
    let lower = computed.to_ascii_lowercase();
    if lower.contains("var(") || lower.contains("env(") || (lower.contains("attr(") && name != "content") {
        return Vec::new();
    }
    crate::parser::parse_declarations(&format!("{name}: {computed}{}", if important { " !important" } else { "" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::properties::{Length, LengthUnit, PropertyValue};

    fn ctx(custom: Option<&dyn VarSource>) -> ResolveContext<'_> {
        ResolveContext { custom, font_size: 20.0, parent_font_size: 10.0, root_font_size: 16.0, viewport: (1000.0, 800.0) }
    }

    fn px(decls: &[Declaration]) -> Option<f32> {
        decls.iter().find_map(|d| match d.value {
            PropertyValue::Length(Length { value, unit: LengthUnit::Px }) => Some(value),
            _ => None,
        })
    }

    #[test]
    fn var_substitution() {
        let m: HashMap<String, String> = [("--a".to_string(), "10px".to_string()), ("--c".to_string(), "red".to_string())].into();
        let m: &dyn VarSource = &m;
        assert_eq!(substitute_vars("var(--a) var(--b, 2px) VAR(--c)", Some(m)).as_deref(), Some("10px 2px red"));
        assert_eq!(substitute_vars("var(--missing, var(--a))", Some(m)).as_deref(), Some("10px"));
        assert_eq!(substitute_vars("var(--missing)", Some(m)), None);
        // Not a var() call
        assert_eq!(substitute_vars("navvar(x)", Some(m)).as_deref(), Some("navvar(x)"));
    }

    #[test]
    fn custom_property_inheritance_and_cycles() {
        let root = std::sync::Arc::new(compute_custom_properties(None, &[("--base", "4px"), ("--x", "1")]));
        let own = [("--a", "var(--b)"), ("--b", "calc(var(--base) * 2)"), ("--x", "var(--y)"), ("--y", "var(--x)")];
        let m = compute_custom_properties(Some(&root), &own);
        assert_eq!(m.get("--base"), Some("4px"));
        assert_eq!(m.get("--b"), Some("calc(4px * 2)"));
        assert_eq!(m.get("--a"), Some("calc(4px * 2)"));
        // A cycle invalidates, hiding the inherited value too
        assert!(m.get("--x").is_none() && m.get("--y").is_none());
        assert_eq!(m.to_map().len(), 3);
        // Long chains flatten without losing anything
        let mut chain = std::sync::Arc::new(m);
        for i in 0..40 {
            let name = format!("--v{i}");
            chain = std::sync::Arc::new(compute_custom_properties(Some(&chain), &[(name.as_str(), "x")]));
        }
        assert_eq!(chain.get("--base"), Some("4px"));
        assert_eq!(chain.get("--v0"), Some("x"));
        assert!(chain.get("--x").is_none());
    }

    #[test]
    fn math_functions() {
        let c = ctx(None);
        let eval = |e: &str| evaluate_math(e, &c, None);
        assert_eq!(eval("calc(1rem + 2px)").as_deref(), Some("18px"));
        assert_eq!(eval("calc(2em - 5px)").as_deref(), Some("35px"));
        assert_eq!(eval("calc((10px + 2px) * 3)").as_deref(), Some("36px"));
        assert_eq!(eval("calc(100px / 4)").as_deref(), Some("25px"));
        assert_eq!(eval("calc(10vw)").as_deref(), Some("100px"));
        assert_eq!(eval("min(10px, 2em, 1vw)").as_deref(), Some("10px"));
        assert_eq!(eval("max(1rem, 12px)").as_deref(), Some("16px"));
        assert_eq!(eval("clamp(12px, 5vw, 40px)").as_deref(), Some("40px"));
        assert_eq!(eval("calc(50%)").as_deref(), Some("50%"));
        // Mixed with a percentage: both parts kept for layout
        assert_eq!(eval("calc(100% - 20px)").as_deref(), Some("-fos-mix(-20px 100%)"));
        assert_eq!(eval("calc(10px * 2px)"), None);
        assert_eq!(eval("calc(1.5 * 2)").as_deref(), Some("3"));
    }

    #[test]
    fn resolving_declarations() {
        let m: HashMap<String, String> = [("--size".to_string(), "1.5em".to_string()), ("--gap".to_string(), "4px".to_string())].into();
        let c = ctx(Some(&m as &dyn VarSource));
        // font-size em is the parent's (10px)
        assert_eq!(px(&resolve_declaration("font-size", "calc(var(--size) + 1px)", false, &c)), Some(16.0));
        assert_eq!(px(&resolve_declaration("font-size", "calc(50% + 2px)", false, &c)), Some(7.0));
        // elsewhere em is the element's (20px)
        assert_eq!(px(&resolve_declaration("margin-top", "calc(var(--size) * 2)", false, &c)), Some(60.0));
        assert!(resolve_declaration("color", "var(--nope)", false, &c).is_empty());
        let d = resolve_declaration("margin", "var(--gap) calc(var(--gap) * 2)", true, &c);
        assert!(d.len() == 4 && d.iter().all(|d| d.important));
    }
}
