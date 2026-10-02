//! A page's style rules, compiled for matching
//!
//! Every selector is parsed once into a `fos_dom::SelectorList` (full CSS
//! matching: combinators, attributes, structural pseudo-classes) and filed
//! under the most selective part of its subject: its id, first class or
//! tag. An element is then tested only against the rules filed under its
//! own id, classes and tag, plus the few that can match anything, which
//! is how browsers keep style matching fast on pages with thousands of
//! rules.
//!
//! Matching declarations apply in cascade order: by specificity, then by
//! source order, with `!important` declarations after all normal ones.

use std::collections::HashMap;

use std::sync::LazyLock;

use fos_css::style::{Style, StyleContext};
use fos_css::{Specificity, Stylesheet};
use fos_dom::{DomTree, ElementData, NodeId, SelectorList, SubjectKey};

struct CompiledSelector {
    selector: SelectorList,
    specificity: Specificity,
    /// Index of the rule in the stylesheet (also its source order)
    rule: u32,
    /// Key hashes the element's ancestors must carry
    ancestors: Box<[u32]>,
}

/// A counting Bloom filter of the ids, classes and tags of the ancestors of
/// the element being styled, maintained while walking down the tree.
/// Selectors needing an ancestor key the filter lacks are rejected without
/// walking up the tree, which is most of them.
pub struct AncestorFilter {
    counts: Box<[u8; 4096]>,
    /// Hashes pushed for each open ancestor, and where each one starts
    hashes: Vec<u32>,
    starts: Vec<usize>,
}

impl Default for AncestorFilter {
    fn default() -> Self {
        Self { counts: Box::new([0; 4096]), hashes: Vec::new(), starts: Vec::new() }
    }
}

impl AncestorFilter {
    fn slots(h: u32) -> [usize; 2] {
        [(h & 0xfff) as usize, ((h >> 12) & 0xfff) as usize]
    }

    /// Enter element `node` (its descendants are styled next)
    pub fn push(&mut self, tree: &DomTree, node: NodeId) {
        self.starts.push(self.hashes.len());
        let Some(e) = tree.get(node).and_then(|n| n.as_element()) else { return };
        let start = self.hashes.len();
        if let Some(id) = e.id {
            self.hashes.push(fos_dom::key_hash(fos_dom::KEY_ID, tree.resolve(id)));
        }
        for &c in &e.classes {
            self.hashes.push(fos_dom::key_hash(fos_dom::KEY_CLASS, tree.resolve(c)));
        }
        self.hashes.push(fos_dom::key_hash(fos_dom::KEY_TAG, tree.resolve(e.name.local)));
        for &h in &self.hashes[start..] {
            for s in Self::slots(h) {
                // Saturated counters stay (never a false "absent")
                self.counts[s] = self.counts[s].saturating_add(1);
            }
        }
    }

    /// Leave the element entered last
    pub fn pop(&mut self) {
        let Some(start) = self.starts.pop() else { return };
        for h in self.hashes.drain(start..) {
            for s in Self::slots(h) {
                if self.counts[s] != u8::MAX {
                    self.counts[s] -= 1;
                }
            }
        }
    }

    fn may_contain(&self, h: u32) -> bool {
        Self::slots(h).iter().all(|&s| self.counts[s] > 0)
    }
}

/// Style rules ready for matching
pub struct PageStyles {
    stylesheet: Stylesheet,
    selectors: Vec<CompiledSelector>,
    by_id: HashMap<String, Vec<u32>>,
    by_class: HashMap<String, Vec<u32>>,
    by_tag: HashMap<String, Vec<u32>>,
    /// Selectors whose subject needs an attribute (`[data-x]`, ...)
    by_attr: HashMap<String, Vec<u32>>,
    /// Selectors that can match any element (`*`, `:hover`, `[attr]`, ...)
    universal: Vec<u32>,
}

impl PageStyles {
    pub fn new(stylesheet: Stylesheet) -> Self {
        let mut styles = PageStyles {
            selectors: Vec::new(),
            by_id: HashMap::new(),
            by_class: HashMap::new(),
            by_tag: HashMap::new(),
            by_attr: HashMap::new(),
            universal: Vec::new(),
            stylesheet: Stylesheet { rules: Vec::new() },
        };
        let mut skipped = 0;
        for (rule_index, rule) in stylesheet.rules.iter().enumerate() {
            if rule.declarations.is_empty() {
                continue;
            }
            for selector in &rule.selectors {
                // Browsers drop selectors they cannot parse
                let Some(list) = SelectorList::parse(&selector.text) else {
                    skipped += 1;
                    continue;
                };
                // A list of several complex selectors keeps no ancestor
                // requirement (each alternative needs different ancestors)
                let ancestors = match list.ancestor_hashes().as_slice() {
                    [one] => one.clone().into_boxed_slice(),
                    _ => Box::default(),
                };
                for key in list.subject_keys() {
                    let idx = styles.selectors.len() as u32;
                    match key {
                        Some(SubjectKey::Id(id)) => styles.by_id.entry(id).or_default().push(idx),
                        Some(SubjectKey::Class(c)) => styles.by_class.entry(c).or_default().push(idx),
                        Some(SubjectKey::Tag(t)) => styles.by_tag.entry(t).or_default().push(idx),
                        Some(SubjectKey::Attr(a)) => styles.by_attr.entry(a).or_default().push(idx),
                        None => {
                            log::trace!("universal selector: {}", selector.text);
                            styles.universal.push(idx)
                        }
                    }
                }
                styles.selectors.push(CompiledSelector {
                    selector: list,
                    specificity: selector.specificity,
                    rule: rule_index as u32,
                    ancestors,
                });
            }
        }
        if skipped > 0 {
            log::debug!("Skipped {skipped} unsupported selectors");
        }
        styles.stylesheet = stylesheet;
        styles
    }

    /// Selectors per bucket kind (id, class, tag, universal), for tuning
    pub fn bucket_sizes(&self) -> (usize, usize, usize, usize) {
        let sum = |m: &HashMap<String, Vec<u32>>| m.values().map(Vec::len).sum();
        (sum(&self.by_id), sum(&self.by_class), sum(&self.by_tag) + sum(&self.by_attr), self.universal.len())
    }

    /// Number of rules
    pub fn rule_count(&self) -> usize {
        self.stylesheet.rules.len()
    }

    /// Style element `node` with these rules alone (plus the UA's), as a
    /// child of `parent`; `filter`, if given, holds the element's ancestors
    pub fn style(&self, tree: &DomTree, node: NodeId, element: &ElementData, parent: &Style, filter: Option<&AncestorFilter>) -> Style {
        let mut style = Style::inherit_from(parent);
        cascade(Some(self), tree, node, element, filter, &[], &mut style, parent, &StyleContext::default(), &mut Default::default());
        style
    }

    /// The rules matching element `node`, lowest priority first (each once)
    fn matching_rules(&self, tree: &DomTree, node: NodeId, element: &ElementData, filter: Option<&AncestorFilter>) -> Vec<u32> {
        let mut candidates: Vec<u32> = Vec::new();
        if let Some(id) = element.id {
            if let Some(list) = self.by_id.get(tree.resolve(id)) {
                candidates.extend_from_slice(list);
            }
        }
        for &class in &element.classes {
            if let Some(list) = self.by_class.get(tree.resolve(class)) {
                candidates.extend_from_slice(list);
            }
        }
        let tag = tree.resolve(element.name.local);
        let tag_list = if tag.bytes().any(|b| b.is_ascii_uppercase()) {
            self.by_tag.get(&tag.to_ascii_lowercase())
        } else {
            self.by_tag.get(tag)
        };
        if let Some(list) = tag_list {
            candidates.extend_from_slice(list);
        }
        if !self.by_attr.is_empty() {
            for a in &element.attrs {
                if let Some(list) = self.by_attr.get(tree.resolve(a.name.local)) {
                    candidates.extend_from_slice(list);
                }
            }
        }
        if candidates.is_empty() && self.universal.is_empty() {
            return Vec::new();
        }
        // A selector list may be filed under several keys, and an element
        // may repeat a class: each selector is tried once. The universal
        // list is long and already sorted, so it is merged in unsorted.
        candidates.sort_unstable();
        candidates.dedup();
        let universal = self.universal.iter().copied().filter(|i| candidates.binary_search(i).is_err());
        let mut matched: Vec<(Specificity, u32)> = candidates
            .iter()
            .copied()
            .chain(universal)
            .map(|i| &self.selectors[i as usize])
            .filter(|c| filter.is_none_or(|f| c.ancestors.iter().all(|&h| f.may_contain(h))))
            .filter(|c| c.selector.matches(tree, node))
            .map(|c| (c.specificity, c.rule))
            .collect();
        matched.sort_unstable();
        // Several selectors of one rule may match: the rule applies once,
        // at its highest specificity
        let mut rules: Vec<u32> = Vec::with_capacity(matched.len());
        for &(_, rule) in matched.iter().rev() {
            if !rules.contains(&rule) {
                rules.push(rule);
            }
        }
        rules.reverse();
        rules
    }
}

/// The browser's default styles (`ua.css`), compiled once
static UA: LazyLock<PageStyles> = LazyLock::new(|| PageStyles::new(fos_css::parse_stylesheet(include_str!("ua.css")).unwrap_or(Stylesheet { rules: Vec::new() })));

/// Presentational attributes (`bgcolor`, `align`, `width`, ...) as
/// declarations, which rank just above the UA's styles
fn presentational_hints(tree: &DomTree, element: &ElementData) -> Vec<fos_css::Declaration> {
    let tag = tree.resolve(element.name.local);
    let relevant = matches!(
        tag,
        "body" | "table" | "tr" | "td" | "th" | "font" | "img" | "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "caption" | "canvas" | "video"
            | "iframe" | "embed" | "object" | "col" | "hr" | "thead" | "tbody" | "tfoot" | "input" | "textarea" | "select" | "legend"
    );
    if !relevant || element.attrs.is_empty() {
        return Vec::new();
    }
    // A dimension attribute: a number of pixels or a percentage
    let dimension = |v: &str| -> Option<String> {
        let v = v.trim();
        let end = v.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(v.len());
        let n: f32 = v[..end].parse().ok()?;
        Some(if v[end..].starts_with('%') { format!("{n}%") } else { format!("{n}px") })
    };
    let mut css = String::new();
    for a in element.attrs.iter() {
        let value = a.value.trim();
        match (tree.resolve(a.name.local), tag) {
            ("bgcolor", "body" | "table" | "tr" | "td" | "th" | "thead" | "tbody" | "tfoot") => css += &format!("background-color: {value};"),
            ("text", "body") | ("color", "font" | "hr") => css += &format!("color: {value};"),
            ("face", "font") => css += &format!("font-family: {value};"),
            ("size", "font") => {
                let sizes = ["x-small", "small", "medium", "large", "x-large", "xx-large", "xxx-large"];
                let n = match value.strip_prefix('+') {
                    Some(r) => r.parse::<i32>().ok().map(|n| 3 + n),
                    None => match value.strip_prefix('-') {
                        Some(r) => r.parse::<i32>().ok().map(|n| 3 - n),
                        None => value.parse::<i32>().ok(),
                    },
                };
                if let Some(n) = n {
                    css += &format!("font-size: {};", sizes[(n.clamp(1, 7) - 1) as usize]);
                }
            }
            ("width", "img" | "canvas" | "video" | "iframe" | "embed" | "object" | "table" | "td" | "th" | "col" | "hr" | "input") => {
                if let Some(d) = dimension(value) {
                    css += &format!("width: {d};");
                }
            }
            ("height", "img" | "canvas" | "video" | "iframe" | "embed" | "object" | "table" | "td" | "th" | "tr") => {
                if let Some(d) = dimension(value) {
                    css += &format!("height: {d};");
                }
            }
            ("align", "table") => match value.to_ascii_lowercase().as_str() {
                "left" => css += "float: left;",
                "right" => css += "float: right;",
                "center" => css += "margin-left: auto; margin-right: auto;",
                _ => {}
            },
            ("align", "img" | "iframe" | "embed" | "object" | "input") => match value.to_ascii_lowercase().as_str() {
                "left" => css += "float: left;",
                "right" => css += "float: right;",
                "middle" | "absmiddle" | "center" => css += "vertical-align: middle;",
                "top" => css += "vertical-align: top;",
                "bottom" | "baseline" => css += "vertical-align: baseline;",
                _ => {}
            },
            ("align", _) => {
                let v = value.to_ascii_lowercase();
                if matches!(v.as_str(), "left" | "right" | "center" | "justify") {
                    css += &format!("text-align: {v};");
                }
            }
            ("valign", "td" | "th" | "tr" | "thead" | "tbody" | "tfoot") => {
                let v = value.to_ascii_lowercase();
                if matches!(v.as_str(), "top" | "middle" | "bottom" | "baseline") {
                    css += &format!("vertical-align: {v};");
                }
            }
            ("border", "table" | "img" | "object") => {
                let n = value.parse::<f32>().unwrap_or(if value.is_empty() { 1.0 } else { 0.0 });
                css += &format!("border: {n}px {} gray;", if tag == "table" { "outset" } else { "solid" });
            }
            ("cellspacing", "table") => {
                if let Some(d) = dimension(value) {
                    css += &format!("border-spacing: {d};");
                }
            }
            ("nowrap", "td" | "th") => css += "white-space: nowrap;",
            ("hspace", "img") => {
                if let Some(d) = dimension(value) {
                    css += &format!("margin-left: {d}; margin-right: {d};");
                }
            }
            ("vspace", "img") => {
                if let Some(d) = dimension(value) {
                    css += &format!("margin-top: {d}; margin-bottom: {d};");
                }
            }
            ("noshade", "hr") => css += "border-style: solid; background-color: gray;",
            ("size", "hr") => {
                if let Ok(n) = value.parse::<f32>() {
                    css += &format!("height: {}px;", (n - 2.0).max(0.0));
                }
            }
            _ => {}
        }
    }
    if css.is_empty() {
        Vec::new()
    } else {
        fos_css::parse_declarations(&css)
    }
}

/// Style element `node`, starting from `style` (its inherited values):
/// the UA's rules, presentational hints, the page's matching rules (if
/// there is a stylesheet) and the `style` attribute's declarations
/// (`inline`), in cascade order: normal declarations by origin,
/// specificity and source order (inline above rules), then `!important`
/// ones with the origins reversed. `parent` is what `inherit` refers to;
/// custom properties, `var()` and math functions are computed against the
/// element and `ctx`.
#[allow(clippy::too_many_arguments)]
pub fn cascade(
    styles: Option<&PageStyles>,
    tree: &DomTree,
    node: NodeId,
    element: &ElementData,
    filter: Option<&AncestorFilter>,
    inline: &[fos_css::Declaration],
    style: &mut Style,
    parent: &Style,
    ctx: &StyleContext,
    cache: &mut fos_css::ResolveCache,
) {
    let ua = &*UA;
    let ua_rules = ua.matching_rules(tree, node, element, filter);
    let hints = presentational_hints(tree, element);
    let rules = styles.map_or(Vec::new(), |s| s.matching_rules(tree, node, element, filter));
    if ua_rules.is_empty() && hints.is_empty() && rules.is_empty() && inline.is_empty() {
        style.finish();
        return;
    }
    let mut ordered: Vec<&fos_css::Declaration> = Vec::new();
    fn of(s: &PageStyles, rule: u32, important: bool) -> impl Iterator<Item = &fos_css::Declaration> {
        s.stylesheet.rules[rule as usize].declarations.iter().filter(move |d| d.important == important)
    }
    for &rule in &ua_rules {
        ordered.extend(of(ua, rule, false));
    }
    ordered.extend(hints.iter());
    if let Some(s) = styles {
        for &rule in &rules {
            ordered.extend(of(s, rule, false));
        }
    }
    ordered.extend(inline.iter().filter(|d| !d.important));
    if let Some(s) = styles {
        for &rule in &rules {
            ordered.extend(of(s, rule, true));
        }
    }
    ordered.extend(inline.iter().filter(|d| d.important));
    for &rule in &ua_rules {
        ordered.extend(of(ua, rule, true));
    }
    style.cascade(&ordered, parent, ctx, cache);
}

#[cfg(test)]
mod tests {
    use super::*;
    use fos_css::parse_stylesheet;
    use fos_css::style::{Display, Lp, LpAuto};

    /// Style an element as layout does: its ancestors first, down the tree
    fn style_with(css: &str, html: &str, id: &str, filtered: bool) -> Style {
        let doc = fos_html::parse(html);
        let styles = PageStyles::new(parse_stylesheet(css).unwrap());
        let tree = doc.tree();
        let target = doc.get_element_by_id(id).unwrap();
        let mut chain = vec![target];
        while let Some(p) = tree.get(*chain.last().unwrap()).map(|n| n.parent).filter(|p| tree.get(*p).is_some_and(|n| n.is_element())) {
            chain.push(p);
        }
        let mut filter = AncestorFilter::default();
        let mut parent = Style::default();
        for &n in chain.iter().rev() {
            let e = tree.get(n).unwrap().as_element().unwrap();
            parent = styles.style(tree, n, e, &parent, filtered.then_some(&filter));
            filter.push(tree, n);
        }
        parent
    }

    fn style_of(css: &str, html: &str, id: &str) -> Style {
        let s = style_with(css, html, id, false);
        // Same result through an ancestor filter
        assert!(style_with(css, html, id, true) == s);
        s
    }

    const HTML: &str = r#"<html><body><nav><a id="in" class="l">x</a></nav><a id="out" class="l">y</a><p id="p" class="a b">z</p></body></html>"#;

    #[test]
    fn combinators_and_pseudo_classes() {
        let css = "nav a { font-size: 30px } a:hover { font-size: 50px } p::before { font-size: 60px }";
        assert_eq!(style_of(css, HTML, "in").font_size(), 30.0);
        assert_ne!(style_of(css, HTML, "out").font_size(), 30.0);
        assert_ne!(style_of(css, HTML, "out").font_size(), 50.0);
        assert_ne!(style_of(css, HTML, "p").font_size(), 60.0);
    }

    #[test]
    fn custom_properties_and_calc() {
        let css = ":root { --big: 30px; --unit: 4px } #p { --big: 40px; font-size: var(--big) } .a { margin-top: calc(var(--unit) * 3) } #in { font-size: calc(1em + var(--unit)) } #out { width: calc(100% - 2em) }";
        let html = r#"<html><body><nav><a id="in" class="l">x</a></nav><a id="out">y</a><p id="p" class="a b">z</p></body></html>"#;
        assert_eq!(style_of(css, html, "p").font_size(), 40.0);
        assert_eq!(style_of(css, html, "in").font_size(), 20.0);
        assert_eq!(style_of(css, html, "p").box_.margin[0], LpAuto::Lp(Lp::px(12.0)));
        assert_eq!(style_of(css, html, "out").box_.width, LpAuto::Lp(Lp { px: -32.0, pct: 100.0 }));
    }

    #[test]
    fn cascade_order() {
        // Specificity beats source order
        let css = "#p { font-size: 20px } .a { font-size: 10px } p { font-size: 5px }";
        assert_eq!(style_of(css, HTML, "p").font_size(), 20.0);
        // Equal specificity: the later rule wins
        let css = ".a { font-size: 10px } .b { font-size: 12px }";
        assert_eq!(style_of(css, HTML, "p").font_size(), 12.0);
        // !important beats specificity
        let css = "p { font-size: 7px !important } #p { font-size: 20px }";
        assert_eq!(style_of(css, HTML, "p").font_size(), 7.0);
    }

    #[test]
    fn user_agent_styles_and_hints() {
        let html = r#"<html><body><p id="p">x</p><h1 id="h">y</h1><ul><li id="li">z</li></ul><span id="s">s</span><div id="d" hidden>q</div>
            <table id="t" width="50%" bgcolor="red" align="center"><tr><td id="td" nowrap>1</td></tr></table><font id="f" color="blue" size="5">f</font></body></html>"#;
        // Page rules override the UA's
        let p = style_of("p { margin-top: 3px }", html, "p");
        assert_eq!((p.display(), p.box_.margin[0], p.box_.margin[2]), (Display::Block, LpAuto::Lp(Lp::px(3.0)), LpAuto::Lp(Lp::px(16.0))));
        let h = style_of("", html, "h");
        assert_eq!((h.font_size(), h.inherited.font_weight), (32.0, 700));
        assert_eq!(style_of("", html, "li").display(), Display::ListItem);
        assert_eq!(style_of("", html, "s").display(), Display::Inline);
        assert_eq!(style_of("", html, "d").display(), Display::None);
        let t = style_of("", html, "t");
        assert_eq!((t.display(), t.box_.width, t.box_.margin[1]), (Display::Table, LpAuto::Lp(Lp::pct(50.0)), LpAuto::Auto));
        assert_eq!(t.background.color, fos_css::properties::Color::rgb(255, 0, 0));
        assert_eq!(style_of("", html, "td").inherited.white_space, fos_css::style::WhiteSpace::Nowrap);
        let f = style_of("", html, "f");
        assert_eq!((f.color(), f.font_size()), (fos_css::properties::Color::rgb(0, 0, 255), 24.0));
        // Hints rank below the page's rules
        assert_eq!(style_of("table { width: 10px }", html, "t").box_.width, LpAuto::Lp(Lp::px(10.0)));
    }
}
