//! Style Inheritance Snapshots (Phase 24.3)
//!
//! Freeze inherited style at element. Compare to parent on mutation.
//! Skip cascade if parent unchanged. Incremental recalculation.

use std::collections::HashMap;
use std::sync::Arc;

/// Node ID type
pub type NodeId = u32;

/// Style generation
pub type StyleGeneration = u32;

/// Inherited style properties (simplified)
#[derive(Debug, Clone, PartialEq)]
pub struct InheritedStyle {
    /// Font family
    pub font_family: Arc<str>,
    /// Font size (px)
    pub font_size: f32,
    /// Font weight
    pub font_weight: u16,
    /// Line height
    pub line_height: f32,
    /// Text color
    pub color: u32,
    /// Text align
    pub text_align: TextAlign,
    /// Visibility
    pub visibility: Visibility,
    /// Cursor
    pub cursor: Cursor,
    /// Direction
    pub direction: Direction,
    /// White space
    pub white_space: WhiteSpace,
    /// Word break
    pub word_break: WordBreak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Right,
    Center,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cursor {
    #[default]
    Auto,
    Pointer,
    Text,
    Wait,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordBreak {
    #[default]
    Normal,
    BreakAll,
    KeepAll,
    BreakWord,
}

impl Default for InheritedStyle {
    fn default() -> Self {
        Self {
            font_family: Arc::from("sans-serif"),
            font_size: 16.0,
            font_weight: 400,
            line_height: 1.2,
            color: 0x000000FF, // Black
            text_align: TextAlign::Left,
            visibility: Visibility::Visible,
            cursor: Cursor::Auto,
            direction: Direction::Ltr,
            white_space: WhiteSpace::Normal,
            word_break: WordBreak::Normal,
        }
    }
}

impl InheritedStyle {
    /// Hash for quick comparison
    pub fn hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.font_family.hash(&mut hasher);
        self.font_size.to_bits().hash(&mut hasher);
        self.font_weight.hash(&mut hasher);
        self.color.hash(&mut hasher);
        hasher.finish()
    }
}

/// Style snapshot
#[derive(Debug, Clone)]
pub struct StyleSnapshot {
    /// The inherited style
    pub style: Arc<InheritedStyle>,
    /// Hash for quick comparison
    pub hash: u64,
    /// Generation when snapshot was taken
    pub generation: StyleGeneration,
    /// Parent's hash at snapshot time
    pub parent_hash: u64,
}

impl StyleSnapshot {
    pub fn new(style: InheritedStyle, generation: StyleGeneration, parent_hash: u64) -> Self {
        let hash = style.hash();
        Self {
            style: Arc::new(style),
            hash,
            generation,
            parent_hash,
        }
    }
    
    /// Check if snapshot is still valid
    pub fn is_valid(&self, current_parent_hash: u64);
    
    /// Check if parent changed
    pub fn parent_changed(&self, current_parent_hash: u64) -> bool {
        self.parent_hash != current_parent_hash
    }
}

impl StyleSnapshot {
    fn is_valid(&self, current_parent_hash: u64) -> bool {
        self.parent_hash == current_parent_hash
    }
}

/// Inheritance manager
#[derive(Debug)]
pub struct InheritanceManager {
    /// Snapshots per node
    snapshots: HashMap<NodeId, StyleSnapshot>,
    /// Current generation
    generation: StyleGeneration,
    /// Statistics
    stats: InheritanceStats,
}

/// Inheritance statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct InheritanceStats {
    pub snapshots_created: u64,
    pub cascades_skipped: u64,
    pub cascades_performed: u64,
    pub invalidations: u64,
}

impl InheritanceStats {
    pub fn skip_ratio(&self) -> f64 {
        let total = self.cascades_skipped + self.cascades_performed;
        if total == 0 {
            0.0
        } else {
            self.cascades_skipped as f64 / total as f64
        }
    }
}

impl Default for InheritanceManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InheritanceManager {
    pub fn new() -> Self {
        Self {
            snapshots: HashMap::new(),
            generation: 0,
            stats: InheritanceStats::default(),
        }
    }
    
    /// Create or update snapshot
    pub fn snapshot(&mut self, node: NodeId, style: InheritedStyle, parent_hash: u64) {
        let snapshot = StyleSnapshot::new(style, self.generation, parent_hash);
        self.snapshots.insert(node, snapshot);
        self.stats.snapshots_created += 1;
    }
    
    /// Get snapshot
    pub fn get_snapshot(&self, node: NodeId) -> Option<&StyleSnapshot> {
        self.snapshots.get(&node)
    }
    
    /// Check if cascade can be skipped
    pub fn can_skip_cascade(&mut self, node: NodeId, parent_hash: u64) -> Option<&InheritedStyle> {
        if let Some(snapshot) = self.snapshots.get(&node) {
            if snapshot.is_valid(parent_hash) {
                self.stats.cascades_skipped += 1;
                return Some(&snapshot.style);
            }
        }
        
        self.stats.cascades_performed += 1;
        None
    }
    
    /// Invalidate a node's snapshot
    pub fn invalidate(&mut self, node: NodeId) {
        self.snapshots.remove(&node);
        self.stats.invalidations += 1;
    }
    
    /// Invalidate all snapshots
    pub fn invalidate_all(&mut self) {
        self.snapshots.clear();
        self.generation += 1;
    }
    
    /// Get statistics
    pub fn stats(&self) -> &InheritanceStats {
        &self.stats
    }
    
    /// Get inherited style, computing if necessary
    pub fn get_inherited(
        &mut self,
        node: NodeId,
        parent: Option<NodeId>,
        compute_fn: impl FnOnce(Option<&InheritedStyle>) -> InheritedStyle,
    ) -> Arc<InheritedStyle> {
        let parent_hash = parent
            .and_then(|p| self.snapshots.get(&p))
            .map(|s| s.hash)
            .unwrap_or(0);
        
        // Try to use cached
        if let Some(cached) = self.can_skip_cascade(node, parent_hash) {
            return Arc::new(cached.clone());
        }
        
        // Compute new style
        let parent_style = parent
            .and_then(|p| self.snapshots.get(&p))
            .map(|s| &*s.style);
        
        let new_style = compute_fn(parent_style);
        self.snapshot(node, new_style.clone(), parent_hash);
        
        Arc::new(new_style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_inherited_style_default() {
        let style = InheritedStyle::default();
        assert_eq!(style.font_size, 16.0);
        assert_eq!(style.color, 0x000000FF);
    }
    
    #[test]
    fn test_snapshot_validity() {
        let style = InheritedStyle::default();
        let snapshot = StyleSnapshot::new(style, 0, 12345);
        
        assert!(snapshot.is_valid(12345));
        assert!(!snapshot.is_valid(67890));
    }
    
    #[test]
    fn test_cascade_skip() {
        let mut manager = InheritanceManager::new();
        
        let style = InheritedStyle::default();
        let parent_hash = 12345;
        
        manager.snapshot(1, style.clone(), parent_hash);
        
        // Should skip cascade
        let cached = manager.can_skip_cascade(1, parent_hash);
        assert!(cached.is_some());
        assert_eq!(manager.stats().cascades_skipped, 1);
        
        // Should not skip if parent changed
        let cached = manager.can_skip_cascade(1, 99999);
        assert!(cached.is_none());
        assert_eq!(manager.stats().cascades_performed, 1);
    }
    
    #[test]
    fn test_get_inherited() {
        let mut manager = InheritanceManager::new();
        
        // First call computes
        let style1 = manager.get_inherited(1, None, |_| InheritedStyle::default());
        
        // Second call uses cache
        let style2 = manager.get_inherited(1, None, |_| panic!("Should not be called"));
        
        assert_eq!(style1.font_size, style2.font_size);
        assert_eq!(manager.stats().cascades_skipped, 1);
    }
}
