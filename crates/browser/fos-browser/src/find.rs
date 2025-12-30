//! Find in Page
//!
//! Text search within rendered page content.

use fos_dom::{Document, DomTree, NodeId};
use fos_render::Color;

/// Search match in the page
#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Node containing the match
    pub node_id: NodeId,
    /// Character offset within the text node
    pub offset: usize,
    /// Length of matched text
    pub length: usize,
    /// Y position for scrolling
    pub y_position: f32,
}

/// Find in page controller
#[derive(Debug)]
pub struct FindInPage {
    /// Current search query
    query: String,
    /// All matches in document
    matches: Vec<SearchMatch>,
    /// Current match index
    current_index: usize,
    /// Case sensitive search
    case_sensitive: bool,
    /// Is find bar visible
    pub visible: bool,
}

impl Default for FindInPage {
    fn default() -> Self {
        Self::new()
    }
}

impl FindInPage {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            current_index: 0,
            case_sensitive: false,
            visible: false,
        }
    }
    
    /// Show find bar
    pub fn show(&mut self) {
        self.visible = true;
    }
    
    /// Hide find bar
    pub fn hide(&mut self) {
        self.visible = false;
        self.clear();
    }
    
    /// Toggle find bar visibility
    pub fn toggle(&mut self) {
        if self.visible {
            self.hide();
        } else {
            self.show();
        }
    }
    
    /// Set search query and perform search
    pub fn search(&mut self, query: &str, document: &Document) {
        self.query = query.to_string();
        self.matches.clear();
        self.current_index = 0;
        
        if query.is_empty() {
            return;
        }
        
        let tree = document.tree();
        let body = document.body();
        
        if body.is_valid() {
            self.search_node(tree, body, 0.0);
        }
    }
    
    /// Recursively search nodes
    fn search_node(&mut self, tree: &DomTree, node_id: NodeId, y_position: f32) {
        let mut current_y = y_position;
        
        for (child_id, child_node) in tree.children(node_id) {
            // Check text nodes
            if let Some(text) = child_node.as_text() {
                self.search_text(child_id, text, current_y);
            }
            
            // Check if element (to skip script/style)
            if let Some(elem) = child_node.as_element() {
                let tag = tree.resolve(elem.name.local).to_lowercase();
                if tag == "script" || tag == "style" || tag == "noscript" {
                    continue;
                }
                
                // Estimate y position based on block elements
                if matches!(tag.as_str(), "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "li" | "tr") {
                    current_y += 20.0;
                }
            }
            
            // Recurse
            self.search_node(tree, child_id, current_y);
        }
    }
    
    /// Search within a text node
    fn search_text(&mut self, node_id: NodeId, text: &str, y_position: f32) {
        let search_text = if self.case_sensitive {
            text.to_string()
        } else {
            text.to_lowercase()
        };
        
        let query = if self.case_sensitive {
            self.query.clone()
        } else {
            self.query.to_lowercase()
        };
        
        let mut offset = 0;
        while let Some(pos) = search_text[offset..].find(&query) {
            self.matches.push(SearchMatch {
                node_id,
                offset: offset + pos,
                length: query.len(),
                y_position,
            });
            offset += pos + 1;
        }
    }
    
    /// Get current query
    pub fn query(&self) -> &str {
        &self.query
    }
    
    /// Get number of matches
    pub fn match_count(&self) -> usize {
        self.matches.len()
    }
    
    /// Get current match index (1-based for display)
    pub fn current_match(&self) -> usize {
        if self.matches.is_empty() {
            0
        } else {
            self.current_index + 1
        }
    }
    
    /// Go to next match
    pub fn next(&mut self) -> Option<f32> {
        if self.matches.is_empty() {
            return None;
        }
        
        self.current_index = (self.current_index + 1) % self.matches.len();
        Some(self.matches[self.current_index].y_position)
    }
    
    /// Go to previous match
    pub fn prev(&mut self) -> Option<f32> {
        if self.matches.is_empty() {
            return None;
        }
        
        if self.current_index == 0 {
            self.current_index = self.matches.len() - 1;
        } else {
            self.current_index -= 1;
        }
        Some(self.matches[self.current_index].y_position)
    }
    
    /// Get Y position of current match for scrolling
    pub fn current_y(&self) -> Option<f32> {
        self.matches.get(self.current_index).map(|m| m.y_position)
    }
    
    /// Clear search
    pub fn clear(&mut self) {
        self.query.clear();
        self.matches.clear();
        self.current_index = 0;
    }
    
    /// Check if a node has a match at given offset
    pub fn is_highlighted(&self, node_id: NodeId, char_offset: usize) -> bool {
        self.matches.iter().any(|m| {
            m.node_id == node_id && 
            char_offset >= m.offset && 
            char_offset < m.offset + m.length
        })
    }
    
    /// Check if this is the current match
    pub fn is_current_match(&self, node_id: NodeId, char_offset: usize) -> bool {
        if let Some(current) = self.matches.get(self.current_index) {
            current.node_id == node_id && 
            char_offset >= current.offset && 
            char_offset < current.offset + current.length
        } else {
            false
        }
    }
    
    /// Get highlight color for matches
    pub fn highlight_color(&self) -> Color {
        Color::rgba(255, 255, 0, 180) // Yellow
    }
    
    /// Get highlight color for current match
    pub fn current_highlight_color(&self) -> Color {
        Color::rgba(255, 165, 0, 220) // Orange
    }
    
    /// Toggle case sensitivity
    pub fn toggle_case_sensitive(&mut self) {
        self.case_sensitive = !self.case_sensitive;
    }
    
    pub fn is_case_sensitive(&self) -> bool {
        self.case_sensitive
    }
    
    /// Get status text (e.g., "3 of 10")
    pub fn status_text(&self) -> String {
        if self.query.is_empty() {
            String::new()
        } else if self.matches.is_empty() {
            "No matches".to_string()
        } else {
            format!("{} of {}", self.current_match(), self.match_count())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_find_basic() {
        let mut find = FindInPage::new();
        // Without a document we can't fully test, but we can test the API
        assert_eq!(find.match_count(), 0);
        assert_eq!(find.status_text(), "");
        
        find.query = "test".to_string();
        assert_eq!(find.status_text(), "No matches");
    }
}
