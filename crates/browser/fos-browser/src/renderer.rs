//! Rendering Pipeline
//!
//! Integrates fos-engine components for rendering web pages.
//!
//! Rendering is split in two phases:
//! 1. **Layout** walks the DOM once per page and viewport width, producing
//!    positioned lines of text (a display list).
//! 2. **Paint** draws only the lines that intersect the requested region.
//!
//! The layout is cached, so scrolling and re-rendering the same page only
//! repaint the visible lines. The cache keeps just the compact display list,
//! not the per-node styles it was built from. A layout built from a live
//! DOM is keyed by the tree's revision, so any DOM mutation (from scripts,
//! for example) is picked up by the next render.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use fos_dom::{Document, NodeId, DomTree, DomRevision};
use fos_css::computed::{ComputedStyle, Display, SizeValue, EdgeSizes};
use fos_css::properties::LengthUnit;
use fos_css::{Stylesheet, Selector, SelectorPart, parse_stylesheet, StyleResolver};
use fos_render::{Canvas, Color, TextRenderer};
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
    /// Pixel buffer, one `0xAARRGGBB` word per pixel (window framebuffer format)
    pub pixels: Vec<u32>,
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Content height (for scroll)
    pub content_height: f32,
    /// Clickable link regions (buffer coordinates)
    pub links: Vec<LinkRegion>,
    /// Anchor positions for in-page navigation (buffer coordinates)
    pub anchors: Vec<AnchorPosition>,
    /// Document y of the buffer's first row
    origin: f32,
    /// Layout the pixels were painted from (see `PageRenderer::layout_generation`)
    layout_generation: u64,
}

/// A run of text with uniform style within a line
#[derive(Debug, Clone)]
struct TextSegment {
    text: String,
    font_size: f32,
    color: Color,
    /// Link href if this is a link
    href: Option<String>,
    /// Element the text belongs to (for hit testing clicks)
    node: NodeId,
}

/// A laid-out line of text, in document coordinates
#[derive(Debug, Clone)]
struct LaidOutLine {
    /// Baseline y
    y: f32,
    /// Start x
    x: f32,
    segments: Vec<TextSegment>,
}

/// A horizontal rule (`<hr>`), in document coordinates
#[derive(Debug, Clone, Copy)]
struct Rule {
    x: f32,
    y: f32,
    width: f32,
}

/// Display list for a page at one viewport width
#[derive(Debug, Default)]
struct PageLayout {
    /// Lines in document order (ascending `y`)
    lines: Vec<LaidOutLine>,
    /// Horizontal rules in document order
    rules: Vec<Rule>,
    /// Element ids and their document y positions
    anchors: Vec<AnchorPosition>,
    /// Height of the laid-out document
    content_height: f32,
    /// Largest font size used (bounds how far glyphs reach above a baseline)
    max_font_size: f32,
}

/// What a cached layout was built from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutSource {
    /// HTML source, by hash of the source and base URL
    Html(u64),
    /// A DOM tree in a given state
    Dom(DomRevision),
}

/// Cached layout for the most recently rendered page
struct CachedLayout {
    /// What the layout was built from
    source: LayoutSource,
    /// Viewport width the layout was built for
    width: u32,
    layout: PageLayout,
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
    /// Layout of the most recently rendered page
    cached: Option<CachedLayout>,
    /// Incremented for every new layout, to tell whether a rendered
    /// buffer was painted from the current one
    layout_generation: u64,
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
            cached: None,
            layout_generation: 0,
        }
    }

    /// Set viewport size
    pub fn set_viewport(&mut self, width: u32, height: u32) {
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// Drop the cached layout and glyphs (e.g. on memory pressure)
    pub fn clear_cache(&mut self) {
        self.cached = None;
        self.text_renderer.clear_glyph_cache();
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

    /// Parse and render HTML to pixels with scroll offset. Prefer
    /// [`Self::render_document`] when the page is already parsed.
    pub fn render_html(&mut self, html: &str, base_url: &str, scroll_offset: f32) -> Option<RenderedPage> {
        let source = LayoutSource::Html(page_key(html, base_url));
        if !self.has_layout(source) {
            let document = fos_html::parse_with_url(html, base_url);
            self.lay_out(source, &document);
        }
        self.paint_cached(scroll_offset)
    }

    /// Render a parsed document to pixels with scroll offset.
    ///
    /// The layout is rebuilt only when the DOM changed since the last
    /// render, or the viewport width did; otherwise this only repaints.
    pub fn render_document(&mut self, document: &Document, scroll_offset: f32) -> Option<RenderedPage> {
        let source = LayoutSource::Dom(document.tree().revision());
        if !self.has_layout(source) {
            self.lay_out(source, document);
        }
        self.paint_cached(scroll_offset)
    }

    /// Render like [`Self::render_document`], reusing `previous` (a render
    /// of the same layout at another scroll offset) when the viewport has
    /// only moved: rows still in view are moved, and only the newly exposed
    /// rows are painted. Falls back to a full render when the buffers are
    /// not compatible (new layout, new size, fractional or large scroll).
    pub fn render_document_scrolled(
        &mut self,
        document: &Document,
        scroll_offset: f32,
        previous: RenderedPage,
    ) -> Option<RenderedPage> {
        let source = LayoutSource::Dom(document.tree().revision());
        if !self.has_layout(source) {
            self.lay_out(source, document);
        }

        let delta = scroll_offset - previous.origin;
        let reusable = previous.layout_generation == self.layout_generation
            && previous.width == self.viewport_width
            && previous.height == self.viewport_height
            && previous.pixels.len() == self.viewport_width as usize * self.viewport_height as usize
            && delta.fract() == 0.0
            && delta.abs() < self.viewport_height as f32;
        if reusable {
            self.repaint_scrolled(previous, scroll_offset)
        } else {
            drop(previous);
            self.paint_cached(scroll_offset)
        }
    }

    /// Whether the cached layout reflects `document` as it is now
    /// The element whose text is at `(x, y)` in document coordinates,
    /// per the current layout
    pub fn node_at(&mut self, x: f32, y: f32) -> Option<NodeId> {
        let cached = self.cached.take()?;
        let mut found = None;
        // Lines are sorted by baseline; text spans the line height above it
        let first = cached.layout.lines.partition_point(|line| line.y < y);
        for line in cached.layout.lines[first..].iter().take(4) {
            let mut sx = line.x;
            for segment in &line.segments {
                let top = line.y - segment.font_size * 1.2;
                let width = self.measure_text(&segment.text, segment.font_size);
                if y >= top && y <= line.y + segment.font_size * 0.3 && x >= sx && x <= sx + width {
                    found = Some(segment.node);
                    break;
                }
                sx += width;
            }
            if found.is_some() {
                break;
            }
        }
        self.cached = Some(cached);
        found.filter(|n| n.is_valid())
    }

    pub fn is_layout_current(&self, document: &Document) -> bool {
        self.has_layout(LayoutSource::Dom(document.tree().revision()))
    }

    fn has_layout(&self, source: LayoutSource) -> bool {
        self.cached.as_ref().is_some_and(|c| c.source == source && c.width == self.viewport_width)
    }

    /// Lay out `document` at the current viewport width and cache the result
    fn lay_out(&mut self, source: LayoutSource, document: &Document) {
        // Free the old layout first, so two are never alive at once
        self.cached = None;
        let width = self.viewport_width;
        let styler = Styler {
            renderer: self,
            tree: document.tree(),
            stylesheet: self.page_stylesheet(document),
        };
        let layout = build_layout(document, &styler, width);
        // Only the display list is kept
        self.cached = Some(CachedLayout { source, width, layout });
        self.layout_generation += 1;
    }

    /// Paint the visible region of the cached layout
    fn paint_cached(&mut self, scroll_offset: f32) -> Option<RenderedPage> {
        let cached = self.cached.take()?;

        let mut links = Vec::new();
        let painted = self.paint(&cached.layout, scroll_offset, self.viewport_height, &mut links);
        let content_height = cached.layout.content_height;
        let anchors = anchors_from(&cached.layout, scroll_offset);
        self.cached = Some(cached);

        Some(RenderedPage {
            pixels: painted?,
            width: self.viewport_width,
            height: self.viewport_height,
            content_height,
            links,
            anchors,
            origin: scroll_offset,
            layout_generation: self.layout_generation,
        })
    }

    /// Move `page` (painted from the cached layout) to `scroll_offset`,
    /// less than a buffer height away, painting only the exposed rows
    fn repaint_scrolled(&mut self, mut page: RenderedPage, scroll_offset: f32) -> Option<RenderedPage> {
        let delta = (scroll_offset - page.origin) as i64;
        if delta == 0 {
            return Some(page);
        }
        let cached = self.cached.take()?;

        let width = page.width as usize;
        let height = page.height as usize;
        let shift = delta.unsigned_abs() as usize;
        // Rows [kept_top, kept_bottom) of the new buffer come from the old one
        let (kept_top, band_top) = if delta > 0 {
            page.pixels.copy_within(shift * width.., 0);
            (0, height - shift)
        } else {
            page.pixels.copy_within(..(height - shift) * width, shift * width);
            (shift, 0)
        };
        let kept_bottom = kept_top + (height - shift);

        let mut band_links = Vec::new();
        let band_origin = scroll_offset + band_top as f32;
        let band = self.paint(&cached.layout, band_origin, shift as u32, &mut band_links);
        page.anchors = anchors_from(&cached.layout, scroll_offset);
        self.cached = Some(cached);
        page.pixels[band_top * width..(band_top + shift) * width].copy_from_slice(&band?);

        // Links: the old ones still in view, moved, plus the band's. A link
        // crossing the band edge is in both lists.
        let (kept_top, kept_bottom) = (kept_top as f32, kept_bottom as f32);
        page.links.retain_mut(|link| {
            link.y -= delta as f32;
            link.y + link.height > kept_top && link.y < kept_bottom
        });
        for mut link in band_links {
            link.y += band_top as f32;
            let duplicate = page.links.iter().any(|l| {
                l.href == link.href && l.x == link.x && (l.y - link.y).abs() < 0.5
            });
            if !duplicate {
                page.links.push(link);
            }
        }

        page.origin = scroll_offset;
        Some(page)
    }

    /// Parse the page's own CSS (`<style>` elements)
    fn page_stylesheet(&self, document: &Document) -> Option<Stylesheet> {
        let css_text = self.extract_css_from_document(document);
        if css_text.is_empty() {
            return None;
        }
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

    /// Compute an element's style: browser defaults, then the page's
    /// matching rules, then its `style` attribute
    fn compute_element_style(
        &self,
        tree: &DomTree,
        node_id: NodeId,
        element: &fos_dom::ElementData,
        stylesheet: Option<&Stylesheet>,
    ) -> ComputedStyle {
        let mut style = ComputedStyle::default();
        let tag_name = tree.resolve(element.name.local);

        apply_default_styles(&mut style, tag_name);
        if let Some(ss) = stylesheet {
            self.apply_matching_rules(tree, node_id, element, tag_name, ss, &mut style);
        }
        for attr in element.attrs.iter() {
            if tree.resolve(attr.name.local) == "style" {
                self.apply_inline_style(&attr.value, &mut style);
            }
        }
        style
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

    /// Paint the part of `layout` starting at document y `scroll_offset`.
    ///
    /// Only lines intersecting the canvas are shaped and drawn.
    fn paint(&mut self, layout: &PageLayout, scroll_offset: f32, height: u32, links: &mut Vec<LinkRegion>) -> Option<Vec<u32>> {
        let mut canvas = Canvas::filled(self.viewport_width, height, Color::WHITE)?;

        if layout.lines.is_empty() && layout.rules.is_empty() {
            // Clipped to the canvas like any text
            self.paint_text(&mut canvas, "Page loaded but no visible content", 20.0, 50.0 - scroll_offset, Color::rgb(100, 100, 100), 16.0);
            return Some(canvas.into_argb32());
        }

        let canvas_height = canvas.height() as f32;
        // Glyphs extend at most ~2 font sizes above and below a baseline
        let reach = layout.max_font_size.max(16.0) * 2.0;
        let top = scroll_offset - reach;
        let bottom = scroll_offset + canvas_height + reach;

        let first = layout.lines.partition_point(|line| line.y < top);
        for line in &layout.lines[first..] {
            if line.y > bottom {
                break;
            }
            let y = line.y - scroll_offset;
            let mut x = line.x;
            for segment in &line.segments {
                // Painting returns the advance, so the text is shaped only once
                let width = self.paint_text(&mut canvas, &segment.text, x, y, segment.color, segment.font_size);

                // Text is drawn with its baseline at y, so the region spans
                // the line above it. Lines just outside the canvas are painted
                // for their overhang, but their links are not on it.
                let height = segment.font_size * 1.2;
                if let Some(href) = segment.href.as_ref().filter(|_| y > 0.0 && y - height < canvas_height) {
                    links.push(LinkRegion {
                        x,
                        y: y - height,
                        width,
                        height,
                        href: href.clone(),
                    });
                }

                x += width;
            }
        }

        let first_rule = layout.rules.partition_point(|rule| rule.y < top);
        for rule in layout.rules[first_rule..].iter().take_while(|rule| rule.y <= bottom) {
            canvas.fill_rect(rule.x, rule.y - scroll_offset, rule.width, 1.0, Color::rgb(128, 128, 128));
        }

        Some(canvas.into_argb32())
    }

    /// Text painting using TextRenderer with proper fonts.
    ///
    /// Returns the advance width of the painted text.
    fn paint_text(
        &mut self,
        canvas: &mut Canvas,
        text: &str,
        x: f32,
        y: f32,
        color: Color,
        font_size: f32,
    ) -> f32 {
        // Use TextRenderer if we have a font
        if let Some(font_id) = self.default_font {
            // Use the proper font rendering
            return self.text_renderer.draw_text(canvas, text, x, y, font_id, font_size, color);
        }

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

        x_pos - x
    }
}

/// Cache key identifying a page's HTML and base URL
fn page_key(html: &str, base_url: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    html.len().hash(&mut hasher);
    html.hash(&mut hasher);
    base_url.hash(&mut hasher);
    hasher.finish()
}

/// Anchor positions relative to a buffer starting at document y `origin`
fn anchors_from(layout: &PageLayout, origin: f32) -> Vec<AnchorPosition> {
    layout.anchors.iter()
        .map(|a| AnchorPosition { id: a.id.clone(), y: a.y - origin })
        .collect()
}

/// Computes element styles on demand while laying out.
///
/// Layout visits each element once and needs its style only while visiting
/// it, so styles are never stored for the whole tree: memory stays
/// proportional to the tree's depth, and elements layout skips (`<head>`,
/// scripts, hidden subtrees) are never styled at all.
struct Styler<'a> {
    renderer: &'a PageRenderer,
    tree: &'a DomTree,
    stylesheet: Option<Stylesheet>,
}

impl Styler<'_> {
    /// Style of an element (`None` for other nodes)
    fn style(&self, node_id: NodeId) -> Option<ComputedStyle> {
        let element = self.tree.get(node_id)?.as_element()?;
        Some(self.renderer.compute_element_style(self.tree, node_id, element, self.stylesheet.as_ref()))
    }
}

/// Lay out the document body into lines for a viewport of `width` pixels
fn build_layout(document: &Document, styler: &Styler<'_>, width: u32) -> PageLayout {
    let tree = document.tree();
    let body = document.body();

    log::debug!("DOM tree size: {}, body valid: {}", tree.len(), body.is_valid());

    let mut builder = LayoutBuilder {
        tree,
        styler,
        // Leave margin for the right edge
        line_buffer: LineBuffer::new(8.0, width as f32 - 30.0, 16.0),
        // Document y of the first line
        y: 20.0,
        layout: PageLayout::default(),
    };

    if body.is_valid() {
        builder.layout_node(body);
        builder.flush();
    } else {
        log::error!("Body element not valid!");
    }

    let mut layout = builder.layout;
    layout.content_height = builder.y.max(0.0);
    layout
}

/// Walks the DOM, accumulating inline text into lines
struct LayoutBuilder<'a> {
    tree: &'a DomTree,
    styler: &'a Styler<'a>,
    line_buffer: LineBuffer,
    /// Current document y (baseline of the next line)
    y: f32,
    layout: PageLayout,
}

impl LayoutBuilder<'_> {
    /// Move buffered text into laid-out lines
    fn flush(&mut self) {
        self.line_buffer.flush(&mut self.y, &mut self.layout);
    }

    /// Lay out a node and its children with inline/block handling
    fn layout_node(&mut self, node_id: NodeId) {
        let tree = self.tree;
        let node = match tree.get(node_id) {
            Some(n) => n,
            None => return,
        };

        // Get style
        let style = self.styler.style(node_id);

        // Check if hidden
        if style.as_ref().is_some_and(|s| matches!(s.display, Display::None)) {
            return;
        }

        // If text node, add to line buffer (collapsing whitespace)
        if let Some(text) = node.as_text() {
            let mut words = text.split_whitespace();
            if let Some(first) = words.next() {
                let mut collapsed = String::with_capacity(text.len());
                collapsed.push_str(first);
                for word in words {
                    collapsed.push(' ');
                    collapsed.push_str(word);
                }

                let font_size = self.line_buffer.current_font_size.max(14.0);
                let text_color = self.line_buffer.current_color;
                self.line_buffer.add_text(&collapsed, font_size, text_color, node.parent);
            }
            return;
        }

        // If element, handle block vs inline
        let Some(element) = node.as_element() else { return };

        // HTML tag names are already lowercase after parsing
        let tag_name = tree.resolve(element.name.local);
        let lowered;
        let tag: &str = if tag_name.bytes().any(|b| b.is_ascii_uppercase()) {
            lowered = tag_name.to_ascii_lowercase();
            &lowered
        } else {
            tag_name
        };

        // Skip elements that never render
        if matches!(tag, "script" | "style" | "noscript" | "template" | "head") {
            return;
        }

        let is_block = matches!(tag,
            "div" | "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" |
            "ul" | "ol" | "li" | "section" | "article" | "header" | "footer" |
            "main" | "nav" | "aside" | "figure" | "figcaption" | "blockquote" |
            "pre" | "hr" | "br" | "table" | "tr" | "form" | "td" | "th");

        let font_size = style.as_ref().map(|s| s.font_size).unwrap_or(self.line_buffer.current_font_size);

        // Block elements flush the line buffer and add vertical space
        if is_block {
            self.flush();
            // Better spacing based on element type
            let margin_before = match tag {
                "h1" => 20.0,
                "h2" => 16.0,
                "h3" | "h4" => 12.0,
                "p" => 8.0,
                "ul" | "ol" => 6.0,
                "li" => 2.0,
                "td" | "th" | "tr" => 2.0,
                _ => font_size * 0.3,
            };
            self.y += margin_before;
        }

        // Handle special elements
        if tag == "br" {
            self.flush();
            return;
        }

        if tag == "hr" {
            self.flush();
            let line_buffer = &self.line_buffer;
            self.layout.rules.push(Rule {
                x: line_buffer.start_x,
                y: self.y,
                width: line_buffer.max_width - line_buffer.start_x - 20.0,
            });
            self.y += 10.0;
            return;
        }

        let line_buffer = &mut self.line_buffer;

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
        if (tag == "td" || tag == "th") && line_buffer.current_x > line_buffer.effective_start_x() + 5.0 {
            // Add cell separator if not first in row
            let font_size = line_buffer.current_font_size;
            line_buffer.add_text(" | ", font_size, Color::rgb(180, 180, 180), node_id);
        }

        // Table headers get slightly bold look (darker color)
        if tag == "th" {
            line_buffer.current_color = Color::rgb(40, 40, 40);
        }

        // List items get a bullet or number marker
        if tag == "li" {
            self.flush();
            let line_buffer = &mut self.line_buffer;
            // Add marker based on list type
            let font_size = line_buffer.current_font_size;
            let color = line_buffer.current_color;
            if line_buffer.list_counter > 0 {
                // Ordered list - show number
                let marker = format!("{}. ", line_buffer.list_counter);
                line_buffer.add_text(&marker, font_size, color, node_id);
                line_buffer.list_counter += 1;
            } else {
                // Unordered list - show bullet
                line_buffer.add_text("• ", font_size, color, node_id);
            }
        }

        let line_buffer = &mut self.line_buffer;

        // Set font size based on heading
        match tag {
            "h1" => line_buffer.current_font_size = 28.0,
            "h2" => line_buffer.current_font_size = 24.0,
            "h3" => line_buffer.current_font_size = 20.0,
            "h4" => line_buffer.current_font_size = 18.0,
            "h5" => line_buffer.current_font_size = 16.0,
            "h6" => line_buffer.current_font_size = 14.0,
            "small" => line_buffer.current_font_size = (saved_font_size * 0.8).max(12.0),
            _ => {}
        };

        // Single pass over attributes: link target, anchor id, inline color
        for attr in element.attrs.iter() {
            match tree.resolve(attr.name.local) {
                "href" if tag == "a" => {
                    line_buffer.current_href = Some(attr.value.to_string());
                }
                "id" if !attr.value.is_empty() => {
                    // Record element ID for anchor navigation
                    self.layout.anchors.push(AnchorPosition {
                        id: attr.value.to_string(),
                        y: self.y,
                    });
                }
                "style" => {
                    if let Some(color) = parse_color_from_style(&attr.value) {
                        line_buffer.current_color = color;
                    }
                }
                _ => {}
            }
        }

        // Links get blue color (unless styled inline)
        if tag == "a" && line_buffer.current_color == saved_color {
            line_buffer.current_color = Color::rgb(51, 102, 204); // Wikipedia link blue
        }

        // Recurse into children
        for (child_id, _) in tree.children(node_id) {
            self.layout_node(child_id);
        }

        // Restore state
        let line_buffer = &mut self.line_buffer;
        line_buffer.current_font_size = saved_font_size;
        line_buffer.current_color = saved_color;
        line_buffer.indent_level = saved_indent;
        line_buffer.current_href = saved_href;
        line_buffer.list_counter = saved_list_counter;
        line_buffer.current_x = line_buffer.effective_start_x();

        // Block elements flush after and add space
        if is_block {
            self.flush();
            // Better spacing based on element type
            let margin_after = match tag {
                "h1" => 12.0,
                "h2" => 10.0,
                "h3" | "h4" => 8.0,
                "p" => 12.0,  // Paragraphs need good separation
                "li" => 2.0,
                _ => 4.0,
            };
            self.y += margin_after;
        }
    }
}

/// Parse color from inline style attribute
fn parse_color_from_style(style: &str) -> Option<Color> {
    for part in style.split(';') {
        let Some((name, value)) = part.split_once(':') else { continue };
        if name.trim().eq_ignore_ascii_case("color") {
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
    let value = value.trim().trim_end_matches("!important").trim().to_ascii_lowercase();

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
        if !hex.is_ascii() {
            return None;
        }
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
    /// Pending segments; `None` marks a line break
    segments: Vec<Option<TextSegment>>,
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

    fn add_text_with_measure<F: Fn(&str, f32) -> f32>(&mut self, text: &str, font_size: f32, color: Color, node: NodeId, measure: F) {
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
                    self.segments.push(None);
                }
                self.current_x = effective_start;
            }

            // Add the text segment
            let mut segment_text = String::with_capacity(trimmed.len() + 1);
            segment_text.push_str(trimmed);
            segment_text.push(' ');
            self.segments.push(Some(TextSegment {
                text: segment_text,
                font_size,
                color,
                href: self.current_href.clone(),
                node,
            }));

            self.current_x += line_width + space_width;
        }
    }

    // Keep fallback without measure function for backwards compatibility
    fn add_text(&mut self, text: &str, font_size: f32, color: Color, node: NodeId) {
        // Fallback using approximate character width
        let char_width = font_size * 0.5;
        self.add_text_with_measure(text, font_size, color, node, |s, _| s.chars().count() as f32 * char_width);
    }

    /// Emit the buffered text as lines starting at `y_cursor`
    fn flush(&mut self, y_cursor: &mut f32, layout: &mut PageLayout) {
        if self.segments.is_empty() {
            return;
        }

        let effective_start = self.effective_start_x();
        let line_height = self.current_font_size * 1.3;

        let mut line = LaidOutLine { y: *y_cursor, x: effective_start, segments: Vec::new() };
        for segment in self.segments.drain(..) {
            match segment {
                Some(segment) => {
                    layout.max_font_size = layout.max_font_size.max(segment.font_size);
                    line.segments.push(segment);
                }
                None => {
                    *y_cursor += line_height;
                    let next = LaidOutLine { y: *y_cursor, x: effective_start, segments: Vec::new() };
                    let done = std::mem::replace(&mut line, next);
                    if !done.segments.is_empty() {
                        layout.lines.push(done);
                    }
                }
            }
        }
        if !line.segments.is_empty() {
            layout.lines.push(line);
        }

        *y_cursor += line_height;
        self.current_x = effective_start;
    }
}

/// Apply default user-agent styles based on element type
fn apply_default_styles(style: &mut ComputedStyle, tag_name: &str) {
    let lowered;
    let tag: &str = if tag_name.bytes().any(|b| b.is_ascii_uppercase()) {
        lowered = tag_name.to_ascii_lowercase();
        &lowered
    } else {
        tag_name
    };

    match tag {
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

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<!DOCTYPE html><html><head><title>T</title></head><body>
        <h1 id="top">Heading</h1>
        <p>First paragraph with <a href="/next">a link</a>.</p>
        <hr>
        <ul><li>One</li><li>Two</li></ul>
        <p id="end">Last paragraph.</p>
    </body></html>"#;

    #[test]
    fn test_render_produces_viewport_sized_buffer() {
        let mut renderer = PageRenderer::new(320, 240);
        let page = renderer.render_html(PAGE, "https://example.com/", 0.0).unwrap();
        assert_eq!(page.pixels.len(), 320 * 240);
        assert_eq!((page.width, page.height), (320, 240));
        assert!(page.anchors.iter().any(|a| a.id == "top"));
        assert!(page.content_height > 0.0);
        assert!(page.links.iter().any(|l| l.href == "/next"));
    }

    #[test]
    fn test_layout_is_cached_between_renders() {
        let mut renderer = PageRenderer::new(320, 240);
        renderer.render_html(PAGE, "https://example.com/", 0.0).unwrap();
        let key = renderer.cached.as_ref().unwrap().source;

        // Scrolling reuses the layout
        renderer.render_html(PAGE, "https://example.com/", 100.0).unwrap();
        assert_eq!(renderer.cached.as_ref().unwrap().source, key);

        // A new width re-lays out the same page
        renderer.set_viewport(200, 240);
        renderer.render_html(PAGE, "https://example.com/", 0.0).unwrap();
        assert_eq!(renderer.cached.as_ref().unwrap().width, 200);

        // A different page replaces it
        renderer.render_html("<p>other</p>", "https://example.com/", 0.0).unwrap();
        assert_ne!(renderer.cached.as_ref().unwrap().source, key);

        renderer.clear_cache();
        assert!(renderer.cached.is_none());
    }

    #[test]
    fn test_document_layout_follows_dom_mutations() {
        let mut document = fos_html::parse_with_url(PAGE, "https://example.com/");
        let mut renderer = PageRenderer::new(320, 240);

        let before = renderer.render_document(&document, 0.0).unwrap();
        assert!(renderer.is_layout_current(&document));
        let source = renderer.cached.as_ref().unwrap().source;

        // Re-rendering an unchanged DOM reuses the layout
        renderer.render_document(&document, 50.0).unwrap();
        assert_eq!(renderer.cached.as_ref().unwrap().source, source);

        // A mutation (as a script would make) invalidates it
        let body = document.body();
        let tree = document.tree_mut();
        for i in 0..20 {
            let p = tree.create_element("p");
            let text = tree.create_text(&format!("Added by script {i}"));
            tree.append_child(p, text);
            tree.append_child(body, p);
        }
        assert!(!renderer.is_layout_current(&document));

        let after = renderer.render_document(&document, 0.0).unwrap();
        assert!(renderer.is_layout_current(&document));
        assert!(after.content_height > before.content_height);
    }

    /// A page tall enough to scroll, with links spread through it
    fn long_page() -> String {
        let mut html = String::from("<html><body>");
        for i in 0..200 {
            html.push_str(&format!("<p id=\"p{i}\">Paragraph {i} with <a href=\"/l{i}\">a link</a> and text.</p>"));
        }
        html.push_str("</body></html>");
        html
    }

    #[test]
    fn test_scrolled_render_matches_full_render() {
        let document = fos_html::parse_with_url(&long_page(), "https://example.com/");
        let mut renderer = PageRenderer::new(300, 240);
        let mut page = renderer.render_document(&document, 0.0).unwrap();

        // Down, further down, back up (both band positions), and a jump
        // too far to reuse anything
        for offset in [100.0, 339.0, 250.0, 17.0, 1500.0, 1400.0] {
            page = renderer.render_document_scrolled(&document, offset, page).unwrap();
            let mut fresh = PageRenderer::new(300, 240);
            let full = fresh.render_document(&document, offset).unwrap();

            assert!(page.pixels == full.pixels, "pixels differ at offset {offset}");
            let mut got: Vec<_> = page.links.iter().map(|l| (l.href.clone(), l.y.round() as i32)).collect();
            let mut want: Vec<_> = full.links.iter().map(|l| (l.href.clone(), l.y.round() as i32)).collect();
            got.sort();
            want.sort();
            assert_eq!(got, want, "links differ at offset {offset}");
            assert_eq!(page.anchors.len(), full.anchors.len());
            assert_eq!(page.anchors[5].y, full.anchors[5].y);
        }
    }

    #[test]
    fn test_scrolled_render_after_mutation_is_full() {
        let mut document = fos_html::parse_with_url(&long_page(), "https://example.com/");
        let mut renderer = PageRenderer::new(300, 240);
        let page = renderer.render_document(&document, 0.0).unwrap();

        let body = document.body();
        let tree = document.tree_mut();
        let p = tree.create_element("p");
        let text = tree.create_text("Inserted at the end");
        tree.append_child(p, text);
        tree.append_child(body, p);

        let scrolled = renderer.render_document_scrolled(&document, 50.0, page).unwrap();
        let full = PageRenderer::new(300, 240).render_document(&document, 50.0).unwrap();
        assert!(scrolled.pixels == full.pixels);
        assert_eq!(scrolled.content_height, full.content_height);
    }

    #[test]
    fn test_anchors_and_links_use_buffer_coordinates() {
        let mut renderer = PageRenderer::new(320, 240);
        let at_top = renderer.render_html(PAGE, "https://example.com/", 0.0).unwrap();
        let scrolled = renderer.render_html(PAGE, "https://example.com/", 30.0).unwrap();

        let y = |page: &RenderedPage, id: &str| page.anchors.iter().find(|a| a.id == id).unwrap().y;
        assert_eq!(y(&at_top, "end") - y(&scrolled, "end"), 30.0);

        let link_y = |page: &RenderedPage| page.links.iter().find(|l| l.href == "/next").unwrap().y;
        assert_eq!(link_y(&at_top) - link_y(&scrolled), 30.0);

        // Scrolled far below the content: nothing is visible or clickable
        let past_end = renderer.render_html(PAGE, "https://example.com/", 10_000.0).unwrap();
        assert!(past_end.links.is_empty());
    }

    #[test]
    fn test_layout_lines_are_ordered() {
        let document = fos_html::parse_with_url(PAGE, "https://example.com/");
        let renderer = PageRenderer::new(320, 240);
        let styler = Styler { renderer: &renderer, tree: document.tree(), stylesheet: renderer.page_stylesheet(&document) };
        let layout = build_layout(&document, &styler, 320);

        assert!(!layout.lines.is_empty());
        assert!(layout.lines.windows(2).all(|w| w[0].y <= w[1].y));
        assert_eq!(layout.rules.len(), 1);
        assert!(layout.content_height >= layout.lines.last().unwrap().y);
    }

    #[test]
    fn test_empty_page_does_not_panic() {
        let mut renderer = PageRenderer::new(320, 240);
        let page = renderer.render_html("<html><body></body></html>", "about:blank", 0.0).unwrap();
        assert_eq!(page.pixels.len(), 320 * 240);
    }

    #[test]
    fn test_parse_color_from_style() {
        assert_eq!(parse_color_from_style("color: red"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(parse_color_from_style("font-weight:bold; COLOR:#00f"), Some(Color::rgb(0, 0, 255)));
        assert_eq!(parse_color_from_style("background-color: red"), None);
        assert_eq!(parse_color_from_style("color: #ff0000 !important"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(parse_color_from_style("color: #é1"), None);
    }
}
