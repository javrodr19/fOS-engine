//! Parallel Style Resolution
//!
//! Concurrent style resolution for DOM trees using custom parallel primitives.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

/// Element identifier in the style tree
pub type ElementId = usize;

/// Matched selector with source info
#[derive(Debug, Clone)]
pub struct MatchedSelector {
    /// Specificity (a, b, c)
    pub specificity: (u32, u32, u32),
    /// Source order for cascade
    pub source_order: u32,
    /// Declarations
    pub declarations: Vec<(String, String, bool)>, // (property, value, important)
}

/// Style context for an element
#[derive(Debug, Clone, Default)]
pub struct ElementStyleContext {
    /// Element ID
    pub id: ElementId,
    /// Tag name
    pub tag_name: String,
    /// Element classes
    pub classes: Vec<String>,
    /// Element ID attribute
    pub element_id: Option<String>,
    /// Other attributes
    pub attributes: HashMap<String, String>,
    /// Parent element ID
    pub parent_id: Option<ElementId>,
    /// Is in shadow DOM
    pub in_shadow: bool,
    /// Has custom properties from parent
    pub has_inherited_custom_props: bool,
    /// Is affected by :has() selectors
    pub affected_by_has: bool,
}

/// Computed style for an element
#[derive(Debug, Clone, Default)]
pub struct ComputedElementStyle {
    /// Element ID
    pub element_id: ElementId,
    /// Computed properties
    pub properties: HashMap<String, String>,
    /// Used values (after cascade)
    pub used_values: HashMap<String, String>,
}

/// Style tree for parallel resolution
pub struct StyleTree {
    /// All elements
    elements: Vec<ElementStyleContext>,
    /// Root element IDs
    roots: Vec<ElementId>,
    /// Children map
    children: HashMap<ElementId, Vec<ElementId>>,
}

impl StyleTree {
    /// Create new style tree
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            roots: Vec::new(),
            children: HashMap::new(),
        }
    }
    
    /// Add an element
    pub fn add_element(&mut self, ctx: ElementStyleContext) -> ElementId {
        let id = self.elements.len();
        
        if ctx.parent_id.is_none() {
            self.roots.push(id);
        } else if let Some(parent_id) = ctx.parent_id {
            self.children.entry(parent_id).or_default().push(id);
        }
        
        self.elements.push(ctx);
        id
    }
    
    /// Get element by ID
    pub fn get(&self, id: ElementId) -> Option<&ElementStyleContext> {
        self.elements.get(id)
    }
    
    /// Get all elements
    pub fn elements(&self) -> &[ElementStyleContext] {
        &self.elements
    }
    
    /// Get children of an element
    pub fn children(&self, id: ElementId) -> &[ElementId] {
        self.children.get(&id).map(|v| v.as_slice()).unwrap_or(&[])
    }
    
    /// Get roots
    pub fn roots(&self) -> &[ElementId] {
        &self.roots
    }
    
    /// Number of elements
    pub fn len(&self) -> usize {
        self.elements.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

impl Default for StyleTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Stylesheet rules for matching
pub struct StyleRules {
    /// All rules with selectors
    rules: Vec<StyleRuleEntry>,
}

/// A single style rule
#[derive(Debug, Clone)]
pub struct StyleRuleEntry {
    /// Selector text
    pub selector: String,
    /// Specificity
    pub specificity: (u32, u32, u32),
    /// Source order
    pub source_order: u32,
    /// Declarations
    pub declarations: Vec<(String, String, bool)>,
}

impl StyleRules {
    /// Create new style rules
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }
    
    /// Add a rule
    pub fn add_rule(&mut self, rule: StyleRuleEntry) {
        self.rules.push(rule);
    }
    
    /// Get all rules
    pub fn rules(&self) -> &[StyleRuleEntry] {
        &self.rules
    }
}

impl Default for StyleRules {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve styles for a tree in parallel
pub fn resolve_styles_parallel(
    tree: &StyleTree,
    rules: &StyleRules,
) -> Vec<ComputedElementStyle> {
    let num_elements = tree.len();
    if num_elements == 0 {
        return Vec::new();
    }
    
    let num_threads = std::thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);
    
    // Phase 1: Match selectors in parallel (read-only)
    let matches = match_selectors_parallel(tree, rules, num_threads);
    
    // Phase 2: Cascade (parallel per subtree for inherited props)
    let computed = cascade_parallel(tree, &matches, num_threads);
    
    computed
}

/// Match selectors for all elements in parallel
fn match_selectors_parallel(
    tree: &StyleTree,
    rules: &StyleRules,
    num_threads: usize,
) -> Vec<Vec<MatchedSelector>> {
    let num_elements = tree.len();
    let results = Arc::new(Mutex::new(vec![Vec::new(); num_elements]));
    
    if num_elements <= num_threads {
        // Sequential for small trees
        for (id, elem) in tree.elements().iter().enumerate() {
            let matched = match_selectors_for_element(elem, rules);
            results.lock().unwrap()[id] = matched;
        }
    } else {
        // Parallel matching
        let chunk_size = (num_elements + num_threads - 1) / num_threads;
        let elements: Vec<_> = tree.elements().iter().enumerate().collect();
        
        std::thread::scope(|s| {
            for chunk in elements.chunks(chunk_size) {
                let results = Arc::clone(&results);
                let chunk: Vec<_> = chunk.iter().map(|(i, e)| (*i, (*e).clone())).collect();
                
                s.spawn(move || {
                    for (id, elem) in chunk {
                        let matched = match_selectors_for_element(&elem, rules);
                        results.lock().unwrap()[id] = matched;
                    }
                });
            }
        });
    }
    
    Arc::try_unwrap(results).unwrap().into_inner().unwrap()
}

/// Match selectors for a single element
fn match_selectors_for_element(
    elem: &ElementStyleContext,
    rules: &StyleRules,
) -> Vec<MatchedSelector> {
    let mut matched = Vec::new();
    
    for rule in rules.rules() {
        if matches_selector(&rule.selector, elem) {
            matched.push(MatchedSelector {
                specificity: rule.specificity,
                source_order: rule.source_order,
                declarations: rule.declarations.clone(),
            });
        }
    }
    
    // Sort by specificity and source order
    matched.sort_by(|a, b| {
        match a.specificity.cmp(&b.specificity) {
            std::cmp::Ordering::Equal => a.source_order.cmp(&b.source_order),
            other => other,
        }
    });
    
    matched
}

/// Simple selector matching
fn matches_selector(selector: &str, elem: &ElementStyleContext) -> bool {
    let selector = selector.trim();
    
    // Simple matching for common cases
    // Tag name
    if selector.chars().all(|c| c.is_alphanumeric() || c == '-') {
        return elem.tag_name.eq_ignore_ascii_case(selector);
    }
    
    // Class selector
    if selector.starts_with('.') {
        let class = &selector[1..];
        return elem.classes.iter().any(|c| c.eq_ignore_ascii_case(class));
    }
    
    // ID selector
    if selector.starts_with('#') {
        let id = &selector[1..];
        return elem.element_id.as_ref().map(|i| i.eq_ignore_ascii_case(id)).unwrap_or(false);
    }
    
    // Universal selector
    if selector == "*" {
        return true;
    }
    
    // Compound selector (simple implementation)
    let mut parts = Vec::new();
    let mut current = String::new();
    
    for c in selector.chars() {
        if c == '.' || c == '#' {
            if !current.is_empty() {
                parts.push(current);
                current = String::new();
            }
            current.push(c);
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    
    for part in parts {
        if part.starts_with('.') {
            let class = &part[1..];
            if !elem.classes.iter().any(|c| c.eq_ignore_ascii_case(class)) {
                return false;
            }
        } else if part.starts_with('#') {
            let id = &part[1..];
            if !elem.element_id.as_ref().map(|i| i.eq_ignore_ascii_case(id)).unwrap_or(false) {
                return false;
            }
        } else if !part.is_empty() {
            if !elem.tag_name.eq_ignore_ascii_case(&part) {
                return false;
            }
        }
    }
    
    true
}

/// Cascade styles in parallel
fn cascade_parallel(
    tree: &StyleTree,
    matches: &[Vec<MatchedSelector>],
    num_threads: usize,
) -> Vec<ComputedElementStyle> {
    let num_elements = tree.len();
    let results = Arc::new(Mutex::new(vec![ComputedElementStyle::default(); num_elements]));
    
    // Process in level order to ensure parent styles are computed first
    // But elements at the same level can be computed in parallel
    
    let mut current_level: Vec<ElementId> = tree.roots().to_vec();
    
    while !current_level.is_empty() {
        let level_results = Arc::clone(&results);
        let level_matches: Vec<_> = current_level.iter()
            .map(|&id| (id, &matches[id], tree.get(id).and_then(|e| e.parent_id)))
            .collect();
        
        if current_level.len() <= num_threads {
            // Sequential for small levels
            for (id, matched, parent_id) in level_matches {
                let parent_style = parent_id.map(|p| {
                    level_results.lock().unwrap()[p].clone()
                });
                let computed = compute_style(id, matched, parent_style.as_ref());
                level_results.lock().unwrap()[id] = computed;
            }
        } else {
            // Parallel
            let chunk_size = (current_level.len() + num_threads - 1) / num_threads;
            
            std::thread::scope(|s| {
                for chunk in level_matches.chunks(chunk_size) {
                    let results = Arc::clone(&level_results);
                    let chunk: Vec<_> = chunk.to_vec();
                    
                    s.spawn(move || {
                        for (id, matched, parent_id) in chunk {
                            let parent_style = parent_id.map(|p| {
                                results.lock().unwrap()[p].clone()
                            });
                            let computed = compute_style(id, matched, parent_style.as_ref());
                            results.lock().unwrap()[id] = computed;
                        }
                    });
                }
            });
        }
        
        // Get next level (children of current level)
        let mut next_level = Vec::new();
        for id in current_level {
            next_level.extend(tree.children(id).iter().copied());
        }
        current_level = next_level;
    }
    
    Arc::try_unwrap(results).unwrap().into_inner().unwrap()
}

/// Compute style for a single element
fn compute_style(
    id: ElementId,
    matches: &[MatchedSelector],
    parent_style: Option<&ComputedElementStyle>,
) -> ComputedElementStyle {
    let mut properties = HashMap::new();
    
    // Apply matched rules in order
    for matched in matches {
        for (prop, value, important) in &matched.declarations {
            // Handle !important
            if *important {
                properties.insert(prop.clone(), format!("{}!important", value));
            } else if !properties.get(prop).map(|v: &String| v.ends_with("!important")).unwrap_or(false) {
                properties.insert(prop.clone(), value.clone());
            }
        }
    }
    
    // Inherit from parent
    if let Some(parent) = parent_style {
        for prop in INHERITED_PROPERTIES {
            if !properties.contains_key(*prop) {
                if let Some(value) = parent.properties.get(*prop) {
                    properties.insert((*prop).to_string(), value.clone());
                }
            }
        }
    }
    
    // Compute used values (resolve inherit, initial, etc.)
    let mut used_values = HashMap::new();
    for (prop, value) in &properties {
        let resolved = if value == "inherit" {
            parent_style
                .and_then(|p| p.used_values.get(prop))
                .cloned()
                .unwrap_or_else(|| get_initial_value(prop))
        } else if value == "initial" {
            get_initial_value(prop)
        } else if value == "unset" {
            if INHERITED_PROPERTIES.contains(&prop.as_str()) {
                parent_style
                    .and_then(|p| p.used_values.get(prop))
                    .cloned()
                    .unwrap_or_else(|| get_initial_value(prop))
            } else {
                get_initial_value(prop)
            }
        } else {
            value.trim_end_matches("!important").to_string()
        };
        
        used_values.insert(prop.clone(), resolved);
    }
    
    ComputedElementStyle {
        element_id: id,
        properties,
        used_values,
    }
}

/// Inherited CSS properties
const INHERITED_PROPERTIES: &[&str] = &[
    "color",
    "font-family",
    "font-size",
    "font-style",
    "font-weight",
    "line-height",
    "letter-spacing",
    "text-align",
    "text-indent",
    "text-transform",
    "white-space",
    "word-spacing",
    "direction",
    "visibility",
    "cursor",
    "list-style",
    "list-style-type",
    "list-style-position",
];

/// Get initial value for a property
fn get_initial_value(property: &str) -> String {
    match property {
        "color" => "canvastext".to_string(),
        "font-family" => "serif".to_string(),
        "font-size" => "medium".to_string(),
        "font-style" => "normal".to_string(),
        "font-weight" => "normal".to_string(),
        "line-height" => "normal".to_string(),
        "text-align" => "start".to_string(),
        "visibility" => "visible".to_string(),
        "display" => "inline".to_string(),
        "position" => "static".to_string(),
        "width" | "height" => "auto".to_string(),
        "margin" | "padding" => "0".to_string(),
        "background-color" => "transparent".to_string(),
        "opacity" => "1".to_string(),
        _ => "".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_matching() {
        let elem = ElementStyleContext {
            id: 0,
            tag_name: "div".to_string(),
            classes: vec!["container".to_string()],
            element_id: Some("main".to_string()),
            ..Default::default()
        };
        
        assert!(matches_selector("div", &elem));
        assert!(matches_selector(".container", &elem));
        assert!(matches_selector("#main", &elem));
        assert!(matches_selector("div.container", &elem));
        assert!(!matches_selector("span", &elem));
        assert!(!matches_selector(".other", &elem));
    }
    
    #[test]
    fn test_parallel_style_resolution() {
        let mut tree = StyleTree::new();
        let mut rules = StyleRules::new();
        
        // Add elements
        tree.add_element(ElementStyleContext {
            id: 0,
            tag_name: "div".to_string(),
            classes: vec!["root".to_string()],
            ..Default::default()
        });
        
        // Add rules
        rules.add_rule(StyleRuleEntry {
            selector: "div".to_string(),
            specificity: (0, 0, 1),
            source_order: 0,
            declarations: vec![("color".to_string(), "red".to_string(), false)],
        });
        
        let computed = resolve_styles_parallel(&tree, &rules);
        
        assert_eq!(computed.len(), 1);
        assert_eq!(
            computed[0].used_values.get("color"),
            Some(&"red".to_string())
        );
    }
    
    #[test]
    fn test_inheritance() {
        let mut tree = StyleTree::new();
        let mut rules = StyleRules::new();
        
        // Parent
        tree.add_element(ElementStyleContext {
            id: 0,
            tag_name: "div".to_string(),
            ..Default::default()
        });
        
        // Child
        tree.add_element(ElementStyleContext {
            id: 1,
            tag_name: "span".to_string(),
            parent_id: Some(0),
            ..Default::default()
        });
        
        // Rule on parent
        rules.add_rule(StyleRuleEntry {
            selector: "div".to_string(),
            specificity: (0, 0, 1),
            source_order: 0,
            declarations: vec![("color".to_string(), "blue".to_string(), false)],
        });
        
        let computed = resolve_styles_parallel(&tree, &rules);
        
        // Child should inherit color
        assert_eq!(
            computed[1].used_values.get("color"),
            Some(&"blue".to_string())
        );
    }
}
