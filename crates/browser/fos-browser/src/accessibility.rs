//! Accessibility Integration
//!
//! Integrates fos-a11y for keyboard navigation, focus management,
//! and accessibility tree support.

use fos_dom::{Document, DomTree, NodeId};
use fos_a11y::{
    AccessibilityTree, AriaRole,
    FocusManager, FocusIndicator,
};

/// Accessibility manager for the browser
pub struct AccessibilityManager {
    /// Accessibility tree
    pub tree: AccessibilityTree,
    /// Focus manager
    pub focus: FocusManager,
    /// Focus indicator style
    pub focus_indicator: FocusIndicator,
    /// Link regions for keyboard navigation
    link_regions: Vec<FocusableRegion>,
    /// Form input regions
    input_regions: Vec<FocusableRegion>,
}

/// A focusable region in the page
#[derive(Debug, Clone)]
pub struct FocusableRegion {
    pub id: u64,
    pub element_type: FocusableType,
    pub bounds: FocusBounds,
    pub url: Option<String>,  // For links
    pub name: String,         // Accessible name
}

/// Type of focusable element
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusableType {
    Link,
    Button,
    Input,
    Checkbox,
    Radio,
    Select,
    Textarea,
}

/// Bounds of a focusable element
#[derive(Debug, Clone, Default)]
pub struct FocusBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl AccessibilityManager {
    /// Create new accessibility manager
    pub fn new() -> Self {
        Self {
            tree: AccessibilityTree::new(),
            focus: FocusManager::new(),
            focus_indicator: FocusIndicator::default(),
            link_regions: Vec::new(),
            input_regions: Vec::new(),
        }
    }
    
    /// Build accessibility tree from DOM document
    pub fn build_from_document(&mut self, document: &Document) {
        self.tree = AccessibilityTree::new();
        self.link_regions.clear();
        self.input_regions.clear();
        
        let tree = document.tree();
        let root_id = self.tree.create_root();
        
        self.build_tree_recursive(tree, tree.root(), root_id);
        
        // Build focus order from collected regions
        let focus_order: Vec<u64> = self.link_regions.iter()
            .chain(self.input_regions.iter())
            .map(|r| r.id)
            .collect();
        
        self.focus.set_focus_order(focus_order);
        
        log::debug!("Built a11y tree: {} links, {} inputs", 
            self.link_regions.len(), self.input_regions.len());
    }
    
    /// Recursively build accessibility tree from DOM
    fn build_tree_recursive(&mut self, tree: &DomTree, node_id: NodeId, parent_a11y_id: u64) {
        if !node_id.is_valid() {
            return;
        }
        
        if let Some(node) = tree.get(node_id) {
            if let Some(element) = node.as_element() {
                let tag = tree.resolve(element.name.local).to_lowercase();
                
                // Map HTML elements to ARIA roles
                let role = match tag.as_str() {
                    "a" => AriaRole::Link,
                    "button" => AriaRole::Button,
                    "input" => AriaRole::TextBox,
                    "textarea" => AriaRole::TextBox,
                    "select" => AriaRole::Generic, // Should be listbox but not in AriaRole
                    "img" => AriaRole::Img,
                    "nav" => AriaRole::Navigation,
                    "main" => AriaRole::Main,
                    "header" => AriaRole::Banner,
                    "footer" => AriaRole::ContentInfo,
                    "aside" => AriaRole::Complementary,
                    "article" => AriaRole::Article,
                    "section" => AriaRole::Region,
                    "form" => AriaRole::Form,
                    "table" => AriaRole::Table,
                    "ul" | "ol" => AriaRole::List,
                    "li" => AriaRole::ListItem,
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => AriaRole::Heading,
                    _ => AriaRole::Generic,
                };
                
                // Add to accessibility tree
                let a11y_id = self.tree.add_node(role, Some(parent_a11y_id));
                
                // Extract accessible name and attributes
                let mut name = String::new();
                let mut href: Option<String> = None;
                let mut input_type = "text";
                
                for attr in element.attrs.iter() {
                    let attr_name = tree.resolve(attr.name.local);
                    match attr_name {
                        "aria-label" | "alt" | "title" => {
                            if name.is_empty() {
                                name = attr.value.clone();
                            }
                        }
                        "href" => {
                            href = Some(attr.value.clone());
                        }
                        "type" => {
                            input_type = Box::leak(attr.value.clone().into_boxed_str());
                        }
                        _ => {}
                    }
                }
                
                // Set accessible name
                if let Some(a_node) = self.tree.get_node_mut(a11y_id) {
                    a_node.set_name(&name);
                    a_node.focusable = matches!(tag.as_str(), 
                        "a" | "button" | "input" | "select" | "textarea"
                    );
                }
                
                // Track focusable regions for keyboard navigation
                match tag.as_str() {
                    "a" => {
                        self.link_regions.push(FocusableRegion {
                            id: a11y_id,
                            element_type: FocusableType::Link,
                            bounds: FocusBounds::default(),
                            url: href,
                            name,
                        });
                    }
                    "button" => {
                        self.input_regions.push(FocusableRegion {
                            id: a11y_id,
                            element_type: FocusableType::Button,
                            bounds: FocusBounds::default(),
                            url: None,
                            name,
                        });
                    }
                    "input" => {
                        let ftype = match input_type {
                            "checkbox" => FocusableType::Checkbox,
                            "radio" => FocusableType::Radio,
                            _ => FocusableType::Input,
                        };
                        self.input_regions.push(FocusableRegion {
                            id: a11y_id,
                            element_type: ftype,
                            bounds: FocusBounds::default(),
                            url: None,
                            name,
                        });
                    }
                    "select" => {
                        self.input_regions.push(FocusableRegion {
                            id: a11y_id,
                            element_type: FocusableType::Select,
                            bounds: FocusBounds::default(),
                            url: None,
                            name,
                        });
                    }
                    "textarea" => {
                        self.input_regions.push(FocusableRegion {
                            id: a11y_id,
                            element_type: FocusableType::Textarea,
                            bounds: FocusBounds::default(),
                            url: None,
                            name,
                        });
                    }
                    _ => {}
                }
                
                // Recurse into children
                for (child_id, _) in tree.children(node_id) {
                    self.build_tree_recursive(tree, child_id, a11y_id);
                }
            }
        }
    }
    
    /// Handle Tab key - focus next element
    pub fn focus_next(&mut self) -> Option<u64> {
        self.focus.focus_next()
    }
    
    /// Handle Shift+Tab - focus previous element
    pub fn focus_prev(&mut self) -> Option<u64> {
        self.focus.focus_prev()
    }
    
    /// Get currently focused element ID
    pub fn get_focused(&self) -> Option<u64> {
        self.focus.get_focused()
    }
    
    /// Get focused region details
    pub fn get_focused_region(&self) -> Option<&FocusableRegion> {
        let focused = self.focus.get_focused()?;
        self.link_regions.iter()
            .chain(self.input_regions.iter())
            .find(|r| r.id == focused)
    }
    
    /// Get URL of focused link (for Enter key activation)
    pub fn get_focused_link_url(&self) -> Option<&str> {
        let region = self.get_focused_region()?;
        if region.element_type == FocusableType::Link {
            region.url.as_deref()
        } else {
            None
        }
    }
    
    /// Clear focus
    pub fn blur(&mut self) {
        self.focus.blur();
    }
    
    /// Get all link regions
    pub fn get_links(&self) -> &[FocusableRegion] {
        &self.link_regions
    }
    
    /// Get all input regions
    pub fn get_inputs(&self) -> &[FocusableRegion] {
        &self.input_regions
    }
    
    /// Get accessibility statistics
    pub fn stats(&self) -> AccessibilityStats {
        AccessibilityStats {
            total_nodes: self.tree.get_focusable_nodes().len() + 
                         self.tree.get_landmarks().len(),
            focusable_count: self.link_regions.len() + self.input_regions.len(),
            link_count: self.link_regions.len(),
            input_count: self.input_regions.len(),
            landmark_count: self.tree.get_landmarks().len(),
        }
    }
}

impl Default for AccessibilityManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Accessibility statistics
#[derive(Debug, Clone)]
pub struct AccessibilityStats {
    pub total_nodes: usize,
    pub focusable_count: usize,
    pub link_count: usize,
    pub input_count: usize,
    pub landmark_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_accessibility_manager_creation() {
        let manager = AccessibilityManager::new();
        assert_eq!(manager.get_focused(), None);
        assert!(manager.get_links().is_empty());
    }
    
    #[test]
    fn test_focus_navigation() {
        let mut manager = AccessibilityManager::new();
        manager.focus.set_focus_order(vec![1, 2, 3]);
        
        assert_eq!(manager.focus_next(), Some(1));
        assert_eq!(manager.focus_next(), Some(2));
        assert_eq!(manager.focus_prev(), Some(1));
    }
}
