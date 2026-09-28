//! DOM Tree - Arena-based allocation
//!
//! All nodes stored in a single Vec for:
//! - Cache-friendly traversal
//! - No individual heap allocations per node
//! - O(1) node lookup by ID
//! - Easy serialization/cloning

use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Node, NodeId, NodeData, QualName, InternedString, StringInterner};

/// Source of process-unique tree IDs
static NEXT_TREE_ID: AtomicU64 = AtomicU64::new(1);

/// Identifies one state of one DOM tree.
///
/// Two equal revisions mean the same tree with no mutation in between, so
/// anything derived from the tree (styles, layout) is still valid. Trees
/// get process-unique IDs, so revisions of different trees never collide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DomRevision {
    tree: u64,
    mutations: u64,
}

/// Arena-based DOM tree
pub struct DomTree {
    /// All nodes in contiguous memory (pub for TreeSink access). Code that
    /// writes to it directly must call [`DomTree::mark_mutated`].
    pub nodes: Vec<Node>,
    /// String interner for deduplication
    interner: StringInterner,
    /// Process-unique ID of this tree
    id: u64,
    /// Mutations made through the tree's API
    mutations: u64,
}

impl DomTree {
    /// Create a new empty DOM tree
    pub fn new() -> Self {
        let mut tree = Self {
            nodes: Vec::with_capacity(256), // Pre-allocate for typical page
            interner: StringInterner::new(),
            id: NEXT_TREE_ID.fetch_add(1, Ordering::Relaxed),
            mutations: 0,
        };
        
        // Create document root at index 0
        tree.nodes.push(Node::document());
        
        tree
    }
    
    /// Create with capacity hint
    pub fn with_capacity(node_count: usize) -> Self {
        let mut tree = Self {
            nodes: Vec::with_capacity(node_count),
            interner: StringInterner::new(),
            id: NEXT_TREE_ID.fetch_add(1, Ordering::Relaxed),
            mutations: 0,
        };
        tree.nodes.push(Node::document());
        tree
    }
    
    /// The tree's current revision; it changes on every mutation
    #[inline]
    pub fn revision(&self) -> DomRevision {
        DomRevision { tree: self.id, mutations: self.mutations }
    }

    /// Record a mutation made by writing to `nodes` directly
    #[inline]
    pub fn mark_mutated(&mut self) {
        self.mutations += 1;
    }

    /// Get the document root
    #[inline]
    pub fn root(&self) -> NodeId {
        NodeId::ROOT
    }
    
    /// Get a node by ID
    #[inline]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }
    
    /// Get a mutable node by ID (counts as a mutation)
    #[inline]
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.mark_mutated();
        self.nodes.get_mut(id.index())
    }
    
    /// Create a new element node
    pub fn create_element(&mut self, tag: &str) -> NodeId {
        let local = self.interner.intern(tag);
        let ns = InternedString::EMPTY;
        let name = QualName::new(ns, local);
        
        self.mark_mutated();
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node::element(name));
        id
    }
    
    /// Create a new element with namespace
    pub fn create_element_ns(&mut self, ns: &str, tag: &str) -> NodeId {
        let ns = self.interner.intern(ns);
        let local = self.interner.intern(tag);
        let name = QualName::new(ns, local);
        
        self.mark_mutated();
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node::element(name));
        id
    }
    
    /// Create a new text node
    pub fn create_text(&mut self, content: &str) -> NodeId {
        self.mark_mutated();
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node::text(content.to_string()));
        id
    }
    
    /// Create a comment node
    pub fn create_comment(&mut self, content: &str) -> NodeId {
        self.mark_mutated();
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node {
            parent: NodeId::NONE,
            first_child: NodeId::NONE,
            last_child: NodeId::NONE,
            prev_sibling: NodeId::NONE,
            next_sibling: NodeId::NONE,
            data: NodeData::Comment(content.to_string()),
        });
        id
    }
    
    /// Append a child to a parent node
    pub fn append_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        self.mark_mutated();
        // Update child's parent
        if let Some(child) = self.nodes.get_mut(child_id.index()) {
            child.parent = parent_id;
        }
        
        // Get parent's current last child
        let last_child_id = self.nodes.get(parent_id.index())
            .map(|n| n.last_child)
            .unwrap_or(NodeId::NONE);
        
        if last_child_id.is_valid() {
            // Link with previous last child
            if let Some(last_child) = self.nodes.get_mut(last_child_id.index()) {
                last_child.next_sibling = child_id;
            }
            if let Some(child) = self.nodes.get_mut(child_id.index()) {
                child.prev_sibling = last_child_id;
            }
        } else {
            // First child
            if let Some(parent) = self.nodes.get_mut(parent_id.index()) {
                parent.first_child = child_id;
            }
        }
        
        // Update parent's last child
        if let Some(parent) = self.nodes.get_mut(parent_id.index()) {
            parent.last_child = child_id;
        }
    }
    
    /// Remove a node from its parent
    pub fn remove(&mut self, node_id: NodeId) {
        self.mark_mutated();
        let (parent_id, prev_id, next_id) = {
            let node = match self.nodes.get(node_id.index()) {
                Some(n) => n,
                None => return,
            };
            (node.parent, node.prev_sibling, node.next_sibling)
        };
        
        // Update siblings
        if prev_id.is_valid() {
            if let Some(prev) = self.nodes.get_mut(prev_id.index()) {
                prev.next_sibling = next_id;
            }
        } else if parent_id.is_valid() {
            // Was first child
            if let Some(parent) = self.nodes.get_mut(parent_id.index()) {
                parent.first_child = next_id;
            }
        }
        
        if next_id.is_valid() {
            if let Some(next) = self.nodes.get_mut(next_id.index()) {
                next.prev_sibling = prev_id;
            }
        } else if parent_id.is_valid() {
            // Was last child
            if let Some(parent) = self.nodes.get_mut(parent_id.index()) {
                parent.last_child = prev_id;
            }
        }
        
        // Clear node's links (but don't remove from arena - would invalidate IDs)
        if let Some(node) = self.nodes.get_mut(node_id.index()) {
            node.parent = NodeId::NONE;
            node.prev_sibling = NodeId::NONE;
            node.next_sibling = NodeId::NONE;
        }
    }
    
    /// Get string interner
    #[inline]
    pub fn interner(&self) -> &StringInterner {
        &self.interner
    }
    
    /// Get mutable string interner
    #[inline]
    pub fn interner_mut(&mut self) -> &mut StringInterner {
        &mut self.interner
    }
    
    /// Resolve an interned string
    #[inline]
    pub fn resolve(&self, s: InternedString) -> &str {
        self.interner.get(s)
    }
    
    /// Number of nodes
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    
    /// Check if empty (only root)
    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }
    
    /// Iterate over children of a node
    pub fn children(&self, parent_id: NodeId) -> ChildIterator<'_> {
        let first = self.get(parent_id).map(|n| n.first_child).unwrap_or(NodeId::NONE);
        ChildIterator {
            tree: self,
            current: first,
        }
    }
    
    /// Memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.nodes.capacity() * std::mem::size_of::<Node>()
            + self.interner.memory_usage()
    }
}

impl Default for DomTree {
    fn default() -> Self {
        Self::new()
    }
}

/// Iterator over children of a node
pub struct ChildIterator<'a> {
    tree: &'a DomTree,
    current: NodeId,
}

impl<'a> Iterator for ChildIterator<'a> {
    type Item = (NodeId, &'a Node);
    
    fn next(&mut self) -> Option<Self::Item> {
        if !self.current.is_valid() {
            return None;
        }
        
        let node = self.tree.get(self.current)?;
        let id = self.current;
        self.current = node.next_sibling;
        Some((id, node))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_tree() {
        let mut tree = DomTree::new();
        assert_eq!(tree.len(), 1); // Just root
        
        let div = tree.create_element("div");
        let span = tree.create_element("span");
        let text = tree.create_text("Hello");
        
        tree.append_child(tree.root(), div);
        tree.append_child(div, span);
        tree.append_child(span, text);
        
        assert_eq!(tree.len(), 4);
    }
    
    #[test]
    fn test_revision_tracks_mutations() {
        let mut tree = DomTree::new();
        let other = DomTree::new();
        assert_ne!(tree.revision(), other.revision());

        let start = tree.revision();
        assert_eq!(tree.revision(), start);

        let div = tree.create_element("div");
        let after_create = tree.revision();
        assert_ne!(after_create, start);

        tree.append_child(tree.root(), div);
        let after_append = tree.revision();
        assert_ne!(after_append, after_create);

        tree.get_mut(div);
        assert_ne!(tree.revision(), after_append);

        let before_remove = tree.revision();
        tree.remove(div);
        assert_ne!(tree.revision(), before_remove);
    }

    #[test]
    fn test_memory_size() {
        // Verify Node is reasonably sized
        let node_size = std::mem::size_of::<Node>();
        println!("Node size: {} bytes", node_size);
        // Current size is ~216 bytes. Target is <100 bytes.
        // This will be optimized in future iterations by:
        // - Using interned strings for text content
        // - Using a separate arena for large string data
        // - Compacting SmallVec storage
        assert!(node_size < 256, "Node size should be < 256 bytes, got {}", node_size);
    }
}
