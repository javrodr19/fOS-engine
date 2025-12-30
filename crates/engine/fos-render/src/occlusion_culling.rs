//! Occluded Element Culling (Phase 24.5)
//!
//! Track which elements are fully covered. Skip rendering occluded elements.
//! Depth-based visibility. 30% render skip on complex pages.

use std::collections::{HashMap, HashSet};

/// Node ID type
pub type NodeId = u32;

/// Z-index value
pub type ZIndex = i32;

/// Occlusion rect
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OcclusionRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub z_index: ZIndex,
    pub node_id: NodeId,
    /// Is opaque (fully covers)
    pub is_opaque: bool,
}

impl OcclusionRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32, z_index: ZIndex, node_id: NodeId) -> Self {
        Self {
            x, y, width, height, z_index, node_id,
            is_opaque: true,
        }
    }
    
    pub fn with_opacity(mut self, is_opaque: bool) -> Self {
        self.is_opaque = is_opaque;
        self
    }
    
    /// Check if fully contains another rect
    pub fn fully_contains(&self, other: &OcclusionRect) -> bool {
        self.x <= other.x &&
        self.y <= other.y &&
        self.x + self.width >= other.x + other.width &&
        self.y + self.height >= other.y + other.height
    }
    
    /// Check if overlaps (any intersection)
    pub fn overlaps(&self, other: &OcclusionRect) -> bool {
        self.x < other.x + other.width &&
        self.x + self.width > other.x &&
        self.y < other.y + other.height &&
        self.y + self.height > other.y
    }
    
    /// Area
    pub fn area(&self) -> f32 {
        self.width * self.height
    }
}

/// Occlusion state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcclusionState {
    /// Fully visible
    Visible,
    /// Partially occluded
    PartiallyOccluded,
    /// Fully occluded (can skip render)
    FullyOccluded,
}

/// Occlusion tracker
#[derive(Debug)]
pub struct OcclusionTracker {
    /// All rects sorted by z-index (front to back)
    rects: Vec<OcclusionRect>,
    /// Occlusion state per node
    states: HashMap<NodeId, OcclusionState>,
    /// Nodes that can be skipped
    culled: HashSet<NodeId>,
    /// Statistics
    stats: OcclusionStats,
}

/// Occlusion statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct OcclusionStats {
    pub elements_analyzed: u64,
    pub elements_culled: u64,
    pub partial_occlusions: u64,
    pub area_culled: f64,
    pub total_area: f64,
}

impl OcclusionStats {
    pub fn cull_ratio(&self) -> f64 {
        if self.elements_analyzed == 0 {
            0.0
        } else {
            self.elements_culled as f64 / self.elements_analyzed as f64
        }
    }
    
    pub fn area_savings(&self) -> f64 {
        if self.total_area < 0.001 {
            0.0
        } else {
            self.area_culled / self.total_area
        }
    }
}

impl Default for OcclusionTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl OcclusionTracker {
    pub fn new() -> Self {
        Self {
            rects: Vec::new(),
            states: HashMap::new(),
            culled: HashSet::new(),
            stats: OcclusionStats::default(),
        }
    }
    
    /// Add an element rect
    pub fn add(&mut self, rect: OcclusionRect) {
        self.stats.elements_analyzed += 1;
        self.stats.total_area += rect.area() as f64;
        self.rects.push(rect);
    }
    
    /// Analyze occlusion
    pub fn analyze(&mut self) {
        // Sort by z-index (highest first = front to back)
        self.rects.sort_by(|a, b| b.z_index.cmp(&a.z_index));
        
        // For each rect, check if occluded by elements in front
        for i in 0..self.rects.len() {
            let rect = self.rects[i];
            let mut state = OcclusionState::Visible;
            
            // Check all elements in front (lower indices = higher z)
            for j in 0..i {
                let front = &self.rects[j];
                
                // Only opaque elements can occlude
                if !front.is_opaque {
                    continue;
                }
                
                if front.fully_contains(&rect) {
                    state = OcclusionState::FullyOccluded;
                    break;
                } else if front.overlaps(&rect) {
                    state = OcclusionState::PartiallyOccluded;
                }
            }
            
            self.states.insert(rect.node_id, state);
            
            if state == OcclusionState::FullyOccluded {
                self.culled.insert(rect.node_id);
                self.stats.elements_culled += 1;
                self.stats.area_culled += rect.area() as f64;
            } else if state == OcclusionState::PartiallyOccluded {
                self.stats.partial_occlusions += 1;
            }
        }
    }
    
    /// Check if node is culled
    pub fn is_culled(&self, node_id: NodeId) -> bool {
        self.culled.contains(&node_id)
    }
    
    /// Get occlusion state
    pub fn get_state(&self, node_id: NodeId) -> OcclusionState {
        self.states.get(&node_id).copied().unwrap_or(OcclusionState::Visible)
    }
    
    /// Get all culled nodes
    pub fn culled_nodes(&self) -> &HashSet<NodeId> {
        &self.culled
    }
    
    /// Get nodes to render (not culled)
    pub fn visible_nodes(&self) -> Vec<NodeId> {
        self.rects.iter()
            .filter(|r| !self.culled.contains(&r.node_id))
            .map(|r| r.node_id)
            .collect()
    }
    
    /// Get statistics
    pub fn stats(&self) -> &OcclusionStats {
        &self.stats
    }
    
    /// Clear for next frame
    pub fn clear(&mut self) {
        self.rects.clear();
        self.states.clear();
        self.culled.clear();
    }
}

/// Hierarchical occlusion with screen tiles
#[derive(Debug)]
pub struct TiledOcclusionTracker {
    /// Tile size
    tile_size: u32,
    /// Grid dimensions
    grid_width: u32,
    grid_height: u32,
    /// Opaque rects per tile
    tiles: Vec<Vec<OcclusionRect>>,
    /// Statistics
    stats: OcclusionStats,
}

impl TiledOcclusionTracker {
    pub fn new(viewport_width: u32, viewport_height: u32, tile_size: u32) -> Self {
        let grid_width = (viewport_width + tile_size - 1) / tile_size;
        let grid_height = (viewport_height + tile_size - 1) / tile_size;
        let total_tiles = (grid_width * grid_height) as usize;
        
        Self {
            tile_size,
            grid_width,
            grid_height,
            tiles: vec![Vec::new(); total_tiles],
            stats: OcclusionStats::default(),
        }
    }
    
    /// Get tile index
    fn tile_index(&self, x: u32, y: u32) -> usize {
        (y * self.grid_width + x) as usize
    }
    
    /// Add opaque rect (only add to covering tiles)
    pub fn add_opaque(&mut self, rect: OcclusionRect) {
        let tx0 = (rect.x.max(0.0) as u32) / self.tile_size;
        let ty0 = (rect.y.max(0.0) as u32) / self.tile_size;
        let tx1 = ((rect.x + rect.width) as u32) / self.tile_size + 1;
        let ty1 = ((rect.y + rect.height) as u32) / self.tile_size + 1;
        
        for ty in ty0..ty1.min(self.grid_height) {
            for tx in tx0..tx1.min(self.grid_width) {
                let idx = self.tile_index(tx, ty);
                if idx < self.tiles.len() {
                    self.tiles[idx].push(rect);
                }
            }
        }
    }
    
    /// Check if rect is occluded
    pub fn is_occluded(&self, rect: &OcclusionRect) -> bool {
        let tx = (rect.x as u32 + rect.width as u32 / 2) / self.tile_size;
        let ty = (rect.y as u32 + rect.height as u32 / 2) / self.tile_size;
        
        let idx = self.tile_index(tx.min(self.grid_width - 1), ty.min(self.grid_height - 1));
        
        if idx < self.tiles.len() {
            for front in &self.tiles[idx] {
                if front.z_index > rect.z_index && front.is_opaque && front.fully_contains(rect) {
                    return true;
                }
            }
        }
        
        false
    }
    
    /// Clear
    pub fn clear(&mut self) {
        for tile in &mut self.tiles {
            tile.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rect_containment() {
        let outer = OcclusionRect::new(0.0, 0.0, 100.0, 100.0, 10, 1);
        let inner = OcclusionRect::new(10.0, 10.0, 50.0, 50.0, 5, 2);
        let partial = OcclusionRect::new(50.0, 50.0, 100.0, 100.0, 5, 3);
        
        assert!(outer.fully_contains(&inner));
        assert!(!outer.fully_contains(&partial));
        assert!(outer.overlaps(&partial));
    }
    
    #[test]
    fn test_occlusion_analysis() {
        let mut tracker = OcclusionTracker::new();
        
        // Back element
        tracker.add(OcclusionRect::new(0.0, 0.0, 100.0, 100.0, 0, 1));
        
        // Front opaque element that fully covers
        tracker.add(OcclusionRect::new(0.0, 0.0, 150.0, 150.0, 10, 2));
        
        tracker.analyze();
        
        // Back element should be culled
        assert!(tracker.is_culled(1));
        assert!(!tracker.is_culled(2));
    }
    
    #[test]
    fn test_partial_occlusion() {
        let mut tracker = OcclusionTracker::new();
        
        tracker.add(OcclusionRect::new(0.0, 0.0, 100.0, 100.0, 0, 1));
        tracker.add(OcclusionRect::new(50.0, 50.0, 100.0, 100.0, 10, 2));
        
        tracker.analyze();
        
        // Should be partially occluded, not culled
        assert!(!tracker.is_culled(1));
        assert_eq!(tracker.get_state(1), OcclusionState::PartiallyOccluded);
    }
    
    #[test]
    fn test_transparent_no_occlude() {
        let mut tracker = OcclusionTracker::new();
        
        tracker.add(OcclusionRect::new(0.0, 0.0, 100.0, 100.0, 0, 1));
        tracker.add(OcclusionRect::new(0.0, 0.0, 150.0, 150.0, 10, 2).with_opacity(false));
        
        tracker.analyze();
        
        // Transparent element doesn't occlude
        assert!(!tracker.is_culled(1));
    }
}
