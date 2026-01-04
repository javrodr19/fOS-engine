//! Partial Invalidation
//!
//! Fine-grained partial invalidation for minimal re-rendering.
//! Track exactly what changed to avoid full document repaints.

use std::collections::{HashMap, HashSet};

/// Node ID type
pub type NodeId = u32;

/// Invalidation flags (bitfield)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InvalidationFlags(u16);

impl InvalidationFlags {
    /// Nothing invalidated
    pub const NONE: Self = Self(0);
    /// Layout invalidated
    pub const LAYOUT: Self = Self(1 << 0);
    /// Paint invalidated
    pub const PAINT: Self = Self(1 << 1);
    /// Composite invalidated
    pub const COMPOSITE: Self = Self(1 << 2);
    /// Text content changed
    pub const TEXT: Self = Self(1 << 3);
    /// Attributes changed
    pub const ATTRIBUTES: Self = Self(1 << 4);
    /// Children changed
    pub const CHILDREN: Self = Self(1 << 5);
    /// Transform changed
    pub const TRANSFORM: Self = Self(1 << 6);
    /// Opacity changed
    pub const OPACITY: Self = Self(1 << 7);
    /// Filter changed
    pub const FILTER: Self = Self(1 << 8);
    /// Clip changed
    pub const CLIP: Self = Self(1 << 9);
    /// Visibility changed
    pub const VISIBILITY: Self = Self(1 << 10);
    /// All flags set
    pub const ALL: Self = Self(0xFFFF);
    
    /// Create new flags
    pub fn new() -> Self {
        Self::NONE
    }
    
    /// Set flag
    pub fn set(&mut self, flag: InvalidationFlags) {
        self.0 |= flag.0;
    }
    
    /// Clear flag
    pub fn clear(&mut self, flag: InvalidationFlags) {
        self.0 &= !flag.0;
    }
    
    /// Check if flag is set
    pub fn has(self, flag: InvalidationFlags) -> bool {
        (self.0 & flag.0) != 0
    }
    
    /// Check if any flag is set
    pub fn any(self) -> bool {
        self.0 != 0
    }
    
    /// Check if needs layout
    pub fn needs_layout(self) -> bool {
        self.has(Self::LAYOUT) || self.has(Self::CHILDREN) || self.has(Self::TEXT)
    }
    
    /// Check if needs paint
    pub fn needs_paint(self) -> bool {
        self.needs_layout() || self.has(Self::PAINT) || self.has(Self::ATTRIBUTES)
    }
    
    /// Check if needs composite only
    pub fn needs_composite_only(self) -> bool {
        !self.needs_paint() && (
            self.has(Self::COMPOSITE) || 
            self.has(Self::TRANSFORM) || 
            self.has(Self::OPACITY) ||
            self.has(Self::FILTER)
        )
    }
    
    /// Merge with other flags
    pub fn merge(&mut self, other: InvalidationFlags) {
        self.0 |= other.0;
    }
    
    /// Get underlying bits
    pub fn bits(self) -> u16 {
        self.0
    }
}

/// Scope of invalidation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvalidationScope {
    /// Only this node
    Self_,
    /// This node and direct children
    Children,
    /// This node and all descendants
    Subtree,
    /// Only following siblings
    Siblings,
    /// All (needs full relayout)
    All,
}

/// Dirty region for repaint
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirtyRegion {
    /// X position
    pub x: f32,
    /// Y position
    pub y: f32,
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
    /// Layer ID (if specific to a layer)
    pub layer_id: Option<u32>,
}

impl DirtyRegion {
    /// Create new region
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            layer_id: None,
        }
    }
    
    /// Create with layer
    pub fn with_layer(x: f32, y: f32, width: f32, height: f32, layer_id: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
            layer_id: Some(layer_id),
        }
    }
    
    /// Full viewport
    pub fn full_viewport(width: f32, height: f32) -> Self {
        Self::new(0.0, 0.0, width, height)
    }
    
    /// Check if overlaps with another region
    pub fn overlaps(&self, other: &DirtyRegion) -> bool {
        self.x < other.x + other.width &&
        self.x + self.width > other.x &&
        self.y < other.y + other.height &&
        self.y + self.height > other.y
    }
    
    /// Union with another region
    pub fn union(&self, other: &DirtyRegion) -> DirtyRegion {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        
        DirtyRegion {
            x,
            y,
            width: right - x,
            height: bottom - y,
            layer_id: self.layer_id.or(other.layer_id),
        }
    }
    
    /// Area
    pub fn area(&self) -> f32 {
        self.width * self.height
    }
    
    /// Expand by amount
    pub fn expand(&self, amount: f32) -> DirtyRegion {
        DirtyRegion {
            x: self.x - amount,
            y: self.y - amount,
            width: self.width + 2.0 * amount,
            height: self.height + 2.0 * amount,
            layer_id: self.layer_id,
        }
    }
}

/// Per-layer invalidation tracking
#[derive(Debug, Clone)]
pub struct LayerInvalidation {
    /// Layer ID
    pub layer_id: u32,
    /// Dirty regions within this layer
    pub regions: Vec<DirtyRegion>,
    /// Full repaint needed
    pub full_repaint: bool,
    /// Content changed
    pub content_changed: bool,
    /// Transform changed (can composite)
    pub transform_changed: bool,
}

impl LayerInvalidation {
    /// Create new layer invalidation
    pub fn new(layer_id: u32) -> Self {
        Self {
            layer_id,
            regions: Vec::new(),
            full_repaint: false,
            content_changed: false,
            transform_changed: false,
        }
    }
    
    /// Add dirty region
    pub fn add_region(&mut self, region: DirtyRegion) {
        if self.full_repaint {
            return;
        }
        
        // Merge overlapping regions
        for existing in &mut self.regions {
            if existing.overlaps(&region) {
                *existing = existing.union(&region);
                return;
            }
        }
        
        self.regions.push(region);
    }
    
    /// Mark full repaint
    pub fn mark_full_repaint(&mut self) {
        self.full_repaint = true;
        self.regions.clear();
    }
    
    /// Check if needs repaint
    pub fn needs_repaint(&self) -> bool {
        self.full_repaint || !self.regions.is_empty()
    }
}

/// CSS property invalidation tracking
#[derive(Debug, Clone, Default)]
pub struct PropertyInvalidation {
    /// Properties that changed
    changed: HashSet<u16>,
}

impl PropertyInvalidation {
    /// Predefined property IDs
    pub const DISPLAY: u16 = 1;
    pub const POSITION: u16 = 2;
    pub const WIDTH: u16 = 3;
    pub const HEIGHT: u16 = 4;
    pub const MARGIN: u16 = 5;
    pub const PADDING: u16 = 6;
    pub const BORDER: u16 = 7;
    pub const BACKGROUND: u16 = 8;
    pub const COLOR: u16 = 9;
    pub const FONT: u16 = 10;
    pub const TRANSFORM: u16 = 11;
    pub const OPACITY: u16 = 12;
    pub const FILTER: u16 = 13;
    pub const VISIBILITY: u16 = 14;
    pub const OVERFLOW: u16 = 15;
    pub const FLEX: u16 = 16;
    pub const GRID: u16 = 17;
    pub const Z_INDEX: u16 = 18;
    
    /// Create new
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Mark property changed
    pub fn mark_changed(&mut self, property_id: u16) {
        self.changed.insert(property_id);
    }
    
    /// Check if property changed
    pub fn is_changed(&self, property_id: u16) -> bool {
        self.changed.contains(&property_id)
    }
    
    /// Get invalidation flags for changed properties
    pub fn to_flags(&self) -> InvalidationFlags {
        let mut flags = InvalidationFlags::NONE;
        
        for &prop in &self.changed {
            match prop {
                Self::DISPLAY | Self::POSITION | Self::WIDTH | Self::HEIGHT |
                Self::MARGIN | Self::PADDING | Self::FLEX | Self::GRID => {
                    flags.set(InvalidationFlags::LAYOUT);
                }
                Self::BORDER | Self::BACKGROUND | Self::COLOR | Self::FONT => {
                    flags.set(InvalidationFlags::PAINT);
                }
                Self::TRANSFORM => {
                    flags.set(InvalidationFlags::TRANSFORM);
                }
                Self::OPACITY => {
                    flags.set(InvalidationFlags::OPACITY);
                }
                Self::FILTER => {
                    flags.set(InvalidationFlags::FILTER);
                }
                Self::VISIBILITY => {
                    flags.set(InvalidationFlags::VISIBILITY);
                }
                Self::OVERFLOW => {
                    flags.set(InvalidationFlags::CLIP);
                }
                Self::Z_INDEX => {
                    flags.set(InvalidationFlags::COMPOSITE);
                }
                _ => {
                    flags.set(InvalidationFlags::PAINT);
                }
            }
        }
        
        flags
    }
    
    /// Clear
    pub fn clear(&mut self) {
        self.changed.clear();
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.changed.is_empty()
    }
}

/// Node invalidation record
#[derive(Debug, Clone)]
pub struct NodeInvalidation {
    /// Node ID
    pub node_id: NodeId,
    /// Invalidation flags
    pub flags: InvalidationFlags,
    /// Scope
    pub scope: InvalidationScope,
    /// Dirty region (if known)
    pub region: Option<DirtyRegion>,
    /// Property changes
    pub properties: PropertyInvalidation,
}

impl NodeInvalidation {
    /// Create new
    pub fn new(node_id: NodeId, flags: InvalidationFlags) -> Self {
        Self {
            node_id,
            flags,
            scope: InvalidationScope::Self_,
            region: None,
            properties: PropertyInvalidation::new(),
        }
    }
    
    /// Set scope
    pub fn with_scope(mut self, scope: InvalidationScope) -> Self {
        self.scope = scope;
        self
    }
    
    /// Set region
    pub fn with_region(mut self, region: DirtyRegion) -> Self {
        self.region = Some(region);
        self
    }
}

/// Partial invalidation tracker
#[derive(Debug)]
pub struct PartialInvalidationTracker {
    /// Node invalidations
    nodes: HashMap<NodeId, NodeInvalidation>,
    /// Layer invalidations
    layers: HashMap<u32, LayerInvalidation>,
    /// Global dirty regions
    dirty_regions: Vec<DirtyRegion>,
    /// Needs full layout
    needs_full_layout: bool,
    /// Needs full paint
    needs_full_paint: bool,
    /// Statistics
    stats: InvalidationStats,
}

/// Invalidation statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct InvalidationStats {
    /// Total invalidations
    pub total_invalidations: usize,
    /// Layout invalidations
    pub layout_invalidations: usize,
    /// Paint invalidations
    pub paint_invalidations: usize,
    /// Composite-only invalidations
    pub composite_invalidations: usize,
    /// Full invalidations avoided
    pub full_avoided: usize,
    /// Regions merged
    pub regions_merged: usize,
}

impl Default for PartialInvalidationTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl PartialInvalidationTracker {
    /// Create new tracker
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            layers: HashMap::new(),
            dirty_regions: Vec::new(),
            needs_full_layout: false,
            needs_full_paint: false,
            stats: InvalidationStats::default(),
        }
    }
    
    /// Invalidate a node
    pub fn invalidate(&mut self, inv: NodeInvalidation) {
        self.stats.total_invalidations += 1;
        
        if inv.flags.needs_layout() {
            self.stats.layout_invalidations += 1;
        } else if inv.flags.needs_paint() {
            self.stats.paint_invalidations += 1;
        } else if inv.flags.needs_composite_only() {
            self.stats.composite_invalidations += 1;
        }
        
        // Check for full invalidation
        if inv.scope == InvalidationScope::All {
            self.needs_full_layout = true;
            self.needs_full_paint = true;
        }
        
        // Merge with existing
        if let Some(existing) = self.nodes.get_mut(&inv.node_id) {
            existing.flags.merge(inv.flags);
            if inv.scope as u8 > existing.scope as u8 {
                existing.scope = inv.scope;
            }
            if let Some(region) = inv.region {
                if let Some(ref mut existing_region) = existing.region {
                    *existing_region = existing_region.union(&region);
                    self.stats.regions_merged += 1;
                } else {
                    existing.region = Some(region);
                }
            }
        } else {
            self.nodes.insert(inv.node_id, inv);
        }
    }
    
    /// Invalidate layout for node
    pub fn invalidate_layout(&mut self, node_id: NodeId) {
        self.invalidate(NodeInvalidation::new(node_id, InvalidationFlags::LAYOUT));
    }
    
    /// Invalidate paint for node
    pub fn invalidate_paint(&mut self, node_id: NodeId, region: Option<DirtyRegion>) {
        let mut inv = NodeInvalidation::new(node_id, InvalidationFlags::PAINT);
        if let Some(r) = region {
            inv = inv.with_region(r);
        }
        self.invalidate(inv);
    }
    
    /// Invalidate transform (composite only)
    pub fn invalidate_transform(&mut self, node_id: NodeId) {
        self.invalidate(NodeInvalidation::new(node_id, InvalidationFlags::TRANSFORM));
    }
    
    /// Invalidate opacity (composite only)
    pub fn invalidate_opacity(&mut self, node_id: NodeId) {
        self.invalidate(NodeInvalidation::new(node_id, InvalidationFlags::OPACITY));
    }
    
    /// Invalidate children changed
    pub fn invalidate_children(&mut self, node_id: NodeId) {
        let inv = NodeInvalidation::new(node_id, InvalidationFlags::CHILDREN)
            .with_scope(InvalidationScope::Children);
        self.invalidate(inv);
    }
    
    /// Invalidate layer
    pub fn invalidate_layer(&mut self, layer_id: u32, region: DirtyRegion) {
        let layer = self.layers
            .entry(layer_id)
            .or_insert_with(|| LayerInvalidation::new(layer_id));
        layer.add_region(region);
    }
    
    /// Add dirty region
    pub fn add_dirty_region(&mut self, region: DirtyRegion) {
        // Merge overlapping regions
        for existing in &mut self.dirty_regions {
            if existing.overlaps(&region) {
                *existing = existing.union(&region);
                self.stats.regions_merged += 1;
                return;
            }
        }
        self.dirty_regions.push(region);
    }
    
    /// Check if needs layout
    pub fn needs_layout(&self) -> bool {
        self.needs_full_layout || self.nodes.values().any(|n| n.flags.needs_layout())
    }
    
    /// Check if needs paint
    pub fn needs_paint(&self) -> bool {
        self.needs_full_paint || self.nodes.values().any(|n| n.flags.needs_paint())
    }
    
    /// Check if needs composite only
    pub fn needs_composite_only(&self) -> bool {
        !self.needs_layout() && !self.needs_paint() &&
        self.nodes.values().any(|n| n.flags.needs_composite_only())
    }
    
    /// Get nodes needing layout
    pub fn nodes_needing_layout(&self) -> Vec<NodeId> {
        self.nodes.iter()
            .filter(|(_, inv)| inv.flags.needs_layout())
            .map(|(&id, _)| id)
            .collect()
    }
    
    /// Get nodes needing paint
    pub fn nodes_needing_paint(&self) -> Vec<NodeId> {
        self.nodes.iter()
            .filter(|(_, inv)| inv.flags.needs_paint())
            .map(|(&id, _)| id)
            .collect()
    }
    
    /// Get dirty regions for painting
    pub fn get_dirty_regions(&self) -> Vec<DirtyRegion> {
        let mut regions = self.dirty_regions.clone();
        
        for inv in self.nodes.values() {
            if let Some(region) = &inv.region {
                regions.push(region.clone());
            }
        }
        
        // Merge overlapping regions
        let mut merged: Vec<DirtyRegion> = Vec::new();
        'outer: for region in regions {
            for existing in &mut merged {
                if existing.overlaps(&region) {
                    *existing = existing.union(&region);
                    continue 'outer;
                }
            }
            merged.push(region);
        }
        
        merged
    }
    
    /// Get layer invalidations
    pub fn layer_invalidations(&self) -> &HashMap<u32, LayerInvalidation> {
        &self.layers
    }
    
    /// Clear after processing
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.layers.clear();
        self.dirty_regions.clear();
        self.needs_full_layout = false;
        self.needs_full_paint = false;
    }
    
    /// Get statistics
    pub fn stats(&self) -> &InvalidationStats {
        &self.stats
    }
    
    /// Check if has any pending invalidations
    pub fn has_pending(&self) -> bool {
        !self.nodes.is_empty() || !self.dirty_regions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_flags() {
        let mut flags = InvalidationFlags::NONE;
        assert!(!flags.any());
        
        flags.set(InvalidationFlags::LAYOUT);
        assert!(flags.has(InvalidationFlags::LAYOUT));
        assert!(flags.needs_layout());
        assert!(flags.needs_paint());
        
        flags.clear(InvalidationFlags::LAYOUT);
        assert!(!flags.has(InvalidationFlags::LAYOUT));
    }
    
    #[test]
    fn test_dirty_region_overlap() {
        let r1 = DirtyRegion::new(0.0, 0.0, 100.0, 100.0);
        let r2 = DirtyRegion::new(50.0, 50.0, 100.0, 100.0);
        let r3 = DirtyRegion::new(200.0, 200.0, 50.0, 50.0);
        
        assert!(r1.overlaps(&r2));
        assert!(!r1.overlaps(&r3));
    }
    
    #[test]
    fn test_dirty_region_union() {
        let r1 = DirtyRegion::new(0.0, 0.0, 100.0, 100.0);
        let r2 = DirtyRegion::new(50.0, 50.0, 100.0, 100.0);
        
        let union = r1.union(&r2);
        assert_eq!(union.x, 0.0);
        assert_eq!(union.y, 0.0);
        assert_eq!(union.width, 150.0);
        assert_eq!(union.height, 150.0);
    }
    
    #[test]
    fn test_tracker_basic() {
        let mut tracker = PartialInvalidationTracker::new();
        
        tracker.invalidate_layout(1);
        tracker.invalidate_paint(2, Some(DirtyRegion::new(0.0, 0.0, 50.0, 50.0)));
        tracker.invalidate_transform(3);
        
        assert!(tracker.needs_layout());
        assert!(tracker.needs_paint());
        
        let layout_nodes = tracker.nodes_needing_layout();
        assert!(layout_nodes.contains(&1));
    }
    
    #[test]
    fn test_property_invalidation() {
        let mut props = PropertyInvalidation::new();
        props.mark_changed(PropertyInvalidation::WIDTH);
        props.mark_changed(PropertyInvalidation::BACKGROUND);
        
        let flags = props.to_flags();
        assert!(flags.has(InvalidationFlags::LAYOUT));
        assert!(flags.has(InvalidationFlags::PAINT));
    }
    
    #[test]
    fn test_region_merging() {
        let mut tracker = PartialInvalidationTracker::new();
        
        tracker.add_dirty_region(DirtyRegion::new(0.0, 0.0, 100.0, 100.0));
        tracker.add_dirty_region(DirtyRegion::new(50.0, 50.0, 100.0, 100.0));
        
        let regions = tracker.get_dirty_regions();
        assert_eq!(regions.len(), 1); // Should be merged
    }
}
