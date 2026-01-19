//! CSS Property Delta Tracking (Phase 24.6) 
//!
//! Track which CSS properties changed. Only restyle changed.
//! Property-level dirty bits. Skip unchanged subtrees.
//!
//! ## CSS Roadmap Phase 5 Enhancements
//! - Delta-only style storage: only store property differences from parent
//! - Efficient property diff computation
//! - Minimal memory footprint for style changes

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// CSS property ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum CssProperty {
    // Box model
    Width = 0,
    Height = 1,
    MarginTop = 2,
    MarginRight = 3,
    MarginBottom = 4,
    MarginLeft = 5,
    PaddingTop = 6,
    PaddingRight = 7,
    PaddingBottom = 8,
    PaddingLeft = 9,
    BorderTopWidth = 10,
    BorderRightWidth = 11,
    BorderBottomWidth = 12,
    BorderLeftWidth = 13,
    
    // Positioning
    Position = 14,
    Top = 15,
    Right = 16,
    Bottom = 17,
    Left = 18,
    ZIndex = 19,
    
    // Display
    Display = 20,
    Visibility = 21,
    Opacity = 22,
    Overflow = 23,
    
    // Flexbox
    FlexDirection = 24,
    FlexWrap = 25,
    JustifyContent = 26,
    AlignItems = 27,
    FlexGrow = 28,
    FlexShrink = 29,
    
    // Text
    Color = 30,
    FontSize = 31,
    FontWeight = 32,
    FontFamily = 33,
    LineHeight = 34,
    TextAlign = 35,
    
    // Background
    BackgroundColor = 36,
    BackgroundImage = 37,
    
    // Transform
    Transform = 38,
    TransformOrigin = 39,
    
    // Other
    BoxShadow = 40,
    BorderRadius = 41,
    Cursor = 42,
    PointerEvents = 43,
}

impl CssProperty {
    pub const COUNT: usize = 44;
    
    /// Does this property affect layout?
    pub fn affects_layout(self) -> bool {
        matches!(self,
            CssProperty::Width |
            CssProperty::Height |
            CssProperty::MarginTop |
            CssProperty::MarginRight |
            CssProperty::MarginBottom |
            CssProperty::MarginLeft |
            CssProperty::PaddingTop |
            CssProperty::PaddingRight |
            CssProperty::PaddingBottom |
            CssProperty::PaddingLeft |
            CssProperty::BorderTopWidth |
            CssProperty::BorderRightWidth |
            CssProperty::BorderBottomWidth |
            CssProperty::BorderLeftWidth |
            CssProperty::Position |
            CssProperty::Top |
            CssProperty::Right |
            CssProperty::Bottom |
            CssProperty::Left |
            CssProperty::Display |
            CssProperty::FlexDirection |
            CssProperty::FlexWrap |
            CssProperty::JustifyContent |
            CssProperty::AlignItems |
            CssProperty::FlexGrow |
            CssProperty::FlexShrink |
            CssProperty::FontSize |
            CssProperty::LineHeight
        )
    }
    
    /// Does this property affect paint only?
    pub fn affects_paint_only(self) -> bool {
        matches!(self,
            CssProperty::Color |
            CssProperty::BackgroundColor |
            CssProperty::BackgroundImage |
            CssProperty::BoxShadow |
            CssProperty::Cursor |
            CssProperty::Opacity |
            CssProperty::Visibility
        )
    }
    
    /// Does this property affect compositing?
    pub fn affects_compositing(self) -> bool {
        matches!(self,
            CssProperty::Transform |
            CssProperty::TransformOrigin |
            CssProperty::Opacity |
            CssProperty::ZIndex
        )
    }
}

/// Property change flags (packed as bits)
#[derive(Clone, Copy, Default)]
pub struct PropertyDirtyBits {
    bits: [u64; 1], // 64 bits = 64 properties
}

impl PropertyDirtyBits {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Mark a property as dirty
    pub fn mark(&mut self, prop: CssProperty) {
        let idx = prop as usize;
        if idx < 64 {
            self.bits[0] |= 1 << idx;
        }
    }
    
    /// Check if a property is dirty
    pub fn is_dirty(&self, prop: CssProperty) -> bool {
        let idx = prop as usize;
        if idx < 64 {
            (self.bits[0] >> idx) & 1 == 1
        } else {
            false
        }
    }
    
    /// Check if any property is dirty
    pub fn any_dirty(&self) -> bool {
        self.bits[0] != 0
    }
    
    /// Check if any layout property is dirty
    pub fn any_layout_dirty(&self) -> bool {
        // Check layout-affecting properties
        for i in 0..CssProperty::COUNT {
            let prop: CssProperty = unsafe { std::mem::transmute(i as u8) };
            if prop.affects_layout() && self.is_dirty(prop) {
                return true;
            }
        }
        false
    }
    
    /// Check if only paint properties are dirty
    pub fn only_paint_dirty(&self) -> bool {
        for i in 0..CssProperty::COUNT {
            let prop: CssProperty = unsafe { std::mem::transmute(i as u8) };
            if self.is_dirty(prop) && !prop.affects_paint_only() {
                return false;
            }
        }
        true
    }
    
    /// Get list of dirty properties
    pub fn dirty_properties(&self) -> Vec<CssProperty> {
        let mut result = Vec::new();
        for i in 0..CssProperty::COUNT {
            let prop: CssProperty = unsafe { std::mem::transmute(i as u8) };
            if self.is_dirty(prop) {
                result.push(prop);
            }
        }
        result
    }
    
    /// Clear all bits
    pub fn clear(&mut self) {
        self.bits[0] = 0;
    }
    
    /// Count of dirty properties
    pub fn count(&self) -> usize {
        self.bits[0].count_ones() as usize
    }
    
    /// Union with another set
    pub fn union(&mut self, other: &PropertyDirtyBits) {
        self.bits[0] |= other.bits[0];
    }
}

impl std::fmt::Debug for PropertyDirtyBits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let props = self.dirty_properties();
        f.debug_struct("PropertyDirtyBits")
            .field("count", &self.count())
            .field("properties", &props)
            .finish()
    }
}

/// Node ID type
pub type NodeId = u32;

/// CSS delta tracker for the whole document
#[derive(Debug)]
pub struct CssDeltaTracker {
    /// Dirty bits per node
    dirty: HashMap<NodeId, PropertyDirtyBits>,
    /// Nodes needing layout
    needs_layout: HashSet<NodeId>,
    /// Nodes needing paint only
    needs_paint: HashSet<NodeId>,
    /// Nodes needing compositing update
    needs_composite: HashSet<NodeId>,
    /// Statistics
    stats: DeltaStats,
}

/// Statistics for CSS delta tracking
#[derive(Debug, Clone, Copy, Default)]
pub struct DeltaStats {
    pub properties_changed: u64,
    pub nodes_changed: u64,
    pub layout_avoided: u64,
    pub paint_only_updates: u64,
    pub composite_only_updates: u64,
}

impl Default for CssDeltaTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl CssDeltaTracker {
    pub fn new() -> Self {
        Self {
            dirty: HashMap::new(),
            needs_layout: HashSet::new(),
            needs_paint: HashSet::new(),
            needs_composite: HashSet::new(),
            stats: DeltaStats::default(),
        }
    }
    
    /// Mark a property as changed for a node
    pub fn mark_changed(&mut self, node_id: NodeId, prop: CssProperty) {
        let bits = self.dirty.entry(node_id).or_default();
        bits.mark(prop);
        
        self.stats.properties_changed += 1;
        
        // Categorize the change
        if prop.affects_layout() {
            self.needs_layout.insert(node_id);
        } else if prop.affects_compositing() {
            self.needs_composite.insert(node_id);
        } else if prop.affects_paint_only() {
            // Only add to paint if not already needing layout
            if !self.needs_layout.contains(&node_id) {
                self.needs_paint.insert(node_id);
            }
        }
    }
    
    /// Mark multiple properties as changed
    pub fn mark_changed_batch(&mut self, node_id: NodeId, props: &[CssProperty]) {
        for &prop in props {
            self.mark_changed(node_id, prop);
        }
    }
    
    /// Get dirty bits for a node
    pub fn get_dirty(&self, node_id: NodeId) -> Option<&PropertyDirtyBits> {
        self.dirty.get(&node_id)
    }
    
    /// Check if node needs any update
    pub fn needs_update(&self, node_id: NodeId) -> bool {
        self.dirty.contains_key(&node_id)
    }
    
    /// Check if node needs layout
    pub fn needs_layout(&self, node_id: NodeId) -> bool {
        self.needs_layout.contains(&node_id)
    }
    
    /// Check if node only needs paint
    pub fn needs_paint_only(&self, node_id: NodeId) -> bool {
        self.needs_paint.contains(&node_id) && !self.needs_layout.contains(&node_id)
    }
    
    /// Get all nodes needing layout
    pub fn layout_nodes(&self) -> &HashSet<NodeId> {
        &self.needs_layout
    }
    
    /// Get all nodes needing paint only
    pub fn paint_nodes(&self) -> &HashSet<NodeId> {
        &self.needs_paint
    }
    
    /// Get all nodes needing composite update
    pub fn composite_nodes(&self) -> &HashSet<NodeId> {
        &self.needs_composite
    }
    
    /// Clear dirty state for a node
    pub fn clear_node(&mut self, node_id: NodeId) {
        self.dirty.remove(&node_id);
        self.needs_layout.remove(&node_id);
        self.needs_paint.remove(&node_id);
        self.needs_composite.remove(&node_id);
    }
    
    /// Clear all dirty state
    pub fn clear(&mut self) {
        self.dirty.clear();
        self.needs_layout.clear();
        self.needs_paint.clear();
        self.needs_composite.clear();
    }
    
    /// Finalize frame and update stats
    pub fn finalize_frame(&mut self) {
        let total_changed = self.dirty.len() as u64;
        let layout_count = self.needs_layout.len() as u64;
        let paint_only = self.needs_paint.len() as u64;
        let composite_only = self.needs_composite.len() as u64;
        
        self.stats.nodes_changed += total_changed;
        self.stats.layout_avoided += total_changed.saturating_sub(layout_count);
        self.stats.paint_only_updates += paint_only;
        self.stats.composite_only_updates += composite_only;
        
        self.clear();
    }
    
    /// Get statistics
    pub fn stats(&self) -> &DeltaStats {
        &self.stats
    }
}

// ============================================================================
// Delta Style Storage (Phase 5: Surpassing Chromium)
// ============================================================================

/// A property value that can be stored in a delta
#[derive(Debug, Clone)]
pub enum DeltaValue {
    /// Numeric value (px, em, etc)
    Number(f32),
    /// Integer value
    Integer(i32),
    /// Color value (packed RGBA)
    Color(u32),
    /// String value (font-family, etc)
    String(Arc<str>),
    /// Enum value (display, position, etc)
    Enum(u8),
    /// None/auto/inherit
    Keyword(Keyword),
}

/// CSS keywords
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Auto,
    None,
    Inherit,
    Initial,
    Unset,
}

/// Delta-only style storage - only stores differences from parent
#[derive(Debug, Clone, Default)]
pub struct DeltaStyle {
    /// Properties that differ from parent (sparse storage)
    pub overrides: HashMap<CssProperty, DeltaValue>,
    /// Reference to parent style for inheritance
    pub parent: Option<Arc<DeltaStyle>>,
    /// Hash for comparison
    pub hash: u64,
}

impl DeltaStyle {
    /// Create a new empty delta style
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create with a parent
    pub fn with_parent(parent: Arc<DeltaStyle>) -> Self {
        Self {
            overrides: HashMap::new(),
            parent: Some(parent),
            hash: 0,
        }
    }
    
    /// Set a property override
    pub fn set(&mut self, prop: CssProperty, value: DeltaValue) {
        self.overrides.insert(prop, value);
        self.update_hash();
    }
    
    /// Get a property value (checks parent chain)
    pub fn get(&self, prop: CssProperty) -> Option<&DeltaValue> {
        if let Some(val) = self.overrides.get(&prop) {
            return Some(val);
        }
        
        // Check parent chain
        if let Some(ref parent) = self.parent {
            return parent.get(prop);
        }
        
        None
    }
    
    /// Check if property is overridden locally
    pub fn is_overridden(&self, prop: CssProperty) -> bool {
        self.overrides.contains_key(&prop)
    }
    
    /// Get number of local overrides
    pub fn override_count(&self) -> usize {
        self.overrides.len()
    }
    
    /// Compute size in bytes (sparse = smaller)
    pub fn size_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + 
        self.overrides.len() * (std::mem::size_of::<CssProperty>() + std::mem::size_of::<DeltaValue>())
    }
    
    /// Update hash after changes
    fn update_hash(&mut self) {
        use std::hash::Hasher;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        
        for (prop, val) in &self.overrides {
            hasher.write_u8(*prop as u8);
            match val {
                DeltaValue::Number(n) => hasher.write_u32(n.to_bits()),
                DeltaValue::Integer(i) => hasher.write_i32(*i),
                DeltaValue::Color(c) => hasher.write_u32(*c),
                DeltaValue::String(s) => std::hash::Hash::hash(s.as_ref(), &mut hasher),
                DeltaValue::Enum(e) => hasher.write_u8(*e),
                DeltaValue::Keyword(k) => hasher.write_u8(*k as u8),
            }
        }
        
        self.hash = hasher.finish();
    }
    
    /// Compare with another delta style
    pub fn equivalent(&self, other: &DeltaStyle) -> bool {
        self.hash == other.hash && self.overrides.len() == other.overrides.len()
    }
}

/// Compute property differences between two styles
pub fn compute_delta(base: &DeltaStyle, modified: &DeltaStyle) -> DeltaStyle {
    let mut delta = DeltaStyle::new();
    
    // Add all overrides from modified that differ from base
    for (prop, val) in &modified.overrides {
        let base_val = base.overrides.get(prop);
        if base_val.map_or(true, |bv| !values_equal(bv, val)) {
            delta.overrides.insert(*prop, val.clone());
        }
    }
    
    delta.update_hash();
    delta
}

fn values_equal(a: &DeltaValue, b: &DeltaValue) -> bool {
    match (a, b) {
        (DeltaValue::Number(x), DeltaValue::Number(y)) => (x - y).abs() < 0.001,
        (DeltaValue::Integer(x), DeltaValue::Integer(y)) => x == y,
        (DeltaValue::Color(x), DeltaValue::Color(y)) => x == y,
        (DeltaValue::String(x), DeltaValue::String(y)) => x == y,
        (DeltaValue::Enum(x), DeltaValue::Enum(y)) => x == y,
        (DeltaValue::Keyword(x), DeltaValue::Keyword(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dirty_bits() {
        let mut bits = PropertyDirtyBits::new();
        
        assert!(!bits.any_dirty());
        
        bits.mark(CssProperty::Width);
        bits.mark(CssProperty::Color);
        
        assert!(bits.is_dirty(CssProperty::Width));
        assert!(bits.is_dirty(CssProperty::Color));
        assert!(!bits.is_dirty(CssProperty::Height));
        
        assert!(bits.any_layout_dirty()); // Width affects layout
        assert_eq!(bits.count(), 2);
    }
    
    #[test]
    fn test_paint_only() {
        let mut bits = PropertyDirtyBits::new();
        
        // Only paint-affecting properties
        bits.mark(CssProperty::Color);
        bits.mark(CssProperty::BackgroundColor);
        
        assert!(bits.only_paint_dirty());
        assert!(!bits.any_layout_dirty());
        
        // Add a layout property
        bits.mark(CssProperty::Width);
        
        assert!(!bits.only_paint_dirty());
        assert!(bits.any_layout_dirty());
    }
    
    #[test]
    fn test_delta_tracker() {
        let mut tracker = CssDeltaTracker::new();
        
        // Change paint-only property
        tracker.mark_changed(1, CssProperty::Color);
        assert!(tracker.needs_paint_only(1));
        assert!(!tracker.needs_layout(1));
        
        // Change layout property on different node
        tracker.mark_changed(2, CssProperty::Width);
        assert!(tracker.needs_layout(2));
        
        // Stats
        assert_eq!(tracker.paint_nodes().len(), 1);
        assert_eq!(tracker.layout_nodes().len(), 1);
    }
    
    #[test]
    fn test_property_categorization() {
        assert!(CssProperty::Width.affects_layout());
        assert!(CssProperty::MarginTop.affects_layout());
        
        assert!(CssProperty::Color.affects_paint_only());
        assert!(CssProperty::BackgroundColor.affects_paint_only());
        
        assert!(CssProperty::Transform.affects_compositing());
        assert!(CssProperty::Opacity.affects_compositing());
    }
    
    #[test]
    fn test_delta_style() {
        let mut style = DeltaStyle::new();
        
        style.set(CssProperty::Width, DeltaValue::Number(100.0));
        style.set(CssProperty::Color, DeltaValue::Color(0xFF0000FF));
        
        assert_eq!(style.override_count(), 2);
        assert!(style.is_overridden(CssProperty::Width));
        assert!(!style.is_overridden(CssProperty::Height));
    }
    
    #[test]
    fn test_delta_style_inheritance() {
        let mut parent = DeltaStyle::new();
        parent.set(CssProperty::Color, DeltaValue::Color(0xFF0000FF));
        
        let parent = Arc::new(parent);
        let mut child = DeltaStyle::with_parent(parent);
        child.set(CssProperty::Width, DeltaValue::Number(50.0));
        
        // Child should get color from parent
        assert!(child.get(CssProperty::Color).is_some());
        assert!(child.get(CssProperty::Width).is_some());
        
        // Only width is locally overridden
        assert!(!child.is_overridden(CssProperty::Color));
        assert!(child.is_overridden(CssProperty::Width));
    }
}

