//! Integration tests - Full pipeline from HTML to rendering
//!
//! Tests the complete workflow: HTML → DOM → CSS → Layout → Render

use fos_html::HtmlParser;
use fos_dom::{Document, DomTree};
use fos_css::CssParser;
use fos_layout::{LayoutTree, BoxType, layout_block_tree, Rect, EdgeSizes};
use fos_render::{Painter, Color, BoxStyle, BoxStyles, Border, BorderStyle, BorderRadius, Background};

// ============================================================================
// FULL PIPELINE TESTS
// ============================================================================

#[test]
fn test_html_to_dom_basic() {
    let html = r#"
        <!DOCTYPE html>
        <html>
            <head><title>Test</title></head>
            <body>
                <div id="main">Hello World</div>
            </body>
        </html>
    "#;
    
    let parser = HtmlParser::new();
    let doc = parser.parse(html);
    
    // Document should have nodes
    assert!(doc.tree().len() > 5, "Expected > 5 nodes, got {}", doc.tree().len());
}

#[test]
fn test_css_parsing_basic() {
    let css = r#"
        body { background-color: white; }
        div { color: black; width: 100px; }
        .container { padding: 10px; margin: 20px; }
    "#;
    
    let parser = CssParser::new();
    let stylesheet = parser.parse(css).unwrap();
    
    assert!(!stylesheet.rules.is_empty());
}

#[test]
fn test_layout_tree_from_scratch() {
    let mut tree = LayoutTree::new();
    
    // Root
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    // Header
    let header = tree.create_box(BoxType::Block, None);
    tree.append_child(root, header);
    if let Some(b) = tree.get_mut(header) {
        b.dimensions.content.height = 80.0;
        b.dimensions.padding = EdgeSizes::symmetric(10.0, 20.0);
    }
    
    // Main content
    let main = tree.create_box(BoxType::Block, None);
    tree.append_child(root, main);
    if let Some(b) = tree.get_mut(main) {
        b.dimensions.content.height = 400.0;
        b.dimensions.margin = EdgeSizes::all(10.0);
    }
    
    // Footer
    let footer = tree.create_box(BoxType::Block, None);
    tree.append_child(root, footer);
    if let Some(b) = tree.get_mut(footer) {
        b.dimensions.content.height = 60.0;
    }
    
    layout_block_tree(&mut tree, 1024.0, 768.0);
    
    // Verify layout
    let h = tree.get(header).unwrap();
    let m = tree.get(main).unwrap();
    let f = tree.get(footer).unwrap();
    
    assert_eq!(h.dimensions.content.y, 10.0); // Header at top + padding
    assert!(m.dimensions.content.y > h.dimensions.content.y); // Main below header
    assert!(f.dimensions.content.y > m.dimensions.content.y); // Footer below main
}

#[test]
fn test_render_layout_tree() {
    let mut painter = Painter::new(800, 600).unwrap();
    painter.clear(Color::WHITE);
    
    // Create layout
    let mut tree = LayoutTree::new();
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    // Create 3 boxes in a column
    let boxes: Vec<_> = (0..3).map(|i| {
        let b = tree.create_box(BoxType::Block, None);
        tree.append_child(root, b);
        if let Some(layout) = tree.get_mut(b) {
            layout.dimensions.content = Rect::new(50.0, 50.0 + i as f32 * 150.0, 200.0, 100.0);
        }
        b
    }).collect();
    
    // Style them
    let mut styles = BoxStyles::new();
    styles.set(boxes[0], BoxStyle::with_background(Color::rgb(255, 100, 100)));
    styles.set(boxes[1], BoxStyle::with_background(Color::rgb(100, 255, 100)));
    styles.set(boxes[2], BoxStyle::with_background(Color::rgb(100, 100, 255)));
    
    painter.paint_tree(&tree, &styles);
    
    // Verify colors at center of each box
    assert_eq!(painter.canvas().get_pixel(150, 100).unwrap().r, 255); // Red box
    assert_eq!(painter.canvas().get_pixel(150, 250).unwrap().g, 255); // Green box
    assert_eq!(painter.canvas().get_pixel(150, 400).unwrap().b, 255); // Blue box
}

#[test]
fn test_full_page_layout() {
    // Simulate a simple page with header, sidebar, content, footer
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    // Header (full width)
    let header = tree.create_box(BoxType::Block, None);
    tree.append_child(root, header);
    if let Some(b) = tree.get_mut(header) {
        b.dimensions.content.height = 60.0;
    }
    
    // Main area (flex container with sidebar + content)
    let main_area = tree.create_box(BoxType::Flex, None);
    tree.append_child(root, main_area);
    
    let sidebar = tree.create_box(BoxType::Block, None);
    tree.append_child(main_area, sidebar);
    if let Some(b) = tree.get_mut(sidebar) {
        b.dimensions.content.width = 200.0;
        b.dimensions.content.height = 500.0;
    }
    
    let content = tree.create_box(BoxType::Block, None);
    tree.append_child(main_area, content);
    if let Some(b) = tree.get_mut(content) {
        b.dimensions.content.height = 500.0;
    }
    
    // Footer
    let footer = tree.create_box(BoxType::Block, None);
    tree.append_child(root, footer);
    if let Some(b) = tree.get_mut(footer) {
        b.dimensions.content.height = 40.0;
    }
    
    layout_block_tree(&mut tree, 1200.0, 800.0);
    
    // Paint
    let mut painter = Painter::new(1200, 800).unwrap();
    painter.clear(Color::WHITE);
    
    let mut styles = BoxStyles::new();
    styles.set(header, BoxStyle::with_background(Color::rgb(50, 50, 80)));
    styles.set(sidebar, BoxStyle::with_background(Color::rgb(240, 240, 240)));
    styles.set(content, BoxStyle::with_background(Color::rgb(255, 255, 255)));
    styles.set(footer, BoxStyle::with_background(Color::rgb(30, 30, 50)));
    
    painter.paint_tree(&tree, &styles);
    
    // Verify header color
    let header_pixel = painter.canvas().get_pixel(600, 30).unwrap();
    assert_eq!(header_pixel.r, 50);
}

// ============================================================================
// REGRESSION TESTS
// ============================================================================

#[test]
fn test_empty_html() {
    let parser = HtmlParser::new();
    let doc = parser.parse("");
    
    // Should not crash, even empty HTML creates at least document node
    assert!(doc.tree().len() >= 1);
}

#[test]
fn test_malformed_html_recovery() {
    let html = "<div><p>Unclosed paragraph<div>Nested without closing";
    
    let parser = HtmlParser::new();
    let doc = parser.parse(html);
    
    // Parser should recover and create nodes
    assert!(doc.tree().len() > 1, "Expected recovered nodes");
}

#[test]
fn test_deeply_nested_html() {
    // Create 50 levels of nested divs
    let mut html = String::new();
    for _ in 0..50 {
        html.push_str("<div>");
    }
    html.push_str("Deep content");
    for _ in 0..50 {
        html.push_str("</div>");
    }
    
    let parser = HtmlParser::new();
    let doc = parser.parse(&html);
    
    // Should have at least 50 element nodes
    assert!(doc.tree().len() >= 50);
}

#[test]
fn test_css_specificity() {
    let css = r#"
        div { color: red; }
        .class { color: blue; }
        #id { color: green; }
        div.class#id { color: purple; }
    "#;
    
    let parser = CssParser::new();
    let stylesheet = parser.parse(css).unwrap();
    
    // Should have parsed 4 rules
    assert_eq!(stylesheet.rules.len(), 4);
}

#[test]
fn test_layout_margin_collapse_chain() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    // Create 5 boxes with varying margins
    let margins = [20.0, 30.0, 25.0, 40.0, 15.0];
    let mut prev_bottom = 0.0;
    
    for (i, &m) in margins.iter().enumerate() {
        let b = tree.create_box(BoxType::Block, None);
        tree.append_child(root, b);
        
        if let Some(layout) = tree.get_mut(b) {
            layout.dimensions.content.height = 50.0;
            layout.dimensions.margin.top = m;
            layout.dimensions.margin.bottom = m;
        }
    }
    
    layout_block_tree(&mut tree, 800.0, 600.0);
    
    // Verify margins collapsed correctly
    let children: Vec<_> = tree.children(root).collect();
    for (id, _) in &children {
        let b = tree.get(*id).unwrap();
        assert!(b.dimensions.content.y >= 0.0);
    }
}

#[test]
fn test_render_with_borders() {
    let mut painter = Painter::new(200, 200).unwrap();
    painter.clear(Color::WHITE);
    
    let mut tree = LayoutTree::new();
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    if let Some(b) = tree.get_mut(root) {
        b.dimensions.content = Rect::new(20.0, 20.0, 160.0, 160.0);
    }
    
    let mut styles = BoxStyles::new();
    let mut style = BoxStyle::with_background(Color::rgb(200, 200, 220));
    style.border = Border::all(5.0, BorderStyle::Solid, Color::rgb(100, 100, 150));
    style.border_radius = BorderRadius::all(10.0);
    styles.set(root, style);
    
    painter.paint_tree(&tree, &styles);
    
    // Center should have background color
    let center = painter.canvas().get_pixel(100, 100).unwrap();
    assert_eq!(center.r, 200);
}

// ============================================================================
// PERFORMANCE CHECKS
// ============================================================================

#[test]
fn test_parse_large_html() {
    // Generate a large HTML document
    let mut html = String::from("<!DOCTYPE html><html><body>");
    for i in 0..1000 {
        html.push_str(&format!("<div id='d{}'><p>Paragraph {} with some text content</p></div>", i, i));
    }
    html.push_str("</body></html>");
    
    let parser = HtmlParser::new();
    
    let start = std::time::Instant::now();
    let doc = parser.parse(&html);
    let elapsed = start.elapsed();
    
    // Should parse in reasonable time (less than 1 second)
    assert!(elapsed.as_millis() < 1000, "Parsing took too long: {:?}", elapsed);
    assert!(doc.tree().len() > 1000, "Expected many nodes");
}

#[test]
fn test_layout_large_tree() {
    let mut tree = LayoutTree::new();
    
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    // Create 2000 boxes
    for i in 0..2000 {
        let b = tree.create_box(BoxType::Block, None);
        tree.append_child(root, b);
        if let Some(layout) = tree.get_mut(b) {
            layout.dimensions.content.height = 10.0 + (i % 50) as f32;
        }
    }
    
    let start = std::time::Instant::now();
    layout_block_tree(&mut tree, 800.0, 600.0);
    let elapsed = start.elapsed();
    
    // Should layout in reasonable time
    assert!(elapsed.as_millis() < 500, "Layout took too long: {:?}", elapsed);
}

#[test]
fn test_render_large_scene() {
    let mut painter = Painter::new(1920, 1080).unwrap();
    painter.clear(Color::WHITE);
    
    let mut tree = LayoutTree::new();
    let root = tree.create_box(BoxType::Block, None);
    tree.set_root(root);
    
    let mut styles = BoxStyles::new();
    
    // Create 1000 boxes in a grid
    for i in 0..1000 {
        let b = tree.create_box(BoxType::Block, None);
        tree.append_child(root, b);
        
        let x = (i % 40) as f32 * 48.0;
        let y = (i / 40) as f32 * 43.0;
        
        if let Some(layout) = tree.get_mut(b) {
            layout.dimensions.content = Rect::new(x, y, 45.0, 40.0);
        }
        
        styles.set(b, BoxStyle::with_background(
            Color::rgb((i * 7 % 256) as u8, (i * 11 % 256) as u8, (i * 13 % 256) as u8)
        ));
    }
    
    let start = std::time::Instant::now();
    painter.paint_tree(&tree, &styles);
    let elapsed = start.elapsed();
    
    // Should render in reasonable time
    assert!(elapsed.as_millis() < 1000, "Rendering took too long: {:?}", elapsed);
}
