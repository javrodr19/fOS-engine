//! Accessibility Tree
//!
//! Accessibility tree for screen readers.

use super::aria::{AriaRole, AriaAttributes};

/// Accessibility node
#[derive(Debug, Clone)]
pub struct AccessibilityNode {
    pub id: u64,
    pub role: AriaRole,
    pub name: String,
    pub description: String,
    pub value: Option<String>,
    pub aria: AriaAttributes,
    pub bounds: NodeBounds,
    pub focusable: bool,
    pub focused: bool,
    pub children: Vec<u64>,
    pub parent: Option<u64>,
}

/// Node bounds
#[derive(Debug, Clone, Default)]
pub struct NodeBounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl AccessibilityNode {
    pub fn new(id: u64, role: AriaRole) -> Self {
        Self {
            id,
            role,
            name: String::new(),
            description: String::new(),
            value: None,
            aria: AriaAttributes::default(),
            bounds: NodeBounds::default(),
            focusable: false,
            focused: false,
            children: Vec::new(),
            parent: None,
        }
    }
    
    /// Set accessible name
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
    
    /// Get text alternative (name or description)
    pub fn get_accessible_name(&self) -> &str {
        if !self.name.is_empty() {
            &self.name
        } else if let Some(label) = self.aria.get_label() {
            label
        } else {
            ""
        }
    }
    
    /// Check if interactive
    pub fn is_interactive(&self) -> bool {
        self.role.is_widget() || self.focusable
    }
}

/// Accessibility tree
#[derive(Debug, Default)]
pub struct AccessibilityTree {
    nodes: Vec<AccessibilityNode>,
    root_id: Option<u64>,
    next_id: u64,
}

impl AccessibilityTree {
    pub fn new() -> Self { Self::default() }
    
    /// Create root node
    pub fn create_root(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let node = AccessibilityNode::new(id, AriaRole::Document);
        self.nodes.push(node);
        self.root_id = Some(id);
        id
    }
    
    /// Add node
    pub fn add_node(&mut self, role: AriaRole, parent_id: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let mut node = AccessibilityNode::new(id, role);
        node.parent = parent_id;
        
        // Add to parent's children
        if let Some(pid) = parent_id {
            if let Some(parent) = self.get_node_mut(pid) {
                parent.children.push(id);
            }
        }
        
        self.nodes.push(node);
        id
    }
    
    /// Get node by ID
    pub fn get_node(&self, id: u64) -> Option<&AccessibilityNode> {
        self.nodes.iter().find(|n| n.id == id)
    }
    
    /// Get mutable node
    pub fn get_node_mut(&mut self, id: u64) -> Option<&mut AccessibilityNode> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }
    
    /// Get all focusable nodes
    pub fn get_focusable_nodes(&self) -> Vec<&AccessibilityNode> {
        self.nodes.iter().filter(|n| n.focusable).collect()
    }
    
    /// Get all landmarks
    pub fn get_landmarks(&self) -> Vec<&AccessibilityNode> {
        self.nodes.iter().filter(|n| n.role.is_landmark()).collect()
    }
    
    /// Find node by name
    pub fn find_by_name(&self, name: &str) -> Option<&AccessibilityNode> {
        self.nodes.iter().find(|n| n.name == name)
    }
    
    /// Get tree depth
    pub fn get_depth(&self, id: u64) -> usize {
        let mut depth = 0;
        let mut current = id;
        
        while let Some(node) = self.get_node(current) {
            if let Some(parent) = node.parent {
                depth += 1;
                current = parent;
            } else {
                break;
            }
        }
        
        depth
    }
}

/// Text alternative computation
pub fn compute_text_alternative(node: &AccessibilityNode) -> String {
    // 1. aria-labelledby
    // 2. aria-label
    // 3. Native text (depends on element)
    // 4. title attribute
    
    if let Some(label) = node.aria.get_label() {
        return label.to_string();
    }
    
    if !node.name.is_empty() {
        return node.name.clone();
    }
    
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_accessibility_tree() {
        let mut tree = AccessibilityTree::new();
        let root = tree.create_root();
        
        let button = tree.add_node(AriaRole::Button, Some(root));
        if let Some(node) = tree.get_node_mut(button) {
            node.set_name("Submit");
            node.focusable = true;
        }
        
        assert_eq!(tree.get_focusable_nodes().len(), 1);
        assert_eq!(tree.get_node(button).unwrap().get_accessible_name(), "Submit");
    }
}
