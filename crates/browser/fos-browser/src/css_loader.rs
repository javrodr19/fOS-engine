//! Loading a page's external stylesheets
//!
//! `<link rel="stylesheet">` targets are fetched together (the fetcher can
//! run them in parallel), then the `@import`s they start with, one level
//! at a time. Each sheet is stored with its imports inlined in place
//! (wrapped in `@media` when the import has a media query), so the
//! renderer sees one text per `<link>`, in cascade order.

use std::collections::HashMap;
use std::sync::Arc;

use fos_dom::{Document, DomTree, NodeId};

/// Stylesheet texts by absolute URL
pub type Stylesheets = Arc<HashMap<String, Arc<str>>>;

/// Most levels of nested `@import`s followed
const MAX_IMPORT_DEPTH: usize = 5;

/// The document's base URL (`<base href>` or its own URL)
pub fn base_url(document: &Document) -> String {
    let tree = document.tree();
    let base = fos_dom::SelectorList::parse("base[href]")
        .and_then(|s| s.query_first(tree, tree.root()))
        .and_then(|b| tree.get_attribute(b, "href"));
    match base {
        Some(href) => fos_net::url_util::resolve(document.url(), href.trim()),
        None => document.url().to_string(),
    }
}

/// Whether `node` is a `<link>` to a (non-alternate) stylesheet
pub fn is_stylesheet_link(tree: &DomTree, node: NodeId) -> bool {
    let Some(e) = tree.get(node).and_then(|n| n.as_element()) else { return false };
    if !tree.resolve(e.name.local).eq_ignore_ascii_case("link") || tree.get_attribute(node, "disabled").is_some() {
        return false;
    }
    let rel = tree.get_attribute(node, "rel").unwrap_or("").to_ascii_lowercase();
    let mut words = rel.split_ascii_whitespace();
    let is_sheet = rel.split_ascii_whitespace().any(|w| w == "stylesheet");
    is_sheet && !words.any(|w| w == "alternate")
}

/// Absolute URLs of the document's stylesheet links, in document order
pub fn stylesheet_urls(document: &Document) -> Vec<String> {
    let tree = document.tree();
    let base = base_url(document);
    let mut urls = Vec::new();
    fos_dom::selector::walk_elements(tree, tree.root(), &mut |id| {
        if is_stylesheet_link(tree, id) {
            if let Some(href) = tree.get_attribute(id, "href").map(str::trim).filter(|h| !h.is_empty()) {
                let url = fos_net::url_util::resolve(&base, href);
                if !urls.contains(&url) {
                    urls.push(url);
                }
            }
        }
        true
    });
    urls
}

/// An `@import` at the top of a stylesheet
struct Import {
    /// Byte range of the whole statement
    start: usize,
    end: usize,
    url: String,
    media: String,
}

/// The `@import` statements a stylesheet starts with (after `@charset`)
fn imports(css: &str) -> Vec<Import> {
    let b = css.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        // Whitespace and comments
        loop {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if b[i..].starts_with(b"/*") {
                i = css[i + 2..].find("*/").map_or(b.len(), |p| i + 2 + p + 2);
            } else {
                break;
            }
        }
        let rest = &css[i..];
        let lower_start: String = rest.chars().take(8).collect::<String>().to_ascii_lowercase();
        if lower_start.starts_with("@charset") || lower_start.starts_with("@layer") && rest.find(';').is_some_and(|s| rest.find('{').is_none_or(|b| s < b)) {
            i += rest.find(';').map_or(rest.len(), |p| p + 1);
            continue;
        }
        if !lower_start.starts_with("@import") {
            return out;
        }
        let start = i;
        let stmt_end = rest.find(';').map_or(b.len(), |p| i + p);
        let stmt = &css[i + 7..stmt_end.min(b.len())];
        i = (stmt_end + 1).min(b.len());
        let stmt = stmt.trim();
        // url("x") | url(x) | "x"
        let (url, after) = if let Some(r) = stmt.strip_prefix("url(").or_else(|| stmt.strip_prefix("URL(")) {
            let close = r.find(')').unwrap_or(r.len());
            (r[..close].trim().trim_matches(['"', '\'']).to_string(), &r[(close + 1).min(r.len())..])
        } else if let Some(q) = stmt.chars().next().filter(|c| *c == '"' || *c == '\'') {
            let r = &stmt[1..];
            let close = r.find(q).unwrap_or(r.len());
            (r[..close].to_string(), &r[(close + 1).min(r.len())..])
        } else {
            continue;
        };
        // Layer and supports() conditions are not modeled; the media list
        // is what remains
        let mut media = after.trim();
        for prefix in ["layer", "supports("] {
            if media.to_ascii_lowercase().starts_with(prefix) {
                let skip = if prefix == "layer" && !media[5..].starts_with('(') {
                    5
                } else {
                    media.find(')').map_or(media.len(), |p| p + 1)
                };
                media = media[skip..].trim_start();
            }
        }
        if !url.is_empty() {
            out.push(Import { start, end: i, url, media: media.to_string() });
        }
    }
}

/// Load the stylesheets at `urls` with `fetch` (which gets a batch of
/// URLs and returns their texts, `None` for failures), following imports
pub fn load(urls: &[String], fetch: &mut dyn FnMut(&[String]) -> Vec<Option<String>>) -> Stylesheets {
    let mut texts: HashMap<String, String> = HashMap::new();
    let mut frontier: Vec<String> = urls.to_vec();
    for _ in 0..=MAX_IMPORT_DEPTH {
        frontier.retain(|u| !texts.contains_key(u));
        frontier.dedup();
        if frontier.is_empty() {
            break;
        }
        let got = fetch(&frontier);
        let mut next = Vec::new();
        for (url, text) in frontier.iter().zip(got) {
            let text = text.unwrap_or_default();
            for imp in imports(&text) {
                next.push(fos_net::url_util::resolve(url, &imp.url));
            }
            texts.insert(url.clone(), text);
        }
        frontier = next;
    }
    let mut sheets = HashMap::new();
    for url in urls {
        let mut stack = Vec::new();
        sheets.insert(url.clone(), Arc::from(inline_imports(url, &texts, &mut stack)));
    }
    Arc::new(sheets)
}

/// The text of sheet `url` with its imports (recursively) in place
fn inline_imports(url: &str, texts: &HashMap<String, String>, stack: &mut Vec<String>) -> String {
    let Some(text) = texts.get(url) else { return String::new() };
    let list = imports(text);
    if list.is_empty() || stack.len() > MAX_IMPORT_DEPTH {
        return text.clone();
    }
    stack.push(url.to_string());
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for imp in list {
        out.push_str(&text[at..imp.start]);
        let child = fos_net::url_util::resolve(url, &imp.url);
        if !stack.contains(&child) {
            let inner = inline_imports(&child, texts, stack);
            if imp.media.is_empty() {
                out.push_str(&inner);
            } else {
                out.push_str(&format!("@media {} {{\n{}\n}}", imp.media, inner));
            }
            out.push('\n');
        }
        at = imp.end;
    }
    out.push_str(&text[at..]);
    stack.pop();
    out
}

/// Fetch the external stylesheets of `page` (in parallel, through the
/// HTTP cache) with the stylesheets they import
pub fn load_for_page(network: &mut crate::network::NetworkManager, page: &crate::page::Page) -> Stylesheets {
    use crate::loader::Loader;
    let Some(doc) = page.document() else { return Default::default() };
    let urls = stylesheet_urls(&doc.lock().unwrap_or_else(|p| p.into_inner()));
    if urls.is_empty() {
        return Default::default();
    }
    let start = std::time::Instant::now();
    let page_url = page.url.clone();
    let page_is_local = Loader::is_local_url(&page_url);
    let sheets = load(&urls, &mut |batch| {
        let mut texts: Vec<Option<String>> = vec![None; batch.len()];
        let mut remote = Vec::new();
        for (i, url) in batch.iter().enumerate() {
            if Loader::is_local_url(url) {
                // Only local pages may use local files
                if page_is_local {
                    texts[i] = crate::loader::file_url_to_path(url)
                        .and_then(|p| std::fs::read(p).ok())
                        .map(|b| crate::charset::decode_html(b, Some("text/css")));
                }
            } else {
                remote.push(i);
            }
        }
        let remote_urls: Vec<String> = remote.iter().map(|&i| batch[i].clone()).collect();
        for (&i, result) in remote.iter().zip(network.fetch_many(&remote_urls, Some(&page_url))) {
            match result {
                Ok(r) => texts[i] = Some(crate::charset::decode_html(r.body, Some(&r.content_type))),
                Err(e) => log::warn!("Stylesheet {} failed: {}", batch[i], e),
            }
        }
        texts
    });
    log::info!("Loaded {} stylesheets in {:?}", sheets.len(), start.elapsed());
    sheets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_in_document_order() {
        let doc = fos_html::parse_with_url(
            r#"<html><head><base href="https://cdn.example/css/"><link rel="stylesheet" href="a.css">
            <link rel="alternate stylesheet" href="alt.css"><link rel="icon" href="i.png"></head>
            <body><link rel="Stylesheet" href="/b.css"><link rel=stylesheet href="a.css"></body></html>"#,
            "https://example.com/page",
        );
        assert_eq!(stylesheet_urls(&doc), vec!["https://cdn.example/css/a.css", "https://cdn.example/b.css"]);
    }

    #[test]
    fn imports_are_inlined() {
        let files: HashMap<&str, &str> = [
            ("https://x/main.css", "@charset \"utf-8\";\n@import url(\"base.css\");\n@import 'print.css' print;\n@import url(main.css);\nmain { color: red }"),
            ("https://x/base.css", "@import \"deep/d.css\"; base { color: blue }"),
            ("https://x/deep/d.css", "deep { color: green }"),
            ("https://x/print.css", "p { color: black }"),
        ]
        .into_iter()
        .collect();
        let mut batches = Vec::new();
        let sheets = load(&["https://x/main.css".to_string()], &mut |urls| {
            batches.push(urls.to_vec());
            urls.iter().map(|u| files.get(u.as_str()).map(|s| s.to_string())).collect()
        });
        // One batch per import level, fetched together
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[1].len(), 2);
        let css = &sheets["https://x/main.css"];
        let order: Vec<usize> = ["deep {", "base {", "@media print", "main {"].iter().map(|s| css.find(s).unwrap_or_else(|| panic!("{s} missing in {css}"))).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "{css}");
        // The self-import is dropped
        assert_eq!(css.matches("main {").count(), 1);
    }
}
