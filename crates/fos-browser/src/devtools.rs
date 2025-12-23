//! DevTools Integration
//!
//! Integrates fos-devtools for browser debugging and inspection.

use std::collections::HashMap;
use fos_dom::{Document, DomTree, NodeId};
use fos_devtools::{
    Console, ConsoleMessage,
    Inspector, InspectedNode,
    NetworkPanel, NetworkRequest,
};

/// DevTools manager for the browser
pub struct DevTools {
    /// Console panel
    pub console: Console,
    /// Element inspector
    pub inspector: Inspector,
    /// Network panel
    pub network: NetworkPanel,
    /// Whether DevTools is open
    is_open: bool,
    /// Active panel
    active_panel: DevToolsPanel,
}

/// DevTools panel types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DevToolsPanel {
    Console,
    Elements,
    Network,
    Sources,
    Performance,
}

impl Default for DevToolsPanel {
    fn default() -> Self {
        DevToolsPanel::Console
    }
}

impl DevTools {
    /// Create new DevTools instance
    pub fn new() -> Self {
        Self {
            console: Console::new(),
            inspector: Inspector::new(),
            network: NetworkPanel::new(),
            is_open: false,
            active_panel: DevToolsPanel::Console,
        }
    }
    
    /// Toggle DevTools visibility
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
        log::info!("DevTools {}", if self.is_open { "opened" } else { "closed" });
    }
    
    /// Check if DevTools is open
    pub fn is_open(&self) -> bool {
        self.is_open
    }
    
    /// Set active panel
    pub fn set_panel(&mut self, panel: DevToolsPanel) {
        self.active_panel = panel;
    }
    
    /// Get active panel
    pub fn active_panel(&self) -> DevToolsPanel {
        self.active_panel
    }
    
    // === Console Methods ===
    
    /// Log a message to console
    pub fn log(&mut self, message: &str) {
        self.console.log(message, Vec::new());
    }
    
    /// Log a warning
    pub fn warn(&mut self, message: &str) {
        self.console.warn(message, Vec::new());
    }
    
    /// Log an error
    pub fn error(&mut self, message: &str) {
        self.console.error(message, Vec::new());
    }
    
    /// Get console messages
    pub fn get_console_messages(&self) -> Vec<&ConsoleMessage> {
        self.console.get_messages().iter().collect()
    }
    
    /// Clear console
    pub fn clear_console(&mut self) {
        self.console.clear();
    }
    
    // === Inspector Methods ===
    
    /// Build inspector from DOM document
    pub fn inspect_document(&mut self, document: &Document) {
        let tree = document.tree();
        self.build_inspector_tree(tree, tree.root(), None);
    }
    
    /// Recursively build inspector tree from DOM
    fn build_inspector_tree(&mut self, tree: &DomTree, node_id: NodeId, parent_id: Option<u64>) {
        if !node_id.is_valid() {
            return;
        }
        
        let id = node_id.index() as u64;
        
        if let Some(node) = tree.get(node_id) {
            let mut inspected = if let Some(element) = node.as_element() {
                let tag = tree.resolve(element.name.local);
                let mut n = InspectedNode::element(id, tag);
                
                // Extract attributes
                for attr in element.attrs.iter() {
                    let name = tree.resolve(attr.name.local);
                    n.attributes.insert(name.to_string(), attr.value.clone());
                    
                    if name == "id" {
                        n.id_attr = Some(attr.value.clone());
                    }
                    if name == "class" {
                        n.class_list = attr.value.split_whitespace()
                            .map(String::from)
                            .collect();
                    }
                }
                
                n
            } else if let Some(text) = node.as_text() {
                let content = text.trim();
                if content.is_empty() {
                    return; // Skip empty text nodes
                }
                InspectedNode::text(id, content)
            } else {
                return;
            };
            
            inspected.parent = parent_id;
            
            // Collect children first
            let children: Vec<NodeId> = tree.children(node_id)
                .map(|(child_id, _)| child_id)
                .collect();
            
            inspected.children = children.iter()
                .map(|c| c.index() as u64)
                .collect();
            
            self.inspector.add_node(inspected);
            
            // Recurse
            for child_id in children {
                self.build_inspector_tree(tree, child_id, Some(id));
            }
        }
    }
    
    /// Select element by ID
    pub fn select_element(&mut self, id: u64) {
        self.inspector.select(id);
    }
    
    /// Get selected element
    pub fn get_selected_element(&self) -> Option<&InspectedNode> {
        self.inspector.get_selected()
    }
    
    /// Get DOM tree as string
    pub fn get_dom_tree_string(&self, root_id: u64, depth: usize) -> String {
        self.inspector.get_dom_tree(root_id, depth)
    }
    
    // === Network Methods ===
    
    /// Log a network request start
    pub fn log_request(&mut self, url: &str, method: &str) -> u64 {
        self.network.log_request(url, method, HashMap::new())
    }
    
    /// Log a network response
    pub fn log_response(&mut self, request_id: u64, status: u16, status_text: &str) {
        self.network.log_response(request_id, status, status_text, HashMap::new());
    }
    
    /// Log a network error
    pub fn log_network_error(&mut self, request_id: u64, error: &str) {
        self.network.log_error(request_id, error);
    }
    
    /// Get all network requests
    pub fn get_network_requests(&self) -> &[NetworkRequest] {
        self.network.get_requests()
    }
    
    /// Clear network log
    pub fn clear_network(&mut self) {
        self.network.clear();
    }
    
    /// Get network stats
    pub fn get_network_stats(&self) -> NetworkStats {
        let requests = self.network.get_requests();
        let total = requests.len();
        let pending = requests.iter()
            .filter(|r| matches!(r.status, fos_devtools::network::RequestStatus::Pending))
            .count();
        let failed = requests.iter()
            .filter(|r| matches!(r.status, fos_devtools::network::RequestStatus::Failed { .. }))
            .count();
        let total_size = self.network.get_total_size();
        
        NetworkStats {
            total_requests: total,
            pending_requests: pending,
            failed_requests: failed,
            total_bytes: total_size,
        }
    }
}

impl Default for DevTools {
    fn default() -> Self {
        Self::new()
    }
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub total_requests: usize,
    pub pending_requests: usize,
    pub failed_requests: usize,
    pub total_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_devtools_creation() {
        let devtools = DevTools::new();
        assert!(!devtools.is_open());
        assert_eq!(devtools.active_panel(), DevToolsPanel::Console);
    }
    
    #[test]
    fn test_devtools_toggle() {
        let mut devtools = DevTools::new();
        devtools.toggle();
        assert!(devtools.is_open());
        devtools.toggle();
        assert!(!devtools.is_open());
    }
    
    #[test]
    fn test_console_logging() {
        let mut devtools = DevTools::new();
        devtools.log("Test message");
        devtools.warn("Warning");
        devtools.error("Error");
        
        assert_eq!(devtools.get_console_messages().len(), 3);
    }
    
    #[test]
    fn test_network_logging() {
        let mut devtools = DevTools::new();
        let id = devtools.log_request("https://example.com", "GET");
        devtools.log_response(id, 200, "OK");
        
        let stats = devtools.get_network_stats();
        assert_eq!(stats.total_requests, 1);
    }
}
