//! Selection API
//!
//! Implementation of the Selection and Range APIs for text selection.

use std::sync::Arc;

/// Selection direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionDirection {
    #[default]
    None,
    Forward,
    Backward,
}

/// Selection type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionType {
    #[default]
    None,
    Caret,
    Range,
}

/// Text range within a document
#[derive(Debug, Clone)]
pub struct Range {
    /// Start container node ID
    pub start_container: u64,
    /// Start offset within container
    pub start_offset: usize,
    /// End container node ID
    pub end_container: u64,
    /// End offset within container
    pub end_offset: usize,
    /// Whether range is collapsed (start == end)
    pub collapsed: bool,
}

impl Range {
    /// Create a collapsed range at position
    pub fn collapsed_at(node_id: u64, offset: usize) -> Self {
        Self {
            start_container: node_id,
            start_offset: offset,
            end_container: node_id,
            end_offset: offset,
            collapsed: true,
        }
    }
    
    /// Create a range spanning text
    pub fn new(start_node: u64, start_offset: usize, end_node: u64, end_offset: usize) -> Self {
        let collapsed = start_node == end_node && start_offset == end_offset;
        Self {
            start_container: start_node,
            start_offset,
            end_container: end_node,
            end_offset,
            collapsed,
        }
    }
    
    /// Collapse range to start
    pub fn collapse_to_start(&mut self) {
        self.end_container = self.start_container;
        self.end_offset = self.start_offset;
        self.collapsed = true;
    }
    
    /// Collapse range to end
    pub fn collapse_to_end(&mut self) {
        self.start_container = self.end_container;
        self.start_offset = self.end_offset;
        self.collapsed = true;
    }
    
    /// Set start position
    pub fn set_start(&mut self, node: u64, offset: usize) {
        self.start_container = node;
        self.start_offset = offset;
        self.update_collapsed();
    }
    
    /// Set end position
    pub fn set_end(&mut self, node: u64, offset: usize) {
        self.end_container = node;
        self.end_offset = offset;
        self.update_collapsed();
    }
    
    fn update_collapsed(&mut self) {
        self.collapsed = self.start_container == self.end_container 
            && self.start_offset == self.end_offset;
    }
    
    /// Check if range intersects with another
    pub fn intersects(&self, other: &Range) -> bool {
        // Simplified - full impl would need tree traversal
        self.start_container == other.start_container ||
        self.start_container == other.end_container ||
        self.end_container == other.start_container ||
        self.end_container == other.end_container
    }
    
    /// Clone range contents (returns text content)
    pub fn clone_contents(&self, get_text: impl Fn(u64) -> String) -> String {
        if self.collapsed {
            return String::new();
        }
        
        if self.start_container == self.end_container {
            let text = get_text(self.start_container);
            text.chars()
                .skip(self.start_offset)
                .take(self.end_offset - self.start_offset)
                .collect()
        } else {
            // Multi-node selection - simplified
            let start_text = get_text(self.start_container);
            start_text.chars().skip(self.start_offset).collect()
        }
    }
}

impl Default for Range {
    fn default() -> Self {
        Self::collapsed_at(0, 0)
    }
}

/// Document selection
#[derive(Debug, Clone, Default)]
pub struct Selection {
    /// Current selection ranges
    ranges: Vec<Range>,
    /// Anchor node ID
    pub anchor_node: Option<u64>,
    /// Anchor offset
    pub anchor_offset: usize,
    /// Focus node ID
    pub focus_node: Option<u64>,
    /// Focus offset
    pub focus_offset: usize,
    /// Selection direction
    pub direction: SelectionDirection,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get number of ranges in selection
    pub fn range_count(&self) -> usize {
        self.ranges.len()
    }
    
    /// Get range at index
    pub fn get_range_at(&self, index: usize) -> Option<&Range> {
        self.ranges.get(index)
    }
    
    /// Add a range to selection
    pub fn add_range(&mut self, range: Range) {
        // Update anchor/focus
        if self.ranges.is_empty() {
            self.anchor_node = Some(range.start_container);
            self.anchor_offset = range.start_offset;
        }
        self.focus_node = Some(range.end_container);
        self.focus_offset = range.end_offset;
        
        self.ranges.push(range);
    }
    
    /// Remove a range
    pub fn remove_range(&mut self, index: usize) {
        if index < self.ranges.len() {
            self.ranges.remove(index);
        }
    }
    
    /// Remove all ranges
    pub fn remove_all_ranges(&mut self) {
        self.ranges.clear();
        self.anchor_node = None;
        self.anchor_offset = 0;
        self.focus_node = None;
        self.focus_offset = 0;
    }
    
    /// Collapse selection to a point
    pub fn collapse(&mut self, node: u64, offset: usize) {
        self.remove_all_ranges();
        let range = Range::collapsed_at(node, offset);
        self.anchor_node = Some(node);
        self.anchor_offset = offset;
        self.focus_node = Some(node);
        self.focus_offset = offset;
        self.ranges.push(range);
    }
    
    /// Collapse to start of first range
    pub fn collapse_to_start(&mut self) {
        if let Some(range) = self.ranges.first() {
            let node = range.start_container;
            let offset = range.start_offset;
            self.collapse(node, offset);
        }
    }
    
    /// Collapse to end of last range
    pub fn collapse_to_end(&mut self) {
        if let Some(range) = self.ranges.last() {
            let node = range.end_container;
            let offset = range.end_offset;
            self.collapse(node, offset);
        }
    }
    
    /// Check if selection is collapsed
    pub fn is_collapsed(&self) -> bool {
        self.ranges.is_empty() || (self.ranges.len() == 1 && self.ranges[0].collapsed)
    }
    
    /// Get selection type
    pub fn selection_type(&self) -> SelectionType {
        if self.ranges.is_empty() {
            SelectionType::None
        } else if self.is_collapsed() {
            SelectionType::Caret
        } else {
            SelectionType::Range
        }
    }
    
    /// Extend selection to a point
    pub fn extend(&mut self, node: u64, offset: usize) {
        if let Some(range) = self.ranges.last_mut() {
            range.set_end(node, offset);
            self.focus_node = Some(node);
            self.focus_offset = offset;
        }
    }
    
    /// Select all content in a node
    pub fn select_all_children(&mut self, node: u64, end_offset: usize) {
        self.remove_all_ranges();
        let range = Range::new(node, 0, node, end_offset);
        self.add_range(range);
    }
    
    /// Get string representation of selection
    pub fn to_string(&self, get_text: impl Fn(u64) -> String) -> String {
        self.ranges.iter()
            .map(|r| r.clone_contents(&get_text))
            .collect::<Vec<_>>()
            .join("")
    }
}

/// Input element selection (for text inputs)
#[derive(Debug, Clone, Default)]
pub struct InputSelection {
    /// Selection start
    pub start: usize,
    /// Selection end
    pub end: usize,
    /// Selection direction
    pub direction: SelectionDirection,
}

impl InputSelection {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn set_range(&mut self, start: usize, end: usize, direction: SelectionDirection) {
        self.start = start;
        self.end = end;
        self.direction = direction;
    }
    
    pub fn is_collapsed(&self) -> bool {
        self.start == self.end
    }
    
    pub fn length(&self) -> usize {
        if self.end >= self.start {
            self.end - self.start
        } else {
            0
        }
    }
    
    /// Select all text
    pub fn select_all(&mut self, text_length: usize) {
        self.start = 0;
        self.end = text_length;
        self.direction = SelectionDirection::Forward;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_range_collapsed() {
        let range = Range::collapsed_at(1, 5);
        assert!(range.collapsed);
        assert_eq!(range.start_offset, 5);
        assert_eq!(range.end_offset, 5);
    }
    
    #[test]
    fn test_range_span() {
        let range = Range::new(1, 0, 1, 10);
        assert!(!range.collapsed);
    }
    
    #[test]
    fn test_selection_add_range() {
        let mut sel = Selection::new();
        sel.add_range(Range::new(1, 0, 1, 5));
        
        assert_eq!(sel.range_count(), 1);
        assert_eq!(sel.anchor_node, Some(1));
    }
    
    #[test]
    fn test_selection_collapse() {
        let mut sel = Selection::new();
        sel.add_range(Range::new(1, 0, 1, 10));
        sel.collapse(1, 5);
        
        assert!(sel.is_collapsed());
        assert_eq!(sel.focus_offset, 5);
    }
    
    #[test]
    fn test_input_selection() {
        let mut sel = InputSelection::new();
        sel.set_range(5, 15, SelectionDirection::Forward);
        
        assert_eq!(sel.length(), 10);
        assert!(!sel.is_collapsed());
    }
}
