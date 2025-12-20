//! Structural Sharing / Persistent Data Structures (Phase 24.2)
//!
//! Immutable DOM with path copying. Share unchanged subtrees between
//! versions. Enables undo/redo for free. Like Clojure's persistent vectors.

use std::sync::Arc;
use std::collections::HashMap;

/// Version ID for tracking DOM versions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Version(pub u64);

impl Version {
    pub const INITIAL: Self = Version(0);
    
    pub fn next(self) -> Self {
        Version(self.0 + 1)
    }
}

/// Persistent node - immutable and shareable
#[derive(Debug, Clone)]
pub struct PersistentNode {
    /// Unique ID within a version
    pub id: u32,
    /// Tag name (interned)
    pub tag: u32,
    /// Attributes (shared)
    pub attrs: Arc<[(u32, Arc<str>)]>,
    /// Children (shared)
    pub children: Arc<[Arc<PersistentNode>]>,
    /// Text content (if text node)
    pub text: Option<Arc<str>>,
}

impl PersistentNode {
    /// Create a new element node
    pub fn element(id: u32, tag: u32) -> Self {
        Self {
            id,
            tag,
            attrs: Arc::new([]),
            children: Arc::new([]),
            text: None,
        }
    }
    
    /// Create a new text node
    pub fn text(id: u32, content: &str) -> Self {
        Self {
            id,
            tag: 0, // Text nodes have tag 0
            attrs: Arc::new([]),
            children: Arc::new([]),
            text: Some(Arc::from(content)),
        }
    }
    
    /// Create a modified copy with new attributes
    pub fn with_attrs(self: &Arc<Self>, attrs: Vec<(u32, Arc<str>)>) -> Arc<Self> {
        Arc::new(PersistentNode {
            id: self.id,
            tag: self.tag,
            attrs: Arc::from(attrs),
            children: Arc::clone(&self.children),
            text: self.text.clone(),
        })
    }
    
    /// Create a modified copy with new children
    pub fn with_children(self: &Arc<Self>, children: Vec<Arc<PersistentNode>>) -> Arc<Self> {
        Arc::new(PersistentNode {
            id: self.id,
            tag: self.tag,
            attrs: Arc::clone(&self.attrs),
            children: Arc::from(children),
            text: self.text.clone(),
        })
    }
    
    /// Create a modified copy with updated child at index
    pub fn with_child_at(self: &Arc<Self>, index: usize, child: Arc<PersistentNode>) -> Arc<Self> {
        let mut new_children: Vec<_> = self.children.iter().cloned().collect();
        if index < new_children.len() {
            new_children[index] = child;
        }
        self.with_children(new_children)
    }
    
    /// Create a modified copy with child appended
    pub fn with_child_appended(self: &Arc<Self>, child: Arc<PersistentNode>) -> Arc<Self> {
        let mut new_children: Vec<_> = self.children.iter().cloned().collect();
        new_children.push(child);
        self.with_children(new_children)
    }
    
    /// Create a modified copy with child removed
    pub fn with_child_removed(self: &Arc<Self>, index: usize) -> Arc<Self> {
        let new_children: Vec<_> = self.children.iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, c)| c.clone())
            .collect();
        self.with_children(new_children)
    }
    
    /// Count total nodes in subtree
    pub fn node_count(&self) -> usize {
        1 + self.children.iter().map(|c| c.node_count()).sum::<usize>()
    }
    
    /// Estimate memory usage (shared references counted once)
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.attrs.len() * (std::mem::size_of::<u32>() + std::mem::size_of::<Arc<str>>())
            + self.children.len() * std::mem::size_of::<Arc<PersistentNode>>()
            + self.text.as_ref().map(|t| t.len()).unwrap_or(0)
    }
}

/// Persistent DOM tree with version history
#[derive(Debug)]
pub struct PersistentDom {
    /// Current root
    root: Arc<PersistentNode>,
    /// Current version
    version: Version,
    /// Version history (for undo)
    history: Vec<Arc<PersistentNode>>,
    /// Maximum history size
    max_history: usize,
    /// Next node ID
    next_id: u32,
}

impl PersistentDom {
    /// Create a new persistent DOM
    pub fn new(root: PersistentNode) -> Self {
        let root = Arc::new(root);
        Self {
            root: Arc::clone(&root),
            version: Version::INITIAL,
            history: vec![root],
            max_history: 100,
            next_id: 1,
        }
    }
    
    /// Get current root
    pub fn root(&self) -> &Arc<PersistentNode> {
        &self.root
    }
    
    /// Get current version
    pub fn version(&self) -> Version {
        self.version
    }
    
    /// Allocate a new node ID
    pub fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    
    /// Apply a modification and create a new version
    pub fn modify<F>(&mut self, f: F) -> Version
    where
        F: FnOnce(&Arc<PersistentNode>) -> Arc<PersistentNode>,
    {
        let new_root = f(&self.root);
        
        // Advance version
        self.version = self.version.next();
        self.root = new_root;
        
        // Save to history
        if self.history.len() >= self.max_history {
            self.history.remove(0);
        }
        self.history.push(Arc::clone(&self.root));
        
        self.version
    }
    
    /// Undo to previous version
    pub fn undo(&mut self) -> Option<Version> {
        if self.history.len() > 1 {
            self.history.pop();
            self.root = Arc::clone(self.history.last().unwrap());
            self.version = self.version.next();
            Some(self.version)
        } else {
            None
        }
    }
    
    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.history.len() > 1
    }
    
    /// Get history depth
    pub fn history_depth(&self) -> usize {
        self.history.len()
    }
    
    /// Statistics
    pub fn stats(&self) -> PersistentDomStats {
        PersistentDomStats {
            version: self.version,
            history_depth: self.history.len(),
            node_count: self.root.node_count(),
            memory_estimate: self.root.memory_size(),
        }
    }
}

/// Statistics for persistent DOM
#[derive(Debug, Clone, Copy)]
pub struct PersistentDomStats {
    pub version: Version,
    pub history_depth: usize,
    pub node_count: usize,
    pub memory_estimate: usize,
}

/// Path in the DOM tree for targeted modifications
#[derive(Debug, Clone)]
pub struct NodePath {
    /// Indices from root to target
    indices: Vec<usize>,
}

impl NodePath {
    pub fn new() -> Self {
        Self { indices: Vec::new() }
    }
    
    pub fn push(&mut self, index: usize) {
        self.indices.push(index);
    }
    
    pub fn pop(&mut self) -> Option<usize> {
        self.indices.pop()
    }
    
    /// Navigate to the node at this path
    pub fn navigate<'a>(&self, root: &'a Arc<PersistentNode>) -> Option<&'a Arc<PersistentNode>> {
        let mut current = root;
        for &idx in &self.indices {
            current = current.children.get(idx)?;
        }
        Some(current)
    }
    
    /// Modify the node at this path (path copying)
    pub fn modify<F>(&self, root: &Arc<PersistentNode>, f: F) -> Arc<PersistentNode>
    where
        F: FnOnce(&Arc<PersistentNode>) -> Arc<PersistentNode>,
    {
        self.modify_at(root, &self.indices, f)
    }
    
    fn modify_at<F>(&self, node: &Arc<PersistentNode>, path: &[usize], f: F) -> Arc<PersistentNode>
    where
        F: FnOnce(&Arc<PersistentNode>) -> Arc<PersistentNode>,
    {
        if path.is_empty() {
            // We've reached the target - apply modification
            f(node)
        } else {
            // Recurse down, path copy on the way up
            let idx = path[0];
            if let Some(child) = node.children.get(idx) {
                let new_child = self.modify_at(child, &path[1..], f);
                node.with_child_at(idx, new_child)
            } else {
                Arc::clone(node)
            }
        }
    }
}

impl Default for NodePath {
    fn default() -> Self {
        Self::new()
    }
}

/// Diff between two DOM versions
#[derive(Debug, Clone)]
pub enum DomDiff {
    /// No change
    None,
    /// Node replaced
    Replace(Arc<PersistentNode>),
    /// Attributes changed
    AttrsChanged(Arc<[(u32, Arc<str>)]>),
    /// Children changed
    ChildrenChanged(Vec<ChildDiff>),
    /// Text changed
    TextChanged(Arc<str>),
}

/// Diff for a single child
#[derive(Debug, Clone)]
pub enum ChildDiff {
    /// Child unchanged
    Same,
    /// Child modified
    Modified(Box<DomDiff>),
    /// Child inserted
    Inserted(Arc<PersistentNode>),
    /// Child removed
    Removed,
}

/// Compute diff between two nodes
pub fn diff_nodes(old: &Arc<PersistentNode>, new: &Arc<PersistentNode>) -> DomDiff {
    // Same reference = no change (structural sharing)
    if Arc::ptr_eq(old, new) {
        return DomDiff::None;
    }
    
    // Different tags = full replace
    if old.tag != new.tag {
        return DomDiff::Replace(Arc::clone(new));
    }
    
    // Check text content
    if old.text != new.text {
        if let Some(ref text) = new.text {
            return DomDiff::TextChanged(Arc::clone(text));
        }
    }
    
    // Check attributes
    if !Arc::ptr_eq(&old.attrs, &new.attrs) {
        // Attributes have changed
        return DomDiff::AttrsChanged(Arc::clone(&new.attrs));
    }
    
    // Check children
    if !Arc::ptr_eq(&old.children, &new.children) {
        let child_diffs = diff_children(&old.children, &new.children);
        return DomDiff::ChildrenChanged(child_diffs);
    }
    
    DomDiff::None
}

/// Compute diff for children
fn diff_children(old: &[Arc<PersistentNode>], new: &[Arc<PersistentNode>]) -> Vec<ChildDiff> {
    let max_len = old.len().max(new.len());
    let mut diffs = Vec::with_capacity(max_len);
    
    for i in 0..max_len {
        match (old.get(i), new.get(i)) {
            (Some(o), Some(n)) => {
                if Arc::ptr_eq(o, n) {
                    diffs.push(ChildDiff::Same);
                } else {
                    diffs.push(ChildDiff::Modified(Box::new(diff_nodes(o, n))));
                }
            }
            (None, Some(n)) => {
                diffs.push(ChildDiff::Inserted(Arc::clone(n)));
            }
            (Some(_), None) => {
                diffs.push(ChildDiff::Removed);
            }
            (None, None) => unreachable!(),
        }
    }
    
    diffs
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_persistent_node() {
        let node = Arc::new(PersistentNode::element(1, 10));
        
        // Add children
        let child1 = Arc::new(PersistentNode::text(2, "Hello"));
        let child2 = Arc::new(PersistentNode::text(3, "World"));
        
        let with_children = node.with_children(vec![child1.clone(), child2.clone()]);
        
        assert_eq!(with_children.children.len(), 2);
        assert_eq!(node.children.len(), 0); // Original unchanged
    }
    
    #[test]
    fn test_structural_sharing() {
        let child1 = Arc::new(PersistentNode::text(1, "Hello"));
        let child2 = Arc::new(PersistentNode::text(2, "World"));
        
        let node1 = Arc::new(PersistentNode::element(0, 10))
            .with_children(vec![child1.clone(), child2.clone()]);
        
        // Modify only first child
        let new_child1 = Arc::new(PersistentNode::text(1, "Hi"));
        let node2 = node1.with_child_at(0, new_child1);
        
        // Second child is shared
        assert!(Arc::ptr_eq(&node1.children[1], &node2.children[1]));
    }
    
    #[test]
    fn test_persistent_dom_undo() {
        let root = PersistentNode::element(0, 1);
        let mut dom = PersistentDom::new(root);
        
        assert_eq!(dom.history_depth(), 1);
        
        // Make a modification
        let child = Arc::new(PersistentNode::text(1, "Hello"));
        dom.modify(|r| r.with_child_appended(child.clone()));
        
        assert_eq!(dom.history_depth(), 2);
        assert_eq!(dom.root().children.len(), 1);
        
        // Undo
        dom.undo();
        assert_eq!(dom.root().children.len(), 0);
    }
    
    #[test]
    fn test_node_path() {
        let leaf = Arc::new(PersistentNode::text(3, "Leaf"));
        let child = Arc::new(PersistentNode::element(2, 20))
            .with_children(vec![leaf]);
        let root = Arc::new(PersistentNode::element(1, 10))
            .with_children(vec![child]);
        
        let mut path = NodePath::new();
        path.push(0);
        path.push(0);
        
        let found = path.navigate(&root);
        assert!(found.is_some());
        assert_eq!(found.unwrap().text.as_ref().map(|t| t.as_ref()), Some("Leaf"));
    }
    
    #[test]
    fn test_diff_nodes() {
        let node1 = Arc::new(PersistentNode::text(1, "Hello"));
        let node2 = Arc::new(PersistentNode::text(1, "World"));
        
        // Same reference = no diff
        assert!(matches!(diff_nodes(&node1, &node1), DomDiff::None));
        
        // Different = text changed
        assert!(matches!(diff_nodes(&node1, &node2), DomDiff::TextChanged(_)));
    }
}
