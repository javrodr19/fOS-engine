//! Minimal Invalidation (Phase 24)
//!
//! Precise dirty tracking across layout, paint, and composite.
//! Avoid full-document invalidation. Track exactly what changed.

use std::collections::HashSet;

/// Node ID type  
pub type NodeId = u32;

/// What kind of invalidation is needed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvalidationType {
    /// Node content changed (text, children)
    Content,
    /// Style changed requiring layout
    StyleLayout,
    /// Style changed requiring paint only
    StylePaint,
    /// Style changed requiring composite only
    StyleComposite,
    /// Node was added
    Added,
    /// Node was removed
    Removed,
    /// Node was moved
    Moved,
    /// Scroll position changed
    Scroll,
}

impl InvalidationType {
    /// Does this type require layout?
    pub fn needs_layout(self) -> bool {
        matches!(self,
            InvalidationType::Content |
            InvalidationType::StyleLayout |
            InvalidationType::Added |
            InvalidationType::Removed |
            InvalidationType::Moved
        )
    }
    
    /// Does this type need paint?
    pub fn needs_paint(self) -> bool {
        matches!(self,
            InvalidationType::Content |
            InvalidationType::StyleLayout |
            InvalidationType::StylePaint |
            InvalidationType::Added |
            InvalidationType::Removed |
            InvalidationType::Moved |
            InvalidationType::Scroll
        )
    }
    
    /// Does this type need composite?
    pub fn needs_composite(self) -> bool {
        matches!(self,
            InvalidationType::StyleComposite |
            InvalidationType::Scroll
        )
    }
}

/// Invalidation rect (screen coordinates)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InvalidRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl InvalidRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Check if overlaps with another rect
    pub fn overlaps(&self, other: &InvalidRect) -> bool {
        self.x < other.x + other.width &&
        self.x + self.width > other.x &&
        self.y < other.y + other.height &&
        self.y + self.height > other.y
    }
    
    /// Union with another rect
    pub fn union(&self, other: &InvalidRect) -> InvalidRect {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let x2 = (self.x + self.width).max(other.x + other.width);
        let y2 = (self.y + self.height).max(other.y + other.height);
        
        InvalidRect::new(x, y, x2 - x, y2 - y)
    }
    
    /// Area
    pub fn area(&self) -> f32 {
        self.width * self.height
    }
}

/// Invalidation record
#[derive(Debug, Clone)]
pub struct Invalidation {
    /// Which node was invalidated
    pub node_id: NodeId,
    /// What type of invalidation
    pub inv_type: InvalidationType,
    /// Affected rect (if known)
    pub rect: Option<InvalidRect>,
}

impl Invalidation {
    pub fn new(node_id: NodeId, inv_type: InvalidationType) -> Self {
        Self {
            node_id,
            inv_type,
            rect: None,
        }
    }
    
    pub fn with_rect(mut self, rect: InvalidRect) -> Self {
        self.rect = Some(rect);
        self
    }
}

/// Minimal invalidation tracker
#[derive(Debug)]
pub struct InvalidationTracker {
    /// All pending invalidations
    invalidations: Vec<Invalidation>,
    /// Nodes needing layout
    needs_layout: HashSet<NodeId>,
    /// Nodes needing paint
    needs_paint: HashSet<NodeId>,
    /// Nodes needing composite
    needs_composite: HashSet<NodeId>,
    /// Dirty regions for paint
    dirty_regions: Vec<InvalidRect>,
    /// Statistics
    stats: InvalidationStats,
}

/// Invalidation statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct InvalidationStats {
    pub total_invalidations: u64,
    pub layout_invalidations: u64,
    pub paint_invalidations: u64,
    pub composite_invalidations: u64,
    pub regions_merged: u64,
    pub full_paints_avoided: u64,
}

impl Default for InvalidationTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl InvalidationTracker {
    pub fn new() -> Self {
        Self {
            invalidations: Vec::new(),
            needs_layout: HashSet::new(),
            needs_paint: HashSet::new(),
            needs_composite: HashSet::new(),
            dirty_regions: Vec::new(),
            stats: InvalidationStats::default(),
        }
    }
    
    /// Record an invalidation
    pub fn invalidate(&mut self, inv: Invalidation) {
        self.stats.total_invalidations += 1;
        
        // Categorize
        if inv.inv_type.needs_layout() {
            self.needs_layout.insert(inv.node_id);
            self.stats.layout_invalidations += 1;
        }
        if inv.inv_type.needs_paint() {
            self.needs_paint.insert(inv.node_id);
            self.stats.paint_invalidations += 1;
        }
        if inv.inv_type.needs_composite() {
            self.needs_composite.insert(inv.node_id);
            self.stats.composite_invalidations += 1;
        }
        
        // Track dirty region
        if let Some(rect) = inv.rect {
            self.add_dirty_region(rect);
        }
        
        self.invalidations.push(inv);
    }
    
    /// Add a dirty region (with merging)
    fn add_dirty_region(&mut self, rect: InvalidRect) {
        // Try to merge with existing region
        for existing in &mut self.dirty_regions {
            if existing.overlaps(&rect) {
                *existing = existing.union(&rect);
                self.stats.regions_merged += 1;
                return;
            }
        }
        
        // Add new region
        self.dirty_regions.push(rect);
    }
    
    /// Invalidate content change
    pub fn invalidate_content(&mut self, node_id: NodeId, rect: Option<InvalidRect>) {
        let mut inv = Invalidation::new(node_id, InvalidationType::Content);
        if let Some(r) = rect {
            inv = inv.with_rect(r);
        }
        self.invalidate(inv);
    }
    
    /// Invalidate style (layout variant)
    pub fn invalidate_style_layout(&mut self, node_id: NodeId, rect: Option<InvalidRect>) {
        let mut inv = Invalidation::new(node_id, InvalidationType::StyleLayout);
        if let Some(r) = rect {
            inv = inv.with_rect(r);
        }
        self.invalidate(inv);
    }
    
    /// Invalidate style (paint only)
    pub fn invalidate_style_paint(&mut self, node_id: NodeId, rect: Option<InvalidRect>) {
        let mut inv = Invalidation::new(node_id, InvalidationType::StylePaint);
        if let Some(r) = rect {
            inv = inv.with_rect(r);
        }
        self.invalidate(inv);
    }
    
    /// Invalidate style (composite only)
    pub fn invalidate_style_composite(&mut self, node_id: NodeId) {
        self.invalidate(Invalidation::new(node_id, InvalidationType::StyleComposite));
    }
    
    /// Check if any invalidation pending
    pub fn has_pending(&self) -> bool {
        !self.invalidations.is_empty()
    }
    
    /// Check if layout needed
    pub fn needs_layout(&self) -> bool {
        !self.needs_layout.is_empty()
    }
    
    /// Check if paint needed
    pub fn needs_paint(&self) -> bool {
        !self.needs_paint.is_empty()
    }
    
    /// Check if composite needed
    pub fn needs_composite(&self) -> bool {
        !self.needs_composite.is_empty()
    }
    
    /// Get nodes needing layout
    pub fn layout_nodes(&self) -> &HashSet<NodeId> {
        &self.needs_layout
    }
    
    /// Get dirty paint regions
    pub fn dirty_regions(&self) -> &[InvalidRect] {
        &self.dirty_regions
    }
    
    /// Get all pending invalidations
    pub fn pending(&self) -> &[Invalidation] {
        &self.invalidations
    }
    
    /// Clear after processing
    pub fn clear(&mut self) {
        if !self.dirty_regions.is_empty() && !self.needs_paint.is_empty() {
            // We did incremental paint instead of full
            self.stats.full_paints_avoided += 1;
        }
        
        self.invalidations.clear();
        self.needs_layout.clear();
        self.needs_paint.clear();
        self.needs_composite.clear();
        self.dirty_regions.clear();
    }
    
    /// Get statistics
    pub fn stats(&self) -> &InvalidationStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_invalidation_type() {
        assert!(InvalidationType::Content.needs_layout());
        assert!(InvalidationType::Content.needs_paint());
        
        assert!(!InvalidationType::StylePaint.needs_layout());
        assert!(InvalidationType::StylePaint.needs_paint());
        
        assert!(!InvalidationType::StyleComposite.needs_layout());
        assert!(!InvalidationType::StyleComposite.needs_paint());
        assert!(InvalidationType::StyleComposite.needs_composite());
    }
    
    #[test]
    fn test_rect_overlap() {
        let r1 = InvalidRect::new(0.0, 0.0, 100.0, 100.0);
        let r2 = InvalidRect::new(50.0, 50.0, 100.0, 100.0);
        let r3 = InvalidRect::new(200.0, 200.0, 50.0, 50.0);
        
        assert!(r1.overlaps(&r2));
        assert!(!r1.overlaps(&r3));
    }
    
    #[test]
    fn test_rect_union() {
        let r1 = InvalidRect::new(0.0, 0.0, 100.0, 100.0);
        let r2 = InvalidRect::new(50.0, 50.0, 100.0, 100.0);
        
        let u = r1.union(&r2);
        assert_eq!(u.x, 0.0);
        assert_eq!(u.y, 0.0);
        assert_eq!(u.width, 150.0);
        assert_eq!(u.height, 150.0);
    }
    
    #[test]
    fn test_invalidation_tracker() {
        let mut tracker = InvalidationTracker::new();
        
        // Invalidate with paint only
        tracker.invalidate_style_paint(1, Some(InvalidRect::new(0.0, 0.0, 50.0, 50.0)));
        
        assert!(tracker.needs_paint());
        assert!(!tracker.needs_layout());
        assert_eq!(tracker.dirty_regions().len(), 1);
        
        // Invalidate with layout
        tracker.invalidate_content(2, None);
        
        assert!(tracker.needs_layout());
        assert!(tracker.layout_nodes().contains(&2));
    }
    
    #[test]
    fn test_region_merging() {
        let mut tracker = InvalidationTracker::new();
        
        // Add overlapping regions
        tracker.invalidate_style_paint(1, Some(InvalidRect::new(0.0, 0.0, 100.0, 100.0)));
        tracker.invalidate_style_paint(2, Some(InvalidRect::new(50.0, 50.0, 100.0, 100.0)));
        
        // Should be merged into one region
        assert_eq!(tracker.dirty_regions().len(), 1);
        assert!(tracker.stats().regions_merged > 0);
    }
}
