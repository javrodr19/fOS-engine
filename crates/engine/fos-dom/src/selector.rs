//! CSS selector matching against a `DomTree`
//!
//! Supports selector lists of complex selectors: type, universal, `#id`,
//! `.class`, attribute selectors (`[a]`, `=`, `~=`, `|=`, `^=`, `$=`,
//! `*=`, with the `i` flag), the descendant, child (`>`), next-sibling
//! (`+`) and subsequent-sibling (`~`) combinators, and the structural
//! pseudo-classes (`:first-child`, `:nth-child(2n+1)`, `:not(...)`,
//! `:is(...)`, ...). User-action pseudo-classes (`:hover`, `:focus`, ...)
//! never match, as nothing is hovered or focused from the matcher's view.
//!
//! Matching goes right to left: the last compound selector is tested
//! against the element first, so most candidates are rejected after a
//! single comparison.

use crate::{DomTree, ElementData, NodeId};

/// A parsed selector list (`a, b > c`)
#[derive(Debug, Clone)]
pub struct SelectorList(Vec<Complex>);

/// Compound selectors joined by combinators, stored right to left:
/// `parts[0]` is the subject, and `parts[i].1` says how `parts[i]` relates
/// to `parts[i + 1]`
#[derive(Debug, Clone)]
struct Complex {
    parts: Vec<(Compound, Combinator)>,
}

impl Complex {
    fn specificity(&self) -> (u32, u32, u32) {
        let mut total = (0, 0, 0);
        for (c, _) in &self.parts {
            let s = c.specificity();
            total = (total.0 + s.0, total.1 + s.1, total.2 + s.2);
        }
        total
    }
}

impl Compound {
    fn specificity(&self) -> (u32, u32, u32) {
        let mut a = self.id.is_some() as u32;
        let mut b = (self.classes.len() + self.attrs.len()) as u32;
        let mut c = self.tag.is_some() as u32 + self.pseudo_elements as u32;
        for p in &self.pseudos {
            match p {
                Pseudo::Where(_) => {}
                Pseudo::Not(list) | Pseudo::Is(list) | Pseudo::Has(list, _) => {
                    let s = list.max_specificity();
                    a += s.0;
                    b += s.1;
                    c += s.2;
                }
                Pseudo::Never if self.pseudo_elements > 0 => {}
                _ => b += 1,
            }
        }
        (a, b, c)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Combinator {
    /// The subject (no selector to its left)
    None,
    Descendant,
    Child,
    NextSibling,
    SubsequentSibling,
}

#[derive(Debug, Clone, Default)]
struct Compound {
    tag: Option<String>,
    id: Option<String>,
    classes: Vec<String>,
    attrs: Vec<AttrSel>,
    pseudos: Vec<Pseudo>,
    /// `::before` and friends (count as type selectors for specificity)
    pseudo_elements: u8,
}

#[derive(Debug, Clone)]
struct AttrSel {
    name: String,
    op: AttrOp,
    value: String,
    ignore_case: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AttrOp {
    Exists,
    Equals,
    Includes,
    DashMatch,
    Prefix,
    Suffix,
    Substring,
}

#[derive(Debug, Clone)]
enum Pseudo {
    FirstChild,
    LastChild,
    OnlyChild,
    FirstOfType,
    LastOfType,
    OnlyOfType,
    /// `an+b`, counting from the end, of the same type only
    Nth { a: i32, b: i32, from_end: bool, of_type: bool },
    Not(SelectorList),
    Is(SelectorList),
    /// Like `Is`, but adds no specificity
    Where(SelectorList),
    /// `:has(x)`, or `:has(> x)` when `direct`
    Has(SelectorList, bool),
    Root,
    Empty,
    Checked,
    Disabled,
    Enabled,
    Link,
    Never,
}

/// Kinds of keys in ancestor filters (see [`key_hash`])
pub const KEY_ID: u8 = 1;
pub const KEY_CLASS: u8 = 2;
/// Tags are hashed lowercase
pub const KEY_TAG: u8 = 3;

/// Hash of an id, class or tag an element carries, for ancestor Bloom
/// filters: a selector needing an ancestor with a key whose hash is not in
/// the element's ancestor filter cannot match
pub fn key_hash(kind: u8, s: &str) -> u32 {
    // FNV-1a
    let mut h: u32 = 0x811c9dc5 ^ kind as u32;
    h = h.wrapping_mul(0x01000193);
    for b in s.bytes() {
        let b = if kind == KEY_TAG { b.to_ascii_lowercase() } else { b };
        h = (h ^ b as u32).wrapping_mul(0x01000193);
    }
    h
}

/// The most selective simple selector an element must have to match a
/// complex selector: style engines file rules under it, so an element is
/// only tested against rules that can match it
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SubjectKey {
    Id(String),
    Class(String),
    /// Lowercase tag name
    Tag(String),
    /// An attribute the subject must carry (lowercase name)
    Attr(String),
}

/// Keys one of which any element matching `c` carries: its id, a class,
/// its tag or an attribute, or for `:is()`/`:where()` subjects the keys of
/// every alternative inside
fn compound_keys(c: &Compound) -> Option<Vec<SubjectKey>> {
    if let Some(id) = &c.id {
        return Some(vec![SubjectKey::Id(id.clone())]);
    }
    if let Some(class) = c.classes.first() {
        return Some(vec![SubjectKey::Class(class.clone())]);
    }
    if let Some(tag) = &c.tag {
        return Some(vec![SubjectKey::Tag(tag.clone())]);
    }
    if let Some(a) = c.attrs.first() {
        return Some(vec![SubjectKey::Attr(a.name.to_ascii_lowercase())]);
    }
    c.pseudos.iter().find_map(|p| match p {
        // The root of an HTML document is its html element
        Pseudo::Root => Some(vec![SubjectKey::Tag("html".into())]),
        Pseudo::Is(list) | Pseudo::Where(list) => {
            let mut keys = Vec::new();
            for alt in &list.0 {
                keys.extend(compound_keys(&alt.parts.first()?.0)?);
            }
            Some(keys)
        }
        _ => None,
    })
}

impl SelectorList {
    /// For each complex selector: key hashes (see [`key_hash`]) that its
    /// element's ancestors must carry (from compounds joined to the subject
    /// by descendant and child combinators only)
    pub fn ancestor_hashes(&self) -> Vec<Vec<u32>> {
        self.0
            .iter()
            .map(|c| {
                let mut out = Vec::new();
                for i in 1..c.parts.len() {
                    if !matches!(c.parts[i - 1].1, Combinator::Descendant | Combinator::Child) {
                        break;
                    }
                    let comp = &c.parts[i].0;
                    if let Some(id) = &comp.id {
                        out.push(key_hash(KEY_ID, id));
                    }
                    out.extend(comp.classes.iter().map(|cl| key_hash(KEY_CLASS, cl)));
                    if let Some(t) = &comp.tag {
                        out.push(key_hash(KEY_TAG, t));
                    }
                }
                out.sort_unstable();
                out.dedup();
                out
            })
            .collect()
    }

    /// Specificity (ids, classes, types) of each complex selector
    pub fn specificities(&self) -> Vec<(u32, u32, u32)> {
        self.0.iter().map(Complex::specificity).collect()
    }

    /// The highest specificity of the list (what `:is()` and `:not()` add)
    fn max_specificity(&self) -> (u32, u32, u32) {
        self.0.iter().map(Complex::specificity).max().unwrap_or((0, 0, 0))
    }

    /// Keys under which to file the list for matching: for each complex
    /// selector, keys one of which its subject must carry (several for an
    /// `:is()` subject), or `None` when it can match any element
    pub fn subject_keys(&self) -> Vec<Option<SubjectKey>> {
        let mut out = Vec::new();
        for c in &self.0 {
            match c.parts.first().and_then(|(subject, _)| compound_keys(subject)) {
                Some(keys) => out.extend(keys.into_iter().map(Some)),
                None => out.push(None),
            }
        }
        out
    }

    /// Parse a selector list; `None` if it is not valid (`querySelector`
    /// then throws a SyntaxError)
    pub fn parse(src: &str) -> Option<SelectorList> {
        let mut p = Parser { s: src.as_bytes(), i: 0 };
        let list = p.list()?;
        p.ws();
        (p.i == p.s.len()).then_some(list)
    }

    /// Whether the element `node` matches any selector of the list
    pub fn matches(&self, tree: &DomTree, node: NodeId) -> bool {
        self.0.iter().any(|c| match_complex(tree, node, &c.parts))
    }

    /// The first element in the subtree of `root` (excluding `root`), in
    /// document order, that matches
    pub fn query_first(&self, tree: &DomTree, root: NodeId) -> Option<NodeId> {
        let mut found = None;
        walk_elements(tree, root, &mut |id| {
            if self.matches(tree, id) {
                found = Some(id);
                false
            } else {
                true
            }
        });
        found
    }

    /// Every element in the subtree of `root` (excluding `root`), in
    /// document order, that matches
    pub fn query_all(&self, tree: &DomTree, root: NodeId) -> Vec<NodeId> {
        let mut found = Vec::new();
        walk_elements(tree, root, &mut |id| {
            if self.matches(tree, id) {
                found.push(id);
            }
            true
        });
        found
    }
}

/// Visit the elements below `root` in document order until `f` returns false
pub fn walk_elements(tree: &DomTree, root: NodeId, f: &mut dyn FnMut(NodeId) -> bool) {
    let Some(r) = tree.get(root) else { return };
    let mut id = r.first_child;
    while id.is_valid() {
        let Some(node) = tree.get(id) else { return };
        if node.is_element() && !f(id) {
            return;
        }
        // Next in pre-order, not leaving `root`'s subtree
        if node.first_child.is_valid() {
            id = node.first_child;
            continue;
        }
        let mut cur = id;
        loop {
            if cur == root {
                return;
            }
            let Some(n) = tree.get(cur) else { return };
            if n.next_sibling.is_valid() {
                id = n.next_sibling;
                break;
            }
            cur = n.parent;
            if !cur.is_valid() || cur == root {
                return;
            }
        }
    }
}

fn element(tree: &DomTree, id: NodeId) -> Option<&ElementData> {
    tree.get(id)?.as_element()
}

fn parent_element(tree: &DomTree, id: NodeId) -> Option<NodeId> {
    let p = tree.get(id)?.parent;
    element(tree, p).map(|_| p)
}

fn prev_element(tree: &DomTree, id: NodeId) -> Option<NodeId> {
    let mut cur = tree.get(id)?.prev_sibling;
    while cur.is_valid() {
        let n = tree.get(cur)?;
        if n.is_element() {
            return Some(cur);
        }
        cur = n.prev_sibling;
    }
    None
}

fn next_element(tree: &DomTree, id: NodeId) -> Option<NodeId> {
    let mut cur = tree.get(id)?.next_sibling;
    while cur.is_valid() {
        let n = tree.get(cur)?;
        if n.is_element() {
            return Some(cur);
        }
        cur = n.next_sibling;
    }
    None
}

fn match_complex(tree: &DomTree, node: NodeId, parts: &[(Compound, Combinator)]) -> bool {
    let Some((compound, comb)) = parts.first() else { return false };
    if !match_compound(tree, node, compound) {
        return false;
    }
    let rest = &parts[1..];
    match comb {
        Combinator::None => true,
        Combinator::Child => parent_element(tree, node).is_some_and(|p| match_complex(tree, p, rest)),
        Combinator::Descendant => {
            let mut cur = parent_element(tree, node);
            while let Some(p) = cur {
                if match_complex(tree, p, rest) {
                    return true;
                }
                cur = parent_element(tree, p);
            }
            false
        }
        Combinator::NextSibling => prev_element(tree, node).is_some_and(|p| match_complex(tree, p, rest)),
        Combinator::SubsequentSibling => {
            let mut cur = prev_element(tree, node);
            while let Some(p) = cur {
                if match_complex(tree, p, rest) {
                    return true;
                }
                cur = prev_element(tree, p);
            }
            false
        }
    }
}

fn attr<'t>(tree: &'t DomTree, e: &'t ElementData, name: &str) -> Option<&'t str> {
    e.attrs.iter().find(|a| tree.resolve(a.name.local).eq_ignore_ascii_case(name)).map(|a| a.value.as_str())
}

fn match_compound(tree: &DomTree, node: NodeId, c: &Compound) -> bool {
    let Some(e) = element(tree, node) else { return false };
    if let Some(tag) = &c.tag {
        if !tree.resolve(e.name.local).eq_ignore_ascii_case(tag) {
            return false;
        }
    }
    if let Some(id) = &c.id {
        if e.id.map(|i| tree.resolve(i)) != Some(id.as_str()) {
            return false;
        }
    }
    for class in &c.classes {
        if !e.classes.iter().any(|&k| tree.resolve(k) == class) {
            return false;
        }
    }
    for a in &c.attrs {
        let Some(v) = attr(tree, e, &a.name) else { return false };
        let (v, want) = if a.ignore_case {
            (v.to_ascii_lowercase(), a.value.to_ascii_lowercase())
        } else {
            (v.to_string(), a.value.clone())
        };
        let ok = match a.op {
            AttrOp::Exists => true,
            AttrOp::Equals => v == want,
            AttrOp::Includes => v.split_ascii_whitespace().any(|w| w == want),
            AttrOp::DashMatch => v == want || v.strip_prefix(&want).is_some_and(|r| r.starts_with('-')),
            AttrOp::Prefix => !want.is_empty() && v.starts_with(&want),
            AttrOp::Suffix => !want.is_empty() && v.ends_with(&want),
            AttrOp::Substring => !want.is_empty() && v.contains(&want),
        };
        if !ok {
            return false;
        }
    }
    c.pseudos.iter().all(|p| match_pseudo(tree, node, e, p))
}

/// 1-based position among element siblings, optionally of the same type
fn position(tree: &DomTree, node: NodeId, from_end: bool, of_type: bool) -> i32 {
    let name = element(tree, node).map(|e| e.name.local);
    let step = if from_end { next_element } else { prev_element };
    let mut n = 1;
    let mut cur = step(tree, node);
    while let Some(s) = cur {
        if !of_type || element(tree, s).map(|e| e.name.local) == name {
            n += 1;
        }
        cur = step(tree, s);
    }
    n
}

fn nth(a: i32, b: i32, pos: i32) -> bool {
    if a == 0 {
        return pos == b;
    }
    let d = pos - b;
    d % a == 0 && d / a >= 0
}

fn match_pseudo(tree: &DomTree, node: NodeId, e: &ElementData, p: &Pseudo) -> bool {
    match p {
        Pseudo::FirstChild => prev_element(tree, node).is_none(),
        Pseudo::LastChild => next_element(tree, node).is_none(),
        Pseudo::OnlyChild => prev_element(tree, node).is_none() && next_element(tree, node).is_none(),
        Pseudo::FirstOfType => position(tree, node, false, true) == 1,
        Pseudo::LastOfType => position(tree, node, true, true) == 1,
        Pseudo::OnlyOfType => position(tree, node, false, true) == 1 && position(tree, node, true, true) == 1,
        Pseudo::Nth { a, b, from_end, of_type } => nth(*a, *b, position(tree, node, *from_end, *of_type)),
        Pseudo::Not(list) => !list.matches(tree, node),
        Pseudo::Is(list) | Pseudo::Where(list) => list.matches(tree, node),
        Pseudo::Has(list, false) => list.query_first(tree, node).is_some(),
        Pseudo::Has(list, true) => tree.children(node).any(|(c, n)| n.is_element() && list.matches(tree, c)),
        Pseudo::Root => !parent_element(tree, node).is_some(),
        Pseudo::Empty => tree.children(node).all(|(_, c)| !c.is_element() && c.as_text().is_none_or(str::is_empty)),
        Pseudo::Checked => attr(tree, e, "checked").is_some() || attr(tree, e, "selected").is_some(),
        Pseudo::Disabled => attr(tree, e, "disabled").is_some(),
        Pseudo::Enabled => attr(tree, e, "disabled").is_none() && is_form_control(tree.resolve(e.name.local)),
        Pseudo::Link => {
            let tag = tree.resolve(e.name.local);
            (tag.eq_ignore_ascii_case("a") || tag.eq_ignore_ascii_case("area")) && attr(tree, e, "href").is_some()
        }
        Pseudo::Never => false,
    }
}

fn is_form_control(tag: &str) -> bool {
    ["input", "button", "select", "textarea", "option", "optgroup", "fieldset"].iter().any(|t| tag.eq_ignore_ascii_case(t))
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) -> bool {
        let start = self.i;
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')) {
            self.i += 1;
        }
        self.i > start
    }

    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn list(&mut self) -> Option<SelectorList> {
        let mut out = vec![self.complex()?];
        loop {
            self.ws();
            if !self.eat(b',') {
                out.shrink_to_fit();
                return Some(SelectorList(out));
            }
            out.push(self.complex()?);
        }
    }

    fn complex(&mut self) -> Option<Complex> {
        self.ws();
        // Left to right first, then reversed
        let mut compounds = vec![self.compound()?];
        let mut combs = Vec::new();
        loop {
            let had_ws = self.ws();
            let comb = match self.peek() {
                Some(b'>') => Combinator::Child,
                Some(b'+') => Combinator::NextSibling,
                Some(b'~') => Combinator::SubsequentSibling,
                Some(b',' | b')') | None => break,
                _ if had_ws => Combinator::Descendant,
                _ => return None,
            };
            if comb != Combinator::Descendant {
                self.i += 1;
                self.ws();
            }
            combs.push(comb);
            compounds.push(self.compound()?);
        }
        let mut parts = Vec::with_capacity(compounds.len());
        for (i, c) in compounds.into_iter().enumerate().rev() {
            let comb = if i == 0 { Combinator::None } else { combs[i - 1] };
            parts.push((c, comb));
        }
        Some(Complex { parts })
    }

    fn ident(&mut self) -> Option<String> {
        // The common case, no escapes: one slice of the source
        let start = self.i;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 0x80 {
                self.i += 1;
            } else {
                break;
            }
        }
        if self.peek() != Some(b'\\') {
            return (self.i > start).then(|| String::from_utf8_lossy(&self.s[start..self.i]).into_owned());
        }
        let mut out = String::from_utf8_lossy(&self.s[start..self.i]).into_owned();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 0x80 {
                let run = self.i;
                while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c >= 0x80) {
                    self.i += 1;
                }
                out.push_str(&String::from_utf8_lossy(&self.s[run..self.i]));
            } else if c == b'\\' {
                self.i += 1;
                let c = self.peek()?;
                out.push(c as char);
                self.i += 1;
            } else {
                break;
            }
        }
        (!out.is_empty()).then_some(out)
    }

    fn string(&mut self) -> Option<String> {
        let q = self.peek()?;
        if q != b'"' && q != b'\'' {
            return self.ident();
        }
        self.i += 1;
        let start = self.i;
        while self.peek()? != q {
            if self.peek() == Some(b'\\') {
                self.i += 1;
            }
            self.i += 1;
        }
        let v = std::str::from_utf8(&self.s[start..self.i]).ok()?.replace('\\', "");
        self.i += 1;
        Some(v)
    }

    fn compound(&mut self) -> Option<Compound> {
        let mut c = Compound::default();
        let mut any = false;
        if self.eat(b'*') {
            any = true;
        } else if let Some(tag) = self.ident() {
            c.tag = Some(tag.to_ascii_lowercase());
            any = true;
        }
        loop {
            match self.peek() {
                Some(b'#') => {
                    self.i += 1;
                    c.id = Some(self.ident()?);
                }
                Some(b'.') => {
                    self.i += 1;
                    c.classes.push(self.ident()?);
                }
                Some(b'[') => {
                    self.i += 1;
                    c.attrs.push(self.attr()?);
                }
                Some(b':') => {
                    self.i += 1;
                    if self.eat(b':') {
                        // Pseudo-elements are never elements in the tree
                        self.ident()?;
                        c.pseudos.push(Pseudo::Never);
                        c.pseudo_elements += 1;
                    } else {
                        c.pseudos.push(self.pseudo()?);
                    }
                }
                _ => break,
            }
            any = true;
        }
        // Stylesheets keep thousands of these: no spare capacity
        c.classes.shrink_to_fit();
        c.attrs.shrink_to_fit();
        c.pseudos.shrink_to_fit();
        any.then_some(c)
    }

    fn attr(&mut self) -> Option<AttrSel> {
        self.ws();
        let name = self.ident()?;
        self.ws();
        let op = match self.peek()? {
            b']' => {
                self.i += 1;
                return Some(AttrSel { name, op: AttrOp::Exists, value: String::new(), ignore_case: false });
            }
            b'=' => AttrOp::Equals,
            b'~' => AttrOp::Includes,
            b'|' => AttrOp::DashMatch,
            b'^' => AttrOp::Prefix,
            b'$' => AttrOp::Suffix,
            b'*' => AttrOp::Substring,
            _ => return None,
        };
        self.i += 1;
        if op != AttrOp::Equals && !self.eat(b'=') {
            return None;
        }
        self.ws();
        let value = self.string()?;
        self.ws();
        let mut ignore_case = false;
        if matches!(self.peek(), Some(b'i' | b'I')) {
            self.i += 1;
            ignore_case = true;
        } else if matches!(self.peek(), Some(b's' | b'S')) {
            self.i += 1;
        }
        self.ws();
        self.eat(b']').then_some(AttrSel { name, op, value, ignore_case })
    }

    fn pseudo(&mut self) -> Option<Pseudo> {
        let name = self.ident()?.to_ascii_lowercase();
        if self.eat(b'(') {
            self.ws();
            let p = match name.as_str() {
                "not" => Pseudo::Not(self.list()?),
                "is" | "matches" | "-webkit-any" | "-moz-any" => Pseudo::Is(self.list()?),
                "where" => Pseudo::Where(self.list()?),
                "has" => {
                    let direct = self.eat(b'>');
                    Pseudo::Has(self.list()?, direct)
                }
                "nth-child" | "nth-last-child" | "nth-of-type" | "nth-last-of-type" => {
                    let (a, b) = self.an_b()?;
                    Pseudo::Nth { a, b, from_end: name.contains("last"), of_type: name.ends_with("of-type") }
                }
                // :lang(), :dir() and the like: skip the argument
                _ => {
                    let mut depth = 1;
                    while depth > 0 {
                        match self.peek()? {
                            b'(' => depth += 1,
                            b')' => depth -= 1,
                            _ => {}
                        }
                        self.i += 1;
                    }
                    self.i -= 1;
                    Pseudo::Never
                }
            };
            self.ws();
            return self.eat(b')').then_some(p);
        }
        Some(match name.as_str() {
            "first-child" => Pseudo::FirstChild,
            "last-child" => Pseudo::LastChild,
            "only-child" => Pseudo::OnlyChild,
            "first-of-type" => Pseudo::FirstOfType,
            "last-of-type" => Pseudo::LastOfType,
            "only-of-type" => Pseudo::OnlyOfType,
            "root" => Pseudo::Root,
            "empty" => Pseudo::Empty,
            "checked" => Pseudo::Checked,
            "disabled" => Pseudo::Disabled,
            "enabled" => Pseudo::Enabled,
            "link" | "any-link" => Pseudo::Link,
            // Legacy single-colon pseudo-elements, user action and state
            _ => Pseudo::Never,
        })
    }

    /// `odd`, `even`, `3`, `-n+3`, `2n + 1`
    fn an_b(&mut self) -> Option<(i32, i32)> {
        let start = self.i;
        while let Some(c) = self.peek() {
            if c == b')' {
                break;
            }
            self.i += 1;
        }
        let text: String = std::str::from_utf8(&self.s[start..self.i]).ok()?.chars().filter(|c| !c.is_whitespace()).collect();
        let text = text.to_ascii_lowercase();
        match text.as_str() {
            "odd" => return Some((2, 1)),
            "even" => return Some((2, 0)),
            _ => {}
        }
        if let Some(n) = text.find('n') {
            let a = match &text[..n] {
                "" | "+" => 1,
                "-" => -1,
                s => s.parse().ok()?,
            };
            let rest = &text[n + 1..];
            let b = if rest.is_empty() { 0 } else { rest.trim_start_matches('+').parse().ok()? };
            Some((a, b))
        } else {
            Some((0, text.trim_start_matches('+').parse().ok()?))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// <div id=a class="x y"><p>1</p><span class=x lang=en-US>2</span><p>3</p></div>
    fn tree() -> (DomTree, [NodeId; 4]) {
        let mut t = DomTree::new();
        let div = t.create_element("div");
        t.set_attribute(div, "id", "a");
        t.set_attribute(div, "class", "x y");
        let p1 = t.create_element("p");
        let span = t.create_element("span");
        t.set_attribute(span, "class", "x");
        t.set_attribute(span, "lang", "en-US");
        let p2 = t.create_element("p");
        let root = t.root();
        t.append_child(root, div);
        for c in [p1, span, p2] {
            t.append_child(div, c);
        }
        (t, [div, p1, span, p2])
    }

    fn all(t: &DomTree, sel: &str) -> Vec<NodeId> {
        SelectorList::parse(sel).unwrap_or_else(|| panic!("parse {sel}")).query_all(t, t.root())
    }

    #[test]
    fn matching() {
        let (t, [div, p1, span, p2]) = tree();
        assert_eq!(all(&t, "p"), vec![p1, p2]);
        assert_eq!(all(&t, "#a > .x"), vec![span]);
        assert_eq!(all(&t, ".x"), vec![div, span]);
        assert_eq!(all(&t, ".x.y"), vec![div]);
        assert_eq!(all(&t, "div p"), vec![p1, p2]);
        assert_eq!(all(&t, "p + span"), vec![span]);
        assert_eq!(all(&t, "p ~ p"), vec![p2]);
        assert_eq!(all(&t, "p:first-child, p:last-child"), vec![p1, p2]);
        assert_eq!(all(&t, ":nth-child(2)"), vec![span]);
        assert_eq!(all(&t, "div > :nth-child(odd)"), vec![p1, p2]);
        assert_eq!(all(&t, "p:nth-of-type(2)"), vec![p2]);
        assert_eq!(all(&t, "[lang|=en]"), vec![span]);
        assert_eq!(all(&t, "[class~=y]"), vec![div]);
        assert_eq!(all(&t, "span[lang^='en' i]"), vec![span]);
        assert_eq!(all(&t, "div :not(p)"), vec![span]);
        assert_eq!(all(&t, "div:has(> span)"), vec![div]);
        assert_eq!(all(&t, "*:hover"), Vec::<NodeId>::new());
        assert_eq!(
            SelectorList::parse("#a .x, ul > li.y.z, p, *:hover").unwrap().subject_keys(),
            vec![Some(SubjectKey::Class("x".into())), Some(SubjectKey::Class("y".into())), Some(SubjectKey::Tag("p".into())), None]
        );
        assert_eq!(
            SelectorList::parse(":where(.a, b) > i, :is(.c, :hover)").unwrap().subject_keys(),
            vec![Some(SubjectKey::Tag("i".into())), None]
        );
        assert_eq!(
            SelectorList::parse(":where(.a, b):hover, [data-x]").unwrap().subject_keys(),
            vec![Some(SubjectKey::Class("a".into())), Some(SubjectKey::Tag("b".into())), Some(SubjectKey::Attr("data-x".into()))]
        );
        let anc = |s: &str| SelectorList::parse(s).unwrap().ancestor_hashes()[0].clone();
        let mut want = vec![key_hash(KEY_CLASS, "a"), key_hash(KEY_ID, "m"), key_hash(KEY_TAG, "ul")];
        want.sort_unstable();
        assert_eq!(anc("#m.a ul > li span"), {
            let mut w = want.clone();
            w.push(key_hash(KEY_TAG, "li"));
            w.sort_unstable();
            w
        });
        // Past a sibling combinator nothing is known about ancestors
        assert_eq!(anc(".a + .b .c"), vec![key_hash(KEY_CLASS, "b")]);
        let spec = |s: &str| SelectorList::parse(s).unwrap().specificities()[0];
        assert_eq!(spec("#a .x > p:first-child"), (1, 2, 1));
        assert_eq!(spec("ul li a[href]::before"), (0, 1, 4));
        assert_eq!(spec(":is(#a, .b) span"), (1, 0, 1));
        assert_eq!(spec(":where(#a, .b) span"), (0, 0, 1));
        assert_eq!(spec("*"), (0, 0, 0));
        assert!(SelectorList::parse("div >").is_none());
        assert!(SelectorList::parse("..x").is_none());
    }
}
