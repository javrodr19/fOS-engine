//! Random canvas API calls with extreme arguments on live contexts, paths
//! and gradients, reporting panics (a page must never crash the browser).
//! Unlike `fuzz_dom`, every method gets real receivers and up to nine
//! arguments, so the drawing code itself is reached.
//!
//! `cargo run -p fos-browser --example fuzz_canvas [rounds] [seed]` (not
//! `--release`: release builds abort on panic, so it could not be reported).

use std::panic::{catch_unwind, AssertUnwindSafe};

use fos_browser::page::Page;

const SETUP: &str = r#"
var fuzz = (seed, calls) => {
  let s = seed >>> 0 || 1;
  const rnd = (n) => { s ^= s << 13; s >>>= 0; s ^= s >>> 17; s ^= s << 5; s >>>= 0; return s % n; };
  const canvas = document.createElement('canvas');
  canvas.width = 64; canvas.height = 48;
  const ctx = canvas.getContext('2d');
  const off = new OffscreenCanvas(16, 16), octx = off.getContext('2d');
  octx.fillRect(0, 0, 8, 8);
  const path = new Path2D('M1 1 L20 5 Q 3 3 9 9 A5 5 0 1 1 30 30 Z');
  const grad = ctx.createRadialGradient(5, 5, 1, 10, 10, 20);
  grad.addColorStop(0, 'red'); grad.addColorStop(1, 'blue');
  const conic = ctx.createConicGradient(1, 20, 20); conic.addColorStop(0.5, 'lime');
  const pattern = ctx.createPattern(off, 'repeat-x');
  const nums = [0, -0, 1, -1, 0.5, 3, 7, 63, 64, 100, 1e3, 1e5, -1e5, 2 ** 31 - 1, -(2 ** 31), 2 ** 32, 1e30, -1e30, 1e300, Infinity, -Infinity, NaN, Math.PI, -Math.PI * 4];
  const others = ['', 'red', 'evenodd', 'nonzero', 'repeat', 'bold 1e9px serif', '10px x', 'M0 0 L', 'copy', 'xor', 'lighter', null, undefined, true, {},
    [], [1, 2, 3], [1e30, -1], [{ x: 1, y: 2 }], { a: 2, d: 2, e: 1e30 }, { m11: NaN }, path, grad, conic, pattern, off, canvas, ctx, new ImageData(2, 3)];
  const arg = () => rnd(3) ? nums[rnd(nums.length)] : others[rnd(others.length)];
  const targets = [[ctx, CanvasRenderingContext2D.prototype], [octx, OffscreenCanvasRenderingContext2D.prototype], [path, Path2D.prototype], [grad, CanvasGradient.prototype], [pattern, CanvasPattern.prototype]];
  const props = ['fillStyle', 'strokeStyle', 'lineWidth', 'lineCap', 'lineJoin', 'miterLimit', 'lineDashOffset', 'globalAlpha', 'globalCompositeOperation',
    'shadowOffsetX', 'shadowOffsetY', 'shadowBlur', 'shadowColor', 'font', 'textAlign', 'textBaseline', 'direction', 'letterSpacing', 'wordSpacing',
    'imageSmoothingEnabled', 'imageSmoothingQuality', 'filter'];
  let done = 0;
  for (let i = 0; i < calls; i++) {
    const [target, proto] = targets[rnd(targets.length)];
    if (target === ctx || target === octx) {
      if (rnd(4) === 0) { try { target[props[rnd(props.length)]] = arg(); } catch {} continue; }
      if (rnd(200) === 0) { canvas.width = [0, 1, 64, 5000][rnd(4)]; continue; }
    }
    const names = Object.getOwnPropertyNames(proto).filter(n => typeof Object.getOwnPropertyDescriptor(proto, n).value === 'function' && n !== 'constructor');
    const name = names[rnd(names.length)];
    const args = Array.from({ length: rnd(10) }, arg);
    try { target[name](...args); done++; } catch {}
  }
  try { canvas.toDataURL(); off.convertToBlob(); createImageBitmap(canvas, -5, -5, 10, 10); } catch {}
  return done;
};
"#;

fn main() {
    let rounds: u32 = std::env::args().nth(1).and_then(|n| n.parse().ok()).unwrap_or(200);
    let first_seed: u32 = std::env::args().nth(2).and_then(|n| n.parse().ok()).unwrap_or(1);
    if std::env::var_os("FUZZ_VERBOSE").is_none() {
        std::panic::set_hook(Box::new(|_| {}));
    }
    let (mut ok_calls, mut panics) = (0u64, Vec::new());
    for seed in first_seed..first_seed + rounds {
        let r = catch_unwind(AssertUnwindSafe(|| {
            let mut page = Page::from_html("https://example.com/", "<html><body></body></html>");
            page.initialize_javascript().expect("bootstrap");
            let rt = page.js_runtime.as_mut().unwrap();
            rt.eval(SETUP).expect("setup");
            let n = rt.eval(&format!("fuzz({seed}, 400)")).ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            let _ = rt.process_timers(&mut |_: &str| None);
            n
        }));
        match r {
            Ok(n) => ok_calls += n,
            Err(e) => {
                let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
                panics.push(format!("seed {seed}: {msg}"));
            }
        }
    }
    println!("{rounds} rounds, {ok_calls} calls returned normally, {} panics", panics.len());
    for p in &panics {
        println!("  {p}");
    }
}
