//! CSS Rule Tree
//!
//! Servo-inspired rule tree for efficient style sharing.
//! Rules with matching selectors share subtrees.

use std::collections::HashMap;
use std::sync::Arc;

/// Rule tree node
#[derive(Debug)]
pub struct RuleNode {
    /// Parent node (None for root)
    pub parent: Option<Arc<RuleNode>>,
    /// Rule source
    pub source: RuleSource,
    /// Declarations from this rule
    pub declarations: Arc<DeclarationBlock>,
    /// Specificity
    pub specificity: RuleSpecificity,
    /// Cascade level
    pub level: CascadeLevel,
}

/// Source of a style rule
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleSource {
    /// User agent stylesheet
    UserAgent,
    /// User stylesheet
    User,
    /// Author stylesheet
    Author,
    /// Inline style
    Inline,
    /// Animation
    Animation,
    /// Transition
    Transition,
}

/// Cascade level for ordering
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CascadeLevel(pub u8);

impl CascadeLevel {
    pub const UA_NORMAL: CascadeLevel = CascadeLevel(0);
    pub const USER_NORMAL: CascadeLevel = CascadeLevel(1);
    pub const AUTHOR_NORMAL: CascadeLevel = CascadeLevel(2);
    pub const AUTHOR_IMPORTANT: CascadeLevel = CascadeLevel(3);
    pub const USER_IMPORTANT: CascadeLevel = CascadeLevel(4);
    pub const UA_IMPORTANT: CascadeLevel = CascadeLevel(5);
    pub const ANIMATION: CascadeLevel = CascadeLevel(6);
    pub const TRANSITION: CascadeLevel = CascadeLevel(7);
}

/// CSS specificity (compact)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct RuleSpecificity(pub u32);

impl RuleSpecificity {
    pub fn new(ids: u8, classes: u8, types: u8) -> Self {
        Self(((ids as u32) << 16) | ((classes as u32) << 8) | (types as u32))
    }
    
    pub fn ids(&self) -> u8 {
        ((self.0 >> 16) & 0xFF) as u8
    }
    
    pub fn classes(&self) -> u8 {
        ((self.0 >> 8) & 0xFF) as u8
    }
    
    pub fn types(&self) -> u8 {
        (self.0 & 0xFF) as u8
    }
}

/// Declaration block (shared via Arc)
#[derive(Debug, Clone, Default)]
pub struct DeclarationBlock {
    /// Property declarations
    pub declarations: Vec<Declaration>,
    /// Hash for quick comparison
    pub hash: u64,
}

/// Single declaration
#[derive(Debug, Clone)]
pub struct Declaration {
    pub property_id: PropertyId,
    pub value: PackedValue,
    pub important: bool,
}

/// Property identifier (compact)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PropertyId(pub u16);

impl PropertyId {
    // Common properties
    pub const DISPLAY: PropertyId = PropertyId(0);
    pub const POSITION: PropertyId = PropertyId(1);
    pub const WIDTH: PropertyId = PropertyId(2);
    pub const HEIGHT: PropertyId = PropertyId(3);
    pub const MARGIN_TOP: PropertyId = PropertyId(4);
    pub const MARGIN_RIGHT: PropertyId = PropertyId(5);
    pub const MARGIN_BOTTOM: PropertyId = PropertyId(6);
    pub const MARGIN_LEFT: PropertyId = PropertyId(7);
    pub const PADDING_TOP: PropertyId = PropertyId(8);
    pub const PADDING_RIGHT: PropertyId = PropertyId(9);
    pub const PADDING_BOTTOM: PropertyId = PropertyId(10);
    pub const PADDING_LEFT: PropertyId = PropertyId(11);
    pub const COLOR: PropertyId = PropertyId(12);
    pub const BACKGROUND_COLOR: PropertyId = PropertyId(13);
    pub const FONT_SIZE: PropertyId = PropertyId(14);
    pub const FONT_WEIGHT: PropertyId = PropertyId(15);
    pub const FONT_FAMILY: PropertyId = PropertyId(16);
    // ... more properties
    pub const MAX_PROPERTY_ID: u16 = 512;
}

/// Bit-packed CSS value (4 bytes)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackedValue(pub u32);

impl PackedValue {
    /// Tag bits (top 4 bits)
    const TAG_MASK: u32 = 0xF0000000;
    const TAG_SHIFT: u32 = 28;
    
    // Value types
    const TAG_INITIAL: u32 = 0;
    const TAG_INHERIT: u32 = 1;
    const TAG_LENGTH_PX: u32 = 2;
    const TAG_PERCENT: u32 = 3;
    const TAG_COLOR: u32 = 4;
    const TAG_KEYWORD: u32 = 5;
    const TAG_INTEGER: u32 = 6;
    const TAG_INDEX: u32 = 7; // Index into interned table
    
    pub fn initial() -> Self {
        Self(Self::TAG_INITIAL << Self::TAG_SHIFT)
    }
    
    pub fn inherit() -> Self {
        Self(Self::TAG_INHERIT << Self::TAG_SHIFT)
    }
    
    pub fn length_px(px: f32) -> Self {
        // Store as fixed-point (12.16)
        let fixed = (px * 256.0) as i32;
        let bits = (fixed as u32) & 0x0FFFFFFF;
        Self((Self::TAG_LENGTH_PX << Self::TAG_SHIFT) | bits)
    }
    
    pub fn percent(pct: f32) -> Self {
        let fixed = (pct * 256.0) as i32;
        let bits = (fixed as u32) & 0x0FFFFFFF;
        Self((Self::TAG_PERCENT << Self::TAG_SHIFT) | bits)
    }
    
    pub fn color_index(index: u8) -> Self {
        Self((Self::TAG_COLOR << Self::TAG_SHIFT) | (index as u32))
    }
    
    pub fn keyword(id: u16) -> Self {
        Self((Self::TAG_KEYWORD << Self::TAG_SHIFT) | (id as u32))
    }
    
    pub fn get_tag(&self) -> u32 {
        (self.0 & Self::TAG_MASK) >> Self::TAG_SHIFT
    }
    
    pub fn is_initial(&self) -> bool {
        self.get_tag() == Self::TAG_INITIAL
    }
    
    pub fn is_inherit(&self) -> bool {
        self.get_tag() == Self::TAG_INHERIT
    }
    
    pub fn as_length_px(&self) -> Option<f32> {
        if self.get_tag() == Self::TAG_LENGTH_PX {
            let bits = (self.0 & 0x0FFFFFFF) as i32;
            // Sign extend if negative
            let fixed = if bits & 0x08000000 != 0 {
                bits | 0xF0000000u32 as i32
            } else {
                bits
            };
            Some(fixed as f32 / 256.0)
        } else {
            None
        }
    }
    
    pub fn as_percent(&self) -> Option<f32> {
        if self.get_tag() == Self::TAG_PERCENT {
            let bits = (self.0 & 0x0FFFFFFF) as i32;
            Some(bits as f32 / 256.0)
        } else {
            None
        }
    }
}

/// CSS Property Presence Bitmask
/// 64 bytes = 512 bits = covers up to 512 properties
#[derive(Debug, Clone, Default)]
pub struct OptPropertyMask {
    bits: [u64; 8],
}

impl OptPropertyMask {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn set(&mut self, property: PropertyId) {
        let idx = (property.0 as usize) / 64;
        let bit = (property.0 as usize) % 64;
        if idx < 8 {
            self.bits[idx] |= 1u64 << bit;
        }
    }
    
    pub fn get(&self, property: PropertyId) -> bool {
        let idx = (property.0 as usize) / 64;
        let bit = (property.0 as usize) % 64;
        if idx < 8 {
            (self.bits[idx] & (1u64 << bit)) != 0
        } else {
            false
        }
    }
    
    pub fn clear(&mut self, property: PropertyId) {
        let idx = (property.0 as usize) / 64;
        let bit = (property.0 as usize) % 64;
        if idx < 8 {
            self.bits[idx] &= !(1u64 << bit);
        }
    }
    
    pub fn count(&self) -> u32 {
        self.bits.iter().map(|b| b.count_ones()).sum()
    }
    
    /// Merge another mask (OR)
    pub fn merge(&mut self, other: &OptPropertyMask) {
        for i in 0..8 {
            self.bits[i] |= other.bits[i];
        }
    }
}

/// Interned color values
#[derive(Debug, Default)]
pub struct ColorInterner {
    colors: Vec<(u8, u8, u8, u8)>,
    lookup: HashMap<u32, u8>,
}

impl ColorInterner {
    pub fn new() -> Self {
        let mut interner = Self::default();
        // Pre-intern common colors
        interner.intern(0, 0, 0, 255);       // black = 0
        interner.intern(255, 255, 255, 255); // white = 1
        interner.intern(0, 0, 0, 0);         // transparent = 2
        interner
    }
    
    pub fn intern(&mut self, r: u8, g: u8, b: u8, a: u8) -> u8 {
        let key = ((r as u32) << 24) | ((g as u32) << 16) | ((b as u32) << 8) | (a as u32);
        
        if let Some(&idx) = self.lookup.get(&key) {
            return idx;
        }
        
        let idx = self.colors.len() as u8;
        if idx < 255 {
            self.colors.push((r, g, b, a));
            self.lookup.insert(key, idx);
            idx
        } else {
            0 // Fallback to black if table full
        }
    }
    
    pub fn get(&self, index: u8) -> Option<(u8, u8, u8, u8)> {
        self.colors.get(index as usize).copied()
    }
    
    pub fn len(&self) -> usize {
        self.colors.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.colors.is_empty()
    }
}

/// Rule tree for style sharing
#[derive(Debug, Default)]
pub struct RuleTree {
    /// Root node
    root: Option<Arc<RuleNode>>,
    /// All nodes
    nodes: Vec<Arc<RuleNode>>,
}

impl RuleTree {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Insert a rule path and return the leaf node
    pub fn insert(&mut self, rules: Vec<(RuleSource, Arc<DeclarationBlock>, RuleSpecificity, CascadeLevel)>) -> Option<Arc<RuleNode>> {
        let mut current = self.root.clone();
        
        for (source, declarations, specificity, level) in rules {
            let node = Arc::new(RuleNode {
                parent: current.clone(),
                source,
                declarations,
                specificity,
                level,
            });
            self.nodes.push(Arc::clone(&node));
            current = Some(node);
        }
        
        current
    }
    
    /// Get styles by walking up from a node (returns cloned Arcs)
    pub fn get_style_arcs(&self, node: &Arc<RuleNode>) -> Vec<Arc<DeclarationBlock>> {
        let mut styles = Vec::new();
        let mut current = Some(Arc::clone(node));
        
        while let Some(n) = current {
            styles.push(Arc::clone(&n.declarations));
            current = n.parent.clone();
        }
        
        styles.reverse();
        styles
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_specificity() {
        let s = RuleSpecificity::new(1, 2, 3);
        assert_eq!(s.ids(), 1);
        assert_eq!(s.classes(), 2);
        assert_eq!(s.types(), 3);
    }
    
    #[test]
    fn test_packed_value_length() {
        let v = PackedValue::length_px(16.5);
        assert_eq!(v.as_length_px(), Some(16.5));
    }
    
    #[test]
    fn test_packed_value_percent() {
        let v = PackedValue::percent(50.0);
        assert_eq!(v.as_percent(), Some(50.0));
    }
    
    #[test]
    fn test_property_mask() {
        let mut mask = OptPropertyMask::new();
        mask.set(PropertyId::DISPLAY);
        mask.set(PropertyId::WIDTH);
        
        assert!(mask.get(PropertyId::DISPLAY));
        assert!(mask.get(PropertyId::WIDTH));
        assert!(!mask.get(PropertyId::HEIGHT));
        assert_eq!(mask.count(), 2);
    }
    
    #[test]
    fn test_color_interner() {
        let mut interner = ColorInterner::new();
        
        let black = interner.intern(0, 0, 0, 255);
        assert_eq!(black, 0); // Pre-interned
        
        let custom = interner.intern(255, 128, 64, 255);
        assert_eq!(interner.get(custom), Some((255, 128, 64, 255)));
    }
}
