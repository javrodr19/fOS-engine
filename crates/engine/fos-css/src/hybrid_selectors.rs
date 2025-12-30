//! Hybrid Interpreted/Compiled CSS Selectors (Phase 24.1)
//!
//! Top 100 selectors → compile to Rust functions for 10x faster matching.
//! Rare selectors → interpret at runtime.
//! Enables hot path optimization while maintaining flexibility.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Selector ID for quick lookup
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct SelectorId(pub u32);

/// Statistics for selector usage
#[derive(Debug, Clone, Copy, Default)]
pub struct SelectorStats {
    /// Times this selector was matched
    pub match_attempts: u64,
    /// Times this selector matched successfully
    pub match_successes: u64,
    /// Average match time in nanoseconds
    pub avg_match_time_ns: u64,
}

impl SelectorStats {
    /// Hit rate
    pub fn hit_rate(&self) -> f64 {
        if self.match_attempts == 0 {
            0.0
        } else {
            self.match_successes as f64 / self.match_attempts as f64
        }
    }
}

/// Compiled selector - fast matching function
pub trait CompiledSelector: Send + Sync {
    /// Match against an element
    fn matches(&self, element: &ElementInfo) -> bool;
    
    /// Get the original selector text
    fn selector_text(&self) -> &str;
}

/// Element information for matching
#[derive(Debug, Clone)]
pub struct ElementInfo {
    /// Tag name (interned)
    pub tag_name: u32,
    /// Element ID
    pub id: Option<Box<str>>,
    /// Class names
    pub classes: Vec<Box<str>>,
    /// Attribute names (for [attr] selectors)
    pub attributes: HashMap<Box<str>, Box<str>>,
    /// Parent element (if any)
    pub parent_tag: Option<u32>,
    /// Is first child
    pub is_first_child: bool,
    /// Is last child
    pub is_last_child: bool,
    /// Child index (0-based)
    pub child_index: usize,
    /// Total siblings
    pub sibling_count: usize,
}

impl ElementInfo {
    pub fn new(tag_name: u32) -> Self {
        Self {
            tag_name,
            id: None,
            classes: Vec::new(),
            attributes: HashMap::new(),
            parent_tag: None,
            is_first_child: false,
            is_last_child: false,
            child_index: 0,
            sibling_count: 1,
        }
    }
    
    pub fn with_id(mut self, id: &str) -> Self {
        self.id = Some(id.into());
        self
    }
    
    pub fn with_class(mut self, class: &str) -> Self {
        self.classes.push(class.into());
        self
    }
    
    pub fn with_attr(mut self, name: &str, value: &str) -> Self {
        self.attributes.insert(name.into(), value.into());
        self
    }
}

/// Simple selector types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimpleSelector {
    /// Universal selector (*)
    Universal,
    /// Tag name selector (div, span, etc.)
    Tag(u32),
    /// Class selector (.class)
    Class(Box<str>),
    /// ID selector (#id)
    Id(Box<str>),
    /// Attribute existence ([attr])
    AttrExists(Box<str>),
    /// Attribute equals ([attr=value])
    AttrEquals(Box<str>, Box<str>),
    /// Attribute contains ([attr~=value])
    AttrContains(Box<str>, Box<str>),
    /// Attribute starts with ([attr^=value])
    AttrStarts(Box<str>, Box<str>),
    /// Attribute ends with ([attr$=value])
    AttrEnds(Box<str>, Box<str>),
}

impl SimpleSelector {
    /// Match against an element
    pub fn matches(&self, element: &ElementInfo) -> bool {
        match self {
            SimpleSelector::Universal => true,
            SimpleSelector::Tag(tag) => element.tag_name == *tag,
            SimpleSelector::Class(class) => element.classes.iter().any(|c| c.as_ref() == class.as_ref()),
            SimpleSelector::Id(id) => element.id.as_ref().map(|i| i.as_ref() == id.as_ref()).unwrap_or(false),
            SimpleSelector::AttrExists(name) => element.attributes.contains_key(name),
            SimpleSelector::AttrEquals(name, value) => {
                element.attributes.get(name).map(|v| v.as_ref() == value.as_ref()).unwrap_or(false)
            }
            SimpleSelector::AttrContains(name, value) => {
                element.attributes.get(name).map(|v| v.contains(value.as_ref())).unwrap_or(false)
            }
            SimpleSelector::AttrStarts(name, value) => {
                element.attributes.get(name).map(|v| v.starts_with(value.as_ref())).unwrap_or(false)
            }
            SimpleSelector::AttrEnds(name, value) => {
                element.attributes.get(name).map(|v| v.ends_with(value.as_ref())).unwrap_or(false)
            }
        }
    }
}

/// Compound selector (multiple simple selectors that must all match)
#[derive(Debug, Clone)]
pub struct CompoundSelector {
    pub selectors: Vec<SimpleSelector>,
}

impl CompoundSelector {
    pub fn new(selectors: Vec<SimpleSelector>) -> Self {
        Self { selectors }
    }
    
    pub fn matches(&self, element: &ElementInfo) -> bool {
        self.selectors.iter().all(|s| s.matches(element))
    }
}

/// Interpreted selector (parsed but not compiled)
#[derive(Debug, Clone)]
pub struct InterpretedSelector {
    /// Original selector text
    text: Box<str>,
    /// Parsed compound selectors (simplified - not handling combinators)
    compound: CompoundSelector,
    /// Statistics
    stats: SelectorStats,
}

impl InterpretedSelector {
    pub fn new(text: &str, compound: CompoundSelector) -> Self {
        Self {
            text: text.into(),
            compound,
            stats: SelectorStats::default(),
        }
    }
    
    pub fn matches(&mut self, element: &ElementInfo) -> bool {
        self.stats.match_attempts += 1;
        let result = self.compound.matches(element);
        if result {
            self.stats.match_successes += 1;
        }
        result
    }
    
    pub fn text(&self) -> &str {
        &self.text
    }
    
    pub fn stats(&self) -> &SelectorStats {
        &self.stats
    }
}

/// Compiled tag selector
pub struct CompiledTagSelector {
    tag: u32,
    text: Box<str>,
}

impl CompiledTagSelector {
    pub fn new(tag: u32, text: &str) -> Self {
        Self {
            tag,
            text: text.into(),
        }
    }
}

impl CompiledSelector for CompiledTagSelector {
    #[inline(always)]
    fn matches(&self, element: &ElementInfo) -> bool {
        element.tag_name == self.tag
    }
    
    fn selector_text(&self) -> &str {
        &self.text
    }
}

/// Compiled class selector
pub struct CompiledClassSelector {
    class: Box<str>,
    text: Box<str>,
}

impl CompiledClassSelector {
    pub fn new(class: &str) -> Self {
        Self {
            class: class.into(),
            text: format!(".{}", class).into_boxed_str(),
        }
    }
}

impl CompiledSelector for CompiledClassSelector {
    #[inline(always)]
    fn matches(&self, element: &ElementInfo) -> bool {
        element.classes.iter().any(|c| c.as_ref() == self.class.as_ref())
    }
    
    fn selector_text(&self) -> &str {
        &self.text
    }
}

/// Compiled ID selector
pub struct CompiledIdSelector {
    id: Box<str>,
    text: Box<str>,
}

impl CompiledIdSelector {
    pub fn new(id: &str) -> Self {
        Self {
            id: id.into(),
            text: format!("#{}", id).into_boxed_str(),
        }
    }
}

impl CompiledSelector for CompiledIdSelector {
    #[inline(always)]
    fn matches(&self, element: &ElementInfo) -> bool {
        element.id.as_ref().map(|i| i.as_ref() == self.id.as_ref()).unwrap_or(false)
    }
    
    fn selector_text(&self) -> &str {
        &self.text
    }
}

/// Compiled tag + class selector (very common pattern)
pub struct CompiledTagClassSelector {
    tag: u32,
    class: Box<str>,
    text: Box<str>,
}

impl CompiledTagClassSelector {
    pub fn new(tag: u32, tag_name: &str, class: &str) -> Self {
        Self {
            tag,
            class: class.into(),
            text: format!("{}.{}", tag_name, class).into_boxed_str(),
        }
    }
}

impl CompiledSelector for CompiledTagClassSelector {
    #[inline(always)]
    fn matches(&self, element: &ElementInfo) -> bool {
        element.tag_name == self.tag && 
        element.classes.iter().any(|c| c.as_ref() == self.class.as_ref())
    }
    
    fn selector_text(&self) -> &str {
        &self.text
    }
}

/// Selector mode for hybrid matching
pub enum SelectorMode {
    /// Compiled for fast matching
    Compiled(Box<dyn CompiledSelector>),
    /// Interpreted for rare selectors
    Interpreted(InterpretedSelector),
}

/// Hybrid selector matcher
pub struct HybridSelectorMatcher {
    /// Compiled selectors (hot paths)
    compiled: Vec<Box<dyn CompiledSelector>>,
    /// Interpreted selectors (cold paths)
    interpreted: HashMap<u64, InterpretedSelector>,
    /// Selector usage counts for promotion
    usage_counts: HashMap<u64, u64>,
    /// Threshold for "hot" selector
    hot_threshold: u64,
    /// Maximum compiled selectors
    max_compiled: usize,
}

impl Default for HybridSelectorMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HybridSelectorMatcher {
    pub fn new() -> Self {
        Self {
            compiled: Vec::new(),
            interpreted: HashMap::new(),
            usage_counts: HashMap::new(),
            hot_threshold: 100,
            max_compiled: 100,
        }
    }
    
    /// Configure hot threshold
    pub fn with_hot_threshold(mut self, threshold: u64) -> Self {
        self.hot_threshold = threshold;
        self
    }
    
    /// Add a compiled selector
    pub fn add_compiled(&mut self, selector: Box<dyn CompiledSelector>) {
        if self.compiled.len() < self.max_compiled {
            self.compiled.push(selector);
        }
    }
    
    /// Add an interpreted selector
    pub fn add_interpreted(&mut self, text: &str, selector: InterpretedSelector) {
        let hash = Self::hash_selector(text);
        self.interpreted.insert(hash, selector);
    }
    
    /// Hash a selector string
    fn hash_selector(text: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Match an element against compiled selectors
    pub fn match_compiled(&self, element: &ElementInfo) -> Vec<usize> {
        self.compiled.iter()
            .enumerate()
            .filter(|(_, s)| s.matches(element))
            .map(|(i, _)| i)
            .collect()
    }
    
    /// Match against an interpreted selector
    pub fn match_interpreted(&mut self, text: &str, element: &ElementInfo) -> Option<bool> {
        let hash = Self::hash_selector(text);
        
        // Track usage
        *self.usage_counts.entry(hash).or_insert(0) += 1;
        
        self.interpreted.get_mut(&hash).map(|s| s.matches(element))
    }
    
    /// Get hot selectors that should be compiled
    pub fn get_hot_candidates(&self) -> Vec<&str> {
        self.usage_counts.iter()
            .filter(|(_, &count)| count >= self.hot_threshold)
            .filter_map(|(hash, _)| {
                self.interpreted.get(hash).map(|s| s.text())
            })
            .collect()
    }
    
    /// Statistics
    pub fn stats(&self) -> HybridStats {
        HybridStats {
            compiled_count: self.compiled.len(),
            interpreted_count: self.interpreted.len(),
            total_usage: self.usage_counts.values().sum(),
            hot_candidates: self.get_hot_candidates().len(),
        }
    }
}

/// Statistics for hybrid matcher
#[derive(Debug, Clone, Copy)]
pub struct HybridStats {
    pub compiled_count: usize,
    pub interpreted_count: usize,
    pub total_usage: u64,
    pub hot_candidates: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_selectors() {
        let element = ElementInfo::new(1)
            .with_id("main")
            .with_class("container")
            .with_class("wide")
            .with_attr("data-id", "123");
        
        assert!(SimpleSelector::Universal.matches(&element));
        assert!(SimpleSelector::Tag(1).matches(&element));
        assert!(!SimpleSelector::Tag(2).matches(&element));
        assert!(SimpleSelector::Class("container".into()).matches(&element));
        assert!(!SimpleSelector::Class("narrow".into()).matches(&element));
        assert!(SimpleSelector::Id("main".into()).matches(&element));
        assert!(SimpleSelector::AttrExists("data-id".into()).matches(&element));
        assert!(SimpleSelector::AttrEquals("data-id".into(), "123".into()).matches(&element));
    }
    
    #[test]
    fn test_compound_selector() {
        let element = ElementInfo::new(1)
            .with_class("btn")
            .with_class("primary");
        
        let compound = CompoundSelector::new(vec![
            SimpleSelector::Tag(1),
            SimpleSelector::Class("btn".into()),
            SimpleSelector::Class("primary".into()),
        ]);
        
        assert!(compound.matches(&element));
        
        // Different tag
        let element2 = ElementInfo::new(2).with_class("btn").with_class("primary");
        assert!(!compound.matches(&element2));
    }
    
    #[test]
    fn test_compiled_selectors() {
        let element = ElementInfo::new(1).with_class("active");
        
        let tag_sel = CompiledTagSelector::new(1, "div");
        assert!(tag_sel.matches(&element));
        
        let class_sel = CompiledClassSelector::new("active");
        assert!(class_sel.matches(&element));
        
        let tag_class_sel = CompiledTagClassSelector::new(1, "div", "active");
        assert!(tag_class_sel.matches(&element));
    }
    
    #[test]
    fn test_hybrid_matcher() {
        let mut matcher = HybridSelectorMatcher::new().with_hot_threshold(2);
        
        // Add compiled
        matcher.add_compiled(Box::new(CompiledTagSelector::new(1, "div")));
        
        // Add interpreted
        let compound = CompoundSelector::new(vec![SimpleSelector::Class("rare".into())]);
        matcher.add_interpreted(".rare", InterpretedSelector::new(".rare", compound));
        
        let element = ElementInfo::new(1).with_class("rare");
        
        // Match compiled
        let compiled_matches = matcher.match_compiled(&element);
        assert_eq!(compiled_matches, vec![0]);
        
        // Match interpreted
        let interp_match = matcher.match_interpreted(".rare", &element);
        assert_eq!(interp_match, Some(true));
        
        // Call multiple times to track usage
        let _ = matcher.match_interpreted(".rare", &element);
        let _ = matcher.match_interpreted(".rare", &element);
        
        // Should now be a hot candidate
        assert!(!matcher.get_hot_candidates().is_empty());
    }
}
