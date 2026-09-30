//! Load a page, run its scripts and timers headlessly, and report what
//! happened: `cargo run --release --example run_page -- <url> [ms]`

use std::time::{Duration, Instant};

use fos_browser::loader::Loader;
use fos_browser::network::NetworkManager;
use fos_browser::page::Page;

fn rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| l.split_whitespace().nth(1)?.parse().ok()))
        .unwrap_or(0)
}

fn main() {
    env_logger::init();
    let mut args = std::env::args().skip(1);
    let url = args.next().expect("usage: run_page <url> [ms]");
    let run_for = Duration::from_millis(args.next().and_then(|a| a.parse().ok()).unwrap_or(500));

    let mut network = NetworkManager::new();
    let start = Instant::now();
    let mut page = if Loader::is_local_url(&url) {
        Loader::new().load_sync(&url).expect("load")
    } else {
        let fetched = network.fetch_page(&url).expect("fetch");
        Page::from_html(&fetched.url, fetched.html)
    };
    println!("loaded in {:?}, RSS {} KB", start.elapsed(), rss_kb());

    let page_url = page.url.clone();
    let mut fetched = Vec::new();
    let mut fetch = |u: &str| -> Option<String> {
        let t = Instant::now();
        let r = if Loader::is_local_url(u) {
            let path = u.strip_prefix("file://")?;
            std::fs::read_to_string(path).ok()
        } else {
            network.fetch(u, Some(&page_url)).ok().map(|r| String::from_utf8_lossy(&r.body).into_owned())
        };
        fetched.push(format!("{u} ({} bytes, {:?})", r.as_ref().map_or(0, |s| s.len()), t.elapsed()));
        r
    };

    let t = Instant::now();
    page.initialize_javascript().expect("init");
    let init = t.elapsed();
    let t = Instant::now();
    page.execute_scripts_with(&mut fetch).expect("scripts");
    println!("JS init {:?}, scripts ran in {:?} (incl. fetching), RSS {} KB", init, t.elapsed(), rss_kb());

    let deadline = Instant::now() + run_for;
    let mut ticks = 0;
    while Instant::now() < deadline {
        match page.next_timer_due() {
            Some(due) => {
                let now = Instant::now();
                if due > now {
                    std::thread::sleep((due - now).min(deadline - now));
                }
                page.process_timers_with(&mut fetch).expect("timers");
                ticks += 1;
            }
            None => break,
        }
    }
    println!("{ticks} timer rounds; timers still pending: {}", page.has_pending_timers());
    drop(fetch);
    for f in &fetched {
        println!("  fetched {f}");
    }

    if let Some(rt) = page.js_runtime.as_ref() {
        println!("JS heap {} KB", rt.heap_size() / 1024);
        let msgs = rt.console_messages();
        println!("{} console messages", msgs.len());
        for m in msgs.iter().take(30) {
            let text: String = m.message.chars().take(300).collect();
            println!("  [{:?}] {}", m.level, text);
        }
    }
    if let Some(doc) = page.document() {
        let doc = doc.lock().unwrap();
        let text = doc.tree().text_content(doc.body());
        let words: Vec<&str> = text.split_whitespace().take(60).collect();
        println!("DOM: {} nodes; title {:?}", doc.tree().len(), doc.title());
        println!("body text: {}", words.join(" "));
    }
    // Further arguments: expressions to evaluate in the page
    for expr in args {
        if let Some(rt) = page.js_runtime.as_mut() {
            println!("> {expr}\n{:?}", rt.eval(&expr));
        }
    }
    if let Some(nav) = page.take_script_navigation() {
        println!("script navigated to {nav}");
    }
    println!("final RSS {} KB", rss_kb());
}
