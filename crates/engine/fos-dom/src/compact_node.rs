//! Compact DOM Node Representation
//!
//! Ultra-compact node struct targeting 32 bytes (vs typical 100+).
//! Uses inline small text, packed enums, and bitfield flags.

use std::mem::size_of;

/// Compact node ID (4 bytes)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct CompactNodeId(pub u32);

impl CompactNodeId {
    pub const NONE: Self = Self(u32::MAX);
    
    #[inline]
    pub fn is_valid(self) -> bool {
        self != Self::NONE
    }
}

/// Node type (1 byte)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompactNodeType {
    Element = 1,
    Text = 3,
    Comment = 8,
    Document = 9,
    DocumentFragment = 11,
}

/// Node flags packed into 1 byte
#[derive(Debug, Clone, Copy, Default)]
#[repr(transparent)]
pub struct NodeFlags(pub u8);

impl NodeFlags {
    pub const ATTACHED: u8 = 1 << 0;
    pub const HAS_CHILDREN: u8 = 1 << 1;
    pub const HAS_ATTRIBUTES: u8 = 1 << 2;
    pub const IN_DOCUMENT: u8 = 1 << 3;
    pub const CONTENTS_DIRTY: u8 = 1 << 4;
    pub const STYLE_DIRTY: u8 = 1 << 5;
    pub const LAYOUT_DIRTY: u8 = 1 << 6;
    pub const IS_SHADOW_HOST: u8 = 1 << 7;
    
    #[inline]
    pub fn new() -> Self {
        Self(0)
    }
    
    #[inline]
    pub fn set(&mut self, flag: u8) {
        self.0 |= flag;
    }
    
    #[inline]
    pub fn clear(&mut self, flag: u8) {
        self.0 &= !flag;
    }
    
    #[inline]
    pub fn has(&self, flag: u8) -> bool {
        (self.0 & flag) != 0
    }
    
    #[inline]
    pub fn is_attached(&self) -> bool {
        self.has(Self::ATTACHED)
    }
    
    #[inline]
    pub fn has_children(&self) -> bool {
        self.has(Self::HAS_CHILDREN)
    }
    
    #[inline]
    pub fn is_dirty(&self) -> bool {
        self.has(Self::CONTENTS_DIRTY | Self::STYLE_DIRTY | Self::LAYOUT_DIRTY)
    }
}

/// Inline small text (24 bytes max, stored in node)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct InlineText {
    /// Length of text (0-23)
    len: u8,
    /// Inline storage
    data: [u8; 23],
}

impl InlineText {
    pub const MAX_LEN: usize = 23;
    
    pub fn new() -> Self {
        Self {
            len: 0,
            data: [0; 23],
        }
    }
    
    pub fn from_str(s: &str) -> Option<Self> {
        if s.len() <= Self::MAX_LEN {
            let mut inline = Self::new();
            inline.len = s.len() as u8;
            inline.data[..s.len()].copy_from_slice(s.as_bytes());
            Some(inline)
        } else {
            None
        }
    }
    
    pub fn as_str(&self) -> &str {
        // Safety: we only store valid UTF-8
        unsafe {
            std::str::from_utf8_unchecked(&self.data[..self.len as usize])
        }
    }
    
    pub fn len(&self) -> usize {
        self.len as usize
    }
    
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Default for InlineText {
    fn default() -> Self {
        Self::new()
    }
}

/// Element name ID (2 bytes, interned)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct ElementNameId(pub u16);

impl ElementNameId {
    // Common HTML elements (pre-registered)
    pub const DIV: Self = Self(1);
    pub const SPAN: Self = Self(2);
    pub const P: Self = Self(3);
    pub const A: Self = Self(4);
    pub const IMG: Self = Self(5);
    pub const BUTTON: Self = Self(6);
    pub const INPUT: Self = Self(7);
    pub const FORM: Self = Self(8);
    pub const UL: Self = Self(9);
    pub const LI: Self = Self(10);
    pub const H1: Self = Self(11);
    pub const H2: Self = Self(12);
    pub const H3: Self = Self(13);
    pub const SCRIPT: Self = Self(14);
    pub const STYLE: Self = Self(15);
    pub const HEAD: Self = Self(16);
    pub const BODY: Self = Self(17);
    pub const HTML: Self = Self(18);
    pub const TABLE: Self = Self(19);
    pub const TR: Self = Self(20);
    pub const TD: Self = Self(21);
    pub const TH: Self = Self(22);
    pub const NAV: Self = Self(23);
    pub const HEADER: Self = Self(24);
    pub const FOOTER: Self = Self(25);
    pub const SECTION: Self = Self(26);
    pub const ARTICLE: Self = Self(27);
    pub const MAIN: Self = Self(28);
    pub const ASIDE: Self = Self(29);
    pub const TEXTAREA: Self = Self(30);
    pub const SELECT: Self = Self(31);
    
    pub const CUSTOM_START: u16 = 1000;
}

/// Compact 32-byte node
/// Layout:
///   0-3:   parent (4 bytes)
///   4-7:   first_child (4 bytes)
///   8-11:  next_sibling (4 bytes)
///   12:    node_type (1 byte)
///   13:    flags (1 byte)
///   14-15: element_name (2 bytes) OR text_overflow_ptr high bits
///   16-39: inline text (24 bytes) OR overflow pointer for long text
#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
pub struct CompactNode {
    /// Parent node ID
    pub parent: CompactNodeId,
    /// First child (linked list)
    pub first_child: CompactNodeId,
    /// Next sibling
    pub next_sibling: CompactNodeId,
    /// Node type
    pub node_type: u8,
    /// Flags
    pub flags: NodeFlags,
    /// Element name ID (for elements) or overflow indicator
    pub element_name: ElementNameId,
    /// Inline content or overflow pointer
    pub content: NodeContent,
}

/// Node content union (16 bytes)
#[derive(Clone, Copy)]
#[repr(C)]
pub union NodeContent {
    /// Inline text for small text nodes (up to 16 bytes here + element_name area)
    pub inline_text: [u8; 16],
    /// Overflow pointer for large text
    pub overflow_ptr: u64,
    /// Element data
    pub element: ElementContent,
}

impl std::fmt::Debug for NodeContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NodeContent")
    }
}

/// Element content (16 bytes)
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ElementContent {
    /// Inline attributes (2 slots: attr_id + value)
    pub attr1_id: u16,
    pub attr1_val: u16,
    pub attr2_id: u16,
    pub attr2_val: u16,
    /// Overflow attribute array index (0 = none)
    pub attr_overflow: u32,
    /// Number of attributes
    pub attr_count: u8,
    /// Reserved
    _reserved: [u8; 3],
}

impl Default for ElementContent {
    fn default() -> Self {
        Self {
            attr1_id: 0,
            attr1_val: 0,
            attr2_id: 0,
            attr2_val: 0,
            attr_overflow: 0,
            attr_count: 0,
            _reserved: [0; 3],
        }
    }
}

impl CompactNode {
    /// Create a new element node
    pub fn element(name: ElementNameId) -> Self {
        Self {
            parent: CompactNodeId::NONE,
            first_child: CompactNodeId::NONE,
            next_sibling: CompactNodeId::NONE,
            node_type: CompactNodeType::Element as u8,
            flags: NodeFlags::new(),
            element_name: name,
            content: NodeContent {
                element: ElementContent::default(),
            },
        }
    }
    
    /// Create a text node with inline text
    pub fn text_inline(text: &str) -> Option<Self> {
        if text.len() > 16 {
            return None;
        }
        
        let mut inline = [0u8; 16];
        inline[..text.len()].copy_from_slice(text.as_bytes());
        
        Some(Self {
            parent: CompactNodeId::NONE,
            first_child: CompactNodeId::NONE,
            next_sibling: CompactNodeId::NONE,
            node_type: CompactNodeType::Text as u8,
            flags: NodeFlags::new(),
            element_name: ElementNameId(text.len() as u16), // Store length
            content: NodeContent { inline_text: inline },
        })
    }
    
    /// Create a text node with overflow pointer
    pub fn text_overflow(overflow_ptr: u64, len: usize) -> Self {
        Self {
            parent: CompactNodeId::NONE,
            first_child: CompactNodeId::NONE,
            next_sibling: CompactNodeId::NONE,
            node_type: CompactNodeType::Text as u8,
            flags: NodeFlags::new(),
            element_name: ElementNameId((len | 0x8000) as u16), // High bit = overflow
            content: NodeContent { overflow_ptr },
        }
    }
    
    /// Check if this is an element
    #[inline]
    pub fn is_element(&self) -> bool {
        self.node_type == CompactNodeType::Element as u8
    }
    
    /// Check if this is a text node
    #[inline]
    pub fn is_text(&self) -> bool {
        self.node_type == CompactNodeType::Text as u8
    }
    
    /// Get inline text (if text node with inline storage)
    pub fn get_inline_text(&self) -> Option<&str> {
        if !self.is_text() {
            return None;
        }
        
        let len = self.element_name.0;
        if len & 0x8000 != 0 {
            return None; // Overflow
        }
        
        let len = len as usize;
        if len > 16 {
            return None;
        }
        
        // Safety: we only store valid UTF-8
        unsafe {
            let bytes = &self.content.inline_text[..len];
            Some(std::str::from_utf8_unchecked(bytes))
        }
    }
}

/// DOM generation ID for O(1) cache validation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DomGeneration(pub u64);

impl DomGeneration {
    pub fn new() -> Self {
        Self(0)
    }
    
    pub fn increment(&mut self) {
        self.0 = self.0.wrapping_add(1);
    }
}

/// SmallVec for children (up to 8 inline)
#[derive(Debug, Clone)]
pub struct SmallChildren {
    inline: [CompactNodeId; 8],
    len: u8,
    overflow: Option<Vec<CompactNodeId>>,
}

impl SmallChildren {
    pub fn new() -> Self {
        Self {
            inline: [CompactNodeId::NONE; 8],
            len: 0,
            overflow: None,
        }
    }
    
    pub fn push(&mut self, id: CompactNodeId) {
        if (self.len as usize) < 8 {
            self.inline[self.len as usize] = id;
            self.len += 1;
        } else {
            if self.overflow.is_none() {
                self.overflow = Some(Vec::new());
            }
            self.overflow.as_mut().unwrap().push(id);
        }
    }
    
    pub fn len(&self) -> usize {
        let base = self.len as usize;
        base + self.overflow.as_ref().map(|v| v.len()).unwrap_or(0)
    }
    
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    
    pub fn iter(&self) -> impl Iterator<Item = CompactNodeId> + '_ {
        let inline_count = self.len as usize;
        self.inline[..inline_count]
            .iter()
            .copied()
            .chain(self.overflow.iter().flat_map(|v| v.iter().copied()))
    }
    
    pub fn get(&self, index: usize) -> Option<CompactNodeId> {
        if index < self.len as usize {
            Some(self.inline[index])
        } else if let Some(ref overflow) = self.overflow {
            overflow.get(index - self.len as usize).copied()
        } else {
            None
        }
    }
}

impl Default for SmallChildren {
    fn default() -> Self {
        Self::new()
    }
}

// Verify sizes at compile time
const _: () = assert!(size_of::<CompactNode>() == 32);
const _: () = assert!(size_of::<CompactNodeId>() == 4);
const _: () = assert!(size_of::<NodeFlags>() == 1);
const _: () = assert!(size_of::<ElementNameId>() == 2);

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_node_size() {
        assert_eq!(size_of::<CompactNode>(), 32);
    }
    
    #[test]
    fn test_element_node() {
        let node = CompactNode::element(ElementNameId::DIV);
        assert!(node.is_element());
        assert!(!node.is_text());
    }
    
    #[test]
    fn test_text_inline() {
        let node = CompactNode::text_inline("Hello").unwrap();
        assert!(node.is_text());
        assert_eq!(node.get_inline_text(), Some("Hello"));
    }
    
    #[test]
    fn test_node_flags() {
        let mut flags = NodeFlags::new();
        assert!(!flags.is_attached());
        
        flags.set(NodeFlags::ATTACHED);
        assert!(flags.is_attached());
        
        flags.clear(NodeFlags::ATTACHED);
        assert!(!flags.is_attached());
    }
    
    #[test]
    fn test_small_children() {
        let mut children = SmallChildren::new();
        
        // Add 8 inline
        for i in 0..8 {
            children.push(CompactNodeId(i));
        }
        assert_eq!(children.len(), 8);
        assert!(children.overflow.is_none());
        
        // Add 9th, triggers overflow
        children.push(CompactNodeId(8));
        assert_eq!(children.len(), 9);
        assert!(children.overflow.is_some());
    }
    
    #[test]
    fn test_dom_generation() {
        let mut generation = DomGeneration::new();
        assert_eq!(generation.0, 0);
        
        generation.increment();
        assert_eq!(generation.0, 1);
    }
}
