//! DOM Node Operations
//!
//! Core node manipulation: appendChild, removeChild, insertBefore, cloneNode.

use crate::NodeId;

/// Result type for DOM operations
pub type DomResult<T> = Result<T, DomError>;

/// DOM operation errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomError {
    /// Node not found
    NotFound,
    /// Hierarchy error (e.g., inserting ancestor)
    HierarchyRequest,
    /// Wrong document
    WrongDocument,
    /// Invalid node type
    InvalidNodeType,
    /// Node is not a child
    NotAChild,
}

impl std::fmt::Display for DomError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "Node not found"),
            Self::HierarchyRequest => write!(f, "Hierarchy request error"),
            Self::WrongDocument => write!(f, "Wrong document"),
            Self::InvalidNodeType => write!(f, "Invalid node type"),
            Self::NotAChild => write!(f, "Node is not a child"),
        }
    }
}

impl std::error::Error for DomError {}

/// Node operations trait
pub trait NodeOperations {
    /// Append a child node
    fn append_child(&mut self, parent: NodeId, child: NodeId) -> DomResult<NodeId>;
    
    /// Remove a child node
    fn remove_child(&mut self, parent: NodeId, child: NodeId) -> DomResult<NodeId>;
    
    /// Insert before a reference node
    fn insert_before(&mut self, parent: NodeId, new_child: NodeId, ref_child: Option<NodeId>) -> DomResult<NodeId>;
    
    /// Replace a child with another node
    fn replace_child(&mut self, parent: NodeId, new_child: NodeId, old_child: NodeId) -> DomResult<NodeId>;
    
    /// Clone a node
    fn clone_node(&self, node: NodeId, deep: bool) -> DomResult<NodeId>;
    
    /// Normalize text nodes (merge adjacent)
    fn normalize(&mut self, node: NodeId) -> DomResult<()>;
}

/// Document fragment - lightweight container
#[derive(Debug, Clone, Default)]
pub struct DocumentFragment {
    pub children: Vec<NodeId>,
}

impl DocumentFragment {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn append(&mut self, node: NodeId) {
        self.children.push(node);
    }
    
    pub fn prepend(&mut self, node: NodeId) {
        self.children.insert(0, node);
    }
    
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }
    
    pub fn len(&self) -> usize {
        self.children.len()
    }
    
    /// Take all children (clears fragment)
    pub fn take_children(&mut self) -> Vec<NodeId> {
        std::mem::take(&mut self.children)
    }
}

/// Node relationship helper
#[derive(Debug, Clone, Default)]
pub struct NodeRelation {
    pub parent: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub last_child: Option<NodeId>,
    pub prev_sibling: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
}

impl NodeRelation {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn has_children(&self) -> bool {
        self.first_child.is_some()
    }
    
    pub fn has_parent(&self) -> bool {
        self.parent.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_document_fragment() {
        let mut frag = DocumentFragment::new();
        frag.append(NodeId(1));
        frag.append(NodeId(2));
        
        assert_eq!(frag.len(), 2);
        assert!(!frag.is_empty());
        
        let children = frag.take_children();
        assert_eq!(children.len(), 2);
        assert!(frag.is_empty());
    }
    
    #[test]
    fn test_node_relation() {
        let rel = NodeRelation {
            parent: Some(NodeId(0)),
            first_child: Some(NodeId(2)),
            ..Default::default()
        };
        
        assert!(rel.has_parent());
        assert!(rel.has_children());
    }
}
