//! ES modules: linking, live bindings, cycles, re-exports, namespaces,
//! top-level await, errors, JSON modules and `import()`
//!
//! Modules come from an in-memory map (`mem:/name.js`); each case reports
//! the global `result` once everything has run.

use std::collections::HashMap;

use fos_jsvm::Vm;

fn resolve(spec: &str, _referrer: &str) -> Result<String, String> {
    match spec.strip_prefix("./") {
        Some(rest) => Ok(format!("mem:/{rest}")),
        None if spec.starts_with("mem:/") => Ok(spec.to_string()),
        None => Err(format!("Failed to resolve module specifier '{spec}'")),
    }
}

fn show(vm: &mut Vm, v: fos_jsvm::value::Value) -> String {
    match v.as_string() {
        Some(s) => format!("'{}'", s.get().to_rust_string()),
        None => vm.display(v),
    }
}

/// Run `main.js` of `files`; returns `result`, or how it failed
fn run(files: &[(&str, &str)]) -> String {
    let mut vm = Vm::new();
    if std::env::var_os("FOS_GC_STRESS").is_some() {
        vm.heap.set_stress(true);
    }
    let map: HashMap<String, String> = files.iter().map(|(n, s)| (format!("mem:/{n}"), s.to_string())).collect();
    let mut fetch = |urls: &[String]| -> Vec<Result<String, String>> {
        urls.iter().map(|u| map.get(u).cloned().ok_or_else(|| format!("404 {u}"))).collect()
    };
    let mut resolve = resolve;
    let main = map.get("mem:/main.js").cloned().unwrap_or_default();
    let root = match vm.compile_module("mem:/main.js", &main, true) {
        Ok(id) => id,
        Err(e) => return format!("throws {}", vm.display(e)),
    };
    if let Err(e) = vm.load_module_graph(root, &mut resolve, &mut fetch) {
        return format!("throws {}", vm.display(e));
    }
    let done = match vm.run_module(root) {
        Ok(p) => p,
        Err(e) => return format!("throws {}", vm.display(e)),
    };
    // Answer `import()` calls until there are none left
    loop {
        vm.run_jobs();
        let calls = vm.take_dynamic_imports();
        if calls.is_empty() {
            break;
        }
        for call in calls {
            let referrer = call.referrer.clone().unwrap_or_else(|| "mem:/main.js".into());
            let loaded = match resolve(&call.specifier, &referrer) {
                Ok(url) => match vm.find_module(&url, false) {
                    Some(id) => Ok(id),
                    None => match fetch(std::slice::from_ref(&url)).remove(0) {
                        Ok(src) => vm.compile_module(&url, &src, true),
                        Err(e) => Err(vm.type_error(&e)),
                    },
                },
                Err(e) => Err(vm.type_error(&e)),
            };
            let loaded = loaded.and_then(|id| vm.load_module_graph(id, &mut resolve, &mut fetch).map(|()| id));
            vm.finish_dynamic_import(call.ticket, loaded);
        }
    }
    match vm.promise_outcome(done) {
        None => return "pending".into(),
        Some((false, e)) => return format!("rejects {}", vm.display(e)),
        Some((true, _)) => {}
    }
    match vm.eval("globalThis.result") {
        Ok(v) => show(&mut vm, v),
        Err(e) => format!("throws {}", vm.display(e)),
    }
}

fn check(cases: &[(&[(&str, &str)], &str)]) {
    let mut failures = Vec::new();
    for (files, want) in cases {
        let got = run(files);
        let ok = if let Some(prefix) = want.strip_suffix('*') { got.starts_with(prefix) } else { got == *want };
        if !ok {
            failures.push(format!("  {:?}\n    want: {want}\n     got: {got}", files.iter().map(|f| f.0).collect::<Vec<_>>()));
        }
    }
    assert!(failures.is_empty(), "{} failure(s):\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn imports_are_live_bindings() {
    check(&[(
        &[
            (
                "main.js",
                "import def, { count, inc, PI as pi } from './counter.js';
                 import * as ns from './counter.js';
                 const before = count; inc(); inc();
                 globalThis.result = [def(), before, count, ns.count, pi, typeof ns, Object.keys(ns).join(), ns[Symbol.toStringTag], Object.isExtensible(ns)].join(' ');",
            ),
            (
                "counter.js",
                "export let count = 0;
                 export function inc() { count++; }
                 export const PI = 3.14;
                 export default function () { return 'dflt'; }",
            ),
        ],
        "'dflt 0 2 2 3.14 object PI,count,default,inc Module false'",
    )]);
}

#[test]
fn re_exports() {
    check(&[(
        &[
            ("main.js", "import { a, b, c, ns2, default as d } from './re.js'; globalThis.result = [a, b, c, ns2.x, d].join();"),
            (
                "re.js",
                "export { a } from './a.js'; export * from './b.js'; export * as ns2 from './x.js';
                 export { y as c } from './x.js'; export default 'D';",
            ),
            ("a.js", "export const a = 'A';"),
            ("b.js", "export const b = 'B'; export default 'not re-exported';"),
            ("x.js", "export const x = 'X', y = 'Y';"),
        ],
        "'A,B,Y,X,D'",
    )]);
    // Re-exporting an import, and names exported twice through `export *`
    check(&[
        (
            &[
                ("main.js", "import { v } from './mid.js'; globalThis.result = v;"),
                ("mid.js", "import { v as w } from './src.js'; export { w as v };"),
                ("src.js", "export const v = 'through';"),
            ],
            "'through'",
        ),
        (
            &[
                ("main.js", "import * as ns from './both.js'; globalThis.result = 'dup' in ns;"),
                ("both.js", "export * from './p.js'; export * from './q.js';"),
                ("p.js", "export const dup = 1;"),
                ("q.js", "export const dup = 2;"),
            ],
            "false",
        ),
        (
            &[
                ("main.js", "import { dup } from './both.js';"),
                ("both.js", "export * from './p.js'; export * from './q.js';"),
                ("p.js", "export const dup = 1;"),
                ("q.js", "export const dup = 2;"),
            ],
            "throws SyntaxError: The requested module 'mem:/both.js' contains conflicting star exports for name 'dup'*",
        ),
    ]);
}

#[test]
fn cycles_and_order() {
    check(&[
        // Function declarations exist before either module runs
        (
            &[
                ("main.js", "import { even } from './even.js'; globalThis.result = [even(10), even(7)].join();"),
                ("even.js", "import { odd } from './odd.js'; export function even(n) { return n === 0 ? true : odd(n - 1); }"),
                ("odd.js", "import { even } from './even.js'; export function odd(n) { return n === 0 ? false : even(n - 1); }"),
            ],
            "'true,false'",
        ),
        // A binding read before its module ran is in its dead zone
        (
            &[
                ("main.js", "import './a.js'; globalThis.result = globalThis.log.join();"),
                ("a.js", "import { b } from './b.js'; export const a = 'a'; globalThis.log.push('a:' + b);"),
                (
                    "b.js",
                    "import { a } from './a.js'; globalThis.log = [];
                     try { a; } catch (e) { log.push(e.constructor.name); }
                     export const b = 'b'; log.push('b');",
                ),
            ],
            "'ReferenceError,b,a:b'",
        ),
        // Dependencies first, each module once
        (
            &[
                ("main.js", "import './x.js'; import './y.js'; globalThis.result = order.join();"),
                ("x.js", "import './shared.js'; order.push('x');"),
                ("y.js", "import './shared.js'; order.push('y');"),
                ("shared.js", "globalThis.order = ['shared'];"),
            ],
            "'shared,x,y'",
        ),
    ]);
}

#[test]
fn top_level_await() {
    check(&[
        (
            &[
                ("main.js", "import { v } from './slow.js'; globalThis.result = 'main sees ' + v;"),
                ("slow.js", "export let v = 1; await Promise.resolve(); v = 2; await null; v = 3;"),
            ],
            "'main sees 3'",
        ),
        (
            &[
                ("main.js", "import './a.js'; import './b.js'; globalThis.result = log.join();"),
                ("a.js", "globalThis.log = ['a1']; await 0; log.push('a2');"),
                ("b.js", "log.push('b');"),
            ],
            "'a1,a2,b'",
        ),
        (&[("main.js", "for await (const x of [Promise.resolve(1), 2]) { globalThis.result = (globalThis.result || 0) + x; }")], "3"),
        (&[("main.js", "await null; throw new Error('late');")], "rejects Error: late"),
    ]);
}

#[test]
fn module_semantics() {
    check(&[
        // Strict, `this` undefined, declarations not global
        (
            &[(
                "main.js",
                "var v = 1; function f() { return this; }
                 globalThis.result = [this === undefined, f() === undefined, 'v' in globalThis, typeof f].join();",
            )],
            "'true,true,false,function'",
        ),
        (&[("main.js", "undeclared = 1;")], "rejects ReferenceError*"),
        (&[("main.js", "globalThis.result = hoisted(); export function hoisted() { return 'hoisted'; }")], "'hoisted'"),
        // Default exports of anonymous functions and classes are named "default"
        (
            &[
                ("main.js", "import f from './f.js'; import C from './c.js'; import g from './g.js'; globalThis.result = [f.name, C.name, g.name].join();"),
                ("f.js", "export default function () {}"),
                ("c.js", "export default class {}"),
                ("g.js", "export default () => 1;"),
            ],
            "'default,default,default'",
        ),
        (&[("main.js", "globalThis.result = import.meta.url;")], "'mem:/main.js'"),
        (
            &[
                ("main.js", "import { where } from './w.js'; globalThis.result = where();"),
                ("w.js", "export function where() { return import.meta.url; }"),
            ],
            "'mem:/w.js'",
        ),
        (
            &[
                ("main.js", "import data from './d.json' with { type: 'json' }; globalThis.result = data.list[1] + data.name;"),
                ("d.json", r#"{"list": [1, 2], "name": "x"}"#),
            ],
            "'2x'",
        ),
    ]);
}

#[test]
fn errors() {
    check(&[
        (
            &[("main.js", "import { nope } from './a.js';"), ("a.js", "export const a = 1;")],
            "throws SyntaxError: The requested module 'mem:/a.js' does not provide an export named 'nope'*",
        ),
        (&[("main.js", "import './bad.js'; globalThis.result = 'ran';"), ("bad.js", "throw new Error('boom');")], "rejects Error: boom"),
        (&[("main.js", "import './broken.js';"), ("broken.js", "export const = 1;")], "throws SyntaxError*"),
        (&[("main.js", "import './missing.js';")], "throws TypeError: Failed to load module mem:/missing.js*"),
        (&[("main.js", "import 'bare';")], "throws TypeError: Failed to resolve module specifier 'bare'"),
        (&[("main.js", "import { a } from './a.js'; a = 2;"), ("a.js", "export let a = 1;")], "rejects TypeError: Assignment to constant variable."),
        (&[("main.js", "export const x = 1; export { x };")], "throws SyntaxError: Duplicate export of 'x'*"),
        (&[("main.js", "export { nothing };")], "throws SyntaxError: Export 'nothing' is not defined in module*"),
        (&[("main.js", "let x; var x;")], "throws SyntaxError*"),
        (&[("main.js", "import { a } from './a.js'; let a;"), ("a.js", "export const a = 1;")], "throws SyntaxError*"),
        (&[("main.js", "var await = 1;")], "throws SyntaxError*"),
    ]);
}

#[test]
fn dynamic_import() {
    check(&[
        (
            &[
                ("main.js", "import('./lazy.js').then(ns => { globalThis.result = ns.default + ns.n; });"),
                ("lazy.js", "export default 'lazy'; export const n = 1;"),
            ],
            "'lazy1'",
        ),
        // The same module instance as a static import
        (
            &[
                ("main.js", "import * as a from './m.js'; import('./m.js').then(b => { globalThis.result = a === b; });"),
                ("m.js", "export const x = 1;"),
            ],
            "true",
        ),
        (
            &[("main.js", "import('./missing.js').catch(e => { globalThis.result = e.constructor.name; });")],
            "'TypeError'",
        ),
        (
            &[
                ("main.js", "const ns = await import('./tla.js'); globalThis.result = ns.v;"),
                ("tla.js", "export let v = 'early'; await null; v = 'after await';"),
            ],
            "'after await'",
        ),
        (
            &[
                ("main.js", "import('./throws.js').then(() => {}, e => { globalThis.result = e.message; });"),
                ("throws.js", "throw new Error('nope');"),
            ],
            "'nope'",
        ),
    ]);
}

#[test]
fn dynamic_import_from_scripts() {
    // Scripts compile `import()` and leave the loading to the embedder
    let mut vm = Vm::new();
    let v = vm.eval("var p = import('./x.js'); p instanceof Promise").unwrap();
    assert_eq!(vm.display(v), "true");
    let calls = vm.take_dynamic_imports();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].specifier, "./x.js");
    assert_eq!(calls[0].referrer, None);
    let id = vm.compile_module("mem:/x.js", "export const answer = 42;", true).unwrap();
    vm.finish_dynamic_import(calls[0].ticket, Ok(id));
    let v = vm.eval("var out; p.then(ns => out = ns.answer); out").unwrap();
    assert_eq!(vm.display(v), "undefined");
    vm.run_jobs();
    let v = vm.eval("out").unwrap();
    assert_eq!(vm.display(v), "42");
}

#[test]
fn decorated_classes() {
    check(&[(
        &[(
            "main.js",
            "const tag = (name) => (cls) => { cls.tagName = name; return cls; };
             class R { on() { return 'on'; } }
             R = tag('toggle-switch')(R);
             export { R };
             globalThis.result = R.tagName + ' ' + new R().on();",
        )],
        "'toggle-switch on'",
    )]);
}
