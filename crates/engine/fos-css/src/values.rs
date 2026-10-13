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

impl CustomProperties {
    /// Names declared in the layers above `root` (by the element and its
    /// ancestors below the root)
    fn declared_above<'s>(&'s self, root: &CustomProperties) -> Vec<&'s str> {
        let mut out = Vec::new();
        let mut at = Some(self);
        while let Some(c) = at {
            if std::ptr::eq(c, root) {
                break;
            }
            out.extend(c.own.keys().map(String::as_str));
            at = c.parent.as_deref();
        }
        out
    }
}

/// Custom properties registered by `@property` rules (CSS Properties and
/// Values 1): their initial values, and whether they inherit
#[derive(Debug, Default)]
pub struct PropertyRegistry {
    props: HashMap<String, (bool, Option<String>)>,
    /// The initial values, as the root's custom properties
    root: Option<std::sync::Arc<CustomProperties>>,
}

impl PropertyRegistry {
    pub fn new(rules: Vec<(String, bool, Option<String>)>) -> Self {
        let props: HashMap<String, (bool, Option<String>)> = rules.into_iter().map(|(n, inherits, initial)| (n, (inherits, initial))).collect();
        let initials: Vec<(&str, &str)> = props.iter().filter_map(|(n, (_, v))| Some((n.as_str(), v.as_deref()?))).collect();
        let root = (!initials.is_empty()).then(|| std::sync::Arc::new(compute_custom_properties(None, &initials)));
        PropertyRegistry { props, root }
    }

    pub fn is_empty(&self) -> bool {
        self.props.is_empty()
    }

    /// An element's custom properties given those it inherits and those it
    /// declares (`own`, in cascade order): the root starts from the initial
    /// values, a property that does not inherit takes its initial value
    /// where the element does not declare it, and `initial` is the
    /// registered initial value
    pub fn compute(&self, inherited: Option<&std::sync::Arc<CustomProperties>>, own: &[(&str, &str)]) -> Option<std::sync::Arc<CustomProperties>> {
        let inherited = inherited.or(self.root.as_ref());
        let mut decls: Vec<(&str, &str)> = Vec::new();
        if let (Some(from), Some(root)) = (inherited, self.root.as_deref()) {
            for name in from.declared_above(root) {
                if let Some((false, initial)) = self.props.get(name) {
                    let initial = initial.as_deref().unwrap_or("initial");
                    if !own.iter().any(|(n, _)| *n == name) && from.get(name) != Some(initial) && !decls.iter().any(|(n, _)| *n == name) {
                        decls.push((name, initial));
                    }
                }
            }
        }
        for &(name, value) in own {
            let value = match self.props.get(name) {
                Some((_, Some(initial))) if value.trim().eq_ignore_ascii_case("initial") => initial.as_str(),
                _ => value,
            };
            decls.push((name, value));
        }
        if decls.is_empty() {
            return inherited.cloned();
        }
        Some(std::sync::Arc::new(compute_custom_properties(inherited, &decls)))
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
    /// `ch` and `ex` in pixels for the element's font, and for its
    /// parent's (what they mean in `font-size`)
    pub ch_ex: (f32, f32),
    pub parent_ch_ex: (f32, f32),
    /// `lh` for the element and its parent (font-size's)
    pub lh: (f32, f32),
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

/// `s` without the math already left for layout (`-fos-calc(...)`), which
/// is not to be computed again
pub(crate) fn without_deferred_math(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains("-fos-calc(") {
        return s.into();
    }
    let mut out = String::with_capacity(s.len());
    let mut at = 0;
    while let Some(start) = s[at..].find("-fos-calc(").map(|i| at + i) {
        out.push_str(&s[at..start]);
        let open = start + "-fos-calc".len();
        let Some(close) = matching_paren(s, open) else { return s.into() };
        out.push('0');
        at = close + 1;
    }
    out.push_str(&s[at..]);
    out.into()
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
        // A substituted value is tokens of its own, never run together
        // with what is beside it: `var(--x)var(--y)` (minified Tailwind's
        // translate) is two values, not "-50%0"
        if runs_together(out.chars().next_back(), replacement.chars().next()) {
            out.push(' ');
        }
        out.push_str(&replacement);
        at = close + 1;
        if runs_together(replacement.chars().next_back(), value[at..].chars().next()) {
            out.push(' ');
        }
    }
    out.push_str(&value[at..]);
    Some(out)
}

/// Whether text ending in `a` and text starting with `b` would merge into
/// one token
fn runs_together(a: Option<char>, b: Option<char>) -> bool {
    let (Some(a), Some(b)) = (a, b) else { return false };
    (a.is_alphanumeric() || matches!(a, '%' | ')' | '.' | '_' | '-')) && (b.is_alphanumeric() || matches!(b, '-' | '.' | '#' | '_'))
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
    // CSS-wide keywords: `initial` is the guaranteed-invalid value (so
    // var() takes its fallback); the others give the inherited value, as
    // custom properties inherit
    let keyword = value.trim();
    if keyword.eq_ignore_ascii_case("initial") {
        return None;
    }
    if ["inherit", "unset", "revert", "revert-layer"].iter().any(|k| keyword.eq_ignore_ascii_case(k)) {
        let v = inherited.and_then(|m| m.get(name)).map(str::to_string);
        if let Some(v) = &v {
            done.insert(name.to_string(), v.clone());
        }
        return v;
    }
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

/// A math function over lengths whose comparisons must wait for the
/// percentage basis (`min(1040px, 100%)`), evaluated at layout
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CalcNode {
    Length { px: f32, pct: f32 },
    Sum(Vec<CalcNode>),
    Scale(Box<CalcNode>, f32),
    Min(Vec<CalcNode>),
    Max(Vec<CalcNode>),
    Clamp(Box<[CalcNode; 3]>),
}

impl CalcNode {
    pub(crate) fn eval(&self, basis: f32) -> f32 {
        match self {
            CalcNode::Length { px, pct } => px + pct * basis / 100.0,
            CalcNode::Sum(v) => v.iter().map(|n| n.eval(basis)).sum(),
            CalcNode::Scale(n, k) => n.eval(basis) * k,
            CalcNode::Min(v) => v.iter().map(|n| n.eval(basis)).fold(f32::INFINITY, f32::min),
            CalcNode::Max(v) => v.iter().map(|n| n.eval(basis)).fold(f32::NEG_INFINITY, f32::max),
            CalcNode::Clamp(b) => {
                let [lo, v, hi] = &**b;
                let lo = lo.eval(basis);
                v.eval(basis).min(hi.eval(basis)).max(lo)
            }
        }
    }

    /// As math in px and % alone, without spaces, which parses back to
    /// the same tree
    pub(crate) fn css(&self) -> String {
        let list = |v: &[CalcNode], sep: &str| v.iter().map(|n| n.css()).collect::<Vec<_>>().join(sep);
        match self {
            CalcNode::Length { px, pct } if *pct == 0.0 => format!("{px}px"),
            CalcNode::Length { px, pct } if *px == 0.0 => format!("{pct}%"),
            CalcNode::Length { px, pct } => format!("calc({px}px+{pct}%)"),
            CalcNode::Sum(v) => format!("calc({})", list(v, "+")),
            CalcNode::Scale(n, k) => format!("calc({}*{k})", n.css()),
            CalcNode::Min(v) => format!("min({})", list(v, ",")),
            CalcNode::Max(v) => format!("max({})", list(v, ",")),
            CalcNode::Clamp(b) => format!("clamp({})", list(&b[..], ",")),
        }
    }
}

/// Deferred math functions, interned by their text so lengths refer to
/// them by index (an [`Lp`](crate::style::Lp) has room for 22 bits of it)
static CALCS: std::sync::RwLock<Vec<CalcNode>> = std::sync::RwLock::new(Vec::new());
static CALC_INDEX: std::sync::OnceLock<std::sync::Mutex<HashMap<String, u32>>> = std::sync::OnceLock::new();
const MAX_CALCS: u32 = 1 << 22;

/// The length for `-fos-calc(<math>)`: what [`evaluate_math`] leaves for
/// layout when comparisons need the percentage basis
pub(crate) fn calc_length(v: &str) -> Option<crate::style::Lp> {
    let inner = v.strip_prefix("-fos-calc(")?.strip_suffix(')')?;
    let ctx = ResolveContext { custom: None, font_size: 16.0, parent_font_size: 16.0, root_font_size: 16.0, viewport: (0.0, 0.0), ch_ex: (8.0, 8.0), parent_ch_ex: (8.0, 8.0), lh: (19.2, 19.2) };
    let mut p = MathParser { s: inner.as_bytes(), i: 0, ctx: &ctx, percent_basis: None };
    let Val::Tree(tree) = p.value()? else { return None };
    if p.i != inner.len() {
        return None;
    }
    let mut index = CALC_INDEX.get_or_init(Default::default).lock().ok()?;
    let at = match index.get(inner) {
        Some(&i) => i,
        None => {
            let mut calcs = CALCS.write().ok()?;
            let i = calcs.len() as u32;
            if i >= MAX_CALCS {
                return None;
            }
            calcs.push(tree.clone());
            index.insert(inner.to_string(), i);
            i
        }
    };
    Some(crate::style::Lp::calc(at, tree.eval(0.0)))
}

/// An interned math function's value for a percentage basis
pub(crate) fn eval_calc(index: u32, basis: f32) -> f32 {
    CALCS.read().ok().and_then(|c| c.get(index as usize).map(|t| t.eval(basis))).unwrap_or(0.0)
}

/// An interned math function as CSS (`min(1040px, 100%)`)
pub(crate) fn calc_css(index: u32) -> String {
    let text = CALCS.read().ok().and_then(|c| c.get(index as usize).map(|t| t.css())).unwrap_or_default();
    text.replace(',', ", ").replace('+', " + ").replace('*', " * ")
}

/// A value being computed: a quantity, or a length kept as a tree
#[derive(Debug, Clone)]
enum Val {
    Q(Quantity),
    Tree(CalcNode),
}

impl Val {
    fn node(self) -> Option<CalcNode> {
        match self {
            Val::Q(q) if !q.is_number => Some(CalcNode::Length { px: q.px, pct: q.percent }),
            Val::Q(_) => None,
            Val::Tree(t) => Some(t),
        }
    }

    fn number(&self) -> Option<f32> {
        match self {
            Val::Q(q) if q.is_number => Some(q.number),
            _ => None,
        }
    }

    fn add(self, other: Val, sign: f32) -> Option<Val> {
        match (self, other) {
            (Val::Q(a), Val::Q(b)) => a.add(b, sign).map(Val::Q),
            (a, b) => {
                let b = b.node()?;
                let b = if sign < 0.0 { CalcNode::Scale(Box::new(b), -1.0) } else { b };
                Some(Val::Tree(CalcNode::Sum(vec![a.node()?, b])))
            }
        }
    }

    fn scale(self, k: f32) -> Val {
        match self {
            Val::Q(q) => Val::Q(q.scale(k)),
            Val::Tree(t) => Val::Tree(CalcNode::Scale(Box::new(t), k)),
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

    fn sum(&mut self) -> Option<Val> {
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

    fn product(&mut self) -> Option<Val> {
        let mut acc = self.value()?;
        loop {
            self.ws();
            match self.s.get(self.i) {
                Some(b'*') => {
                    self.i += 1;
                    let rhs = self.value()?;
                    acc = match (acc.number(), rhs.number()) {
                        (_, Some(k)) => acc.scale(k),
                        (Some(k), None) => rhs.scale(k),
                        _ => return None,
                    };
                }
                Some(b'/') => {
                    self.i += 1;
                    let rhs = self.value()?;
                    match rhs.number() {
                        Some(k) if k != 0.0 => acc = acc.scale(1.0 / k),
                        _ => return None,
                    }
                }
                _ => return Some(acc),
            }
        }
    }

    fn args(&mut self) -> Option<Vec<Val>> {
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

    fn value(&mut self) -> Option<Val> {
        self.ws();
        let rest = std::str::from_utf8(&self.s[self.i..]).ok()?;
        let lower = rest.to_ascii_lowercase();
        for f in ["calc(", "min(", "max(", "clamp("] {
            if lower.starts_with(f) {
                self.i += f.len();
                let args = self.args()?;
                // Quantities that compare now (numbers, or lengths all in
                // px or all in %), else lengths kept as a tree for layout
                let quantities: Option<Vec<Quantity>> = args.iter().map(|a| match a {
                    Val::Q(q) => self.resolve_percent(*q),
                    Val::Tree(_) => None,
                }).collect();
                let comparable: Option<Vec<f32>> = quantities.as_ref().and_then(|qs| {
                    let values: Option<Vec<f32>> = qs.iter().map(|q| q.comparable()).collect();
                    // Px and % compare only once the basis is known
                    let units = qs.iter().filter(|q| !q.is_number && q.px != 0.0).count();
                    let pcts = qs.iter().filter(|q| !q.is_number && q.percent != 0.0).count();
                    values.filter(|_| units == 0 || pcts == 0)
                });
                let tree = |args: Vec<Val>| -> Option<Vec<CalcNode>> { args.into_iter().map(|a| a.node()).collect() };
                return match (f, args.len()) {
                    ("calc(", 1) => args.into_iter().next(),
                    ("min(", n) | ("max(", n) if n > 0 => match comparable {
                        Some(c) => {
                            let mut best = 0;
                            for i in 1..n {
                                if (f == "min(" && c[i] < c[best]) || (f == "max(" && c[i] > c[best]) {
                                    best = i;
                                }
                            }
                            args.into_iter().nth(best)
                        }
                        None if f == "min(" => Some(Val::Tree(CalcNode::Min(tree(args)?))),
                        None => Some(Val::Tree(CalcNode::Max(tree(args)?))),
                    },
                    ("clamp(", 3) => match comparable {
                        Some(c) => {
                            let (l, x, h) = (c[0], c[1], c[2]);
                            let i = if x < l { 0 } else if x > h.max(l) { 2 } else { 1 };
                            args.into_iter().nth(i)
                        }
                        None => {
                            let [lo, v, hi]: [CalcNode; 3] = tree(args)?.try_into().ok()?;
                            Some(Val::Tree(CalcNode::Clamp(Box::new([lo, v, hi]))))
                        }
                    },
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
        Some(Val::Q(match unit.as_str() {
            "" => Quantity::number(n),
            "px" => Quantity::length(n, 0.0),
            "%" => Quantity::length(0.0, n),
            "em" => Quantity::length(n * c.font_size, 0.0),
            "rem" => Quantity::length(n * c.root_font_size, 0.0),
            "ch" => Quantity::length(n * c.ch_ex.0, 0.0),
            "ex" => Quantity::length(n * c.ch_ex.1, 0.0),
            "lh" => Quantity::length(n * c.lh.0, 0.0),
            "vw" | "svw" | "lvw" | "dvw" => Quantity::length(n * vw / 100.0, 0.0),
            "vh" | "svh" | "lvh" | "dvh" => Quantity::length(n * vh / 100.0, 0.0),
            "vmin" | "svmin" | "lvmin" | "dvmin" => Quantity::length(n * vw.min(vh) / 100.0, 0.0),
            "vmax" | "svmax" | "lvmax" | "dvmax" => Quantity::length(n * vw.max(vh) / 100.0, 0.0),
            "pt" => Quantity::length(n * 4.0 / 3.0, 0.0),
            "pc" => Quantity::length(n * 16.0, 0.0),
            "in" => Quantity::length(n * 96.0, 0.0),
            "cm" => Quantity::length(n * 96.0 / 2.54, 0.0),
            "mm" => Quantity::length(n * 96.0 / 25.4, 0.0),
            "q" => Quantity::length(n * 96.0 / 101.6, 0.0),
            _ => return None,
        }))
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
    let v = p.value()?;
    p.ws();
    if p.i != expr.len() {
        return None;
    }
    let q = match v {
        Val::Q(q) => q,
        Val::Tree(t) => match percent_basis {
            Some(basis) => Quantity::length(t.eval(basis), 0.0),
            // For layout to finish once the basis is known
            None => return Some(format!("-fos-calc({})", t.css())),
        },
    };
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

/// A math function of absolute lengths (`calc(1120px - 1px)`) in px, as
/// media queries use them: `em` and `rem` are the initial 16px
pub(crate) fn math_length(expr: &str, viewport: (f32, f32)) -> Option<f32> {
    let ctx = ResolveContext { custom: None, font_size: 16.0, parent_font_size: 16.0, root_font_size: 16.0, viewport, ch_ex: (8.0, 8.0), parent_ch_ex: (8.0, 8.0), lh: (19.2, 19.2) };
    let out = evaluate_math(expr.trim(), &ctx, None)?;
    match out.strip_suffix("px") {
        Some(px) => px.parse().ok(),
        None => out.parse::<f32>().ok().filter(|n| *n == 0.0),
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
        // In a relative color, math over its channels (`calc(h + 120)`)
        // stays for the color parser
        let original = &value[start..=close];
        match evaluate_math(original, ctx, percent_basis) {
            Some(v) => out.push_str(&v),
            None if value.to_ascii_lowercase().contains("(from ") => out.push_str(original),
            None => return None,
        }
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
    units: [u32; 6],
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
            units: [ctx.ch_ex.0, ctx.ch_ex.1, ctx.parent_ch_ex.0, ctx.parent_ch_ex.1, ctx.lh.0, ctx.lh.1].map(f32::to_bits),
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
        (ResolveContext { font_size: ctx.parent_font_size, ch_ex: ctx.parent_ch_ex, lh: (ctx.lh.1, ctx.lh.1), ..*ctx }, Some(ctx.parent_font_size))
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
        ResolveContext { custom, font_size: 20.0, parent_font_size: 10.0, root_font_size: 16.0, viewport: (1000.0, 800.0), ch_ex: (10.0, 10.0), parent_ch_ex: (5.0, 5.0), lh: (24.0, 12.0) }
    }

    fn px(decls: &[Declaration]) -> Option<f32> {
        decls.iter().find_map(|d| match d.value {
            PropertyValue::Length(Length { value, unit: LengthUnit::Px }) => Some(value),
            _ => None,
        })
    }

    #[test]
    fn math_over_color_channels_stays_for_the_color_parser() {
        // calc() that is not about lengths (a relative color's channels)
        // reaches the property's own parser instead of voiding it
        let m: HashMap<String, String> = [("--tint".to_string(), "#ff0000".to_string())].into();
        let d = resolve_declaration("color", "hsl(from var(--tint) calc(h + 120) s l)", false, &ctx(Some(&m)));
        assert!(matches!(d.first().map(|d| &d.value), Some(PropertyValue::Color(c)) if (c.r, c.g, c.b) == (0, 255, 0)), "{d:?}");
        // Still rejected when nothing understands it
        assert!(resolve_declaration("width", "calc(h + 1)", false, &ctx(None)).is_empty());
    }

    #[test]
    fn registered_properties_start_from_initial_values() {
        let reg = PropertyRegistry::new(vec![
            ("--style".into(), false, Some("solid".into())),
            ("--size".into(), true, Some("1px".into())),
            ("--none".into(), false, None),
        ]);
        // The root has the initial values
        let root = reg.compute(None, &[]).unwrap();
        assert_eq!((root.get("--style"), root.get("--size"), root.get("--none")), (Some("solid"), Some("1px"), None));
        // A parent's values: inheriting ones pass down, others do not
        let parent = reg.compute(Some(&root), &[("--style", "dashed"), ("--size", "2px"), ("--none", "x")]).unwrap();
        let child = reg.compute(Some(&parent), &[]).unwrap();
        assert_eq!((child.get("--style"), child.get("--size"), child.get("--none")), (Some("solid"), Some("2px"), None));
        // `initial` is the registered initial value
        let own = reg.compute(Some(&parent), &[("--size", "initial")]).unwrap();
        assert_eq!(own.get("--size"), Some("1px"));
    }

    #[test]
    fn substituted_values_stay_apart() {
        // Minified Tailwind: `translate:var(--tw-translate-x)var(--tw-translate-y)`
        let m: HashMap<String, String> = [("--x".to_string(), "-50%".to_string()), ("--y".to_string(), "0".to_string())].into();
        let m: &dyn VarSource = &m;
        assert_eq!(substitute_vars("var(--x)var(--y)", Some(m)).as_deref(), Some("-50% 0"));
        // Nothing is added where tokens could not merge
        assert_eq!(substitute_vars("calc(var(--x)*-1)", Some(m)).as_deref(), Some("calc(-50%*-1)"));
        assert_eq!(substitute_vars("var(--x), var(--y)", Some(m)).as_deref(), Some("-50%, 0"));
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
    fn custom_properties_take_css_wide_keywords() {
        let root = std::sync::Arc::new(compute_custom_properties(None, &[("--a", "1"), ("--b", "2")]));
        // `initial` is the guaranteed-invalid value, hiding what is
        // inherited (Tailwind resets its variables so); `inherit` and
        // `unset` take the parent's value
        let own = [("--a", "initial"), ("--b", "unset"), ("--c", "inherit"), ("--d", "var(--a, 7)")];
        let m = compute_custom_properties(Some(&root), &own);
        assert_eq!(m.get("--a"), None);
        assert_eq!(m.get("--b"), Some("2"));
        assert_eq!(m.get("--c"), None);
        assert_eq!(m.get("--d"), Some("7"));
        assert_eq!(substitute_vars("var(--a, 1.5)", Some(&m)).as_deref(), Some("1.5"));
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
        // Comparisons between px and % wait for layout, as a tree in px
        // and % alone (em resolved now)
        assert_eq!(eval("min(1040px, 100%)").as_deref(), Some("-fos-calc(min(1040px,100%))"));
        assert_eq!(eval("calc(max(1em, (100% - 1000px) / 2) * 2)").as_deref(), Some("-fos-calc(calc(max(20px,calc(-500px+50%))*2))"));
        assert_eq!(eval("min(10px, 5%, 1em)").as_deref(), Some("-fos-calc(min(10px,5%,20px))"));
        // With the basis known, they compare now
        assert_eq!(evaluate_math("min(1040px, 100%)", &c, Some(800.0)).as_deref(), Some("800px"));
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
