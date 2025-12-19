//! Comprehensive edge case tests for fos-layout
//!
//! Tests for edge cases, stress testing, and potential bugs.

use fos_layout::*;

// ============================================================================
// BOX MODEL EDGE CASES
// ============================================================================

#[test]
fn test_rect_zero_size() {
    let rect = Rect::new(10.0, 10.0, 0.0, 0.0);
    // A point at the exact corner is technically "contained" (edge case)
    // This is mathematically correct: 10 >= 10 && 10 <= 10
    assert!(rect.contains(10.0, 10.0)); // Corner point
    assert!(!rect.contains(10.1, 10.0)); // Just outside
    assert!(!rect.contains(10.0, 10.1)); // Just outside
    assert_eq!(rect.right(), 10.0);
    assert_eq!(rect.bottom(), 10.0);
}

#[test]
fn test_rect_negative_coords() {
    let rect = Rect::new(-100.0, -100.0, 50.0, 50.0);
    assert!(rect.contains(-75.0, -75.0));
    assert!(!rect.contains(0.0, 0.0));
}

#[test]
fn test_rect_expand_shrink_roundtrip() {
    let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
    let expanded = rect.expand(10.0, 10.0, 10.0, 10.0);
    let shrunk = expanded.shrink(10.0, 10.0, 10.0, 10.0);
    
    assert_eq!(shrunk.x, rect.x);
    assert_eq!(shrunk.y, rect.y);
    assert_eq!(shrunk.width, rect.width);
    assert_eq!(shrunk.height, rect.height);
}

#[test]
fn test_rect_shrink_below_zero() {
    let rect = Rect::new(0.0, 0.0, 10.0, 10.0);
    let shrunk = rect.shrink(20.0, 20.0, 20.0, 20.0);
    
    // Should clamp to zero, not go negative
    assert_eq!(shrunk.width, 0.0);
    assert_eq!(shrunk.height, 0.0);
}

#[test]
fn test_edge_sizes_zero() {
    let edges = EdgeSizes::all(0.0);
    assert!(edges.is_zero());
    assert_eq!(edges.horizontal(), 0.0);
    assert_eq!(edges.vertical(), 0.0);
}

#[test]
fn test_box_dimensions_zero() {
    let dims = BoxDimensions::default();
    assert_eq!(dims.total_width(), 0.0);
    assert_eq!(dims.total_height(), 0.0);
}

#[test]
fn test_box_dimensions_content_only() {
    let mut dims = BoxDimensions::default();
    dims.content = Rect::new(0.0, 0.0, 100.0, 50.0);
    
    assert_eq!(dims.content_box(), dims.content);
    assert_eq!(dims.padding_box(), dims.content);
    assert_eq!(dims.border_box(), dims.content);
    assert_eq!(dims.margin_box(), dims.content);
}

// ============================================================================
// LAYOUT TREE EDGE CASES
// ============================================================================

#[test]
fn test_empty_tree() {
    let tree = LayoutTree::new();
    assert!(tree.is_empty());
    assert_eq!(tree.len(), 0);
    assert!(tree.root().is_none());
    assert!(tree.hit_test(0.0, 0.0).is_none());
}

#[test]
fn test_single_box_tree() {
    let mut tree = LayoutTree::new();
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    assert_eq!(tree.len(), 1);
    assert_eq!(tree.root(), Some(root));
}

#[test]
fn test_deeply_nested_tree() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    let mut parent = root;
    for _ in 0..100 {
        let child = tree.create_box(BoxType::Block, None);
        tree.append_child(parent, child);
        parent = child;
    }
    
    assert_eq!(tree.len(), 101);
}

#[test]
fn test_wide_tree() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    for _ in 0..500 {
        let child = tree.create_box(BoxType::Block, None);
        tree.append_child(root, child);
    }
    
    let children: Vec<_> = tree.children(root).collect();
    assert_eq!(children.len(), 500);
}

#[test]
fn test_box_types() {
    assert!(BoxType::Block.is_block_level());
    assert!(BoxType::Flex.is_block_level());
    assert!(!BoxType::Block.is_inline_level());
    
    assert!(BoxType::Inline.is_inline_level());
    assert!(BoxType::Text.is_inline_level());
    assert!(!BoxType::Inline.is_block_level());
}

#[test]
fn test_hit_test_overlapping_boxes() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    if let Some(b) = tree.get_mut(root) {
        b.dimensions.content = Rect::new(0.0, 0.0, 400.0, 400.0);
    }
    
    // Create two overlapping children (later child should be hit first)
    let child1 = tree.create_box(BoxType::Block, None);
    let child2 = tree.create_box(BoxType::Block, None);
    
    tree.append_child(root, child1);
    tree.append_child(root, child2);
    
    if let Some(b) = tree.get_mut(child1) {
        b.dimensions.content = Rect::new(0.0, 0.0, 200.0, 200.0);
    }
    if let Some(b) = tree.get_mut(child2) {
        b.dimensions.content = Rect::new(100.0, 100.0, 200.0, 200.0);
    }
    
    // Point at 150, 150 is in both - should hit child2 (later sibling)
    let hit = tree.hit_test(150.0, 150.0);
    assert_eq!(hit, Some(child2));
}

// ============================================================================
// BLOCK LAYOUT EDGE CASES
// ============================================================================

#[test]
fn test_block_empty_children() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    layout_block_tree(&mut tree, 800.0, 600.0);
    
    let r = tree.get(root).unwrap();
    assert_eq!(r.dimensions.content.width, 800.0);
    assert_eq!(r.dimensions.content.height, 0.0);
}

#[test]
fn test_block_zero_viewport() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    let child = tree.create_box(BoxType::Block, None);
    tree.append_child(root, child);
    
    if let Some(b) = tree.get_mut(child) {
        b.dimensions.content.height = 100.0;
    }
    
    layout_block_tree(&mut tree, 0.0, 0.0);
    
    let c = tree.get(child).unwrap();
    assert_eq!(c.dimensions.content.width, 0.0);
}

#[test]
fn test_block_negative_margins() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    let child1 = tree.create_box(BoxType::Block, None);
    let child2 = tree.create_box(BoxType::Block, None);
    
    tree.append_child(root, child1);
    tree.append_child(root, child2);
    
    if let Some(b) = tree.get_mut(child1) {
        b.dimensions.content.height = 100.0;
    }
    if let Some(b) = tree.get_mut(child2) {
        b.dimensions.content.height = 50.0;
        b.dimensions.margin.top = -20.0; // Negative margin pulls up
    }
    
    layout_block_tree(&mut tree, 800.0, 600.0);
    
    // TODO: Negative margins are complex - for now just ensure no crash
    let _ = tree.get(child2).unwrap();
}

#[test]
fn test_block_large_margins() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    let child = tree.create_box(BoxType::Block, None);
    tree.append_child(root, child);
    
    if let Some(b) = tree.get_mut(child) {
        b.dimensions.content.height = 100.0;
        b.dimensions.margin = EdgeSizes::all(1000.0); // Huge margins
    }
    
    layout_block_tree(&mut tree, 800.0, 600.0);
    
    let c = tree.get(child).unwrap();
    // Content width should be clamped to 0 when margins exceed container
    assert!(c.dimensions.content.width >= 0.0);
}

#[test]
fn test_margin_collapse_three_siblings() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    let c1 = tree.create_box(BoxType::Block, None);
    let c2 = tree.create_box(BoxType::Block, None);
    let c3 = tree.create_box(BoxType::Block, None);
    
    tree.append_child(root, c1);
    tree.append_child(root, c2);
    tree.append_child(root, c3);
    
    if let Some(b) = tree.get_mut(c1) {
        b.dimensions.content.height = 50.0;
        b.dimensions.margin.bottom = 30.0;
    }
    if let Some(b) = tree.get_mut(c2) {
        b.dimensions.content.height = 50.0;
        b.dimensions.margin.top = 20.0;
        b.dimensions.margin.bottom = 40.0;
    }
    if let Some(b) = tree.get_mut(c3) {
        b.dimensions.content.height = 50.0;
        b.dimensions.margin.top = 25.0;
    }
    
    layout_block_tree(&mut tree, 800.0, 600.0);
    
    let b1 = tree.get(c1).unwrap();
    let b2 = tree.get(c2).unwrap();
    let b3 = tree.get(c3).unwrap();
    
    // c2 should be at c1 bottom + max(30, 20) = 50 + 30 = 80
    assert_eq!(b2.dimensions.content.y, 50.0 + 30.0);
    
    // c3 should be at c2 bottom + max(40, 25) = 130 + 40 = 170
    assert_eq!(b3.dimensions.content.y, 50.0 + 30.0 + 50.0 + 40.0);
}

// ============================================================================
// INLINE LAYOUT EDGE CASES
// ============================================================================

#[test]
fn test_inline_empty_text() {
    let mut tree = LayoutTree::new();
    let text_box = tree.create_box(BoxType::Text, None);
    
    let mut ifc = InlineFormattingContext::new(100.0, 0.0, 0.0, 16.0);
    ifc.add_text(&mut tree, text_box, "", 8.0, 16.0);
    
    let lines = ifc.finish();
    assert!(lines.is_empty() || lines[0].fragments.is_empty());
}

#[test]
fn test_inline_long_word() {
    let mut tree = LayoutTree::new();
    let text_box = tree.create_box(BoxType::Text, None);
    
    let mut ifc = InlineFormattingContext::new(100.0, 0.0, 0.0, 16.0);
    // Word longer than container
    ifc.add_text(&mut tree, text_box, "supercalifragilisticexpialidocious", 8.0, 16.0);
    
    let lines = ifc.finish();
    // Should wrap onto multiple lines
    assert!(lines.len() >= 2);
}

#[test]
fn test_inline_many_small_boxes() {
    let mut tree = LayoutTree::new();
    
    let mut ifc = InlineFormattingContext::new(100.0, 0.0, 0.0, 16.0);
    
    for _ in 0..100 {
        let inline = tree.create_box(BoxType::Inline, None);
        ifc.add_inline_box(&mut tree, inline, 5.0, 16.0);
    }
    
    let lines = ifc.finish();
    // 100 boxes * 5px = 500px, should wrap onto multiple lines
    assert!(lines.len() >= 5);
}

#[test]
fn test_inline_varying_heights() {
    let mut tree = LayoutTree::new();
    
    let mut ifc = InlineFormattingContext::new(1000.0, 0.0, 0.0, 16.0);
    
    let small = tree.create_box(BoxType::Inline, None);
    let large = tree.create_box(BoxType::Inline, None);
    
    ifc.add_inline_box(&mut tree, small, 50.0, 16.0);
    ifc.add_inline_box(&mut tree, large, 50.0, 48.0);
    
    let lines = ifc.finish();
    
    // Line height should be max of children (48.0)
    assert_eq!(lines[0].height, 48.0);
}

// ============================================================================
// FLEXBOX EDGE CASES
// ============================================================================

#[test]
fn test_flex_empty_container() {
    let mut tree = LayoutTree::new();
    
    let container = tree.create_box(BoxType::Flex, None);
    if let Some(c) = tree.get_mut(container) {
        c.dimensions.content = Rect::new(0.0, 0.0, 300.0, 100.0);
    }
    
    let style = FlexContainerStyle::default();
    layout_flex_container(&mut tree, container, style, &[]);
    
    // Should not crash with empty items
}

#[test]
fn test_flex_single_item() {
    let mut tree = LayoutTree::new();
    
    let container = tree.create_box(BoxType::Flex, None);
    if let Some(c) = tree.get_mut(container) {
        c.dimensions.content = Rect::new(0.0, 0.0, 300.0, 100.0);
    }
    
    let item = tree.create_box(BoxType::FlexItem, None);
    if let Some(b) = tree.get_mut(item) {
        b.dimensions.content = Rect::new(0.0, 0.0, 100.0, 50.0);
    }
    
    let style = FlexContainerStyle::default();
    let items = vec![(item, FlexItemStyle::default())];
    
    layout_flex_container(&mut tree, container, style, &items);
    
    let b = tree.get(item).unwrap();
    assert_eq!(b.dimensions.content.x, 0.0);
}

#[test]
fn test_flex_column_direction() {
    let mut tree = LayoutTree::new();
    
    let container = tree.create_box(BoxType::Flex, None);
    if let Some(c) = tree.get_mut(container) {
        c.dimensions.content = Rect::new(0.0, 0.0, 300.0, 300.0);
    }
    
    let item1 = tree.create_box(BoxType::FlexItem, None);
    let item2 = tree.create_box(BoxType::FlexItem, None);
    
    if let Some(b) = tree.get_mut(item1) {
        b.dimensions.content = Rect::new(0.0, 0.0, 100.0, 50.0);
    }
    if let Some(b) = tree.get_mut(item2) {
        b.dimensions.content = Rect::new(0.0, 0.0, 100.0, 50.0);
    }
    
    let style = FlexContainerStyle {
        direction: FlexDirection::Column,
        ..Default::default()
    };
    
    let items = vec![
        (item1, FlexItemStyle::default()),
        (item2, FlexItemStyle::default()),
    ];
    
    layout_flex_container(&mut tree, container, style, &items);
    
    let b1 = tree.get(item1).unwrap();
    let b2 = tree.get(item2).unwrap();
    
    // Items should stack vertically
    assert_eq!(b1.dimensions.content.y, 0.0);
    assert_eq!(b2.dimensions.content.y, 50.0);
}

#[test]
fn test_flex_space_between() {
    let mut tree = LayoutTree::new();
    
    let container = tree.create_box(BoxType::Flex, None);
    if let Some(c) = tree.get_mut(container) {
        c.dimensions.content = Rect::new(0.0, 0.0, 300.0, 100.0);
    }
    
    let item1 = tree.create_box(BoxType::FlexItem, None);
    let item2 = tree.create_box(BoxType::FlexItem, None);
    
    if let Some(b) = tree.get_mut(item1) {
        b.dimensions.content = Rect::new(0.0, 0.0, 50.0, 50.0);
    }
    if let Some(b) = tree.get_mut(item2) {
        b.dimensions.content = Rect::new(0.0, 0.0, 50.0, 50.0);
    }
    
    let style = FlexContainerStyle {
        justify_content: JustifyContent::SpaceBetween,
        ..Default::default()
    };
    
    let items = vec![
        (item1, FlexItemStyle::default()),
        (item2, FlexItemStyle::default()),
    ];
    
    layout_flex_container(&mut tree, container, style, &items);
    
    let b1 = tree.get(item1).unwrap();
    let b2 = tree.get(item2).unwrap();
    
    // First item at start, second at end (300 - 50 = 250)
    assert_eq!(b1.dimensions.content.x, 0.0);
    assert_eq!(b2.dimensions.content.x, 250.0);
}

#[test]
fn test_flex_shrink() {
    let mut tree = LayoutTree::new();
    
    let container = tree.create_box(BoxType::Flex, None);
    if let Some(c) = tree.get_mut(container) {
        c.dimensions.content = Rect::new(0.0, 0.0, 100.0, 100.0);
    }
    
    let item1 = tree.create_box(BoxType::FlexItem, None);
    let item2 = tree.create_box(BoxType::FlexItem, None);
    
    // Items want more space than container has
    if let Some(b) = tree.get_mut(item1) {
        b.dimensions.content = Rect::new(0.0, 0.0, 80.0, 50.0);
    }
    if let Some(b) = tree.get_mut(item2) {
        b.dimensions.content = Rect::new(0.0, 0.0, 80.0, 50.0);
    }
    
    let style = FlexContainerStyle::default();
    let items = vec![
        (item1, FlexItemStyle { shrink: 1.0, ..Default::default() }),
        (item2, FlexItemStyle { shrink: 1.0, ..Default::default() }),
    ];
    
    layout_flex_container(&mut tree, container, style, &items);
    
    let b1 = tree.get(item1).unwrap();
    let b2 = tree.get(item2).unwrap();
    
    // Both should shrink to fit
    assert!(b1.dimensions.content.width < 80.0);
    assert!(b2.dimensions.content.width < 80.0);
    assert!((b1.dimensions.content.width + b2.dimensions.content.width - 100.0).abs() < 0.1);
}

// ============================================================================
// INTEGRATION TESTS
// ============================================================================

#[test]
fn test_mixed_layout() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    // Block -> Flex -> Items
    let header = tree.create_box(BoxType::Flex, None);
    let main_content = tree.create_box(BoxType::Block, None);
    let footer = tree.create_box(BoxType::Block, None);
    
    tree.append_child(root, header);
    tree.append_child(root, main_content);
    tree.append_child(root, footer);
    
    if let Some(b) = tree.get_mut(header) {
        b.dimensions.content.height = 60.0;
    }
    if let Some(b) = tree.get_mut(main_content) {
        b.dimensions.content.height = 400.0;
    }
    if let Some(b) = tree.get_mut(footer) {
        b.dimensions.content.height = 40.0;
    }
    
    layout_block_tree(&mut tree, 1200.0, 800.0);
    
    let h = tree.get(header).unwrap();
    let m = tree.get(main_content).unwrap();
    let f = tree.get(footer).unwrap();
    
    assert_eq!(h.dimensions.content.y, 0.0);
    assert_eq!(m.dimensions.content.y, 60.0);
    assert_eq!(f.dimensions.content.y, 460.0);
}

#[test]
fn test_stress_large_tree() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    // Create 1000 nested sections with content
    for _ in 0..100 {
        let section = tree.create_box(BoxType::Block, None);
        tree.append_child(root, section);
        
        for _ in 0..10 {
            let item = tree.create_box(BoxType::Block, None);
            tree.append_child(section, item);
            
            if let Some(b) = tree.get_mut(item) {
                b.dimensions.content.height = 20.0;
            }
        }
    }
    
    layout_block_tree(&mut tree, 800.0, 600.0);
    
    // Should have 1 root + 100 sections + 1000 items
    assert_eq!(tree.len(), 1101);
    
    // Root height should be sum of all content
    let r = tree.get(root).unwrap();
    assert!(r.dimensions.content.height > 0.0);
}
