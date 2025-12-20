//! Advanced Memory Optimizations
//!
//! Zero-copy parsing, mmap, lazy loading, SmallVec, packed enums.

use std::collections::HashMap;
use std::sync::Arc;

/// Zero-copy string slice into source
#[derive(Debug, Clone, Copy)]
pub struct SourceSlice<'a> {
    pub source: &'a str,
    pub start: usize,
    pub end: usize,
}

impl<'a> SourceSlice<'a> {
    pub fn new(source: &'a str, start: usize, end: usize) -> Self {
        Self { source, start, end }
    }
    
    pub fn as_str(&self) -> &'a str {
        &self.source[self.start..self.end]
    }
    
    pub fn len(&self) -> usize {
        self.end - self.start
    }
    
    pub fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// Zero-copy CSS value
#[derive(Debug)]
pub enum ZeroCopyCssValue<'a> {
    /// String slice into source
    Slice(SourceSlice<'a>),
    /// Interned common value
    Interned(InternedCssValue),
    /// Computed/dynamic value (owned)
    Owned(String),
}

/// Interned common CSS values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum InternedCssValue {
    Auto = 0,
    None = 1,
    Inherit = 2,
    Initial = 3,
    Unset = 4,
    Normal = 5,
    Hidden = 6,
    Visible = 7,
    Block = 8,
    Inline = 9,
    Flex = 10,
    Grid = 11,
    Absolute = 12,
    Relative = 13,
    Fixed = 14,
    Sticky = 15,
    Static = 16,
    Left = 17,
    Right = 18,
    Center = 19,
    Top = 20,
    Bottom = 21,
    Solid = 22,
    Dotted = 23,
    Dashed = 24,
    Zero = 25,
    Full = 26,
}

impl InternedCssValue {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "auto" => Self::Auto,
            "none" => Self::None,
            "inherit" => Self::Inherit,
            "initial" => Self::Initial,
            "unset" => Self::Unset,
            "normal" => Self::Normal,
            "hidden" => Self::Hidden,
            "visible" => Self::Visible,
            "block" => Self::Block,
            "inline" => Self::Inline,
            "flex" => Self::Flex,
            "grid" => Self::Grid,
            "absolute" => Self::Absolute,
            "relative" => Self::Relative,
            "fixed" => Self::Fixed,
            "sticky" => Self::Sticky,
            "static" => Self::Static,
            "left" => Self::Left,
            "right" => Self::Right,
            "center" => Self::Center,
            "top" => Self::Top,
            "bottom" => Self::Bottom,
            "solid" => Self::Solid,
            "dotted" => Self::Dotted,
            "dashed" => Self::Dashed,
            "0" => Self::Zero,
            "100%" => Self::Full,
            _ => return None,
        })
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::Inherit => "inherit",
            Self::Initial => "initial",
            Self::Unset => "unset",
            Self::Normal => "normal",
            Self::Hidden => "hidden",
            Self::Visible => "visible",
            Self::Block => "block",
            Self::Inline => "inline",
            Self::Flex => "flex",
            Self::Grid => "grid",
            Self::Absolute => "absolute",
            Self::Relative => "relative",
            Self::Fixed => "fixed",
            Self::Sticky => "sticky",
            Self::Static => "static",
            Self::Left => "left",
            Self::Right => "right",
            Self::Center => "center",
            Self::Top => "top",
            Self::Bottom => "bottom",
            Self::Solid => "solid",
            Self::Dotted => "dotted",
            Self::Dashed => "dashed",
            Self::Zero => "0",
            Self::Full => "100%",
        }
    }
}

/// SmallVec for inline storage
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
                    // Spill to heap
                    let mut vec: Vec<T> = data.iter_mut()
                        .filter_map(|opt| opt.take())
                        .collect();
                    vec.push(value);
                    *self = Self::Heap(vec);
                }
            }
            Self::Heap(vec) => vec.push(value),
        }
    }
    
    pub fn len(&self) -> usize {
        match self {
            Self::Inline { len, .. } => *len,
            Self::Heap(vec) => vec.len(),
        }
    }
    
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    pub fn get(&self, index: usize) -> Option<&T> {
        match self {
            Self::Inline { data, len } => {
                if index < *len {
                    data[index].as_ref()
                } else {
                    None
                }
            }
            Self::Heap(vec) => vec.get(index),
        }
    }
    
    pub fn is_inline(&self) -> bool {
        matches!(self, Self::Inline { .. })
    }
}

impl<T: Default + Clone, const N: usize> Default for SmallVec<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Packed node type (1 byte)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PackedNodeType {
    Element = 1,
    Text = 3,
    CData = 4,
    Comment = 8,
    Document = 9,
    DocumentType = 10,
    DocumentFragment = 11,
}

/// Packed display value (1 byte)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PackedDisplay {
    None = 0,
    Block = 1,
    Inline = 2,
    InlineBlock = 3,
    Flex = 4,
    InlineFlex = 5,
    Grid = 6,
    InlineGrid = 7,
    Table = 8,
    TableRow = 9,
    TableCell = 10,
    ListItem = 11,
    Contents = 12,
}

/// Packed CSS unit (1 byte)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PackedUnit {
    Px = 0,
    Em = 1,
    Rem = 2,
    Percent = 3,
    Vw = 4,
    Vh = 5,
    Vmin = 6,
    Vmax = 7,
    Ch = 8,
    Ex = 9,
    Auto = 10,
    None = 11,
}

/// Combined tag + flags (4 bytes)
#[derive(Debug, Clone, Copy)]
pub struct PackedElement {
    /// Tag ID (12 bits = 4096 unique tags)
    /// Flags (20 bits)
    packed: u32,
}

impl PackedElement {
    const TAG_MASK: u32 = 0xFFF;
    const FLAG_HAS_ID: u32 = 1 << 12;
    const FLAG_HAS_CLASS: u32 = 1 << 13;
    const FLAG_HAS_STYLE: u32 = 1 << 14;
    const FLAG_FOCUSABLE: u32 = 1 << 15;
    const FLAG_HIDDEN: u32 = 1 << 16;
    const FLAG_DISABLED: u32 = 1 << 17;
    const FLAG_CHECKED: u32 = 1 << 18;
    const FLAG_DIRTY: u32 = 1 << 19;
    
    pub fn new(tag_id: u16) -> Self {
        Self { packed: tag_id as u32 & Self::TAG_MASK }
    }
    
    pub fn tag_id(&self) -> u16 {
        (self.packed & Self::TAG_MASK) as u16
    }
    
    pub fn has_id(&self) -> bool { self.packed & Self::FLAG_HAS_ID != 0 }
    pub fn has_class(&self) -> bool { self.packed & Self::FLAG_HAS_CLASS != 0 }
    pub fn has_style(&self) -> bool { self.packed & Self::FLAG_HAS_STYLE != 0 }
    pub fn is_focusable(&self) -> bool { self.packed & Self::FLAG_FOCUSABLE != 0 }
    pub fn is_hidden(&self) -> bool { self.packed & Self::FLAG_HIDDEN != 0 }
    
    pub fn set_has_id(&mut self, v: bool) {
        if v { self.packed |= Self::FLAG_HAS_ID; }
        else { self.packed &= !Self::FLAG_HAS_ID; }
    }
    
    pub fn set_has_class(&mut self, v: bool) {
        if v { self.packed |= Self::FLAG_HAS_CLASS; }
        else { self.packed &= !Self::FLAG_HAS_CLASS; }
    }
    
    pub fn set_dirty(&mut self, v: bool) {
        if v { self.packed |= Self::FLAG_DIRTY; }
        else { self.packed &= !Self::FLAG_DIRTY; }
    }
}

/// Style presence bitfield
#[derive(Debug, Clone, Copy, Default)]
pub struct StylePresenceBits(u64);

impl StylePresenceBits {
    pub const DISPLAY: u64 = 1 << 0;
    pub const POSITION: u64 = 1 << 1;
    pub const WIDTH: u64 = 1 << 2;
    pub const HEIGHT: u64 = 1 << 3;
    pub const MARGIN: u64 = 1 << 4;
    pub const PADDING: u64 = 1 << 5;
    pub const BORDER: u64 = 1 << 6;
    pub const COLOR: u64 = 1 << 7;
    pub const BACKGROUND: u64 = 1 << 8;
    pub const FONT: u64 = 1 << 9;
    pub const FLEX: u64 = 1 << 10;
    pub const GRID: u64 = 1 << 11;
    pub const TRANSFORM: u64 = 1 << 12;
    pub const OPACITY: u64 = 1 << 13;
    pub const OVERFLOW: u64 = 1 << 14;
    pub const ZINDEX: u64 = 1 << 15;
    
    pub fn has(&self, bit: u64) -> bool {
        self.0 & bit != 0
    }
    
    pub fn set(&mut self, bit: u64) {
        self.0 |= bit;
    }
    
    pub fn clear(&mut self, bit: u64) {
        self.0 &= !bit;
    }
    
    pub fn count(&self) -> u32 {
        self.0.count_ones()
    }
}

/// Memory-mapped resource
#[derive(Debug)]
pub struct MmapResource {
    pub path: String,
    pub size: usize,
    /// Would use actual mmap in production
    pub data: Arc<Vec<u8>>,
}

impl MmapResource {
    pub fn new(path: &str, data: Vec<u8>) -> Self {
        Self {
            path: path.to_string(),
            size: data.len(),
            data: Arc::new(data),
        }
    }
    
    pub fn as_slice(&self) -> &[u8] {
        &self.data
    }
}

/// Mmap resource cache
#[derive(Debug, Default)]
pub struct MmapCache {
    resources: HashMap<String, MmapResource>,
}

impl MmapCache {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn get(&self, path: &str) -> Option<&MmapResource> {
        self.resources.get(path)
    }
    
    pub fn insert(&mut self, resource: MmapResource) {
        self.resources.insert(resource.path.clone(), resource);
    }
    
    /// Share resource across tabs (returns Arc clone)
    pub fn share(&self, path: &str) -> Option<Arc<Vec<u8>>> {
        self.resources.get(path).map(|r| Arc::clone(&r.data))
    }
    
    pub fn total_size(&self) -> usize {
        self.resources.values().map(|r| r.size).sum()
    }
}

/// Lazy loader for deferred operations
pub struct LazyLoader<T> {
    loaded: Option<T>,
    load_fn: Option<Box<dyn FnOnce() -> T + Send>>,
}

impl<T: std::fmt::Debug> std::fmt::Debug for LazyLoader<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazyLoader")
            .field("loaded", &self.loaded)
            .field("load_fn", &self.load_fn.is_some())
            .finish()
    }
}

impl<T> LazyLoader<T> {
    pub fn new<F: FnOnce() -> T + Send + 'static>(f: F) -> Self {
        Self {
            loaded: None,
            load_fn: Some(Box::new(f)),
        }
    }
    
    pub fn immediate(value: T) -> Self {
        Self {
            loaded: Some(value),
            load_fn: None,
        }
    }
    
    pub fn get(&mut self) -> &T {
        if self.loaded.is_none() {
            if let Some(f) = self.load_fn.take() {
                self.loaded = Some(f());
            }
        }
        self.loaded.as_ref().unwrap()
    }
    
    pub fn is_loaded(&self) -> bool {
        self.loaded.is_some()
    }
}

/// Viewport-only layout region
#[derive(Debug)]
pub struct ViewportRegion {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub buffer: f32, // Extra area around viewport
}

impl ViewportRegion {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height, buffer: 200.0 }
    }
    
    pub fn contains(&self, px: f32, py: f32, pw: f32, ph: f32) -> bool {
        let vx = self.x - self.buffer;
        let vy = self.y - self.buffer;
        let vw = self.width + self.buffer * 2.0;
        let vh = self.height + self.buffer * 2.0;
        
        px + pw > vx && px < vx + vw && py + ph > vy && py < vy + vh
    }
    
    pub fn should_layout(&self, element_y: f32, element_height: f32) -> bool {
        self.contains(0.0, element_y, self.width, element_height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_interned_css_value() {
        assert_eq!(InternedCssValue::from_str("auto"), Some(InternedCssValue::Auto));
        assert_eq!(InternedCssValue::Auto.as_str(), "auto");
        assert_eq!(std::mem::size_of::<InternedCssValue>(), 1);
    }
    
    #[test]
    fn test_small_vec() {
        let mut sv: SmallVec<i32, 4> = SmallVec::new();
        sv.push(1);
        sv.push(2);
        sv.push(3);
        
        assert!(sv.is_inline());
        assert_eq!(sv.len(), 3);
        
        sv.push(4);
        sv.push(5); // Spills to heap
        
        assert!(!sv.is_inline());
        assert_eq!(sv.len(), 5);
    }
    
    #[test]
    fn test_packed_element() {
        let mut elem = PackedElement::new(42);
        assert_eq!(elem.tag_id(), 42);
        
        elem.set_has_id(true);
        elem.set_has_class(true);
        
        assert!(elem.has_id());
        assert!(elem.has_class());
        assert!(!elem.has_style());
        
        assert_eq!(std::mem::size_of::<PackedElement>(), 4);
    }
    
    #[test]
    fn test_style_presence() {
        let mut bits = StylePresenceBits::default();
        bits.set(StylePresenceBits::DISPLAY);
        bits.set(StylePresenceBits::COLOR);
        
        assert!(bits.has(StylePresenceBits::DISPLAY));
        assert!(bits.has(StylePresenceBits::COLOR));
        assert!(!bits.has(StylePresenceBits::FONT));
        assert_eq!(bits.count(), 2);
    }
    
    #[test]
    fn test_viewport_region() {
        let vp = ViewportRegion::new(0.0, 0.0, 800.0, 600.0);
        
        assert!(vp.should_layout(100.0, 50.0)); // In viewport
        assert!(!vp.should_layout(2000.0, 50.0)); // Out of viewport
    }
}
