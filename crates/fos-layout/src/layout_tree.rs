//! Layout Tree
//!
//! The layout tree is a parallel structure to the DOM tree, representing
//! the visual layout of elements with computed positions and sizes.

use crate::BoxDimensions;
use fos_dom::NodeId;

/// Layout tree - arena of positioned boxes
#[derive(Debug, Default)]
pub struct LayoutTree {
    /// All layout boxes in the tree
    boxes: Vec<LayoutBox>,
    /// Root box index
    root: Option<usize>,
}

impl LayoutTree {
    pub fn new() -> Self {
        Self { 
            boxes: Vec::with_capacity(256),
            root: None,
        }
    }
    
    /// Create a new layout box and return its index
    pub fn create_box(&mut self, box_type: BoxType, dom_node: Option<NodeId>) -> LayoutBoxId {
        let id = LayoutBoxId(self.boxes.len());
        self.boxes.push(LayoutBox {
            dimensions: BoxDimensions::default(),
            box_type,
            dom_node,
            parent: None,
            first_child: None,
            last_child: None,
            next_sibling: None,
            prev_sibling: None,
        });
        id
    }
    
    /// Set the root box
    pub fn set_root(&mut self, id: LayoutBoxId) {
        self.root = Some(id.0);
    }
    
    /// Get the root box ID
    pub fn root(&self) -> Option<LayoutBoxId> {
        self.root.map(LayoutBoxId)
    }
    
    /// Get a layout box by ID
    pub fn get(&self, id: LayoutBoxId) -> Option<&LayoutBox> {
        self.boxes.get(id.0)
    }
    
    /// Get a mutable layout box by ID
    pub fn get_mut(&mut self, id: LayoutBoxId) -> Option<&mut LayoutBox> {
        self.boxes.get_mut(id.0)
    }
    
    /// Append a child box to a parent
    pub fn append_child(&mut self, parent_id: LayoutBoxId, child_id: LayoutBoxId) {
        // Update child's parent
        if let Some(child) = self.boxes.get_mut(child_id.0) {
            child.parent = Some(parent_id);
        }
        
        // Get parent's current last child
        let last_child = self.boxes.get(parent_id.0)
            .and_then(|p| p.last_child);
        
        if let Some(last_id) = last_child {
            // Link with previous last child
            if let Some(last) = self.boxes.get_mut(last_id.0) {
                last.next_sibling = Some(child_id);
            }
            if let Some(child) = self.boxes.get_mut(child_id.0) {
                child.prev_sibling = Some(last_id);
            }
        } else {
            // First child
            if let Some(parent) = self.boxes.get_mut(parent_id.0) {
                parent.first_child = Some(child_id);
            }
        }
        
        // Update parent's last child
        if let Some(parent) = self.boxes.get_mut(parent_id.0) {
            parent.last_child = Some(child_id);
        }
    }
    
    /// Iterate over children of a box
    pub fn children(&self, parent_id: LayoutBoxId) -> ChildIterator<'_> {
        let first = self.get(parent_id).and_then(|b| b.first_child);
        ChildIterator { tree: self, current: first }
    }
    
    /// Number of boxes
    pub fn len(&self) -> usize {
        self.boxes.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.boxes.is_empty()
    }
    
    /// Get hit target at point
    pub fn hit_test(&self, x: f32, y: f32) -> Option<LayoutBoxId> {
        // Start from root and find deepest containing box
        let root = self.root?;
        self.hit_test_box(LayoutBoxId(root), x, y)
    }
    
    fn hit_test_box(&self, id: LayoutBoxId, x: f32, y: f32) -> Option<LayoutBoxId> {
        let layout_box = self.get(id)?;
        let border_box = layout_box.dimensions.border_box();
        
        if !border_box.contains(x, y) {
            return None;
        }
        
        // Check children (reverse order for front-to-back)
        let mut child = layout_box.last_child;
        while let Some(child_id) = child {
            if let Some(hit) = self.hit_test_box(child_id, x, y) {
                return Some(hit);
            }
            child = self.get(child_id).and_then(|c| c.prev_sibling);
        }
        
        // Return this box if no child was hit
        Some(id)
    }
}

/// Layout box identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LayoutBoxId(pub usize);

/// A box in the layout tree
#[derive(Debug)]
pub struct LayoutBox {
    /// Computed dimensions
    pub dimensions: BoxDimensions,
    /// Type of box (block, inline, etc.)
    pub box_type: BoxType,
    /// Link to DOM node (if any - anonymous boxes have None)
    pub dom_node: Option<NodeId>,
    /// Parent box
    pub parent: Option<LayoutBoxId>,
    /// First child box
    pub first_child: Option<LayoutBoxId>,
    /// Last child box
    pub last_child: Option<LayoutBoxId>,
    /// Next sibling box
    pub next_sibling: Option<LayoutBoxId>,
    /// Previous sibling box
    pub prev_sibling: Option<LayoutBoxId>,
}

impl LayoutBox {
    /// Check if this box has children
    pub fn has_children(&self) -> bool {
        self.first_child.is_some()
    }
    
    /// Check if this is an anonymous box
    pub fn is_anonymous(&self) -> bool {
        self.dom_node.is_none()
    }
}

/// Type of layout box
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoxType {
    /// Block-level box
    Block,
    /// Inline-level box
    Inline,
    /// Inline-block box
    InlineBlock,
    /// Flex container
    Flex,
    /// Flex item
    FlexItem,
    /// Grid container (future)
    Grid,
    /// Anonymous block (for text in block context)
    AnonymousBlock,
    /// Anonymous inline (for blocks in inline context)
    AnonymousInline,
    /// Text run
    Text,
}

impl BoxType {
    /// Is this a block-level box?
    pub fn is_block_level(&self) -> bool {
        matches!(self, BoxType::Block | BoxType::Flex | BoxType::Grid | BoxType::AnonymousBlock)
    }
    
    /// Is this an inline-level box?
    pub fn is_inline_level(&self) -> bool {
        matches!(self, BoxType::Inline | BoxType::InlineBlock | BoxType::Text | BoxType::AnonymousInline)
    }
}

/// Iterator over children of a layout box
pub struct ChildIterator<'a> {
    tree: &'a LayoutTree,
    current: Option<LayoutBoxId>,
}

impl<'a> Iterator for ChildIterator<'a> {
    type Item = (LayoutBoxId, &'a LayoutBox);
    
    fn next(&mut self) -> Option<Self::Item> {
        let id = self.current?;
        let layout_box = self.tree.get(id)?;
        self.current = layout_box.next_sibling;
        Some((id, layout_box))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_tree() {
        let mut tree = LayoutTree::new();
        
        let root = tree.create_box(BoxType::Block, None);
        tree.set_root(root);
        
        let child1 = tree.create_box(BoxType::Block, None);
        let child2 = tree.create_box(BoxType::Block, None);
        
        tree.append_child(root, child1);
        tree.append_child(root, child2);
        
        assert_eq!(tree.len(), 3);
        
        let children: Vec<_> = tree.children(root).collect();
        assert_eq!(children.len(), 2);
    }
    
    #[test]
    fn test_hit_test() {
        use crate::box_model::Rect;
        
        let mut tree = LayoutTree::new();
        
        let root = tree.create_box(BoxType::Block, None);
        tree.set_root(root);
        
        // Set dimensions for root
        if let Some(b) = tree.get_mut(root) {
            b.dimensions.content = Rect::new(0.0, 0.0, 800.0, 600.0);
        }
        
        let child = tree.create_box(BoxType::Block, None);
        tree.append_child(root, child);
        
        // Set dimensions for child
        if let Some(b) = tree.get_mut(child) {
            b.dimensions.content = Rect::new(100.0, 100.0, 200.0, 100.0);
        }
        
        // Hit test inside child
        let hit = tree.hit_test(150.0, 120.0);
        assert_eq!(hit, Some(child));
        
        // Hit test outside child but inside root
        let hit = tree.hit_test(50.0, 50.0);
        assert_eq!(hit, Some(root));
        
        // Hit test outside root
        let hit = tree.hit_test(900.0, 700.0);
        assert_eq!(hit, None);
    }
}
