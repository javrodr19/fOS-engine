//! fOS DOM - Memory-Efficient Document Object Model
//!
//! Design principles for minimal RAM:
//! 1. Arena-based allocation - all nodes in contiguous memory
//! 2. String interning - deduplicate tag names, attribute names
//! 3. Compact node IDs - u32 indices instead of pointers
//! 4. Inline small strings - avoid heap for short text
//! 5. Flat attribute storage - avoid Vec overhead for common cases

mod node;
mod tree;
mod document;
mod interner;

pub use node::{Node, NodeData, ElementData, TextData};
pub use tree::DomTree;
pub use document::Document;
pub use interner::{StringInterner, InternedString};

/// Node identifier - 4 bytes (vs 8 bytes for pointer on 64-bit)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct NodeId(pub u32);

impl NodeId {
    /// Invalid/null node ID
    pub const NONE: NodeId = NodeId(u32::MAX);
    
    /// Root node ID (always 0)
    pub const ROOT: NodeId = NodeId(0);
    
    /// Check if this is a valid node ID
    #[inline]
    pub fn is_valid(self) -> bool {
        self != Self::NONE
    }
    
    /// Get the raw index
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// Attribute ID - index into attribute storage
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct AttrId(pub(crate) u32);

/// Qualified name (namespace + local name)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QualName {
    pub ns: InternedString,
    pub local: InternedString,
}

impl QualName {
    pub fn new(ns: InternedString, local: InternedString) -> Self {
        Self { ns, local }
    }
}
