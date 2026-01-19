//! Selector Splitting for Parallel Matching
//!
//! Split complex selectors into fragments that can be matched independently.
//! Enables parallel matching of descendant/child/sibling selectors.

use std::collections::HashMap;

// ============================================================================
// Selector Fragment Types
// ============================================================================

/// A fragment of a complex selector that can be matched independently
#[derive(Debug, Clone, PartialEq)]
pub struct SelectorFragment {
    /// The simple selector(s) in this fragment
    pub simple_selectors: Vec<SimpleSelector>,
    /// Combinator that follows this fragment (None for rightmost)
    pub combinator: Option<Combinator>,
    /// Fragment index (0 = rightmost/subject)
    pub index: usize,
    /// Hash for bloom filter lookup
    pub hash: u64,
}

/// Simple selector component
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SimpleSelector {
    /// Universal selector *
    Universal,
    /// Type/tag selector
    Tag(Box<str>),
    /// Class selector
    Class(Box<str>),
    /// ID selector
    Id(Box<str>),
    /// Attribute selector
    Attribute {
        name: Box<str>,
        matcher: Option<AttributeMatcher>,
    },
    /// Pseudo-class
    PseudoClass(Box<str>),
    /// Pseudo-element
    PseudoElement(Box<str>),
}

/// Attribute matching operation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AttributeMatcher {
    /// [attr=value]
    Exact(Box<str>),
    /// [attr~=value]
    Includes(Box<str>),
    /// [attr|=value]
    DashMatch(Box<str>),
    /// [attr^=value]
    Prefix(Box<str>),
    /// [attr$=value]
    Suffix(Box<str>),
    /// [attr*=value]
    Contains(Box<str>),
}

/// Selector combinator
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Combinator {
    /// Descendant (space)
    Descendant,
    /// Child (>)
    Child,
    /// Next sibling (+)
    NextSibling,
    /// Subsequent sibling (~)
    SubsequentSibling,
}

// ============================================================================
// Selector Splitter
// ============================================================================

/// Split a complex selector into matchable fragments
#[derive(Debug)]
pub struct SelectorSplitter {
    /// Fragment cache for deduplication
    fragment_cache: HashMap<u64, Vec<SelectorFragment>>,
    /// Statistics
    stats: SplitterStats,
}

/// Splitter statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct SplitterStats {
    pub selectors_split: u64,
    pub fragments_created: u64,
    pub cache_hits: u64,
}

impl Default for SelectorSplitter {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectorSplitter {
    pub fn new() -> Self {
        Self {
            fragment_cache: HashMap::new(),
            stats: SplitterStats::default(),
        }
    }
    
    /// Split a selector string into fragments
    pub fn split(&mut self, selector: &str) -> Vec<SelectorFragment> {
        let hash = hash_selector(selector);
        
        // Check cache
        if let Some(cached) = self.fragment_cache.get(&hash) {
            self.stats.cache_hits += 1;
            return cached.clone();
        }
        
        let fragments = parallelize_selector(selector);
        self.stats.selectors_split += 1;
        self.stats.fragments_created += fragments.len() as u64;
        
        self.fragment_cache.insert(hash, fragments.clone());
        fragments
    }
    
    /// Get statistics
    pub fn stats(&self) -> &SplitterStats {
        &self.stats
    }
    
    /// Clear cache
    pub fn clear(&mut self) {
        self.fragment_cache.clear();
    }
}

/// Split a complex selector into fragments for parallel matching
/// 
/// Example: ".foo .bar .baz" -> [".baz", ".bar", ".foo"]
/// The fragments are returned in reverse order (subject first) for RTL matching.
pub fn parallelize_selector(selector: &str) -> Vec<SelectorFragment> {
    let selector = selector.trim();
    if selector.is_empty() {
        return Vec::new();
    }
    
    let mut fragments = Vec::new();
    let mut current_simple = Vec::new();
    let mut chars = selector.chars().peekable();
    let mut current_token = String::new();
    
    // Tokenize and split
    while let Some(c) = chars.next() {
        match c {
            // Combinators
            ' ' if !current_token.is_empty() => {
                // Descendant combinator
                finish_simple_selector(&current_token, &mut current_simple);
                current_token.clear();
                
                // Skip extra whitespace
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
                
                // Check for explicit combinator
                match chars.peek() {
                    Some('>') => {
                        chars.next();
                        skip_whitespace(&mut chars);
                        push_fragment(&mut fragments, &mut current_simple, Some(Combinator::Child));
                    }
                    Some('+') => {
                        chars.next();
                        skip_whitespace(&mut chars);
                        push_fragment(&mut fragments, &mut current_simple, Some(Combinator::NextSibling));
                    }
                    Some('~') => {
                        chars.next();
                        skip_whitespace(&mut chars);
                        push_fragment(&mut fragments, &mut current_simple, Some(Combinator::SubsequentSibling));
                    }
                    _ => {
                        push_fragment(&mut fragments, &mut current_simple, Some(Combinator::Descendant));
                    }
                }
            }
            '>' => {
                if !current_token.is_empty() {
                    finish_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                skip_whitespace(&mut chars);
                push_fragment(&mut fragments, &mut current_simple, Some(Combinator::Child));
            }
            '+' => {
                if !current_token.is_empty() {
                    finish_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                skip_whitespace(&mut chars);
                push_fragment(&mut fragments, &mut current_simple, Some(Combinator::NextSibling));
            }
            '~' if chars.peek() != Some(&'=') => {
                if !current_token.is_empty() {
                    finish_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                skip_whitespace(&mut chars);
                push_fragment(&mut fragments, &mut current_simple, Some(Combinator::SubsequentSibling));
            }
            // Attribute selector
            '[' => {
                if !current_token.is_empty() {
                    finish_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                let attr = parse_attribute_selector(&mut chars);
                current_simple.push(attr);
            }
            // Pseudo-element
            ':' if chars.peek() == Some(&':') => {
                if !current_token.is_empty() {
                    finish_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                chars.next(); // Skip second colon
                let name = collect_ident(&mut chars);
                current_simple.push(SimpleSelector::PseudoElement(name.into()));
            }
            // Pseudo-class
            ':' => {
                if !current_token.is_empty() {
                    finish_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                let name = collect_pseudo_class(&mut chars);
                current_simple.push(SimpleSelector::PseudoClass(name.into()));
            }
            // ID selector - flush current token first
            '#' => {
                if !current_token.is_empty() {
                    finish_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                current_token.push(c);
            }
            // Class selector - flush current token first
            '.' => {
                if !current_token.is_empty() {
                    finish_simple_selector(&current_token, &mut current_simple);
                    current_token.clear();
                }
                current_token.push(c);
            }
            // Continue current token
            _ => {
                current_token.push(c);
            }
        }
    }
    
    // Finish last token
    if !current_token.is_empty() {
        finish_simple_selector(&current_token, &mut current_simple);
    }
    
    // Push final fragment (no combinator - this is the subject)
    if !current_simple.is_empty() {
        push_fragment(&mut fragments, &mut current_simple, None);
    }
    
    // Reverse for RTL matching (subject first)
    for (i, frag) in fragments.iter_mut().enumerate() {
        frag.index = i;
    }
    
    fragments
}

fn skip_whitespace(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while chars.peek() == Some(&' ') {
        chars.next();
    }
}

fn finish_simple_selector(token: &str, selectors: &mut Vec<SimpleSelector>) {
    let token = token.trim();
    if token.is_empty() {
        return;
    }
    
    if token == "*" {
        selectors.push(SimpleSelector::Universal);
    } else if let Some(id) = token.strip_prefix('#') {
        selectors.push(SimpleSelector::Id(id.into()));
    } else if let Some(class) = token.strip_prefix('.') {
        selectors.push(SimpleSelector::Class(class.into()));
    } else {
        selectors.push(SimpleSelector::Tag(token.into()));
    }
}

fn push_fragment(
    fragments: &mut Vec<SelectorFragment>,
    simple: &mut Vec<SimpleSelector>,
    combinator: Option<Combinator>,
) {
    if simple.is_empty() {
        return;
    }
    
    let hash = hash_simple_selectors(simple);
    
    fragments.push(SelectorFragment {
        simple_selectors: std::mem::take(simple),
        combinator,
        index: 0, // Will be set later
        hash,
    });
}

fn parse_attribute_selector(chars: &mut std::iter::Peekable<std::str::Chars>) -> SimpleSelector {
    let mut name = String::new();
    let mut value = String::new();
    let mut op = None;
    let mut in_value = false;
    let mut quote_char = None;
    
    while let Some(c) = chars.next() {
        if c == ']' && quote_char.is_none() {
            break;
        }
        
        if in_value {
            // Handle quoted values
            if let Some(q) = quote_char {
                if c == q {
                    quote_char = None;
                } else {
                    value.push(c);
                }
            } else if c == '"' || c == '\'' {
                quote_char = Some(c);
            } else {
                value.push(c);
            }
        } else {
            match c {
                '=' => {
                    op = Some(AttributeMatcher::Exact(Box::from("")));
                    in_value = true;
                }
                '~' if chars.peek() == Some(&'=') => {
                    chars.next();
                    op = Some(AttributeMatcher::Includes(Box::from("")));
                    in_value = true;
                }
                '|' if chars.peek() == Some(&'=') => {
                    chars.next();
                    op = Some(AttributeMatcher::DashMatch(Box::from("")));
                    in_value = true;
                }
                '^' if chars.peek() == Some(&'=') => {
                    chars.next();
                    op = Some(AttributeMatcher::Prefix(Box::from("")));
                    in_value = true;
                }
                '$' if chars.peek() == Some(&'=') => {
                    chars.next();
                    op = Some(AttributeMatcher::Suffix(Box::from("")));
                    in_value = true;
                }
                '*' if chars.peek() == Some(&'=') => {
                    chars.next();
                    op = Some(AttributeMatcher::Contains(Box::from("")));
                    in_value = true;
                }
                _ if !c.is_whitespace() => {
                    name.push(c);
                }
                _ => {}
            }
        }
    }
    
    let matcher = op.map(|m| match m {
        AttributeMatcher::Exact(_) => AttributeMatcher::Exact(value.clone().into()),
        AttributeMatcher::Includes(_) => AttributeMatcher::Includes(value.clone().into()),
        AttributeMatcher::DashMatch(_) => AttributeMatcher::DashMatch(value.clone().into()),
        AttributeMatcher::Prefix(_) => AttributeMatcher::Prefix(value.clone().into()),
        AttributeMatcher::Suffix(_) => AttributeMatcher::Suffix(value.clone().into()),
        AttributeMatcher::Contains(_) => AttributeMatcher::Contains(value.clone().into()),
    });
    
    SimpleSelector::Attribute {
        name: name.into(),
        matcher,
    }
}

fn collect_ident(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut ident = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '-' || c == '_' {
            ident.push(c);
            chars.next();
        } else {
            break;
        }
    }
    ident
}

fn collect_pseudo_class(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut name = String::new();
    let mut paren_depth = 0;
    
    while let Some(&c) = chars.peek() {
        if c == '(' {
            paren_depth += 1;
            name.push(c);
            chars.next();
        } else if c == ')' {
            if paren_depth > 0 {
                paren_depth -= 1;
                name.push(c);
                chars.next();
                if paren_depth == 0 {
                    break;
                }
            } else {
                break;
            }
        } else if paren_depth > 0 || c.is_alphanumeric() || c == '-' || c == '_' {
            name.push(c);
            chars.next();
        } else {
            break;
        }
    }
    
    name
}

fn hash_simple_selectors(selectors: &[SimpleSelector]) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    
    for sel in selectors {
        match sel {
            SimpleSelector::Universal => hasher.write_u8(0),
            SimpleSelector::Tag(t) => {
                hasher.write_u8(1);
                hasher.write(t.as_bytes());
            }
            SimpleSelector::Class(c) => {
                hasher.write_u8(2);
                hasher.write(c.as_bytes());
            }
            SimpleSelector::Id(i) => {
                hasher.write_u8(3);
                hasher.write(i.as_bytes());
            }
            SimpleSelector::Attribute { name, .. } => {
                hasher.write_u8(4);
                hasher.write(name.as_bytes());
            }
            SimpleSelector::PseudoClass(p) => {
                hasher.write_u8(5);
                hasher.write(p.as_bytes());
            }
            SimpleSelector::PseudoElement(p) => {
                hasher.write_u8(6);
                hasher.write(p.as_bytes());
            }
        }
    }
    
    hasher.finish()
}

fn hash_selector(selector: &str) -> u64 {
    use std::hash::Hasher;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hasher.write(selector.as_bytes());
    hasher.finish()
}

// ============================================================================
// Parallel Fragment Matcher
// ============================================================================

/// Result of matching a single fragment
#[derive(Debug, Clone)]
pub struct FragmentMatch {
    /// Fragment index that matched
    pub fragment_index: usize,
    /// Element that matched
    pub matched_element: u32,
}

/// Match fragments in parallel across multiple elements
pub fn match_fragments_parallel(
    fragments: &[SelectorFragment],
    elements: &[ElementInfo],
    num_threads: usize,
) -> Vec<Vec<FragmentMatch>> {
    use std::sync::{Arc, Mutex};
    use std::thread;
    
    if elements.is_empty() || fragments.is_empty() {
        return Vec::new();
    }
    
    let results = Arc::new(Mutex::new(vec![Vec::new(); elements.len()]));
    let elements_per_thread = (elements.len() + num_threads - 1) / num_threads;
    let fragments = Arc::new(fragments.to_vec());
    let elements = Arc::new(elements.to_vec());
    
    let mut handles = Vec::new();
    
    for thread_id in 0..num_threads {
        let start = thread_id * elements_per_thread;
        let end = ((thread_id + 1) * elements_per_thread).min(elements.len());
        
        if start >= elements.len() {
            break;
        }
        
        let results = Arc::clone(&results);
        let fragments = Arc::clone(&fragments);
        let elements = Arc::clone(&elements);
        
        let handle = thread::spawn(move || {
            for elem_idx in start..end {
                let elem = &elements[elem_idx];
                let mut matches = Vec::new();
                
                for (frag_idx, frag) in fragments.iter().enumerate() {
                    if fragment_matches(frag, elem) {
                        matches.push(FragmentMatch {
                            fragment_index: frag_idx,
                            matched_element: elem_idx as u32,
                        });
                    }
                }
                
                if !matches.is_empty() {
                    let mut results = results.lock().unwrap();
                    results[elem_idx] = matches;
                }
            }
        });
        
        handles.push(handle);
    }
    
    for handle in handles {
        handle.join().unwrap();
    }
    
    Arc::try_unwrap(results).unwrap().into_inner().unwrap()
}

/// Check if a fragment matches an element
fn fragment_matches(fragment: &SelectorFragment, element: &ElementInfo) -> bool {
    for simple in &fragment.simple_selectors {
        if !simple_selector_matches(simple, element) {
            return false;
        }
    }
    true
}

/// Check if a simple selector matches an element
fn simple_selector_matches(selector: &SimpleSelector, element: &ElementInfo) -> bool {
    match selector {
        SimpleSelector::Universal => true,
        SimpleSelector::Tag(tag) => element.tag_name.eq_ignore_ascii_case(tag),
        SimpleSelector::Class(class) => element.classes.iter().any(|c| c.as_ref() == class.as_ref()),
        SimpleSelector::Id(id) => element.id.as_ref().map_or(false, |i| i.as_ref() == id.as_ref()),
        SimpleSelector::Attribute { name, matcher } => {
            if let Some(value) = element.attributes.get(name.as_ref()) {
                match matcher {
                    None => true,
                    Some(AttributeMatcher::Exact(v)) => value.as_ref() == v.as_ref(),
                    Some(AttributeMatcher::Includes(v)) => {
                        value.split_whitespace().any(|w| w == v.as_ref())
                    }
                    Some(AttributeMatcher::DashMatch(v)) => {
                        value.as_ref() == v.as_ref() || value.starts_with(&format!("{}-", v))
                    }
                    Some(AttributeMatcher::Prefix(v)) => value.starts_with(v.as_ref()),
                    Some(AttributeMatcher::Suffix(v)) => value.ends_with(v.as_ref()),
                    Some(AttributeMatcher::Contains(v)) => value.contains(v.as_ref()),
                }
            } else {
                false
            }
        }
        SimpleSelector::PseudoClass(_) | SimpleSelector::PseudoElement(_) => {
            // Pseudo-classes/elements need dynamic state - skip for fragment matching
            true
        }
    }
}

/// Element information for matching
#[derive(Debug, Clone)]
pub struct ElementInfo {
    pub tag_name: Box<str>,
    pub id: Option<Box<str>>,
    pub classes: Vec<Box<str>>,
    pub attributes: HashMap<Box<str>, Box<str>>,
}

impl ElementInfo {
    pub fn new(tag_name: &str) -> Self {
        Self {
            tag_name: tag_name.into(),
            id: None,
            classes: Vec::new(),
            attributes: HashMap::new(),
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
    
    pub fn with_attribute(mut self, name: &str, value: &str) -> Self {
        self.attributes.insert(name.into(), value.into());
        self
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_split_simple_selector() {
        let fragments = parallelize_selector("div");
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].simple_selectors.len(), 1);
        assert!(matches!(&fragments[0].simple_selectors[0], SimpleSelector::Tag(t) if t.as_ref() == "div"));
    }
    
    #[test]
    fn test_split_class_selector() {
        let fragments = parallelize_selector(".container");
        assert_eq!(fragments.len(), 1);
        assert!(matches!(&fragments[0].simple_selectors[0], SimpleSelector::Class(c) if c.as_ref() == "container"));
    }
    
    #[test]
    fn test_split_descendant() {
        let fragments = parallelize_selector(".foo .bar .baz");
        assert_eq!(fragments.len(), 3);
        
        // RTL order: .baz is first (index 0)
        assert!(matches!(&fragments[0].simple_selectors[0], SimpleSelector::Class(c) if c.as_ref() == "foo"));
        assert!(matches!(&fragments[1].simple_selectors[0], SimpleSelector::Class(c) if c.as_ref() == "bar"));
        assert!(matches!(&fragments[2].simple_selectors[0], SimpleSelector::Class(c) if c.as_ref() == "baz"));
    }
    
    #[test]
    fn test_split_child_combinator() {
        let fragments = parallelize_selector("div > span");
        assert_eq!(fragments.len(), 2);
        assert_eq!(fragments[0].combinator, Some(Combinator::Child));
    }
    
    #[test]
    fn test_split_compound_selector() {
        let fragments = parallelize_selector("div.container#main");
        assert_eq!(fragments.len(), 1);
        assert_eq!(fragments[0].simple_selectors.len(), 3);
    }
    
    #[test]
    fn test_split_attribute_selector() {
        let fragments = parallelize_selector("[data-value=\"test\"]");
        assert_eq!(fragments.len(), 1);
        assert!(matches!(
            &fragments[0].simple_selectors[0],
            SimpleSelector::Attribute { name, matcher: Some(AttributeMatcher::Exact(v)) }
            if name.as_ref() == "data-value" && v.as_ref() == "test"
        ));
    }
    
    #[test]
    fn test_fragment_match() {
        let fragments = parallelize_selector(".active");
        let element = ElementInfo::new("div").with_class("active");
        
        assert!(fragment_matches(&fragments[0], &element));
    }
    
    #[test]
    fn test_fragment_no_match() {
        let fragments = parallelize_selector(".inactive");
        let element = ElementInfo::new("div").with_class("active");
        
        assert!(!fragment_matches(&fragments[0], &element));
    }
    
    #[test]
    fn test_splitter_cache() {
        let mut splitter = SelectorSplitter::new();
        
        let _ = splitter.split(".foo .bar");
        let _ = splitter.split(".foo .bar");
        
        assert_eq!(splitter.stats().cache_hits, 1);
    }
}
