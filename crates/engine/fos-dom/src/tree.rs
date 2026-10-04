//! DOM Tree - Arena-based allocation
//!
//! All nodes stored in a single Vec for:
//! - Cache-friendly traversal
//! - No individual heap allocations per node
//! - O(1) node lookup by ID
//! - Easy serialization/cloning

use std::sync::atomic::{AtomicU64, Ordering};

use crate::{Attribute, ElementData, Node, NodeId, NodeData, QualName, InternedString, StringInterner, TextData};

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
    /// Custom element names defined by scripts (`customElements.define`)
    custom_defined: std::collections::HashSet<Box<str>>,
}

impl DomTree {
    /// Create a new empty DOM tree
    pub fn new() -> Self {
        let mut tree = Self {
            nodes: Vec::with_capacity(256), // Pre-allocate for typical page
            interner: StringInterner::new(),
            id: NEXT_TREE_ID.fetch_add(1, Ordering::Relaxed),
            mutations: 0,
            custom_defined: Default::default(),
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
            custom_defined: Default::default(),
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

    /// Record that a custom element name has been defined (it then
    /// matches `:defined`)
    pub fn define_custom_element(&mut self, name: &str) {
        if self.custom_defined.insert(name.into()) {
            self.mark_mutated();
        }
    }

    /// Whether custom element `name` has been defined
    pub fn is_custom_element_defined(&self, name: &str) -> bool {
        self.custom_defined.contains(name)
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
    
    /// Create a detached node with the given data
    pub fn create_node(&mut self, data: NodeData) -> NodeId {
        self.mark_mutated();
        let id = NodeId(self.nodes.len() as u32);
        let mut node = Node::document();
        node.data = data;
        self.nodes.push(node);
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
    
    /// Append a child to a parent node (moving it if it already has a parent)
    pub fn append_child(&mut self, parent_id: NodeId, child_id: NodeId) {
        if parent_id == child_id {
            return;
        }
        if self.nodes.get(child_id.index()).is_some_and(|c| c.parent.is_valid()) {
            self.remove(child_id);
        }
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
    
    /// Insert `child_id` into `parent_id` before `reference` (a child of
    /// `parent_id`), or at the end if `reference` is `NodeId::NONE`. The
    /// child is moved if it already has a parent.
    pub fn insert_before(&mut self, parent_id: NodeId, child_id: NodeId, reference: NodeId) {
        if !reference.is_valid() {
            return self.append_child(parent_id, child_id);
        }
        if parent_id == child_id || child_id == reference {
            return;
        }
        if self.nodes.get(child_id.index()).is_some_and(|c| c.parent.is_valid()) {
            self.remove(child_id);
        }
        let Some(prev) = self.nodes.get(reference.index()).map(|r| r.prev_sibling) else { return };
        self.mark_mutated();

        {
            let child = &mut self.nodes[child_id.index()];
            child.parent = parent_id;
            child.prev_sibling = prev;
            child.next_sibling = reference;
        }
        self.nodes[reference.index()].prev_sibling = child_id;
        if prev.is_valid() {
            self.nodes[prev.index()].next_sibling = child_id;
        } else if let Some(parent) = self.nodes.get_mut(parent_id.index()) {
            parent.first_child = child_id;
        }
    }

    /// Rebuild the arena with only the nodes reachable from the root, minus
    /// those `keep` rejects (each dropped with its subtree), stored in
    /// document order. Node IDs change, so this is meant for right after
    /// building a tree (e.g. by the parser), before IDs are handed out.
    ///
    /// Besides freeing unreachable nodes, document order makes tree walks
    /// (style, layout) move through memory sequentially.
    pub fn compact(&mut self, mut keep: impl FnMut(&Node) -> bool) {
        self.mark_mutated();
        let mut old = std::mem::take(&mut self.nodes);
        let mut nodes: Vec<Node> = Vec::with_capacity(old.len());

        let mut root = Node::document();
        root.data = std::mem::replace(&mut old[0].data, NodeData::Document);
        nodes.push(root);

        // Depth-first, pre-order: (old id, new parent id)
        let mut stack: Vec<(NodeId, NodeId)> = Vec::new();
        let mut children: Vec<NodeId> = Vec::new();
        let push_children = |old: &[Node], id: NodeId, new_parent: NodeId, stack: &mut Vec<(NodeId, NodeId)>, children: &mut Vec<NodeId>| {
            children.clear();
            let mut child = old[id.index()].first_child;
            while child.is_valid() {
                children.push(child);
                child = old[child.index()].next_sibling;
            }
            stack.extend(children.iter().rev().map(|&c| (c, new_parent)));
        };
        push_children(&old, NodeId::ROOT, NodeId::ROOT, &mut stack, &mut children);

        while let Some((old_id, parent)) = stack.pop() {
            if !keep(&old[old_id.index()]) {
                continue;
            }
            let id = NodeId(nodes.len() as u32);
            let mut node = Node::document();
            node.data = std::mem::replace(&mut old[old_id.index()].data, NodeData::Document);
            node.parent = parent;

            // Link as the parent's last child
            let last = nodes[parent.index()].last_child;
            node.prev_sibling = last;
            if last.is_valid() {
                nodes[last.index()].next_sibling = id;
            } else {
                nodes[parent.index()].first_child = id;
            }
            nodes[parent.index()].last_child = id;
            nodes.push(node);

            push_children(&old, old_id, id, &mut stack, &mut children);
        }

        nodes.shrink_to_fit();
        self.nodes = nodes;
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
    
    /// The value of attribute `name` of element `node`
    pub fn get_attribute(&self, node: NodeId, name: &str) -> Option<&str> {
        let e = self.get(node)?.as_element()?;
        e.attrs.iter().find(|a| self.resolve(a.name.local) == name).map(|a| a.value.as_str())
    }

    /// Set attribute `name` of element `node`, keeping the id and class
    /// caches the style engine reads in step
    pub fn set_attribute(&mut self, node: NodeId, name: &str, value: &str) {
        let local = self.interner.intern(name);
        let id = (name == "id").then(|| self.interner.intern(value));
        let classes: Option<Vec<InternedString>> =
            (name == "class").then(|| value.split_ascii_whitespace().map(|c| self.interner.intern(c)).collect());
        let Some(e) = self.nodes.get_mut(node.index()).and_then(Node::as_element_mut) else { return };
        if name == "id" {
            e.id = id;
        } else if let Some(c) = classes {
            e.classes = c;
        }
        e.set_attr(QualName::new(InternedString::EMPTY, local), value.to_string());
        self.mark_mutated();
    }

    /// Remove attribute `name` of element `node`; whether it was there
    pub fn remove_attribute(&mut self, node: NodeId, name: &str) -> bool {
        let Some(e) = self.nodes.get(node.index()).and_then(Node::as_element) else { return false };
        let Some(pos) = e.attrs.iter().position(|a| self.interner.get(a.name.local) == name) else { return false };
        let e = self.nodes[node.index()].as_element_mut().unwrap();
        e.attrs.remove(pos);
        if name == "id" {
            e.id = None;
        } else if name == "class" {
            e.classes.clear();
        }
        self.mark_mutated();
        true
    }

    /// The text of `node`'s descendant text nodes, in document order
    pub fn text_content(&self, node: NodeId) -> String {
        let mut out = String::new();
        self.collect_text(node, &mut out);
        out
    }

    fn collect_text(&self, node: NodeId, out: &mut String) {
        let Some(n) = self.get(node) else { return };
        match &n.data {
            NodeData::Text(t) => out.push_str(&t.content),
            NodeData::Comment(_) | NodeData::ProcessingInstruction { .. } => {}
            _ => {
                let mut c = n.first_child;
                while c.is_valid() {
                    self.collect_text(c, out);
                    c = self.nodes[c.index()].next_sibling;
                }
            }
        }
    }

    /// Replace `node`'s children with one text node (or none, for "")
    pub fn set_text_content(&mut self, node: NodeId, text: &str) {
        if let Some(NodeData::Text(t)) = self.get_mut(node).map(|n| &mut n.data) {
            t.content = text.to_string();
            self.mark_mutated();
            return;
        }
        self.remove_children(node);
        if !text.is_empty() {
            let t = self.create_text(text);
            self.append_child(node, t);
        }
        self.mark_mutated();
    }

    /// Detach every child of `node`
    pub fn remove_children(&mut self, node: NodeId) {
        while let Some(c) = self.get(node).map(|n| n.first_child).filter(|c| c.is_valid()) {
            self.remove(c);
        }
    }

    /// Whether `node` is `ancestor` or one of its descendants
    pub fn is_inclusive_descendant(&self, mut node: NodeId, ancestor: NodeId) -> bool {
        while node.is_valid() {
            if node == ancestor {
                return true;
            }
            node = match self.get(node) {
                Some(n) => n.parent,
                None => return false,
            };
        }
        false
    }

    /// Copy node `node` of `src` (with its subtree when `deep`) into this
    /// tree, detached; returns the copy. `src` may be this tree's own
    /// arena contents only through `clone_node`.
    pub fn import_node(&mut self, src: &DomTree, node: NodeId, deep: bool) -> NodeId {
        let Some(n) = src.get(node) else { return NodeId::NONE };
        let data = match &n.data {
            NodeData::Element(e) => {
                let name = QualName::new(self.interner.intern(src.resolve(e.name.ns)), self.interner.intern(src.resolve(e.name.local)));
                let mut copy = ElementData::new(name);
                copy.attrs = e
                    .attrs
                    .iter()
                    .map(|a| Attribute {
                        name: QualName::new(self.interner.intern(src.resolve(a.name.ns)), self.interner.intern(src.resolve(a.name.local))),
                        value: a.value.clone(),
                    })
                    .collect();
                copy.id = e.id.map(|i| self.interner.intern(src.resolve(i)));
                copy.classes = e.classes.iter().map(|&c| self.interner.intern(src.resolve(c))).collect();
                NodeData::Element(copy)
            }
            NodeData::Text(t) => NodeData::Text(TextData { content: t.content.clone() }),
            NodeData::Comment(c) => NodeData::Comment(c.clone()),
            NodeData::Doctype { name, public_id, system_id } => NodeData::Doctype {
                name: self.interner.intern(src.resolve(*name)),
                public_id: public_id.clone(),
                system_id: system_id.clone(),
            },
            NodeData::ProcessingInstruction { target, data } => {
                NodeData::ProcessingInstruction { target: self.interner.intern(src.resolve(*target)), data: data.clone() }
            }
            NodeData::Document => NodeData::Document,
        };
        let copy = self.create_node(data);
        if deep {
            let mut c = n.first_child;
            while c.is_valid() {
                let child = self.import_node(src, c, true);
                self.append_child(copy, child);
                c = src.nodes[c.index()].next_sibling;
            }
        }
        copy
    }

    /// Copy `node` (with its subtree when `deep`) within this tree, detached
    pub fn clone_node(&mut self, node: NodeId, deep: bool) -> NodeId {
        // Interned strings are shared, so a snapshot of the subtree's
        // interner is not needed: copy through a temporary tree
        let mut tmp = DomTree::new();
        let t = tmp.import_node(self, node, deep);
        self.import_node(&tmp, t, deep)
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
    
    fn child_names(tree: &DomTree, parent: NodeId) -> Vec<String> {
        tree.children(parent)
            .map(|(_, node)| match &node.data {
                NodeData::Element(e) => tree.resolve(e.name.local).to_string(),
                NodeData::Text(t) => format!("'{}'", t.content),
                _ => "?".into(),
            })
            .collect()
    }

    #[test]
    fn test_insert_before_and_move() {
        let mut tree = DomTree::new();
        let root = tree.root();
        let a = tree.create_element("a");
        let c = tree.create_element("c");
        tree.append_child(root, a);
        tree.append_child(root, c);

        let b = tree.create_element("b");
        tree.insert_before(root, b, c);
        let first = tree.create_element("first");
        tree.insert_before(root, first, a);
        assert_eq!(child_names(&tree, root), ["first", "a", "b", "c"]);

        // Appending a node that has a parent moves it
        tree.append_child(root, first);
        assert_eq!(child_names(&tree, root), ["a", "b", "c", "first"]);
        tree.append_child(a, b);
        assert_eq!(child_names(&tree, root), ["a", "c", "first"]);
        assert_eq!(child_names(&tree, a), ["b"]);
        assert_eq!(tree.get(b).unwrap().parent, a);
    }

    #[test]
    fn test_compact_keeps_order_and_drops_nodes() {
        let mut tree = DomTree::new();
        let root = tree.root();
        let body = tree.create_element("body");
        let detached = tree.create_element("detached");
        let p = tree.create_element("p");
        let space = tree.create_text("  ");
        let text = tree.create_text("hi");
        let span = tree.create_element("span");
        tree.append_child(root, body);
        tree.append_child(body, p);
        tree.append_child(body, space);
        tree.append_child(body, span);
        tree.append_child(p, text);
        let lost = tree.create_element("lost");
        tree.append_child(detached, lost);
        let removed = tree.create_element("removed");
        tree.append_child(body, removed);
        tree.remove(removed);
        assert_eq!(tree.len(), 9);

        let before = tree.revision();
        tree.compact(|node| !matches!(&node.data, NodeData::Text(t) if t.content.trim().is_empty()));
        assert_ne!(tree.revision(), before);

        // root, body, p, "hi", span: in document order
        assert_eq!(tree.len(), 5);
        let body = NodeId(1);
        assert_eq!(child_names(&tree, tree.root()), ["body"]);
        assert_eq!(child_names(&tree, body), ["p", "span"]);
        assert_eq!(child_names(&tree, NodeId(2)), ["'hi'"]);
        assert_eq!(tree.get(NodeId(4)).unwrap().prev_sibling, NodeId(2));
        assert_eq!(tree.get(NodeId(3)).unwrap().parent, NodeId(2));
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
