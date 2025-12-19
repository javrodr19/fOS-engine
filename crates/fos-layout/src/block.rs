//! Block Layout
//!
//! Implements block formatting context (BFC) layout algorithm.
//! Block boxes stack vertically and expand to fill their container's width.

use crate::{LayoutTree, LayoutBoxId, BoxType, BoxDimensions, EdgeSizes};
use crate::box_model::Rect;

/// Block formatting context
pub struct BlockFormattingContext {
    /// Container width
    container_width: f32,
    /// Current Y position (where next block will be placed)
    cursor_y: f32,
    /// Previous bottom margin (for margin collapsing)
    prev_margin_bottom: f32,
}

impl BlockFormattingContext {
    /// Create a new block formatting context
    pub fn new(container_width: f32, start_y: f32) -> Self {
        Self {
            container_width,
            cursor_y: start_y,
            prev_margin_bottom: 0.0,
        }
    }
    
    /// Layout a block-level box
    pub fn layout_block(
        &mut self,
        tree: &mut LayoutTree,
        box_id: LayoutBoxId,
        containing_width: f32,
        containing_x: f32,
    ) {
        let layout_box = match tree.get_mut(box_id) {
            Some(b) => b,
            None => return,
        };
        
        // Calculate width (block boxes expand to container width by default)
        let dims = &mut layout_box.dimensions;
        
        // For now, assume auto width = container width - horizontal margins
        let content_width = containing_width - dims.margin.horizontal() - 
                           dims.padding.horizontal() - dims.border.horizontal();
        dims.content.width = content_width.max(0.0);
        
        // Calculate X position (centered if auto margins, otherwise left-aligned)
        dims.content.x = containing_x + dims.margin.left + dims.border.left + dims.padding.left;
        
        // Handle margin collapsing
        let margin_top = self.collapse_margins(dims.margin.top);
        
        // Calculate Y position
        dims.content.y = self.cursor_y + margin_top + dims.border.top + dims.padding.top;
        
        // Store children info before mutable borrow
        let first_child = layout_box.first_child;
        let content_x = dims.content.x;
        let content_y = dims.content.y;
        let content_width = dims.content.width;
        
        // Layout children in a nested BFC
        if let Some(first) = first_child {
            // Child container is the content box (already accounts for padding)
            let child_container_width = content_width;
            
            let mut child_bfc = BlockFormattingContext::new(child_container_width, content_y);
            
            let mut child_id = Some(first);
            while let Some(id) = child_id {
                child_bfc.layout_block(tree, id, child_container_width, content_x);
                child_id = tree.get(id).and_then(|b| b.next_sibling);
            }
            
            // Set content height based on children
            if let Some(b) = tree.get_mut(box_id) {
                b.dimensions.content.height = (child_bfc.cursor_y - content_y).max(0.0);
            }
        }
        
        // Update cursor for next sibling
        let dims = &tree.get(box_id).unwrap().dimensions;
        self.cursor_y = dims.content.y + dims.content.height + 
                       dims.padding.bottom + dims.border.bottom;
        self.prev_margin_bottom = dims.margin.bottom;
    }
    
    /// Collapse adjacent vertical margins
    fn collapse_margins(&mut self, margin_top: f32) -> f32 {
        // Adjacent margins collapse to the larger of the two
        let collapsed = margin_top.max(self.prev_margin_bottom);
        self.prev_margin_bottom = 0.0; // Reset after collapsing
        collapsed
    }
    
    /// Get current Y position
    pub fn cursor_y(&self) -> f32 {
        self.cursor_y
    }
}

/// Apply block layout to a tree starting from root
pub fn layout_block_tree(tree: &mut LayoutTree, viewport_width: f32, viewport_height: f32) {
    let root = match tree.root() {
        Some(r) => r,
        None => return,
    };
    
    // Set root dimensions
    if let Some(root_box) = tree.get_mut(root) {
        root_box.dimensions.content.x = 0.0;
        root_box.dimensions.content.y = 0.0;
        root_box.dimensions.content.width = viewport_width;
    }
    
    // Layout root children
    let first_child = tree.get(root).and_then(|b| b.first_child);
    if let Some(first) = first_child {
        let mut bfc = BlockFormattingContext::new(viewport_width, 0.0);
        
        let mut child_id = Some(first);
        while let Some(id) = child_id {
            bfc.layout_block(tree, id, viewport_width, 0.0);
            child_id = tree.get(id).and_then(|b| b.next_sibling);
        }
        
        // Set root height
        if let Some(root_box) = tree.get_mut(root) {
            root_box.dimensions.content.height = bfc.cursor_y();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_block_stacking() {
        let mut tree = LayoutTree::new();
        
        // Create root
        let root = tree.create_box(BoxType::Block, None);
        tree.set_root(root);
        
        // Create two block children
        let child1 = tree.create_box(BoxType::Block, None);
        let child2 = tree.create_box(BoxType::Block, None);
        
        tree.append_child(root, child1);
        tree.append_child(root, child2);
        
        // Set heights
        if let Some(b) = tree.get_mut(child1) {
            b.dimensions.content.height = 100.0;
        }
        if let Some(b) = tree.get_mut(child2) {
            b.dimensions.content.height = 50.0;
        }
        
        // Layout
        layout_block_tree(&mut tree, 800.0, 600.0);
        
        // Check positions
        let c1 = tree.get(child1).unwrap();
        let c2 = tree.get(child2).unwrap();
        
        assert_eq!(c1.dimensions.content.y, 0.0);
        assert_eq!(c1.dimensions.content.width, 800.0);
        
        assert_eq!(c2.dimensions.content.y, 100.0); // Stacked below child1
        assert_eq!(c2.dimensions.content.width, 800.0);
    }
    
    #[test]
    fn test_margin_collapsing() {
        let mut tree = LayoutTree::new();
        
        let root = tree.create_box(BoxType::Block, None);
        tree.set_root(root);
        
        let child1 = tree.create_box(BoxType::Block, None);
        let child2 = tree.create_box(BoxType::Block, None);
        
        tree.append_child(root, child1);
        tree.append_child(root, child2);
        
        // Set heights and margins
        if let Some(b) = tree.get_mut(child1) {
            b.dimensions.content.height = 100.0;
            b.dimensions.margin.bottom = 30.0;
        }
        if let Some(b) = tree.get_mut(child2) {
            b.dimensions.content.height = 50.0;
            b.dimensions.margin.top = 20.0;
        }
        
        layout_block_tree(&mut tree, 800.0, 600.0);
        
        let c2 = tree.get(child2).unwrap();
        // Margins collapse: max(30, 20) = 30
        assert_eq!(c2.dimensions.content.y, 100.0 + 30.0);
    }
    
    #[test]
    fn test_nested_blocks() {
        let mut tree = LayoutTree::new();
        
        let root = tree.create_box(BoxType::Block, None);
        tree.set_root(root);
        
        let parent = tree.create_box(BoxType::Block, None);
        tree.append_child(root, parent);
        
        let child = tree.create_box(BoxType::Block, None);
        tree.append_child(parent, child);
        
        if let Some(b) = tree.get_mut(parent) {
            b.dimensions.padding = EdgeSizes::all(20.0);
        }
        if let Some(b) = tree.get_mut(child) {
            b.dimensions.content.height = 100.0;
        }
        
        layout_block_tree(&mut tree, 800.0, 600.0);
        
        let p = tree.get(parent).unwrap();
        let c = tree.get(child).unwrap();
        
        // Parent width = 800 - 40 (its padding) = 760
        // (block boxes account for their own padding in content width calculation)
        assert_eq!(p.dimensions.content.width, 760.0);
        
        // Child should be inside parent's content box
        // Child width = 760 (parent content width)
        assert_eq!(c.dimensions.content.width, 760.0);
        assert_eq!(c.dimensions.content.x, 20.0); // Offset by parent padding
        assert_eq!(c.dimensions.content.y, 20.0);
    }
}
