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
    /// Some box is `position: fixed` (scrolling must repaint it)
    has_fixed: bool,
    /// The images the page's CSS uses (absolute URLs)
    css_images: Vec<String>,
    /// What relative CSS URLs (inline styles) resolve against
    base: String,
}

impl PageLayout {
    /// Every element's border boxes (inline elements have one per line)
    /// and text rectangles, in document coordinates and tree order
    pub fn boxes(&self) -> Vec<(NodeId, Rect)> {
        self.fragments.element_rects()
    }

    /// [`Self::boxes`] where they are painted: transformed, fixed and
    /// sticky boxes placed for page scroll `scroll` in a viewport
    /// `view_h` tall, and scroll containers' content moved by their
    /// offsets
    pub fn painted_boxes(&self, scroll: f32, view_h: f32, offsets: &std::collections::HashMap<u32, (f32, f32)>) -> Vec<(NodeId, Rect)> {
        self.fragments.painted_element_rects(scroll, view_h, &|n| offsets.get(&n.0).copied().unwrap_or((0.0, 0.0)))
    }

    /// Whether some box moves with the page scroll (fixed or sticky)
    pub fn has_fixed(&self) -> bool {
        self.has_fixed
    }

    /// Height of the laid-out document
    pub fn content_height(&self) -> f32 {
        self.fragments.document_height
    }

    /// The laid-out fragments
    pub fn fragments(&self) -> &FragmentTree {
        &self.fragments
    }

    /// Scroll metrics of every element with a box: visible (padding box)
    /// size and content size
    pub fn scroll_metrics(&self) -> std::collections::HashMap<u32, [f32; 4]> {
        let mut out = std::collections::HashMap::new();
        self.fragments.for_each(|f| {
            if let Fragment::Box(b) = f {
                if b.node.is_valid() && b.kind != BoxFragmentKind::InlinePart {
                    let pad = b.padding_box();
                    let (ew, eh) = b.scroll_extent.unwrap_or((pad.w, pad.h));
                    out.entry(b.node.0).or_insert([pad.w, pad.h, ew, eh]);
                }
            }
        });
        out
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
    /// The page's compiled CSS, by hash of its text and the viewport
    /// (relayouts after DOM changes rarely change the CSS)
    compiled_css: Option<(u64, Option<Arc<PageStyles>>)>,
    /// The scroll position last painted (where fixed boxes are)
    scroll: f32,
    /// The current page's decoded images
    images: crate::image_loader::Images,
    /// Inline `<svg>` elements, rasterized
    svgs: crate::image_loader::SvgCache,
    /// The page's web fonts (in `fonts`' database)
    web_fonts: crate::font_loader::WebFonts,
    /// What scripts drew on the page's canvas elements
    canvases: std::collections::HashMap<NodeId, Arc<crate::image_loader::LoadedImage>>,
    /// Scroll positions of the page's scroll containers
    box_scroll: std::collections::HashMap<NodeId, (f32, f32)>,
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
            compiled_css: None,
            scroll: 0.0,
            images: Default::default(),
            canvases: Default::default(),
            svgs: Default::default(),
            web_fonts: Default::default(),
            box_scroll: Default::default(),
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

    /// Lay out again on the next render (keeping caches)
    pub fn invalidate_layout(&mut self) {
        self.cached = None;
    }

    /// Drop the cached layout, glyphs and shaped words (e.g. on memory
    /// pressure)
    pub fn clear_cache(&mut self) {
        self.cached = None;
        self.compiled_css = None;
        self.canvases.clear();
        self.box_scroll.clear();
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
            && delta.abs() < self.viewport_height as f32
            && self.cached.as_ref().is_some_and(|c| !c.layout.has_fixed);
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
        let offsets = &self.box_scroll;
        self.cached.as_ref()?.layout.fragments.hit_test_scrolled(x, y, self.scroll, &|n| offsets.get(&n).copied().unwrap_or((0.0, 0.0)))
    }

    /// The `href` of the link under document point (x, y), seeing box
    /// scrolling and fixed boxes
    pub fn link_at(&mut self, document: &Document, x: f32, y: f32) -> Option<String> {
        let tree = document.tree();
        let mut node = self.node_at(x, y)?;
        while node.is_valid() {
            let n = tree.get(node)?;
            if n.as_element().is_some_and(|e| tree.resolve(e.name.local) == "a") {
                if let Some(href) = tree.get_attribute(node, "href") {
                    return Some(href.to_string());
                }
            }
            node = n.parent;
        }
        None
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
        let started = std::time::Instant::now();
        let stylesheet = self.compiled_stylesheet(document);
        let parsed = started.elapsed();
        let layout = build_layout(document, stylesheet, &self.images, &mut self.svgs, &mut self.fonts, (width as f32, self.viewport_height as f32));
        log::debug!("layout: css {:?}, styles + layout {:?}", parsed, started.elapsed() - parsed);
        self.cached = Some(CachedLayout { source, width, layout: Arc::new(layout) });
        self.layout_generation += 1;
    }

    /// Paint the visible region of the cached layout
    fn paint_cached(&mut self, scroll_offset: f32) -> Option<RenderedPage> {
        let cached = self.cached.take()?;
        self.scroll = scroll_offset;
        let painted = self.paint(&cached.layout, scroll_offset, scroll_offset, self.viewport_height);
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
        self.scroll = scroll_offset;
        let band = self.paint(&cached.layout, band_origin, scroll_offset, shift as u32);
        page.anchors = anchors_from(&cached.layout, scroll_offset);
        page.links = links_in(&cached.layout, scroll_offset, page.height as f32);
        self.cached = Some(cached);
        page.pixels[band_top * width..(band_top + shift) * width].copy_from_slice(&band?);
        page.origin = scroll_offset;
        Some(page)
    }

    /// Take new canvas bitmaps (`None`: cleared); true if any changed. A
    /// repaint shows them (the layout does not change).
    pub fn update_canvases(&mut self, updates: Vec<(NodeId, (u32, u32), Option<fos_canvas::tiny_skia::Pixmap>)>) -> bool {
        let changed = !updates.is_empty();
        for (node, (w, h), pixmap) in updates {
            match pixmap {
                Some(pixmap) => {
                    self.canvases.insert(node, Arc::new(crate::image_loader::LoadedImage { natural: (w as f32, h as f32), pixmap }));
                }
                None => {
                    self.canvases.remove(&node);
                }
            }
        }
        if changed {
            // Painted buffers are stale
            self.layout_generation += 1;
        }
        changed
    }

    /// Forget the previous page's scroll positions and canvases
    pub fn new_page(&mut self) {
        self.box_scroll.clear();
        self.canvases.clear();
        self.cached = None;
    }

    /// Scroll the innermost scroll container at document point (x, y) that
    /// can move by (dx, dy); false if none can (the page should scroll)
    pub fn scroll_box_at(&mut self, x: f32, y: f32, dx: f32, dy: f32) -> bool {
        let Some(cached) = self.cached.as_ref() else { return false };
        let offsets = &self.box_scroll;
        let chain = cached.layout.fragments.scrollers_at(x, y, self.scroll, &|n| offsets.get(&n).copied().unwrap_or((0.0, 0.0)));
        for (node, visible, extent) in chain {
            let (ox, oy) = self.box_scroll.get(&node).copied().unwrap_or((0.0, 0.0));
            let nx = (ox + dx).clamp(0.0, (extent.0 - visible.0).max(0.0)).round();
            let ny = (oy + dy).clamp(0.0, (extent.1 - visible.1).max(0.0)).round();
            if (nx, ny) != (ox, oy) {
                self.box_scroll.insert(node, (nx, ny));
                // Painted buffers are stale
                self.layout_generation += 1;
                return true;
            }
        }
        false
    }

    /// Scroll a box to (x, y) (clamped to its content); false if it
    /// does not scroll
    pub fn set_box_scroll(&mut self, node: NodeId, x: f32, y: f32) -> bool {
        let Some(m) = self.cached.as_ref().and_then(|c| c.layout.scroll_metrics().get(&node.0).copied()) else { return false };
        let to = (x.clamp(0.0, (m[2] - m[0]).max(0.0)).round(), y.clamp(0.0, (m[3] - m[1]).max(0.0)).round());
        if self.box_scroll(node) == to {
            return false;
        }
        self.box_scroll.insert(node, to);
        self.layout_generation += 1;
        true
    }

    /// Every scrolled box's position (for scripts)
    pub fn box_scrolls(&self) -> std::collections::HashMap<u32, (f32, f32)> {
        self.box_scroll.iter().map(|(n, v)| (n.0, *v)).collect()
    }

    /// An element's scroll position (scrollLeft, scrollTop)
    pub fn box_scroll(&self, node: NodeId) -> (f32, f32) {
        self.box_scroll.get(&node).copied().unwrap_or((0.0, 0.0))
    }

    /// Images the current layout's CSS uses (backgrounds, masks)
    /// The web fonts `document`'s CSS declares and uses
    pub fn web_font_requests(&self, document: &Document) -> Vec<crate::font_loader::FontRequest> {
        let css = self.extract_css_from_document(document);
        let media = fos_css::MediaContext { width: self.viewport_width as f32, height: self.viewport_height as f32 };
        crate::font_loader::requests(&css, &media, &crate::css_loader::base_url(document))
    }

    pub fn web_fonts(&self) -> &crate::font_loader::WebFonts {
        &self.web_fonts
    }

    /// Use `fonts` as the page's web fonts: they join a font database
    /// over the system's, and the layout is redone
    pub fn set_web_fonts(&mut self, fonts: crate::font_loader::WebFonts) {
        if Arc::ptr_eq(&self.web_fonts, &fonts) {
            return;
        }
        let mut db = fos_text::FontDatabase::overlay(fos_text::FontDatabase::shared());
        for f in fonts.iter() {
            let r = &f.request;
            let style = if r.italic { fos_text::FontStyle::Italic } else { fos_text::FontStyle::Normal };
            if let Err(e) = db.add_web_font(&r.family, fos_text::FontWeight(r.weight), style, f.data.clone()) {
                log::debug!("Web font {} unusable: {e}", r.url);
            }
        }
        let db = Arc::new(db);
        self.web_fonts = fonts;
        self.text_renderer.fonts = db.clone();
        self.text_renderer.clear_glyph_cache();
        self.fonts = FontContext::new(db);
        self.cached = None;
    }

    pub fn css_image_urls(&self) -> Vec<String> {
        self.cached.as_ref().map(|c| c.layout.css_images.clone()).unwrap_or_default()
    }

    /// Use `images` for the page's images (fetched by the browser); the
    /// layout is redone if they changed
    pub fn set_images(&mut self, images: crate::image_loader::Images) {
        if !Arc::ptr_eq(&self.images, &images) {
            self.images = images;
            self.cached = None;
        }
    }

    /// The images in use (to keep what is still needed when reloading)
    pub fn images(&self) -> &crate::image_loader::Images {
        &self.images
    }

    /// The page's CSS compiled for matching, reused while its text and
    /// the viewport stay the same
    fn compiled_stylesheet(&mut self, document: &Document) -> Option<Arc<PageStyles>> {
        let started = std::time::Instant::now();
        let css_text = self.extract_css_from_document(document);
        log::debug!("css: extracted {} bytes in {:?}", css_text.len(), started.elapsed());
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        css_text.hash(&mut hasher);
        (self.viewport_width, self.viewport_height).hash(&mut hasher);
        let key = hasher.finish();
        if let Some((k, sheet)) = &self.compiled_css {
            if *k == key {
                return sheet.clone();
            }
        }
        let sheet = self.compile(&css_text).map(Arc::new);
        self.compiled_css = Some((key, sheet.clone()));
        sheet
    }

    /// Parse the page's own CSS (`<style>` elements) and compile it for
    /// matching
    #[cfg(test)]
    fn page_stylesheet(&self, document: &Document) -> Option<Arc<PageStyles>> {
        self.compile(&self.extract_css_from_document(document)).map(Arc::new)
    }

    fn compile(&self, css_text: &str) -> Option<PageStyles> {
        if css_text.is_empty() {
            return None;
        }
        let media = fos_css::MediaContext { width: self.viewport_width as f32, height: self.viewport_height as f32 };
        let styles = PageStyles::new(fos_css::parse_stylesheet_for(css_text, media));
        log::debug!("Parsed {} CSS rules from page, buckets (id, class, tag+attr, universal) {:?}", styles.rule_count(), styles.bucket_sizes());
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
    fn paint(&mut self, layout: &PageLayout, origin: f32, scroll: f32, height: u32) -> Option<Vec<u32>> {
        let bg = layout.background;
        let mut canvas = Canvas::filled(self.viewport_width, height, Color::rgba(bg.r, bg.g, bg.b, 255))?;
        let mut painter = crate::paint::Painter::new(&mut canvas, &mut self.text_renderer, origin, scroll, layout.background_box).with_fixed(layout.has_fixed).with_images(&self.images, &layout.base).with_canvases(&self.canvases).with_box_scroll(&self.box_scroll);
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
    /// The document is in quirks mode
    quirks: bool,
    /// Loaded images, and the URL their sources resolve against
    images: &'a crate::image_loader::Images,
    base: String,
    /// Rasterized inline SVGs
    svgs: &'a mut crate::image_loader::SvgCache,
}

impl layout_engine::Styler for BrowserStyler<'_> {
    fn style(&mut self, tree: &DomTree, node: NodeId, parent: &Style) -> Style {
        let Some(element) = tree.get(node).and_then(|n| n.as_element()) else { return Style::inherit_from(parent) };
        // Quirks mode: tables do not inherit fonts, white-space and
        // text-align (the HTML standard's rendering quirk, as UA rules that
        // author rules still override)
        let quirk_parent;
        let parent = if self.quirks && tree.resolve(element.name.local) == "table" {
            let mut p = parent.clone();
            let i = Arc::make_mut(&mut p.inherited);
            let initial = &Style::initial_ref().inherited;
            i.font_weight = initial.font_weight;
            i.font_style = initial.font_style;
            i.font_size = initial.font_size;
            i.line_height = initial.line_height;
            i.white_space = initial.white_space;
            i.text_align = initial.text_align;
            quirk_parent = p;
            &quirk_parent
        } else {
            parent
        };
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

    fn image(&mut self, tree: &DomTree, node: NodeId) -> Option<((f32, f32), layout_engine::ImageHandle)> {
        if self.images.is_empty() {
            return None;
        }
        let tag = tree.get(node).and_then(|n| n.as_element()).map(|e| tree.resolve(e.name.local))?;
        let src = if tag == "img" { crate::image_loader::image_source(tree, node, self.ctx.viewport.0)? } else { tree.get_attribute(node, "src")?.to_string() };
        let img = self.images.get(&fos_net::url_util::resolve(&self.base, &src))?;
        Some((img.natural, layout_engine::ImageHandle(img.clone())))
    }

    fn content_image(&mut self, url: &str) -> Option<((f32, f32), layout_engine::ImageHandle)> {
        let img = self.images.get(url).or_else(|| self.images.get(&fos_net::url_util::resolve(&self.base, url)))?;
        Some((img.natural, layout_engine::ImageHandle(img.clone())))
    }

    fn inline_svg(&mut self, tree: &DomTree, node: NodeId, style: &Style) -> Option<((f32, f32), layout_engine::ImageHandle)> {
        let c = style.color();
        let img = crate::image_loader::inline_svg(tree, node, [c.r, c.g, c.b, c.a], self.svgs)?;
        Some((img.natural, layout_engine::ImageHandle(img)))
    }

    fn pseudo(&mut self, tree: &DomTree, node: NodeId, pe: fos_dom::PseudoElement, style: &Style) -> Option<Style> {
        let element = tree.get(node)?.as_element()?;
        let filter = self.stylesheet.is_some().then_some(&self.ancestors);
        crate::page_styles::pseudo_style(self.stylesheet, tree, node, element, pe, filter, style, &self.ctx, &mut self.resolved)
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
fn build_layout(document: &Document, stylesheet: Option<Arc<PageStyles>>, images: &crate::image_loader::Images, svgs: &mut crate::image_loader::SvgCache, fonts: &mut FontContext, viewport: (f32, f32)) -> PageLayout {
    let tree = document.tree();
    let root = document.document_element();
    let mut styler = BrowserStyler {
        stylesheet: stylesheet.as_deref(),
        ancestors: AncestorFilter::default(),
        resolved: Default::default(),
        ctx: StyleContext { root_font_size: 16.0, viewport },
        root_styled: false,
        quirks: document.is_quirks(),
        images,
        base: if images.is_empty() { String::new() } else { crate::css_loader::base_url(document) },
        svgs,
    };
    let fragments = if root.is_valid() {
        layout_engine::layout_document(tree, root, &mut styler, fonts, viewport)
    } else {
        FragmentTree { root: None, document_height: viewport.1, document_width: viewport.0, viewport_height: viewport.1 }
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
    let mut has_fixed = false;
    let base = crate::css_loader::base_url(document);
    let mut css_images: Vec<String> = Vec::new();
    fragments.for_each(|f| {
        if let Fragment::Box(b) = f {
            // Sticky boxes move with scrolling too: no scroll blitting
            has_fixed |= matches!(b.style.box_.position, fos_css::style::Position::Fixed | fos_css::style::Position::Sticky);
            let bg = &b.style.background;
            // Background and mask images, and generated content's
            let content = b.style.box_.content.iter().flat_map(|c| c.iter()).filter_map(|i| if let fos_css::style::ContentItem::Image(u) = i { Some(u) } else { None });
            let urls = bg.images.iter().filter_map(|i| if let fos_css::style::Image::Url(u) = i { Some(u) } else { None }).chain(bg.mask.iter()).chain(content);
            for u in urls {
                let abs = fos_net::url_util::resolve(&base, u);
                if !css_images.contains(&abs) && css_images.len() < 400 {
                    css_images.push(abs);
                }
            }
        }
    });
    PageLayout { fragments, links, anchors, background, background_box, has_fixed, css_images, base }
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
        build_layout(document, sheet, &Default::default(), &mut Default::default(), &mut FontContext::default(), (width, 240.0))
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
    fn test_rounded_and_shaped_clips() {
        let svg = "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='40' height='40'><rect width='40' height='40' fill='red'/></svg>";
        let html = format!(
            r#"<html><body style="margin: 0"><div style="display: flex">
            <img src="{svg}" style="width: 40px; height: 40px; border-radius: 50%">
            <div style="width: 40px; height: 40px; border-radius: 50%; overflow: hidden"><div style="height: 40px; background: #f00"></div></div>
            <div style="width: 40px; height: 40px; background: #f00; clip-path: circle(50%)"></div>
            <div style="width: 40px; height: 40px; background: #f00; clip-path: polygon(50% 0, 100% 100%, 0 100%)"></div>
            </div></body></html>"#
        );
        let document = fos_html::parse_with_url(&html, "https://example.com/");
        let mut renderer = PageRenderer::new(200, 50);
        let img = crate::image_loader::decode_data_url(&svg["data:".len()..]).expect("svg decodes");
        renderer.set_images(Arc::new([(svg.to_string(), Arc::new(img))].into_iter().collect()));
        let page = renderer.render_document(&document, 0.0).unwrap();
        let red = |x: usize, y: usize| page.pixels[y * 200 + x] == 0xffff0000;
        for i in 0..4 {
            let x0 = i * 40;
            // Filled in the middle, cut at the top corners
            assert!(red(x0 + 20, 30), "shape {i} center");
            assert!(!red(x0 + 2, 2) && !red(x0 + 37, 2), "shape {i} corners");
        }
    }

    #[test]
    fn test_background_clip() {
        let html = r#"<html><body style="margin: 0">
            <div style="font: bold 40px sans-serif; line-height: 40px; height: 40px; background: #f00; background-clip: text; color: transparent">MMMM</div>
            <div style="width: 20px; height: 20px; border: 5px solid transparent; background: #00f; background-clip: padding-box"></div>
            </body></html>"#;
        let document = fos_html::parse_with_url(html, "https://example.com/");
        let mut renderer = PageRenderer::new(200, 80);
        let page = renderer.render_document(&document, 0.0).unwrap();
        let at = |x: usize, y: usize| page.pixels[y * 200 + x];
        // The background shows through the glyphs only
        let red = (0..40).flat_map(|y| (0..120).map(move |x| (x, y))).filter(|&(x, y)| at(x, y) == 0xffff0000).count();
        assert!(red > 100, "glyphs filled: {red}");
        assert_ne!(at(195, 20), 0xffff0000, "no background beside the text");
        // padding-box: none under the (transparent) border
        assert_ne!(at(2, 42), 0xff0000ff);
        assert_eq!(at(15, 55), 0xff0000ff);
    }

    #[test]
    fn test_object_position() {
        // 10×20: red on top, blue below; shown in 10×10 boxes with cover
        let svg = "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='10' height='20'><rect width='10' height='10' fill='red'/><rect y='10' width='10' height='10' fill='blue'/></svg>";
        let html = format!(
            r#"<html><body style="margin: 0"><img src="{svg}" style="display: block; width: 10px; height: 10px; object-fit: cover; object-position: 100% 0"><img src="{svg}" style="display: block; width: 10px; height: 10px; object-fit: cover; object-position: left bottom"></body></html>"#
        );
        let document = fos_html::parse_with_url(&html, "https://example.com/");
        let mut renderer = PageRenderer::new(50, 30);
        let img = crate::image_loader::decode_data_url(&svg["data:".len()..]).expect("svg decodes");
        renderer.set_images(Arc::new([(svg.to_string(), Arc::new(img))].into_iter().collect()));
        let page = renderer.render_document(&document, 0.0).unwrap();
        let at = |x: usize, y: usize| page.pixels[y * 50 + x];
        // The top of the image in the first, the bottom in the second
        assert_eq!(at(5, 2), 0xffff0000);
        assert_eq!(at(5, 8), 0xffff0000);
        assert_eq!(at(5, 12), 0xff0000ff);
        assert_eq!(at(5, 18), 0xff0000ff);
    }

    #[test]
    fn test_generated_content_images() {
        let svg = "data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10' fill='red'/></svg>";
        let html = format!(r#"<html><head><style>body, p {{ margin: 0 }} .i::before {{ content: url("{svg}") " " }}</style></head><body><p class=i>x</p></body></html>"#);
        let document = fos_html::parse_with_url(&html, "https://example.com/");
        let mut renderer = PageRenderer::new(200, 50);
        let first = renderer.render_document(&document, 0.0).unwrap();
        // Nothing drawn (and no room taken) before the image loads; its URL
        // is among those to fetch
        assert_ne!(first.pixels[5 * 200 + 5], 0xffff0000);
        let urls = renderer.css_image_urls();
        assert_eq!(urls, vec![svg.to_string()]);
        let img = crate::image_loader::decode_data_url(&svg["data:".len()..]).expect("svg decodes");
        renderer.set_images(Arc::new([(svg.to_string(), Arc::new(img))].into_iter().collect()));
        let page = renderer.render_document(&document, 0.0).unwrap();
        // Red somewhere in the image's column (it sits on the baseline)
        let reddish = |p: u32| (p >> 16) & 0xff > 0xe0 && (p >> 8) & 0xff < 0x60;
        assert!((0..20).any(|y| reddish(page.pixels[y * 200 + 5])));
    }

    #[test]
    fn test_clip_and_clip_path_inset_hide_boxes() {
        let html = r#"<html><body style="margin: 0">
            <div style="position: absolute; top: 0; left: 0; width: 40px; height: 40px; background: #f00; clip: rect(0 0 0 0)"></div>
            <div style="position: absolute; top: 0; left: 50px; width: 40px; height: 40px; background: #f00; clip: rect(0, 20px, 40px, auto)"></div>
            <div style="position: absolute; top: 50px; left: 0; width: 40px; height: 40px; background: #f00; clip-path: inset(50%)"></div>
            <div style="position: absolute; top: 50px; left: 50px; width: 40px; height: 40px; background: #f00; clip-path: inset(0 0 0 20px round 4px)"></div>
            </body></html>"#;
        let document = fos_html::parse_with_url(html, "https://example.com/");
        let mut renderer = PageRenderer::new(200, 100);
        let page = renderer.render_document(&document, 0.0).unwrap();
        let red = |x: usize, y: usize| page.pixels[y * 200 + x] == 0xffff0000;
        // rect(0 0 0 0) and inset(50%) hide everything
        assert!(!red(20, 20) && !red(20, 70));
        // rect(0, 20px, 40px, auto): the left half shows
        assert!(red(55, 20) && !red(80, 20));
        // inset(0 0 0 20px): the right half shows
        assert!(!red(55, 70) && red(80, 70));
    }

    #[test]
    fn test_fixed_boxes_stay_in_the_viewport() {
        let mut html = String::from("<html><body><div style='position: fixed; top: 0; left: 0; width: 50px; height: 20px; background: #f00'></div>");
        for i in 0..100 {
            html.push_str(&format!("<p>line {i}</p>"));
        }
        let document = fos_html::parse_with_url(&html, "https://example.com/");
        let mut renderer = PageRenderer::new(200, 100);
        let top = renderer.render_document(&document, 0.0).unwrap();
        let scrolled = renderer.render_document_scrolled(&document, 40.0, top).unwrap();
        // Red (0xffff0000) at the viewport's top-left at both positions
        assert_eq!(scrolled.pixels[5 * 200 + 5], 0xffff0000);
        let far = renderer.render_document(&document, 1000.0).unwrap();
        assert_eq!(far.pixels[5 * 200 + 5], 0xffff0000);
        assert_ne!(far.pixels[50 * 200 + 5], 0xffff0000);
        let fixed = renderer.node_at(10.0, 1010.0).unwrap();
        assert_eq!(document.tree().get(fixed).and_then(|n| n.as_element()).map(|e| document.tree().resolve(e.name.local).to_string()).as_deref(), Some("div"));
    }

    #[test]
    fn test_sticky_boxes_stick_within_their_parent() {
        let html = "<html><body style='margin:0'><div style='height: 1000px'>\
            <div id=h style='position: sticky; top: 0; height: 20px; background: #f00'></div>\
            <div style='height: 500px'></div></div><div style='height: 2000px'></div></body></html>";
        let document = fos_html::parse_with_url(html, "https://example.com/");
        let h = document.get_element_by_id("h").unwrap();
        let mut renderer = PageRenderer::new(200, 100);
        let top = renderer.render_document(&document, 0.0).unwrap();
        assert_eq!(top.pixels[5 * 200 + 5], 0xffff0000);
        // Scrolled: pinned to the viewport's top, and hit there
        let mid = renderer.render_document(&document, 300.0).unwrap();
        assert_eq!(mid.pixels[5 * 200 + 5], 0xffff0000);
        assert_ne!(mid.pixels[50 * 200 + 5], 0xffff0000);
        assert_eq!(renderer.node_at(10.0, 305.0), Some(h));
        // Past its parent's end it scrolls away with it
        let past = renderer.render_document(&document, 1100.0).unwrap();
        assert!(past.pixels.iter().all(|&p| p != 0xffff0000));
        assert_ne!(renderer.node_at(10.0, 1105.0), Some(h));
        // At the parent's end it sits on the parent's bottom edge
        let end = renderer.render_document(&document, 990.0).unwrap();
        assert_eq!(end.pixels[5 * 200 + 5], 0xffff0000);
        assert_ne!(end.pixels[12 * 200 + 5], 0xffff0000);
    }

    #[test]
    fn test_links_in_scrolled_boxes_are_hit_where_shown() {
        let mut html = String::from("<html><body style='margin:0'><div style='height: 40px; width: 100px; overflow: auto'>");
        for i in 0..20 {
            html.push_str(&format!("<div style='height: 20px'><a href='/l{i}' style='display:block'>{i}</a></div>"));
        }
        html.push_str("</div></body></html>");
        let document = fos_html::parse_with_url(&html, "https://example.com/");
        let mut renderer = PageRenderer::new(200, 100);
        renderer.render_document(&document, 0.0).unwrap();
        assert_eq!(renderer.link_at(&document, 10.0, 25.0).as_deref(), Some("/l1"));
        assert!(renderer.scroll_box_at(10.0, 10.0, 0.0, 200.0));
        assert_eq!(renderer.link_at(&document, 10.0, 25.0).as_deref(), Some("/l11"));
        // Outside the box there is no link
        assert_eq!(renderer.link_at(&document, 150.0, 25.0), None);
    }

    #[test]
    fn test_scroll_containers_scroll_their_content() {
        let mut html = String::from("<html><body style='margin:0'><div id=s style='height: 50px; width: 100px; overflow: auto'>");
        for i in 0..20 {
            html.push_str(&format!("<div style='height: 20px; background: {}'>{i}</div>", if i == 0 { "#f00" } else { "#00f" }));
        }
        html.push_str("</div><p>after</p></body></html>");
        let document = fos_html::parse_with_url(&html, "https://example.com/");
        let mut renderer = PageRenderer::new(200, 100);
        let before = renderer.render_document(&document, 0.0).unwrap();
        assert_eq!(before.pixels[5 * 200 + 5], 0xffff0000);
        // Content below the box is clipped: the paragraph follows at 50px
        let s = document.get_element_by_id("s").unwrap();
        assert!(renderer.scroll_box_at(10.0, 10.0, 0.0, 30.0));
        assert_eq!(renderer.box_scroll(s), (0.0, 30.0));
        let after = renderer.render_document(&document, 0.0).unwrap();
        // Row 1 (blue) is now at the top
        assert_eq!(after.pixels[5 * 200 + 5], 0xff0000ff);
        // Clamped at the end (20 rows of 20px in a 50px box)
        assert!(renderer.scroll_box_at(10.0, 10.0, 0.0, 10_000.0));
        assert_eq!(renderer.box_scroll(s), (0.0, 350.0));
        assert!(!renderer.scroll_box_at(10.0, 10.0, 0.0, 10.0));
        // Outside the box nothing scrolls
        assert!(!renderer.scroll_box_at(150.0, 10.0, 0.0, 10.0));
        // Hit testing sees the scrolled content: the last row
        let last = renderer.node_at(10.0, 45.0).unwrap();
        let text: String = document.tree().children(last).filter_map(|(_, n)| n.as_text()).collect();
        assert_eq!(text, "19");
    }

    #[test]
    fn test_empty_page_does_not_panic() {
        let mut renderer = PageRenderer::new(320, 240);
        let page = renderer.render_html("<html><body></body></html>", "about:blank", 0.0).unwrap();
        assert_eq!(page.pixels.len(), 320 * 240);
    }

    #[test]
    fn test_web_fonts_are_used_by_family_name() {
        let Ok(data) = std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf") else { return };
        let html = r#"<html><head><style>
            @font-face { font-family: Brand; src: url(brand.ttf) }
            p { font-family: Brand, sans-serif }</style></head><body><p>web font</p></body></html>"#;
        let document = fos_html::parse_with_url(html, "https://example.com/");
        let mut renderer = PageRenderer::new(400, 200);
        let reqs = renderer.web_font_requests(&document);
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].url, "https://example.com/brand.ttf");
        let font = |r: &mut PageRenderer| {
            r.render_document(&document, 0.0).unwrap();
            let layout = r.layout_snapshot().unwrap();
            let mut family = String::new();
            layout.fragments.for_each(|f| {
                if let layout_engine::Fragment::Text(t) = f {
                    family = t.font.id.and_then(|id| r.fonts.database().font(id)).map(|e| e.family.clone()).unwrap_or_default();
                }
            });
            family
        };
        assert_ne!(font(&mut renderer), "Brand");
        let loaded = crate::font_loader::LoadedFont { request: reqs[0].clone(), data };
        renderer.set_web_fonts(Arc::new(vec![Arc::new(loaded)]));
        assert_eq!(font(&mut renderer), "Brand");
    }

    #[test]
    fn test_box_shadows() {
        let html = r#"<html><body style="margin:0; background: white">
            <div style="margin: 20px; width: 40px; height: 20px; background: white; box-shadow: 10px 10px 0 0 rgb(255, 0, 0)"></div>
            <div style="margin: 20px; width: 40px; height: 40px; box-shadow: inset 0 0 0 5px rgb(0, 0, 255)"></div>
            <div style="margin: 20px; width: 40px; height: 20px; box-shadow: 0 0 8px black"></div>
            </body></html>"#;
        let document = fos_html::parse_with_url(html, "https://example.com/");
        let mut renderer = PageRenderer::new(200, 200);
        let page = renderer.render_document(&document, 0.0).unwrap();
        let px = |x: usize, y: usize| page.pixels[y * 200 + x];
        // A hard shadow offset down-right, not under the box
        assert_eq!(px(65, 45), 0xffff0000);
        assert_eq!(px(55, 35), 0xffffffff);
        assert_ne!(px(25, 45), 0xffff0000);
        // An inset ring 5px wide
        let top = 60;
        assert_eq!(px(22, top + 2), 0xff0000ff);
        assert_ne!(px(40, top + 20), 0xff0000ff);
        // A blurred shadow fades out around the box (third box: 20..60 x 120..140)
        let gray = |p: u32| p & 0xff;
        assert!(gray(px(40, 116)) < 0xff && gray(px(40, 116)) > 0x80, "{:x}", px(40, 116));
        assert!(gray(px(40, 105)) == 0xff);
    }

    #[test]
    fn test_transforms_paint_and_hit() {
        let html = r#"<html><body style="margin:0">
            <div style="position: relative; height: 100px">
              <div id="c" style="position: absolute; left: 50%; top: 50%; width: 20px; height: 20px; transform: translate(-50%, -50%); background: red"></div>
              <div id="r" style="position: absolute; left: 20px; top: 20px; width: 20px; height: 20px; transform: rotate(45deg); background: blue"></div>
              <div id="s" style="margin-left: 150px; width: 10px; height: 10px; scale: 2; transform-origin: 0 0; background: lime"></div>
            </div></body></html>"#;
        let document = fos_html::parse_with_url(html, "https://example.com/");
        let id = |s: &str| document.get_element_by_id(s);
        let mut renderer = PageRenderer::new(200, 100);
        let page = renderer.render_document(&document, 0.0).unwrap();
        let px = |x: usize, y: usize| page.pixels[y * 200 + x];
        // Centered by translate(-50%, -50%): 90..110 x 40..60
        assert_eq!(px(100, 50), 0xffff0000);
        assert_eq!(px(91, 41), 0xffff0000);
        assert_ne!(px(112, 50), 0xffff0000);
        assert_eq!(renderer.node_at(92.0, 42.0), id("c"));
        // Rotated: a diamond around (30, 30) reaching y = 16
        assert_eq!(px(30, 30), 0xff0000ff);
        assert_eq!(px(30, 18), 0xff0000ff);
        assert_ne!(px(21, 21), 0xff0000ff);
        assert_eq!(renderer.node_at(30.0, 17.5), id("r"));
        assert_ne!(renderer.node_at(21.0, 21.0), id("r"));
        // Scaled from its top-left corner to 20px
        assert_eq!(px(168, 18), 0xff00ff00);
        assert_eq!(renderer.node_at(168.0, 18.0), id("s"));
    }

    #[test]
    fn test_inline_svg_is_drawn() {
        let html = r#"<html><body style="margin:0; color: #f00">
            <svg width="20" height="20" viewBox="0 0 10 10"><rect width="10" height="10" fill="currentColor"/></svg><br>
            <svg viewBox="0 0 10 10" style="width: 40px; height: 40px; display: block; color: #00f"><path d="M0 0h10v10H0z" fill="currentColor"/></svg>
            <a href="/x"><svg width="8" height="8"><circle cx="4" cy="4" r="4" fill="lime"/></svg></a>
            </body></html>"#;
        let document = fos_html::parse_with_url(html, "https://example.com/");
        let mut renderer = PageRenderer::new(200, 200);
        let page = renderer.render_document(&document, 0.0).unwrap();
        let px = |x: usize, y: usize| page.pixels[y * 200 + x];
        // currentColor is the element's color; sizes come from attributes,
        // or from CSS with the viewBox's aspect
        assert_eq!(px(10, 10), 0xffff0000);
        assert_ne!(px(25, 10), 0xffff0000);
        let blue = (0..200).find(|&y| px(20, y) == 0xff0000ff).expect("blue svg");
        assert!((0..40).all(|d| px(5, blue + d) == 0xff0000ff || d > 37));
        // The same markup rasterizes once
        let before = renderer.svgs.map.len();
        renderer.render_document(&document, 0.0).unwrap();
        assert_eq!(renderer.svgs.map.len(), before);
    }

    #[test]
    fn test_marker_pseudo_element() {
        let html = r#"<html><head><style>
            ol li::marker { color: #f00 }
            ul li::marker { content: "-> "; color: #00f }
            .none::marker { content: none }
            </style></head><body><ol><li>one</li></ol><ul><li>two</li><li class="none">hidden</li></ul><ol><li id="plain">x</li></ol></body></html>"#;
        let document = fos_html::parse_with_url(html, "https://example.com/");
        let layout = layout_of(&document, 640.0);
        let mut markers: Vec<(Color, f32)> = Vec::new();
        layout.fragments.for_each(|f| {
            if let layout_engine::Fragment::Box(b) = f {
                if let Some(m) = &b.marker {
                    markers.push((Color::rgba(m.color.r, m.color.g, m.color.b, m.color.a), m.rect.w));
                }
            }
        });
        assert_eq!(markers.len(), 3, "{markers:?}");
        assert_eq!(markers[0].0, Color::rgb(255, 0, 0));
        // content replaces the marker text: "-> " is wider than "1. "
        assert_eq!(markers[1].0, Color::rgb(0, 0, 255));
        assert!(markers[1].1 > markers[0].1, "{markers:?}");
    }

    #[test]
    fn test_before_and_after_boxes() {
        let html = r#"<html><head><style>
            body { margin: 0 }
            .x::before { content: "\201C AA"; color: #f00 }
            .x::after { content: attr(data-n); color: #00f }
            .cf::after { content: ""; display: block; clear: both }
            .f { float: left; width: 20px; height: 50px }
            .a { position: relative; height: 30px }
            .a::before { content: ""; position: absolute; left: 0; top: 0; width: 10px; height: 10px; background: #0f0 }
            .fl { display: flex }
            .fl::before { content: "x"; width: 40px; color: #f0f }
            </style></head><body>
            <div class="a" id="a"></div>
            <p class="x" data-n="ZZ">mid</p>
            <div class="cf" id="cf"><div class="f"></div></div>
            <div class="fl"><span>item</span></div>
            <p><q>quoted</q></p>
            </body></html>"#;
        let document = fos_html::parse_with_url(html, "https://example.com/");
        let layout = layout_of(&document, 640.0);
        let mut frags: Vec<(Color, f32, f32)> = Vec::new();
        layout.fragments.for_each(|f| {
            if let layout_engine::Fragment::Text(t) = f {
                frags.push((Color::rgba(t.color.r, t.color.g, t.color.b, t.color.a), t.rect.x, t.rect.w));
            }
        });
        let of = |c: Color| frags.iter().find(|f| f.0 == c).copied();
        let (red, blue) = (of(Color::rgb(255, 0, 0)).expect("::before text"), of(Color::rgb(0, 0, 255)).expect("::after text"));
        // Generated text belongs to its element: "mid" sits between
        assert!(blue.1 > red.1 + red.2 + 10.0, "{red:?} {blue:?}");
        // Flex containers get generated items, as wide as they say
        let magenta = of(Color::rgb(255, 0, 255)).expect("flex ::before");
        let item = texts(&document, &layout, "item")[0].rect;
        assert!(item.x >= magenta.1 + 40.0 - 0.5, "{magenta:?} {item:?}");
        // The clearfix contains its float
        let cf = document.get_element_by_id("cf").unwrap();
        let cf_rect = layout.fragments.element_rects().into_iter().find(|(n, _)| *n == cf).unwrap().1;
        assert!(cf_rect.h >= 50.0, "{cf_rect:?}");
        // Quotes around <q>
        let quoted = texts(&document, &layout, "quoted");
        assert!(!quoted.is_empty());

        // An absolutely positioned ::before paints at its element's corner,
        // and hits there are hits on the element
        let mut renderer = PageRenderer::new(200, 100);
        let page = renderer.render_document(&document, 0.0).unwrap();
        assert_eq!(page.pixels[5 * 200 + 5], 0xff00ff00);
        assert_eq!(renderer.node_at(5.0, 5.0), document.get_element_by_id("a"));
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
