//! Layer Tree
//!
//! Layer tree management for compositing.

use std::collections::HashMap;

/// Layer tree
#[derive(Debug, Default)]
pub struct LayerTree {
    layers: HashMap<u64, Layer>,
    root: Option<u64>,
    next_id: u64,
}

/// Layer
#[derive(Debug, Clone)]
pub struct Layer {
    pub id: u64,
    pub name: String,
    pub visible: bool,
    pub isolated: bool,
    pub compositing_reasons: Vec<CompositingReason>,
    pub paint_order: u32,
    pub children: Vec<u64>,
    pub parent: Option<u64>,
}

/// Reason for layer creation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositingReason {
    Root,
    Transform3D,
    Video,
    Canvas,
    WillChange,
    FixedPosition,
    Overflow,
    Animation,
    Filter,
    Opacity,
    Isolation,
}

impl LayerTree {
    pub fn new() -> Self { Self::default() }
    
    /// Create root layer
    pub fn create_root(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let layer = Layer {
            id,
            name: "root".to_string(),
            visible: true,
            isolated: false,
            compositing_reasons: vec![CompositingReason::Root],
            paint_order: 0,
            children: Vec::new(),
            parent: None,
        };
        
        self.layers.insert(id, layer);
        self.root = Some(id);
        id
    }
    
    /// Create child layer
    pub fn create_layer(&mut self, parent_id: u64, reason: CompositingReason) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let paint_order = self.layers.len() as u32;
        
        let layer = Layer {
            id,
            name: format!("layer_{}", id),
            visible: true,
            isolated: false,
            compositing_reasons: vec![reason],
            paint_order,
            children: Vec::new(),
            parent: Some(parent_id),
        };
        
        self.layers.insert(id, layer);
        
        if let Some(parent) = self.layers.get_mut(&parent_id) {
            parent.children.push(id);
        }
        
        id
    }
    
    /// Get layer
    pub fn get(&self, id: u64) -> Option<&Layer> {
        self.layers.get(&id)
    }
    
    /// Get mutable layer
    pub fn get_mut(&mut self, id: u64) -> Option<&mut Layer> {
        self.layers.get_mut(&id)
    }
    
    /// Get paint order
    pub fn get_paint_order(&self) -> Vec<u64> {
        let mut ordered: Vec<_> = self.layers.values()
            .filter(|l| l.visible)
            .collect();
        ordered.sort_by_key(|l| l.paint_order);
        ordered.iter().map(|l| l.id).collect()
    }
    
    /// Check if element needs own layer
    pub fn needs_compositing_layer(reasons: &[CompositingReason]) -> bool {
        reasons.iter().any(|r| matches!(r,
            CompositingReason::Transform3D |
            CompositingReason::Video |
            CompositingReason::Canvas |
            CompositingReason::WillChange |
            CompositingReason::Filter |
            CompositingReason::Animation
        ))
    }
    
    /// Total layers
    pub fn len(&self) -> usize {
        self.layers.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
}

/// Occlusion culling
#[derive(Debug, Default)]
pub struct OcclusionCuller {
    opaque_rects: Vec<OcclusionRect>,
}

#[derive(Debug, Clone)]
pub struct OcclusionRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl OcclusionCuller {
    pub fn new() -> Self { Self::default() }
    
    /// Add opaque rect
    pub fn add_opaque(&mut self, rect: OcclusionRect) {
        self.opaque_rects.push(rect);
    }
    
    /// Check if rect is occluded
    pub fn is_occluded(&self, x: f64, y: f64, width: f64, height: f64) -> bool {
        for opaque in &self.opaque_rects {
            if x >= opaque.x && 
               y >= opaque.y &&
               x + width <= opaque.x + opaque.width &&
               y + height <= opaque.y + opaque.height {
                return true;
            }
        }
        false
    }
    
    /// Clear
    pub fn clear(&mut self) {
        self.opaque_rects.clear();
    }
}

/// Damage tracker
#[derive(Debug, Default)]
pub struct DamageTracker {
    dirty_regions: Vec<DirtyRegion>,
}

#[derive(Debug, Clone)]
pub struct DirtyRegion {
    pub layer_id: u64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl DamageTracker {
    pub fn new() -> Self { Self::default() }
    
    /// Mark region dirty
    pub fn mark_dirty(&mut self, layer_id: u64, x: f64, y: f64, width: f64, height: f64) {
        self.dirty_regions.push(DirtyRegion {
            layer_id, x, y, width, height,
        });
    }
    
    /// Get dirty regions for layer
    pub fn get_dirty(&self, layer_id: u64) -> Vec<&DirtyRegion> {
        self.dirty_regions.iter()
            .filter(|r| r.layer_id == layer_id)
            .collect()
    }
    
    /// Clear
    pub fn clear(&mut self) {
        self.dirty_regions.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_layer_tree() {
        let mut tree = LayerTree::new();
        let root = tree.create_root();
        let child = tree.create_layer(root, CompositingReason::Canvas);
        
        assert_eq!(tree.len(), 2);
        assert!(tree.get(root).unwrap().children.contains(&child));
    }
    
    #[test]
    fn test_occlusion() {
        let mut culler = OcclusionCuller::new();
        culler.add_opaque(OcclusionRect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 });
        
        assert!(culler.is_occluded(10.0, 10.0, 20.0, 20.0));
        assert!(!culler.is_occluded(150.0, 150.0, 20.0, 20.0));
    }
}
