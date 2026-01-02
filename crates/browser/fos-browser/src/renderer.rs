//! Rendering Pipeline
//!
//! Integrates fos-engine components for rendering web pages.

use std::collections::HashMap;
use fos_dom::{Document, NodeId, DomTree};
use fos_css::computed::{ComputedStyle, Display, SizeValue, EdgeSizes};
use fos_css::properties::LengthUnit;
use fos_css::{Stylesheet, Selector, SelectorPart, parse_stylesheet, StyleResolver};
use fos_layout::{LayoutTree, LayoutBoxId, layout_document};
use fos_render::{Canvas, Color, TextRenderer, css_color_to_render};
use fos_text::{FontId, LineBreaker};

/// A clickable link region in the rendered page
#[derive(Debug, Clone)]
pub struct LinkRegion {
    /// Bounding box x
    pub x: f32,
    /// Bounding box y  
    pub y: f32,
    /// Width
    pub width: f32,
    /// Height
    pub height: f32,
    /// Target URL
    pub href: String,
}

/// An anchor position (element with id attribute) for in-page navigation
#[derive(Debug, Clone)]
pub struct AnchorPosition {
    /// Element ID (without the # prefix)
    pub id: String,
    /// Y position in the document
    pub y: f32,
}

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
    /// Clickable link regions
    pub links: Vec<LinkRegion>,
    /// Anchor positions for in-page navigation
    pub anchors: Vec<AnchorPosition>,
}

/// Page renderer - integrates HTML, CSS, layout, and painting
pub struct PageRenderer {
    /// Viewport width
    viewport_width: u32,
    /// Viewport height
    viewport_height: u32,
    /// Text renderer with font support
    text_renderer: TextRenderer,
    /// Default font ID for text rendering
    default_font: Option<FontId>,
}

impl PageRenderer {
    pub fn new(viewport_width: u32, viewport_height: u32) -> Self {
        let text_renderer = TextRenderer::new();
        // Find a default font (prefer sans-serif fonts)
        let default_font = text_renderer.find_font(&["DejaVu Sans", "Liberation Sans", "Arial", "Helvetica", "sans-serif"]);
        
        if default_font.is_some() {
            log::info!("Font loaded for text rendering");
        } else {
            log::warn!("No system font found, text rendering may fail");
        }
        
        Self {
            viewport_width,
            viewport_height,
            text_renderer,
            default_font,
        }
    }
    
    /// Set viewport size
    pub fn set_viewport(&mut self, width: u32, height: u32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }
    
    /// Measure text width using the text renderer
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> f32 {
        if let Some(font_id) = self.default_font {
            self.text_renderer.measure_text(text, font_id, font_size)
        } else {
            // Fallback: estimate width based on character count
            let char_width = font_size * 0.5;
            text.chars().count() as f32 * char_width
        }
    }
    
    /// Render HTML to pixels with scroll offset
    pub fn render_html(&mut self, html: &str, base_url: &str, scroll_offset: f32) -> Option<RenderedPage> {
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
        
        // 4. Paint to canvas with scroll offset, collecting link regions and anchors
        let mut links = Vec::new();
        let mut anchors = Vec::new();
        let pixels = self.paint(&document, &styles, &layout_tree, scroll_offset, &mut links, &mut anchors)?;
        
        // Calculate content height
        let content_height = self.calculate_content_height(&layout_tree);
        
        Some(RenderedPage {
            pixels,
            width: self.viewport_width,
            height: self.viewport_height,
            content_height,
            links,
            anchors,
        })
    }
    
    /// Compute styles for all elements using CSS from document
    fn compute_styles(&self, document: &Document) -> HashMap<NodeId, ComputedStyle> {
        let mut styles = HashMap::new();
        let tree = document.tree();
        
        // 1. Extract CSS from <style> tags in <head>
        let css_text = self.extract_css_from_document(document);
        
        // 2. Parse CSS into stylesheet
        let stylesheet = if !css_text.is_empty() {
            match parse_stylesheet(&css_text) {
                Ok(ss) => {
                    log::debug!("Parsed {} CSS rules from page", ss.rules.len());
                    Some(ss)
                }
                Err(e) => {
                    log::warn!("CSS parse error: {}", e);
                    None
                }
            }
        } else {
            None
        };
        
        // 3. Compute styles for all nodes (using old method that works)
        self.compute_styles_recursive(tree, tree.root(), &mut styles, stylesheet.as_ref());
        
        styles
    }
    
    /// Extract CSS text from <style> tags in document
    fn extract_css_from_document(&self, document: &Document) -> String {
        let mut css = String::new();
        let tree = document.tree();
        let head = document.head();
        
        if !head.is_valid() {
            return css;
        }
        
        // Find all <style> tags in <head>
        for (style_id, style_node) in tree.children(head) {
            if let Some(element) = style_node.as_element() {
                let tag = tree.resolve(element.name.local);
                if tag.eq_ignore_ascii_case("style") {
                    // Get text content of style element
                    for (_, child) in tree.children(style_id) {
                        if let Some(text) = child.as_text() {
                            css.push_str(text);
                            css.push('\n');
                        }
                    }
                }
            }
        }
        
        // Also look for style tags in body (non-standard but common)
        self.collect_style_text(tree, document.body(), &mut css);
        
        css
    }
    
    /// Recursively collect style tag text (for style tags in body)
    fn collect_style_text(&self, tree: &DomTree, node_id: NodeId, css: &mut String) {
        if !node_id.is_valid() {
            return;
        }
        
        for (child_id, child_node) in tree.children(node_id) {
            if let Some(element) = child_node.as_element() {
                let tag = tree.resolve(element.name.local);
                if tag.eq_ignore_ascii_case("style") {
                    for (_, text_node) in tree.children(child_id) {
                        if let Some(text) = text_node.as_text() {
                            css.push_str(text);
                            css.push('\n');
                        }
                    }
                }
            }
            // Recurse
            self.collect_style_text(tree, child_id, css);
        }
    }
    
    /// Compute styles using the StyleResolver (proper CSS cascade)
    #[allow(dead_code)]
    fn compute_styles_with_resolver(
        &self,
        tree: &DomTree,
        node_id: NodeId,
        styles: &mut HashMap<NodeId, ComputedStyle>,
        resolver: &StyleResolver,
    ) {
        if !node_id.is_valid() {
            return;
        }
        
        // Compute style for this node using the resolver
        let style = resolver.compute_style(tree, node_id);
        styles.insert(node_id, style);
        
        // Recurse to children
        for (child_id, _) in tree.children(node_id) {
            self.compute_styles_with_resolver(tree, child_id, styles, resolver);
        }
    }
    
    /// Recursively compute styles with CSS matching
    fn compute_styles_recursive(
        &self, 
        tree: &DomTree, 
        node_id: NodeId, 
        styles: &mut HashMap<NodeId, ComputedStyle>,
        stylesheet: Option<&Stylesheet>,
    ) {
        if !node_id.is_valid() {
            return;
        }
        
        // Create default computed style
        let mut style = ComputedStyle::default();
        
        // Get node to check for element type and attributes
        if let Some(node) = tree.get(node_id) {
            if let Some(element) = node.as_element() {
                // Get tag name
                let tag_name = tree.resolve(element.name.local);
                
                // 1. Apply default browser styles based on element type
                apply_default_styles(&mut style, tag_name);
                
                // 2. Apply matching CSS rules from stylesheet
                if let Some(ss) = stylesheet {
                    self.apply_matching_rules(tree, node_id, element, tag_name, ss, &mut style);
                }
                
                // 3. Apply inline style attribute  
                for attr in element.attrs.iter() {
                    let attr_name = tree.resolve(attr.name.local);
                    if attr_name == "style" {
                        // Parse inline CSS declarations
                        self.apply_inline_style(&attr.value, &mut style);
                    }
                }
            }
        }
        
        styles.insert(node_id, style);
        
        // Process children
        for (child_id, _) in tree.children(node_id) {
            self.compute_styles_recursive(tree, child_id, styles, stylesheet);
        }
    }
    
    /// Apply matching CSS rules to element style
    fn apply_matching_rules(
        &self,
        tree: &DomTree,
        _node_id: NodeId,
        element: &fos_dom::ElementData,
        tag_name: &str,
        stylesheet: &Stylesheet,
        style: &mut ComputedStyle,
    ) {
        // Get element classes and ID for matching
        let element_id = element.id.map(|id| tree.resolve(id));
        let element_classes: Vec<&str> = element.classes.iter()
            .map(|c| tree.resolve(*c))
            .collect();
        
        // Check each rule in stylesheet
        for rule in &stylesheet.rules {
            for selector in &rule.selectors {
                if self.selector_matches(selector, tag_name, element_id, &element_classes) {
                    // Apply declarations from this rule
                    for decl in &rule.declarations {
                        style.apply_declaration(decl);
                    }
                }
            }
        }
    }
    
    /// Check if a selector matches an element
    fn selector_matches(
        &self,
        selector: &Selector,
        tag_name: &str,
        element_id: Option<&str>,
        classes: &[&str],
    ) -> bool {
        // Safety check: empty selector parts shouldn't match
        if selector.parts.is_empty() {
            return false;
        }
        
        // Track if we've matched at least one meaningful part
        let mut has_match = false;
        
        // Simple matching: check selector parts
        for part in &selector.parts {
            match part {
                SelectorPart::Type(t) => {
                    if !t.eq_ignore_ascii_case(tag_name) {
                        return false;
                    }
                    has_match = true;
                }
                SelectorPart::Class(c) => {
                    if !classes.iter().any(|ec| ec.eq_ignore_ascii_case(c)) {
                        return false;
                    }
                    has_match = true;
                }
                SelectorPart::Id(id) => {
                    if element_id != Some(id.as_str()) {
                        return false;
                    }
                    has_match = true;
                }
                SelectorPart::Universal => {
                    // Universal selector matches everything
                    has_match = true;
                }
                SelectorPart::Combinator(_) => {
                    // Stop at combinators - we don't support parent chain matching
                    // Only count as match if we had something before the combinator
                    break;
                }
                SelectorPart::PseudoClass(_) | SelectorPart::PseudoElement(_) | SelectorPart::Attribute { .. } => {
                    // Skip pseudo-classes/elements and attribute selectors
                    // Don't count as match, but don't reject either
                    // (This means `:hover` alone won't match, but `div:hover` will match div)
                }
            }
        }
        
        has_match
    }
    
    /// Apply inline style declarations
    fn apply_inline_style(&self, style_text: &str, style: &mut ComputedStyle) {
        // Parse inline CSS as if it were a rule body
        let wrapped = format!("*{{{}}}", style_text);
        if let Ok(ss) = parse_stylesheet(&wrapped) {
            for rule in &ss.rules {
                for decl in &rule.declarations {
                    style.apply_declaration(decl);
                }
            }
        }
    }
    
    /// Paint the layout tree with scroll offset
    fn paint(
        &mut self,
        document: &Document,
        styles: &HashMap<NodeId, ComputedStyle>,
        _layout_tree: &LayoutTree,
        scroll_offset: f32,
        links: &mut Vec<LinkRegion>,
        anchors: &mut Vec<AnchorPosition>,
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
            // Apply scroll offset to starting position
            let mut y_cursor = 20.0f32 - scroll_offset;
            self.paint_dom_node(&mut canvas, tree, body, styles, 8.0, &mut y_cursor, links, anchors);
            
            // If no content was painted (taking scroll into account), show a message
            if y_cursor < 30.0 - scroll_offset {
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
    
    /// Paint a DOM node and its children with proper inline/block handling
    fn paint_dom_node(
        &mut self,
        canvas: &mut Canvas,
        tree: &DomTree,
        node_id: NodeId,
        styles: &HashMap<NodeId, ComputedStyle>,
        x_offset: f32,
        y_cursor: &mut f32,
        links: &mut Vec<LinkRegion>,
        anchors: &mut Vec<AnchorPosition>,
    ) {
        // Use a line buffer for text accumulation (leave margin for right edge)
        let max_width = canvas.width() as f32 - 30.0;
        let mut line_buffer = LineBuffer::new(x_offset, max_width, 16.0);
        self.paint_node_recursive(canvas, tree, node_id, styles, &mut line_buffer, y_cursor, links, anchors);
        
        // Flush any remaining text
        if !line_buffer.is_empty() {
            line_buffer.flush(canvas, y_cursor, self, links);
        }
    }
    
    /// Recursive painting with line buffer for text accumulation
    fn paint_node_recursive(
        &mut self,
        canvas: &mut Canvas,
        tree: &DomTree,
        node_id: NodeId,
        styles: &HashMap<NodeId, ComputedStyle>,
        line_buffer: &mut LineBuffer,
        y_cursor: &mut f32,
        links: &mut Vec<LinkRegion>,
        anchors: &mut Vec<AnchorPosition>,
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
        
        // If text node, add to line buffer
        if let Some(text) = node.as_text() {
            let text_str = text.replace('\n', " ");
            let trimmed = text_str.split_whitespace().collect::<Vec<_>>().join(" ");
            if !trimmed.is_empty() {
                let font_size = line_buffer.current_font_size.max(14.0);
                let text_color = line_buffer.current_color;
                
                line_buffer.add_text(&trimmed, font_size, text_color);
            }
            return;
        }
        
        // If element, handle block vs inline
        if let Some(element) = node.as_element() {
            let tag = tree.resolve(element.name.local).to_lowercase();
            let is_block = matches!(tag.as_str(), 
                "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | 
                "ul" | "ol" | "li" | "section" | "article" | "header" | "footer" | 
                "main" | "nav" | "aside" | "figure" | "figcaption" | "blockquote" |
                "pre" | "hr" | "br" | "table" | "tr" | "form" | "td" | "th");
            
            let font_size = style.map(|s| s.font_size).unwrap_or(line_buffer.current_font_size);
            
            // Block elements flush the line buffer and add vertical space
            if is_block {
                if !line_buffer.is_empty() {
                    line_buffer.flush(canvas, y_cursor, self, links);
                }
                // Better spacing based on element type
                let margin_before = match tag.as_str() {
                    "h1" => 20.0,
                    "h2" => 16.0,
                    "h3" | "h4" => 12.0,
                    "p" => 8.0,
                    "ul" | "ol" => 6.0,
                    "li" => 2.0,
                    "td" | "th" | "tr" => 2.0,
                    _ => font_size * 0.3,
                };
                *y_cursor += margin_before;
            }
            
            // Handle special elements
            if tag == "br" {
                line_buffer.flush(canvas, y_cursor, self, links);
                return;
            }
            
            if tag == "hr" {
                line_buffer.flush(canvas, y_cursor, self, links);
                canvas.fill_rect(line_buffer.start_x, *y_cursor, line_buffer.max_width - line_buffer.start_x - 20.0, 1.0, Color::rgb(128, 128, 128));
                *y_cursor += 10.0;
                return;
            }
            
            // Skip hidden elements
            if tag == "script" || tag == "style" || tag == "noscript" || tag == "template" {
                return;
            }
            
            // Save current state for restoration
            let saved_font_size = line_buffer.current_font_size;
            let saved_color = line_buffer.current_color;
            let saved_indent = line_buffer.indent_level;
            let saved_href = line_buffer.current_href.clone();
            let saved_list_counter = line_buffer.list_counter;
            
            // Increment indent for lists and blockquotes
            if tag == "ul" || tag == "ol" || tag == "blockquote" {
                line_buffer.indent_level += 1;
                line_buffer.current_x = line_buffer.effective_start_x();
            }
            
            // Ordered lists start a counter at 1
            if tag == "ol" {
                line_buffer.list_counter = 1;
            }
            // Unordered lists reset counter to 0 (signals bullet mode)
            if tag == "ul" {
                line_buffer.list_counter = 0;
            }
            
            // Table cell handling - simple approach: cells are separated by |
            if tag == "td" || tag == "th" {
                // Add cell separator if not first in row
                if line_buffer.current_x > line_buffer.effective_start_x() + 5.0 {
                    let font_size = line_buffer.current_font_size;
                    let color = Color::rgb(180, 180, 180);
                    line_buffer.add_text(" | ", font_size, color);
                }
            }
            
            // Table headers get slightly bold look (darker color)
            if tag == "th" {
                line_buffer.current_color = Color::rgb(40, 40, 40);
            }
            
            // List items get a bullet or number marker
            if tag == "li" {
                if !line_buffer.is_empty() {
                    line_buffer.flush(canvas, y_cursor, self, links);
                }
                // Add marker based on list type
                let font_size = line_buffer.current_font_size;
                let color = line_buffer.current_color;
                if line_buffer.list_counter > 0 {
                    // Ordered list - show number
                    let marker = format!("{}. ", line_buffer.list_counter);
                    line_buffer.add_text(&marker, font_size, color);
                    line_buffer.list_counter += 1;
                } else {
                    // Unordered list - show bullet
                    line_buffer.add_text("• ", font_size, color);
                }
            }
            
            // Set font size based on heading
            match tag.as_str() {
                "h1" => line_buffer.current_font_size = 28.0,
                "h2" => line_buffer.current_font_size = 24.0,
                "h3" => line_buffer.current_font_size = 20.0,
                "h4" => line_buffer.current_font_size = 18.0,
                "h5" => line_buffer.current_font_size = 16.0,
                "h6" => line_buffer.current_font_size = 14.0,
                "small" => line_buffer.current_font_size = (saved_font_size * 0.8).max(12.0),
                _ => {}
            };
            
            // Links get blue color and save href
            if tag == "a" {
                line_buffer.current_color = Color::rgb(51, 102, 204); // Wikipedia link blue
                // Extract href attribute
                for attr in element.attrs.iter() {
                    let attr_name = tree.resolve(attr.name.local);
                    if attr_name == "href" {
                        line_buffer.current_href = Some(attr.value.to_string());
                        break;
                    }
                }
            }
            
            // Record element ID for anchor navigation
            for attr in element.attrs.iter() {
                let attr_name = tree.resolve(attr.name.local);
                if attr_name == "id" {
                    let id = attr.value.to_string();
                    if !id.is_empty() {
                        anchors.push(AnchorPosition {
                            id,
                            y: *y_cursor,
                        });
                    }
                    break;
                }
            }
            
            // Bold text
            if tag == "b" || tag == "strong" {
                // Just use same color for now - we don't have bold font
            }
            
            // Parse inline style attribute for colors
            for attr in element.attrs.iter() {
                let attr_name = tree.resolve(attr.name.local);
                if attr_name == "style" {
                    if let Some(color) = parse_color_from_style(&attr.value) {
                        line_buffer.current_color = color;
                    }
                    break;
                }
            }
            // Recurse into children
            for (child_id, _) in tree.children(node_id) {
                self.paint_node_recursive(canvas, tree, child_id, styles, line_buffer, y_cursor, links, anchors);
            }
            
            // Restore state
            line_buffer.current_font_size = saved_font_size;
            line_buffer.current_color = saved_color;
            line_buffer.indent_level = saved_indent;
            line_buffer.current_href = saved_href;
            line_buffer.list_counter = saved_list_counter;
            line_buffer.current_x = line_buffer.effective_start_x();
            
            // Block elements flush after and add space
            if is_block {
                if !line_buffer.is_empty() {
                    line_buffer.flush(canvas, y_cursor, self, links);
                }
                // Better spacing based on element type
                let margin_after = match tag.as_str() {
                    "h1" => 12.0,
                    "h2" => 10.0,
                    "h3" | "h4" => 8.0,
                    "p" => 12.0,  // Paragraphs need good separation
                    "li" => 2.0,
                    _ => 4.0,
                };
                *y_cursor += margin_after;
            }
        }
    }
}

/// Parse color from inline style attribute
fn parse_color_from_style(style: &str) -> Option<Color> {
    for part in style.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("color:") {
            return parse_css_color(value.trim());
        }
    }
    None
}

/// Parse background-color from inline style attribute
#[allow(dead_code)]
fn parse_background_from_style(style: &str) -> Option<Color> {
    for part in style.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("background-color:").or_else(|| part.strip_prefix("background:")) {
            // Background can have multiple values, just take the color part
            let value = value.trim().split_whitespace().next()?;
            return parse_css_color(value);
        }
    }
    None
}

/// Parse a CSS color value
fn parse_css_color(value: &str) -> Option<Color> {
    let value = value.trim().to_lowercase();
    
    // Named colors (common web colors)
    match value.as_str() {
        "black" => return Some(Color::rgb(0, 0, 0)),
        "white" => return Some(Color::rgb(255, 255, 255)),
        "red" => return Some(Color::rgb(255, 0, 0)),
        "green" => return Some(Color::rgb(0, 128, 0)),
        "blue" => return Some(Color::rgb(0, 0, 255)),
        "gray" | "grey" => return Some(Color::rgb(128, 128, 128)),
        "lightgray" | "lightgrey" => return Some(Color::rgb(211, 211, 211)),
        "darkgray" | "darkgrey" => return Some(Color::rgb(169, 169, 169)),
        "silver" => return Some(Color::rgb(192, 192, 192)),
        "navy" => return Some(Color::rgb(0, 0, 128)),
        "teal" => return Some(Color::rgb(0, 128, 128)),
        "orange" => return Some(Color::rgb(255, 165, 0)),
        "yellow" => return Some(Color::rgb(255, 255, 0)),
        "purple" => return Some(Color::rgb(128, 0, 128)),
        "pink" => return Some(Color::rgb(255, 192, 203)),
        "brown" => return Some(Color::rgb(165, 42, 42)),
        "transparent" => return None, // Skip transparent
        _ => {}
    }
    
    // Hex colors #rgb or #rrggbb
    if let Some(hex) = value.strip_prefix('#') {
        if hex.len() == 3 {
            let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
            return Some(Color::rgb(r, g, b));
        } else if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            return Some(Color::rgb(r, g, b));
        }
    }
    
    // rgb(r, g, b)
    if let Some(rgb) = value.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<&str> = rgb.split(',').collect();
        if parts.len() == 3 {
            let r = parts[0].trim().parse().ok()?;
            let g = parts[1].trim().parse().ok()?;
            let b = parts[2].trim().parse().ok()?;
            return Some(Color::rgb(r, g, b));
        }
    }
    
    None
}

/// Line buffer for accumulating inline text
struct LineBuffer {
    segments: Vec<TextSegment>,
    start_x: f32,
    current_x: f32,
    max_width: f32,
    current_font_size: f32,
    current_color: Color,
    /// Current indentation level (for lists, blockquotes)
    indent_level: u32,
    /// Current link href (if inside an <a> tag)
    current_href: Option<String>,
    /// Current list counter for <ol> (0 means unordered list or not in list)
    list_counter: u32,
}


#[allow(dead_code)]
struct TextSegment {
    text: String,
    font_size: f32,
    color: Color,
    x: f32,
    /// Link href if this is a link
    href: Option<String>,
}

impl LineBuffer {
    fn new(start_x: f32, max_width: f32, font_size: f32) -> Self {
        Self {
            segments: Vec::new(),
            start_x,
            current_x: start_x,
            max_width,
            current_font_size: font_size,
            current_color: Color::BLACK,
            indent_level: 0,
            current_href: None,
            list_counter: 0,
        }
    }
    
    /// Get effective start x (including indentation)
    fn effective_start_x(&self) -> f32 {
        self.start_x + (self.indent_level as f32 * 20.0)
    }
    
    fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
    
    fn add_text_with_measure<F: Fn(&str, f32) -> f32>(&mut self, text: &str, font_size: f32, color: Color, measure: F) {
        let effective_start = self.effective_start_x();
        let right_margin = 15.0;
        let wrap_width = self.max_width - right_margin;
        
        // Use proper text measurement for space width
        let space_width = measure(" ", font_size);
        
        // Initialize current_x if needed
        if self.current_x < effective_start {
            self.current_x = effective_start;
        }
        
        // Use LineBreaker for proper Unicode-aware line breaking
        let available_width = wrap_width - self.current_x;
        let lines = LineBreaker::break_lines(text, available_width.max(wrap_width * 0.5), |s| measure(s, font_size));
        
        for (i, &(start, end)) in lines.iter().enumerate() {
            let line_text = &text[start..end];
            let trimmed = line_text.trim_end();
            
            if trimmed.is_empty() {
                continue;
            }
            
            // Check if this line needs wrapping from current position
            let line_width = measure(trimmed, font_size);
            
            if i > 0 || (self.current_x + line_width > wrap_width && self.current_x > effective_start) {
                // Need to wrap - start new line
                if !self.segments.is_empty() {
                    self.segments.push(TextSegment {
                        text: "\n".to_string(),
                        font_size,
                        color,
                        x: self.current_x,
                        href: None,
                    });
                }
                self.current_x = effective_start;
            }
            
            // Add the text segment
            self.segments.push(TextSegment {
                text: format!("{} ", trimmed),
                font_size,
                color,
                x: self.current_x,
                href: self.current_href.clone(),
            });
            
            self.current_x += line_width + space_width;
        }
    }
    
    // Keep fallback without measure function for backwards compatibility
    fn add_text(&mut self, text: &str, font_size: f32, color: Color) {
        // Fallback using approximate character width
        let char_width = font_size * 0.5;
        self.add_text_with_measure(text, font_size, color, |s, _| s.chars().count() as f32 * char_width);
    }
    
    fn flush(&mut self, canvas: &mut Canvas, y_cursor: &mut f32, renderer: &mut PageRenderer, links: &mut Vec<LinkRegion>) {
        if self.segments.is_empty() {
            return;
        }
        
        let effective_start = self.effective_start_x();
        let mut x = effective_start;
        let line_height = self.current_font_size * 1.3;
        
        for segment in &self.segments {
            if segment.text == "\n" {
                *y_cursor += line_height;
                x = effective_start;
                continue;
            }
            
            renderer.paint_text(canvas, &segment.text, x, *y_cursor, segment.color, segment.font_size);
            
            // Use proper text measurement instead of character counting
            let text_width = renderer.measure_text(&segment.text, segment.font_size);
            
            // Record link region if this is a link
            if let Some(ref href) = segment.href {
                // Text is drawn at y - char_height (baseline), so link region should match
                let char_height = segment.font_size * 1.2; // Approximate line height
                links.push(LinkRegion {
                    x,
                    y: *y_cursor - char_height,  // Match where text is actually drawn
                    width: text_width,
                    height: char_height,
                    href: href.clone(),
                });
            }
            
            x += text_width;
        }
        
        *y_cursor += line_height;
        self.segments.clear();
        self.current_x = effective_start;
    }
}

impl PageRenderer {
    /// Paint a single layout box and its children
    fn paint_box(
        &mut self,
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
            
            let y_offset = dims.content.y + font_size;
            
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
    
    /// Text painting using TextRenderer with proper fonts
    fn paint_text(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        x: f32,
        y: f32,
        color: Color,
        font_size: f32,
    ) {
        // Use TextRenderer if we have a font
        if let Some(font_id) = self.default_font {
            // Use the proper font rendering
            self.text_renderer.draw_text(canvas, text, x, y, font_id, font_size, color);
        } else {
            // No font available - use bitmap fallback
            let scale = (font_size / 8.0).max(1.0);
            let char_width = 6.0 * scale;
            let char_height = 8.0 * scale;
            
            let mut x_pos = x;
            
            for c in text.chars() {
                if c == '\n' {
                    continue;
                }
                if c == ' ' {
                    x_pos += char_width * 0.8;
                    continue;
                }
                if x_pos > canvas.width() as f32 {
                    break;
                }
                
                let pattern = get_char_pattern(c);
                
                for (row, &bits) in pattern.iter().enumerate() {
                    for col in 0..8 {
                        if (bits >> (7 - col)) & 1 == 1 {
                            let px = x_pos + col as f32 * scale;
                            let py = y - char_height + row as f32 * scale;
                            let rect_size = scale.max(1.0);
                            canvas.fill_rect(px, py, rect_size, rect_size, color);
                        }
                    }
                }
                
                x_pos += char_width;
            }
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
        // Digits with distinct patterns
        '0' => [0b00111100, 0b01000110, 0b01001010, 0b01010010, 0b01100010, 0b01000010, 0b00111100, 0b00000000],
        '1' => [0b00011000, 0b00111000, 0b00011000, 0b00011000, 0b00011000, 0b00011000, 0b01111110, 0b00000000],
        '2' => [0b00111100, 0b01000010, 0b00000010, 0b00001100, 0b00110000, 0b01000000, 0b01111110, 0b00000000],
        '3' => [0b00111100, 0b01000010, 0b00000010, 0b00011100, 0b00000010, 0b01000010, 0b00111100, 0b00000000],
        '4' => [0b00000100, 0b00001100, 0b00010100, 0b00100100, 0b01111110, 0b00000100, 0b00000100, 0b00000000],
        '5' => [0b01111110, 0b01000000, 0b01111100, 0b00000010, 0b00000010, 0b01000010, 0b00111100, 0b00000000],
        '6' => [0b00011100, 0b00100000, 0b01000000, 0b01111100, 0b01000010, 0b01000010, 0b00111100, 0b00000000],
        '7' => [0b01111110, 0b00000010, 0b00000100, 0b00001000, 0b00010000, 0b00010000, 0b00010000, 0b00000000],
        '8' => [0b00111100, 0b01000010, 0b01000010, 0b00111100, 0b01000010, 0b01000010, 0b00111100, 0b00000000],
        '9' => [0b00111100, 0b01000010, 0b01000010, 0b00111110, 0b00000010, 0b00000100, 0b00111000, 0b00000000],
        '.' => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00011000, 0b00011000, 0b00000000],
        ',' => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00011000, 0b00011000, 0b00110000, 0b00000000],
        ':' => [0b00000000, 0b00011000, 0b00011000, 0b00000000, 0b00011000, 0b00011000, 0b00000000, 0b00000000],
        '-' => [0b00000000, 0b00000000, 0b00000000, 0b01111110, 0b00000000, 0b00000000, 0b00000000, 0b00000000],
        // Bullet for lists (small filled circle)
        '•' => [0b00000000, 0b00000000, 0b00011000, 0b00111100, 0b00111100, 0b00011000, 0b00000000, 0b00000000],
        _ => [0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000, 0b00000000],
    }
}
