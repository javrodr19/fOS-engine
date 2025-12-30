//! Render Tree Pruning (Phase 24.2)
//!
//! Remove from render tree if invisible. Reconstruct when visible.
//! Separate visible/hidden trees. Smaller working set.

use std::collections::{HashMap, HashSet};

/// Node ID type
pub type NodeId = u32;

/// Visibility state of a node
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityState {
    /// Node is visible and in render tree
    Visible,
    /// Node is hidden (visibility: hidden) - takes space but not painted
    Hidden,
    /// Node is not displayed (display: none) - no space, no paint
    None,
    /// Node is off-screen - not currently visible
    Offscreen,
    /// Node is pruned from render tree
    Pruned,
}

/// Pruning reason
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PruneReason {
    /// display: none
    DisplayNone,
    /// visibility: hidden and no visible children
    VisibilityHidden,
    /// Off-screen for extended period
    OffscreenTimeout,
    /// Occluded by other elements
    Occluded,
    /// Collapsed (e.g., empty element)
    Collapsed,
}

/// Pruned node data - minimal info needed for reconstruction
#[derive(Debug, Clone)]
pub struct PrunedNode {
    /// Node ID
    pub id: NodeId,
    /// Reason for pruning
    pub reason: PruneReason,
    /// Bounding box (for visibility checks)
    pub bounds: (f32, f32, f32, f32), // x, y, w, h
    /// Serialized node data (for reconstruction)
    pub serialized: Vec<u8>,
    /// Child IDs (also pruned)
    pub children: Vec<NodeId>,
}

/// Statistics for render tree pruning
#[derive(Debug, Clone, Copy, Default)]
pub struct PruningStats {
    pub visible_count: usize,
    pub pruned_count: usize,
    pub reconstructed_count: u64,
    pub memory_saved_bytes: usize,
}

impl PruningStats {
    pub fn pruned_ratio(&self) -> f64 {
        let total = self.visible_count + self.pruned_count;
        if total == 0 { 0.0 } else { self.pruned_count as f64 / total as f64 }
    }
}

/// Render tree pruner
#[derive(Debug)]
pub struct RenderTreePruner {
    /// Nodes currently in visible tree
    visible: HashSet<NodeId>,
    /// Pruned nodes
    pruned: HashMap<NodeId, PrunedNode>,
    /// Visibility states
    states: HashMap<NodeId, VisibilityState>,
    /// Pruning configuration
    config: PruningConfig,
    /// Statistics
    stats: PruningStats,
}

/// Configuration for pruning
#[derive(Debug, Clone)]
pub struct PruningConfig {
    /// Minimum time off-screen before pruning (ms)
    pub offscreen_timeout_ms: u64,
    /// Enable occlusion culling
    pub occlusion_culling: bool,
    /// Maximum pruned nodes to keep
    pub max_pruned: usize,
}

impl Default for PruningConfig {
    fn default() -> Self {
        Self {
            offscreen_timeout_ms: 5000,
            occlusion_culling: true,
            max_pruned: 10000,
        }
    }
}

impl Default for RenderTreePruner {
    fn default() -> Self {
        Self::new(PruningConfig::default())
    }
}

impl RenderTreePruner {
    pub fn new(config: PruningConfig) -> Self {
        Self {
            visible: HashSet::new(),
            pruned: HashMap::new(),
            states: HashMap::new(),
            config,
            stats: PruningStats::default(),
        }
    }
    
    /// Add a node to visible tree
    pub fn add_visible(&mut self, id: NodeId) {
        self.visible.insert(id);
        self.states.insert(id, VisibilityState::Visible);
    }
    
    /// Set visibility state
    pub fn set_state(&mut self, id: NodeId, state: VisibilityState) {
        self.states.insert(id, state);
    }
    
    /// Check if node should be pruned
    pub fn should_prune(&self, id: NodeId) -> Option<PruneReason> {
        match self.states.get(&id) {
            Some(VisibilityState::None) => Some(PruneReason::DisplayNone),
            Some(VisibilityState::Hidden) => Some(PruneReason::VisibilityHidden),
            Some(VisibilityState::Offscreen) => Some(PruneReason::OffscreenTimeout),
            _ => None,
        }
    }
    
    /// Prune a node from render tree
    pub fn prune(&mut self, node: PrunedNode) {
        let id = node.id;
        let size = node.serialized.len();
        
        // Remove from visible
        self.visible.remove(&id);
        for child_id in &node.children {
            self.visible.remove(child_id);
        }
        
        // Add to pruned
        self.pruned.insert(id, node);
        self.states.insert(id, VisibilityState::Pruned);
        
        // Update stats
        self.stats.pruned_count = self.pruned.len();
        self.stats.visible_count = self.visible.len();
        self.stats.memory_saved_bytes += size;
        
        // Evict if too many pruned
        self.evict_if_needed();
    }
    
    /// Evict oldest pruned nodes if over limit
    fn evict_if_needed(&mut self) {
        while self.pruned.len() > self.config.max_pruned {
            // Remove first (oldest) entry
            if let Some(&id) = self.pruned.keys().next() {
                if let Some(node) = self.pruned.remove(&id) {
                    self.stats.memory_saved_bytes = 
                        self.stats.memory_saved_bytes.saturating_sub(node.serialized.len());
                }
            } else {
                break;
            }
        }
    }
    
    /// Check if a node is pruned
    pub fn is_pruned(&self, id: NodeId) -> bool {
        self.pruned.contains_key(&id)
    }
    
    /// Check if a node is visible
    pub fn is_visible(&self, id: NodeId) -> bool {
        self.visible.contains(&id)
    }
    
    /// Get pruned node data
    pub fn get_pruned(&self, id: NodeId) -> Option<&PrunedNode> {
        self.pruned.get(&id)
    }
    
    /// Reconstruct a pruned node (returns serialized data)
    pub fn reconstruct(&mut self, id: NodeId) -> Option<Vec<u8>> {
        if let Some(node) = self.pruned.remove(&id) {
            // Remove children from pruned too
            for child_id in &node.children {
                self.pruned.remove(child_id);
            }
            
            // Add back to visible
            self.visible.insert(id);
            self.states.insert(id, VisibilityState::Visible);
            
            // Update stats
            self.stats.pruned_count = self.pruned.len();
            self.stats.visible_count = self.visible.len();
            self.stats.reconstructed_count += 1;
            self.stats.memory_saved_bytes = 
                self.stats.memory_saved_bytes.saturating_sub(node.serialized.len());
            
            Some(node.serialized)
        } else {
            None
        }
    }
    
    /// Get nodes that should be reconstructed (visible in viewport)
    pub fn get_reconstruction_candidates(&self, viewport: (f32, f32, f32, f32)) -> Vec<NodeId> {
        let (vx, vy, vw, vh) = viewport;
        
        self.pruned.iter()
            .filter(|(_, node)| {
                let (x, y, w, h) = node.bounds;
                // Check if bounds intersect viewport
                x < vx + vw && x + w > vx && y < vy + vh && y + h > vy
            })
            .map(|(&id, _)| id)
            .collect()
    }
    
    /// Get statistics
    pub fn stats(&self) -> &PruningStats {
        &self.stats
    }
    
    /// Get visibility state
    pub fn state(&self, id: NodeId) -> Option<VisibilityState> {
        self.states.get(&id).copied()
    }
}

/// Occlusion tracker for culling
#[derive(Debug, Default)]
pub struct OcclusionTracker {
    /// Stack of occluding regions
    occluders: Vec<(f32, f32, f32, f32)>,
}

impl OcclusionTracker {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Push an occluding rect
    pub fn push(&mut self, rect: (f32, f32, f32, f32)) {
        self.occluders.push(rect);
    }
    
    /// Pop an occluder
    pub fn pop(&mut self) {
        self.occluders.pop();
    }
    
    /// Check if a rect is fully occluded
    pub fn is_occluded(&self, rect: (f32, f32, f32, f32)) -> bool {
        let (x, y, w, h) = rect;
        
        for &(ox, oy, ow, oh) in &self.occluders {
            // Check if occluder fully contains rect
            if ox <= x && oy <= y && ox + ow >= x + w && oy + oh >= y + h {
                return true;
            }
        }
        
        false
    }
    
    /// Clear all occluders
    pub fn clear(&mut self) {
        self.occluders.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pruner_basic() {
        let mut pruner = RenderTreePruner::default();
        
        pruner.add_visible(1);
        pruner.add_visible(2);
        pruner.add_visible(3);
        
        assert!(pruner.is_visible(1));
        assert_eq!(pruner.stats().visible_count, 3);
    }
    
    #[test]
    fn test_prune_node() {
        let mut pruner = RenderTreePruner::default();
        
        pruner.add_visible(1);
        pruner.set_state(1, VisibilityState::None);
        
        let pruned = PrunedNode {
            id: 1,
            reason: PruneReason::DisplayNone,
            bounds: (0.0, 0.0, 100.0, 100.0),
            serialized: vec![1, 2, 3, 4],
            children: vec![],
        };
        
        pruner.prune(pruned);
        
        assert!(!pruner.is_visible(1));
        assert!(pruner.is_pruned(1));
        assert_eq!(pruner.stats().pruned_count, 1);
    }
    
    #[test]
    fn test_reconstruct() {
        let mut pruner = RenderTreePruner::default();
        
        let pruned = PrunedNode {
            id: 1,
            reason: PruneReason::OffscreenTimeout,
            bounds: (0.0, 0.0, 100.0, 100.0),
            serialized: vec![5, 6, 7, 8],
            children: vec![],
        };
        
        pruner.prune(pruned);
        
        let data = pruner.reconstruct(1);
        assert_eq!(data, Some(vec![5, 6, 7, 8]));
        assert!(pruner.is_visible(1));
        assert!(!pruner.is_pruned(1));
    }
    
    #[test]
    fn test_occlusion() {
        let mut tracker = OcclusionTracker::new();
        
        // Add a large occluder
        tracker.push((0.0, 0.0, 1000.0, 1000.0));
        
        // Small rect should be occluded
        assert!(tracker.is_occluded((100.0, 100.0, 50.0, 50.0)));
        
        // Rect outside should not be occluded
        assert!(!tracker.is_occluded((2000.0, 2000.0, 50.0, 50.0)));
    }
}
