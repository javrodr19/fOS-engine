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
}

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
    fos_dom::selector::walk_elements(tree, tree.root(), &mut |id| {
        let Some(e) = tree.get(id).and_then(|n| n.as_element()) else { return true };
        let tag = tree.resolve(e.name.local);
        let src = match tag {
            "img" => image_source(tree, id, viewport_w),
            "input" if tree.get_attribute(id, "type").is_some_and(|t| t.eq_ignore_ascii_case("image")) => tree.get_attribute(id, "src").map(str::to_string),
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
    urls
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
    Some(LoadedImage { natural: (w as f32, h as f32), pixmap: shrink(pixmap) })
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

fn decode_svg(bytes: &[u8]) -> Option<LoadedImage> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
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
    Some(LoadedImage { natural: (w, h), pixmap })
}

/// Fetch and decode the images of `page` (in parallel, through the HTTP
/// cache), keeping those in `previous` that are still used
pub fn load_for_page(network: &mut crate::network::NetworkManager, page: &crate::page::Page, viewport_w: f32, previous: &Images) -> Images {
    use crate::loader::Loader;
    let Some(doc) = page.document() else { return Default::default() };
    let urls = image_urls(&doc.lock().unwrap_or_else(|p| p.into_inner()), viewport_w);
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
fn decode_data_url(rest: &str) -> Option<LoadedImage> {
    let (meta, payload) = rest.split_once(',')?;
    let bytes = if meta.ends_with(";base64") {
        let clean: String = payload.chars().filter(|c| !c.is_whitespace()).collect();
        crate::script_fetch::base64_decode(clean.as_bytes())?
    } else {
        percent_decode(payload)
    };
    decode(&bytes)
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
