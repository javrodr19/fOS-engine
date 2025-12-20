//! Selector Optimization
//!
//! High-performance selector matching with hash indexing and RTL matching.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Selector hash index for O(1) lookup
#[derive(Debug, Default)]
pub struct SelectorIndex {
    /// Index by element type (tag name)
    by_type: HashMap<String, Vec<usize>>,
    /// Index by ID
    by_id: HashMap<String, Vec<usize>>,
    /// Index by class
    by_class: HashMap<String, Vec<usize>>,
    /// Universal selectors (no good index)
    universal: Vec<usize>,
}

impl SelectorIndex {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add a selector to the index
    pub fn add(&mut self, idx: usize, selector: &IndexableSelector) {
        match selector {
            IndexableSelector::Type(name) => {
                self.by_type.entry(name.clone()).or_default().push(idx);
            }
            IndexableSelector::Id(id) => {
                self.by_id.entry(id.clone()).or_default().push(idx);
            }
            IndexableSelector::Class(class) => {
                self.by_class.entry(class.clone()).or_default().push(idx);
            }
            IndexableSelector::Universal => {
                self.universal.push(idx);
            }
        }
    }
    
    /// Get potential matching selector indices for an element
    pub fn get_candidates(&self, element: &ElementInfo) -> Vec<usize> {
        let mut candidates = Vec::new();
        
        // Add universal selectors
        candidates.extend(self.universal.iter().copied());
        
        // Add by type
        if let Some(indices) = self.by_type.get(&element.tag_name) {
            candidates.extend(indices.iter().copied());
        }
        
        // Add by ID
        if let Some(id) = &element.id {
            if let Some(indices) = self.by_id.get(id) {
                candidates.extend(indices.iter().copied());
            }
        }
        
        // Add by class
        for class in &element.classes {
            if let Some(indices) = self.by_class.get(class) {
                candidates.extend(indices.iter().copied());
            }
        }
        
        // Deduplicate and sort
        candidates.sort_unstable();
        candidates.dedup();
        
        candidates
    }
    
    /// Stats
    pub fn stats(&self) -> IndexStats {
        IndexStats {
            types: self.by_type.len(),
            ids: self.by_id.len(),
            classes: self.by_class.len(),
            universal: self.universal.len(),
        }
    }
}

/// Indexable selector key
#[derive(Debug, Clone)]
pub enum IndexableSelector {
    Type(String),
    Id(String),
    Class(String),
    Universal,
}

/// Element info for matching
#[derive(Debug, Clone)]
pub struct ElementInfo {
    pub tag_name: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
}

/// Index statistics
#[derive(Debug, Clone)]
pub struct IndexStats {
    pub types: usize,
    pub ids: usize,
    pub classes: usize,
    pub universal: usize,
}

/// Right-to-left matcher
/// CSS selectors are matched from right to left for efficiency
#[derive(Debug)]
pub struct RtlMatcher {
    /// Compiled selectors
    selectors: Vec<CompiledSelector>,
}

/// Compiled selector for fast matching
#[derive(Debug, Clone)]
pub struct CompiledSelector {
    /// Selector parts (right to left)
    pub parts: Vec<CompiledPart>,
    /// Specificity
    pub specificity: u32,
    /// Rule index
    pub rule_index: usize,
}

/// Compiled selector part
#[derive(Debug, Clone)]
pub enum CompiledPart {
    /// Match element type
    Type(u32),  // Interned string ID
    /// Match class
    Class(u32),
    /// Match ID  
    Id(u32),
    /// Match attribute presence
    HasAttribute(u32),
    /// Match attribute value
    AttributeEquals(u32, u32), // attr_id, value_id
    /// Descendant combinator
    Descendant,
    /// Child combinator
    Child,
    /// Next sibling combinator
    NextSibling,
    /// Subsequent sibling combinator
    SubsequentSibling,
    /// Universal
    Universal,
}

impl RtlMatcher {
    pub fn new() -> Self {
        Self {
            selectors: Vec::new(),
        }
    }
    
    /// Add a compiled selector
    pub fn add(&mut self, selector: CompiledSelector) {
        self.selectors.push(selector);
    }
    
    /// Match element against all selectors
    pub fn match_element(&self, element: &MatchContext) -> Vec<MatchResult> {
        let mut results = Vec::new();
        
        for (idx, selector) in self.selectors.iter().enumerate() {
            if self.matches_selector(selector, element) {
                results.push(MatchResult {
                    selector_index: idx,
                    specificity: selector.specificity,
                    rule_index: selector.rule_index,
                });
            }
        }
        
        results
    }
    
    fn matches_selector(&self, selector: &CompiledSelector, element: &MatchContext) -> bool {
        // Start from the rightmost part (key selector)
        if selector.parts.is_empty() {
            return false;
        }
        
        // Check if key selector matches
        let key = &selector.parts[0];
        if !self.matches_part(key, element) {
            return false;
        }
        
        // Walk up the tree for ancestor selectors
        let mut current_element = element.parent.as_deref();
        let mut part_idx = 1;
        
        while part_idx < selector.parts.len() {
            let part = &selector.parts[part_idx];
            
            match part {
                CompiledPart::Descendant => {
                    // Find any ancestor that matches next part
                    part_idx += 1;
                    if part_idx >= selector.parts.len() {
                        break;
                    }
                    
                    let next_part = &selector.parts[part_idx];
                    loop {
                        match current_element {
                            Some(elem) => {
                                if self.matches_part(next_part, elem) {
                                    current_element = elem.parent.as_deref();
                                    part_idx += 1;
                                    break;
                                }
                                current_element = elem.parent.as_deref();
                            }
                            None => return false,
                        }
                    }
                }
                CompiledPart::Child => {
                    // Must match immediate parent
                    part_idx += 1;
                    if part_idx >= selector.parts.len() {
                        break;
                    }
                    
                    let next_part = &selector.parts[part_idx];
                    match current_element {
                        Some(elem) => {
                            if !self.matches_part(next_part, elem) {
                                return false;
                            }
                            current_element = elem.parent.as_deref();
                            part_idx += 1;
                        }
                        None => return false,
                    }
                }
                _ => {
                    // Simple part, check against current element
                    match current_element {
                        Some(elem) => {
                            if !self.matches_part(part, elem) {
                                return false;
                            }
                            part_idx += 1;
                        }
                        None => return false,
                    }
                }
            }
        }
        
        true
    }
    
    fn matches_part(&self, part: &CompiledPart, element: &MatchContext) -> bool {
        match part {
            CompiledPart::Type(id) => element.type_id == *id,
            CompiledPart::Class(id) => element.class_ids.contains(id),
            CompiledPart::Id(id) => element.id_id == Some(*id),
            CompiledPart::Universal => true,
            CompiledPart::HasAttribute(id) => element.attribute_ids.contains(id),
            CompiledPart::AttributeEquals(attr_id, value_id) => {
                element.attribute_values.get(attr_id) == Some(value_id)
            }
            _ => true, // Combinators handled separately
        }
    }
}

impl Default for RtlMatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Element context for matching
#[derive(Debug)]
pub struct MatchContext<'a> {
    pub type_id: u32,
    pub id_id: Option<u32>,
    pub class_ids: Vec<u32>,
    pub attribute_ids: Vec<u32>,
    pub attribute_values: HashMap<u32, u32>,
    pub parent: Option<Box<&'a MatchContext<'a>>>,
}

/// Match result
#[derive(Debug, Clone)]
pub struct MatchResult {
    pub selector_index: usize,
    pub specificity: u32,
    pub rule_index: usize,
}

/// Hybrid interpreter/compiler for selectors
#[derive(Debug)]
pub struct HybridSelector {
    /// Interpretation mode (simple selectors)
    interpreted: Vec<InterpretedSelector>,
    /// Compiled mode (complex selectors)
    compiled: Vec<CompiledSelector>,
    /// Compilation threshold
    compilation_threshold: u32,
    /// Hit counts for hot selectors
    hit_counts: HashMap<usize, u32>,
}

/// Simple interpreted selector
#[derive(Debug, Clone)]
pub struct InterpretedSelector {
    pub text: String,
    pub specificity: u32,
    pub rule_index: usize,
}

impl HybridSelector {
    pub fn new() -> Self {
        Self {
            interpreted: Vec::new(),
            compiled: Vec::new(),
            compilation_threshold: 10, // Compile after 10 hits
            hit_counts: HashMap::new(),
        }
    }
    
    /// Add a selector (starts in interpreted mode)
    pub fn add(&mut self, selector: InterpretedSelector) {
        let idx = self.interpreted.len();
        self.interpreted.push(selector);
        self.hit_counts.insert(idx, 0);
    }
    
    /// Record a hit on a selector, potentially triggering compilation
    pub fn record_hit(&mut self, idx: usize) {
        if let Some(count) = self.hit_counts.get_mut(&idx) {
            *count += 1;
            if *count >= self.compilation_threshold {
                self.compile(idx);
            }
        }
    }
    
    /// Compile a hot selector
    fn compile(&mut self, _idx: usize) {
        // In a real implementation:
        // 1. Parse the selector text
        // 2. Create CompiledSelector with interned IDs
        // 3. Move from interpreted to compiled
    }
    
    /// Get statistics
    pub fn stats(&self) -> HybridStats {
        HybridStats {
            interpreted: self.interpreted.len(),
            compiled: self.compiled.len(),
            hot_selectors: self.hit_counts.values().filter(|&&c| c >= self.compilation_threshold).count(),
        }
    }
}

impl Default for HybridSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// Hybrid selector stats
#[derive(Debug, Clone)]
pub struct HybridStats {
    pub interpreted: usize,
    pub compiled: usize,
    pub hot_selectors: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_selector_index() {
        let mut index = SelectorIndex::new();
        index.add(0, &IndexableSelector::Type("div".to_string()));
        index.add(1, &IndexableSelector::Class("foo".to_string()));
        index.add(2, &IndexableSelector::Id("bar".to_string()));
        
        let element = ElementInfo {
            tag_name: "div".to_string(),
            id: Some("bar".to_string()),
            classes: vec!["foo".to_string()],
        };
        
        let candidates = index.get_candidates(&element);
        assert!(candidates.contains(&0));
        assert!(candidates.contains(&1));
        assert!(candidates.contains(&2));
    }
    
    #[test]
    fn test_hybrid_selector() {
        let mut hybrid = HybridSelector::new();
        hybrid.add(InterpretedSelector {
            text: "div.foo".to_string(),
            specificity: 11,
            rule_index: 0,
        });
        
        for _ in 0..15 {
            hybrid.record_hit(0);
        }
        
        let stats = hybrid.stats();
        assert!(stats.hot_selectors >= 1);
    }
}
