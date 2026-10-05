//! Call every built-in function with unusual receivers and arguments and
//! report any that panic (a script must never crash the engine).
//!
//! `cargo run --example fuzz_builtins [filter]` (not `--release`: release
//! builds abort on panic, so the panic could not be reported)

use std::panic::{catch_unwind, AssertUnwindSafe};

/// Collects `path` strings of every function reachable from the global
/// object through own properties (constructors, prototypes, namespaces)
const ENUMERATE: &str = r#"
(() => {
  const out = [], seen = new Set();
  const visit = (o, path, depth) => {
    if (o === null || (typeof o !== 'object' && typeof o !== 'function') || seen.has(o) || depth > 3) return;
    seen.add(o);
    for (const k of Reflect.ownKeys(o)) {
      if (typeof k === 'symbol') {
        if (k !== Symbol.iterator && k !== Symbol.toPrimitive) continue;
      }
      let d; try { d = Object.getOwnPropertyDescriptor(o, k); } catch { continue; }
      if (!d) continue;
      const name = typeof k === 'symbol' ? `[${k.description}]` : k;
      const p = `${path}|${name}`;
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

/// Resolves a path and calls the function with each receiver/argument
/// combination; exceptions are fine, panics are not
const CALLER: &str = r#"
(path) => {
  const parts = path.split('|');
  let o = globalThis, f;
  const kind = parts[parts.length - 1];
  const keys = (kind === 'get' || kind === 'set') ? parts.slice(1, -1) : parts.slice(1);
  const key = (k) => k.startsWith('[') ? Symbol[k.slice(8, -1)] ?? k : k;
  for (let i = 0; i < keys.length - 1; i++) o = o[key(keys[i])];
  const d = Object.getOwnPropertyDescriptor(o, key(keys[keys.length - 1]));
  f = kind === 'get' ? d.get : kind === 'set' ? d.set : d.value;
  const ab = new ArrayBuffer(8);
  const vals = [undefined, null, 0, -1, 1.5, NaN, 2 ** 31 - 1, -(2 ** 53), '', 'abc', '\uD800', true, Symbol('s'), 10n,
    {}, [], [1, , 3], Object.create(null), () => 1, ab, new Uint8Array(ab), new DataView(ab), new Proxy({}, {}), new Proxy([], {}),
    new Map([[1, 2]]), new Set([1]), new WeakMap(), Promise.resolve(1), /x/g, new Date(NaN), new Error('e'), Object.freeze([1]),
    { length: 3, 0: 'a', 2: 'c' }, { length: -1, 0: 'a' }, { valueOf() { throw 1; } }, { get x() { throw 2; } }, Math, JSON, globalThis];
  let n = 0;
  const attempt = (thisArg, args, construct) => {
    n++;
    try { construct ? Reflect.construct(f, args) : Reflect.apply(f, thisArg, args); } catch {}
  };
  for (const t of vals) {
    attempt(t, [], false);
    for (const a of vals) attempt(t, [a], false);
  }
  for (const a of vals) for (const b of [undefined, 0, -1, 'x', {}, 2 ** 31, Infinity]) attempt(o, [a, b], false), attempt(o, [b, a, a], false);
  for (const a of vals) attempt(undefined, [a], true);
  return n;
}
"#;

fn main() {
    let filter = std::env::args().nth(1);
    let mut vm = fos_jsvm::Vm::new();
    vm.print = Box::new(|_| {});
    let list = vm.eval(ENUMERATE).map(|v| vm.display(v)).expect("enumerate");
    let paths: Vec<String> = list.lines().map(String::from).filter(|p| filter.as_ref().is_none_or(|f| p.contains(f.as_str()))).collect();
    println!("{} functions", paths.len());
    std::panic::set_hook(Box::new(|_| {}));
    let mut panics = Vec::new();
    let mut calls = 0u64;
    for path in &paths {
        eprintln!("{path}");
        // A fresh VM per function: a panic may leave one inconsistent, and
        // calls must not see what earlier ones changed
        let mut vm = fos_jsvm::Vm::new();
        vm.print = Box::new(|_| {});
        let r = catch_unwind(AssertUnwindSafe(|| {
            let caller = vm.eval(CALLER).unwrap();
            let p = vm.str_value(path);
            let n = vm.call(caller, fos_jsvm::Value::UNDEFINED, &[p]);
            vm.run_jobs();
            n.ok().and_then(|v| v.as_number()).unwrap_or(0.0)
        }));
        match r {
            Ok(n) => calls += n as u64,
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
