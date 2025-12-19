//! DOM Tree (arena-based allocation)

use crate::{Node, NodeId};

/// Arena-based DOM tree for memory efficiency
#[derive(Debug, Default)]
pub struct DomTree {
    nodes: Vec<Node>,
}

impl DomTree {
    /// Create a new empty DOM tree
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }
    
    /// Get a node by ID
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.0 as usize)
    }
    
    /// Get a mutable node by ID
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id.0 as usize)
    }
    
    /// Number of nodes in the tree
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    
    /// Check if tree is empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
