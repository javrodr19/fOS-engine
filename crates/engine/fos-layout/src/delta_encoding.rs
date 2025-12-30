//! Delta Encoding for Layout (Phase 24.2)
//!
//! Store layout as deltas from previous frame. If only one margin changed,
//! store 4 bytes. Compression for incremental updates.

use std::collections::HashMap;

/// Layout property that changed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum LayoutProperty {
    X = 0,
    Y = 1,
    Width = 2,
    Height = 3,
    MarginTop = 4,
    MarginRight = 5,
    MarginBottom = 6,
    MarginLeft = 7,
    PaddingTop = 8,
    PaddingRight = 9,
    PaddingBottom = 10,
    PaddingLeft = 11,
    BorderTop = 12,
    BorderRight = 13,
    BorderBottom = 14,
    BorderLeft = 15,
}

impl LayoutProperty {
    pub const COUNT: usize = 16;
}

/// Compact layout state (all properties)
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LayoutState {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub margin: [f32; 4],  // top, right, bottom, left
    pub padding: [f32; 4],
    pub border: [f32; 4],
}

impl LayoutState {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get property by index
    pub fn get(&self, prop: LayoutProperty) -> f32 {
        match prop {
            LayoutProperty::X => self.x,
            LayoutProperty::Y => self.y,
            LayoutProperty::Width => self.width,
            LayoutProperty::Height => self.height,
            LayoutProperty::MarginTop => self.margin[0],
            LayoutProperty::MarginRight => self.margin[1],
            LayoutProperty::MarginBottom => self.margin[2],
            LayoutProperty::MarginLeft => self.margin[3],
            LayoutProperty::PaddingTop => self.padding[0],
            LayoutProperty::PaddingRight => self.padding[1],
            LayoutProperty::PaddingBottom => self.padding[2],
            LayoutProperty::PaddingLeft => self.padding[3],
            LayoutProperty::BorderTop => self.border[0],
            LayoutProperty::BorderRight => self.border[1],
            LayoutProperty::BorderBottom => self.border[2],
            LayoutProperty::BorderLeft => self.border[3],
        }
    }
    
    /// Set property by index
    pub fn set(&mut self, prop: LayoutProperty, value: f32) {
        match prop {
            LayoutProperty::X => self.x = value,
            LayoutProperty::Y => self.y = value,
            LayoutProperty::Width => self.width = value,
            LayoutProperty::Height => self.height = value,
            LayoutProperty::MarginTop => self.margin[0] = value,
            LayoutProperty::MarginRight => self.margin[1] = value,
            LayoutProperty::MarginBottom => self.margin[2] = value,
            LayoutProperty::MarginLeft => self.margin[3] = value,
            LayoutProperty::PaddingTop => self.padding[0] = value,
            LayoutProperty::PaddingRight => self.padding[1] = value,
            LayoutProperty::PaddingBottom => self.padding[2] = value,
            LayoutProperty::PaddingLeft => self.padding[3] = value,
            LayoutProperty::BorderTop => self.border[0] = value,
            LayoutProperty::BorderRight => self.border[1] = value,
            LayoutProperty::BorderBottom => self.border[2] = value,
            LayoutProperty::BorderLeft => self.border[3] = value,
        }
    }
    
    /// Memory size
    pub const fn size() -> usize {
        std::mem::size_of::<Self>()
    }
}

/// Single property delta
#[derive(Debug, Clone, Copy)]
pub struct PropertyDelta {
    /// Which property changed
    pub property: LayoutProperty,
    /// Delta value
    pub delta: f32,
}

impl PropertyDelta {
    /// Size of a single delta
    pub const fn size() -> usize {
        std::mem::size_of::<Self>()
    }
}

/// Layout delta - compact representation of changes
#[derive(Debug, Clone)]
pub struct LayoutDelta {
    /// Node ID
    pub node_id: u32,
    /// Deltas for changed properties
    pub deltas: Vec<PropertyDelta>,
}

impl LayoutDelta {
    pub fn new(node_id: u32) -> Self {
        Self {
            node_id,
            deltas: Vec::new(),
        }
    }
    
    /// Add a property delta
    pub fn add(&mut self, property: LayoutProperty, delta: f32) {
        if delta.abs() > f32::EPSILON {
            self.deltas.push(PropertyDelta { property, delta });
        }
    }
    
    /// Check if empty (no changes)
    pub fn is_empty(&self) -> bool {
        self.deltas.is_empty()
    }
    
    /// Apply delta to a layout state
    pub fn apply(&self, state: &mut LayoutState) {
        for delta in &self.deltas {
            let current = state.get(delta.property);
            state.set(delta.property, current + delta.delta);
        }
    }
    
    /// Memory size of this delta
    pub fn size(&self) -> usize {
        4 + self.deltas.len() * PropertyDelta::size() // node_id + deltas
    }
}

/// Compute delta between two layout states
pub fn compute_delta(node_id: u32, old: &LayoutState, new: &LayoutState) -> LayoutDelta {
    let mut delta = LayoutDelta::new(node_id);
    
    // Check each property
    for prop_idx in 0..LayoutProperty::COUNT {
        let prop = unsafe { std::mem::transmute::<u8, LayoutProperty>(prop_idx as u8) };
        let old_val = old.get(prop);
        let new_val = new.get(prop);
        delta.add(prop, new_val - old_val);
    }
    
    delta
}

/// Delta-encoded layout storage
#[derive(Debug)]
pub struct DeltaLayoutStore {
    /// Base layout states
    base_states: HashMap<u32, LayoutState>,
    /// Pending deltas
    pending_deltas: Vec<LayoutDelta>,
    /// Statistics
    stats: DeltaStats,
}

/// Statistics for delta encoding
#[derive(Debug, Clone, Copy, Default)]
pub struct DeltaStats {
    pub full_updates: u64,
    pub delta_updates: u64,
    pub bytes_full: u64,
    pub bytes_delta: u64,
    pub properties_changed: u64,
}

impl DeltaStats {
    pub fn compression_ratio(&self) -> f64 {
        if self.bytes_delta == 0 {
            0.0
        } else {
            self.bytes_full as f64 / self.bytes_delta as f64
        }
    }
    
    pub fn avg_properties_changed(&self) -> f64 {
        if self.delta_updates == 0 {
            0.0
        } else {
            self.properties_changed as f64 / self.delta_updates as f64
        }
    }
}

impl Default for DeltaLayoutStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DeltaLayoutStore {
    pub fn new() -> Self {
        Self {
            base_states: HashMap::new(),
            pending_deltas: Vec::new(),
            stats: DeltaStats::default(),
        }
    }
    
    /// Set initial layout state
    pub fn set_base(&mut self, node_id: u32, state: LayoutState) {
        self.base_states.insert(node_id, state);
        self.stats.full_updates += 1;
        self.stats.bytes_full += LayoutState::size() as u64;
    }
    
    /// Update layout with delta encoding
    pub fn update(&mut self, node_id: u32, new_state: LayoutState) -> Option<LayoutDelta> {
        if let Some(old_state) = self.base_states.get(&node_id) {
            let delta = compute_delta(node_id, old_state, &new_state);
            
            if delta.is_empty() {
                return None;
            }
            
            // Update stats
            self.stats.delta_updates += 1;
            self.stats.bytes_delta += delta.size() as u64;
            self.stats.properties_changed += delta.deltas.len() as u64;
            
            // Update base state
            self.base_states.insert(node_id, new_state);
            
            Some(delta)
        } else {
            // First time - set base
            self.set_base(node_id, new_state);
            None
        }
    }
    
    /// Get current layout state
    pub fn get(&self, node_id: u32) -> Option<&LayoutState> {
        self.base_states.get(&node_id)
    }
    
    /// Get statistics
    pub fn stats(&self) -> &DeltaStats {
        &self.stats
    }
    
    /// Clear all states
    pub fn clear(&mut self) {
        self.base_states.clear();
        self.pending_deltas.clear();
    }
}

/// Serialize deltas to byte buffer
pub fn serialize_deltas(deltas: &[LayoutDelta]) -> Vec<u8> {
    let mut buf = Vec::new();
    
    // Write count
    buf.extend_from_slice(&(deltas.len() as u32).to_le_bytes());
    
    for delta in deltas {
        // Write node ID
        buf.extend_from_slice(&delta.node_id.to_le_bytes());
        // Write delta count
        buf.push(delta.deltas.len() as u8);
        
        for d in &delta.deltas {
            buf.push(d.property as u8);
            buf.extend_from_slice(&d.delta.to_le_bytes());
        }
    }
    
    buf
}

/// Deserialize deltas from byte buffer
pub fn deserialize_deltas(data: &[u8]) -> Vec<LayoutDelta> {
    let mut deltas = Vec::new();
    let mut pos = 0;
    
    if data.len() < 4 {
        return deltas;
    }
    
    let count = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    pos += 4;
    
    for _ in 0..count {
        if pos + 5 > data.len() {
            break;
        }
        
        let node_id = u32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]);
        pos += 4;
        
        let delta_count = data[pos] as usize;
        pos += 1;
        
        let mut delta = LayoutDelta::new(node_id);
        
        for _ in 0..delta_count {
            if pos + 5 > data.len() {
                break;
            }
            
            let prop = data[pos];
            pos += 1;
            
            let value = f32::from_le_bytes([data[pos], data[pos+1], data[pos+2], data[pos+3]]);
            pos += 4;
            
            if prop < LayoutProperty::COUNT as u8 {
                delta.deltas.push(PropertyDelta {
                    property: unsafe { std::mem::transmute(prop) },
                    delta: value,
                });
            }
        }
        
        deltas.push(delta);
    }
    
    deltas
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_layout_state() {
        let mut state = LayoutState::new();
        state.set(LayoutProperty::X, 100.0);
        state.set(LayoutProperty::MarginTop, 10.0);
        
        assert_eq!(state.get(LayoutProperty::X), 100.0);
        assert_eq!(state.get(LayoutProperty::MarginTop), 10.0);
    }
    
    #[test]
    fn test_compute_delta() {
        let old = LayoutState {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
            ..Default::default()
        };
        
        let new = LayoutState {
            x: 10.0, // Changed
            y: 0.0,
            width: 100.0,
            height: 60.0, // Changed
            ..Default::default()
        };
        
        let delta = compute_delta(1, &old, &new);
        
        assert_eq!(delta.deltas.len(), 2);
        
        // Apply delta to old state
        let mut result = old;
        delta.apply(&mut result);
        
        assert_eq!(result.x, 10.0);
        assert_eq!(result.height, 60.0);
    }
    
    #[test]
    fn test_delta_store() {
        let mut store = DeltaLayoutStore::new();
        
        let initial = LayoutState {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            ..Default::default()
        };
        
        store.set_base(1, initial);
        
        // Update with small change
        let updated = LayoutState {
            x: 5.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
            ..Default::default()
        };
        
        let delta = store.update(1, updated);
        assert!(delta.is_some());
        
        let d = delta.unwrap();
        assert_eq!(d.deltas.len(), 1); // Only x changed
        
        // Check compression ratio
        println!("Compression ratio: {:.2}x", store.stats().compression_ratio());
    }
    
    #[test]
    fn test_serialization() {
        let mut delta = LayoutDelta::new(42);
        delta.add(LayoutProperty::X, 10.0);
        delta.add(LayoutProperty::Height, -5.0);
        
        let serialized = serialize_deltas(&[delta]);
        let deserialized = deserialize_deltas(&serialized);
        
        assert_eq!(deserialized.len(), 1);
        assert_eq!(deserialized[0].node_id, 42);
        assert_eq!(deserialized[0].deltas.len(), 2);
    }
}
