//! Text Selection API
//!
//! DOM text selection and ranges.

use fos_dom::NodeId;

/// A point in text content
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPoint {
    pub node: NodeId,
    pub offset: usize,
}

/// Selection type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionType {
    #[default]
    None,
    Caret,
    Range,
}

/// Text selection state
#[derive(Debug, Clone)]
pub struct Selection {
    pub anchor: Option<TextPoint>,
    pub focus: Option<TextPoint>,
    pub is_collapsed: bool,
    pub selection_type: SelectionType,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            anchor: None,
            focus: None,
            is_collapsed: true,
            selection_type: SelectionType::None,
        }
    }
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get selected text bounds
    pub fn get_range(&self) -> Option<(TextPoint, TextPoint)> {
        match (self.anchor, self.focus) {
            (Some(a), Some(f)) => Some((a, f)),
            _ => None,
        }
    }
    
    /// Check if selection is empty
    pub fn is_empty(&self) -> bool {
        self.anchor.is_none() && self.focus.is_none()
    }
}

/// Selection manager
#[derive(Debug, Default)]
pub struct SelectionManager {
    selection: Selection,
    /// Selected text content (cached)
    selected_text: String,
}

impl SelectionManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get current selection
    pub fn get_selection(&self) -> &Selection {
        &self.selection
    }
    
    /// Set selection to a single point (caret)
    pub fn set_caret(&mut self, node: NodeId, offset: usize) {
        let point = TextPoint { node, offset };
        self.selection = Selection {
            anchor: Some(point),
            focus: Some(point),
            is_collapsed: true,
            selection_type: SelectionType::Caret,
        };
        self.selected_text.clear();
    }
    
    /// Set selection range
    pub fn set_range(&mut self, anchor: TextPoint, focus: TextPoint, text: &str) {
        self.selection = Selection {
            anchor: Some(anchor),
            focus: Some(focus),
            is_collapsed: anchor == focus,
            selection_type: if anchor == focus { SelectionType::Caret } else { SelectionType::Range },
        };
        self.selected_text = text.to_string();
    }
    
    /// Extend selection to point
    pub fn extend_to(&mut self, focus: TextPoint, text: &str) {
        if let Some(anchor) = self.selection.anchor {
            self.selection.focus = Some(focus);
            self.selection.is_collapsed = anchor == focus;
            self.selection.selection_type = if anchor == focus { 
                SelectionType::Caret 
            } else { 
                SelectionType::Range 
            };
            self.selected_text = text.to_string();
        }
    }
    
    /// Clear selection
    pub fn collapse(&mut self) {
        self.selection = Selection::default();
        self.selected_text.clear();
    }
    
    /// Get selected text
    pub fn get_text(&self) -> &str {
        &self.selected_text
    }
    
    /// Select all content
    pub fn select_all(&mut self, start: TextPoint, end: TextPoint, text: &str) {
        self.set_range(start, end, text);
    }
    
    /// Check if point is in selection
    pub fn contains(&self, node: NodeId, offset: usize) -> bool {
        if self.selection.is_collapsed {
            return false;
        }
        
        match (self.selection.anchor, self.selection.focus) {
            (Some(a), Some(f)) => {
                // Simplified check - assumes same node
                if a.node == node && f.node == node {
                    let (start, end) = if a.offset < f.offset { 
                        (a.offset, f.offset) 
                    } else { 
                        (f.offset, a.offset) 
                    };
                    offset >= start && offset <= end
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

/// Text range for DOM operations
#[derive(Debug, Clone)]
pub struct TextRange {
    pub start: TextPoint,
    pub end: TextPoint,
    pub contents: String,
}

impl TextRange {
    pub fn new(start: TextPoint, end: TextPoint) -> Self {
        Self {
            start,
            end,
            contents: String::new(),
        }
    }
    
    pub fn with_contents(start: TextPoint, end: TextPoint, contents: String) -> Self {
        Self { start, end, contents }
    }
    
    pub fn is_collapsed(&self) -> bool {
        self.start == self.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_selection_manager() {
        let mut mgr = SelectionManager::new();
        let node = NodeId::from_raw_parts(1, 0);
        
        mgr.set_caret(node, 5);
        assert!(mgr.get_selection().is_collapsed);
        
        let anchor = TextPoint { node, offset: 0 };
        let focus = TextPoint { node, offset: 10 };
        mgr.set_range(anchor, focus, "Hello World");
        
        assert!(!mgr.get_selection().is_collapsed);
        assert_eq!(mgr.get_text(), "Hello World");
    }
}
