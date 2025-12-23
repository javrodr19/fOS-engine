//! Advanced Memory Optimization Integration
//!
//! SmallVec, packed enums, interned values, lazy loading.

use std::sync::Arc;
use std::collections::HashMap;

/// SmallVec for inline storage (avoids heap allocation for small arrays)
#[derive(Debug)]
pub enum SmallVec<T, const N: usize> {
    Inline { data: [Option<T>; N], len: usize },
    Heap(Vec<T>),
}

impl<T: Default + Clone, const N: usize> SmallVec<T, N> {
    pub fn new() -> Self {
        Self::Inline {
            data: std::array::from_fn(|_| None),
            len: 0,
        }
    }
    
    pub fn push(&mut self, value: T) {
        match self {
            Self::Inline { data, len } => {
                if *len < N {
                    data[*len] = Some(value);
                    *len += 1;
                } else {
                    let mut vec: Vec<T> = data.iter_mut().filter_map(|opt| opt.take()).collect();
                    vec.push(value);
                    *self = Self::Heap(vec);
                }
            }
            Self::Heap(vec) => vec.push(value),
        }
    }
    
    pub fn len(&self) -> usize {
        match self { Self::Inline { len, .. } => *len, Self::Heap(vec) => vec.len() }
    }
    
    pub fn is_empty(&self) -> bool { self.len() == 0 }
    pub fn is_inline(&self) -> bool { matches!(self, Self::Inline { .. }) }
    
    pub fn get(&self, index: usize) -> Option<&T> {
        match self {
            Self::Inline { data, len } => if index < *len { data[index].as_ref() } else { None },
            Self::Heap(vec) => vec.get(index),
        }
    }
}

impl<T: Default + Clone, const N: usize> Default for SmallVec<T, N> {
    fn default() -> Self { Self::new() }
}

/// Interned common CSS values (1 byte instead of String)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InternedCssValue {
    Auto, None, Inherit, Initial, Unset, Normal, Hidden, Visible,
    Block, Inline, Flex, Grid, Absolute, Relative, Fixed, Sticky, Static,
    Left, Right, Center, Top, Bottom, Solid, Dotted, Dashed, Zero, Full,
}

impl InternedCssValue {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "auto" => Self::Auto, "none" => Self::None, "inherit" => Self::Inherit,
            "block" => Self::Block, "inline" => Self::Inline, "flex" => Self::Flex,
            "absolute" => Self::Absolute, "relative" => Self::Relative,
            "left" => Self::Left, "right" => Self::Right, "center" => Self::Center,
            _ => return None,
        })
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto", Self::None => "none", Self::Inherit => "inherit",
            Self::Block => "block", Self::Inline => "inline", Self::Flex => "flex",
            Self::Absolute => "absolute", Self::Relative => "relative",
            Self::Left => "left", Self::Right => "right", Self::Center => "center",
            _ => "unknown",
        }
    }
}

/// Packed element flags (4 bytes for tag + 20 flags)
#[derive(Debug, Clone, Copy)]
pub struct PackedElement(u32);

impl PackedElement {
    const TAG_MASK: u32 = 0xFFF;
    const FLAG_HAS_ID: u32 = 1 << 12;
    const FLAG_HAS_CLASS: u32 = 1 << 13;
    const FLAG_HIDDEN: u32 = 1 << 16;
    const FLAG_DIRTY: u32 = 1 << 19;
    
    pub fn new(tag_id: u16) -> Self { Self(tag_id as u32 & Self::TAG_MASK) }
    pub fn tag_id(&self) -> u16 { (self.0 & Self::TAG_MASK) as u16 }
    pub fn has_id(&self) -> bool { self.0 & Self::FLAG_HAS_ID != 0 }
    pub fn has_class(&self) -> bool { self.0 & Self::FLAG_HAS_CLASS != 0 }
    pub fn is_hidden(&self) -> bool { self.0 & Self::FLAG_HIDDEN != 0 }
    pub fn is_dirty(&self) -> bool { self.0 & Self::FLAG_DIRTY != 0 }
    
    pub fn set_has_id(&mut self, v: bool) {
        if v { self.0 |= Self::FLAG_HAS_ID; } else { self.0 &= !Self::FLAG_HAS_ID; }
    }
    pub fn set_dirty(&mut self, v: bool) {
        if v { self.0 |= Self::FLAG_DIRTY; } else { self.0 &= !Self::FLAG_DIRTY; }
    }
}

/// Style presence bitfield (8 bytes tracks 64 properties)
#[derive(Debug, Clone, Copy, Default)]
pub struct StylePresenceBits(u64);

impl StylePresenceBits {
    pub const DISPLAY: u64 = 1 << 0;
    pub const WIDTH: u64 = 1 << 2;
    pub const HEIGHT: u64 = 1 << 3;
    pub const MARGIN: u64 = 1 << 4;
    pub const PADDING: u64 = 1 << 5;
    pub const COLOR: u64 = 1 << 7;
    pub const BACKGROUND: u64 = 1 << 8;
    
    pub fn has(&self, bit: u64) -> bool { self.0 & bit != 0 }
    pub fn set(&mut self, bit: u64) { self.0 |= bit; }
    pub fn count(&self) -> u32 { self.0.count_ones() }
}

/// Viewport-only layout region
#[derive(Debug)]
pub struct ViewportRegion {
    pub x: f32, pub y: f32, pub width: f32, pub height: f32, pub buffer: f32,
}

impl ViewportRegion {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height, buffer: 200.0 }
    }
    
    pub fn should_layout(&self, element_y: f32, element_height: f32) -> bool {
        let vy = self.y - self.buffer;
        let vh = self.height + self.buffer * 2.0;
        element_y + element_height > vy && element_y < vy + vh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_small_vec() {
        let mut sv: SmallVec<i32, 4> = SmallVec::new();
        sv.push(1); sv.push(2); sv.push(3);
        assert!(sv.is_inline());
        
        sv.push(4); sv.push(5); // Spills to heap
        assert!(!sv.is_inline());
    }
    
    #[test]
    fn test_packed_element() {
        let mut elem = PackedElement::new(42);
        assert_eq!(elem.tag_id(), 42);
        elem.set_has_id(true);
        assert!(elem.has_id());
        assert_eq!(std::mem::size_of::<PackedElement>(), 4);
    }
    
    #[test]
    fn test_interned_css() {
        assert_eq!(InternedCssValue::from_str("auto"), Some(InternedCssValue::Auto));
        assert_eq!(std::mem::size_of::<InternedCssValue>(), 1);
    }
}
