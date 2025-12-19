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
    
    /// Memory usage in bytes
    pub fn memory_usage(&self) -> usize {
        self.tree.memory_usage() + self.url.capacity()
    }
}

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
