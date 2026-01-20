//! Document - High-level document API

use crate::{DomTree, NodeId, InternedString};

/// HTML Document
pub struct Document {
    /// The DOM tree
    pub tree: DomTree,
    /// Document URL
    url: String,
    /// Cached reference to <html> element
    html_element: NodeId,
    /// Cached reference to <head> element
    head_element: NodeId,
    /// Cached reference to <body> element
    body_element: NodeId,
}

impl Document {
    /// Create a new empty document
    pub fn new(url: &str) -> Self {
        let mut tree = DomTree::new();
        
        // Create basic document structure
        let html = tree.create_element("html");
        let head = tree.create_element("head");
        let body = tree.create_element("body");
        
        tree.append_child(tree.root(), html);
        tree.append_child(html, head);
        tree.append_child(html, body);
        
        Self {
            tree,
            url: url.to_string(),
            html_element: html,
            head_element: head,
            body_element: body,
        }
    }
    
    /// Create an empty document (no structure)
    pub fn empty(url: &str) -> Self {
        Self {
            tree: DomTree::new(),
            url: url.to_string(),
            html_element: NodeId::NONE,
            head_element: NodeId::NONE,
            body_element: NodeId::NONE,
        }
    }
    
    /// Finalize the document after parsing - finds html, head, body elements
    pub fn finalize(&mut self) {
        // Find <html> element (first child of root that is an element)
        for (id, node) in self.tree.children(self.tree.root()) {
            if let Some(elem) = node.as_element() {
                let tag = self.tree.resolve(elem.name.local);
                if tag.eq_ignore_ascii_case("html") {
                    self.html_element = id;
                    break;
                }
            }
        }
        
        // Find <head> and <body> within <html>
        if self.html_element.is_valid() {
            for (id, node) in self.tree.children(self.html_element) {
                if let Some(elem) = node.as_element() {
                    let tag = self.tree.resolve(elem.name.local);
                    if tag.eq_ignore_ascii_case("head") {
                        self.head_element = id;
                    } else if tag.eq_ignore_ascii_case("body") {
                        self.body_element = id;
                    }
                }
            }
        }
    }
    
    /// Get document URL
    pub fn url(&self) -> &str {
        &self.url
    }
    
    /// Get document title
    pub fn title(&self) -> String {
        // Find <title> in <head>
        if !self.head_element.is_valid() {
            return String::new();
        }
        
        for (id, node) in self.tree.children(self.head_element) {
            if let Some(elem) = node.as_element() {
                let tag = self.tree.resolve(elem.name.local);
                if tag == "title" {
                    // Get text content of title
                    for (_, child) in self.tree.children(id) {
                        if let Some(text) = child.as_text() {
                            return text.to_string();
                        }
                    }
                }
            }
        }
        
        String::new()
    }
    
    /// Get <html> element
    pub fn document_element(&self) -> NodeId {
        self.html_element
    }
    
    /// Get <head> element
    pub fn head(&self) -> NodeId {
        self.head_element
    }
    
    /// Get <body> element
    pub fn body(&self) -> NodeId {
        self.body_element
    }
    
    /// Get element by ID
    pub fn get_element_by_id(&self, id: &str) -> Option<NodeId> {
        let id_interned = self.tree.interner().intern_lookup(id)?;
        self.find_element_with_id(self.tree.root(), id_interned)
    }
    
    fn find_element_with_id(&self, start: NodeId, target_id: InternedString) -> Option<NodeId> {
        for (node_id, node) in self.tree.children(start) {
            if let Some(elem) = node.as_element() {
                if elem.id == Some(target_id) {
                    return Some(node_id);
                }
            }
            // Recurse into children
            if let Some(found) = self.find_element_with_id(node_id, target_id) {
                return Some(found);
            }
        }
        None
    }
    
    /// Access the DOM tree
    pub fn tree(&self) -> &DomTree {
        &self.tree
    }
    
    /// Access the DOM tree mutably
    pub fn tree_mut(&mut self) -> &mut DomTree {
        &mut self.tree
    }
    
    /// Adopt a node from another document
    /// This changes the node's owner document without cloning
    pub fn adopt_node(&mut self, node_id: NodeId) -> Result<NodeId, DocumentError> {
        // In a real implementation, this would:
        // 1. Remove node from its current parent in the source document
        // 2. Update the node's ownerDocument to this document
        // 3. Return the adopted node
        
        // For now, we just verify the node exists in our tree
        if self.tree.get(node_id).is_some() {
            Ok(node_id)
        } else {
            Err(DocumentError::NodeNotFound)
        }
    }
    
    /// Import a node from another document (deep clone)
    pub fn import_node(&mut self, source_tree: &DomTree, node_id: NodeId, deep: bool) -> Result<NodeId, DocumentError> {
        let source = source_tree.get(node_id)
            .ok_or(DocumentError::NodeNotFound)?;
        
        // Clone the node structure
        let new_node = if source.is_element() {
            let elem = source.as_element().unwrap();
            let tag = source_tree.resolve(elem.name.local);
            let imported = self.tree.create_element(tag);
            
            // Copy attributes
            // Note: In a full impl, we'd copy all attributes here
            
            if deep {
                // Recursively import children
                self.import_children(source_tree, node_id, imported)?;
            }
            
            imported
        } else if let Some(text) = source.as_text() {
            self.tree.create_text(text)
        } else {
            return Err(DocumentError::InvalidNodeType);
        };
        
        Ok(new_node)
    }
    
    fn import_children(&mut self, source_tree: &DomTree, source_parent: NodeId, dest_parent: NodeId) -> Result<(), DocumentError> {
        for (child_id, _) in source_tree.children(source_parent) {
            let imported = self.import_node(source_tree, child_id, true)?;
            self.tree.append_child(dest_parent, imported);
        }
        Ok(())
    }
    
    /// Compare document position of two nodes
    /// Returns a bitmask per DOM spec:
    /// - 1: DISCONNECTED
    /// - 2: PRECEDING
    /// - 4: FOLLOWING
    /// - 8: CONTAINS
    /// - 16: CONTAINED_BY
    pub fn compare_document_position(&self, node_a: NodeId, node_b: NodeId) -> u8 {
        const DISCONNECTED: u8 = 1;
        const PRECEDING: u8 = 2;
        const FOLLOWING: u8 = 4;
        const CONTAINS: u8 = 8;
        const CONTAINED_BY: u8 = 16;
        
        if node_a == node_b {
            return 0; // Same node
        }
        
        // Check if nodes exist
        if self.tree.get(node_a).is_none() || self.tree.get(node_b).is_none() {
            return DISCONNECTED;
        }
        
        // Check if A contains B
        if self.is_ancestor_of(node_a, node_b) {
            return CONTAINS | PRECEDING;
        }
        
        // Check if B contains A
        if self.is_ancestor_of(node_b, node_a) {
            return CONTAINED_BY | FOLLOWING;
        }
        
        // Check document order by walking the tree
        let mut found_a = false;
        let mut found_b = false;
        
        self.walk_tree(self.tree.root(), &mut |id| {
            if id == node_a {
                found_a = true;
            }
            if id == node_b {
                found_b = true;
            }
            !found_a || !found_b
        });
        
        if found_a && !found_b {
            PRECEDING
        } else if found_b && !found_a {
            FOLLOWING
        } else {
            PRECEDING // A comes before B in document order
        }
    }
    
    fn is_ancestor_of(&self, ancestor: NodeId, descendant: NodeId) -> bool {
        let mut current = descendant;
        while let Some(node) = self.tree.get(current) {
            if node.parent == ancestor {
                return true;
            }
            if !node.parent.is_valid() {
                break;
            }
            current = node.parent;
        }
        false
    }
    
    fn walk_tree(&self, start: NodeId, callback: &mut impl FnMut(NodeId) -> bool) {
        if !callback(start) {
            return;
        }
        for (child_id, _) in self.tree.children(start) {
            self.walk_tree(child_id, callback);
        }
    }
    
    /// Normalize the document (merge adjacent text nodes)
    /// Note: This is a simplified implementation that removes empty text nodes
    /// and recursively normalizes children. Full text merging would require
    /// additional DOM tree mutation capabilities.
    pub fn normalize(&mut self, node_id: NodeId) {
        // Collect children to avoid borrow issues
        let children: Vec<NodeId> = self.tree.children(node_id)
            .map(|(id, _)| id)
            .collect();
        
        // Track empty text nodes to remove
        let mut to_remove: Vec<NodeId> = Vec::new();
        
        for child_id in &children {
            if let Some(node) = self.tree.get(*child_id) {
                if node.is_text() {
                    // Check for empty text
                    if let Some(text) = node.as_text() {
                        if text.trim().is_empty() {
                            to_remove.push(*child_id);
                        }
                    }
                } else if node.is_element() {
                    // Recursively normalize child elements
                    self.normalize(*child_id);
                }
            }
        }
        
        // Remove empty text nodes
        for id in to_remove {
            self.tree.remove(id);
        }
    }
    
    /// Memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.tree.memory_usage() + self.url.capacity()
    }
}

/// Document operation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentError {
    NodeNotFound,
    InvalidNodeType,
    WrongDocument,
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NodeNotFound => write!(f, "Node not found"),
            Self::InvalidNodeType => write!(f, "Invalid node type for operation"),
            Self::WrongDocument => write!(f, "Node belongs to a different document"),
        }
    }
}

impl std::error::Error for DocumentError {}

impl Default for Document {
    fn default() -> Self {
        Self::new("about:blank")
    }
}

// Add lookup method to StringInterner
impl crate::StringInterner {
    /// Look up a string without interning it
    pub fn intern_lookup(&self, s: &str) -> Option<InternedString> {
        // Use the map to look up without interning
        self.map.get(s).map(|&idx| InternedString(idx))
    }
}
