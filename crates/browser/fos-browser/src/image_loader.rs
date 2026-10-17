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
/// The longer side of an inline SVG sized in percentages, which has no
/// natural size to rasterize at (CSS sizes its box)
const RELATIVE_SVG_SIDE: f32 = 512.0;

/// A decoded image
pub struct LoadedImage {
    /// Its natural size in CSS pixels
    pub natural: (f32, f32),
    /// Premultiplied pixels (possibly smaller or larger than natural)
    pub pixmap: Pixmap,
    /// An SVG image's markup, for inline `<svg>`s whose `<use>` refers to
    /// its elements (sprite sheets)
    pub svg_source: Option<Arc<str>>,
    /// How an SVG with a `viewBox` fits a box of another shape
    /// (`preserveAspectRatio`); `None` stretches it to the box
    pub svg_fit: Option<SvgFit>,
    /// Only the ratio of `natural` is the image's: an SVG with a viewBox
    /// but no absolute width or height has no natural size
    pub ratio_only: bool,
}

/// The root `<svg>` start tag of `markup`
fn svg_root_tag(markup: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(&markup[..markup.len().min(64 * 1024)]);
    let start = text.match_indices("<svg").map(|(i, _)| i).find(|&i| text[i + 4..].starts_with(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/'))?;
    let tag = &text[start..];
    Some(tag[..tag.find('>').unwrap_or(tag.len())].to_string())
}

/// An attribute's value in a start tag
fn tag_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    tag_attr_span(tag, name).map(|(_, value)| value)
}

/// Where an attribute is in a start tag (from its name through its closing
/// quote), and its value
fn tag_attr_span<'a>(tag: &'a str, name: &str) -> Option<(std::ops::Range<usize>, &'a str)> {
    let at = tag.match_indices(name).map(|(i, _)| i).find(|&i| tag[..i].ends_with(|c: char| c.is_ascii_whitespace()) && tag[i + name.len()..].trim_start().starts_with('='))?;
    let after_eq = tag[at + name.len()..].trim_start()[1..].trim_start();
    let quote = after_eq.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value_at = tag.len() - after_eq.len() + 1;
    let len = tag[value_at..].find(quote)?;
    Some((at..value_at + len + 1, &tag[value_at..value_at + len]))
}

/// An SVG whose root has a viewBox but neither an absolute width nor
/// height: as an image it has a ratio but no natural size
fn svg_ratio_only(markup: &[u8]) -> bool {
    let Some(tag) = svg_root_tag(markup) else { return false };
    tag_attr(&tag, "viewBox").is_some() && svg_absolute(&tag, "width").is_none() && svg_absolute(&tag, "height").is_none()
}

/// An absolute `width` or `height` on a root `<svg>` tag, in px (an em is
/// taken as 16px, as SVG images have no font context)
fn svg_absolute(tag: &str, name: &str) -> Option<f32> {
    let v = tag_attr(tag, name)?.trim();
    let split = v.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(v.len());
    let n: f32 = v[..split].trim().parse().ok().filter(|n: &f32| *n > 0.0 && n.is_finite())?;
    let per = match v[split..].to_ascii_lowercase().as_str() {
        "" | "px" => 1.0,
        "mm" => 96.0 / 25.4,
        "cm" => 96.0 / 2.54,
        "q" => 96.0 / 101.6,
        "in" => 96.0,
        "pt" => 4.0 / 3.0,
        "pc" => 16.0,
        "em" | "rem" => 16.0,
        "ex" => 8.0,
        _ => return None,
    };
    Some(n * per)
}

/// An SVG whose root has a viewBox and only one of width and height (and
/// not the other at all): the other side follows the viewBox's shape
/// (SVG 2 §8.2) rather than being the viewBox's own size. remix.run's
/// footer logo is height="8" with a 440×43 viewBox, so 81.86px wide
fn svg_with_derived_side(markup: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(markup);
    let start = text.match_indices("<svg").map(|(i, _)| i).find(|&i| text[i + 4..].starts_with(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/'))?;
    let end = start + text[start..].find('>')?;
    let tag = &text[start..end];
    let vb: Vec<f32> = tag_attr(tag, "viewBox")?.split(|c: char| c.is_ascii_whitespace() || c == ',').filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
    let [_, _, vw, vh] = vb[..] else { return None };
    if !(vw > 0.0 && vh > 0.0) {
        return None;
    }
    let added = match (svg_absolute(tag, "width"), svg_absolute(tag, "height")) {
        (Some(w), None) if tag_attr(tag, "height").is_none() => format!(" height=\"{}\"", w * vh / vw),
        (None, Some(h)) if tag_attr(tag, "width").is_none() => format!(" width=\"{}\"", h * vw / vh),
        _ => return None,
    };
    Some(format!("{}{}{}", &text[..start + 4], added, &text[start + 4..]))
}

/// An SVG's `preserveAspectRatio`: its contents keep their shape,
/// scaled to fit the box (`meet`) or to cover it (`slice`) and aligned in
/// it (0 start, 0.5 middle, 1 end, per axis)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SvgFit {
    pub align: (f32, f32),
    pub slice: bool,
}

impl SvgFit {
    /// From the root `<svg>` tag of `markup`: only an SVG with a `viewBox`
    /// scales to its box keeping its shape
    fn of(markup: &[u8]) -> Option<SvgFit> {
        let tag = svg_root_tag(markup)?;
        let attr = |name: &str| tag_attr(&tag, name);
        attr("viewBox")?;
        let par = attr("preserveAspectRatio").unwrap_or("").trim();
        let mut words = par.split_ascii_whitespace();
        let align = words.next().unwrap_or("xMidYMid");
        if align == "none" {
            return None;
        }
        let at = |s: &str| match s {
            "Min" => 0.0,
            "Max" => 1.0,
            _ => 0.5,
        };
        let (ax, ay) = match (align.strip_prefix('x'), align.find('Y')) {
            (Some(_), Some(y)) => (at(&align[1..y]), at(&align[y + 1..])),
            _ => (0.5, 0.5),
        };
        Some(SvgFit { align: (ax, ay), slice: words.next() == Some("slice") })
    }

    /// Where an image of `natural` shape is drawn in `dest`
    pub fn place(&self, natural: (f32, f32), dest: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = dest;
        let (sx, sy) = (w / natural.0, h / natural.1);
        let s = if self.slice { sx.max(sy) } else { sx.min(sy) };
        let (dw, dh) = (natural.0 * s, natural.1 * s);
        (x + (w - dw) * self.align.0, y + (h - dh) * self.align.1, dw, dh)
    }
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
    image_source_density(tree, node, viewport_w).map(|(url, _)| url)
}

/// [`image_source`], and the density of the chosen candidate: how many of
/// its pixels make a CSS pixel (its natural size is its pixels' over that)
pub fn image_source_density(tree: &DomTree, node: NodeId, viewport_w: f32) -> Option<(String, f32)> {
    let parent = tree.get(node).map_or(NodeId::NONE, |n| n.parent);
    let in_picture = tree.get(parent).and_then(|p| p.as_element()).is_some_and(|e| tree.resolve(e.name.local) == "picture");
    // The slot width `sizes` gives (100vw without one)
    let slot = |el: NodeId| tree.get_attribute(el, "sizes").and_then(|s| sizes_width(s, viewport_w)).unwrap_or(viewport_w);
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
            if let Some(pick) = tree.get_attribute(child, "srcset").and_then(|s| pick_srcset(s, slot(child))) {
                return Some(pick);
            }
        }
    }
    if let Some(pick) = tree.get_attribute(node, "srcset").and_then(|s| pick_srcset(s, slot(node))) {
        return Some(pick);
    }
    tree.get_attribute(node, "src").map(str::trim).filter(|s| !s.is_empty()).map(|s| (s.to_string(), 1.0))
}

/// The best `srcset` candidate for a slot `slot_w` CSS pixels wide (at one
/// device pixel per CSS pixel): the lowest density of 1 or more, else the
/// highest; and that density
fn pick_srcset(srcset: &str, slot_w: f32) -> Option<(String, f32)> {
    let mut candidates: Vec<(f32, &str)> = Vec::new();
    for candidate in srcset.split(',') {
        let mut parts = candidate.split_whitespace();
        let Some(url) = parts.next() else { continue };
        let density = match parts.next() {
            Some(d) if d.ends_with('w') => match d[..d.len() - 1].parse::<f32>() {
                Ok(w) if w > 0.0 => w / slot_w.max(1.0),
                _ => continue,
            },
            Some(d) if d.ends_with('x') => match d[..d.len() - 1].parse::<f32>() {
                Ok(x) if x > 0.0 => x,
                _ => continue,
            },
            _ => 1.0,
        };
        candidates.push((density, url));
    }
    candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
    candidates.iter().find(|(d, _)| *d >= 1.0).or(candidates.last()).map(|&(d, url)| (url.to_string(), d))
}

/// The slot width a `sizes` attribute gives in a viewport `viewport_w`
/// wide: its first entry whose media condition matches (`auto`: the
/// viewport's width)
fn sizes_width(sizes: &str, viewport_w: f32) -> Option<f32> {
    let mut depth = 0;
    let mut start = 0;
    let mut entries = Vec::new();
    for (i, c) in sizes.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                entries.push(&sizes[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    entries.push(&sizes[start..]);
    let media = fos_css::MediaContext { width: viewport_w, height: viewport_w * 0.75 };
    for entry in entries.iter().map(|e| e.trim()).filter(|e| !e.is_empty()) {
        // The length ends the entry: a function's call or a last word
        let split = if entry.ends_with(')') {
            let mut depth = 0;
            let open = entry.char_indices().rev().find(|&(_, c)| {
                match c {
                    ')' => depth += 1,
                    '(' => depth -= 1,
                    _ => {}
                }
                depth == 0
            });
            open.map(|(i, _)| entry[..i].rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).map_or(0, |j| j + 1))
        } else {
            Some(entry.rfind(char::is_whitespace).map_or(0, |i| i + 1))
        };
        let Some(at) = split else { continue };
        let (condition, length) = (entry[..at].trim(), entry[at..].trim());
        if !condition.is_empty() && !fos_css::media_matches(condition, &media) {
            continue;
        }
        if length.eq_ignore_ascii_case("auto") {
            return Some(viewport_w);
        }
        return SizeExpr { s: length.as_bytes(), at: 0, vw: viewport_w }.sum().filter(|w| w.is_finite() && *w >= 0.0);
    }
    None
}

/// A `sizes` length: px, em, rem, vw, vh, in calc(), min(), max() and
/// clamp() with + - * /
struct SizeExpr<'a> {
    s: &'a [u8],
    at: usize,
    vw: f32,
}

impl SizeExpr<'_> {
    fn ws(&mut self) {
        while self.s.get(self.at).is_some_and(|c| c.is_ascii_whitespace()) {
            self.at += 1;
        }
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.s.get(self.at) == Some(&c) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn sum(&mut self) -> Option<f32> {
        let mut v = self.product()?;
        loop {
            if self.eat(b'+') {
                v += self.product()?;
            } else if self.eat(b'-') {
                v -= self.product()?;
            } else {
                return Some(v);
            }
        }
    }

    fn product(&mut self) -> Option<f32> {
        let mut v = self.term()?;
        loop {
            if self.eat(b'*') {
                v *= self.term()?;
            } else if self.eat(b'/') {
                v /= self.term()?;
            } else {
                return Some(v);
            }
        }
    }

    fn term(&mut self) -> Option<f32> {
        self.ws();
        let rest = std::str::from_utf8(&self.s[self.at..]).ok()?;
        if self.eat(b'(') {
            let v = self.sum()?;
            return self.eat(b')').then_some(v);
        }
        for (name, f) in [("calc(", 0u8), ("min(", 1), ("max(", 2), ("clamp(", 3)] {
            if rest.len() >= name.len() && rest[..name.len()].eq_ignore_ascii_case(name) {
                self.at += name.len();
                let mut args = vec![self.sum()?];
                while self.eat(b',') {
                    args.push(self.sum()?);
                }
                if !self.eat(b')') {
                    return None;
                }
                return match f {
                    0 => args.first().copied(),
                    1 => args.into_iter().reduce(f32::min),
                    2 => args.into_iter().reduce(f32::max),
                    _ => (args.len() == 3).then(|| args[1].clamp(args[0], args[2].max(args[0]))),
                };
            }
        }
        let sign = usize::from(rest.starts_with(['-', '+']));
        let num_len = sign + rest[sign..].find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(rest.len() - sign);
        let n: f32 = rest[..num_len].parse().ok()?;
        self.at += num_len;
        let unit_len = rest[num_len..].find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(rest.len() - num_len);
        let unit = rest[num_len..num_len + unit_len].to_ascii_lowercase();
        self.at += unit_len;
        Some(match unit.as_str() {
            "px" | "" => n,
            "vw" | "svw" | "lvw" | "dvw" => n * self.vw / 100.0,
            "vh" | "svh" | "lvh" | "dvh" => n * self.vw * 0.75 / 100.0,
            "em" | "rem" => n * 16.0,
            _ => return None,
        })
    }
}

/// The absolute URLs of the document's images, in document order
/// The `<img>` elements whose images are in `images`, with their URLs
pub fn loaded_img_elements(document: &Document, images: &Images, viewport_w: f32) -> Vec<(NodeId, String)> {
    let tree = document.tree();
    let base = crate::css_loader::base_url(document);
    let mut out = Vec::new();
    let mut roots = vec![tree.root()];
    roots.extend(tree.shadow_roots().filter(|&(host, _)| tree.is_connected(host)).map(|(_, root)| root));
    for root in roots {
        fos_dom::selector::walk_elements(tree, root, &mut |id| {
            let is_img = tree.get(id).and_then(|n| n.as_element()).is_some_and(|e| tree.resolve(e.name.local) == "img");
            if let Some(src) = is_img.then(|| image_source(tree, id, viewport_w)).flatten() {
                let url = fos_net::url_util::resolve(&base, src.trim());
                if images.contains_key(&url) {
                    out.push((id, url));
                }
            }
            true
        });
    }
    out
}

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
    Some(LoadedImage { natural: (w as f32, h as f32), pixmap: shrink(pixmap), svg_source: None, svg_fit: None, ratio_only: false })
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

/// What CSS makes of an inline SVG's elements
#[derive(Default)]
pub struct SvgStyles {
    /// Declarations (fill, stroke) for elements, put before their own
    /// style attribute's
    pub paint: HashMap<NodeId, String>,
    /// Presentation attributes with `var()` in them, substituted (`None`
    /// when invalid: the attribute is then left out)
    pub attrs: HashMap<NodeId, Vec<(String, Option<String>)>>,
}

/// An inline `<svg>` element drawn as an image: its subtree serialized
/// as SVG markup, `currentColor` being `color` (CSS rgba)
pub fn inline_svg(tree: &DomTree, node: NodeId, color: [u8; 4], paint: &SvgStyles, refs: Option<SvgRefs>, cache: &mut SvgCache) -> Option<Arc<LoadedImage>> {
    let mut markup = String::new();
    let mut defs = Vec::new();
    serialize_svg(tree, node, true, color, paint, refs, &mut defs, &mut markup);
    // A percentage size is of the containing block, which CSS resolves (a
    // presentation attribute); the drawing takes the viewBox's shape
    let relative = tree.get_attribute(node, "viewBox").is_some() && ["width", "height"].iter().any(|a| tree.get_attribute(node, a).is_some_and(|v| v.trim().ends_with('%')));
    let key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        markup.hash(&mut h);
        h.finish()
    };
    if let Some(hit) = cache.map.get(&key) {
        return hit.clone();
    }
    // With a viewBox but no absolute size, it has only a ratio, and fills
    // the width it is given as an <img> of it would
    let ratio_only = svg_ratio_only(markup.as_bytes());
    let img = match svg_with_derived_side(markup.as_bytes()).filter(|_| !relative) {
        Some(derived) => decode_svg_at(derived.as_bytes(), 0.0),
        None => decode_svg_at(markup.as_bytes(), if relative || ratio_only { RELATIVE_SVG_SIDE } else { 0.0 }),
    }
    .map(|i| Arc::new(LoadedImage { ratio_only, ..i }));
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

/// `styles`: what CSS makes of the elements. `<use>` references to other
/// SVG documents point into the document's contents, which `defs`
/// gathers (by URL) and the root gets as `<defs>`.
#[allow(clippy::too_many_arguments)]
fn serialize_svg(tree: &DomTree, node: NodeId, root: bool, color: [u8; 4], styles: &SvgStyles, refs: Option<SvgRefs>, defs: &mut Vec<(String, Arc<str>)>, out: &mut String) {
    let paint = &styles.paint;
    let substituted = styles.attrs.get(&node);
    let Some(n) = tree.get(node) else { return };
    if let Some(text) = n.as_text() {
        escape_xml(text, out);
        return;
    }
    let Some(e) = n.as_element() else { return };
    let name = tree.resolve(e.name.local);
    out.push('<');
    out.push_str(name);
    let has_view_box = root && e.attrs.iter().any(|a| tree.resolve(a.name.local) == "viewBox");
    for a in &e.attrs {
        let local = tree.resolve(a.name.local);
        if root && (local == "xmlns" || local.starts_with("xmlns:") || local == "color") {
            continue;
        }
        if has_view_box && (local == "width" || local == "height") && a.value.trim().ends_with('%') {
            continue;
        }
        let value = match substituted.and_then(|l| l.iter().find(|(n, _)| n == local)) {
            Some((_, Some(v))) => v.as_str(),
            Some((_, None)) => continue,
            None => &a.value,
        };
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
            None => escape_xml(value, out),
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
        serialize_svg(tree, child, false, color, styles, refs, defs, out);
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
    // One sized by its box is drawn at a size that box may well have, in
    // its viewBox's shape (a percentage width or height there is of the
    // box, not of the viewBox)
    if !svg_ratio_only(bytes) {
        if let Some(markup) = svg_with_derived_side(bytes) {
            let img = decode_svg_at(markup.as_bytes(), 0.0)?;
            return Some(LoadedImage { svg_source: img.svg_source.and(Some(String::from_utf8_lossy(bytes).into())), ..img });
        }
        return decode_svg_at(bytes, 0.0);
    }
    let text = String::from_utf8_lossy(bytes);
    let start = text.match_indices("<svg").map(|(i, _)| i).find(|&i| text[i + 4..].starts_with(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/'))?;
    let end = start + text[start..].find('>')?;
    let mut tag = text[start..end].to_string();
    for name in ["width", "height"] {
        if let Some((span, _)) = tag_attr_span(&tag, name) {
            tag.replace_range(span, "");
        }
    }
    let markup = format!("{}{}{}", &text[..start], tag, &text[end..]);
    let img = decode_svg_at(markup.as_bytes(), RELATIVE_SVG_SIDE)?;
    Some(LoadedImage { ratio_only: true, svg_source: img.svg_source.and(Some(Arc::from(text.as_ref()))), ..img })
}

/// Rasterized with its longer side at least `min_side` pixels (within the
/// pixel budget)
fn decode_svg_at(bytes: &[u8], min_side: f32) -> Option<LoadedImage> {
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
    let scale = SVG_SCALE.max(min_side / w.max(h)).min(budget).max(0.01);
    let (pw, ph) = ((w * scale).ceil().max(1.0) as u32, (h * scale).ceil().max(1.0) as u32);
    let mut pixmap = Pixmap::new(pw, ph)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(pw as f32 / w, ph as f32 / h), &mut pixmap.as_mut());
    let svg_source = (bytes.len() <= MAX_SVG_SOURCE).then(|| String::from_utf8_lossy(bytes).into());
    Some(LoadedImage { natural: (w, h), pixmap, svg_source, svg_fit: SvgFit::of(bytes), ratio_only: false })
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
    fn srcset_candidates_fit_the_sizes_slot() {
        let set = "a.jpg 240w, b.jpg 640w, c.jpg 800w, d.jpg 1536w";
        // (min-width: 768px) 50vw: a 640px slot in a 1280px viewport
        let slot = sizes_width("(min-width: 768px) 50vw, 100vw", 1280.0).unwrap();
        assert_eq!(slot, 640.0);
        assert_eq!(pick_srcset(set, slot), Some(("b.jpg".to_string(), 1.0)));
        assert_eq!(sizes_width("(min-width: 768px) 50vw, 100vw", 600.0), Some(600.0));
        assert_eq!(sizes_width("(max-width: 600px) 100vw, calc(33vw - 2 * 16px)", 1200.0), Some(396.0 - 32.0));
        assert_eq!(sizes_width("min(400px, 50vw)", 1000.0), Some(400.0));
        assert_eq!(sizes_width("25vw", 1280.0), Some(320.0));
        // The widest candidate for a slot wider than all, at its density
        let (url, density) = pick_srcset(set, 2000.0).unwrap();
        assert_eq!((url.as_str(), density), ("d.jpg", 1536.0 / 2000.0));
        assert_eq!(pick_srcset("x1.png 1x, x2.png 2x", 500.0), Some(("x1.png".to_string(), 1.0)));
    }

    #[test]
    fn svgs_with_a_view_box_keep_their_shape() {
        let fit = |m: &str| SvgFit::of(m.as_bytes());
        let mid = SvgFit { align: (0.5, 0.5), slice: false };
        assert_eq!(fit(r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 512"><path/></svg>"#), Some(mid));
        assert_eq!(fit(r#"<svg width="10" height="10"><rect/></svg>"#), None);
        assert_eq!(fit(r#"<svg viewBox='0 0 1 1' preserveAspectRatio='none'/>"#), None);
        assert_eq!(fit(r#"<svg viewBox="0 0 1 1" preserveAspectRatio="xMinYMax slice">"#), Some(SvgFit { align: (0.0, 1.0), slice: true }));
        // A 320x512 icon in a 40px square: 25px wide, centered
        assert_eq!(mid.place((320.0, 512.0), (0.0, 0.0, 40.0, 40.0)), (7.5, 0.0, 25.0, 40.0));
    }

    #[test]
    fn svgs_without_a_size_have_only_a_ratio() {
        // As images, SVGs with a viewBox but no absolute width or height
        // fill their box in the viewBox's shape (htmx.org's sponsor logos)
        assert!(svg_ratio_only(br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 842 206"/>"#));
        assert!(svg_ratio_only(br#"<svg width="50%" stroke-width="2" viewBox="0 0 100 50"/>"#));
        assert!(!svg_ratio_only(br#"<svg width="100" viewBox="0 0 100 50"/>"#));
        assert!(!svg_ratio_only(br#"<svg width="10" height="10"/>"#));
        assert!(!svg_ratio_only(br#"<svg/>"#));
        let img = decode_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="50%" stroke-width="2" viewBox="0 0 100 50"><rect width="100" height="50"/></svg>"#).unwrap();
        assert!(img.ratio_only);
        assert_eq!(img.natural.0 / img.natural.1, 2.0);
        // Drawn at a size a box may well have
        assert!(img.pixmap.width() >= 512);
        let sized = decode_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10"/></svg>"#).unwrap();
        assert!(!sized.ratio_only);
        assert_eq!(sized.natural, (20.0, 10.0));
    }

    #[test]
    fn svgs_with_one_side_take_the_other_from_the_viewbox() {
        // remix.run's footer logo: height="8" and a 440×43 viewBox is
        // 81.86px wide, not the viewBox's 440
        let img = decode_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" height="8" viewBox="0 0 440 43"><rect width="440" height="43"/></svg>"#).unwrap();
        assert!(!img.ratio_only);
        assert_eq!(((img.natural.0 * 100.0).round(), img.natural.1), (8186.0, 8.0));
        let img = decode_svg(br#"<svg xmlns="http://www.w3.org/2000/svg" width="100px" viewBox="0,0,440,43"><rect width="440" height="43"/></svg>"#).unwrap();
        assert_eq!((img.natural.0, (img.natural.1 * 100.0).round()), (100.0, 977.0));
        // Both sides, or a percentage other side, leave it as it was
        assert!(svg_with_derived_side(br#"<svg width="10" height="8" viewBox="0 0 440 43"/>"#).is_none());
        assert!(svg_with_derived_side(br#"<svg width="50%" height="8" viewBox="0 0 440 43"/>"#).is_none());
        // Absolute units are sizes too (sqlite.org's logo is in mm)
        assert!(!svg_ratio_only(br#"<svg width="103.7mm" height="46.7mm" viewBox="0 0 103.7 46.7"/>"#));
        assert_eq!(svg_absolute(r#"<svg width="1in" height="12pt""#, "width"), Some(96.0));
        assert_eq!(svg_absolute(r#"<svg width="1in" height="12pt""#, "height"), Some(16.0));
    }

    #[test]
    fn loaded_images_are_found_by_element() {
        let doc = fos_html::parse_with_url(r#"<html><body><img id=a src="a.png"><img id=b src="b.png"><img id=c srcset="c.png 1x"><p>x</p></body></html>"#, "https://example.com/");
        let image = Arc::new(LoadedImage { natural: (1.0, 1.0), pixmap: Pixmap::new(1, 1).unwrap(), svg_source: None, svg_fit: None, ratio_only: false });
        let images: Images = Arc::new(["https://example.com/a.png", "https://example.com/c.png"].into_iter().map(|u| (u.to_string(), image.clone())).collect());
        let found = loaded_img_elements(&doc, &images, 800.0);
        let ids: Vec<&str> = found.iter().filter_map(|(n, _)| doc.tree().get_attribute(*n, "id")).collect();
        assert_eq!(ids, ["a", "c"]);
        assert_eq!(found[1].1, "https://example.com/c.png");
    }

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
        let url = |set: &str| pick_srcset(set, 800.0).map(|(u, _)| u);
        assert_eq!(url("a.png 1x, b.png 2x").as_deref(), Some("a.png"));
        assert_eq!(url("s.png 320w, m.png 800w, l.png 1600w").as_deref(), Some("m.png"));
        assert_eq!(url("s.png 320w, m.png 640w").as_deref(), Some("m.png"));
        assert_eq!(url("only.png").as_deref(), Some("only.png"));
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
