//! :has() Selector Implementation
//!
//! The :has() relational pseudo-class represents elements with descendants/siblings
//! matching the provided selector list. This requires subject-finding which is
//! expensive, so aggressive caching is used.

use std::collections::HashMap;
use std::hash::Hash;

// ============================================================================
// :has() Selector Types
// ============================================================================

/// A :has() selector with its relative selector list
#[derive(Debug, Clone)]
pub struct HasSelector {
    /// Unique ID for caching
    pub id: HasSelectorId,
    /// Relative selector list
    pub relative_selectors: Vec<RelativeSelector>,
    /// Match statistics
    pub match_count: u64,
}

/// ID for :has() selector caching
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HasSelectorId(pub u32);

/// A relative selector (the argument to :has())
#[derive(Debug, Clone)]
pub struct RelativeSelector {
    /// Combinator from subject to target
    pub combinator: RelativeCombinator,
    /// Selector components to match
    pub components: Vec<SelectorComponent>,
}

/// Combinator for relative selector (defaults to descendant)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RelativeCombinator {
    /// Descendant (default, space)
    #[default]
    Descendant,
    /// Direct child (>)
    Child,
    /// Next sibling (+)
    NextSibling,
    /// Subsequent sibling (~)
    SubsequentSibling,
}

/// Selector component for matching
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SelectorComponent {
    Universal,
    Tag(Box<str>),
    Class(Box<str>),
    Id(Box<str>),
    Attribute(Box<str>, Option<Box<str>>),
    PseudoClass(Box<str>),
}

// ============================================================================
// :has() Cache
// ============================================================================

/// Cache entry for :has() match results
#[derive(Debug, Clone)]
struct HasCacheEntry {
    /// Does it match?
    matches: bool,
    /// DOM generation when cached
    dom_generation: u64,
    /// Last access time for LRU
    last_access: u64,
}

/// Cache key for :has() lookups
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct HasCacheKey {
    /// Element ID
    element_id: u32,
    /// :has() selector ID
    selector_id: HasSelectorId,
}

/// Cache for :has() selector results
#[derive(Debug)]
pub struct HasSelectorCache {
    /// Cached match results
    cache: HashMap<HasCacheKey, HasCacheEntry>,
    /// Maximum cache size
    max_size: usize,
    /// Current DOM generation
    dom_generation: u64,
    /// Access counter for LRU
    access_counter: u64,
    /// Statistics
    stats: HasCacheStats,
}

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct HasCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub invalidations: u64,
    pub evictions: u64,
}

impl HasCacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

impl Default for HasSelectorCache {
    fn default() -> Self {
        Self::new(8192)
    }
}

impl HasSelectorCache {
    /// Create with given capacity
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_size),
            max_size,
            dom_generation: 0,
            access_counter: 0,
            stats: HasCacheStats::default(),
        }
    }
    
    /// Check if a :has() selector matches an element
    pub fn get(&mut self, element_id: u32, selector_id: HasSelectorId) -> Option<bool> {
        let key = HasCacheKey { element_id, selector_id };
        
        if let Some(entry) = self.cache.get_mut(&key) {
            // Check if still valid
            if entry.dom_generation == self.dom_generation {
                self.access_counter += 1;
                entry.last_access = self.access_counter;
                self.stats.hits += 1;
                return Some(entry.matches);
            } else {
                // Stale entry - will be overwritten
                self.stats.invalidations += 1;
            }
        }
        
        self.stats.misses += 1;
        None
    }
    
    /// Cache a :has() match result
    pub fn insert(&mut self, element_id: u32, selector_id: HasSelectorId, matches: bool) {
        // Evict if at capacity
        if self.cache.len() >= self.max_size {
            self.evict_lru();
        }
        
        self.access_counter += 1;
        
        let key = HasCacheKey { element_id, selector_id };
        self.cache.insert(key, HasCacheEntry {
            matches,
            dom_generation: self.dom_generation,
            last_access: self.access_counter,
        });
    }
    
    /// Invalidate cache on DOM mutation
    pub fn invalidate_subtree(&mut self, _root_element_id: u32) {
        // Increment generation to invalidate all entries
        // A more precise implementation would track subtree relationships
        self.dom_generation += 1;
        self.stats.invalidations += 1;
    }
    
    /// Full invalidation
    pub fn invalidate_all(&mut self) {
        self.dom_generation += 1;
        self.stats.invalidations += 1;
    }
    
    /// Evict least recently used entries
    fn evict_lru(&mut self) {
        let target = self.max_size / 2;
        let mut entries: Vec<_> = self.cache.iter()
            .map(|(k, v)| (k.clone(), v.last_access))
            .collect();
        
        entries.sort_by_key(|(_, access)| *access);
        
        for (key, _) in entries.iter().take(self.cache.len().saturating_sub(target)) {
            self.cache.remove(key);
            self.stats.evictions += 1;
        }
    }
    
    /// Get statistics
    pub fn stats(&self) -> &HasCacheStats {
        &self.stats
    }
    
    /// Clear cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.dom_generation = 0;
        self.access_counter = 0;
    }
}

// ============================================================================
// :has() Matcher
// ============================================================================

/// Matcher for :has() selectors
#[derive(Debug)]
pub struct HasMatcher {
    /// Registered :has() selectors
    selectors: HashMap<HasSelectorId, HasSelector>,
    /// Result cache
    cache: HasSelectorCache,
    /// Next selector ID
    next_id: u32,
}

impl Default for HasMatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HasMatcher {
    pub fn new() -> Self {
        Self {
            selectors: HashMap::new(),
            cache: HasSelectorCache::default(),
            next_id: 1,
        }
    }
    
    /// Register a :has() selector
    pub fn register(&mut self, relative_selectors: Vec<RelativeSelector>) -> HasSelectorId {
        let id = HasSelectorId(self.next_id);
        self.next_id += 1;
        
        self.selectors.insert(id, HasSelector {
            id,
            relative_selectors,
            match_count: 0,
        });
        
        id
    }
    
    /// Check if :has() matches for an element
    pub fn matches(
        &mut self,
        element_id: u32,
        selector_id: HasSelectorId,
        tree: &dyn HasMatchContext,
    ) -> bool {
        // Check cache first
        if let Some(result) = self.cache.get(element_id, selector_id) {
            return result;
        }
        
        // Compute match
        let result = self.compute_match(element_id, selector_id, tree);
        
        // Cache result
        self.cache.insert(element_id, selector_id, result);
        
        // Update stats
        if result {
            if let Some(sel) = self.selectors.get_mut(&selector_id) {
                sel.match_count += 1;
            }
        }
        
        result
    }
    
    /// Compute :has() match (expensive)
    fn compute_match(
        &self,
        element_id: u32,
        selector_id: HasSelectorId,
        tree: &dyn HasMatchContext,
    ) -> bool {
        let selector = match self.selectors.get(&selector_id) {
            Some(s) => s,
            None => return false,
        };
        
        // Match any relative selector
        for rel_sel in &selector.relative_selectors {
            if self.match_relative(element_id, rel_sel, tree) {
                return true;
            }
        }
        
        false
    }
    
    /// Match a relative selector
    fn match_relative(
        &self,
        subject_id: u32,
        rel_sel: &RelativeSelector,
        tree: &dyn HasMatchContext,
    ) -> bool {
        match rel_sel.combinator {
            RelativeCombinator::Descendant => {
                // Check all descendants
                self.match_descendants(subject_id, &rel_sel.components, tree)
            }
            RelativeCombinator::Child => {
                // Check direct children only
                for child_id in tree.children(subject_id) {
                    if self.element_matches(child_id, &rel_sel.components, tree) {
                        return true;
                    }
                }
                false
            }
            RelativeCombinator::NextSibling => {
                // Check next sibling only
                if let Some(next_id) = tree.next_sibling(subject_id) {
                    self.element_matches(next_id, &rel_sel.components, tree)
                } else {
                    false
                }
            }
            RelativeCombinator::SubsequentSibling => {
                // Check all following siblings
                let mut current = tree.next_sibling(subject_id);
                while let Some(sibling_id) = current {
                    if self.element_matches(sibling_id, &rel_sel.components, tree) {
                        return true;
                    }
                    current = tree.next_sibling(sibling_id);
                }
                false
            }
        }
    }
    
    /// Match descendants recursively
    fn match_descendants(
        &self,
        parent_id: u32,
        components: &[SelectorComponent],
        tree: &dyn HasMatchContext,
    ) -> bool {
        for child_id in tree.children(parent_id) {
            if self.element_matches(child_id, components, tree) {
                return true;
            }
            // Recurse into children
            if self.match_descendants(child_id, components, tree) {
                return true;
            }
        }
        false
    }
    
    /// Check if element matches components
    fn element_matches(
        &self,
        element_id: u32,
        components: &[SelectorComponent],
        tree: &dyn HasMatchContext,
    ) -> bool {
        let info = match tree.element_info(element_id) {
            Some(info) => info,
            None => return false,
        };
        
        for component in components {
            let matches = match component {
                SelectorComponent::Universal => true,
                SelectorComponent::Tag(tag) => info.tag_name.eq_ignore_ascii_case(tag),
                SelectorComponent::Class(class) => info.classes.iter().any(|c| c.as_ref() == class.as_ref()),
                SelectorComponent::Id(id) => info.id.as_ref().map_or(false, |i| i.as_ref() == id.as_ref()),
                SelectorComponent::Attribute(name, value) => {
                    if let Some(attr_val) = info.attributes.get(name.as_ref()) {
                        value.as_ref().map_or(true, |v| attr_val.as_ref() == v.as_ref())
                    } else {
                        false
                    }
                }
                SelectorComponent::PseudoClass(pseudo) => {
                    // Handle common pseudo-classes
                    match pseudo.as_ref() {
                        "first-child" => tree.is_first_child(element_id),
                        "last-child" => tree.is_last_child(element_id),
                        "only-child" => tree.is_only_child(element_id),
                        "empty" => tree.is_empty(element_id),
                        _ => true, // Unknown pseudo-classes pass
                    }
                }
            };
            
            if !matches {
                return false;
            }
        }
        
        true
    }
    
    /// Invalidate cache for subtree
    pub fn invalidate_subtree(&mut self, root_id: u32) {
        self.cache.invalidate_subtree(root_id);
    }
    
    /// Get cache statistics
    pub fn cache_stats(&self) -> &HasCacheStats {
        self.cache.stats()
    }
}

// ============================================================================
// Match Context Trait
// ============================================================================

/// Element info for matching
#[derive(Debug, Clone)]
pub struct ElementMatchInfo {
    pub tag_name: Box<str>,
    pub id: Option<Box<str>>,
    pub classes: Vec<Box<str>>,
    pub attributes: HashMap<Box<str>, Box<str>>,
}

/// Context trait for DOM tree traversal during :has() matching
pub trait HasMatchContext {
    /// Get children of an element
    fn children(&self, element_id: u32) -> Vec<u32>;
    
    /// Get next sibling
    fn next_sibling(&self, element_id: u32) -> Option<u32>;
    
    /// Get element info for matching
    fn element_info(&self, element_id: u32) -> Option<ElementMatchInfo>;
    
    /// Check if element is first child
    fn is_first_child(&self, element_id: u32) -> bool;
    
    /// Check if element is last child
    fn is_last_child(&self, element_id: u32) -> bool;
    
    /// Check if element is only child
    fn is_only_child(&self, element_id: u32) -> bool;
    
    /// Check if element is empty
    fn is_empty(&self, element_id: u32) -> bool;
}

// ============================================================================
// :has() Parser
// ============================================================================

/// Parse a :has() argument string into relative selectors
pub fn parse_has_argument(arg: &str) -> Vec<RelativeSelector> {
    let arg = arg.trim();
    if arg.is_empty() {
        return Vec::new();
    }
    
    // Split by comma for selector list
    let mut selectors = Vec::new();
    
    for part in arg.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        
        if let Some(sel) = parse_relative_selector(part) {
            selectors.push(sel);
        }
    }
    
    selectors
}

/// Parse a single relative selector
fn parse_relative_selector(input: &str) -> Option<RelativeSelector> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    
    // Determine combinator
    let (combinator, selector_str) = if input.starts_with('>') {
        (RelativeCombinator::Child, input[1..].trim())
    } else if input.starts_with('+') {
        (RelativeCombinator::NextSibling, input[1..].trim())
    } else if input.starts_with('~') {
        (RelativeCombinator::SubsequentSibling, input[1..].trim())
    } else {
        (RelativeCombinator::Descendant, input)
    };
    
    // Parse selector components
    let components = parse_selector_components(selector_str);
    
    if components.is_empty() {
        return None;
    }
    
    Some(RelativeSelector {
        combinator,
        components,
    })
}

/// Parse selector components
fn parse_selector_components(input: &str) -> Vec<SelectorComponent> {
    let mut components = Vec::new();
    let mut chars = input.chars().peekable();
    let mut current = String::new();
    
    while let Some(c) = chars.next() {
        match c {
            '#' => {
                if !current.is_empty() {
                    if current == "*" {
                        components.push(SelectorComponent::Universal);
                    } else {
                        components.push(SelectorComponent::Tag(current.clone().into()));
                    }
                    current.clear();
                }
                // Collect ID
                let mut id = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        id.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !id.is_empty() {
                    components.push(SelectorComponent::Id(id.into()));
                }
            }
            '.' => {
                if !current.is_empty() {
                    if current == "*" {
                        components.push(SelectorComponent::Universal);
                    } else {
                        components.push(SelectorComponent::Tag(current.clone().into()));
                    }
                    current.clear();
                }
                // Collect class
                let mut class = String::new();
                while let Some(&c) = chars.peek() {
                    if c.is_alphanumeric() || c == '-' || c == '_' {
                        class.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !class.is_empty() {
                    components.push(SelectorComponent::Class(class.into()));
                }
            }
            '[' => {
                if !current.is_empty() {
                    if current == "*" {
                        components.push(SelectorComponent::Universal);
                    } else {
                        components.push(SelectorComponent::Tag(current.clone().into()));
                    }
                    current.clear();
                }
                // Collect attribute
                let mut attr_name = String::new();
                let mut attr_value = None;
                let mut in_value = false;
                
                while let Some(c) = chars.next() {
                    if c == ']' {
                        break;
                    }
                    if c == '=' && !in_value {
                        in_value = true;
                        attr_value = Some(String::new());
                    } else if in_value {
                        if let Some(ref mut v) = attr_value {
                            if c != '"' && c != '\'' {
                                v.push(c);
                            }
                        }
                    } else {
                        attr_name.push(c);
                    }
                }
                
                if !attr_name.is_empty() {
                    components.push(SelectorComponent::Attribute(
                        attr_name.into(),
                        attr_value.map(|v| v.into()),
                    ));
                }
            }
            ':' if chars.peek() != Some(&':') => {
                if !current.is_empty() {
                    if current == "*" {
                        components.push(SelectorComponent::Universal);
                    } else {
                        components.push(SelectorComponent::Tag(current.clone().into()));
                    }
                    current.clear();
                }
                // Collect pseudo-class
                let mut pseudo = String::new();
                let mut paren_depth = 0;
                
                while let Some(&c) = chars.peek() {
                    if c == '(' {
                        paren_depth += 1;
                        pseudo.push(c);
                        chars.next();
                    } else if c == ')' {
                        pseudo.push(c);
                        chars.next();
                        paren_depth -= 1;
                        if paren_depth == 0 {
                            break;
                        }
                    } else if paren_depth > 0 || c.is_alphanumeric() || c == '-' || c == '_' {
                        pseudo.push(c);
                        chars.next();
                    } else {
                        break;
                    }
                }
                
                if !pseudo.is_empty() {
                    components.push(SelectorComponent::PseudoClass(pseudo.into()));
                }
            }
            _ if !c.is_whitespace() => {
                current.push(c);
            }
            _ => {
                // Whitespace - finish current token
                if !current.is_empty() {
                    if current == "*" {
                        components.push(SelectorComponent::Universal);
                    } else {
                        components.push(SelectorComponent::Tag(current.clone().into()));
                    }
                    current.clear();
                }
            }
        }
    }
    
    // Handle remaining token
    if !current.is_empty() {
        if current == "*" {
            components.push(SelectorComponent::Universal);
        } else {
            components.push(SelectorComponent::Tag(current.into()));
        }
    }
    
    components
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_has_simple() {
        let selectors = parse_has_argument(".active");
        assert_eq!(selectors.len(), 1);
        assert_eq!(selectors[0].combinator, RelativeCombinator::Descendant);
        assert!(matches!(&selectors[0].components[0], SelectorComponent::Class(c) if c.as_ref() == "active"));
    }
    
    #[test]
    fn test_parse_has_child() {
        let selectors = parse_has_argument("> .direct-child");
        assert_eq!(selectors.len(), 1);
        assert_eq!(selectors[0].combinator, RelativeCombinator::Child);
    }
    
    #[test]
    fn test_parse_has_sibling() {
        let selectors = parse_has_argument("+ .next-sibling");
        assert_eq!(selectors.len(), 1);
        assert_eq!(selectors[0].combinator, RelativeCombinator::NextSibling);
    }
    
    #[test]
    fn test_parse_has_list() {
        let selectors = parse_has_argument(".foo, .bar");
        assert_eq!(selectors.len(), 2);
    }
    
    #[test]
    fn test_cache_operations() {
        let mut cache = HasSelectorCache::new(100);
        let sel_id = HasSelectorId(1);
        
        cache.insert(1, sel_id, true);
        assert_eq!(cache.get(1, sel_id), Some(true));
        assert_eq!(cache.stats().hits, 1);
        
        cache.invalidate_all();
        assert_eq!(cache.get(1, sel_id), None);
    }
    
    #[test]
    fn test_has_matcher_register() {
        let mut matcher = HasMatcher::new();
        
        let id = matcher.register(parse_has_argument(".foo"));
        assert_eq!(id.0, 1);
        
        let id2 = matcher.register(parse_has_argument(".bar"));
        assert_eq!(id2.0, 2);
    }
}
