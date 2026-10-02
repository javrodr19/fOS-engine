//! Rendering Pipeline
//!
//! Rendering is split in two phases:
//! 1. **Layout** styles the DOM and lays it out with the box layout engine
//!    (`fos_layout::engine`) into a fragment tree: positioned boxes and
//!    shaped text, plus the page's link regions and anchors.
//! 2. **Paint** draws the part of the fragment tree inside the requested
//!    band (`crate::paint`).
//!
//! The layout is cached, so scrolling and re-rendering the same page only
//! repaint. Styles are computed during box tree construction and kept only
//! on the fragments that need them. A layout built from a live DOM is keyed
//! by the tree's revision, so any DOM mutation (from scripts, for example)
//! is picked up by the next render.

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use crate::page_styles::{AncestorFilter, PageStyles};
use fos_css::style::{Style, StyleContext};
use fos_dom::{Document, DomRevision, DomTree, NodeId};
use fos_layout::engine::{self as layout_engine, BoxFragment, BoxFragmentKind, FontContext, Fragment, FragmentTree, Rect};
use fos_render::{Canvas, Color, TextRenderer};

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

/// A page laid out at one viewport size
#[derive(Debug)]
pub struct PageLayout {
    fragments: FragmentTree,
    /// Link regions in document coordinates, by top edge
    links: Vec<(Rect, Arc<str>)>,
    /// Element ids and their document y positions
    anchors: Vec<AnchorPosition>,
    /// The canvas color (the root's or body's background)
    background: fos_css::properties::Color,
    /// The element whose background is the canvas's
    background_box: Option<NodeId>,
}

impl PageLayout {
    /// Every element's border boxes (inline elements have one per line)
    /// and text rectangles, in document coordinates and tree order
    pub fn boxes(&self) -> Vec<(NodeId, Rect)> {
        self.fragments.element_rects()
    }

    /// Height of the laid-out document
    pub fn content_height(&self) -> f32 {
        self.fragments.document_height
    }

    /// The laid-out fragments
    pub fn fragments(&self) -> &FragmentTree {
        &self.fragments
    }

    /// The deepest element at document point (x, y)
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.fragments.hit_test(x, y)
    }
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
    /// Shared with the page's scripts (element geometry)
    layout: Arc<PageLayout>,
}

/// Page renderer - integrates HTML, CSS, layout, and painting
pub struct PageRenderer {
    /// Viewport width
    viewport_width: u32,
    /// Viewport height
    viewport_height: u32,
    /// Glyph rasterization and caching
    text_renderer: TextRenderer,
    /// Font matching, metrics and shaped words for layout
    fonts: FontContext,
    /// Layout of the most recently rendered page
    cached: Option<CachedLayout>,
    /// Incremented for every new layout, to tell whether a rendered
    /// buffer was painted from the current one
    layout_generation: u64,
    /// The current page's external stylesheets
    stylesheets: crate::css_loader::Stylesheets,
}

impl PageRenderer {
    pub fn new(viewport_width: u32, viewport_height: u32) -> Self {
        let text_renderer = TextRenderer::new();
        if text_renderer.fonts.is_empty() {
            log::warn!("No system font found, text will not be drawn");
        }
        let fonts = FontContext::new(text_renderer.fonts.clone());
        Self {
            viewport_width,
            viewport_height,
            text_renderer,
            fonts,
            cached: None,
            layout_generation: 0,
            stylesheets: Default::default(),
        }
    }

    /// Set viewport size
    pub fn set_viewport(&mut self, width: u32, height: u32) {
        if height != self.viewport_height {
            // Viewport units and percentages of the root depend on it
            self.cached = None;
        }
        self.viewport_width = width;
        self.viewport_height = height;
    }

    /// Drop the cached layout, glyphs and shaped words (e.g. on memory
    /// pressure)
    pub fn clear_cache(&mut self) {
        self.cached = None;
        self.text_renderer.clear_glyph_cache();
        self.fonts = FontContext::new(self.text_renderer.fonts.clone());
    }

    /// Width of `text` in the default sans-serif font
    pub fn measure_text(&mut self, text: &str, font_size: f32) -> f32 {
        let mut style = Style::default();
        let i = Arc::make_mut(&mut style.inherited);
        i.font_size = font_size;
        i.font_family = Arc::from([Arc::from("sans-serif")]);
        let font = self.fonts.resolve(&style);
        self.fonts.shape(&font, text).width
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

    /// The element at `(x, y)` in document coordinates, per the current
    /// layout
    pub fn node_at(&mut self, x: f32, y: f32) -> Option<NodeId> {
        self.cached.as_ref()?.layout.hit_test(x, y)
    }

    /// Whether the cached layout reflects `document` as it is now
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
        let stylesheet = self.page_stylesheet(document);
        let layout = build_layout(document, stylesheet, &mut self.fonts, (width as f32, self.viewport_height as f32));
        self.cached = Some(CachedLayout { source, width, layout: Arc::new(layout) });
        self.layout_generation += 1;
    }

    /// Paint the visible region of the cached layout
    fn paint_cached(&mut self, scroll_offset: f32) -> Option<RenderedPage> {
        let cached = self.cached.take()?;
        let painted = self.paint(&cached.layout, scroll_offset, self.viewport_height);
        let content_height = cached.layout.content_height();
        let links = links_in(&cached.layout, scroll_offset, self.viewport_height as f32);
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
        let band_top = if delta > 0 {
            page.pixels.copy_within(shift * width.., 0);
            height - shift
        } else {
            page.pixels.copy_within(..(height - shift) * width, shift * width);
            0
        };

        let band_origin = scroll_offset + band_top as f32;
        let band = self.paint(&cached.layout, band_origin, shift as u32);
        page.anchors = anchors_from(&cached.layout, scroll_offset);
        page.links = links_in(&cached.layout, scroll_offset, page.height as f32);
        self.cached = Some(cached);
        page.pixels[band_top * width..(band_top + shift) * width].copy_from_slice(&band?);
        page.origin = scroll_offset;
        Some(page)
    }

    /// Parse the page's own CSS (`<style>` elements) and compile it for
    /// matching
    fn page_stylesheet(&self, document: &Document) -> Option<PageStyles> {
        let css_text = self.extract_css_from_document(document);
        if css_text.is_empty() {
            return None;
        }
        let media = fos_css::MediaContext { width: self.viewport_width as f32, height: self.viewport_height as f32 };
        let styles = PageStyles::new(fos_css::parse_stylesheet_for(&css_text, media));
        log::debug!("Parsed {} CSS rules from page", styles.rule_count());
        Some(styles)
    }

    /// The current layout, for element geometry (`getBoundingClientRect`)
    pub fn layout_snapshot(&self) -> Option<Arc<PageLayout>> {
        self.cached.as_ref().map(|c| c.layout.clone())
    }

    /// Use `sheets` for the page's `<link rel="stylesheet">` elements
    /// (fetched by the browser); the layout is redone if they changed
    pub fn set_stylesheets(&mut self, sheets: crate::css_loader::Stylesheets) {
        if !Arc::ptr_eq(&self.stylesheets, &sheets) {
            self.stylesheets = sheets;
            self.cached = None;
        }
    }

    /// The page's CSS: `<style>` contents and fetched `<link>` sheets, in
    /// document order (cascade order), skipping those whose `media`
    /// attribute does not match the viewport
    fn extract_css_from_document(&self, document: &Document) -> String {
        let tree = document.tree();
        let media = fos_css::MediaContext { width: self.viewport_width as f32, height: self.viewport_height as f32 };
        let mut base: Option<String> = None;
        let mut css = String::new();
        fos_dom::selector::walk_elements(tree, tree.root(), &mut |id| {
            let Some(e) = tree.get(id).and_then(|n| n.as_element()) else { return true };
            let tag = tree.resolve(e.name.local);
            let is_style = tag.eq_ignore_ascii_case("style");
            let is_link = !is_style && crate::css_loader::is_stylesheet_link(tree, id);
            if !is_style && !is_link {
                return true;
            }
            if let Some(m) = tree.get_attribute(id, "media") {
                if !fos_css::media_matches(m, &media) {
                    return true;
                }
            }
            if is_style {
                let mut text = String::new();
                for (_, child) in tree.children(id) {
                    if let Some(t) = child.as_text() {
                        text.push_str(t);
                    }
                }
                // A sheet scripts changed through the CSSOM (`insertRule`)
                css.push_str(document.sheet_override(id, &text).unwrap_or(&text));
                css.push('\n');
            } else if let Some(href) = tree.get_attribute(id, "href") {
                let base = base.get_or_insert_with(|| crate::css_loader::base_url(document));
                let url = fos_net::url_util::resolve(base, href.trim());
                if let Some(text) = self.stylesheets.get(&url) {
                    css.push_str(text);
                    css.push('\n');
                }
            }
            true
        });
        css.push_str(document.adopted_css());
        css
    }

    /// Paint the band of `layout` starting at document y `origin`,
    /// `height` rows tall
    fn paint(&mut self, layout: &PageLayout, origin: f32, height: u32) -> Option<Vec<u32>> {
        let bg = layout.background;
        let mut canvas = Canvas::filled(self.viewport_width, height, Color::rgba(bg.r, bg.g, bg.b, 255))?;
        let mut painter = crate::paint::Painter::new(&mut canvas, &mut self.text_renderer, origin, layout.background_box);
        painter.paint(&layout.fragments);
        Some(canvas.into_argb32())
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
    layout.anchors.iter().map(|a| AnchorPosition { id: a.id.clone(), y: a.y - origin }).collect()
}

/// Link regions visible in a buffer `height` rows tall at document y
/// `origin`, in buffer coordinates
fn links_in(layout: &PageLayout, origin: f32, height: f32) -> Vec<LinkRegion> {
    layout
        .links
        .iter()
        .filter(|(r, _)| r.bottom() > origin && r.y < origin + height && r.w > 0.0 && r.h > 0.0)
        .map(|(r, href)| LinkRegion { x: r.x, y: r.y - origin, width: r.w, height: r.h, href: href.to_string() })
        .collect()
}

/// Computes element styles while the box tree is built: the UA's
/// defaults, the page's matching rules, presentational hints and `style`
/// attributes. Styles live only as long as layout needs them.
struct BrowserStyler<'a> {
    stylesheet: Option<&'a PageStyles>,
    /// Ancestors of the elements being styled
    ancestors: AncestorFilter,
    /// `var()` and math resolutions shared across elements
    resolved: fos_css::ResolveCache,
    /// What lengths are computed against (the root's font size once known)
    ctx: StyleContext,
    root_styled: bool,
}

impl layout_engine::Styler for BrowserStyler<'_> {
    fn style(&mut self, tree: &DomTree, node: NodeId, parent: &Style) -> Style {
        let Some(element) = tree.get(node).and_then(|n| n.as_element()) else { return Style::inherit_from(parent) };
        let mut style = Style::inherit_from(parent);
        let inline: Vec<fos_css::Declaration> = element
            .attrs
            .iter()
            .filter(|a| tree.resolve(a.name.local) == "style")
            .flat_map(|a| fos_css::parse_declarations(&a.value))
            .collect();
        let filter = self.stylesheet.is_some().then_some(&self.ancestors);
        crate::page_styles::cascade(self.stylesheet, tree, node, element, filter, &inline, &mut style, parent, &self.ctx, &mut self.resolved);
        if !self.root_styled {
            // The root element's font size is what `rem` means
            self.root_styled = true;
            self.ctx.root_font_size = style.font_size();
        }
        style
    }

    fn enter(&mut self, tree: &DomTree, node: NodeId) {
        if self.stylesheet.is_some() {
            self.ancestors.push(tree, node);
        }
    }

    fn leave(&mut self) {
        if self.stylesheet.is_some() {
            self.ancestors.pop();
        }
    }
}

/// Lay out `document` in a viewport
fn build_layout(document: &Document, stylesheet: Option<PageStyles>, fonts: &mut FontContext, viewport: (f32, f32)) -> PageLayout {
    let tree = document.tree();
    let root = document.document_element();
    let mut styler = BrowserStyler {
        stylesheet: stylesheet.as_ref(),
        ancestors: AncestorFilter::default(),
        resolved: Default::default(),
        ctx: StyleContext { root_font_size: 16.0, viewport },
        root_styled: false,
    };
    let fragments = if root.is_valid() {
        layout_engine::layout_document(tree, root, &mut styler, fonts, viewport)
    } else {
        FragmentTree { root: None, document_height: viewport.1, document_width: viewport.0 }
    };
    drop(styler);

    // Links and anchors
    let mut links: Vec<(Rect, Arc<str>)> = Vec::new();
    fn walk(tree: &DomTree, b: &BoxFragment, links: &mut Vec<(Rect, Arc<str>)>) {
        if b.node.is_valid() {
            let is_link = tree.get(b.node).and_then(|n| n.as_element()).is_some_and(|e| tree.resolve(e.name.local) == "a");
            if is_link {
                if let Some(href) = tree.get_attribute(b.node, "href") {
                    // A block link's area is its box; an inline link's, each
                    // line's part
                    let r = if b.kind == BoxFragmentKind::InlinePart { b.border_box.union(&b.ink) } else { b.border_box };
                    links.push((r, Arc::from(href)));
                }
            }
        }
        for c in &b.children {
            if let Fragment::Box(cb) = c {
                walk(tree, cb, links);
            }
        }
    }
    if let Some(r) = &fragments.root {
        walk(tree, r, &mut links);
    }
    links.sort_by(|a, b| a.0.y.total_cmp(&b.0.y));

    let mut anchors = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (node, rect) in fragments.element_rects() {
        if seen.insert(node) {
            if let Some(id) = tree.get_attribute(node, "id").filter(|id| !id.is_empty()) {
                anchors.push(AnchorPosition { id: id.to_string(), y: rect.y });
            }
        }
    }

    let body = document.body();
    let (background, background_box) = crate::paint::canvas_background(&fragments, |b| b.node == body);
    PageLayout { fragments, links, anchors, background, background_box }
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

    fn layout_of(document: &Document, width: f32) -> PageLayout {
        let renderer = PageRenderer::new(width as u32, 240);
        let sheet = renderer.page_stylesheet(document);
        build_layout(document, sheet, &mut FontContext::default(), (width, 240.0))
    }

    /// The text fragments of the element whose text is `text`
    fn texts<'a>(document: &Document, layout: &'a PageLayout, text: &str) -> Vec<&'a layout_engine::TextFragment> {
        let tree = document.tree();
        let mut owner = NodeId::NONE;
        fos_dom::selector::walk_elements(tree, tree.root(), &mut |id| {
            let own: String = tree.children(id).filter_map(|(_, n)| n.as_text()).collect();
            if own.trim() == text {
                owner = id;
            }
            true
        });
        let mut out = Vec::new();
        fn walk<'a>(b: &'a BoxFragment, owner: NodeId, out: &mut Vec<&'a layout_engine::TextFragment>) {
            for c in &b.children {
                match c {
                    Fragment::Text(t) if t.node == owner => out.push(t),
                    Fragment::Box(cb) => walk(cb, owner, out),
                    _ => {}
                }
            }
        }
        walk(layout.fragments.root.as_ref().unwrap(), owner, &mut out);
        out
    }

    #[test]
    fn test_layout_boxes_follow_the_document() {
        let document = fos_html::parse_with_url(PAGE, "https://example.com/");
        let layout = layout_of(&document, 320.0);
        let boxes = layout.boxes();
        assert!(!boxes.is_empty());
        // Block boxes come in document order, top to bottom
        let tree = document.tree();
        let tops: Vec<f32> = boxes
            .iter()
            .filter(|(n, _)| tree.get(*n).and_then(|n| n.as_element()).is_some_and(|e| matches!(tree.resolve(e.name.local), "h1" | "p" | "ul")))
            .map(|(_, r)| r.y)
            .collect();
        assert!(tops.len() >= 4 && tops.windows(2).all(|w| w[0] <= w[1]), "{tops:?}");
        // The rule is a bordered block across the body
        let hr = boxes.iter().find(|(n, _)| tree.get(*n).and_then(|n| n.as_element()).is_some_and(|e| tree.resolve(e.name.local) == "hr")).unwrap();
        assert!(hr.1.w > 250.0 && hr.1.h >= 2.0, "{:?}", hr.1);
        assert!(layout.content_height() >= tops[tops.len() - 1]);
        assert!(layout.anchors.iter().any(|a| a.id == "end"));
    }

    #[test]
    fn test_empty_page_does_not_panic() {
        let mut renderer = PageRenderer::new(320, 240);
        let page = renderer.render_html("<html><body></body></html>", "about:blank", 0.0).unwrap();
        assert_eq!(page.pixels.len(), 320 * 240);
    }

    #[test]
    fn test_page_css_reaches_layout() {
        let html = r#"<html><head><style>
            .big { color: red; font-size: 32px }
            nav a { color: #008000 }
            .rel { font-size: 1.5em }
            @media (max-width: 100px) { .big { color: blue } }
            html { font-size: 20px }
            :root { --accent: #00f; --pad: 2px }
            .var { color: var(--accent); font-size: calc(1rem + var(--pad)) }
            .fallback { color: var(--missing, #f0f) }
            </style></head><body>
            <p class="var">vars</p><p class="fallback">fallback</p>
            <p style="color: #00f">inline</p>
            <p class="big">big <span class="rel">rel</span></p>
            <nav><a href="/x">nav link</a></nav><a href="/y">plain link</a>
            <p style="display: none">hidden</p>
            </body></html>"#;
        let document = fos_html::parse_with_url(html, "https://example.com/");
        let layout = layout_of(&document, 640.0);
        let seg = |text: &str| {
            let t = texts(&document, &layout, text);
            let t = t.first().unwrap_or_else(|| panic!("no text {text}"));
            (Color::rgba(t.color.r, t.color.g, t.color.b, t.color.a), t.font.size)
        };
        let rgb = |r, g, b| Color::rgb(r, g, b);
        assert_eq!(seg("inline").0, rgb(0, 0, 255));
        assert_eq!(seg("big"), (rgb(255, 0, 0), 32.0));
        // Inherited color, em relative to the parent's size
        assert_eq!(seg("rel"), (rgb(255, 0, 0), 48.0));
        assert_eq!(seg("nav link").0, rgb(0, 128, 0));
        // The UA stylesheet's link color
        assert_eq!(seg("plain link").0, rgb(0, 0, 238));
        assert!(texts(&document, &layout, "hidden").is_empty());
        // Custom properties from :root, calc() with rem of the root's 20px
        assert_eq!(seg("vars"), (rgb(0, 0, 255), 22.0));
        assert_eq!(seg("fallback").0, rgb(255, 0, 255));
        // The root's font size is inherited
        assert_eq!(seg("inline").1, 20.0);
    }
}
