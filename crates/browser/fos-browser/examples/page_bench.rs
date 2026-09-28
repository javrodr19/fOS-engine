//! Page load benchmark: fetch, render, and scroll a page, reporting timings
//! and peak memory.
//!
//! ```text
//! cargo run --release -p fos-browser --example page_bench -- https://en.wikipedia.org/wiki/Rust
//! cargo run --release -p fos-browser --example page_bench -- --synthetic 2000
//! ```

use std::time::Instant;

use fos_browser::network::NetworkManager;
use fos_browser::renderer::PageRenderer;

const VIEWPORT_WIDTH: u32 = 1024;
const VIEWPORT_HEIGHT: u32 = 768;

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_else(|| "--synthetic".to_string());

    let (html, url) = if arg == "--synthetic" {
        let paragraphs = std::env::args().nth(2).and_then(|n| n.parse().ok()).unwrap_or(2000);
        (synthetic_page(paragraphs), "https://example.com/synthetic".to_string())
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

    // Render three viewports, as the browser does
    let buffer_height = VIEWPORT_HEIGHT * 3;
    let mut renderer = PageRenderer::new(VIEWPORT_WIDTH, buffer_height);

    let start = Instant::now();
    let first = renderer.render_html(&html, &url, 0.0).expect("render failed");
    println!(
        "first render: {:>8.1} ms  {}x{} buffer, document height {:.0}px, {} links",
        ms(start),
        first.width,
        first.height,
        first.content_height,
        first.links.len()
    );

    // Scroll through the document, re-rendering the buffer each viewport
    let steps = ((first.content_height / VIEWPORT_HEIGHT as f32) as usize).clamp(1, 50);
    let start = Instant::now();
    for i in 1..=steps {
        let y = i as f32 * VIEWPORT_HEIGHT as f32;
        renderer.render_html(&html, &url, y).expect("render failed");
    }
    println!("scroll:       {:>8.1} ms/render over {} re-renders", ms(start) / steps as f64, steps);

    if let Some(peak) = peak_rss_kib() {
        println!("peak RSS:     {:>8.1} MiB", peak as f64 / 1024.0);
    }
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

/// Peak resident set size of this process (Linux only)
fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines()
        .find(|line| line.starts_with("VmHWM:"))?
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
