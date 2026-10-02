//! Call every function a page can reach (DOM bindings, the bootstrap's web
//! APIs, the engine's builtins) with unusual receivers and arguments, and
//! report any that panic: a page must never crash the browser.
//!
//! `cargo run -p fos-browser --example fuzz_dom [filter]` (not `--release`:
//! release builds abort on panic, so the panic could not be reported).
//! `FUZZ_VERBOSE=1` prints each panic with its location.

use std::panic::{catch_unwind, AssertUnwindSafe};

use fos_browser::page::Page;

const PAGE: &str = r#"<html><head><style>p { color: red }</style></head><body><div id=d class="a b" data-x=1>text<p>para</p><!--c--><input id=i value=v><svg><circle/></svg><template><b>t</b></template></div></body></html>"#;

const ENUMERATE: &str = r#"
(() => {
  const out = [], seen = new Set();
  const skip = new Set(['close', 'open', 'stop', 'print', 'alert', 'confirm', 'prompt', 'location', 'history', 'navigate', 'reload', 'assign', 'replace', 'write', 'writeln', '__fosNavigate']);
  const visit = (o, path, depth) => {
    if (o === null || (typeof o !== 'object' && typeof o !== 'function') || seen.has(o) || depth > 3) return;
    seen.add(o);
    for (const k of Reflect.ownKeys(o)) {
      if (typeof k === 'symbol' || skip.has(k)) continue;
      let d; try { d = Object.getOwnPropertyDescriptor(o, k); } catch { continue; }
      if (!d) continue;
      const p = `${path}|${k}`;
      for (const f of [d.value, d.get, d.set]) {
        if (typeof f === 'function') out.push(p + (f === d.get ? '|get' : f === d.set ? '|set' : ''));
      }
      if (d.value && (typeof d.value === 'object' || typeof d.value === 'function')) visit(d.value, p, depth + 1);
    }
  };
  visit(globalThis, 'globalThis', 0);
  return out.join('\n');
})()
"#;

const CALL: &str = r#"
((path) => {
  const parts = path.split('|');
  let o = globalThis;
  const kind = parts[parts.length - 1];
  const keys = (kind === 'get' || kind === 'set') ? parts.slice(1, -1) : parts.slice(1);
  for (let i = 0; i < keys.length - 1; i++) o = o[keys[i]];
  const d = Object.getOwnPropertyDescriptor(o, keys[keys.length - 1]);
  const f = kind === 'get' ? d.get : kind === 'set' ? d.set : d.value;
  const div = document.getElementById('d');
  const ctx = document.createElement('canvas').getContext('2d');
  const vals = [undefined, null, 0, -1, 1.5, NaN, 2 ** 31 - 1, '', 'abc', '<b>x</b>', 'div', '#d', '\uD800', true, Symbol('s'), 10n,
    {}, [], () => 1, new ArrayBuffer(8), new Uint8Array(4), new Proxy({}, {}), new Map(), /x/g, new Error('e'),
    document, document.documentElement, div, div.firstChild, div.querySelector('p'), div.childNodes[2], document.createDocumentFragment(),
    document.createElement('span'), document.createTextNode('t'), document.querySelector('template'), document.querySelector('circle'),
    document.getElementById('i'), new Event('x'), new Blob(['b']), window, Object.create(HTMLElement.prototype), Object.create(Node.prototype),
    ctx, new Path2D('M0 0L5 5'), new OffscreenCanvas(3, 3), new ImageData(2, 2), ctx.createLinearGradient(0, 0, 1, 1), document.createElement('canvas')];
  let n = 0;
  const attempt = (thisArg, args, construct) => {
    n++;
    try { const r = construct ? Reflect.construct(f, args) : Reflect.apply(f, thisArg, args); if (r && typeof r.then === 'function') r.then(() => {}, () => {}); } catch {}
  };
  for (const t of vals) {
    attempt(t, [], false);
    for (const a of vals) attempt(t, [a], false);
  }
  for (const a of vals) for (const b of [undefined, 0, -1, 'x', {}, div, 'beforeend']) { attempt(o, [a, b], false); attempt(o, [b, a, a], false); }
  for (const a of vals) attempt(undefined, [a], true);
  return n;
})
"#;

fn fresh_page() -> Page {
    let mut page = Page::from_html("https://example.com/", PAGE);
    page.initialize_javascript().expect("bootstrap");
    page
}

fn main() {
    let filter = std::env::args().nth(1);
    let mut page = fresh_page();
    let rt = page.js_runtime.as_mut().unwrap();
    let list = rt.eval(ENUMERATE).expect("enumerate");
    let paths: Vec<String> = list.lines().map(String::from).filter(|p| filter.as_ref().is_none_or(|f| p.contains(f.as_str()))).collect();
    println!("{} functions", paths.len());
    if std::env::var_os("FUZZ_VERBOSE").is_none() {
        std::panic::set_hook(Box::new(|_| {}));
    }
    let (mut calls, mut panics) = (0u64, Vec::new());
    for path in &paths {
        eprintln!("{path}");
        let r = catch_unwind(AssertUnwindSafe(|| {
            // A fresh page per function, so calls cannot see earlier damage
            let mut page = fresh_page();
            let rt = page.js_runtime.as_mut().unwrap();
            let call = format!("{CALL}({path:?})");
            let n = rt.eval(&call).ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            let _ = rt.process_timers(&mut |_: &str| None);
            n
        }));
        match r {
            Ok(n) => calls += n,
            Err(e) => {
                let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                panics.push(format!("{path}: {msg}"));
            }
        }
    }
    println!("{calls} calls, {} panics", panics.len());
    for p in &panics {
        println!("  {p}");
    }
}
