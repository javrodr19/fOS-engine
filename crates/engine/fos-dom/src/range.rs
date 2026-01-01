//! Range and Selection API
//!
//! Range represents a contiguous part of the document.
//! Selection represents the user's text selection.
//!
//! These APIs are used for:
//! - Text selection and manipulation
//! - Programmatic editing (insertNode, deleteContents)
//! - Copy/paste operations

use crate::NodeId;

/// Range boundary point
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundaryPoint {
    /// The container node
    pub node: NodeId,
    /// Offset within the container (character offset for text, child index for elements)
    pub offset: u32,
}

impl BoundaryPoint {
    pub fn new(node: NodeId, offset: u32) -> Self {
        Self { node, offset }
    }

    /// Check if this point is before another point
    /// (Requires tree access for full implementation)
    pub fn is_before(&self, other: &BoundaryPoint) -> bool {
        if self.node == other.node {
            self.offset < other.offset
        } else {
            // Would need tree structure to properly compare
            false
        }
    }
}

/// Compare position of two boundary points
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionComparison {
    Before,
    Equal,
    After,
}

/// Range - a contiguous part of the document
#[derive(Debug, Clone)]
pub struct Range {
    /// Start boundary point
    start: BoundaryPoint,
    /// End boundary point
    end: BoundaryPoint,
    /// Whether the range is collapsed (start == end)
    collapsed: bool,
    /// Common ancestor container
    common_ancestor: NodeId,
}

impl Range {
    /// Create a new range at the given position (collapsed)
    pub fn new(start_container: NodeId, start_offset: u32) -> Self {
        let point = BoundaryPoint::new(start_container, start_offset);
        Self {
            start: point,
            end: point,
            collapsed: true,
            common_ancestor: start_container,
        }
    }

    /// Create a range between two points
    pub fn between(
        start_container: NodeId,
        start_offset: u32,
        end_container: NodeId,
        end_offset: u32,
    ) -> Self {
        let start = BoundaryPoint::new(start_container, start_offset);
        let end = BoundaryPoint::new(end_container, end_offset);
        let collapsed = start == end;
        Self {
            start,
            end,
            collapsed,
            common_ancestor: start_container, // Would compute actual ancestor
        }
    }

    // --- Getters ---

    /// Get the start container
    pub fn start_container(&self) -> NodeId {
        self.start.node
    }

    /// Get the start offset
    pub fn start_offset(&self) -> u32 {
        self.start.offset
    }

    /// Get the end container
    pub fn end_container(&self) -> NodeId {
        self.end.node
    }

    /// Get the end offset
    pub fn end_offset(&self) -> u32 {
        self.end.offset
    }

    /// Check if the range is collapsed
    pub fn collapsed(&self) -> bool {
        self.collapsed
    }

    /// Get the common ancestor container
    pub fn common_ancestor_container(&self) -> NodeId {
        self.common_ancestor
    }

    // --- Setters ---

    /// Set the start position
    pub fn set_start(&mut self, node: NodeId, offset: u32) {
        self.start = BoundaryPoint::new(node, offset);
        self.update_collapsed();
    }

    /// Set the end position
    pub fn set_end(&mut self, node: NodeId, offset: u32) {
        self.end = BoundaryPoint::new(node, offset);
        self.update_collapsed();
    }

    /// Set start before a node
    pub fn set_start_before(&mut self, node: NodeId, parent: NodeId, index: u32) {
        self.start = BoundaryPoint::new(parent, index);
        self.update_collapsed();
    }

    /// Set start after a node
    pub fn set_start_after(&mut self, node: NodeId, parent: NodeId, index: u32) {
        self.start = BoundaryPoint::new(parent, index + 1);
        self.update_collapsed();
    }

    /// Set end before a node
    pub fn set_end_before(&mut self, node: NodeId, parent: NodeId, index: u32) {
        self.end = BoundaryPoint::new(parent, index);
        self.update_collapsed();
    }

    /// Set end after a node
    pub fn set_end_after(&mut self, node: NodeId, parent: NodeId, index: u32) {
        self.end = BoundaryPoint::new(parent, index + 1);
        self.update_collapsed();
    }

    fn update_collapsed(&mut self) {
        self.collapsed = self.start == self.end;
    }

    // --- Manipulation ---

    /// Collapse the range to one of its boundaries
    pub fn collapse(&mut self, to_start: bool) {
        if to_start {
            self.end = self.start;
        } else {
            self.start = self.end;
        }
        self.collapsed = true;
    }

    /// Select a node
    pub fn select_node(&mut self, node: NodeId, parent: NodeId, index: u32, child_count: u32) {
        self.start = BoundaryPoint::new(parent, index);
        self.end = BoundaryPoint::new(parent, index + 1);
        self.collapsed = false;
    }

    /// Select the contents of a node
    pub fn select_node_contents(&mut self, node: NodeId, length: u32) {
        self.start = BoundaryPoint::new(node, 0);
        self.end = BoundaryPoint::new(node, length);
        self.collapsed = length == 0;
        self.common_ancestor = node;
    }

    /// Clone this range
    pub fn clone_range(&self) -> Range {
        self.clone()
    }

    /// Detach the range (no-op in modern DOM)
    pub fn detach(&mut self) {
        // No-op in modern DOM, kept for compatibility
    }

    // --- Comparison ---

    /// Compare boundary points
    pub fn compare_boundary_points(&self, how: RangeCompare, source_range: &Range) -> PositionComparison {
        let (this_point, source_point) = match how {
            RangeCompare::StartToStart => (&self.start, &source_range.start),
            RangeCompare::StartToEnd => (&self.start, &source_range.end),
            RangeCompare::EndToEnd => (&self.end, &source_range.end),
            RangeCompare::EndToStart => (&self.end, &source_range.start),
        };

        if this_point == source_point {
            PositionComparison::Equal
        } else if this_point.is_before(source_point) {
            PositionComparison::Before
        } else {
            PositionComparison::After
        }
    }

    /// Check if a point is in the range
    pub fn is_point_in_range(&self, node: NodeId, offset: u32) -> bool {
        let point = BoundaryPoint::new(node, offset);
        // Simplified: only works for same container
        if self.start.node == node && self.end.node == node {
            offset >= self.start.offset && offset <= self.end.offset
        } else {
            false
        }
    }

    /// Compare point to range
    pub fn compare_point(&self, node: NodeId, offset: u32) -> PositionComparison {
        let point = BoundaryPoint::new(node, offset);
        if point == self.start {
            PositionComparison::Equal
        } else if point.is_before(&self.start) {
            PositionComparison::Before
        } else {
            PositionComparison::After
        }
    }

    /// Check if ranges intersect
    pub fn intersects_node(&self, node: NodeId) -> bool {
        // Simplified: check if node is the same as start or end container
        self.start.node == node || self.end.node == node || self.common_ancestor == node
    }
}

/// Range comparison types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeCompare {
    StartToStart,
    StartToEnd,
    EndToEnd,
    EndToStart,
}

impl Default for Range {
    fn default() -> Self {
        Self::new(NodeId::ROOT, 0)
    }
}

/// Pending range operations
#[derive(Debug, Clone)]
pub enum RangeOperation {
    /// Delete contents of the range
    DeleteContents,
    /// Extract contents (delete and return as fragment)
    ExtractContents,
    /// Clone contents (return as fragment without deleting)
    CloneContents,
    /// Insert a node at the start
    InsertNode(NodeId),
    /// Surround contents with a node
    SurroundContents(NodeId),
}

/// Selection - represents the current selection in the document
#[derive(Debug, Clone)]
pub struct Selection {
    /// Ranges in the selection (usually one, but can be multiple)
    ranges: Vec<Range>,
    /// The anchor node (start of the selection)
    anchor_node: Option<NodeId>,
    /// Anchor offset
    anchor_offset: u32,
    /// The focus node (end of the selection)
    focus_node: Option<NodeId>,
    /// Focus offset
    focus_offset: u32,
    /// Is the selection collapsed?
    is_collapsed: bool,
    /// Selection type
    selection_type: SelectionType,
}

/// Selection type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionType {
    #[default]
    None,
    Caret,
    Range,
}

impl Selection {
    /// Create a new empty selection
    pub fn new() -> Self {
        Self {
            ranges: Vec::new(),
            anchor_node: None,
            anchor_offset: 0,
            focus_node: None,
            focus_offset: 0,
            is_collapsed: true,
            selection_type: SelectionType::None,
        }
    }

    // --- Getters ---

    /// Get the anchor node
    pub fn anchor_node(&self) -> Option<NodeId> {
        self.anchor_node
    }

    /// Get the anchor offset
    pub fn anchor_offset(&self) -> u32 {
        self.anchor_offset
    }

    /// Get the focus node
    pub fn focus_node(&self) -> Option<NodeId> {
        self.focus_node
    }

    /// Get the focus offset
    pub fn focus_offset(&self) -> u32 {
        self.focus_offset
    }

    /// Check if selection is collapsed
    pub fn is_collapsed(&self) -> bool {
        self.is_collapsed
    }

    /// Get the number of ranges
    pub fn range_count(&self) -> usize {
        self.ranges.len()
    }

    /// Get the selection type
    pub fn selection_type(&self) -> SelectionType {
        self.selection_type
    }

    /// Get a range by index
    pub fn get_range_at(&self, index: usize) -> Option<&Range> {
        self.ranges.get(index)
    }

    // --- Manipulation ---

    /// Add a range to the selection
    pub fn add_range(&mut self, range: Range) {
        self.ranges.push(range.clone());
        
        // Update anchor and focus
        if self.anchor_node.is_none() {
            self.anchor_node = Some(range.start_container());
            self.anchor_offset = range.start_offset();
        }
        self.focus_node = Some(range.end_container());
        self.focus_offset = range.end_offset();
        
        self.is_collapsed = range.collapsed();
        self.selection_type = if range.collapsed() {
            SelectionType::Caret
        } else {
            SelectionType::Range
        };
    }

    /// Remove a range from the selection
    pub fn remove_range(&mut self, index: usize) {
        if index < self.ranges.len() {
            self.ranges.remove(index);
        }
        if self.ranges.is_empty() {
            self.empty();
        }
    }

    /// Remove all ranges
    pub fn remove_all_ranges(&mut self) {
        self.ranges.clear();
        self.empty();
    }

    /// Empty the selection
    pub fn empty(&mut self) {
        self.ranges.clear();
        self.anchor_node = None;
        self.anchor_offset = 0;
        self.focus_node = None;
        self.focus_offset = 0;
        self.is_collapsed = true;
        self.selection_type = SelectionType::None;
    }

    /// Collapse the selection to a point
    pub fn collapse(&mut self, node: NodeId, offset: u32) {
        self.remove_all_ranges();
        let range = Range::new(node, offset);
        self.add_range(range);
        self.is_collapsed = true;
        self.selection_type = SelectionType::Caret;
    }

    /// Collapse to the start
    pub fn collapse_to_start(&mut self) {
        if let Some(first) = self.ranges.first() {
            let node = first.start_container();
            let offset = first.start_offset();
            self.collapse(node, offset);
        }
    }

    /// Collapse to the end
    pub fn collapse_to_end(&mut self) {
        if let Some(last) = self.ranges.last() {
            let node = last.end_container();
            let offset = last.end_offset();
            self.collapse(node, offset);
        }
    }

    /// Extend the selection to a point
    pub fn extend(&mut self, node: NodeId, offset: u32) {
        self.focus_node = Some(node);
        self.focus_offset = offset;
        
        // Update the last range
        if let Some(last) = self.ranges.last_mut() {
            last.set_end(node, offset);
        }
        
        self.is_collapsed = false;
        self.selection_type = SelectionType::Range;
    }

    /// Select all children of a node
    pub fn select_all_children(&mut self, node: NodeId, child_count: u32) {
        self.remove_all_ranges();
        let mut range = Range::new(node, 0);
        range.select_node_contents(node, child_count);
        self.add_range(range);
    }

    /// Check if the selection contains a node
    pub fn contains_node(&self, node: NodeId, allow_partial: bool) -> bool {
        for range in &self.ranges {
            if range.intersects_node(node) {
                return true;
            }
        }
        false
    }
}

impl Default for Selection {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_range_creation() {
        let range = Range::new(NodeId(1), 5);
        
        assert_eq!(range.start_container(), NodeId(1));
        assert_eq!(range.start_offset(), 5);
        assert_eq!(range.end_container(), NodeId(1));
        assert_eq!(range.end_offset(), 5);
        assert!(range.collapsed());
    }

    #[test]
    fn test_range_between() {
        let range = Range::between(NodeId(1), 5, NodeId(2), 10);
        
        assert_eq!(range.start_container(), NodeId(1));
        assert_eq!(range.start_offset(), 5);
        assert_eq!(range.end_container(), NodeId(2));
        assert_eq!(range.end_offset(), 10);
        assert!(!range.collapsed());
    }

    #[test]
    fn test_range_collapse() {
        let mut range = Range::between(NodeId(1), 0, NodeId(1), 10);
        
        range.collapse(true);
        assert!(range.collapsed());
        assert_eq!(range.end_offset(), 0);
        
        let mut range2 = Range::between(NodeId(1), 0, NodeId(1), 10);
        range2.collapse(false);
        assert!(range2.collapsed());
        assert_eq!(range2.start_offset(), 10);
    }

    #[test]
    fn test_selection_creation() {
        let selection = Selection::new();
        
        assert!(selection.is_collapsed());
        assert_eq!(selection.range_count(), 0);
        assert_eq!(selection.selection_type(), SelectionType::None);
    }

    #[test]
    fn test_selection_add_range() {
        let mut selection = Selection::new();
        let range = Range::between(NodeId(1), 0, NodeId(1), 10);
        
        selection.add_range(range);
        
        assert_eq!(selection.range_count(), 1);
        assert!(!selection.is_collapsed());
        assert_eq!(selection.selection_type(), SelectionType::Range);
    }

    #[test]
    fn test_selection_collapse() {
        let mut selection = Selection::new();
        selection.add_range(Range::between(NodeId(1), 0, NodeId(1), 10));
        
        selection.collapse(NodeId(2), 5);
        
        assert!(selection.is_collapsed());
        assert_eq!(selection.anchor_node(), Some(NodeId(2)));
        assert_eq!(selection.anchor_offset(), 5);
    }

    #[test]
    fn test_boundary_point() {
        let p1 = BoundaryPoint::new(NodeId(1), 5);
        let p2 = BoundaryPoint::new(NodeId(1), 10);
        
        assert!(p1.is_before(&p2));
        assert!(!p2.is_before(&p1));
    }
}
