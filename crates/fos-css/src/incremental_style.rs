//! Incremental Style Resolution (Phase 24.6)
//!
//! Only restyle changed subtrees. Track style dependencies.
//! Parallel style resolution. Cache computed styles.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Node ID type
pub type NodeId = u32;

/// Style rule ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuleId(pub u32);

/// Computed style (immutable, shareable)
#[derive(Debug, Clone)]
pub struct ComputedStyle {
    /// Style properties (simplified)
    pub properties: Arc<StyleProperties>,
    /// Rules that contributed
    pub contributing_rules: Vec<RuleId>,
    /// Parent style (for inheritance)
    pub parent: Option<Arc<ComputedStyle>>,
    /// Hash for comparison
    pub hash: u64,
}

/// Style properties (simplified)
#[derive(Debug, Clone, Default)]
pub struct StyleProperties {
    /// Display mode
    pub display: DisplayMode,
    /// Width/height
    pub width: Option<f32>,
    pub height: Option<f32>,
    /// Margins
    pub margin: [f32; 4],
    /// Padding
    pub padding: [f32; 4],
    /// Colors
    pub color: u32,
    pub background_color: u32,
    /// Font
    pub font_size: f32,
    pub font_weight: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DisplayMode {
    #[default]
    Block,
    Inline,
    Flex,
    Grid,
    None,
}

impl ComputedStyle {
    pub fn new(properties: StyleProperties) -> Self {
        let hash = Self::compute_hash(&properties);
        Self {
            properties: Arc::new(properties),
            contributing_rules: Vec::new(),
            parent: None,
            hash,
        }
    }
    
    fn compute_hash(props: &StyleProperties) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        props.color.hash(&mut hasher);
        props.background_color.hash(&mut hasher);
        (props.font_size as u32).hash(&mut hasher);
        hasher.finish()
    }
    
    /// Check if two styles are equivalent
    pub fn equivalent(&self, other: &ComputedStyle) -> bool {
        self.hash == other.hash
    }
}

/// Style dependency
#[derive(Debug, Clone)]
pub enum StyleDependency {
    /// Depends on parent style
    Parent,
    /// Depends on specific rule
    Rule(RuleId),
    /// Depends on sibling
    Sibling(NodeId),
    /// Depends on attribute
    Attribute(Box<str>),
    /// Depends on class
    Class(Box<str>),
    /// Depends on pseudo-class
    PseudoClass(PseudoClass),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoClass {
    Hover,
    Focus,
    Active,
    Visited,
    FirstChild,
    LastChild,
    NthChild,
}

/// Style cache entry
#[derive(Debug)]
struct CacheEntry {
    /// Computed style
    style: Arc<ComputedStyle>,
    /// Dependencies
    dependencies: Vec<StyleDependency>,
    /// Generation when computed
    generation: u64,
}

/// Incremental style resolver
#[derive(Debug)]
pub struct IncrementalStyleResolver {
    /// Cached computed styles
    cache: HashMap<NodeId, CacheEntry>,
    /// Current generation
    generation: u64,
    /// Dirty nodes
    dirty: HashSet<NodeId>,
    /// Nodes affected by rule changes
    rule_dependents: HashMap<RuleId, HashSet<NodeId>>,
    /// Nodes affected by class changes
    class_dependents: HashMap<Box<str>, HashSet<NodeId>>,
    /// Statistics
    stats: StyleStats,
}

/// Style resolution statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct StyleStats {
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub incremental_updates: u64,
    pub full_updates: u64,
    pub nodes_restyled: u64,
}

impl StyleStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.cache_hits + self.cache_misses;
        if total == 0 { 0.0 } else { self.cache_hits as f64 / total as f64 }
    }
}

impl Default for IncrementalStyleResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalStyleResolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            generation: 0,
            dirty: HashSet::new(),
            rule_dependents: HashMap::new(),
            class_dependents: HashMap::new(),
            stats: StyleStats::default(),
        }
    }
    
    /// Mark node as needing restyle
    pub fn mark_dirty(&mut self, node_id: NodeId) {
        self.dirty.insert(node_id);
    }
    
    /// Mark multiple nodes as dirty
    pub fn mark_dirty_batch(&mut self, nodes: &[NodeId]) {
        for &node in nodes {
            self.dirty.insert(node);
        }
    }
    
    /// Invalidate nodes depending on a rule
    pub fn invalidate_rule(&mut self, rule_id: RuleId) {
        if let Some(dependents) = self.rule_dependents.get(&rule_id) {
            for &node in dependents {
                self.dirty.insert(node);
            }
        }
    }
    
    /// Invalidate nodes depending on a class
    pub fn invalidate_class(&mut self, class: &str) {
        if let Some(dependents) = self.class_dependents.get(class) {
            for &node in dependents {
                self.dirty.insert(node);
            }
        }
    }
    
    /// Get cached style (if still valid)
    pub fn get_cached(&mut self, node_id: NodeId) -> Option<Arc<ComputedStyle>> {
        if self.dirty.contains(&node_id) {
            self.stats.cache_misses += 1;
            return None;
        }
        
        if let Some(entry) = self.cache.get(&node_id) {
            self.stats.cache_hits += 1;
            return Some(entry.style.clone());
        }
        
        self.stats.cache_misses += 1;
        None
    }
    
    /// Store computed style
    pub fn store(
        &mut self,
        node_id: NodeId,
        style: ComputedStyle,
        dependencies: Vec<StyleDependency>,
    ) {
        // Track rule dependencies
        for dep in &dependencies {
            match dep {
                StyleDependency::Rule(rule_id) => {
                    self.rule_dependents.entry(*rule_id).or_default().insert(node_id);
                }
                StyleDependency::Class(class) => {
                    self.class_dependents.entry(class.clone()).or_default().insert(node_id);
                }
                _ => {}
            }
        }
        
        let entry = CacheEntry {
            style: Arc::new(style),
            dependencies,
            generation: self.generation,
        };
        
        self.cache.insert(node_id, entry);
        self.dirty.remove(&node_id);
        self.stats.nodes_restyled += 1;
    }
    
    /// Check if any nodes need restyle
    pub fn has_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }
    
    /// Get dirty nodes
    pub fn dirty_nodes(&self) -> &HashSet<NodeId> {
        &self.dirty
    }
    
    /// Clear dirty set after processing
    pub fn clear_dirty(&mut self) {
        if !self.dirty.is_empty() {
            self.stats.incremental_updates += 1;
        }
        self.dirty.clear();
        self.generation += 1;
    }
    
    /// Full invalidation (e.g., stylesheet change)
    pub fn invalidate_all(&mut self) {
        for &node in self.cache.keys().collect::<Vec<_>>().iter() {
            self.dirty.insert(*node);
        }
        self.stats.full_updates += 1;
    }
    
    /// Get statistics
    pub fn stats(&self) -> &StyleStats {
        &self.stats
    }
    
    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.rule_dependents.clear();
        self.class_dependents.clear();
    }
}

/// Parallel style resolution batch
#[derive(Debug)]
pub struct StyleBatch {
    /// Nodes to process
    pub nodes: Vec<NodeId>,
    /// Parent styles (for inheritance)
    pub parent_styles: HashMap<NodeId, Arc<ComputedStyle>>,
}

impl StyleBatch {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            parent_styles: HashMap::new(),
        }
    }
    
    pub fn add(&mut self, node_id: NodeId, parent_style: Option<Arc<ComputedStyle>>) {
        self.nodes.push(node_id);
        if let Some(style) = parent_style {
            self.parent_styles.insert(node_id, style);
        }
    }
    
    /// Split batch for parallel processing
    pub fn split(&self, chunks: usize) -> Vec<StyleBatch> {
        let chunk_size = (self.nodes.len() + chunks - 1) / chunks;
        
        self.nodes.chunks(chunk_size)
            .map(|chunk| {
                let mut batch = StyleBatch::new();
                for &node in chunk {
                    batch.nodes.push(node);
                    if let Some(style) = self.parent_styles.get(&node) {
                        batch.parent_styles.insert(node, style.clone());
                    }
                }
                batch
            })
            .collect()
    }
}

impl Default for StyleBatch {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_incremental_resolver() {
        let mut resolver = IncrementalStyleResolver::new();
        
        // Store a style
        let style = ComputedStyle::new(StyleProperties::default());
        resolver.store(1, style.clone(), vec![]);
        
        // Should be cached
        let cached = resolver.get_cached(1);
        assert!(cached.is_some());
        
        // Mark dirty
        resolver.mark_dirty(1);
        let cached = resolver.get_cached(1);
        assert!(cached.is_none());
    }
    
    #[test]
    fn test_rule_invalidation() {
        let mut resolver = IncrementalStyleResolver::new();
        
        let style = ComputedStyle::new(StyleProperties::default());
        let deps = vec![StyleDependency::Rule(RuleId(1))];
        
        resolver.store(1, style.clone(), deps.clone());
        resolver.store(2, style.clone(), deps);
        
        // Invalidate rule 1
        resolver.invalidate_rule(RuleId(1));
        
        assert!(resolver.dirty_nodes().contains(&1));
        assert!(resolver.dirty_nodes().contains(&2));
    }
    
    #[test]
    fn test_class_invalidation() {
        let mut resolver = IncrementalStyleResolver::new();
        
        let style = ComputedStyle::new(StyleProperties::default());
        let deps = vec![StyleDependency::Class("active".into())];
        
        resolver.store(1, style, deps);
        
        resolver.invalidate_class("active");
        assert!(resolver.dirty_nodes().contains(&1));
        
        resolver.invalidate_class("other");
        // Node 1 was already dirty, should still be there
    }
    
    #[test]
    fn test_style_batch_split() {
        let mut batch = StyleBatch::new();
        
        for i in 0..10 {
            batch.add(i, None);
        }
        
        let splits = batch.split(3);
        assert_eq!(splits.len(), 3);
        
        let total: usize = splits.iter().map(|b| b.nodes.len()).sum();
        assert_eq!(total, 10);
    }
}
