//! Comprehensive edge case tests for fos-render
//!
//! Tests for edge cases, stress testing, and potential bugs.

use fos_render::*;

// ============================================================================
// COLOR EDGE CASES
// ============================================================================

#[test]
fn test_color_equality() {
    assert_eq!(Color::rgb(255, 0, 0), Color::RED);
    assert_ne!(Color::rgb(255, 0, 0), Color::rgb(254, 0, 0));
}

#[test]
fn test_color_from_hex_edge_cases() {
    // Valid formats
    assert!(Color::from_hex("#000000").is_some());
    assert!(Color::from_hex("ffffff").is_some());
    assert!(Color::from_hex("#fff").is_some());
    assert!(Color::from_hex("abc").is_some());
    assert!(Color::from_hex("#00ff00ff").is_some()); // With alpha
    
    // Invalid formats
    assert!(Color::from_hex("").is_none());
    assert!(Color::from_hex("#").is_none());
    assert!(Color::from_hex("#gg0000").is_none());
    assert!(Color::from_hex("#12345").is_none()); // Wrong length
    assert!(Color::from_hex("1234567890").is_none());
}

#[test]
fn test_color_transparency() {
    let transparent = Color::rgba(100, 100, 100, 0);
    assert_eq!(transparent.a, 0);
    
    let semi = Color::rgba(100, 100, 100, 128);
    assert_eq!(semi.a, 128);
}

// ============================================================================
// CANVAS EDGE CASES
// ============================================================================

#[test]
fn test_canvas_zero_size() {
    // Zero dimensions should fail
    let canvas = Canvas::new(0, 0);
    assert!(canvas.is_none());
    
    let canvas = Canvas::new(0, 100);
    assert!(canvas.is_none());
    
    let canvas = Canvas::new(100, 0);
    assert!(canvas.is_none());
}

#[test]
fn test_canvas_large_size() {
    // Reasonable large size should work
    let canvas = Canvas::new(4096, 4096);
    assert!(canvas.is_some());
}

#[test]
fn test_canvas_clear_colors() {
    let mut canvas = Canvas::new(10, 10).unwrap();
    
    canvas.clear(Color::RED);
    assert_eq!(canvas.get_pixel(5, 5).unwrap().r, 255);
    
    canvas.clear(Color::GREEN);
    assert_eq!(canvas.get_pixel(5, 5).unwrap().g, 255);
    
    canvas.clear(Color::BLUE);
    assert_eq!(canvas.get_pixel(5, 5).unwrap().b, 255);
}

#[test]
fn test_canvas_get_pixel_out_of_bounds() {
    let canvas = Canvas::new(100, 100).unwrap();
    
    assert!(canvas.get_pixel(0, 0).is_some());
    assert!(canvas.get_pixel(99, 99).is_some());
    assert!(canvas.get_pixel(100, 0).is_none()); // Out of bounds
    assert!(canvas.get_pixel(0, 100).is_none());
    assert!(canvas.get_pixel(500, 500).is_none());
}

#[test]
fn test_fill_rect_zero_size() {
    let mut canvas = Canvas::new(100, 100).unwrap();
    canvas.clear(Color::WHITE);
    
    // Zero-size rects should not crash
    canvas.fill_rect(10.0, 10.0, 0.0, 0.0, Color::RED);
    canvas.fill_rect(10.0, 10.0, 0.0, 50.0, Color::RED);
    canvas.fill_rect(10.0, 10.0, 50.0, 0.0, Color::RED);
    
    // Canvas should still be white
    assert_eq!(canvas.get_pixel(50, 50).unwrap(), Color::WHITE);
}

#[test]
fn test_fill_rect_negative_size() {
    let mut canvas = Canvas::new(100, 100).unwrap();
    canvas.clear(Color::WHITE);
    
    // Negative-size rects should not crash
    canvas.fill_rect(50.0, 50.0, -10.0, -10.0, Color::RED);
    
    // Canvas should still be white
    assert_eq!(canvas.get_pixel(45, 45).unwrap(), Color::WHITE);
}

#[test]
fn test_fill_rect_outside_canvas() {
    let mut canvas = Canvas::new(100, 100).unwrap();
    canvas.clear(Color::WHITE);
    
    // Rect completely outside should not crash
    canvas.fill_rect(200.0, 200.0, 50.0, 50.0, Color::RED);
    canvas.fill_rect(-100.0, -100.0, 50.0, 50.0, Color::RED);
    
    // Canvas should still be white
    assert_eq!(canvas.get_pixel(50, 50).unwrap(), Color::WHITE);
}

#[test]
fn test_fill_rect_partial_overlap() {
    let mut canvas = Canvas::new(100, 100).unwrap();
    canvas.clear(Color::WHITE);
    
    // Rect with negative start but overlapping canvas
    canvas.fill_rect(-10.0, -10.0, 50.0, 50.0, Color::RED);
    
    // Part inside canvas should be red
    let pixel = canvas.get_pixel(10, 10).unwrap();
    assert_eq!(pixel.r, 255);
}

#[test]
fn test_rounded_rect_radius_larger_than_size() {
    let mut canvas = Canvas::new(100, 100).unwrap();
    canvas.clear(Color::WHITE);
    
    // Radius larger than rect should be clamped
    canvas.fill_rounded_rect(10.0, 10.0, 20.0, 20.0, 50.0, Color::BLUE);
    
    // Should not crash
}

#[test]
fn test_stroke_rect_zero_width() {
    let mut canvas = Canvas::new(100, 100).unwrap();
    canvas.clear(Color::WHITE);
    
    // Zero stroke width should not crash
    canvas.stroke_rect(10.0, 10.0, 50.0, 50.0, 0.0, Color::BLACK);
}

#[test]
fn test_draw_line_same_point() {
    let mut canvas = Canvas::new(100, 100).unwrap();
    canvas.clear(Color::WHITE);
    
    // Line from point to same point should not crash
    canvas.draw_line(50.0, 50.0, 50.0, 50.0, 2.0, Color::RED);
}

// ============================================================================
// BORDER EDGE CASES
// ============================================================================

#[test]
fn test_border_no_visible() {
    let border = Border::default();
    assert!(!border.has_visible());
}

#[test]
fn test_border_zero_width() {
    let border = Border::all(0.0, BorderStyle::Solid, Color::BLACK);
    assert!(!border.has_visible());
}

#[test]
fn test_border_hidden_style() {
    let border = Border::all(5.0, BorderStyle::Hidden, Color::BLACK);
    assert!(!border.has_visible());
}

#[test]
fn test_border_radius_edge_cases() {
    let zero = BorderRadius::default();
    assert!(!zero.has_radius());
    
    let small = BorderRadius::all(0.001);
    assert!(small.has_radius());
    
    let large = BorderRadius::all(1000.0);
    assert_eq!(large.max(), 1000.0);
}

// ============================================================================
// BACKGROUND EDGE CASES
// ============================================================================

#[test]
fn test_background_transparent() {
    let bg = Background::color(Color::TRANSPARENT);
    assert!(!bg.is_visible());
}

#[test]
fn test_background_semi_transparent() {
    let bg = Background::color(Color::rgba(255, 0, 0, 128));
    assert!(bg.is_visible());
}

#[test]
fn test_background_empty() {
    let bg = Background::default();
    assert!(!bg.is_visible());
}

// ============================================================================
// PAINTER EDGE CASES
// ============================================================================

#[test]
fn test_painter_empty_tree() {
    let mut painter = Painter::new(100, 100).unwrap();
    painter.clear(Color::WHITE);
    
    let tree = fos_layout::LayoutTree::new();
    let styles = BoxStyles::new();
    
    // Empty tree should not crash
    painter.paint_tree(&tree, &styles);
}

#[test]
fn test_painter_tree_with_no_styles() {
    let mut painter = Painter::new(100, 100).unwrap();
    painter.clear(Color::WHITE);
    
    let mut tree = fos_layout::LayoutTree::new();
    let root = tree.create_box(fos_layout::BoxType::Block, None);
    tree.set_root(root);
    
    let styles = BoxStyles::new(); // Empty styles
    
    // Should paint without crashing (no visible styles = nothing painted)
    painter.paint_tree(&tree, &styles);
}

#[test]
fn test_painter_nested_boxes() {
    let mut painter = Painter::new(200, 200).unwrap();
    painter.clear(Color::WHITE);
    
    let mut tree = fos_layout::LayoutTree::new();
    let root = tree.create_box(fos_layout::BoxType::Block, None);
    tree.set_root(root);
    
    // Create 10 nested children
    let mut parent = root;
    for i in 0..10 {
        let child = tree.create_box(fos_layout::BoxType::Block, None);
        tree.append_child(parent, child);
        
        if let Some(b) = tree.get_mut(child) {
            let offset = i as f32 * 10.0;
            b.dimensions.content = fos_layout::Rect::new(offset, offset, 100.0, 100.0);
        }
        parent = child;
    }
    
    let mut styles = BoxStyles::new();
    
    // Just set style for deepest child
    styles.set(parent, BoxStyle::with_background(Color::rgb(100, 50, 150)));
    
    painter.paint_tree(&tree, &styles);
    
    // Should have painted the deepest box
    let pixel = painter.canvas().get_pixel(120, 120).unwrap();
    assert_eq!(pixel.r, 100);
    assert_eq!(pixel.g, 50);
    assert_eq!(pixel.b, 150);
}

#[test]
fn test_painter_overlapping_boxes() {
    let mut painter = Painter::new(100, 100).unwrap();
    painter.clear(Color::WHITE);
    
    let mut tree = fos_layout::LayoutTree::new();
    let root = tree.create_box(fos_layout::BoxType::Block, None);
    tree.set_root(root);
    
    let box1 = tree.create_box(fos_layout::BoxType::Block, None);
    let box2 = tree.create_box(fos_layout::BoxType::Block, None);
    
    tree.append_child(root, box1);
    tree.append_child(root, box2);
    
    // Overlapping boxes
    if let Some(b) = tree.get_mut(box1) {
        b.dimensions.content = fos_layout::Rect::new(10.0, 10.0, 50.0, 50.0);
    }
    if let Some(b) = tree.get_mut(box2) {
        b.dimensions.content = fos_layout::Rect::new(30.0, 30.0, 50.0, 50.0);
    }
    
    let mut styles = BoxStyles::new();
    styles.set(box1, BoxStyle::with_background(Color::RED));
    styles.set(box2, BoxStyle::with_background(Color::BLUE));
    
    painter.paint_tree(&tree, &styles);
    
    // Overlap area should be blue (painted second)
    let pixel = painter.canvas().get_pixel(40, 40).unwrap();
    assert_eq!(pixel.b, 255);
}

// ============================================================================
// STRESS TESTS
// ============================================================================

#[test]
fn test_stress_many_rects() {
    let mut canvas = Canvas::new(500, 500).unwrap();
    canvas.clear(Color::WHITE);
    
    // Draw 1000 rectangles
    for i in 0..1000 {
        let x = (i % 50) as f32 * 10.0;
        let y = (i / 50) as f32 * 25.0;
        canvas.fill_rect(x, y, 8.0, 20.0, Color::rgb((i % 256) as u8, 100, 200));
    }
}

#[test]
fn test_stress_many_boxes() {
    let mut painter = Painter::new(800, 600).unwrap();
    painter.clear(Color::WHITE);
    
    let mut tree = fos_layout::LayoutTree::new();
    let root = tree.create_box(fos_layout::BoxType::Block, None);
    tree.set_root(root);
    
    let mut styles = BoxStyles::new();
    
    // Create 500 child boxes
    for i in 0..500 {
        let child = tree.create_box(fos_layout::BoxType::Block, None);
        tree.append_child(root, child);
        
        if let Some(b) = tree.get_mut(child) {
            let x = (i % 20) as f32 * 40.0;
            let y = (i / 20) as f32 * 24.0;
            b.dimensions.content = fos_layout::Rect::new(x, y, 35.0, 20.0);
        }
        
        styles.set(child, BoxStyle::with_background(
            Color::rgb((i * 3 % 256) as u8, (i * 5 % 256) as u8, (i * 7 % 256) as u8)
        ));
    }
    
    painter.paint_tree(&tree, &styles);
    
    // Should have painted all 500 boxes
}

#[test]
fn test_rgba_bytes_output() {
    let mut canvas = Canvas::new(2, 2).unwrap();
    canvas.fill_rect(0.0, 0.0, 2.0, 2.0, Color::rgb(100, 150, 200));
    
    let bytes = canvas.as_rgba_bytes();
    
    // 2x2 canvas = 4 pixels = 16 bytes
    assert_eq!(bytes.len(), 16);
    
    // First pixel RGBA
    assert_eq!(bytes[0], 100); // R
    assert_eq!(bytes[1], 150); // G
    assert_eq!(bytes[2], 200); // B
    assert_eq!(bytes[3], 255); // A
}
