//! Rendering Pipeline
//!
//! Integrates fos-engine components for rendering web pages.

use std::collections::HashMap;
use fos_dom::{Document, NodeId, DomTree};
use fos_css::computed::{ComputedStyle, Display, SizeValue, EdgeSizes};
use fos_css::properties::{LengthUnit, Color as CssColor};
use fos_layout::{LayoutTree, LayoutBoxId, layout_document};
use fos_render::{Canvas, Color};

/// Rendered page with pixel buffer
pub struct RenderedPage {
    /// Pixel buffer (RGBA)
    pub pixels: Vec<u8>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Content height (for scroll)
    pub content_height: f32,
}

/// Page renderer - integrates HTML, CSS, layout, and painting
pub struct PageRenderer {
    /// Viewport width
    viewport_width: u32,
    /// Viewport height
    viewport_height: u32,
}

impl PageRenderer {
    pub fn new(viewport_width: u32, viewport_height: u32) -> Self {
        Self {
            viewport_width,
            viewport_height,
        }
    }
    
    /// Set viewport size
    pub fn set_viewport(&mut self, width: u32, height: u32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }
    
    /// Render HTML to pixels
    pub fn render_html(&self, html: &str, base_url: &str) -> Option<RenderedPage> {
        // 1. Parse HTML into DOM
        let document = fos_html::parse_with_url(html, base_url);
        
        // 2. Compute styles for all elements
        let styles = self.compute_styles(&document);
        
        // 3. Layout the document
        let layout_tree = layout_document(
            &document,
            &styles,
            self.viewport_width as f32,
            self.viewport_height as f32,
        );
        
        // 4. Paint to canvas
        let pixels = self.paint(&document, &styles, &layout_tree)?;
        
        // Calculate content height
        let content_height = self.calculate_content_height(&layout_tree);
        
        Some(RenderedPage {
            pixels,
            width: self.viewport_width,
            height: self.viewport_height,
            content_height,
        })
    }
    
    /// Compute styles for all elements
    fn compute_styles(&self, document: &Document) -> HashMap<NodeId, ComputedStyle> {
        let mut styles = HashMap::new();
        
        // Get DOm tree
        let tree = document.tree();
        
        // Iterate through all nodes and compute styles
        self.compute_styles_recursive(tree, tree.root(), &mut styles);
        
        styles
    }
    
    /// Recursively compute styles
    fn compute_styles_recursive(
        &self, 
        tree: &DomTree, 
        node_id: NodeId, 
        styles: &mut HashMap<NodeId, ComputedStyle>
    ) {
        if !node_id.is_valid() {
            return;
        }
        
        // Create default computed style
        let mut style = ComputedStyle::default();
        
        // Get node to check for inline styles and element type
        if let Some(node) = tree.get(node_id) {
            if let Some(element) = node.as_element() {
                // Get tag name
                let tag_name = tree.resolve(element.name.local);
                
                // Apply default styles based on element type
                apply_default_styles(&mut style, tag_name);
                
                // Parse inline style attribute if present
                // Note: element.get_attribute is not available, we'll skip inline styles for now
            }
        }
        
        styles.insert(node_id, style);
        
        // Process children
        for (child_id, _) in tree.children(node_id) {
            self.compute_styles_recursive(tree, child_id, styles);
        }
    }
    
    /// Paint the layout tree
    fn paint(
        &self,
        document: &Document,
        styles: &HashMap<NodeId, ComputedStyle>,
        layout_tree: &LayoutTree,
    ) -> Option<Vec<u8>> {
        // Create canvas
        let mut canvas = Canvas::new(self.viewport_width, self.viewport_height)?;
        
        // Fill with white background
        canvas.clear(Color::WHITE);
        
        // Paint using a simple DOM-based approach
        // Walk the DOM tree and paint text directly
        let tree = document.tree();
        let body = document.body();
        
        log::info!("DOM tree size: {}, body valid: {}", tree.len(), body.is_valid());
        
        if body.is_valid() {
            let mut y_cursor = 20.0f32; // Start position
            self.paint_dom_node(&mut canvas, tree, body, styles, 8.0, &mut y_cursor);
            
            // If no content was painted, show a message
            if y_cursor < 30.0 {
                log::warn!("No text found in DOM, drawing fallback");
                self.paint_text(&mut canvas, "Page loaded but no visible content", 20.0, 50.0, Color::rgb(100, 100, 100), 16.0);
            }
        } else {
            // Body not valid - paint error message
            log::error!("Body element not valid!");
            self.paint_text(&mut canvas, "Error: Could not find body element", 20.0, 50.0, Color::rgb(200, 50, 50), 16.0);
        }
        
        // Get pixels as RGBA bytes
        Some(canvas.as_rgba_bytes())
    }
    
    /// Paint a DOM node and its children (simple vertical layout)
    fn paint_dom_node(
        &self,
        canvas: &mut Canvas,
        tree: &DomTree,
        node_id: NodeId,
        styles: &HashMap<NodeId, ComputedStyle>,
        x_offset: f32,
        y_cursor: &mut f32,
    ) {
        let node = match tree.get(node_id) {
            Some(n) => n,
            None => return,
        };
        
        // Get style
        let style = styles.get(&node_id);
        
        // Check if hidden
        if let Some(s) = style {
            if matches!(s.display, Display::None) {
                return;
            }
        }
        
        // If text node, paint it
        if let Some(text) = node.as_text() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                // Text nodes don't have their own style - use parent's or defaults
                // Ensure minimum readable font size
                let font_size = style.map(|s| s.font_size.max(14.0)).unwrap_or(16.0);
                let text_color = style
                    .map(|s| css_color_to_render(&s.color))
                    .unwrap_or(Color::BLACK);
                
                self.paint_text(canvas, trimmed, x_offset, *y_cursor, text_color, font_size);
                *y_cursor += font_size * 1.5; // Line spacing
            }
            return;
        }
        
        // If element, check for block vs inline behavior
        if let Some(element) = node.as_element() {
            let tag = tree.resolve(element.name.local);
            let is_block = matches!(tag.to_lowercase().as_str(), 
                "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | 
                "ul" | "ol" | "li" | "section" | "article" | "header" | "footer" | "main");
            
            let font_size = style.map(|s| s.font_size).unwrap_or(16.0);
            
            // Add margin for block elements
            if is_block {
                *y_cursor += font_size * 0.5;
            }
            
            // Recurse into children
            for (child_id, _) in tree.children(node_id) {
                // Pass parent style to text children
                let child_style = styles.get(&child_id).or(style);
                let child_styles: HashMap<NodeId, ComputedStyle> = if let Some(s) = child_style {
                    let mut m = HashMap::new();
                    m.insert(child_id, s.clone());
                    m
                } else {
                    HashMap::new()
                };
                
                self.paint_dom_node(canvas, tree, child_id, if child_styles.is_empty() { styles } else { &child_styles }, x_offset, y_cursor);
            }
            
            // Add margin after block elements
            if is_block {
                *y_cursor += font_size * 0.5;
            }
        }
    }
    
    /// Paint a single layout box and its children
    fn paint_box(
        &self,
        canvas: &mut Canvas,
        layout_tree: &LayoutTree,
        box_id: LayoutBoxId,
        document: &Document,
        styles: &HashMap<NodeId, ComputedStyle>,
    ) {
        let layout_box = match layout_tree.get(box_id) {
            Some(b) => b,
            None => return,
        };
        
        let dims = &layout_box.dimensions;
        
        // Get style for this box
        let style = layout_box.dom_node
            .and_then(|id| styles.get(&id));
        
        // Get background color
        let bg_color = style
            .map(|s| css_color_to_render(&s.background_color))
            .unwrap_or(Color::TRANSPARENT);
        
        // Paint background if not transparent
        if bg_color.a > 0 {
            canvas.fill_rect(
                dims.content.x,
                dims.content.y,
                dims.content.width,
                dims.content.height,
                bg_color,
            );
        }
        
        // Paint text content
        // Iterate through DOM children of this node to find text nodes
        if let Some(node_id) = layout_box.dom_node {
            let text_color = style
                .map(|s| css_color_to_render(&s.color))
                .unwrap_or(Color::BLACK);
            let font_size = style.map(|s| s.font_size).unwrap_or(16.0);
            
            let mut y_offset = dims.content.y + font_size;
            
            // Collect all text from this element and its children
            let text = self.collect_text_content(document, node_id);
            if !text.is_empty() {
                // Debug: log text being painted
                static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if !LOGGED.load(std::sync::atomic::Ordering::Relaxed) {
                    LOGGED.store(true, std::sync::atomic::Ordering::Relaxed);
                    log::info!("Painting text: '{}' at ({}, {})", 
                        if text.len() > 50 { &text[..50] } else { &text }, 
                        dims.content.x, y_offset);
                }
                
                self.paint_text(
                    canvas,
                    &text,
                    dims.content.x,
                    y_offset,
                    text_color,
                    font_size,
                );
            }
        }
        
        // Paint layout children
        for (child_id, _) in layout_tree.children(box_id) {
            self.paint_box(canvas, layout_tree, child_id, document, styles);
        }
    }
    
    /// Collect text content from a DOM node and its children
    fn collect_text_content(&self, document: &Document, node_id: NodeId) -> String {
        let tree = document.tree();
        let mut result = String::new();
        
        if let Some(node) = tree.get(node_id) {
            // If this is a text node, return its content
            if let Some(text) = node.as_text() {
                return text.to_string();
            }
            
            // If this is an element, collect text from children
            for (child_id, _) in tree.children(node_id) {
                if let Some(child) = tree.get(child_id) {
                    if let Some(text) = child.as_text() {
                        result.push_str(text);
                    }
                }
            }
        }
        
        result
    }
    
    /// Text painting with improved readability
    fn paint_text(
        &self,
        canvas: &mut Canvas,
        text: &str,
        x: f32,
        y: f32,
        color: Color,
        font_size: f32,
    ) {
        // Scale factor for the 8x8 bitmap font
        let scale = (font_size / 8.0).max(1.0);
        let char_width = 6.0 * scale; // Slightly tighter spacing
        let char_height = 8.0 * scale;
        
        let mut x_pos = x;
        
        for c in text.chars() {
            // Handle newlines
            if c == '\n' {
                // For now, skip newlines (simple single-line rendering)
                continue;
            }
            
            // Handle spaces
            if c == ' ' {
                x_pos += char_width * 0.8; // Narrower space
                continue;
            }
            
            // Skip if out of bounds
            if x_pos > canvas.width() as f32 {
                break;
            }
            
            // Get bitmap pattern
            let pattern = get_char_pattern(c);
            
            // Draw each pixel as a filled rectangle for better visibility
            for (row, &bits) in pattern.iter().enumerate() {
                for col in 0..8 {
                    if (bits >> (7 - col)) & 1 == 1 {
                        let px = x_pos + col as f32 * scale;
                        let py = y - char_height + row as f32 * scale;
                        
                        // Draw filled rectangle for each "pixel" in the font
                        // Using slightly larger rectangles with small gaps for readability
                        let rect_size = scale.max(1.0);
                        canvas.fill_rect(px, py, rect_size, rect_size, color);
                    }
                }
            }
            
            x_pos += char_width;
        }
    }
    
    /// Calculate total content height
    fn calculate_content_height(&self, layout_tree: &LayoutTree) -> f32 {
        layout_tree.root()
            .and_then(|root| layout_tree.get(root))
            .map(|root_box| {
                root_box.dimensions.content.height
                    + root_box.dimensions.padding.top
                    + root_box.dimensions.padding.bottom
                    + root_box.dimensions.border.top
                    + root_box.dimensions.border.bottom
                    + root_box.dimensions.margin.top
                    + root_box.dimensions.margin.bottom
            })
            .unwrap_or(self.viewport_height as f32)
    }
}

/// Convert CSS color to render color
fn css_color_to_render(css: &CssColor) -> Color {
    Color::rgba(css.r, css.g, css.b, css.a)
}

/// Apply default user-agent styles based on element type
fn apply_default_styles(style: &mut ComputedStyle, tag_name: &str) {
    match tag_name.to_lowercase().as_str() {
        // Block elements
        "div" | "p" | "article" | "section" | "main" | "header" | "footer" | "nav" |
        "aside" | "figure" | "figcaption" | "address" | "blockquote" | "pre" => {
            style.display = Display::Block;
        }
        
        // Headings
        "h1" => {
            style.display = Display::Block;
            style.font_size = 32.0;
            style.font_weight = 700;
            style.margin = EdgeSizes {
                top: SizeValue::Length(21.44, LengthUnit::Px),
                right: SizeValue::Length(0.0, LengthUnit::Px),
                bottom: SizeValue::Length(21.44, LengthUnit::Px),
                left: SizeValue::Length(0.0, LengthUnit::Px),
            };
        }
        "h2" => {
            style.display = Display::Block;
            style.font_size = 24.0;
            style.font_weight = 700;
            style.margin = EdgeSizes {
                top: SizeValue::Length(19.92, LengthUnit::Px),
                right: SizeValue::Length(0.0, LengthUnit::Px),
                bottom: SizeValue::Length(19.92, LengthUnit::Px),
                left: SizeValue::Length(0.0, LengthUnit::Px),
            };
        }
        "h3" => {
            style.display = Display::Block;
            style.font_size = 18.72;
            style.font_weight = 700;
        }
        "h4" => {
            style.display = Display::Block;
            style.font_size = 16.0;
            style.font_weight = 700;
        }
        "h5" => {
            style.display = Display::Block;
            style.font_size = 13.28;
            style.font_weight = 700;
        }
        "h6" => {
            style.display = Display::Block;
            style.font_size = 10.72;
            style.font_weight = 700;
        }
        
        // Inline elements  
        "span" | "a" | "em" | "i" | "u" | "code" | "kbd" | "samp" => {
            style.display = Display::Inline;
        }
        
        // Bold
        "strong" | "b" => {
            style.display = Display::Inline;
            style.font_weight = 700;
        }
        
        // Lists
        "ul" | "ol" => {
            style.display = Display::Block;
            style.padding = EdgeSizes {
                top: SizeValue::Length(0.0, LengthUnit::Px),
                right: SizeValue::Length(0.0, LengthUnit::Px),
                bottom: SizeValue::Length(0.0, LengthUnit::Px),
                left: SizeValue::Length(40.0, LengthUnit::Px),
            };
        }
        "li" => {
            style.display = Display::Block;
        }
        
        // Table
        "table" => {
            style.display = Display::Block;
        }
        "tr" => {
            style.display = Display::Block;
        }
        "td" | "th" => {
            style.display = Display::Inline;
        }
        
        // Images
        "img" => {
            style.display = Display::Inline;
        }
        
        // Body
        "body" => {
            style.display = Display::Block;
            style.margin = EdgeSizes {
                top: SizeValue::Length(8.0, LengthUnit::Px),
                right: SizeValue::Length(8.0, LengthUnit::Px),
                bottom: SizeValue::Length(8.0, LengthUnit::Px),
                left: SizeValue::Length(8.0, LengthUnit::Px),
            };
        }
        
        // HTML
        "html" => {
            style.display = Display::Block;
        }
        
        // Head - hidden
        "head" | "title" | "script" | "style" | "meta" | "link" => {
            style.display = Display::None;
        }
        
        _ => {
            style.display = Display::Inline;
        }
    }
}

/// Get 8x8 bitmap pattern for a character (simple bitmap font)
fn get_char_pattern(c: char) -> [u8; 8] {
    match c.to_ascii_lowercase() {
        'a' => [0b00111100, 0b01000010, 0b01000010, 0b01111110, 0b01000010, 0b01000010, 0b01000010, 0b00000000],
        'b' => [0b01111100, 0b01000010, 0b01000010, 0b01111100, 0b01000010, 0b01000010, 0b01111100, 0b00000000],
        'c' => [0b00111100, 0b01000010, 0b01000000, 0b01000000, 0b01000000, 0b01000010, 0b00111100, 0b00000000],
        'd' => [0b01111000, 0b01000100, 0b01000010, 0b01000010, 0b01000010, 0b01000100, 0b01111000, 0b00000000],
        'e' => [0b01111110, 0b01000000, 0b01000000, 0b01111100, 0b01000000, 0b01000000, 0b01111110, 0b00000000],
        'f' => [0b01111110, 0b01000000, 0b01000000, 0b01111100, 0b01000000, 0b01000000, 0b01000000, 0b00000000],
        'g' => [0b00111100, 0b01000010, 0b01000000, 0b01001110, 0b01000010, 0b01000010, 0b00111100, 0b00000000],
        'h' => [0b01000010, 0b01000010, 0b01000010, 0b01111110, 0b01000010, 0b01000010, 0b01000010, 0b00000000],
        'i' => [0b00111100, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00111100, 0b00000000],
        'j' => [0b00001110, 0b00000100, 0b00000100, 0b00000100, 0b00000100, 0b01000100, 0b00111000, 0b00000000],
        'k' => [0b01000100, 0b01001000, 0b01010000, 0b01100000, 0b01010000, 0b01001000, 0b01000100, 0b00000000],
        'l' => [0b01000000, 0b01000000, 0b01000000, 0b01000000, 0b01000000, 0b01000000, 0b01111110, 0b00000000],
        'm' => [0b01000010, 0b01100110, 0b01011010, 0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b00000000],
        'n' => [0b01000010, 0b01100010, 0b01010010, 0b01001010, 0b01000110, 0b01000010, 0b01000010, 0b00000000],
        'o' => [0b00111100, 0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b00111100, 0b00000000],
        'p' => [0b01111100, 0b01000010, 0b01000010, 0b01111100, 0b01000000, 0b01000000, 0b01000000, 0b00000000],
        'q' => [0b00111100, 0b01000010, 0b01000010, 0b01000010, 0b01001010, 0b01000100, 0b00111010, 0b00000000],
        'r' => [0b01111100, 0b01000010, 0b01000010, 0b01111100, 0b01010000, 0b01001000, 0b01000100, 0b00000000],
        's' => [0b00111100, 0b01000010, 0b01000000, 0b00111100, 0b00000010, 0b01000010, 0b00111100, 0b00000000],
        't' => [0b01111110, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00000000],
        'u' => [0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b00111100, 0b00000000],
        'v' => [0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b00100100, 0b00100100, 0b00011000, 0b00000000],
        'w' => [0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b01011010, 0b01100110, 0b01000010, 0b00000000],
        'x' => [0b01000010, 0b00100100, 0b00011000, 0b00011000, 0b00011000, 0b00100100, 0b01000010, 0b00000000],
        'y' => [0b01000010, 0b01000010, 0b00100100, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b00000000],
        'z' => [0b01111110, 0b00000100, 0b00001000, 0b00010000, 0b00100000, 0b01000000, 0b01111110, 0b00000000],
        '0'..='9' => [0b00111100, 0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b01000010, 0b00111100, 0b00000000],
        '.' => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00011000, 0b00011000, 0b00000000],
        ',' => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00011000, 0b00011000, 0b00110000, 0b00000000],
        ':' => [0b00000000, 0b00011000, 0b00011000, 0b00000000, 0b00011000, 0b00011000, 0b00000000, 0b00000000],
        '-' => [0b00000000, 0b00000000, 0b00000000, 0b01111110, 0b00000000, 0b00000000, 0b00000000, 0b00000000],
        _ => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000],
    }
}
