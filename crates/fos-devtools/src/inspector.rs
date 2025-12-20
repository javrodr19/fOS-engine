//! Element Inspector
//!
//! DOM tree and style inspection.

use std::collections::HashMap;

/// Inspected node
#[derive(Debug, Clone)]
pub struct InspectedNode {
    pub id: u64,
    pub node_type: NodeType,
    pub tag_name: String,
    pub id_attr: Option<String>,
    pub class_list: Vec<String>,
    pub attributes: HashMap<String, String>,
    pub text_content: Option<String>,
    pub children: Vec<u64>,
    pub parent: Option<u64>,
    pub computed_styles: HashMap<String, String>,
}

/// Node type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Element,
    Text,
    Comment,
    Document,
    DocumentType,
}

impl InspectedNode {
    pub fn element(id: u64, tag: &str) -> Self {
        Self {
            id,
            node_type: NodeType::Element,
            tag_name: tag.to_string(),
            id_attr: None,
            class_list: Vec::new(),
            attributes: HashMap::new(),
            text_content: None,
            children: Vec::new(),
            parent: None,
            computed_styles: HashMap::new(),
        }
    }
    
    pub fn text(id: u64, content: &str) -> Self {
        Self {
            id,
            node_type: NodeType::Text,
            tag_name: "#text".to_string(),
            id_attr: None,
            class_list: Vec::new(),
            attributes: HashMap::new(),
            text_content: Some(content.to_string()),
            children: Vec::new(),
            parent: None,
            computed_styles: HashMap::new(),
        }
    }
    
    /// Get selector for this node
    pub fn get_selector(&self) -> String {
        let mut selector = self.tag_name.clone();
        
        if let Some(id) = &self.id_attr {
            selector.push('#');
            selector.push_str(id);
        }
        
        for class in &self.class_list {
            selector.push('.');
            selector.push_str(class);
        }
        
        selector
    }
}

/// Style rule
#[derive(Debug, Clone)]
pub struct InspectedStyleRule {
    pub selector: String,
    pub source: StyleSource,
    pub properties: Vec<StyleProperty>,
}

/// Style source
#[derive(Debug, Clone)]
pub struct StyleSource {
    pub url: Option<String>,
    pub line: u32,
    pub column: u32,
    pub is_inline: bool,
}

/// Style property
#[derive(Debug, Clone)]
pub struct StyleProperty {
    pub name: String,
    pub value: String,
    pub priority: bool, // !important
    pub overridden: bool,
}

/// Element inspector
#[derive(Debug, Default)]
pub struct Inspector {
    nodes: HashMap<u64, InspectedNode>,
    selected: Option<u64>,
    styles_cache: HashMap<u64, Vec<InspectedStyleRule>>,
}

impl Inspector {
    pub fn new() -> Self { Self::default() }
    
    /// Add node
    pub fn add_node(&mut self, node: InspectedNode) {
        self.nodes.insert(node.id, node);
    }
    
    /// Get node
    pub fn get_node(&self, id: u64) -> Option<&InspectedNode> {
        self.nodes.get(&id)
    }
    
    /// Select node
    pub fn select(&mut self, id: u64) {
        self.selected = Some(id);
    }
    
    /// Get selected node
    pub fn get_selected(&self) -> Option<&InspectedNode> {
        self.selected.and_then(|id| self.nodes.get(&id))
    }
    
    /// Get computed style
    pub fn get_computed_style(&self, id: u64, property: &str) -> Option<&str> {
        self.nodes.get(&id)
            .and_then(|n| n.computed_styles.get(property))
            .map(String::as_str)
    }
    
    /// Get matching rules for node
    pub fn get_matching_rules(&self, id: u64) -> &[InspectedStyleRule] {
        self.styles_cache.get(&id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    
    /// Set matching rules
    pub fn set_matching_rules(&mut self, id: u64, rules: Vec<InspectedStyleRule>) {
        self.styles_cache.insert(id, rules);
    }
    
    /// Get DOM tree as string
    pub fn get_dom_tree(&self, root_id: u64, depth: usize) -> String {
        let mut result = String::new();
        self.build_tree_string(&mut result, root_id, depth, 0);
        result
    }
    
    fn build_tree_string(&self, result: &mut String, id: u64, max_depth: usize, indent: usize) {
        if indent > max_depth {
            return;
        }
        
        if let Some(node) = self.nodes.get(&id) {
            let prefix = "  ".repeat(indent);
            
            match node.node_type {
                NodeType::Element => {
                    result.push_str(&format!("{}<{}>\n", prefix, node.get_selector()));
                    for child_id in &node.children {
                        self.build_tree_string(result, *child_id, max_depth, indent + 1);
                    }
                    result.push_str(&format!("{}</{}>\n", prefix, node.tag_name));
                }
                NodeType::Text => {
                    if let Some(text) = &node.text_content {
                        let short = if text.len() > 50 { &text[..50] } else { text };
                        result.push_str(&format!("{}\"{}...\"\n", prefix, short.trim()));
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_inspector() {
        let mut inspector = Inspector::new();
        
        let mut div = InspectedNode::element(1, "div");
        div.id_attr = Some("main".into());
        div.class_list = vec!["container".into()];
        
        inspector.add_node(div);
        inspector.select(1);
        
        let selected = inspector.get_selected().unwrap();
        assert_eq!(selected.get_selector(), "div#main.container");
    }
}
