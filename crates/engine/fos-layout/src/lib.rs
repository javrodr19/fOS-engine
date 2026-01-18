//! fOS Layout Engine
//!
//! CSS box model and layout algorithms.
//!
//! This crate transforms a styled DOM tree into a positioned layout tree.
//! It implements:
//! - CSS Box Model (margin, border, padding, content)
//! - Block Formatting Context (vertical stacking, margin collapsing)
//! - Inline Formatting Context (horizontal flow, line wrapping)
//! - Flexbox layout
//! - CSS Grid layout (with subgrid support)
//! - Multi-column layout
//! - Table layout

mod box_model;
mod layout_tree;
mod block;
mod inline;
mod flex;
mod grid;
mod multicolumn;
mod table;
pub mod lazy;
pub mod subgrid;
pub mod streaming_layout;
pub mod layout_cache;
pub mod constraint_cache;
pub mod intrinsic_size_cache;
pub mod parallel_layout;

pub use box_model::{BoxDimensions, EdgeSizes, Rect};
pub use layout_tree::{LayoutTree, LayoutBox, LayoutBoxId, BoxType, ChildIterator};
pub use block::{BlockFormattingContext, layout_block_tree};
pub use inline::{InlineFormattingContext, LineBox, InlineFragment};
pub use flex::{
    layout_flex_container,
    FlexContainerStyle, FlexItemStyle,
    FlexDirection, FlexWrap, FlexBasis,
    JustifyContent, AlignItems, AlignContent,
};
pub use grid::{
    TrackSize, GridTemplate, GridPlacement, GridLine, GridArea,
    GridLayoutContext, resolve_placement, layout_grid_children,
    GridArena,
};
pub use multicolumn::{
    MultiColumnStyle, MultiColumnContext, ColumnRule, ColumnRuleStyle,
    ColumnFill, ColumnSpan,
};
pub use table::{
    TableLayout, BorderCollapse, CaptionSide, EmptyCells, TableStyle,
    CellSpan, TableCell, TableStructure, TableLayoutContext,
    build_table_structure,
};
pub use subgrid::{Subgrid, SubgridContext};
pub use streaming_layout::{
    StreamingLayoutEngine, LayoutChunk, StreamBoxType, IncrementalContext,
    StreamLayoutBox, StreamLayoutStats, LayoutYield, ViewportPriority,
};

use fos_dom::{DomTree, NodeId, Document};
use fos_css::computed::{ComputedStyle, Display};

/// Layout a document and return the layout tree
pub fn layout_document(
    document: &Document,
    styles: &std::collections::HashMap<NodeId, ComputedStyle>,
    viewport_width: f32,
    viewport_height: f32,
) -> LayoutTree {
    let mut tree = LayoutTree::new();
    
    // Create root layout box for viewport
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    if let Some(root_box) = tree.get_mut(root) {
        root_box.dimensions.content.width = viewport_width;
        root_box.dimensions.content.height = viewport_height;
    }
    
    // Build layout boxes from DOM
    let dom = document.tree();
    let body = document.body();
    
    if body.is_valid() {
        build_layout_tree(&mut tree, dom, styles, body, root);
    }
    
    // Perform layout
    layout_block_tree(&mut tree, viewport_width, viewport_height);
    
    tracing::info!("Layout complete: {} boxes", tree.len());
    
    tree
}

/// Build layout tree recursively from DOM
fn build_layout_tree(
    layout_tree: &mut LayoutTree,
    dom: &DomTree,
    styles: &std::collections::HashMap<NodeId, ComputedStyle>,
    node_id: NodeId,
    parent_layout_id: LayoutBoxId,
) {
    let node = match dom.get(node_id) {
        Some(n) => n,
        None => return,
    };
    
    // Get computed style
    let style = styles.get(&node_id);
    
    // Determine box type from display
    let box_type = match style.map(|s| s.display) {
        Some(Display::None) => return, // Don't create box for display:none
        Some(Display::Flex) => BoxType::Flex,
        Some(Display::Grid) => BoxType::Grid,
        Some(Display::Inline) => BoxType::Inline,
        Some(Display::InlineBlock) => BoxType::InlineBlock,
        Some(Display::Block) | Some(Display::Contents) | None => BoxType::Block,
    };
    
    // Create layout box
    let layout_id = layout_tree.create_box(box_type, Some(node_id));
    layout_tree.append_child(parent_layout_id, layout_id);
    
    // Apply style to dimensions
    if let (Some(layout_box), Some(s)) = (layout_tree.get_mut(layout_id), style) {
        apply_style_to_box(layout_box, s);
    }
    
    // Process children
    for (child_id, _) in dom.children(node_id) {
        build_layout_tree(layout_tree, dom, styles, child_id, layout_id);
    }
}

/// Apply computed style to layout box dimensions
fn apply_style_to_box(layout_box: &mut LayoutBox, style: &ComputedStyle) {
    use fos_css::computed::SizeValue;
    
    // Apply margins
    layout_box.dimensions.margin = EdgeSizes {
        top: size_to_px(&style.margin.top),
        right: size_to_px(&style.margin.right),
        bottom: size_to_px(&style.margin.bottom),
        left: size_to_px(&style.margin.left),
    };
    
    // Apply padding
    layout_box.dimensions.padding = EdgeSizes {
        top: size_to_px(&style.padding.top),
        right: size_to_px(&style.padding.right),
        bottom: size_to_px(&style.padding.bottom),
        left: size_to_px(&style.padding.left),
    };
    
    // Apply border widths
    layout_box.dimensions.border = EdgeSizes {
        top: size_to_px(&style.border_width.top),
        right: size_to_px(&style.border_width.right),
        bottom: size_to_px(&style.border_width.bottom),
        left: size_to_px(&style.border_width.left),
    };
    
    // Apply explicit dimensions
    if let SizeValue::Length(v, _) = &style.width {
        layout_box.dimensions.content.width = *v;
    }
    if let SizeValue::Length(v, _) = &style.height {
        layout_box.dimensions.content.height = *v;
    }
}

/// Convert SizeValue to pixels (simplified)
fn size_to_px(size: &fos_css::computed::SizeValue) -> f32 {
    use fos_css::computed::SizeValue;
    use fos_css::properties::LengthUnit;
    
    match size {
        SizeValue::Length(v, unit) => {
            match unit {
                LengthUnit::Px => *v,
                LengthUnit::Em => v * 16.0, // Assuming 16px font
                LengthUnit::Rem => v * 16.0,
                _ => *v, // Approximate
            }
        }
        SizeValue::Auto => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_layout_simple() {
        let mut tree = LayoutTree::new();
        
        let root = tree.create_box(BoxType::Block, None);
        tree.set_root(root);
        
        let child = tree.create_box(BoxType::Block, None);
        tree.append_child(root, child);
        
        if let Some(c) = tree.get_mut(child) {
            c.dimensions.content.height = 100.0;
        }
        
        layout_block_tree(&mut tree, 800.0, 600.0);
        
        let c = tree.get(child).unwrap();
        assert_eq!(c.dimensions.content.width, 800.0);
        assert_eq!(c.dimensions.content.y, 0.0);
    }
}
