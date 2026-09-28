//! Page load benchmark: fetch, render, and scroll a page, reporting timings
//! and peak memory.
//!
//! ```text
//! cargo run --release -p fos-browser --example page_bench -- https://en.wikipedia.org/wiki/Rust
//! cargo run --release -p fos-browser --example page_bench -- --synthetic 2000
//! cargo run --release -p fos-browser --example page_bench -- --file saved.html
//! ```

use std::time::Instant;

use fos_browser::network::NetworkManager;
use fos_browser::page::Page;
use fos_browser::renderer::PageRenderer;

const VIEWPORT_WIDTH: u32 = 1024;
const VIEWPORT_HEIGHT: u32 = 768;

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "--synthetic".to_string());

    let (html, url) = if arg == "--synthetic" {
        let paragraphs = std::env::args().nth(2).and_then(|n| n.parse().ok()).unwrap_or(2000);
        (synthetic_page(paragraphs), "https://example.com/synthetic".to_string())
    } else if arg == "--file" {
        // A saved page, for runs without network variance
        let path = std::env::args().nth(2).expect("--file needs a path");
        let bytes = std::fs::read(&path).unwrap_or_else(|e| {
            eprintln!("cannot read {}: {}", path, e);
            std::process::exit(1);
        });
        (String::from_utf8_lossy(&bytes).into_owned(), format!("file://{}", path))
    } else {
        let mut network = NetworkManager::new();
        let start = Instant::now();
        let page = match network.fetch_page(&arg) {
            Ok(page) => page,
            Err(e) => {
                eprintln!("fetch failed: {}", e);
                std::process::exit(1);
            }
        };
        println!(
            "fetch:        {:>8.1} ms  status {}  {} bytes  final URL {}",
            ms(start),
            page.status,
            page.html.len(),
            page.url
        );
        (page.html, page.url)
    };

    // Parse once into the page's DOM, as the browser does
    let start = Instant::now();
    let page = Page::from_html(&url, html);
    let document = page.document().expect("page has a DOM");
    let document = document.lock().unwrap();
    println!("parse:        {:>8.1} ms  {} DOM nodes  (RSS {})", ms(start), document.tree().len(), rss());

    // Render three viewports, as the browser does
    let buffer_height = VIEWPORT_HEIGHT * 3;
    let mut renderer = PageRenderer::new(VIEWPORT_WIDTH, buffer_height);

    let start = Instant::now();
    let first = renderer.render_document(&document, 0.0).expect("render failed");
    println!(
        "first render: {:>8.1} ms  (layout + paint) {}x{} buffer, document height {:.0}px, {} links",
        ms(start),
        first.width,
        first.height,
        first.content_height,
        first.links.len()
    );
    println!("              RSS after first render {}", rss());

    // Scroll through the document, moving the buffer one viewport at a time
    // as the browser does (only the newly exposed rows are painted)
    let steps = ((first.content_height / VIEWPORT_HEIGHT as f32) as usize).clamp(1, 50);
    let start = Instant::now();
    let mut page = first;
    for i in 1..=steps {
        let y = i as f32 * VIEWPORT_HEIGHT as f32;
        page = renderer.render_document_scrolled(&document, y, page).expect("render failed");
    }
    println!("scroll:       {:>8.1} ms/render over {} re-renders", ms(start) / steps as f64, steps);

    // Full re-renders of the whole buffer (e.g. after a DOM change)
    let start = Instant::now();
    for i in 1..=steps {
        renderer.render_document(&document, i as f32 * VIEWPORT_HEIGHT as f32).expect("render failed");
    }
    println!("full repaint: {:>8.1} ms/render", ms(start) / steps as f64);

    if let Some(peak) = peak_rss_kib() {
        println!("peak RSS:     {:>8.1} MiB", peak as f64 / 1024.0);
    }
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// Peak resident set size of this process (Linux only)
fn peak_rss_kib() -> Option<u64> {
    status_kib("VmHWM:")
}

/// Current resident set size, formatted
fn rss() -> String {
    let mib = |kib: Option<u64>| kib.map_or_else(|| "n/a".into(), |kib| format!("{:.1} MiB", kib as f64 / 1024.0));
    format!("{}, peak so far {}", mib(status_kib("VmRSS:")), mib(status_kib("VmHWM:")))
}

/// A `/proc/self/status` field in KiB (Linux only)
fn status_kib(field: &str) -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines()
        .find(|line| line.starts_with(field))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// A text-heavy page with headings, paragraphs, links and lists
fn synthetic_page(paragraphs: usize) -> String {
    let mut html = String::from("<!DOCTYPE html><html><head><title>Synthetic</title></head><body>");
    for i in 0..paragraphs {
        if i % 20 == 0 {
            html.push_str(&format!("<h2 id=\"s{0}\">Section {0}</h2>", i / 20));
        }
        html.push_str(&format!(
            "<p>Paragraph {i} with some ordinary prose, a <a href=\"/page/{i}\">link to page {i}</a>, \
             and enough words to wrap across more than one line in a typical viewport width.</p>"
        ));
        if i % 10 == 0 {
            html.push_str("<ul><li>First item</li><li>Second item</li><li>Third item</li></ul>");
        }
    }
    html.push_str("</body></html>");
    html
}
