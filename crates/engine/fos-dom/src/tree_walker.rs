//! TreeWalker and NodeIterator
//!
//! DOM traversal APIs for navigating the document tree with filtering.
//!
//! TreeWalker provides tree-based navigation (parent, firstChild, siblings)
//! NodeIterator provides sequential iteration through nodes

use crate::NodeId;

/// What types of nodes to show
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhatToShow(u32);

impl WhatToShow {
    pub const ALL: WhatToShow = WhatToShow(0xFFFFFFFF);
    pub const ELEMENT: WhatToShow = WhatToShow(0x1);
    pub const ATTRIBUTE: WhatToShow = WhatToShow(0x2);
    pub const TEXT: WhatToShow = WhatToShow(0x4);
    pub const CDATA_SECTION: WhatToShow = WhatToShow(0x8);
    pub const PROCESSING_INSTRUCTION: WhatToShow = WhatToShow(0x40);
    pub const COMMENT: WhatToShow = WhatToShow(0x80);
    pub const DOCUMENT: WhatToShow = WhatToShow(0x100);
    pub const DOCUMENT_TYPE: WhatToShow = WhatToShow(0x200);
    pub const DOCUMENT_FRAGMENT: WhatToShow = WhatToShow(0x400);

    /// Check if a node type is shown
    pub fn includes(self, node_type: NodeType) -> bool {
        let flag = match node_type {
            NodeType::Element => Self::ELEMENT.0,
            NodeType::Attribute => Self::ATTRIBUTE.0,
            NodeType::Text => Self::TEXT.0,
            NodeType::CDataSection => Self::CDATA_SECTION.0,
            NodeType::ProcessingInstruction => Self::PROCESSING_INSTRUCTION.0,
            NodeType::Comment => Self::COMMENT.0,
            NodeType::Document => Self::DOCUMENT.0,
            NodeType::DocumentType => Self::DOCUMENT_TYPE.0,
            NodeType::DocumentFragment => Self::DOCUMENT_FRAGMENT.0,
        };
        (self.0 & flag) != 0
    }

    /// Combine two WhatToShow filters
    pub fn and(self, other: WhatToShow) -> WhatToShow {
        WhatToShow(self.0 & other.0)
    }

    /// Union of two WhatToShow filters
    pub fn or(self, other: WhatToShow) -> WhatToShow {
        WhatToShow(self.0 | other.0)
    }
}

impl Default for WhatToShow {
    fn default() -> Self {
        Self::ALL
    }
}

/// Node type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Element,
    Attribute,
    Text,
    CDataSection,
    ProcessingInstruction,
    Comment,
    Document,
    DocumentType,
    DocumentFragment,
}

impl NodeType {
    pub fn from_u32(value: u32) -> Option<Self> {
        match value {
            1 => Some(NodeType::Element),
            2 => Some(NodeType::Attribute),
            3 => Some(NodeType::Text),
            4 => Some(NodeType::CDataSection),
            7 => Some(NodeType::ProcessingInstruction),
            8 => Some(NodeType::Comment),
            9 => Some(NodeType::Document),
            10 => Some(NodeType::DocumentType),
            11 => Some(NodeType::DocumentFragment),
            _ => None,
        }
    }

    pub fn to_u32(self) -> u32 {
        match self {
            NodeType::Element => 1,
            NodeType::Attribute => 2,
            NodeType::Text => 3,
            NodeType::CDataSection => 4,
            NodeType::ProcessingInstruction => 7,
            NodeType::Comment => 8,
            NodeType::Document => 9,
            NodeType::DocumentType => 10,
            NodeType::DocumentFragment => 11,
        }
    }
}

/// Node filter result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterResult {
    /// Accept the node
    Accept,
    /// Reject the node (skip this node and its descendants for NodeIterator,
    /// or continue to children for TreeWalker)
    Reject,
    /// Skip this node but process its children
    Skip,
}

/// Node filter trait for custom filtering
pub trait NodeFilter {
    /// Accept or reject a node
    fn accept_node(&self, node: NodeId, node_type: NodeType) -> FilterResult;
}

/// Default filter that accepts all nodes
#[derive(Debug, Clone, Copy, Default)]
pub struct AcceptAllFilter;

impl NodeFilter for AcceptAllFilter {
    fn accept_node(&self, _node: NodeId, _node_type: NodeType) -> FilterResult {
        FilterResult::Accept
    }
}

/// Filter by element tag name
#[derive(Debug, Clone)]
pub struct TagNameFilter {
    pub tag_names: Vec<String>,
    pub case_insensitive: bool,
}

impl TagNameFilter {
    pub fn new(tag_names: Vec<String>) -> Self {
        Self {
            tag_names,
            case_insensitive: true,
        }
    }
}

/// TreeWalker for navigating the DOM tree
pub struct TreeWalker {
    /// The root node of the traversal
    pub root: NodeId,
    /// What types of nodes to show
    pub what_to_show: WhatToShow,
    /// Current node position
    current_node: NodeId,
    /// Custom filter (optional)
    filter: Option<Box<dyn NodeFilter + Send + Sync>>,
}

impl TreeWalker {
    /// Create a new TreeWalker
    pub fn new(root: NodeId, what_to_show: WhatToShow) -> Self {
        Self {
            root,
            what_to_show,
            current_node: root,
            filter: None,
        }
    }

    /// Create with a custom filter
    pub fn with_filter(
        root: NodeId,
        what_to_show: WhatToShow,
        filter: Box<dyn NodeFilter + Send + Sync>,
    ) -> Self {
        Self {
            root,
            what_to_show,
            current_node: root,
            filter: Some(filter),
        }
    }

    /// Get the current node
    pub fn current_node(&self) -> NodeId {
        self.current_node
    }

    /// Set the current node
    pub fn set_current_node(&mut self, node: NodeId) {
        self.current_node = node;
    }

    /// Check if a node is accepted by filters
    fn is_accepted(&self, node: NodeId, node_type: NodeType) -> FilterResult {
        // First check what_to_show
        if !self.what_to_show.includes(node_type) {
            return FilterResult::Skip;
        }

        // Then check custom filter
        if let Some(ref filter) = self.filter {
            filter.accept_node(node, node_type)
        } else {
            FilterResult::Accept
        }
    }
}

/// TreeWalker navigation commands
/// These are meant to be called by the DOM implementation which has access to the tree structure
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeWalkerNav {
    ParentNode,
    FirstChild,
    LastChild,
    PreviousSibling,
    NextSibling,
    PreviousNode,
    NextNode,
}

/// NodeIterator for sequential traversal
pub struct NodeIterator {
    /// The root node of iteration
    pub root: NodeId,
    /// What types of nodes to show
    pub what_to_show: WhatToShow,
    /// Reference node for iteration position
    reference_node: NodeId,
    /// Whether the iterator is positioned before the reference
    pointer_before_reference: bool,
    /// Custom filter
    filter: Option<Box<dyn NodeFilter + Send + Sync>>,
}

impl NodeIterator {
    /// Create a new NodeIterator
    pub fn new(root: NodeId, what_to_show: WhatToShow) -> Self {
        Self {
            root,
            what_to_show,
            reference_node: root,
            pointer_before_reference: true,
            filter: None,
        }
    }

    /// Create with a custom filter
    pub fn with_filter(
        root: NodeId,
        what_to_show: WhatToShow,
        filter: Box<dyn NodeFilter + Send + Sync>,
    ) -> Self {
        Self {
            root,
            what_to_show,
            reference_node: root,
            pointer_before_reference: true,
            filter: Some(filter),
        }
    }

    /// Get reference node
    pub fn reference_node(&self) -> NodeId {
        self.reference_node
    }

    /// Check if pointer is before reference
    pub fn pointer_before_reference(&self) -> bool {
        self.pointer_before_reference
    }

    /// Set reference node
    pub fn set_reference_node(&mut self, node: NodeId, before: bool) {
        self.reference_node = node;
        self.pointer_before_reference = before;
    }

    /// Check if a node is accepted
    fn is_accepted(&self, node: NodeId, node_type: NodeType) -> FilterResult {
        if !self.what_to_show.includes(node_type) {
            return FilterResult::Skip;
        }

        if let Some(ref filter) = self.filter {
            filter.accept_node(node, node_type)
        } else {
            FilterResult::Accept
        }
    }

    /// Detach the iterator (no-op in modern DOM, kept for compatibility)
    pub fn detach(&mut self) {
        // No-op in modern DOM spec
    }
}

/// Document traversal API - to be implemented by Document
pub trait DocumentTraversal {
    /// Create a TreeWalker
    fn create_tree_walker(
        &self,
        root: NodeId,
        what_to_show: WhatToShow,
        filter: Option<Box<dyn NodeFilter + Send + Sync>>,
    ) -> TreeWalker;

    /// Create a NodeIterator
    fn create_node_iterator(
        &self,
        root: NodeId,
        what_to_show: WhatToShow,
        filter: Option<Box<dyn NodeFilter + Send + Sync>>,
    ) -> NodeIterator;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_what_to_show() {
        assert!(WhatToShow::ALL.includes(NodeType::Element));
        assert!(WhatToShow::ALL.includes(NodeType::Text));
        assert!(WhatToShow::ALL.includes(NodeType::Comment));

        assert!(WhatToShow::ELEMENT.includes(NodeType::Element));
        assert!(!WhatToShow::ELEMENT.includes(NodeType::Text));

        let combined = WhatToShow::ELEMENT.or(WhatToShow::TEXT);
        assert!(combined.includes(NodeType::Element));
        assert!(combined.includes(NodeType::Text));
        assert!(!combined.includes(NodeType::Comment));
    }

    #[test]
    fn test_node_type_conversion() {
        assert_eq!(NodeType::Element.to_u32(), 1);
        assert_eq!(NodeType::Text.to_u32(), 3);
        assert_eq!(NodeType::from_u32(1), Some(NodeType::Element));
        assert_eq!(NodeType::from_u32(999), None);
    }

    #[test]
    fn test_tree_walker_creation() {
        let root = NodeId(0);
        let walker = TreeWalker::new(root, WhatToShow::ALL);

        assert_eq!(walker.root, root);
        assert_eq!(walker.current_node(), root);
    }

    #[test]
    fn test_node_iterator_creation() {
        let root = NodeId(0);
        let iter = NodeIterator::new(root, WhatToShow::ELEMENT);

        assert_eq!(iter.root, root);
        assert_eq!(iter.reference_node(), root);
        assert!(iter.pointer_before_reference());
    }

    #[test]
    fn test_default_filter() {
        let filter = AcceptAllFilter;
        assert_eq!(
            filter.accept_node(NodeId(1), NodeType::Element),
            FilterResult::Accept
        );
    }
}
