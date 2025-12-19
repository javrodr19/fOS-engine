//! Style Cascade & Resolver
//!
//! Computes the final styles for DOM elements by:
//! 1. Matching selectors against elements
//! 2. Sorting by specificity and source order
//! 3. Applying cascade rules (importance, origin)

use crate::{Stylesheet, Rule, Selector, SelectorPart, Combinator, Declaration, Specificity};
use crate::properties::{PropertyId, PropertyValue};
use crate::computed::ComputedStyle;
use fos_dom::{Document, NodeId, DomTree};

/// Style resolver - computes styles for DOM elements
pub struct StyleResolver {
    /// User agent stylesheet (browser defaults)
    ua_styles: Stylesheet,
    /// Author stylesheets (page CSS)
    author_styles: Vec<Stylesheet>,
}

impl StyleResolver {
    pub fn new() -> Self {
        Self {
            ua_styles: Self::default_ua_styles(),
            author_styles: Vec::new(),
        }
    }
    
    /// Add an author stylesheet
    pub fn add_stylesheet(&mut self, stylesheet: Stylesheet) {
        self.author_styles.push(stylesheet);
    }
    
    /// Compute styles for an element
    pub fn compute_style(&self, tree: &DomTree, node_id: NodeId) -> ComputedStyle {
        let mut style = ComputedStyle::default();
        
        // Collect all matching rules with specificity
        let mut matches: Vec<(&Declaration, Specificity, usize)> = Vec::new();
        
        // Match UA styles first
        self.collect_matches(tree, node_id, &self.ua_styles, 0, &mut matches);
        
        // Then author styles (higher precedence)
        for (i, stylesheet) in self.author_styles.iter().enumerate() {
            self.collect_matches(tree, node_id, stylesheet, i + 1, &mut matches);
        }
        
        // Sort by specificity and source order
        matches.sort_by(|a, b| {
            // First compare by !important
            match (a.0.important, b.0.important) {
                (true, false) => std::cmp::Ordering::Greater,
                (false, true) => std::cmp::Ordering::Less,
                _ => {
                    // Then by specificity
                    match a.1.cmp(&b.1) {
                        std::cmp::Ordering::Equal => a.2.cmp(&b.2), // Source order
                        other => other,
                    }
                }
            }
        });
        
        // Apply declarations in order
        for (decl, _, _) in matches {
            style.apply_declaration(decl);
        }
        
        style
    }
    
    fn collect_matches<'a>(
        &self,
        tree: &DomTree,
        node_id: NodeId,
        stylesheet: &'a Stylesheet,
        source_order: usize,
        matches: &mut Vec<(&'a Declaration, Specificity, usize)>,
    ) {
        for rule in &stylesheet.rules {
            for selector in &rule.selectors {
                if self.matches_selector(tree, node_id, selector) {
                    for decl in &rule.declarations {
                        matches.push((decl, selector.specificity, source_order));
                    }
                }
            }
        }
    }
    
    /// Check if a selector matches an element
    fn matches_selector(&self, tree: &DomTree, node_id: NodeId, selector: &Selector) -> bool {
        // Simple selector matching - would be more complex for full CSS
        let node = match tree.get(node_id) {
            Some(n) => n,
            None => return false,
        };
        
        let elem = match node.as_element() {
            Some(e) => e,
            None => return false,
        };
        
        // Check each selector part
        for part in &selector.parts {
            let matches = match part {
                SelectorPart::Type(tag) => {
                    tree.resolve(elem.name.local) == tag.as_str()
                }
                SelectorPart::Class(class) => {
                    elem.classes.iter().any(|c| tree.resolve(*c) == class.as_str())
                }
                SelectorPart::Id(id) => {
                    elem.id.map(|i| tree.resolve(i) == id.as_str()).unwrap_or(false)
                }
                SelectorPart::Universal => true,
                SelectorPart::Attribute { name, op, value } => {
                    self.matches_attribute(tree, elem, name, *op, value)
                }
                SelectorPart::PseudoClass(pseudo) => {
                    self.matches_pseudo_class(tree, node_id, pseudo)
                }
                SelectorPart::Combinator(_) => {
                    // Combinators affect selector structure, handled separately
                    true
                }
                SelectorPart::PseudoElement(_) => {
                    // Pseudo-elements don't affect matching
                    true
                }
            };
            
            if !matches {
                return false;
            }
        }
        
        true
    }
    
    fn matches_attribute(
        &self,
        tree: &DomTree,
        elem: &fos_dom::ElementData,
        name: &str,
        op: crate::AttrOp,
        expected: &str,
    ) -> bool {
        let name_interned = tree.interner().intern_lookup(name);
        let name_interned = match name_interned {
            Some(n) => n,
            None => return false,
        };
        
        let actual = match elem.get_attr(name_interned) {
            Some(v) => v,
            None => return op == crate::AttrOp::Exists && expected.is_empty(),
        };
        
        match op {
            crate::AttrOp::Exists => true,
            crate::AttrOp::Equals => actual == expected,
            crate::AttrOp::Contains => actual.contains(expected),
            crate::AttrOp::StartsWith => actual.starts_with(expected),
            crate::AttrOp::EndsWith => actual.ends_with(expected),
            crate::AttrOp::Includes => actual.split_whitespace().any(|w| w == expected),
            crate::AttrOp::DashMatch => {
                actual == expected || actual.starts_with(&format!("{}-", expected))
            }
        }
    }
    
    fn matches_pseudo_class(&self, tree: &DomTree, node_id: NodeId, pseudo: &str) -> bool {
        let node = match tree.get(node_id) {
            Some(n) => n,
            None => return false,
        };
        
        match pseudo {
            "first-child" => node.prev_sibling == NodeId::NONE,
            "last-child" => node.next_sibling == NodeId::NONE,
            "only-child" => {
                node.prev_sibling == NodeId::NONE && node.next_sibling == NodeId::NONE
            }
            "empty" => node.first_child == NodeId::NONE,
            "root" => node.parent == NodeId::ROOT,
            // Other pseudo-classes would need more context (hover, focus, etc.)
            _ => false,
        }
    }
    
    /// Default user-agent styles
    fn default_ua_styles() -> Stylesheet {
        use crate::{Rule, Declaration};
        use crate::properties::{PropertyId, PropertyValue, Keyword};
        
        Stylesheet {
            rules: vec![
                // Block-level elements
                Rule {
                    selectors: vec![
                        Selector {
                            text: "div, p, h1, h2, h3, h4, h5, h6, ul, ol, li, form, header, footer, section, article, nav, aside, main".into(),
                            specificity: Specificity(0, 0, 1),
                            parts: vec![SelectorPart::Type("div".into())],
                        },
                    ],
                    declarations: vec![
                        Declaration {
                            property: PropertyId::Display,
                            value: PropertyValue::Keyword(Keyword::Block),
                            important: false,
                        },
                    ],
                },
                // Inline elements
                Rule {
                    selectors: vec![
                        Selector {
                            text: "span, a, strong, em, b, i, u".into(),
                            specificity: Specificity(0, 0, 1),
                            parts: vec![SelectorPart::Type("span".into())],
                        },
                    ],
                    declarations: vec![
                        Declaration {
                            property: PropertyId::Display,
                            value: PropertyValue::Keyword(Keyword::Inline),
                            important: false,
                        },
                    ],
                },
                // Hidden elements
                Rule {
                    selectors: vec![
                        Selector {
                            text: "head, script, style, link, meta, title".into(),
                            specificity: Specificity(0, 0, 1),
                            parts: vec![SelectorPart::Type("head".into())],
                        },
                    ],
                    declarations: vec![
                        Declaration {
                            property: PropertyId::Display,
                            value: PropertyValue::Keyword(Keyword::None),
                            important: false,
                        },
                    ],
                },
            ],
        }
    }
}

impl Default for StyleResolver {
    fn default() -> Self {
        Self::new()
    }
}
