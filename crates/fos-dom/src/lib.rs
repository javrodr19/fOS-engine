//! fOS DOM - Document Object Model
//!
//! Memory-efficient DOM tree implementation.

mod node;
mod tree;
mod document;

pub use node::{Node, NodeType, Element, Text};
pub use tree::DomTree;
pub use document::Document;

/// Node identifier (index into arena)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub(crate) u32);

impl NodeId {
    /// Root node ID
    pub const ROOT: NodeId = NodeId(0);
}
