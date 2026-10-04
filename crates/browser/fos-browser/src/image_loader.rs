//! Images of a page: finding them (`<img>` with `srcset` and `<picture>`,
//! image inputs), fetching them in parallel, and decoding them to bitmaps
//! (PNG, JPEG, GIF, WebP and SVG)
//!
//! Decoded images are the largest thing a page holds in memory, so very
//! large ones are kept downscaled: layout still uses their natural size,
//! and painting scales whatever bitmap it has.

use std::collections::HashMap;
use std::sync::Arc;

use fos_canvas::tiny_skia::{self, Pixmap};
use fos_dom::{Document, DomTree, NodeId};

/// Most pixels kept for one image (about 10 MB)
const MAX_PIXELS: u64 = 2_500_000;
/// Images fetched per page (in document order)
const MAX_IMAGES: usize = 200;
/// SVGs are rasterized at up to this many times their natural size, for
/// sharpness when scaled up (and on dense screens)
const SVG_SCALE: f32 = 2.0;

/// A decoded image
pub struct LoadedImage {
    /// Its natural size in CSS pixels
    pub natural: (f32, f32),
    /// Premultiplied pixels (possibly smaller or larger than natural)
    pub pixmap: Pixmap,
    /// An SVG image's markup, for inline `<svg>`s whose `<use>` refers to
    /// its elements (sprite sheets)
    pub svg_source: Option<Arc<str>>,
}

/// Largest SVG kept as markup for `<use>` references
const MAX_SVG_SOURCE: usize = 1 << 20;

impl std::fmt::Debug for LoadedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LoadedImage({}x{})", self.natural.0, self.natural.1)
    }
}

/// A page's images by absolute URL
pub type Images = Arc<HashMap<String, Arc<LoadedImage>>>;

/// The URL an `<img>` shows: from `srcset` (or a `<picture>` source) the
/// candidate best for a viewport `viewport_w` wide, else `src`
pub fn image_source(tree: &DomTree, node: NodeId, viewport_w: f32) -> Option<String> {
    let parent = tree.get(node).map_or(NodeId::NONE, |n| n.parent);
    let in_picture = tree.get(parent).and_then(|p| p.as_element()).is_some_and(|e| tree.resolve(e.name.local) == "picture");
    if in_picture {
        for (child, n) in tree.children(parent) {
            let Some(e) = n.as_element() else { continue };
            if child == node {
                break;
            }
            if tree.resolve(e.name.local) != "source" {
                continue;
            }
            // Formats we decode, and media we match
            let ty = tree.get_attribute(child, "type").unwrap_or("").to_ascii_lowercase();
            if !ty.is_empty() && !matches!(ty.as_str(), "image/png" | "image/jpeg" | "image/jpg" | "image/gif" | "image/webp" | "image/svg+xml") {
                continue;
            }
            if let Some(m) = tree.get_attribute(child, "media") {
                if !fos_css::media_matches(m, &fos_css::MediaContext { width: viewport_w, height: viewport_w * 0.75 }) {
                    continue;
                }
            }
            if let Some(url) = tree.get_attribute(child, "srcset").and_then(|s| pick_srcset(s, viewport_w)) {
                return Some(url);
            }
        }
    }
    if let Some(url) = tree.get_attribute(node, "srcset").and_then(|s| pick_srcset(s, viewport_w)) {
        return Some(url);
    }
    tree.get_attribute(node, "src").map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// The best `srcset` candidate: a 1x density, else the narrowest width
/// descriptor at least the viewport's (or the widest)
fn pick_srcset(srcset: &str, viewport_w: f32) -> Option<String> {
    let mut by_density: Vec<(f32, &str)> = Vec::new();
    let mut by_width: Vec<(f32, &str)> = Vec::new();
    for candidate in srcset.split(',') {
        let mut parts = candidate.split_whitespace();
        let Some(url) = parts.next() else { continue };
        match parts.next() {
            Some(d) if d.ends_with('w') => {
                if let Ok(w) = d[..d.len() - 1].parse::<f32>() {
                    by_width.push((w, url));
                }
            }
            Some(d) if d.ends_with('x') => {
                if let Ok(x) = d[..d.len() - 1].parse::<f32>() {
                    by_density.push((x, url));
                }
            }
            _ => by_density.push((1.0, url)),
        }
    }
    if !by_width.is_empty() {
        by_width.sort_by(|a, b| a.0.total_cmp(&b.0));
        let want = viewport_w.min(1200.0);
        return by_width.iter().find(|(w, _)| *w >= want).or(by_width.last()).map(|c| c.1.to_string());
    }
    by_density.sort_by(|a, b| a.0.total_cmp(&b.0));
    by_density.iter().find(|(x, _)| *x >= 1.0).or(by_density.last()).map(|c| c.1.to_string())
}

/// The absolute URLs of the document's images, in document order
pub fn image_urls(document: &Document, viewport_w: f32) -> Vec<String> {
    let tree = document.tree();
    let base = crate::css_loader::base_url(document);
    let mut urls = Vec::new();
    // The document's tree and the connected shadow trees
    let mut roots = vec![tree.root()];
    roots.extend(tree.shadow_roots().filter(|&(host, _)| tree.is_connected(host)).map(|(_, root)| root));
    for root in roots {
        fos_dom::selector::walk_elements(tree, root, &mut |id| {
            let Some(e) = tree.get(id).and_then(|n| n.as_element()) else { return true };
            let tag = tree.resolve(e.name.local);
            let src = match tag {
                "img" => image_source(tree, id, viewport_w),
                "input" if tree.get_attribute(id, "type").is_some_and(|t| t.eq_ignore_ascii_case("image")) => tree.get_attribute(id, "src").map(str::to_string),
                // An SVG sprite sheet an inline <svg> uses
                "use" => use_href(tree, id).and_then(|h| external_ref(h)).map(|(doc, _)| doc.to_string()),
                _ => None,
            };
            if let Some(src) = src {
                let url = fos_net::url_util::resolve(&base, &src);
                if !urls.contains(&url) {
                    urls.push(url);
                }
            }
            urls.len() < MAX_IMAGES
        });
    }
    urls
}

/// A `<use>` element's reference (`href`, or the older `xlink:href`)
fn use_href(tree: &DomTree, node: NodeId) -> Option<&str> {
    tree.get_attribute(node, "href").or_else(|| tree.get_attribute(node, "xlink:href"))
}

/// An external reference's document and element id (`sprites.svg#icon`)
fn external_ref(href: &str) -> Option<(&str, &str)> {
    let href = href.trim();
    let (doc, id) = href.split_once('#')?;
    (!doc.is_empty() && !id.is_empty()).then_some((doc, id))
}

/// What inline SVGs refer to outside themselves: loaded SVG images (by
/// absolute URL) and the URL references resolve against
#[derive(Clone, Copy)]
pub struct SvgRefs<'a> {
    pub images: &'a Images,
    pub base: &'a str,
}

/// The markup inside an SVG document's root element
fn svg_contents(src: &str) -> Option<&str> {
    let start = src.find("<svg")?;
    let open_end = start + src[start..].find('>')? + 1;
    if src[..open_end].ends_with("/>") {
        return None;
    }
    let close = src.rfind("</svg")?;
    (close >= open_end).then(|| &src[open_end..close])
}

/// Decode image bytes (raster formats, or SVG)
pub fn decode(bytes: &[u8]) -> Option<LoadedImage> {
    let head = &bytes[..bytes.len().min(512)];
    let looks_svg = head.windows(4).any(|w| w == b"<svg") || head.starts_with(b"<?xml");
    if looks_svg {
        return decode_svg(bytes);
    }
    let img = fos_render::ImageDecoder::decode(bytes).ok()?;
    let (w, h) = (img.width, img.height);
    if w == 0 || h == 0 || img.pixels.len() < (w as usize * h as usize * 4) {
        return None;
    }
    let mut data = img.pixels;
    for px in data.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
    }
    let pixmap = Pixmap::from_vec(data, tiny_skia::IntSize::from_wh(w, h)?)?;
    Some(LoadedImage { natural: (w as f32, h as f32), pixmap: shrink(pixmap), svg_source: None })
}

/// Downscale a bitmap over the pixel budget
fn shrink(pixmap: Pixmap) -> Pixmap {
    let (w, h) = (pixmap.width() as u64, pixmap.height() as u64);
    if w * h <= MAX_PIXELS {
        return pixmap;
    }
    let f = (MAX_PIXELS as f64 / (w * h) as f64).sqrt() as f32;
    let (nw, nh) = (((w as f32) * f).max(1.0) as u32, ((h as f32) * f).max(1.0) as u32);
    let Some(mut out) = Pixmap::new(nw, nh) else { return pixmap };
    let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bicubic, ..Default::default() };
    out.draw_pixmap(0, 0, pixmap.as_ref(), &paint, tiny_skia::Transform::from_scale(nw as f32 / w as f32, nh as f32 / h as f32), None);
    out
}

/// Rasterized inline `<svg>` elements by markup (and color): a page's
/// icons repeat, and relayouts reuse them
#[derive(Default)]
pub struct SvgCache {
    pub(crate) map: HashMap<u64, Option<Arc<LoadedImage>>>,
    /// Bitmap bytes held
    bytes: usize,
}

/// Most inline SVG bitmap bytes kept between layouts
const MAX_CACHED_SVG_BYTES: usize = 16 << 20;

/// An inline `<svg>` element drawn as an image: its subtree serialized
/// as SVG markup, `currentColor` being `color` (CSS rgba)
pub fn inline_svg(tree: &DomTree, node: NodeId, color: [u8; 4], paint: &HashMap<NodeId, String>, refs: Option<SvgRefs>, cache: &mut SvgCache) -> Option<Arc<LoadedImage>> {
    let mut markup = String::new();
    let mut defs = Vec::new();
    serialize_svg(tree, node, true, color, paint, refs, &mut defs, &mut markup);
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        markup.hash(&mut h);
        h.finish()
    };
    if let Some(hit) = cache.map.get(&key) {
        return hit.clone();
    }
    let img = decode_svg(markup.as_bytes()).map(Arc::new);
    let size = img.as_ref().map_or(0, |i| i.pixmap.data().len());
    if cache.bytes + size > MAX_CACHED_SVG_BYTES || cache.map.len() >= 4096 {
        cache.map.clear();
        cache.bytes = 0;
    }
    cache.bytes += size;
    cache.map.insert(key, img.clone());
    img
}

fn escape_xml(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

/// `paint`: CSS declarations (fill, stroke) for elements, put before
/// their own style attribute's. `<use>` references to other SVG
/// documents point into the document's contents, which `defs` gathers
/// (by URL) and the root gets as `<defs>`.
#[allow(clippy::too_many_arguments)]
fn serialize_svg(tree: &DomTree, node: NodeId, root: bool, color: [u8; 4], paint: &HashMap<NodeId, String>, refs: Option<SvgRefs>, defs: &mut Vec<(String, Arc<str>)>, out: &mut String) {
    let Some(n) = tree.get(node) else { return };
    if let Some(text) = n.as_text() {
        escape_xml(text, out);
        return;
    }
    let Some(e) = n.as_element() else { return };
    let name = tree.resolve(e.name.local);
    out.push('<');
    out.push_str(name);
    for a in &e.attrs {
        let local = tree.resolve(a.name.local);
        if root && (local == "xmlns" || local.starts_with("xmlns:") || local == "color") {
            continue;
        }
        out.push(' ');
        if tree.resolve(a.name.ns) == "http://www.w3.org/1999/xlink" && !local.contains(':') {
            out.push_str("xlink:");
        }
        out.push_str(local);
        out.push_str("=\"");
        if local == "style" {
            if let Some(p) = paint.get(&node) {
                escape_xml(p, out);
            }
        }
        let external = (name == "use" && (local == "href" || local == "xlink:href"))
            .then(|| external_ref(&a.value))
            .flatten()
            .and_then(|(doc, id)| {
                let refs = refs?;
                let url = fos_net::url_util::resolve(refs.base, doc);
                let src = refs.images.get(&url)?.svg_source.clone()?;
                Some((url, src, id))
            });
        match external {
            Some((url, src, id)) => {
                out.push('#');
                escape_xml(id, out);
                if !defs.iter().any(|(u, _)| *u == url) {
                    defs.push((url, src));
                }
            }
            None => escape_xml(&a.value, out),
        }
        out.push('"');
    }
    if let Some(p) = paint.get(&node).filter(|_| !e.attrs.iter().any(|a| tree.resolve(a.name.local) == "style")) {
        out.push_str(" style=\"");
        escape_xml(p, out);
        out.push('"');
    }
    if root {
        out.push_str(" xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
        out.push_str(&format!(" color=\"rgba({},{},{},{})\"", color[0], color[1], color[2], color[3] as f32 / 255.0));
    }
    out.push('>');
    for (child, _) in tree.children(node) {
        serialize_svg(tree, child, false, color, paint, refs, defs, out);
    }
    if root && !defs.is_empty() {
        out.push_str("<defs>");
        for (_, src) in defs.iter() {
            if let Some(inner) = svg_contents(src) {
                out.push_str(inner);
            }
        }
        out.push_str("</defs>");
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// The system's fonts, for SVG text (loaded once, when first needed), and
/// the families generic names map to
static SVG_FONTS: std::sync::LazyLock<(Arc<resvg::usvg::fontdb::Database>, String)> = std::sync::LazyLock::new(|| {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    let families: std::collections::HashSet<String> = db.faces().flat_map(|f| f.families.iter().map(|(n, _)| n.to_ascii_lowercase())).collect();
    let pick = |names: &[&str]| names.iter().find(|n| families.contains(&n.to_ascii_lowercase())).map(|n| n.to_string());
    let sans = pick(&["Arial", "Helvetica", "Liberation Sans", "DejaVu Sans", "Noto Sans"]).unwrap_or_else(|| "DejaVu Sans".into());
    if let Some(serif) = pick(&["Times New Roman", "Liberation Serif", "DejaVu Serif", "Noto Serif"]) {
        db.set_serif_family(serif);
    }
    if let Some(mono) = pick(&["Courier New", "Liberation Mono", "DejaVu Sans Mono", "Noto Sans Mono"]) {
        db.set_monospace_family(mono);
    }
    db.set_sans_serif_family(sans.clone());
    (Arc::new(db), sans)
});

fn decode_svg(bytes: &[u8]) -> Option<LoadedImage> {
    let mut options = resvg::usvg::Options::default();
    // Only SVGs with text need the fonts
    if bytes.windows(5).any(|w| w == b"<text") {
        let (db, sans) = &*SVG_FONTS;
        options.fontdb = db.clone();
        options.font_family = sans.clone();
    }
    let tree = resvg::usvg::Tree::from_data(bytes, &options).ok()?;
    let size = tree.size();
    let (w, h) = (size.width(), size.height());
    if !(w > 0.0 && h > 0.0) {
        return None;
    }
    // Rasterized larger than natural (icons are often scaled up), within
    // the pixel budget
    let budget = (MAX_PIXELS as f32 / (w * h)).sqrt();
    let scale = SVG_SCALE.min(budget).max(0.01);
    let (pw, ph) = ((w * scale).ceil().max(1.0) as u32, (h * scale).ceil().max(1.0) as u32);
    let mut pixmap = Pixmap::new(pw, ph)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(pw as f32 / w, ph as f32 / h), &mut pixmap.as_mut());
    let svg_source = (bytes.len() <= MAX_SVG_SOURCE).then(|| String::from_utf8_lossy(bytes).into());
    Some(LoadedImage { natural: (w, h), pixmap, svg_source })
}

/// Fetch and decode the images of `page` (in parallel, through the HTTP
/// cache), keeping those in `previous` that are still used
/// (`css_urls` are images the page's CSS uses: backgrounds and masks)
pub fn load_for_page(network: &mut crate::network::NetworkManager, page: &crate::page::Page, viewport_w: f32, previous: &Images, css_urls: &[String]) -> Images {
    use crate::loader::Loader;
    let Some(doc) = page.document() else { return Default::default() };
    let mut urls = image_urls(&doc.lock().unwrap_or_else(|p| p.into_inner()), viewport_w);
    for u in css_urls {
        if !urls.contains(u) && urls.len() < MAX_IMAGES * 2 {
            urls.push(u.clone());
        }
    }
    let mut images: HashMap<String, Arc<LoadedImage>> = HashMap::new();
    let mut missing = Vec::new();
    for url in urls {
        match previous.get(&url) {
            Some(img) => {
                images.insert(url, img.clone());
            }
            None => missing.push(url),
        }
    }
    if missing.is_empty() {
        // Nothing new: the same set keeps the same identity (no relayout)
        if images.len() == previous.len() {
            return previous.clone();
        }
        return Arc::new(images);
    }
    let start = std::time::Instant::now();
    let page_is_local = Loader::is_local_url(&page.url);
    let mut remote = Vec::new();
    for url in missing {
        if let Some(data) = url.strip_prefix("data:") {
            if let Some(img) = decode_data_url(data) {
                images.insert(url, Arc::new(img));
            }
        } else if Loader::is_local_url(&url) {
            if page_is_local {
                if let Some(img) = crate::loader::file_url_to_path(&url).and_then(|p| std::fs::read(p).ok()).and_then(|b| decode(&b)) {
                    images.insert(url, Arc::new(img));
                }
            }
        } else if url.starts_with("http") {
            remote.push(url);
        }
    }
    let fetched = network.fetch_many(&remote, Some(&page.url));
    for (url, result) in remote.into_iter().zip(fetched) {
        match result {
            Ok(r) => match decode(&r.body) {
                Some(img) => {
                    images.insert(url, Arc::new(img));
                }
                None => log::debug!("Image {url} did not decode"),
            },
            Err(e) => log::debug!("Image {url} failed: {e}"),
        }
    }
    log::info!("Loaded {} images in {:?}", images.len(), start.elapsed());
    Arc::new(images)
}

/// `data:` URL images (base64 or percent-encoded)
pub(crate) fn decode_data_url(rest: &str) -> Option<LoadedImage> {
    decode(&data_url_bytes(rest)?)
}

/// The bytes of a `data:` URL (after `data:`)
pub fn data_url_bytes(rest: &str) -> Option<Vec<u8>> {
    let (meta, payload) = rest.split_once(',')?;
    if meta.ends_with(";base64") {
        let clean: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
        crate::script_fetch::base64_decode(clean.as_bytes())
    } else {
        Some(percent_decode(payload))
    }
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_text_is_drawn() {
        // Badges and charts are SVG text: it needs the system's fonts
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="20"><text x="2" y="15" font-family="Verdana,DejaVu Sans,sans-serif" font-size="14" fill="#000">build</text></svg>"##;
        let img = decode(svg).expect("decodes");
        let inked = img.pixmap.pixels().iter().filter(|p| p.alpha() > 128).count();
        assert!(inked > 20, "{inked} text pixels");
        // Without text, no fonts are needed (or loaded)
        let plain = decode(br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4"/></svg>"#).expect("decodes");
        assert_eq!(plain.natural, (4.0, 4.0));
    }

    #[test]
    fn srcset_candidates() {
        assert_eq!(pick_srcset("a.png 1x, b.png 2x", 800.0).as_deref(), Some("a.png"));
        assert_eq!(pick_srcset("s.png 320w, m.png 800w, l.png 1600w", 800.0).as_deref(), Some("m.png"));
        assert_eq!(pick_srcset("s.png 320w, m.png 640w", 800.0).as_deref(), Some("m.png"));
        assert_eq!(pick_srcset("only.png", 800.0).as_deref(), Some("only.png"));
    }

    #[test]
    fn svg_and_data_urls_decode() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="18" height="12"><rect width="18" height="12" fill="#f60"/></svg>"##;
        let img = decode(svg).expect("svg");
        assert_eq!(img.natural, (18.0, 12.0));
        assert_eq!((img.pixmap.width(), img.pixmap.height()), (36, 24));
        let px = img.pixmap.pixel(10, 10).unwrap();
        assert_eq!((px.red(), px.green(), px.blue(), px.alpha()), (255, 102, 0, 255));
        let data = format!("image/svg+xml,{}", String::from_utf8_lossy(svg).replace('#', "%23"));
        assert!(decode_data_url(&data).is_some());
    }

    #[test]
    fn huge_images_are_kept_downscaled() {
        let big = Pixmap::new(4000, 3000).unwrap();
        let small = shrink(big);
        assert!((small.width() as u64 * small.height() as u64) <= MAX_PIXELS);
        assert!((small.width() as i64 * 3 / 4 - small.height() as i64).abs() <= 1);
    }
}
