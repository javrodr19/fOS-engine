//! Layer Tree
//!
//! Layer tree management for compositing.
//!
//! # fos-engine Compatibility
//!
//! The CoW types in this module follow the same patterns as `fos_engine::cow`:
//! - `CowLayer` uses `Arc<Layer>` with `Arc::make_mut` for copy-on-write
//! - `CowLayerTree` enables efficient snapshots for undo/redo
//! - `LayerStyles` implements inherited style optimization
//!
//! Due to Cargo dependency constraints (fos-engine depends on fos-render),
//! this module implements the patterns directly rather than importing them.

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
    
    #[test]
    fn test_cow_layer() {
        let layer = Layer {
            id: 1,
            name: "test".to_string(),
            visible: true,
            isolated: false,
            compositing_reasons: vec![CompositingReason::Canvas],
            paint_order: 0,
            children: Vec::new(),
            parent: None,
        };
        
        let cow1 = CowLayer::new(layer);
        let cow2 = cow1.clone();
        
        // Both share the same data
        assert_eq!(cow1.ref_count(), 2);
        assert_eq!(cow1.get().id, cow2.get().id);
    }
    
    #[test]
    fn test_cow_layer_mutation() {
        let layer = Layer {
            id: 1,
            name: "test".to_string(),
            visible: true,
            isolated: false,
            compositing_reasons: vec![CompositingReason::Canvas],
            paint_order: 0,
            children: Vec::new(),
            parent: None,
        };
        
        let cow1 = CowLayer::new(layer);
        let mut cow2 = cow1.clone();
        
        // Mutate cow2
        cow2.get_mut().name = "modified".to_string();
        
        // Original unchanged
        assert_eq!(cow1.get().name, "test");
        assert_eq!(cow2.get().name, "modified");
    }
    
    #[test]
    fn test_layer_styles() {
        let parent = LayerStyles::new(LayerStyleProperties {
            opacity: 1.0,
            transform: None,
            filter: None,
            blend_mode: BlendModeStyle::Normal,
            isolation: false,
        });
        
        let child = parent.inherit();
        assert_eq!(child.computed().opacity, 1.0);
        
        let mut child2 = parent.inherit();
        child2.set_override(LayerStyleProperties {
            opacity: 0.5,
            transform: None,
            filter: None,
            blend_mode: BlendModeStyle::Normal,
            isolation: false,
        });
        assert_eq!(child2.computed().opacity, 0.5);
    }
}

// ============================================================================
// Copy-on-Write Layer Types
// ============================================================================

use std::sync::Arc;
use std::ops::Deref;

/// Copy-on-Write layer wrapper
/// 
/// Efficiently shares layer data between clones until mutation is needed.
/// On mutation, performs a copy only if multiple references exist.
#[derive(Debug)]
pub struct CowLayer {
    inner: Arc<Layer>,
}

impl CowLayer {
    /// Create a new CoW layer
    pub fn new(layer: Layer) -> Self {
        Self { inner: Arc::new(layer) }
    }
    
    /// Get shared reference to layer
    pub fn get(&self) -> &Layer {
        &self.inner
    }
    
    /// Get mutable reference (clones if shared)
    pub fn get_mut(&mut self) -> &mut Layer {
        Arc::make_mut(&mut self.inner)
    }
    
    /// Check if we own the only reference
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.inner) == 1
    }
    
    /// Number of references
    pub fn ref_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }
    
    /// Layer ID
    pub fn id(&self) -> u64 {
        self.inner.id
    }
}

impl Clone for CowLayer {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}

impl Deref for CowLayer {
    type Target = Layer;
    
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// Copy-on-Write layer tree
/// 
/// Efficiently shares layer tree structure. Useful for undo/redo,
/// caching previous states, and parallel processing.
#[derive(Debug, Default)]
pub struct CowLayerTree {
    layers: Arc<HashMap<u64, CowLayer>>,
    root: Option<u64>,
    next_id: u64,
}

impl CowLayerTree {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create from existing layer tree
    pub fn from_tree(tree: &LayerTree) -> Self {
        let mut layers = HashMap::new();
        for (&id, layer) in tree.layers.iter() {
            layers.insert(id, CowLayer::new(layer.clone()));
        }
        Self {
            layers: Arc::new(layers),
            root: tree.root,
            next_id: tree.next_id,
        }
    }
    
    /// Get layer by ID
    pub fn get(&self, id: u64) -> Option<&CowLayer> {
        self.layers.get(&id)
    }
    
    /// Get mutable access to layers (triggers copy if shared)
    pub fn get_layers_mut(&mut self) -> &mut HashMap<u64, CowLayer> {
        Arc::make_mut(&mut self.layers)
    }
    
    /// Number of layers
    pub fn len(&self) -> usize {
        self.layers.len()
    }
    
    /// Is empty?
    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }
    
    /// Root layer ID
    pub fn root(&self) -> Option<u64> {
        self.root
    }
    
    /// Check if tree structure is unique (not shared)
    pub fn is_unique(&self) -> bool {
        Arc::strong_count(&self.layers) == 1
    }
    
    /// Snapshot the current tree state
    pub fn snapshot(&self) -> Self {
        Self {
            layers: Arc::clone(&self.layers),
            root: self.root,
            next_id: self.next_id,
        }
    }
}

impl Clone for CowLayerTree {
    fn clone(&self) -> Self {
        Self {
            layers: Arc::clone(&self.layers),
            root: self.root,
            next_id: self.next_id,
        }
    }
}

// ============================================================================
// Layer Styles with COW Inheritance
// ============================================================================

/// Blend mode for layer compositing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BlendModeStyle {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
}

/// Layer style properties
#[derive(Debug, Clone, Default)]
pub struct LayerStyleProperties {
    pub opacity: f32,
    pub transform: Option<[f64; 16]>,
    pub filter: Option<String>,
    pub blend_mode: BlendModeStyle,
    pub isolation: bool,
}

/// Copy-on-Write style inheritance
/// 
/// Efficiently shares inherited styles between parent and children,
/// only creating new allocations when styles are overridden.
#[derive(Debug, Clone)]
pub struct LayerStyles {
    inherited: Arc<LayerStyleProperties>,
    overrides: Option<LayerStyleProperties>,
}

impl LayerStyles {
    /// Create new styles
    pub fn new(properties: LayerStyleProperties) -> Self {
        Self {
            inherited: Arc::new(properties),
            overrides: None,
        }
    }
    
    /// Get inherited styles
    pub fn inherited(&self) -> &LayerStyleProperties {
        &self.inherited
    }
    
    /// Get overrides (if any)
    pub fn overrides(&self) -> Option<&LayerStyleProperties> {
        self.overrides.as_ref()
    }
    
    /// Set style overrides
    pub fn set_override(&mut self, overrides: LayerStyleProperties) {
        self.overrides = Some(overrides);
    }
    
    /// Clear overrides
    pub fn clear_overrides(&mut self) {
        self.overrides = None;
    }
    
    /// Create child styles that inherit from this
    pub fn inherit(&self) -> Self {
        Self {
            inherited: Arc::clone(&self.inherited),
            overrides: None,
        }
    }
    
    /// Get computed styles (overrides take precedence)
    pub fn computed(&self) -> LayerStyleProperties {
        if let Some(ref overrides) = self.overrides {
            overrides.clone()
        } else {
            (*self.inherited).clone()
        }
    }
    
    /// Check if using shared inherited styles
    pub fn is_shared(&self) -> bool {
        Arc::strong_count(&self.inherited) > 1
    }
}

impl Default for LayerStyles {
    fn default() -> Self {
        Self::new(LayerStyleProperties {
            opacity: 1.0,
            transform: None,
            filter: None,
            blend_mode: BlendModeStyle::Normal,
            isolation: false,
        })
    }
}
