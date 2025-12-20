//! Element Query and Methods
//!
//! querySelector, getElementsByClassName, closest, matches.

use crate::NodeId;
use std::collections::HashSet;

/// Element query trait
pub trait ElementQuery {
    /// Query single element by CSS selector
    fn query_selector(&self, root: NodeId, selector: &str) -> Option<NodeId>;
    
    /// Query all elements by CSS selector
    fn query_selector_all(&self, root: NodeId, selector: &str) -> Vec<NodeId>;
    
    /// Get elements by class name
    fn get_elements_by_class_name(&self, root: NodeId, class: &str) -> Vec<NodeId>;
    
    /// Get elements by tag name
    fn get_elements_by_tag_name(&self, root: NodeId, tag: &str) -> Vec<NodeId>;
    
    /// Find closest ancestor matching selector
    fn closest(&self, element: NodeId, selector: &str) -> Option<NodeId>;
    
    /// Check if element matches selector
    fn matches(&self, element: NodeId, selector: &str) -> bool;
}

/// Simple selector for matching
#[derive(Debug, Clone)]
pub enum SimpleSelector {
    Tag(String),
    Class(String),
    Id(String),
    Universal,
}

impl SimpleSelector {
    /// Parse a simple selector string
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        
        if s == "*" {
            Some(Self::Universal)
        } else if let Some(id) = s.strip_prefix('#') {
            Some(Self::Id(id.to_string()))
        } else if let Some(class) = s.strip_prefix('.') {
            Some(Self::Class(class.to_string()))
        } else {
            Some(Self::Tag(s.to_lowercase()))
        }
    }
}

/// Element context for query operations
pub struct ElementContext {
    pub tag_name: String,
    pub id: Option<String>,
    pub classes: HashSet<String>,
}

impl ElementContext {
    pub fn matches(&self, selector: &SimpleSelector) -> bool {
        match selector {
            SimpleSelector::Universal => true,
            SimpleSelector::Tag(tag) => self.tag_name.eq_ignore_ascii_case(tag),
            SimpleSelector::Id(id) => self.id.as_deref() == Some(id),
            SimpleSelector::Class(class) => self.classes.contains(class),
        }
    }
}

/// Live node list (updates with DOM changes)
#[derive(Debug, Clone)]
pub struct NodeList {
    nodes: Vec<NodeId>,
}

impl NodeList {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }
    
    pub fn from_vec(nodes: Vec<NodeId>) -> Self {
        Self { nodes }
    }
    
    pub fn length(&self) -> usize {
        self.nodes.len()
    }
    
    pub fn item(&self, index: usize) -> Option<NodeId> {
        self.nodes.get(index).copied()
    }
    
    pub fn iter(&self) -> impl Iterator<Item = &NodeId> {
        self.nodes.iter()
    }
}

impl Default for NodeList {
    fn default() -> Self {
        Self::new()
    }
}

/// HTML collection (live, elements only)
#[derive(Debug, Clone)]
pub struct HTMLCollection {
    elements: Vec<NodeId>,
}

impl HTMLCollection {
    pub fn new() -> Self {
        Self { elements: Vec::new() }
    }
    
    pub fn from_vec(elements: Vec<NodeId>) -> Self {
        Self { elements }
    }
    
    pub fn length(&self) -> usize {
        self.elements.len()
    }
    
    pub fn item(&self, index: usize) -> Option<NodeId> {
        self.elements.get(index).copied()
    }
    
    pub fn named_item(&self, _name: &str) -> Option<NodeId> {
        // Would look up by name/id
        None
    }
}

impl Default for HTMLCollection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_selector_parse() {
        assert!(matches!(SimpleSelector::parse("div"), Some(SimpleSelector::Tag(_))));
        assert!(matches!(SimpleSelector::parse(".class"), Some(SimpleSelector::Class(_))));
        assert!(matches!(SimpleSelector::parse("#id"), Some(SimpleSelector::Id(_))));
        assert!(matches!(SimpleSelector::parse("*"), Some(SimpleSelector::Universal)));
    }
    
    #[test]
    fn test_element_matches() {
        let ctx = ElementContext {
            tag_name: "div".to_string(),
            id: Some("main".to_string()),
            classes: ["container", "active"].iter().map(|s| s.to_string()).collect(),
        };
        
        assert!(ctx.matches(&SimpleSelector::Tag("div".to_string())));
        assert!(ctx.matches(&SimpleSelector::Id("main".to_string())));
        assert!(ctx.matches(&SimpleSelector::Class("container".to_string())));
        assert!(ctx.matches(&SimpleSelector::Universal));
    }
    
    #[test]
    fn test_node_list() {
        let list = NodeList::from_vec(vec![NodeId(1), NodeId(2), NodeId(3)]);
        
        assert_eq!(list.length(), 3);
        assert_eq!(list.item(0), Some(NodeId(1)));
    }
}
