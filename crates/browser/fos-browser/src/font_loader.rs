//! Web fonts: the `@font-face` faces a page uses, fetched in parallel
//! (WOFF2, WOFF, TrueType or OpenType) and handed to the renderer, which
//! adds them to a per-page font database over the system's

use std::collections::HashMap;
use std::sync::Arc;

/// Most web fonts a page loads, and the largest file taken
const MAX_FONTS: usize = 24;
const MAX_FONT_BYTES: usize = 4 << 20;

/// A face a page asks for: where to get it and how CSS names it
#[derive(Debug, Clone, PartialEq)]
pub struct FontRequest {
    pub url: String,
    pub family: String,
    pub weight: u16,
    pub italic: bool,
}

/// A fetched face (its file as served: decoded when added to a database)
#[derive(Debug)]
pub struct LoadedFont {
    pub request: FontRequest,
    pub data: Vec<u8>,
}

pub type WebFonts = Arc<Vec<Arc<LoadedFont>>>;

/// The source to fetch: the first in a format we decode
fn pick_source(src: &[(String, Option<String>)]) -> Option<&str> {
    let usable = |(url, format): &(String, Option<String>)| match format.as_deref() {
        Some(f) => matches!(f, "woff2" | "woff" | "truetype" | "opentype" | "woff2-variations" | "truetype-variations" | "opentype-variations" | "woff-variations"),
        None => {
            let path = url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
            [".woff2", ".woff", ".ttf", ".otf"].iter().any(|e| path.ends_with(e)) || url.starts_with("data:")
        }
    };
    src.iter().find(|s| usable(s)).map(|(u, _)| u.as_str())
}

/// The faces `css` declares for families it uses
pub fn requests(css: &str, media: &fos_css::MediaContext, base: &str) -> Vec<FontRequest> {
    let faces = fos_css::font_faces(css, media);
    if faces.is_empty() {
        return Vec::new();
    }
    let lower = css.to_ascii_lowercase();
    // A family is used when it is named more often than its own
    // @font-face rules name it
    let mut declared: HashMap<String, usize> = HashMap::new();
    for face in &faces {
        *declared.entry(face.family.to_ascii_lowercase()).or_default() += 1;
    }
    let mut used: HashMap<String, bool> = HashMap::new();
    let mut out = Vec::new();
    for face in faces {
        let family = face.family.to_ascii_lowercase();
        let is_used = *used.entry(family.clone()).or_insert_with(|| lower.matches(family.as_str()).count() > declared[&family]);
        // Subsets for other scripts are left to the system's fonts
        if !is_used || !face.covers('a') {
            continue;
        }
        let Some(url) = pick_source(&face.src) else { continue };
        let url = if url.starts_with("data:") { url.to_string() } else { fos_net::url_util::resolve(base, url) };
        // Variable fonts covering 400 serve as regular
        let weight = if (face.weight.0..=face.weight.1).contains(&400) { 400 } else { face.weight.0 };
        let req = FontRequest { url, family: face.family, weight, italic: face.italic };
        if !out.contains(&req) {
            out.push(req);
        }
        if out.len() >= MAX_FONTS {
            break;
        }
    }
    out
}

/// Fetch the faces in `wanted`, reusing those in `previous`
pub fn load(network: &mut crate::network::NetworkManager, page_url: &str, wanted: &[FontRequest], previous: &WebFonts) -> WebFonts {
    let mut fonts: Vec<Arc<LoadedFont>> = Vec::new();
    let mut missing = Vec::new();
    for req in wanted {
        match previous.iter().find(|f| f.request == *req) {
            Some(f) => fonts.push(f.clone()),
            None => missing.push(req.clone()),
        }
    }
    if missing.is_empty() && fonts.len() == previous.len() {
        return previous.clone();
    }
    let remote: Vec<String> = missing.iter().filter(|r| r.url.starts_with("http")).map(|r| r.url.clone()).collect();
    let fetched = network.fetch_many(&remote, Some(page_url));
    let mut bodies: HashMap<String, Vec<u8>> = HashMap::new();
    for (url, result) in remote.into_iter().zip(fetched) {
        match result {
            Ok(r) if r.body.len() <= MAX_FONT_BYTES => {
                bodies.insert(url, r.body.to_vec());
            }
            Ok(_) => log::debug!("Font {url} is too large"),
            Err(e) => log::debug!("Font {url} failed: {e}"),
        }
    }
    for req in missing {
        let data = if let Some(d) = req.url.strip_prefix("data:") {
            crate::image_loader::data_url_bytes(d)
        } else {
            bodies.get(&req.url).cloned()
        };
        if let Some(data) = data {
            fonts.push(Arc::new(LoadedFont { request: req, data }));
        }
    }
    log::info!("Loaded {} web fonts", fonts.len());
    Arc::new(fonts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_only_used_families_in_decodable_formats() {
        let media = fos_css::MediaContext { width: 800.0, height: 600.0 };
        let css = r#"@font-face { font-family: Brand; src: url(b.eot), url(b.woff2) format("woff2"); font-weight: 300 800 }
            @font-face { font-family: Unused; src: url(u.woff2) }
            @font-face { font-family: 'Brand'; src: url(bi.woff); font-style: italic }
            @font-face { font-family: Brand; src: url(cyr.woff2); unicode-range: U+0400-045F }
            body { font-family: Brand, sans-serif }"#;
        let reqs = requests(css, &media, "https://example.com/css/site.css");
        assert_eq!(reqs.len(), 2, "{reqs:?}");
        assert_eq!(reqs[0], FontRequest { url: "https://example.com/css/b.woff2".into(), family: "Brand".into(), weight: 400, italic: false });
        assert!(reqs[1].italic && reqs[1].url.ends_with("/bi.woff"));
    }
}
