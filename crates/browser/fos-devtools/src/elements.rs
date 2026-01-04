//! Elements Panel
//!
//! DOM tree navigation, styles panel, box model, and event listeners.

use std::collections::HashMap;

/// DOM node info for inspector
#[derive(Debug, Clone)]
pub struct ElementNode {
    pub node_id: u64,
    pub node_type: NodeType,
    pub node_name: String,
    pub node_value: Option<String>,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<u64>,
    pub parent: Option<u64>,
    pub pseudo_type: Option<PseudoType>,
}

/// Node type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType { Element = 1, Text = 3, Comment = 8, Document = 9, DocumentType = 10, DocumentFragment = 11 }

/// Pseudo element type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PseudoType { Before, After, Marker, Backdrop, FirstLetter, FirstLine, Selection }

/// Computed styles
#[derive(Debug, Clone, Default)]
pub struct ComputedStyles {
    pub properties: HashMap<String, ComputedValue>,
}

/// Computed value
#[derive(Debug, Clone)]
pub struct ComputedValue {
    pub value: String,
    pub important: bool,
    pub source: StyleSource,
}

/// Style source
#[derive(Debug, Clone)]
pub enum StyleSource {
    UserAgent,
    User,
    Author { selector: String, stylesheet: String, line: u32 },
    Inline,
    Inherited { from: u64 },
}

/// Matched CSS rule
#[derive(Debug, Clone)]
pub struct MatchedRule {
    pub selector: String,
    pub specificity: (u32, u32, u32),
    pub stylesheet_url: Option<String>,
    pub line: u32,
    pub properties: Vec<StyleProperty>,
}

/// Style property
#[derive(Debug, Clone)]
pub struct StyleProperty {
    pub name: String,
    pub value: String,
    pub important: bool,
    pub overridden: bool,
}

/// Box model info
#[derive(Debug, Clone, Default)]
pub struct BoxModel {
    pub content: Rect,
    pub padding: Rect,
    pub border: Rect,
    pub margin: Rect,
    pub width: f64,
    pub height: f64,
}

/// Rectangle
#[derive(Debug, Clone, Default)]
pub struct Rect {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// Event listener info
#[derive(Debug, Clone)]
pub struct EventListenerInfo {
    pub event_type: String,
    pub handler: String,
    pub use_capture: bool,
    pub passive: bool,
    pub once: bool,
    pub source_url: Option<String>,
    pub line: Option<u32>,
}

/// Elements panel
#[derive(Debug, Default)]
pub struct ElementsPanel {
    nodes: HashMap<u64, ElementNode>,
    root_id: Option<u64>,
    selected_id: Option<u64>,
    hover_id: Option<u64>,
    computed_styles: HashMap<u64, ComputedStyles>,
    matched_rules: HashMap<u64, Vec<MatchedRule>>,
    box_models: HashMap<u64, BoxModel>,
    event_listeners: HashMap<u64, Vec<EventListenerInfo>>,
}

impl ElementsPanel {
    pub fn new() -> Self { Self::default() }
    
    pub fn set_document(&mut self, root: ElementNode) {
        self.root_id = Some(root.node_id);
        self.nodes.insert(root.node_id, root);
    }
    
    pub fn add_node(&mut self, node: ElementNode) { self.nodes.insert(node.node_id, node); }
    pub fn get_node(&self, id: u64) -> Option<&ElementNode> { self.nodes.get(&id) }
    pub fn select(&mut self, id: u64) { self.selected_id = Some(id); }
    pub fn hover(&mut self, id: Option<u64>) { self.hover_id = id; }
    pub fn selected(&self) -> Option<u64> { self.selected_id }
    
    pub fn set_computed_styles(&mut self, id: u64, styles: ComputedStyles) { self.computed_styles.insert(id, styles); }
    pub fn get_computed_styles(&self, id: u64) -> Option<&ComputedStyles> { self.computed_styles.get(&id) }
    
    pub fn set_matched_rules(&mut self, id: u64, rules: Vec<MatchedRule>) { self.matched_rules.insert(id, rules); }
    pub fn get_matched_rules(&self, id: u64) -> Option<&Vec<MatchedRule>> { self.matched_rules.get(&id) }
    
    pub fn set_box_model(&mut self, id: u64, model: BoxModel) { self.box_models.insert(id, model); }
    pub fn get_box_model(&self, id: u64) -> Option<&BoxModel> { self.box_models.get(&id) }
    
    pub fn set_event_listeners(&mut self, id: u64, listeners: Vec<EventListenerInfo>) { self.event_listeners.insert(id, listeners); }
    pub fn get_event_listeners(&self, id: u64) -> Option<&Vec<EventListenerInfo>> { self.event_listeners.get(&id) }
    
    /// Get node selector path
    pub fn get_selector_path(&self, id: u64) -> String {
        let mut path = Vec::new();
        let mut current = Some(id);
        
        while let Some(node_id) = current {
            if let Some(node) = self.nodes.get(&node_id) {
                if node.node_type == NodeType::Element {
                    let mut selector = node.node_name.to_lowercase();
                    if let Some((_, id_val)) = node.attributes.iter().find(|(k, _)| k == "id") {
                        selector = format!("#{}", id_val);
                    }
                    path.push(selector);
                }
                current = node.parent;
            } else { break; }
        }
        
        path.reverse();
        path.join(" > ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_elements_panel() {
        let mut panel = ElementsPanel::new();
        panel.add_node(ElementNode { node_id: 1, node_type: NodeType::Element, node_name: "div".into(),
            node_value: None, attributes: vec![("id".into(), "app".into())], children: vec![], parent: None, pseudo_type: None });
        panel.select(1);
        assert_eq!(panel.selected(), Some(1));
    }
    
    #[test]
    fn test_box_model() {
        let model = BoxModel { content: Rect { top: 0.0, right: 100.0, bottom: 50.0, left: 0.0 },
            padding: Rect::default(), border: Rect::default(), margin: Rect::default(), width: 100.0, height: 50.0 };
        assert_eq!(model.width, 100.0);
    }
}
