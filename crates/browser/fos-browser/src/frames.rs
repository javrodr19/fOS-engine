//! Nested browsing contexts: the documents `<iframe>`s show
//!
//! Each rendered iframe of a page gets its own document, stylesheets,
//! images, fonts and (unless sandboxed without `allow-scripts`) script
//! runtime, rendered at the size the parent's layout gave the frame. Its
//! picture is painted into the iframe's box like a canvas's.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use fos_canvas::tiny_skia::Pixmap;
use fos_dom::{Document, DomTree, NodeId};

use crate::network::NetworkManager;
use crate::page::Page;
use crate::renderer::{PageLayout, PageRenderer};

/// Most frames a page shows (each is a document with its own runtime)
const MAX_FRAMES: usize = 16;
/// Largest frame rendered, per side
const MAX_SIDE: u32 = 4096;

/// A message one window posted to another (`postMessage`), its data
/// serialized
#[derive(Clone, Debug)]
pub struct Message {
    pub to: MessageTarget,
    pub data: String,
    /// Where it may go: `*`, or an origin (or URL) the receiver must have
    pub target_origin: String,
}

/// Where a message goes, from the sender's side
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageTarget {
    /// The window containing the sender's frame
    Parent,
    /// The frame of one of the sender's iframes
    Frame(NodeId),
}

/// The serialized origin of `url` (`null` for opaque ones: data:, about:)
pub fn origin_of(url: &str) -> String {
    match fos_net::url_util::origin(url) {
        Some((scheme, host, port)) if matches!(scheme.as_str(), "http" | "https") => {
            let default = if scheme == "https" { 443 } else { 80 };
            if port == default { format!("{scheme}://{host}") } else { format!("{scheme}://{host}:{port}") }
        }
        _ => "null".to_string(),
    }
}

/// Whether a message for `target_origin` may reach a window of `origin`
fn origin_allows(target_origin: &str, origin: &str) -> bool {
    target_origin == "*" || origin_of(target_origin) == origin
}

/// What a frame shows
#[derive(Clone, Debug, PartialEq)]
enum Source {
    Url(String),
    Srcdoc(String),
}

/// A document shown in an iframe
pub struct Frame {
    /// The iframe in the parent document
    pub element: NodeId,
    /// What the iframe's attributes asked for (a link followed in the
    /// frame shows another document, until they change)
    source: Source,
    parent_url: String,
    scripts: bool,
    pub page: Page,
    renderer: PageRenderer,
    size: (u32, u32),
    /// A new picture is owed to the parent
    dirty: bool,
}

/// A page's frames
#[derive(Default)]
pub struct Frames {
    frames: Vec<Frame>,
    /// The iframes whose pictures the parent was given
    shown: Vec<NodeId>,
    /// Iframes whose frames loaded since the parent heard (`load` events)
    loaded: Vec<NodeId>,
}

/// Fetches a frame's scripts through the browser's network stack
pub struct FrameScripts<'a> {
    pub network: &'a mut NetworkManager,
    pub page_url: &'a str,
}

impl crate::js_runtime::ScriptSource for FrameScripts<'_> {
    fn fetch(&mut self, url: &str) -> Option<String> {
        let r = self.network.fetch(url, Some(self.page_url)).ok()?;
        Some(crate::charset::decode_html(r.body, Some(&r.content_type)))
    }

    fn fetch_many(&mut self, urls: &[String]) -> Vec<Option<String>> {
        self.network
            .fetch_many(urls, Some(self.page_url))
            .into_iter()
            .map(|r| r.ok().map(|r| crate::charset::decode_html(r.body, Some(&r.content_type))))
            .collect()
    }
}

/// The iframes of `tree` (in its shadow trees too), in tree order
fn iframes(tree: &DomTree) -> Vec<NodeId> {
    let mut out = Vec::new();
    let mut roots = vec![tree.root()];
    roots.extend(tree.shadow_roots().filter(|&(host, _)| tree.is_connected(host)).map(|(_, root)| root));
    for root in roots {
        fos_dom::selector::walk_elements(tree, root, &mut |id| {
            if tree.get(id).and_then(|n| n.as_element()).is_some_and(|e| tree.resolve(e.name.local) == "iframe") {
                out.push(id);
            }
            true
        });
    }
    out
}

/// What iframe `node` should show (None: nothing, as `about:blank`)
fn source_of(tree: &DomTree, node: NodeId, base: &str) -> Option<Source> {
    if let Some(doc) = tree.get_attribute(node, "srcdoc") {
        return Some(Source::Srcdoc(doc.to_string()));
    }
    let src = tree.get_attribute(node, "src")?.trim();
    if src.is_empty() || src.eq_ignore_ascii_case("about:blank") {
        return None;
    }
    let url = fos_net::url_util::resolve(base, src);
    let scheme = url.split(':').next().unwrap_or("").to_ascii_lowercase();
    matches!(scheme.as_str(), "http" | "https" | "data").then_some(Source::Url(url))
}

/// Whether a frame's scripts may run (`sandbox` without `allow-scripts`
/// forbids them)
fn scripts_allowed(tree: &DomTree, node: NodeId) -> bool {
    match tree.get_attribute(node, "sandbox") {
        Some(tokens) => tokens.split_ascii_whitespace().any(|t| t.eq_ignore_ascii_case("allow-scripts")),
        None => true,
    }
}

impl Frame {
    fn load(network: &mut NetworkManager, parent_url: &str, element: NodeId, source: Source, scripts: bool, size: (u32, u32)) -> Option<Frame> {
        let mut page = match &source {
            // A srcdoc document's URLs resolve against its parent's
            Source::Srcdoc(html) => Page::from_html(parent_url, html.clone()),
            Source::Url(url) if url.len() > 5 && url[..5].eq_ignore_ascii_case("data:") => {
                let rest = &url[5..];
                let bytes = crate::image_loader::data_url_bytes(rest)?;
                let mime = rest.split([',', ';']).next().unwrap_or("").trim();
                let html = crate::charset::decode_html(bytes, Some(if mime.is_empty() { "text/plain" } else { mime }));
                let html = if mime.is_empty() || mime.contains("html") { html } else { format!("<pre>{}</pre>", html.replace('&', "&amp;").replace('<', "&lt;")) };
                Page::from_html(url, html)
            }
            Source::Url(url) => {
                let fetched = network.fetch_frame_document(url, parent_url).ok()?;
                Page::from_html(&fetched.url, fetched.html)
            }
        };
        page.stylesheets = crate::css_loader::load_for_page(network, &page);
        let mut renderer = PageRenderer::new(size.0, size.1);
        renderer.set_stylesheets(page.stylesheets.clone());
        let mut frame = Frame { element, source, parent_url: parent_url.to_string(), page, renderer, size, scripts, dirty: true };
        frame.render_document();
        if scripts {
            frame.page.set_cookie_jar(network.cookie_jar().clone());
            if frame.page.initialize_javascript().is_ok() {
                if let Some(rt) = frame.page.js_runtime.as_mut() {
                    rt.become_frame();
                }
                frame.sync_geometry();
                let url = frame.page.url.clone();
                if let Err(e) = frame.page.execute_scripts_with(&mut FrameScripts { network, page_url: &url }) {
                    log::warn!("Frame {url}: {e}");
                }
            }
        }
        frame.load_resources(network);
        Some(frame)
    }

    /// Fetch the images and web fonts the frame's document uses
    fn load_resources(&mut self, network: &mut NetworkManager) {
        let Some(doc) = self.page.document() else { return };
        let wanted = self.renderer.web_font_requests(&doc.lock().unwrap());
        if !wanted.is_empty() {
            let fonts = crate::font_loader::load(network, &self.page.url, &wanted, &self.renderer.web_fonts().clone());
            self.renderer.set_web_fonts(fonts);
        }
        self.render_document();
        let css = self.renderer.css_image_urls();
        let images = crate::image_loader::load_for_page(network, &self.page, self.size.0 as f32, &Default::default(), &css);
        if !images.is_empty() {
            self.renderer.set_images(images);
        }
        self.dirty = true;
    }

    fn render_document(&mut self) {
        let Some(doc) = self.page.document() else { return };
        self.renderer.render_document(&doc.lock().unwrap(), 0.0);
        self.sync_geometry();
    }

    /// Tell the frame's scripts its layout and viewport
    fn sync_geometry(&mut self) {
        let layout = self.renderer.layout_snapshot();
        let size = (self.size.0 as f32, self.size.1 as f32);
        if let Some(rt) = self.page.js_runtime.as_mut() {
            rt.set_layout(layout, size, (0.0, 0.0));
        }
    }

    /// The frame's picture: its viewport, rendered
    fn picture(&mut self) -> Option<Pixmap> {
        let doc = self.page.document()?;
        self.renderer.set_viewport(self.size.0, self.size.1);
        let shot = self.renderer.render_document(&doc.lock().unwrap(), 0.0)?;
        self.sync_geometry();
        let mut pixmap = Pixmap::new(shot.width, shot.height)?;
        for (out, p) in pixmap.data_mut().chunks_exact_mut(4).zip(&shot.pixels) {
            out.copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8, 255]);
        }
        Some(pixmap)
    }

    /// Whether its DOM changed since it was rendered
    fn changed(&self) -> bool {
        self.page.document().is_some_and(|doc| !self.renderer.is_layout_current(&doc.lock().unwrap()))
    }
}

impl Frames {
    /// No frames (a new page)
    pub fn clear(&mut self) {
        self.frames.clear();
        self.shown.clear();
        self.loaded.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// The frames, in the parent's tree order
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    pub fn frames_mut(&mut self) -> &mut [Frame] {
        &mut self.frames
    }

    /// Bring the frames in line with `document`'s rendered iframes, sized
    /// as `layout` laid them out: load new and changed ones, resize, drop
    /// removed ones. Whether a frame's picture is owed to the parent.
    pub fn sync(&mut self, network: &mut NetworkManager, document: &Arc<Mutex<Document>>, layout: &PageLayout) -> bool {
        let (wanted, parent_url) = {
            let doc = document.lock().unwrap();
            let tree = doc.tree();
            let base = crate::css_loader::base_url(&doc);
            let wanted: Vec<(NodeId, Option<Source>, bool, (u32, u32))> = iframes(tree)
                .into_iter()
                .filter_map(|node| {
                    let rect = layout.content_box(node)?;
                    let size = (rect.w.round() as u32, rect.h.round() as u32);
                    (size.0 > 0 && size.1 > 0).then(|| (node, source_of(tree, node, &base), scripts_allowed(tree, node), (size.0.min(MAX_SIDE), size.1.min(MAX_SIDE))))
                })
                .take(MAX_FRAMES)
                .collect();
            (wanted, base)
        };
        let before = self.frames.len();
        self.frames.retain(|f| wanted.iter().any(|(node, source, _, _)| *node == f.element && source.as_ref() == Some(&f.source)));
        let mut owed = self.frames.len() != before;
        for (node, source, scripts, size) in wanted {
            let Some(source) = source else { continue };
            match self.frames.iter_mut().find(|f| f.element == node) {
                Some(frame) => {
                    if frame.size != size {
                        frame.size = size;
                        frame.renderer.set_viewport(size.0, size.1);
                        frame.dirty = true;
                    }
                }
                None => {
                    let started = Instant::now();
                    if let Some(frame) = Frame::load(network, &parent_url, node, source, scripts, size) {
                        log::info!("Frame {} loaded in {:?}", frame.page.url, started.elapsed());
                        self.frames.push(frame);
                        self.loaded.push(node);
                    }
                }
            }
        }
        owed |= self.frames.iter().any(|f| f.dirty);
        owed
    }

    /// Run the frames' due timers and finished requests. Whether a frame's
    /// document changed.
    pub fn process_tasks(&mut self, network: &mut NetworkManager) -> bool {
        let mut changed = false;
        for frame in &mut self.frames {
            let url = frame.page.url.clone();
            if frame.page.next_timer_due().is_some_and(|due| due <= Instant::now()) {
                let _ = frame.page.process_timers_with(&mut FrameScripts { network, page_url: &url });
            }
            if frame.page.has_pending_network() {
                let _ = frame.page.process_network_with(&mut FrameScripts { network, page_url: &url });
            }
            if frame.changed() {
                frame.dirty = true;
                changed = true;
            }
        }
        changed
    }

    /// Fire the `load` events of iframes whose frames loaded, and deliver
    /// the messages the parent (`parent`, its page at `parent_url`) and the
    /// frames posted to each other. Whether the parent's scripts ran (its
    /// DOM may have changed).
    pub fn exchange(&mut self, parent: &mut crate::js_runtime::PageJsRuntime, parent_url: &str) -> bool {
        let mut parent_ran = false;
        for node in std::mem::take(&mut self.loaded) {
            parent.fire_event(node, "load");
            parent_ran = true;
        }
        let parent_origin = origin_of(parent_url);
        // Replies may follow replies; a few rounds settle them
        for _ in 0..8 {
            let mut moved = false;
            for m in parent.take_messages() {
                let MessageTarget::Frame(node) = m.to else { continue };
                let Some(frame) = self.frames.iter_mut().find(|f| f.element == node) else { continue };
                let origin = origin_of(&frame.page.url);
                if let (true, Some(rt)) = (origin_allows(&m.target_origin, &origin), frame.page.js_runtime.as_mut()) {
                    rt.deliver_message(m.data, parent_origin.clone(), None);
                    moved = true;
                }
            }
            for frame in &mut self.frames {
                let origin = origin_of(&frame.page.url);
                let Some(rt) = frame.page.js_runtime.as_mut() else { continue };
                for m in rt.take_messages() {
                    if m.to == MessageTarget::Parent && origin_allows(&m.target_origin, &parent_origin) {
                        parent.deliver_message(m.data, origin.clone(), Some(frame.element));
                        parent_ran = true;
                        moved = true;
                    }
                }
            }
            if !moved {
                break;
            }
        }
        for frame in &mut self.frames {
            if frame.changed() {
                frame.dirty = true;
            }
        }
        parent_ran
    }

    /// Whether iframe `iframe` shows a frame
    pub fn has_frame(&self, iframe: NodeId) -> bool {
        self.frames.iter().any(|f| f.element == iframe)
    }

    /// A click at (x, y) in the frame of `iframe` (frame coordinates): its
    /// document's handlers run, and a link followed loads in the frame, or
    /// is returned for the tab to follow (`target` `_top`, `_parent` or
    /// `_blank`)
    pub fn click(&mut self, network: &mut NetworkManager, iframe: NodeId, x: f32, y: f32) -> Option<String> {
        let index = self.frames.iter().position(|f| f.element == iframe)?;
        let frame = &mut self.frames[index];
        let node = frame.renderer.node_at(x, y)?;
        let href = match frame.page.js_runtime.is_some() {
            true => frame.page.dispatch_click(node),
            // Without scripts, links still work
            false => frame.page.document().and_then(|doc| {
                let doc = doc.lock().unwrap();
                let tree = doc.tree();
                let mut n = node;
                while n.is_valid() {
                    if tree.get(n).and_then(|e| e.as_element()).is_some_and(|e| tree.resolve(e.name.local) == "a") {
                        if let Some(h) = tree.get_attribute(n, "href") {
                            return Some(fos_net::url_util::resolve(&crate::css_loader::base_url(&doc), h));
                        }
                    }
                    n = tree.get(n).map_or(NodeId::NONE, |e| e.parent);
                }
                None
            }),
        };
        if frame.changed() {
            frame.dirty = true;
        }
        let href = href?;
        if href.trim_start().to_ascii_lowercase().starts_with("javascript:") {
            return None;
        }
        // The link's target: another browsing context, or this frame
        let target = frame.page.document().and_then(|doc| {
            let doc = doc.lock().unwrap();
            let tree = doc.tree();
            let mut n = node;
            while n.is_valid() {
                if let Some(t) = tree.get_attribute(n, "target").filter(|_| tree.get_attribute(n, "href").is_some()) {
                    return Some(t.to_ascii_lowercase());
                }
                n = tree.get(n).map_or(NodeId::NONE, |e| e.parent);
            }
            None
        });
        if matches!(target.as_deref(), Some("_top" | "_parent" | "_blank")) {
            return Some(href);
        }
        // Same-document fragments only scroll (not modeled in frames)
        if href.split('#').next() == frame.page.url.split('#').next() && href.contains('#') {
            return None;
        }
        let (element, source, parent_url, scripts, size) = (frame.element, frame.source.clone(), frame.parent_url.clone(), frame.scripts, frame.size);
        if let Some(mut next) = Frame::load(network, &parent_url, element, Source::Url(href), scripts, size) {
            next.source = source;
            self.frames[index] = next;
            self.loaded.push(element);
        }
        None
    }

    /// When a frame's next timer is due
    pub fn next_timer_due(&self) -> Option<Instant> {
        self.frames.iter().filter_map(|f| f.page.next_timer_due()).min()
    }

    /// The pictures of the frames that changed, by iframe element, and
    /// `None` for frames that went away (updates for the parent
    /// renderer's bitmaps)
    pub fn pictures(&mut self) -> Vec<(NodeId, (u32, u32), Option<Pixmap>)> {
        let frames = &self.frames;
        let mut out: Vec<(NodeId, (u32, u32), Option<Pixmap>)> =
            self.shown.iter().filter(|n| !frames.iter().any(|f| f.element == **n)).map(|&n| (n, (0, 0), None)).collect();
        for f in self.frames.iter_mut().filter(|f| f.dirty) {
            f.dirty = false;
            out.push((f.element, f.size, f.picture()));
        }
        self.shown = self.frames.iter().map(|f| f.element).collect();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_with(html: &str) -> (Arc<Mutex<Document>>, PageRenderer) {
        let doc = Arc::new(Mutex::new(fos_html::parse_with_url(html, "https://example.com/")));
        let mut renderer = PageRenderer::new(400, 300);
        renderer.render_document(&doc.lock().unwrap(), 0.0).unwrap();
        (doc, renderer)
    }

    #[test]
    fn frames_show_their_documents() {
        let (doc, mut renderer) = page_with(
            r#"<html><body style="margin: 0">
            <iframe id=a style="border: 0; display: block" width=200 height=100 srcdoc="<body style='margin: 0; background: rgb(0, 255, 0)'><p id=p>x</p><script>document.body.style.background = 'rgb(0, 0, 255)'</script>"></iframe>
            <iframe id=b sandbox style="border: 0; display: block" width=200 height=100 srcdoc="<body style='margin: 0; background: rgb(0, 255, 0)'><script>document.body.style.background = 'rgb(0, 0, 255)'</script>"></iframe>
            <iframe id=c style="display: none" srcdoc="<p>hidden"></iframe>
            <iframe id=d width=50 height=50></iframe>
            </body></html>"#,
        );
        let mut network = NetworkManager::new();
        let mut frames = Frames::default();
        let layout = renderer.layout_snapshot().unwrap();
        assert!(frames.sync(&mut network, &doc, &layout));
        // Unrendered and blank iframes get no frame
        assert_eq!(frames.frames().len(), 2);
        let pictures = frames.pictures();
        assert_eq!(pictures.len(), 2);
        assert!(renderer.update_canvases(pictures));
        let page = renderer.render_document(&doc.lock().unwrap(), 0.0).unwrap();
        let px = |x: usize, y: usize| page.pixels[y * 400 + x] & 0xffffff;
        // The script ran in the first frame; the sandboxed one has none
        assert_eq!(px(150, 50), 0x0000ff);
        assert_eq!(px(150, 150), 0x00ff00);
        // Nothing new until something changes
        assert!(!frames.sync(&mut network, &doc, &layout));
        assert!(frames.pictures().is_empty());

        // A removed iframe's picture goes
        {
            let mut d = doc.lock().unwrap();
            let a = d.get_element_by_id("a").unwrap();
            d.tree_mut().remove(a);
        }
        renderer.render_document(&doc.lock().unwrap(), 0.0).unwrap();
        let layout = renderer.layout_snapshot().unwrap();
        assert!(frames.sync(&mut network, &doc, &layout));
        let pictures = frames.pictures();
        assert!(pictures.iter().any(|(_, _, p)| p.is_none()));
        assert_eq!(frames.frames().len(), 1);
    }

    #[test]
    fn windows_exchange_messages() {
        let html = r#"<html><body>
            <iframe id=f width=100 height=50 srcdoc="<p id=p>waiting</p><script>
              addEventListener('message', (e) => { document.getElementById('p').textContent = e.data.reply + ' from ' + e.origin + ' ' + (e.source === parent); });
              parent.postMessage({ hello: 'frame', top: top === parent, self: parent !== window }, '*');
              parent.postMessage('lost', 'https://elsewhere.example');
            </script>"></iframe>
            <script>
              window.got = [];
              const f = document.getElementById('f');
              f.addEventListener('load', () => got.push('load'));
              addEventListener('message', (e) => {
                got.push(JSON.stringify(e.data), e.origin, e.source === f.contentWindow);
                e.source.postMessage({ reply: 'hi' }, '*');
              });
            </script></body></html>"#;
        let mut parent = Page::from_html("https://example.com/", html);
        let mut network = NetworkManager::new();
        parent.initialize_javascript().unwrap();
        parent.execute_scripts_with(&mut FrameScripts { network: &mut network, page_url: "https://example.com/" }).unwrap();
        let doc = parent.document().unwrap();
        let mut renderer = PageRenderer::new(400, 300);
        renderer.render_document(&doc.lock().unwrap(), 0.0).unwrap();
        let mut frames = Frames::default();
        frames.sync(&mut network, &doc, &renderer.layout_snapshot().unwrap());
        let rt = parent.js_runtime.as_mut().unwrap();
        assert!(frames.exchange(rt, "https://example.com/"));
        assert_eq!(rt.eval("got.join('|')").unwrap(), r#"load|{"hello":"frame","top":true,"self":true}|https://example.com|true"#);
        let frame_rt = frames.frames_mut()[0].page.js_runtime.as_mut().unwrap();
        assert_eq!(frame_rt.eval("document.getElementById('p').textContent").unwrap(), "hi from https://example.com true");
        // The reply changed the frame: a new picture is owed
        assert_eq!(frames.pictures().len(), 1);
    }

    #[test]
    fn clicks_reach_frames() {
        let (doc, renderer) = page_with(
            r#"<html><body style="margin: 0"><iframe id=f style="border: 0; display: block" width=300 height=150 srcdoc="<body style='margin: 0'>
              <div id=b style='height: 30px' onclick='this.textContent = &quot;clicked&quot;'>button</div>
              <a href='data:text/html,<p>next page' style='display: block; height: 30px'>in frame</a>
              <a href='https://example.org/' target=_top style='display: block; height: 30px'>top</a>"></iframe></body></html>"#,
        );
        let mut network = NetworkManager::new();
        let mut frames = Frames::default();
        frames.sync(&mut network, &doc, &renderer.layout_snapshot().unwrap());
        let iframe = frames.frames()[0].element;
        frames.pictures();
        // A click runs the frame's handlers and repaints it
        assert_eq!(frames.click(&mut network, iframe, 10.0, 10.0), None);
        let text = |frames: &mut Frames| frames.frames_mut()[0].page.js_runtime.as_mut().unwrap().eval("document.body.textContent.trim().split(/\\s+/)[0]").unwrap();
        assert_eq!(text(&mut frames), "clicked");
        assert_eq!(frames.pictures().len(), 1);
        // A link with target _top is the tab's to follow
        assert_eq!(frames.click(&mut network, iframe, 10.0, 75.0).as_deref(), Some("https://example.org/"));
        // Another link loads in the frame, which keeps it across syncs
        assert_eq!(frames.click(&mut network, iframe, 10.0, 45.0), None);
        assert!(frames.frames()[0].page.url.starts_with("data:"));
        assert_eq!(text(&mut frames), "next");
        frames.sync(&mut network, &doc, &renderer.layout_snapshot().unwrap());
        assert!(frames.frames()[0].page.url.starts_with("data:"));
    }
}
