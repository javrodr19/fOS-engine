//! Subtree Isolation Detection
//!
//! Identifies independent subtrees that can be styled in parallel.

use std::collections::{HashMap, HashSet};

/// Element ID type
pub type ElementId = usize;

/// Subtree independence analysis result
#[derive(Debug, Clone)]
pub struct SubtreeAnalysis {
    /// Independent subtrees (can be styled in parallel)
    pub independent_subtrees: Vec<SubtreeId>,
    /// Elements affected by :has() selectors
    pub has_affected: HashSet<ElementId>,
    /// Elements with inherited custom properties
    pub custom_prop_inheritors: HashSet<ElementId>,
    /// Shadow roots (always independent)
    pub shadow_roots: Vec<ElementId>,
}

/// Subtree identifier
#[derive(Debug, Clone)]
pub struct SubtreeId {
    /// Root element of the subtree
    pub root: ElementId,
    /// All elements in the subtree
    pub elements: Vec<ElementId>,
    /// Why this subtree is independent
    pub reason: IndependenceReason,
}

/// Why a subtree is independent
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndependenceReason {
    /// Shadow DOM boundary
    ShadowRoot,
    /// No inherited custom properties
    NoCustomProps,
    /// contain: style applied
    ContainStyle,
    /// Isolated by CSS containment
    CssContainment,
}

/// Element info for analysis
#[derive(Debug, Clone)]
pub struct AnalysisElement {
    /// Element ID
    pub id: ElementId,
    /// Parent ID
    pub parent_id: Option<ElementId>,
    /// Children IDs
    pub children: Vec<ElementId>,
    /// Is a shadow root
    pub is_shadow_root: bool,
    /// Has custom properties defined
    pub defines_custom_props: bool,
    /// Uses custom properties via var()
    pub uses_custom_props: bool,
    /// Has contain: style
    pub has_contain_style: bool,
    /// Matched by :has() selector
    pub matched_by_has: bool,
}

/// Selectors that affect subtree independence
#[derive(Debug, Clone)]
pub struct SelectorDependencies {
    /// :has() selectors in use
    pub has_selectors: Vec<HasSelector>,
}

/// Parsed :has() selector
#[derive(Debug, Clone)]
pub struct HasSelector {
    /// Elements this :has() could match
    pub subject_selector: String,
    /// The argument to :has()
    pub argument: String,
}

/// Identify independent subtrees for parallel styling
pub fn identify_independent_subtrees(
    elements: &[AnalysisElement],
    dependencies: &SelectorDependencies,
) -> SubtreeAnalysis {
    let mut analysis = SubtreeAnalysis {
        independent_subtrees: Vec::new(),
        has_affected: HashSet::new(),
        custom_prop_inheritors: HashSet::new(),
        shadow_roots: Vec::new(),
    };
    
    // Build element lookup
    let element_map: HashMap<ElementId, &AnalysisElement> = elements
        .iter()
        .map(|e| (e.id, e))
        .collect();
    
    // Find elements affected by :has()
    for selector in &dependencies.has_selectors {
        for elem in elements {
            if selector_might_affect(elem, selector) {
                analysis.has_affected.insert(elem.id);
                
                // :has() can affect ancestors too
                let mut parent = elem.parent_id;
                while let Some(pid) = parent {
                    analysis.has_affected.insert(pid);
                    parent = element_map.get(&pid).and_then(|p| p.parent_id);
                }
            }
        }
    }
    
    // Find custom property inheritance chains
    for elem in elements {
        if elem.defines_custom_props || elem.uses_custom_props {
            analysis.custom_prop_inheritors.insert(elem.id);
            
            // Mark ancestor chain
            let mut parent = elem.parent_id;
            while let Some(pid) = parent {
                analysis.custom_prop_inheritors.insert(pid);
                parent = element_map.get(&pid).and_then(|p| p.parent_id);
            }
        }
    }
    
    // Find shadow roots (always independent)
    for elem in elements {
        if elem.is_shadow_root {
            analysis.shadow_roots.push(elem.id);
        }
    }
    
    // Identify independent subtrees
    for elem in elements {
        if is_independent_root(elem, &analysis, &element_map) {
            let subtree_elements = collect_subtree(elem.id, &element_map);
            let reason = determine_independence_reason(elem, &analysis);
            
            analysis.independent_subtrees.push(SubtreeId {
                root: elem.id,
                elements: subtree_elements,
                reason,
            });
        }
    }
    
    analysis
}

/// Check if an element is the root of an independent subtree
fn is_independent_root(
    elem: &AnalysisElement,
    analysis: &SubtreeAnalysis,
    _element_map: &HashMap<ElementId, &AnalysisElement>,
) -> bool {
    // Shadow roots are always independent
    if elem.is_shadow_root {
        return true;
    }
    
    // Elements with contain: style are independent
    if elem.has_contain_style {
        return true;
    }
    
    // Not independent if affected by :has() from outside
    if analysis.has_affected.contains(&elem.id) {
        return false;
    }
    
    // Not independent if inheriting custom properties
    if analysis.custom_prop_inheritors.contains(&elem.id) {
        // Unless this element defines the properties
        if !elem.defines_custom_props {
            return false;
        }
    }
    
    // Check if this could be a parallel styling boundary
    // Elements with no external dependencies can be styled independently
    !elem.uses_custom_props && !elem.matched_by_has
}

/// Determine why a subtree is independent
fn determine_independence_reason(
    elem: &AnalysisElement,
    _analysis: &SubtreeAnalysis,
) -> IndependenceReason {
    if elem.is_shadow_root {
        IndependenceReason::ShadowRoot
    } else if elem.has_contain_style {
        IndependenceReason::ContainStyle
    } else {
        IndependenceReason::NoCustomProps
    }
}

/// Collect all elements in a subtree
fn collect_subtree(
    root: ElementId,
    element_map: &HashMap<ElementId, &AnalysisElement>,
) -> Vec<ElementId> {
    let mut result = vec![root];
    
    if let Some(elem) = element_map.get(&root) {
        for &child_id in &elem.children {
            result.extend(collect_subtree(child_id, element_map));
        }
    }
    
    result
}

/// Check if a :has() selector might affect an element
fn selector_might_affect(elem: &AnalysisElement, selector: &HasSelector) -> bool {
    // Simple heuristic - actual implementation would need full selector matching
    // For now, assume any element could be affected
    elem.matched_by_has || !selector.argument.is_empty()
}

/// Partition elements into groups that can be processed in parallel
pub fn partition_for_parallel(
    elements: &[AnalysisElement],
    analysis: &SubtreeAnalysis,
) -> Vec<Vec<ElementId>> {
    let mut partitions = Vec::new();
    let mut processed = HashSet::new();
    
    // Add independent subtrees as separate partitions
    for subtree in &analysis.independent_subtrees {
        let partition: Vec<_> = subtree.elements.iter()
            .copied()
            .filter(|id| !processed.contains(id))
            .collect();
        
        for &id in &partition {
            processed.insert(id);
        }
        
        if !partition.is_empty() {
            partitions.push(partition);
        }
    }
    
    // Remaining elements go into a sequential partition
    let remaining: Vec<_> = elements.iter()
        .map(|e| e.id)
        .filter(|id| !processed.contains(id))
        .collect();
    
    if !remaining.is_empty() {
        partitions.push(remaining);
    }
    
    partitions
}

/// Builder for analysis elements
pub struct AnalysisBuilder {
    elements: Vec<AnalysisElement>,
    dependencies: SelectorDependencies,
}

impl AnalysisBuilder {
    /// Create new builder
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            dependencies: SelectorDependencies {
                has_selectors: Vec::new(),
            },
        }
    }
    
    /// Add an element
    pub fn add_element(&mut self, elem: AnalysisElement) {
        self.elements.push(elem);
    }
    
    /// Add a :has() selector
    pub fn add_has_selector(&mut self, subject: &str, argument: &str) {
        self.dependencies.has_selectors.push(HasSelector {
            subject_selector: subject.to_string(),
            argument: argument.to_string(),
        });
    }
    
    /// Build the analysis
    pub fn analyze(&self) -> SubtreeAnalysis {
        identify_independent_subtrees(&self.elements, &self.dependencies)
    }
}

impl Default for AnalysisBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_shadow_root_independence() {
        let elements = vec![
            AnalysisElement {
                id: 0,
                parent_id: None,
                children: vec![1],
                is_shadow_root: false,
                defines_custom_props: false,
                uses_custom_props: false,
                has_contain_style: false,
                matched_by_has: false,
            },
            AnalysisElement {
                id: 1,
                parent_id: Some(0),
                children: vec![2],
                is_shadow_root: true,
                defines_custom_props: false,
                uses_custom_props: false,
                has_contain_style: false,
                matched_by_has: false,
            },
            AnalysisElement {
                id: 2,
                parent_id: Some(1),
                children: vec![],
                is_shadow_root: false,
                defines_custom_props: false,
                uses_custom_props: false,
                has_contain_style: false,
                matched_by_has: false,
            },
        ];
        
        let deps = SelectorDependencies { has_selectors: vec![] };
        let analysis = identify_independent_subtrees(&elements, &deps);
        
        assert!(analysis.shadow_roots.contains(&1));
        assert!(analysis.independent_subtrees.iter().any(|s| s.root == 1));
    }
    
    #[test]
    fn test_custom_property_chain() {
        let elements = vec![
            AnalysisElement {
                id: 0,
                parent_id: None,
                children: vec![1],
                is_shadow_root: false,
                defines_custom_props: true,
                uses_custom_props: false,
                has_contain_style: false,
                matched_by_has: false,
            },
            AnalysisElement {
                id: 1,
                parent_id: Some(0),
                children: vec![],
                is_shadow_root: false,
                defines_custom_props: false,
                uses_custom_props: true,
                has_contain_style: false,
                matched_by_has: false,
            },
        ];
        
        let deps = SelectorDependencies { has_selectors: vec![] };
        let analysis = identify_independent_subtrees(&elements, &deps);
        
        // Both should be marked as custom prop inheritors
        assert!(analysis.custom_prop_inheritors.contains(&0));
        assert!(analysis.custom_prop_inheritors.contains(&1));
    }
    
    #[test]
    fn test_contain_style_independence() {
        let elements = vec![
            AnalysisElement {
                id: 0,
                parent_id: None,
                children: vec![1],
                is_shadow_root: false,
                defines_custom_props: false,
                uses_custom_props: false,
                has_contain_style: true,
                matched_by_has: false,
            },
            AnalysisElement {
                id: 1,
                parent_id: Some(0),
                children: vec![],
                is_shadow_root: false,
                defines_custom_props: false,
                uses_custom_props: false,
                has_contain_style: false,
                matched_by_has: false,
            },
        ];
        
        let deps = SelectorDependencies { has_selectors: vec![] };
        let analysis = identify_independent_subtrees(&elements, &deps);
        
        // Element 0 should be independent due to contain: style
        assert!(analysis.independent_subtrees.iter().any(|s| 
            s.root == 0 && s.reason == IndependenceReason::ContainStyle
        ));
    }
}
