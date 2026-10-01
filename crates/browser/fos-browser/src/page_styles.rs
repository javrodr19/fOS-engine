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

use fos_css::computed::ComputedStyle;
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
                        None => styles.universal.push(idx),
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
        (sum(&self.by_id), sum(&self.by_class), sum(&self.by_tag), self.universal.len())
    }

    /// Number of rules
    pub fn rule_count(&self) -> usize {
        self.stylesheet.rules.len()
    }

    /// Apply the declarations of the rules matching element `node`;
    /// `filter`, if given, holds the element's ancestors
    pub fn apply(&self, tree: &DomTree, node: NodeId, element: &ElementData, style: &mut ComputedStyle, filter: Option<&AncestorFilter>) {
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
        candidates.extend_from_slice(&self.universal);
        if candidates.is_empty() {
            return;
        }
        // A selector is filed once, but an element may repeat a class
        candidates.sort_unstable();
        candidates.dedup();

        let mut matched: Vec<(Specificity, u32)> = candidates
            .into_iter()
            .map(|i| &self.selectors[i as usize])
            .filter(|c| filter.is_none_or(|f| c.ancestors.iter().all(|&h| f.may_contain(h))))
            .filter(|c| c.selector.matches(tree, node))
            .map(|c| (c.specificity, c.rule))
            .collect();
        if matched.is_empty() {
            return;
        }
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
        for important in [false, true] {
            for &rule in &rules {
                for decl in &self.stylesheet.rules[rule as usize].declarations {
                    if decl.important == important {
                        style.apply_declaration(decl);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fos_css::parse_stylesheet;

    fn style_of(css: &str, html: &str, id: &str) -> ComputedStyle {
        let doc = fos_html::parse(html);
        let styles = PageStyles::new(parse_stylesheet(css).unwrap());
        let node = doc.get_element_by_id(id).unwrap();
        let element = doc.tree().get(node).unwrap().as_element().unwrap();
        let mut style = ComputedStyle::default();
        styles.apply(doc.tree(), node, element, &mut style, None);
        // Same result through an ancestor filter
        let mut filter = AncestorFilter::default();
        let mut chain = vec![];
        let mut p = doc.tree().get(node).unwrap().parent;
        while p.is_valid() {
            chain.push(p);
            p = doc.tree().get(p).unwrap().parent;
        }
        for &a in chain.iter().rev() {
            filter.push(doc.tree(), a);
        }
        let mut filtered = ComputedStyle::default();
        styles.apply(doc.tree(), node, element, &mut filtered, Some(&filter));
        assert_eq!(filtered.font_size, style.font_size);
        style
    }

    const HTML: &str = r#"<html><body><nav><a id="in" class="l">x</a></nav><a id="out" class="l">y</a><p id="p" class="a b">z</p></body></html>"#;

    #[test]
    fn combinators_and_pseudo_classes() {
        let css = "nav a { font-size: 30px } a:hover { font-size: 50px } p::before { font-size: 60px }";
        assert_eq!(style_of(css, HTML, "in").font_size, 30.0);
        assert_ne!(style_of(css, HTML, "out").font_size, 30.0);
        assert_ne!(style_of(css, HTML, "out").font_size, 50.0);
        assert_ne!(style_of(css, HTML, "p").font_size, 60.0);
    }

    #[test]
    fn cascade_order() {
        // Specificity beats source order
        let css = "#p { font-size: 20px } .a { font-size: 10px } p { font-size: 5px }";
        assert_eq!(style_of(css, HTML, "p").font_size, 20.0);
        // Equal specificity: the later rule wins
        let css = ".a { font-size: 10px } .b { font-size: 12px }";
        assert_eq!(style_of(css, HTML, "p").font_size, 12.0);
        // !important beats specificity
        let css = "p { font-size: 7px !important } #p { font-size: 20px }";
        assert_eq!(style_of(css, HTML, "p").font_size, 7.0);
    }
}
