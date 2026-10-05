//! End-to-end language and built-in behavior
//!
//! Each case evaluates a script and compares the completion value (as
//! `console.log` would print it, strings quoted) or the thrown error.

use fos_jsvm::Vm;

/// Run a script, then its promise jobs, returning the value of the global
/// `result` if the script defines one (for asynchronous tests)
fn run_async(src: &str) -> String {
    let mut vm = Vm::new();
    if std::env::var_os("FOS_GC_STRESS").is_some() {
        vm.heap.set_stress(true);
    }
    if let Err(e) = vm.eval(src) {
        return format!("throws {}", vm.display(e));
    }
    vm.run_jobs();
    match vm.eval("result") {
        Ok(v) => {
            if let Some(s) = v.as_string() {
                format!("'{}'", s.get().to_rust_string())
            } else {
                vm.display(v)
            }
        }
        Err(e) => format!("throws {}", vm.display(e)),
    }
}

fn check_async(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (src, want) in cases {
        let got = run_async(src);
        let ok = if let Some(prefix) = want.strip_suffix('*') { got.starts_with(prefix) } else { got == *want };
        if !ok {
            failures.push(format!("  {src}\n    want: {want}\n     got: {got}"));
        }
    }
    assert!(failures.is_empty(), "{} failure(s):\n{}", failures.len(), failures.join("\n"));
}

fn run(src: &str) -> String {
    let mut vm = Vm::new();
    // FOS_GC_STRESS=1: collect at every safepoint
    if std::env::var_os("FOS_GC_STRESS").is_some() {
        vm.heap.set_stress(true);
    }
    match vm.eval(src) {
        Ok(v) => {
            if let Some(s) = v.as_string() {
                format!("'{}'", s.get().to_rust_string())
            } else {
                vm.display(v)
            }
        }
        Err(e) => format!("throws {}", vm.display(e)),
    }
}

fn check(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for (src, want) in cases {
        let got = run(src);
        let ok = if let Some(prefix) = want.strip_suffix('*') { got.starts_with(prefix) } else { got == *want };
        if !ok {
            failures.push(format!("  {src}\n    want: {want}\n     got: {got}"));
        }
    }
    assert!(failures.is_empty(), "{} failure(s):\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn operators_and_conversions() {
    check(&[
        ("1 + 2 * 3", "7"),
        ("(1 + 2) * 3", "9"),
        ("2 ** 10", "1024"),
        ("2 ** -1", "0.5"),
        ("7 % 3", "1"),
        ("-7 % 3", "-1"),
        ("-4 % 2", "-0"),
        ("5.5 % 2", "1.5"),
        ("1 / 0", "Infinity"),
        ("-1 / 0", "-Infinity"),
        ("0 / 0", "NaN"),
        ("2147483647 + 1", "2147483648"),
        ("-2147483648 - 1", "-2147483649"),
        ("65536 * 65536", "4294967296"),
        ("-0 * 5", "-0"),
        ("0 * -5", "-0"),
        ("1 << 31", "-2147483648"),
        ("-1 >>> 0", "4294967295"),
        ("-16 >> 2", "-4"),
        ("5 & 3 | 8 ^ 1", "9"),
        ("~5", "-6"),
        ("'3' * '4'", "12"),
        ("'3' + 4", "'34'"),
        ("3 + '4'", "'34'"),
        ("1 + true", "2"),
        ("1 + null", "1"),
        ("1 + undefined", "NaN"),
        ("'a' + null", "'anull'"),
        ("[1,2] + [3]", "'1,23'"),
        ("({}) + ''", "'[object Object]'"),
        ("+''", "0"),
        ("+' 42 '", "42"),
        ("+'0x1f'", "31"),
        ("+'1e3'", "1000"),
        ("+'abc'", "NaN"),
        ("+[]", "0"),
        ("+[5]", "5"),
        ("+{}", "NaN"),
        ("'10' - '4'", "6"),
        ("'b' > 'a'", "true"),
        ("'10' < '9'", "true"),
        ("10 < '9'", "false"),
        ("null >= 0", "true"),
        ("undefined == null", "true"),
        ("undefined === null", "false"),
        ("NaN == NaN", "false"),
        ("NaN !== NaN", "true"),
        ("'1' == 1", "true"),
        ("0 == ''", "true"),
        ("0 == '0'", "true"),
        ("false == 'false'", "false"),
        ("[] == false", "true"),
        ("null == 0", "false"),
        ("({valueOf() { return 42 }}) == 42", "true"),
        ("typeof 1", "'number'"),
        ("typeof 'x'", "'string'"),
        ("typeof undefined", "'undefined'"),
        ("typeof null", "'object'"),
        ("typeof {}", "'object'"),
        ("typeof function(){}", "'function'"),
        ("typeof class {}", "'function'"),
        ("typeof Symbol()", "'symbol'"),
        ("typeof notDefinedAnywhere", "'undefined'"),
        ("!!''", "false"),
        ("!!'0'", "true"),
        ("!!NaN", "false"),
        ("!![]", "true"),
        ("null ?? 'd'", "'d'"),
        ("0 ?? 'd'", "0"),
        ("0 || 'd'", "'d'"),
        ("1 && 2", "2"),
        ("void 5", "undefined"),
        ("(1, 2, 3)", "3"),
        ("let x = 5; x++ + ++x", "12"),
        ("let x = 5; x-- - --x", "2"),
        ("let s = '5'; s++; s", "6"),
        ("let a = [1]; a[0]++; a[0]", "2"),
        ("let o = {n: 1}; o.n += 5; o.n", "6"),
        ("let o = {}; o.x ??= 3; o.x ||= 4; o.x &&= 7; o.x", "7"),
        ("let x = 1; x += x += 2; x", "4"),
        ("let a = 1; a + (a = 5)", "6"),
        ("'abc' < 'abd'", "true"),
        ("1 + 2 + '3'", "'33'"),
        ("0.1 + 0.2", "0.30000000000000004"),
        ("1e21", "1e+21"),
        ("123456789012345680000", "123456789012345680000"),
        ("0.000001", "0.000001"),
        ("1e-7", "1e-7"),
        ("(255).toString(16)", "'ff'"),
        ("(0.5).toString(2)", "'0.1'"),
        ("(-255).toString(36)", "'-73'"),
        ("(1.005).toFixed(2)", "'1.00'"),
        ("(1.45).toFixed(1)", "'1.4'"),
        ("(0.5).toFixed(0)", "'1'"),
        ("(2.5).toFixed(0)", "'3'"),
        ("(1234.5678).toFixed(2)", "'1234.57'"),
        ("(123.456).toPrecision(4)", "'123.5'"),
        ("(0.00001).toPrecision(1)", "'0.00001'"),
        ("(1e21).toPrecision(3)", "'1.00e+21'"),
        ("(12345).toExponential(2)", "'1.23e+4'"),
        ("parseInt('42px')", "42"),
        ("parseInt('0x1A')", "26"),
        ("parseInt('101', 2)", "5"),
        ("parseInt('  -12')", "-12"),
        ("parseInt('')", "NaN"),
        ("parseFloat('3.14abc')", "3.14"),
        ("parseFloat('.5')", "0.5"),
        ("parseFloat('-Infinityx')", "-Infinity"),
        ("Number('')", "0"),
        ("Number('12abc')", "NaN"),
        ("Number(null)", "0"),
        ("Number(undefined)", "NaN"),
        ("Number(true)", "1"),
        ("String(null)", "'null'"),
        ("String([1,[2,3]])", "'1,2,3'"),
        ("String(Symbol('s'))", "'Symbol(s)'"),
        ("`${1}${'a'}${null}`", "'1anull'"),
        ("Object.is(-0, 0)", "false"),
        ("Object.is(NaN, NaN)", "true"),
        ("isNaN('abc')", "true"),
        ("Number.isNaN('abc')", "false"),
        ("Number.isInteger(5.0)", "true"),
        ("Number.isSafeInteger(2**53)", "false"),
        ("Number.MAX_SAFE_INTEGER", "9007199254740991"),
        ("1 in [5, 6]", "true"),
        ("'x' in {x: undefined}", "true"),
        ("[] instanceof Array", "true"),
        ("[] instanceof Object", "true"),
        ("({}) instanceof Array", "false"),
        ("5 instanceof Number", "false"),
    ]);
}

#[test]
fn variables_and_scope() {
    check(&[
        ("var a = 1; { var a = 2; } a", "2"),
        ("let a = 1; { let a = 2; } a", "1"),
        ("const c = 1; c = 2", "throws TypeError: Assignment to constant variable."),
        ("{ x; let x = 1; }", "throws ReferenceError: Cannot access 'x' before initialization"),
        ("function f() { return y; } let y = 3; f()", "3"),
        ("function f() { return z; } f(); let z = 1;", "throws ReferenceError: Cannot access 'z' before initialization"),
        ("undeclaredVar", "throws ReferenceError: undeclaredVar is not defined"),
        ("'use strict'; undeclared2 = 1", "throws ReferenceError: undeclared2 is not defined"),
        ("sloppyGlobal = 7; globalThis.sloppyGlobal", "7"),
        ("var g1 = 5; globalThis.g1", "5"),
        ("let l1 = 5; globalThis.l1", "undefined"),
        ("hoisted(); function hoisted() { return 'ok' }", "'ok'"),
        ("typeof hoistedVar; var hoistedVar = 1; typeof hoistedVar", "'number'"),
        ("function f() { var v = 1; if (true) { var v = 2; } return v; } f()", "2"),
        ("function f() { let v = 1; if (true) { let v = 2; } return v; } f()", "1"),
        ("var fs = []; for (var i = 0; i < 3; i++) fs.push(() => i); fs.map(f => f()).join()", "'3,3,3'"),
        ("var fs = []; for (let i = 0; i < 3; i++) fs.push(() => i); fs.map(f => f()).join()", "'0,1,2'"),
        ("var fs = []; for (let i = 0; i < 3; i++) { let j = i * 2; fs.push(() => j); } fs.map(f => f()).join()", "'0,2,4'"),
        ("var fs = []; for (const x of [1,2,3]) fs.push(() => x); fs.map(f => f()).join()", "'1,2,3'"),
        ("var fs = []; for (let k in {a:1, b:2}) fs.push(() => k); fs.map(f => f()).join()", "'a,b'"),
        ("let fs = []; for (let i = 0; i < 3; i++) { fs.push(() => i); i++; } fs.map(f => f()).join()", "'1,3'"),
        ("function counter() { let n = 0; return { inc: () => ++n, get: () => n }; } let c = counter(); c.inc(); c.inc(); c.get()", "2"),
        ("function outer() { let x = 1; function mid() { function inner() { return x; } return inner; } x = 5; return mid()(); } outer()", "5"),
        ("let x = 'global'; function f() { return typeof x; } f()", "'string'"),
        ("(function() { 'use strict'; return this; })()", "undefined"),
        ("(function() { return this === globalThis; })()", "true"),
        ("var o = { f() { return () => this.v; }, v: 9 }; o.f()()", "9"),
        ("let f = function g() { return typeof g; }; f()", "'function'"),
        ("let f = function g() { g = 1; return typeof g; }; f()", "'function'"),
        ("let f = function g(g) { return g; }; f(3)", "3"),
        ("function a(x, x) { return x; } a(1, 2)", "2"),
        ("function f(x) { var x; return x; } f(4)", "4"),
        ("function f(x) { var x = 2; return x; } f(4)", "2"),
        ("{ function blockFn() { return 1; } } blockFn()", "1"),
        ("if (true) { function cond() { return 'yes'; } } cond()", "'yes'"),
        ("let x = 1; switch (1) { case 1: let x = 2; } x", "1"),
        ("switch (2) { case 1: let y = 1; break; case 2: y; }", "throws ReferenceError: Cannot access 'y' before initialization"),
        ("try { throw 1 } catch { } 'ok'", "'ok'"),
        ("let e = 'outer'; try { throw 'inner' } catch (e) { } e", "'outer'"),
        ("var r; try { throw {a: 1, b: 2} } catch ({a, b}) { r = a + b } r", "3"),
        ("let x = 0; { let x = 1; { let x = 2; } } x", "0"),
        ("const {a, ...rest} = {a: 1, b: 2, c: 3}; JSON.stringify(rest)", "'{\"b\":2,\"c\":3}'"),
        ("let q = 10; function shadow(q) { q = 5; return q; } shadow(1) + q", "15"),
        ("var i = 0; while (true) { if (i++ > 5) break; } i", "7"),
        ("eval('1 + 1')", "2"),
        ("new Function('a', 'b', 'return a * b')(6, 7)", "42"),
    ]);
}

#[test]
fn functions() {
    check(&[
        ("function f(a, b = a + 1) { return a + b; } f(1)", "3"),
        ("function f(a, b = 2) { return b; } f(1, undefined)", "2"),
        ("function f(a, b = 2) { return b; } f(1, null)", "null"),
        ("function f(...r) { return r.length; } f(1, 2, 3)", "3"),
        ("function f(a, ...r) { return r; } f(1, 2, 3)", "[ 2, 3 ]"),
        ("function f() { return arguments.length; } f(1, 2, 3)", "3"),
        ("function f(a) { return arguments[1]; } f(1, 'two')", "'two'"),
        ("function f() { return Array.prototype.slice.call(arguments, 1); } f(1,2,3)", "[ 2, 3 ]"),
        ("function f() { return [...arguments]; } f(4,5)", "[ 4, 5 ]"),
        ("function f() { return () => arguments[0]; } f('outer')()", "'outer'"),
        ("function f({a, b: [c]}) { return a + c; } f({a: 1, b: [2]})", "3"),
        ("function f([x, y] = [1, 2]) { return x * y; } f()", "2"),
        ("Math.max(...[1, 5, 3])", "5"),
        ("function f(a, b, c) { return a + b + c; } f(...[1, 2], 3)", "6"),
        ("f.length; function f(a, b = 1, c) {}", "1"),
        ("(function(a, b) {}).length", "2"),
        ("(function namedFn() {}).name", "'namedFn'"),
        ("let anon = function() {}; anon.name", "'anon'"),
        ("let arrow = () => {}; arrow.name", "'arrow'"),
        ("({ method() {} }).method.name", "'method'"),
        ("function f() {} f.prototype.constructor === f", "true"),
        ("(() => {}).prototype", "undefined"),
        ("new (() => {})", "throws TypeError: function is not a constructor"),
        ("function F() { this.a = 1; } new F().a", "1"),
        ("function F() { return {b: 2}; } new F().b", "2"),
        ("function F() { return 3; } typeof new F()", "'object'"),
        ("function F() { if (!new.target) return 'call'; return this; } F()", "'call'"),
        ("function add(a, b) { return a + b; } add.call(null, 1, 2)", "3"),
        ("function add(a, b) { return a + b; } add.apply(null, [3, 4])", "7"),
        ("function mul(a, b) { return a * b; } let d = mul.bind(null, 2); d(5)", "10"),
        ("function who() { return this.n; } who.bind({n: 'bound'})()", "'bound'"),
        ("function F(x) { this.x = x; } let B = F.bind(null, 7); new B().x", "7"),
        ("function f() {} f.bind().name", "'bound f'"),
        ("function fact(n) { return n <= 1 ? 1 : n * fact(n - 1); } fact(10)", "3628800"),
        ("function deep(n) { return n === 0 ? 0 : 1 + deep(n - 1); } deep(10000)", "10000"),
        ("function inf() { return inf(); } inf()", "throws RangeError: Maximum call stack size exceeded"),
        ("let o = { n: 1, get double() { return this.n * 2; }, set val(v) { this.n = v; } }; o.val = 5; o.double", "10"),
        ("(function() { return typeof arguments; })()", "'object'"),
        ("var self = this; (function() { return this; }).call(5) instanceof Number", "true"),
        ("(function() { 'use strict'; return this; }).call(5)", "5"),
        ("let f = (a, b) => ({ sum: a + b }); f(1, 2).sum", "3"),
        ("let curry = a => b => c => a + b + c; curry(1)(2)(3)", "6"),
        ("[1,2,3].map((x) => x * 2)", "[ 2, 4, 6 ]"),
        ("(function(){ return typeof this; }).call('s')", "'object'"),
        ("function f(a = () => b, b = 2) { return a(); } f()", "2"),
        ("Function.prototype.toString.call(Math.max)", "'function max() { [native code] }'"),
    ]);
}

#[test]
fn objects_and_properties() {
    check(&[
        ("let o = {a: 1, 'b-c': 2, 3: 'three'}; o['b-c'] + o[3]", "'2three'"),
        ("let k = 'dyn'; let o = {[k + 1]: 5}; o.dyn1", "5"),
        ("let x = 1, y = 2; let o = {x, y}; o.x + o.y", "3"),
        ("Object.keys({b: 1, a: 2, 1: 3, 0: 4})", "[ '0', '1', 'b', 'a' ]"),
        ("let o = {}; o.z = 1; o.y = 2; o[5] = 3; Object.keys(o)", "[ '5', 'z', 'y' ]"),
        ("let o = {a: 1}; delete o.a; 'a' in o", "false"),
        ("let o = {a: 1, b: 2}; delete o.a; JSON.stringify(o)", "'{\"b\":2}'"),
        ("({...{a: 1}, b: 2, ...{a: 3}})", "{ a: 3, b: 2 }"),
        ("let {a, b: {c = 5} = {}} = {a: 1}; a + c", "6"),
        ("let [p, , q = 9, ...r] = [1, 2, undefined, 4, 5]; [p, q, r]", "[ 1, 9, [ 4, 5 ] ]"),
        ("let a = 1, b = 2; [a, b] = [b, a]; [a, b]", "[ 2, 1 ]"),
        ("let o = {}; [o.x, o['y']] = [1, 2]; o.x + o.y", "3"),
        ("let {length} = 'hello'; length", "5"),
        ("let {0: first} = ['f']; first", "'f'"),
        ("const {x} = null", "throws TypeError*"),
        ("Object.entries({a: 1, b: 'x'})", "[ [ 'a', 1 ], [ 'b', 'x' ] ]"),
        ("Object.values({a: 1, b: 2})", "[ 1, 2 ]"),
        ("Object.assign({a: 1}, {b: 2}, null, {c: 3})", "{ a: 1, b: 2, c: 3 }"),
        ("Object.fromEntries([['a', 1], ['b', 2]])", "{ a: 1, b: 2 }"),
        ("let p = {greet() { return 'hi ' + this.name; }}; let o = Object.create(p); o.name = 'x'; o.greet()", "'hi x'"),
        ("Object.getPrototypeOf(Object.create(null))", "null"),
        ("let o = Object.create(null); o.x = 1; 'toString' in o", "false"),
        ("let o = {}; Object.defineProperty(o, 'x', {value: 1}); o.x = 2; o.x", "1"),
        ("'use strict'; let o = {}; Object.defineProperty(o, 'x', {value: 1}); o.x = 2", "throws TypeError*"),
        ("let o = {}; Object.defineProperty(o, 'x', {value: 1}); Object.keys(o).length", "0"),
        ("let o = {}; Object.defineProperty(o, 'x', {get() { return 42; }, enumerable: true}); o.x", "42"),
        ("let o = {v: 1}; Object.defineProperty(o, 'v', {writable: false}); o.v = 9; o.v", "1"),
        ("Object.getOwnPropertyDescriptor({a: 1}, 'a')", "{ value: 1, writable: true, enumerable: true, configurable: true }"),
        ("let o = Object.freeze({a: 1}); o.a = 2; o.b = 3; [o.a, o.b, Object.isFrozen(o)]", "[ 1, undefined, true ]"),
        ("'use strict'; Object.freeze({a: 1}).a = 2", "throws TypeError*"),
        ("let o = Object.seal({a: 1}); o.a = 2; delete o.a; o.a", "2"),
        ("let a = Object.freeze([1, 2]); a[0] = 9; a[0]", "1"),
        ("let o = {}; Object.preventExtensions(o); o.x = 1; o.x", "undefined"),
        ("Object.getOwnPropertyNames('ab')", "[ '0', '1', 'length' ]"),
        ("Object.getOwnPropertyNames([1])", "[ '0', 'length' ]"),
        ("({}).hasOwnProperty('toString')", "false"),
        ("Object.hasOwn({q: 1}, 'q')", "true"),
        ("Object.prototype.toString.call([])", "'[object Array]'"),
        ("Object.prototype.toString.call(null)", "'[object Null]'"),
        ("String({[Symbol.toStringTag]: 'Custom'})", "'[object Custom]'"),
        ("let o = {toString() { return 'custom'; }}; `${o}`", "'custom'"),
        ("let o = {valueOf() { return 10; }}; o * 2", "20"),
        ("let o = {[Symbol.toPrimitive](h) { return h; }}; `${o}` + (o + '')", "'stringdefault'"),
        ("let o = {a: {b: {c: 1}}}; o?.a?.b?.c", "1"),
        ("let o = null; o?.a.b.c", "undefined"),
        ("let o = {}; o.a?.b", "undefined"),
        ("let o = {f() { return 1; }}; o.f?.() + (o.g?.() ?? 10)", "11"),
        ("let o; o?.[1]", "undefined"),
        ("let o = {a: 1}; o.__proto__ === Object.prototype", "true"),
        ("let o = {__proto__: {inh: 5}}; o.inh", "5"),
        ("let o = {}; o.__proto__ = Array.prototype; o instanceof Array", "true"),
        ("let proto = {x: 1}; let o = Object.create(proto); proto.x = 2; o.x", "2"),
        ("let o = {}; for (let i = 0; i < 100; i++) o['k' + i] = i; o.k99 + Object.keys(o).length", "199"),
        ("let o = {}; for (let i = 0; i < 100; i++) o['k' + i] = i; for (let i = 0; i < 50; i++) delete o['k' + i]; Object.keys(o)[0]", "'k50'"),
        ("let keys = []; for (let k in {a: 1, b: 2}) keys.push(k); keys", "[ 'a', 'b' ]"),
        ("let p = {inherited: 1}; let o = Object.create(p); o.own = 2; let ks = []; for (let k in o) ks.push(k); ks", "[ 'own', 'inherited' ]"),
        ("let ks = []; for (let k in [7, 8]) ks.push(typeof k); ks", "[ 'string', 'string' ]"),
        ("let ks = []; for (let k in 'hi') ks.push(k); ks", "[ '0', '1' ]"),
        ("for (let k in null) throw 1; 'ok'", "'ok'"),
        ("let o = {a: 1}; let n = 0; for (const k in o) { o.b = 2; n++; } n", "1"),
        ("Reflect.ownKeys({a: 1, [Symbol.iterator]: 2})", "[ 'a', Symbol(Symbol.iterator) ]"),
        ("Reflect.has({a: 1}, 'a')", "true"),
        ("Reflect.getPrototypeOf([]) === Array.prototype", "true"),
        ("let o = {}; Reflect.defineProperty(o, 'x', {value: 3}); o.x", "3"),
        ("Reflect.apply(Math.max, null, [1, 3, 2])", "3"),
        ("class A { constructor(x) { this.x = x; } } Reflect.construct(A, [5]).x", "5"),
        ("let s = Symbol('k'); let o = {[s]: 1}; o[s]", "1"),
        ("let s = Symbol('k'); Object.keys({[s]: 1}).length", "0"),
        ("Object.getOwnPropertySymbols({[Symbol.iterator]: 1}).length", "1"),
        ("Symbol('x').toString()", "'Symbol(x)'"),
        ("Symbol('x').description", "'x'"),
        ("Symbol.for('a') === Symbol.for('a')", "true"),
        ("Symbol.keyFor(Symbol.for('reg'))", "'reg'"),
        ("Symbol() + ''", "throws TypeError: Cannot convert a Symbol value to a string"),
        ("null.x", "throws TypeError: Cannot read properties of null (reading 'x')"),
        ("undefined.x = 1", "throws TypeError: Cannot set properties of undefined (setting 'x')"),
        ("let o = {}; o.f()", "throws TypeError: o.f is not a function"),
        ("function local() { let o = {}; return o.f(); } local()", "throws TypeError: f is not a function"),
        ("var g = {a: {}}; g.a.b(1, 2)", "throws TypeError: g.a.b is not a function"),
        ("var h = 5; h()", "throws TypeError: h is not a function"),
        ("var k = {}; k.m.n()", "throws TypeError: Cannot read properties of undefined (reading 'n')*"),
        ("(1, 2)()", "throws TypeError: 2 is not a function"),
        ("'use strict'; 'str'.x = 1", "throws TypeError*"),
        ("'str'.x = 1; 'ok'", "'ok'"),
    ]);
}

#[test]
fn arrays() {
    check(&[
        ("[1, 2, 3].length", "3"),
        ("let a = [1, 2, 3]; a.length = 1; a", "[ 1 ]"),
        ("let a = []; a[5] = 1; a.length", "6"),
        ("let a = [1, , 3]; 1 in a", "false"),
        ("[1, , 3]", "[ 1, <empty>, 3 ]"),
        ("new Array(3).length", "3"),
        ("new Array(3, 4)", "[ 3, 4 ]"),
        ("Array(2).fill(0)", "[ 0, 0 ]"),
        ("new Array(-1)", "throws RangeError: Invalid array length"),
        ("Array.of(7)", "[ 7 ]"),
        ("Array.from('abc')", "[ 'a', 'b', 'c' ]"),
        ("Array.from({length: 3}, (_, i) => i * i)", "[ 0, 1, 4 ]"),
        ("Array.from(new Set([1, 1, 2]))", "[ 1, 2 ]"),
        ("Array.isArray([])", "true"),
        ("let a = [1]; a.push(2, 3); a", "[ 1, 2, 3 ]"),
        ("[1, 2, 3].pop()", "3"),
        ("let a = [1, 2, 3]; a.shift(); a", "[ 2, 3 ]"),
        ("let a = [3]; a.unshift(1, 2); a", "[ 1, 2, 3 ]"),
        ("[1, 2, 3, 4].slice(1, -1)", "[ 2, 3 ]"),
        ("let a = [1, 2, 3, 4]; let r = a.splice(1, 2, 'x'); [a, r]", "[ [ 1, 'x', 4 ], [ 2, 3 ] ]"),
        ("let a = [1, 2]; a.splice(1, 0, 9, 8); a", "[ 1, 9, 8, 2 ]"),
        ("[1, 2].concat([3], 4, [[5]])", "[ 1, 2, 3, 4, [ 5 ] ]"),
        ("[1, 2, 3].join('-')", "'1-2-3'"),
        ("[null, undefined, 1].join()", "',,1'"),
        ("[1, 2, 3].reverse()", "[ 3, 2, 1 ]"),
        ("[1, 2, 3, 2].indexOf(2)", "1"),
        ("[1, 2, 3, 2].lastIndexOf(2)", "3"),
        ("[NaN].indexOf(NaN)", "-1"),
        ("[NaN].includes(NaN)", "true"),
        ("[1, 2, 3].find(x => x > 1)", "2"),
        ("[1, 2, 3].findIndex(x => x > 5)", "-1"),
        ("[1, 2, 3].findLast(x => x < 3)", "2"),
        ("[1, 2, 3, 4].filter(x => x % 2)", "[ 1, 3 ]"),
        ("[1, 2, 3].map((x, i) => x * i)", "[ 0, 2, 6 ]"),
        ("let s = 0; [1, 2, 3].forEach(x => s += x); s", "6"),
        ("[1, 2, 3].some(x => x > 2)", "true"),
        ("[1, 2, 3].every(x => x > 2)", "false"),
        ("[1, 2, 3].reduce((a, b) => a + b)", "6"),
        ("[1, 2, 3].reduce((a, b) => a + b, 10)", "16"),
        ("['a', 'b'].reduceRight((a, b) => a + b)", "'ba'"),
        ("[].reduce((a, b) => a)", "throws TypeError: Reduce of empty array with no initial value"),
        ("[3, 1, 10, 2].sort()", "[ 1, 10, 2, 3 ]"),
        ("[3, 1, 10, 2].sort((a, b) => a - b)", "[ 1, 2, 3, 10 ]"),
        ("['b', undefined, 'a'].sort()", "[ 'a', 'b', undefined ]"),
        ("let a = []; for (let i = 0; i < 100; i++) a.push({k: i % 3, i}); a.sort((x, y) => x.k - y.k); a.slice(0, 3).map(o => o.i)", "[ 0, 3, 6 ]"),
        ("[1, [2, [3, [4]]]].flat()", "[ 1, 2, [ 3, [ 4 ] ] ]"),
        ("[1, [2, [3, [4]]]].flat(Infinity)", "[ 1, 2, 3, 4 ]"),
        ("[1, 2].flatMap(x => [x, x * 10])", "[ 1, 10, 2, 20 ]"),
        ("[1, 2, 3].at(-1)", "3"),
        ("[1, 2, 3, 4, 5].copyWithin(0, 3)", "[ 4, 5, 3, 4, 5 ]"),
        ("[1, 2, 3].fill(0, 1)", "[ 1, 0, 0 ]"),
        ("[...[1, 2].keys()]", "[ 0, 1 ]"),
        ("[...['a'].entries()]", "[ [ 0, 'a' ] ]"),
        ("[3, 1, 2].toSorted()", "[ 1, 2, 3 ]"),
        ("[1, 2, 3].toReversed()", "[ 3, 2, 1 ]"),
        ("[1, 2, 3].with(1, 9)", "[ 1, 9, 3 ]"),
        ("String([1, [2, 3]])", "'1,2,3'"),
        ("let a = [1, 2]; a.x = 'prop'; a.length", "2"),
        ("let a = []; a[4294967295] = 1; a.length", "0"),
        ("let a = [1, 2, 3]; delete a[1]; a", "[ 1, <empty>, 3 ]"),
        ("let a = []; a[1000000] = 1; a.length", "1000001"),
        ("let a = [5, 6]; a['1']", "6"),
        ("let a = [5, 6]; a[1.0]", "6"),
        ("let a = [5]; a[-1] = 2; a.length + a[-1]", "3"),
        ("Array.prototype.map.call('ab', c => c + c)", "[ 'aa', 'bb' ]"),
        ("let al = {length: 2, 0: 'a', 1: 'b'}; Array.prototype.join.call(al, '+')", "'a+b'"),
        ("let a = [1, 2, 3]; let out = []; for (const x of a) { if (x === 2) a.push(4); out.push(x); } out", "[ 1, 2, 3, 4 ]"),
        ("let [a, b] = new Map([[1, 2]]); a", "[ 1, 2 ]"),
        ("[...'hi'].length", "2"),
        ("let a = []; for (let i = 0; i < 1000; i++) a.push(i); a.indexOf(999)", "999"),
        ("class MyArr extends Array {} let m = new MyArr(); m.push(1); [m.length, m instanceof MyArr, Array.isArray(m)]", "[ 1, true, true ]"),
    ]);
}

#[test]
fn strings() {
    check(&[
        ("'hello'.length", "5"),
        ("'hello'[1]", "'e'"),
        ("'hello'.charAt(4)", "'o'"),
        ("'hello'.charCodeAt(0)", "104"),
        ("'😀'.length", "2"),
        ("'😀'.codePointAt(0)", "128512"),
        ("String.fromCodePoint(128512) === '😀'", "true"),
        ("String.fromCharCode(72, 105)", "'Hi'"),
        ("'abc'.at(-1)", "'c'"),
        ("'hello world'.indexOf('o')", "4"),
        ("'hello world'.lastIndexOf('o')", "7"),
        ("'hello'.includes('ell')", "true"),
        ("'hello'.startsWith('he')", "true"),
        ("'hello'.endsWith('lo')", "true"),
        ("'hello'.slice(1, -1)", "'ell'"),
        ("'hello'.substring(3, 1)", "'el'"),
        ("'hello'.substr(-3, 2)", "'ll'"),
        ("'Hello'.toUpperCase() + 'ÄB'.toLowerCase()", "'HELLOäb'"),
        ("'  x  '.trim() + '|'", "'x|'"),
        ("'  x  '.trimStart()", "'x  '"),
        ("'5'.padStart(3, '0')", "'005'"),
        ("'ab'.padEnd(5, 'xy')", "'abxyx'"),
        ("'ab'.repeat(3)", "'ababab'"),
        ("'a,b,,c'.split(',')", "[ 'a', 'b', '', 'c' ]"),
        ("'abc'.split('')", "[ 'a', 'b', 'c' ]"),
        ("'a b c'.split(' ', 2)", "[ 'a', 'b' ]"),
        ("'abc'.split()", "[ 'abc' ]"),
        ("'aXbXc'.replace('X', '-')", "'a-bXc'"),
        ("'aXbXc'.replaceAll('X', '-')", "'a-b-c'"),
        ("'abc'.replace('b', '[$&]')", "'a[b]c'"),
        ("'abc'.replace('b', (m, i) => m.toUpperCase() + i)", "'aB1c'"),
        ("'abc'.concat(1, 2)", "'abc12'"),
        ("'a'.localeCompare('b')", "-1"),
        ("[...'a😀b'].length", "3"),
        ("let s = ''; for (const c of 'héllo') s = c + s; s", "'olléh'"),
        ("'abc' === 'ab' + 'c'", "true"),
        ("let s = ''; for (let i = 0; i < 1000; i++) s += 'x'; s.length", "1000"),
        ("let s = ''; for (let i = 0; i < 100; i++) s += i; s.length", "190"),
        ("let a = 'x'.repeat(100), b = 'x'.repeat(100); a === b", "true"),
        ("`line1\nline2`.split('\\n').length", "2"),
        ("`a${1 + 1}b${'c'}`", "'a2bc'"),
        ("function tag(s, ...v) { return s.raw.join('|') + v.join(','); } tag`x${1}y${2}z`", "'x|y|z1,2'"),
        ("String.raw`a\\nb`", "'a\\nb'"),
        // One frozen template object per call site, whatever the evaluation
        ("function tag(s) { return s; } const f = () => tag`a${1}b`; const a = f(); [a === f(), a !== tag`a${1}b`, Object.isFrozen(a), Object.isFrozen(a.raw)].join()", "'true,true,true,true'"),
        ("'abc'.localeCompare('abc')", "0"),
        ("new String('ab').length", "2"),
        ("typeof new String('ab')", "'object'"),
        ("new String('ab') + 'c'", "'abc'"),
        ("'\\u0041\\x42\\u{43}'", "'ABC'"),
        ("'a' < 'B'", "false"),
    ]);
}

#[test]
fn control_flow() {
    check(&[
        ("let s = 0; for (let i = 0; i < 10; i++) { if (i === 5) continue; if (i === 8) break; s += i; } s", "23"),
        ("let n = 0; outer: for (let i = 0; i < 3; i++) { for (let j = 0; j < 3; j++) { if (j === 1) continue outer; if (i === 2) break outer; n++; } } n", "2"),
        ("let r = []; lbl: { r.push(1); break lbl; r.push(2); } r", "[ 1 ]"),
        ("let i = 0; do { i++; } while (i < 5); i", "5"),
        ("let i = 10; do { i++; } while (false); i", "11"),
        ("function f(x) { switch (x) { case 1: return 'one'; case 2: case 3: return 'two-three'; default: return 'other'; } } [f(1), f(3), f(9)]", "[ 'one', 'two-three', 'other' ]"),
        ("let r = ''; switch (2) { case 1: r += 'a'; case 2: r += 'b'; case 3: r += 'c'; break; case 4: r += 'd'; } r", "'bc'"),
        ("let r = ''; switch (5) { default: r += 'd'; case 1: r += 'a'; } r", "'da'"),
        ("switch ('1') { case 1: 'num'; break; default: 'str'; }", "'str'"),
        ("function f() { try { return 'try'; } finally { log = 'fin'; } } var log; [f(), log]", "[ 'try', 'fin' ]"),
        ("function f() { try { return 'try'; } finally { return 'finally'; } } f()", "'finally'"),
        ("function f() { try { throw 1; } catch (e) { return 'caught ' + e; } finally { x = 1; } } var x; f() + x", "'caught 11'"),
        ("let r = []; for (let i = 0; i < 3; i++) { try { if (i === 1) continue; r.push(i); } finally { r.push('f' + i); } } r", "[ 0, 'f0', 'f1', 2, 'f2' ]"),
        ("let r = []; for (let i = 0; i < 3; i++) { try { if (i === 1) break; } finally { r.push('f' + i); } } r", "[ 'f0', 'f1' ]"),
        ("let r = []; try { try { throw 'inner'; } finally { r.push('f1'); } } catch (e) { r.push(e); } r", "[ 'f1', 'inner' ]"),
        ("let r = []; try { try { throw 1; } catch (e) { throw 2; } finally { r.push('f'); } } catch (e) { r.push(e); } r", "[ 'f', 2 ]"),
        ("function f() { for (const x of [1, 2]) { try { return x; } finally { log.push('f' + x); } } } var log = []; [f(), log]", "[ 1, [ 'f1' ] ]"),
        ("let n = 0; try { n = 1; } finally { n += 10; } n", "11"),
        ("function g() { try { throw new Error('x'); } catch { return 'no binding'; } } g()", "'no binding'"),
        ("try { null.x; } catch (e) { e instanceof TypeError }", "true"),
        ("try { undefinedFn(); } catch (e) { e.name + ': ' + e.message }", "'ReferenceError: undefinedFn is not defined'"),
        ("try { throw {code: 42}; } catch ({code}) { code }", "42"),
        ("let log = []; let it = { [Symbol.iterator]() { let i = 0; return { next: () => ({ value: i++, done: i > 5 }), return() { log.push('closed'); return {}; } }; } }; for (const v of it) { if (v === 2) break; } log", "[ 'closed' ]"),
        ("let log = []; let it = { [Symbol.iterator]() { return { next: () => ({ value: 1, done: false }), return() { log.push('closed'); return {}; } }; } }; try { for (const v of it) throw 'boom'; } catch (e) { log.push(e); } log", "[ 'closed', 'boom' ]"),
        ("let it = { [Symbol.iterator]() { let n = 0; return { next() { return { value: n, done: n++ >= 3 }; } }; } }; [...it]", "[ 0, 1, 2 ]"),
        ("let [a, b] = { [Symbol.iterator]: function* () {} }; a", "undefined"),
        ("let x = 0; while (x < 10) x += 3; x", "12"),
        ("for (;;) { break; } 'done'", "'done'"),
        ("let c = 0; for (let i = 0, j = 10; i < j; i++, j--) c++; c", "5"),
        ("if (0) 'a'; else if ('') 'b'; else 'c'", "'c'"),
        ("let v = 1 ? 2 ? 'x' : 'y' : 'z'; v", "'x'"),
        ("let a = 0; a++ || a++; a", "2"),
        ("(() => { throw new RangeError('r'); })()", "throws RangeError: r"),
    ]);
}

#[test]
fn classes() {
    check(&[
        ("class A { constructor(x) { this.x = x; } get double() { return this.x * 2; } static make() { return new A(4); } } A.make().double", "8"),
        ("class A {} A()", "throws TypeError: Class constructor cannot be invoked without 'new'"),
        ("class A { m() { return 'A'; } } class B extends A { m() { return 'B' + super.m(); } } new B().m()", "'BA'"),
        ("class A { constructor() { this.a = 1; } } class B extends A { constructor() { super(); this.b = 2; } } let b = new B(); b.a + b.b", "3"),
        ("class A { constructor(v) { this.v = v; } } class B extends A {} new B(7).v", "7"),
        ("class A {} class B extends A { constructor() { this.x = 1; } } new B()", "throws ReferenceError*"),
        ("class A { x = 1; y = this.x + 1; } let a = new A(); a.x + a.y", "3"),
        ("class A { static s = 'st'; static t = A.s + '!'; } A.t", "'st!'"),
        ("class A { #p = 5; get p() { return this.#p; } inc() { this.#p++; return this; } } new A().inc().p", "6"),
        ("class A { #secret() { return 'hidden'; } reveal() { return this.#secret(); } } new A().reveal()", "'hidden'"),
        ("class A { #x = 1; } Object.keys(new A()).length", "0"),
        ("class A { static #count = 0; static next() { return ++A.#count; } } A.next(); A.next()", "2"),
        ("class A { static { this.inited = true; } } A.inited", "true"),
        ("class A { ['comp' + 'uted']() { return 1; } } new A().computed()", "1"),
        ("class A { static get g() { return 'sg'; } } A.g", "'sg'"),
        ("class A { set v(x) { this._v = x * 2; } get v() { return this._v; } } let a = new A(); a.v = 3; a.v", "6"),
        ("class A {} class B extends A {} Object.getPrototypeOf(B) === A", "true"),
        ("class A { static s() { return 'a'; } } class B extends A {} B.s()", "'a'"),
        ("class A { m() {} } Object.keys(A.prototype).length", "0"),
        ("class A { m() {} } typeof A.prototype.m", "'function'"),
        ("class A {} A.name", "'A'"),
        ("let C = class {}; C.name", "'C'"),
        ("let C = class Named { who() { return Named.name; } }; new C().who()", "'Named'"),
        ("class E extends Error { constructor(m) { super(m); this.name = 'E'; } } let e = new E('msg'); [e instanceof E, e instanceof Error, e.message, String(e)]", "[ true, true, 'msg', 'E: msg' ]"),
        ("class A { constructor() { return {custom: true}; } } new A().custom", "true"),
        ("class A { static create() { return new this(); } } class B extends A {} B.create() instanceof B", "true"),
        ("class A { m() { return this.constructor.name; } } class B extends A {} new B().m()", "'B'"),
        ("class A { x = 'A field'; } class B extends A { y = this.x + '!'; } new B().y", "'A field!'"),
        ("class A { constructor() { this.init(); } init() { this.v = 'A'; } } class B extends A { init() { this.v = 'B'; } } new B().v", "'B'"),
        ("class A { get x() { return 1; } } class B extends A { get x() { return super.x + 1; } } new B().x", "2"),
        ("let o = { m() { return 'o'; } }; let p = { __proto__: o, m() { return super.m() + 'p'; } }; p.m()", "'op'"),
        ("class A { static m() { return 'A'; } } class B extends A { static m() { return super.m() + 'B'; } } B.m()", "'AB'"),
        ("class A { arrow = () => this; } let a = new A(); a.arrow() === a", "true"),
        ("class A { constructor() { this.f = () => super.toString(); } } typeof new A().f()", "'string'"),
        ("class A extends null {} typeof A", "'function'"),
        ("class A extends 5 {}", "throws TypeError: Class extends value 5 is not a constructor or null"),
        ("class A { static x = 1; static y = this.x + 1; } A.y", "2"),
        ("class Point { constructor(x, y) { Object.assign(this, {x, y}); } toString() { return `(${this.x}, ${this.y})`; } } `${new Point(1, 2)}`", "'(1, 2)'"),
        ("class Counter { static #instances = 0; constructor() { Counter.#instances++; } static get count() { return Counter.#instances; } } new Counter(); new Counter(); Counter.count", "2"),
        ("class A { 'quoted key'() { return 'q'; } } new A()['quoted key']()", "'q'"),
        ("class A { constructor() { this.a = new.target === A; } } class B extends A {} [new A().a, new B().a]", "[ true, false ]"),
        ("class MyMap extends Map { getOr(k, d) { return this.has(k) ? this.get(k) : d; } } new MyMap([[1, 'one']]).getOr(2, 'none')", "'none'"),
    ]);
}

#[test]
fn builtins() {
    check(&[
        ("Math.max()", "-Infinity"),
        ("Math.min(1, -2, 3)", "-2"),
        ("Math.max(1, NaN)", "NaN"),
        ("Math.round(2.5)", "3"),
        ("Math.round(-2.5)", "-2"),
        ("Math.round(-0.2)", "-0"),
        ("Math.floor(-1.5)", "-2"),
        ("Math.ceil(1.1)", "2"),
        ("Math.trunc(-4.7)", "-4"),
        ("Math.sign(-3)", "-1"),
        ("Math.abs(-5)", "5"),
        ("Math.sqrt(16)", "4"),
        ("Math.pow(2, 0.5) === Math.SQRT2", "true"),
        ("Math.hypot(3, 4)", "5"),
        ("Math.imul(0xffffffff, 5)", "-5"),
        ("Math.clz32(1)", "31"),
        ("Math.cbrt(27)", "3"),
        ("let r = Math.random(); r >= 0 && r < 1", "true"),
        ("JSON.stringify({a: [1, 'two', true, null], b: {c: undefined, d: () => 1}})", "'{\"a\":[1,\"two\",true,null],\"b\":{}}'"),
        ("JSON.stringify('he said \"hi\"\\n')", "'\"he said \\\"hi\\\"\\n\"'"),
        ("JSON.stringify([undefined, function() {}, Symbol()])", "'[null,null,null]'"),
        ("JSON.stringify({a: 1, b: [1, 2]}, null, 2)", "'{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2\n  ]\n}'"),
        ("JSON.stringify({a: 1, b: 2, c: 3}, ['a', 'c'])", "'{\"a\":1,\"c\":3}'"),
        ("JSON.stringify({a: 1, b: 'x'}, (k, v) => typeof v === 'number' ? v * 10 : v)", "'{\"a\":10,\"b\":\"x\"}'"),
        ("JSON.stringify({toJSON() { return 'custom'; }})", "'\"custom\"'"),
        ("JSON.stringify(NaN)", "'null'"),
        ("JSON.stringify(undefined)", "undefined"),
        ("let o = {}; o.self = o; JSON.stringify(o)", "throws TypeError: Converting circular structure to JSON"),
        ("JSON.parse('{\"a\": [1, 2.5, \"s\", true, null], \"b\": {\"c\": -1e3}}').b.c", "-1000"),
        ("JSON.parse('\"\\\\u0041\\\\n\"')", "'A\n'"),
        ("JSON.parse('[1, 2', )", "throws SyntaxError*"),
        ("JSON.parse('{\"a\": 1}', (k, v) => typeof v === 'number' ? v + 1 : v).a", "2"),
        ("JSON.parse(' 42 ')", "42"),
        ("JSON.parse('{\"__proto__\": 1}').__proto__", "1"),
        ("let m = new Map(); m.set('a', 1).set(NaN, 'nan').set(0, 'zero'); [m.get('a'), m.get(NaN), m.get(-0), m.size]", "[ 1, 'nan', 'zero', 3 ]"),
        ("let m = new Map([[1, 'a'], [2, 'b']]); m.delete(1); [...m]", "[ [ 2, 'b' ] ]"),
        ("let m = new Map([[1, 'a'], [2, 'b']]); [...m.keys()].concat([...m.values()])", "[ 1, 2, 'a', 'b' ]"),
        ("let m = new Map(); let k = {}; m.set(k, 'obj'); m.get(k) + m.has({})", "'objfalse'"),
        ("let s = new Set([1, 2, 2, 3]); s.add(4); [s.size, s.has(2), s.has(5)]", "[ 4, true, false ]"),
        ("let s = new Set(['a', 'b']); let r = []; s.forEach(v => r.push(v)); r", "[ 'a', 'b' ]"),
        ("let s = new Set([1, 2, 3]); for (const v of s) if (v === 1) s.delete(2); [...s]", "[ 1, 3 ]"),
        ("let m = new Map(); m.set('x', 1); let out = []; for (const [k, v] of m) out.push(k + v); out", "[ 'x1' ]"),
        ("let wm = new WeakMap(); let k = {}; wm.set(k, 5); wm.get(k)", "5"),
        ("new WeakMap().set(1, 1)", "throws TypeError: Invalid value used as weak map key"),
        ("Map()", "throws TypeError: Constructor requires 'new'"),
        ("new Map([[1, 2]])", "Map(1) { 1 => 2 }"),
        ("Object.prototype.toString.call(new Map())", "'[object Map]'"),
        ("let e = new Error('boom'); [e.message, e.name, e instanceof Error]", "[ 'boom', 'Error', true ]"),
        ("new TypeError('t') instanceof Error", "true"),
        ("String(new RangeError('r'))", "'RangeError: r'"),
        ("new Error('x', {cause: 'why'}).cause", "'why'"),
        ("Error('no new').message", "'no new'"),
        ("typeof new Error().stack", "'string'"),
        ("Object.prototype.toString.call(new Error())", "'[object Error]'"),
        ("[1, 2, 3].toString()", "'1,2,3'"),
        ("({}).toString()", "'[object Object]'"),
        ("true.toString()", "'true'"),
        ("new Boolean(false) ? 'truthy' : 'falsy'", "'truthy'"),
        ("new Number(5) + 1", "6"),
        ("Number.parseFloat === parseFloat", "false"),
        ("globalThis.Array === Array", "true"),
        ("let n = 0; queueMicrotask(() => n++); n", "0"),
    ]);
}

#[test]
fn iteration_protocols() {
    check(&[
        ("let it = [1, 2][Symbol.iterator](); [it.next(), it.next(), it.next()]", "[ { value: 1, done: false }, { value: 2, done: false }, { value: undefined, done: true } ]"),
        ("let it = 'ab'[Symbol.iterator](); it.next().value", "'a'"),
        ("let it = new Map([[1, 2]]).entries(); it.next().value", "[ 1, 2 ]"),
        ("let range = { from: 1, to: 4, [Symbol.iterator]() { let c = this.from, t = this.to; return { next: () => c <= t ? { value: c++, done: false } : { value: undefined, done: true } }; } }; [...range]", "[ 1, 2, 3, 4 ]"),
        ("let [a, ...b] = 'xyz'; b", "[ 'y', 'z' ]"),
        ("Array.from({length: 2, 0: 'a', 1: 'b'})", "[ 'a', 'b' ]"),
        ("for (const x of 5) {}", "throws TypeError: 5 is not iterable"),
        ("let [x] = {}", "throws TypeError: object is not iterable"),
        ("Math.max(...new Set([3, 9, 1]))", "9"),
        ("new Set('hello').size", "4"),
        ("Object.fromEntries(new Map([['k', 'v']]))", "{ k: 'v' }"),
        ("let ai = [][Symbol.iterator](); Object.prototype.toString.call(ai)", "'[object Array Iterator]'"),
        ("let ai = [][Symbol.iterator](); ai[Symbol.iterator]() === ai", "true"),
    ]);
}

#[test]
fn garbage_collection_keeps_live_values() {
    check(&[
        // Enough allocation to force several collections
        ("let keep = []; for (let i = 0; i < 200000; i++) { let o = {i, s: 'v' + i}; if (i % 1000 === 0) keep.push(o); } keep.length + keep[199].s.length", "207"),
        ("let m = new Map(); for (let i = 0; i < 100000; i++) m.set('k' + (i % 100), [i]); m.get('k5')[0]", "99905"),
        ("function mk(n) { return () => n; } let fs = []; for (let i = 0; i < 100000; i++) { let f = mk(i); if (i % 10000 === 0) fs.push(f); } fs.map(f => f()).join()", "'0,10000,20000,30000,40000,50000,60000,70000,80000,90000'"),
        ("let s = ''; for (let i = 0; i < 50000; i++) s += String.fromCharCode(97 + i % 26); s.length + s.charCodeAt(49999)", "50098"),
        ("let arr = []; for (let i = 0; i < 300000; i++) arr.push({v: i}); let t = 0; for (const o of arr) t += o.v; t", "44999850000"),
        ("let a = [3, 1, 2]; let calls = 0; for (let r = 0; r < 20000; r++) a.slice().sort((x, y) => { calls++; let junk = {x, y}; return x - y; }); calls > 0", "true"),
        ("let o = {}; for (let i = 0; i < 100000; i++) o['p' + (i % 500)] = {i}; o.p499.i", "99999"),
        ("let res = JSON.parse(JSON.stringify(Array.from({length: 20000}, (_, i) => ({i, s: 'x' + i})))); res[19999].s", "'x19999'"),
    ]);
}

#[test]
fn larger_programs() {
    check(&[
        (
            "function Node(v) { this.v = v; this.next = null; }
             function List() { this.head = null; this.size = 0; }
             List.prototype.push = function (v) { let n = new Node(v); n.next = this.head; this.head = n; this.size++; return this; };
             List.prototype.toArray = function () { let out = []; for (let n = this.head; n; n = n.next) out.push(n.v); return out; };
             let l = new List(); for (let i = 0; i < 5; i++) l.push(i * i); l.toArray().join(' ')",
            "'16 9 4 1 0'",
        ),
        (
            "function quicksort(a) { if (a.length < 2) return a; const [p, ...rest] = a; return [...quicksort(rest.filter(x => x < p)), p, ...quicksort(rest.filter(x => x >= p))]; }
             quicksort([5, 3, 8, 1, 9, 2, 7]).join()",
            "'1,2,3,5,7,8,9'",
        ),
        (
            "const memo = new Map(); function fib(n) { if (n < 2) return n; if (memo.has(n)) return memo.get(n); const r = fib(n - 1) + fib(n - 2); memo.set(n, r); return r; } fib(80)",
            "23416728348467684",
        ),
        (
            "class Stack { #items = []; push(x) { this.#items.push(x); return this; } pop() { return this.#items.pop(); } get size() { return this.#items.length; } }
             const s = new Stack().push(1).push(2).push(3); s.pop(); s.size",
            "2",
        ),
        (
            "const words = 'the quick brown fox jumps over the lazy dog the end'.split(' ');
             const freq = {}; for (const w of words) freq[w] = (freq[w] || 0) + 1;
             Object.entries(freq).sort((a, b) => b[1] - a[1] || (a[0] < b[0] ? -1 : 1)).slice(0, 2).map(([w, c]) => w + ':' + c).join()",
            "'the:3,brown:1'",
        ),
        (
            "const compose = (...fns) => x => fns.reduceRight((v, f) => f(v), x);
             compose(x => x + 1, x => x * 2)(5)",
            "11",
        ),
        (
            "let matrix = Array.from({length: 3}, (_, i) => Array.from({length: 3}, (_, j) => i * 3 + j));
             matrix.map(row => row.reduce((a, b) => a + b)).join()",
            "'3,12,21'",
        ),
        (
            "const inventory = [{name: 'apple', type: 'fruit', n: 3}, {name: 'leek', type: 'veg', n: 1}, {name: 'pear', type: 'fruit', n: 2}];
             const byType = inventory.reduce((acc, {type, n}) => ({...acc, [type]: (acc[type] ?? 0) + n}), {});
             JSON.stringify(byType)",
            "'{\"fruit\":5,\"veg\":1}'",
        ),
        (
            "let log = []; const handler = { events: {}, on(e, f) { (this.events[e] ??= []).push(f); return this; }, emit(e, ...a) { (this.events[e] || []).forEach(f => f(...a)); } };
             handler.on('x', v => log.push('a' + v)).on('x', v => log.push('b' + v)); handler.emit('x', 1); log",
            "[ 'a1', 'b1' ]",
        ),
    ]);
}

#[test]
fn regular_expressions() {
    check(&[
        ("/a+b/.test('xaaab')", "true"),
        ("/^a+b$/.test('xaaab')", "false"),
        ("/(\\d+)-(\\d+)/.exec('tel 12-345').slice(0)", "[ '12-345', '12', '345' ]"),
        ("/(\\d+)-(\\d+)/.exec('tel 12-345').index", "4"),
        ("/(?<year>\\d{4})-(?<month>\\d{2})/.exec('2024-05').groups.month", "'05'"),
        ("/x/.exec('abc')", "null"),
        ("'a1b22c333'.match(/\\d+/g)", "[ '1', '22', '333' ]"),
        ("'abc'.match(/z/g)", "null"),
        ("'Hello World'.replace(/o/g, '0')", "'Hell0 W0rld'"),
        ("'John Smith'.replace(/(\\w+)\\s(\\w+)/, '$2, $1')", "'Smith, John'"),
        ("'2024-05'.replace(/(?<y>\\d+)-(?<m>\\d+)/, '$<m>/$<y>')", "'05/2024'"),
        ("'aaa'.replace(/a/g, (m, i) => i)", "'012'"),
        ("'x'.replace(/(y)?x/, (m, g1) => typeof g1)", "'undefined'"),
        ("'a, b,c'.split(/\\s*,\\s*/)", "[ 'a', 'b', 'c' ]"),
        ("'abc'.split(/(b)/)", "[ 'a', 'b', 'c' ]"),
        ("'abc'.split(/(?:)/)", "[ 'a', 'b', 'c' ]"),
        ("''.split(/x/)", "[ '' ]"),
        ("'a1b2'.split(/\\d/, 1)", "[ 'a' ]"),
        ("'test'.search(/s/)", "2"),
        ("/ABC/i.test('xabcx')", "true"),
        ("/^b/m.test('a\\nb')", "true"),
        ("/^b/.test('a\\nb')", "false"),
        ("/a.c/.test('a\\nc')", "false"),
        ("/a.c/s.test('a\\nc')", "true"),
        ("/\\bfoo\\b/.test('a foo b')", "true"),
        ("/\\bfoo\\b/.test('afoob')", "false"),
        ("/(a|ab)(c|bcd)(d*)/.exec('abcd').slice(0)", "[ 'abcd', 'a', 'bcd', '' ]"),
        ("/a*?b/.exec('aaab')[0]", "'aaab'"),
        ("/<.+?>/.exec('<a><b>')[0]", "'<a>'"),
        ("/<.+>/.exec('<a><b>')[0]", "'<a><b>'"),
        ("/(a)\\1/.test('aa')", "true"),
        ("/(?<q>['\"]).*?\\k<q>/.exec(`say \"hi\" now`)[0]", "'\"hi\"'"),
        ("/\\d{2,3}/.exec('1234')[0]", "'123'"),
        ("/\\d{2,}/.exec('1234567')[0]", "'1234567'"),
        ("/x{2}/.test('xx')", "true"),
        ("/a{,2}/.test('a{,2}')", "true"),
        ("/[a-c]+/.exec('xxabcabd')[0]", "'abcab'"),
        ("/[^a-c]+/.exec('abcxyz')[0]", "'xyz'"),
        ("/[\\d.]+/.exec('v1.2.3')[0]", "'1.2.3'"),
        ("/[\\w-]+/.exec('foo-bar baz')[0]", "'foo-bar'"),
        ("/q(?=u)/.exec('quit').index", "0"),
        ("/q(?!u)/.exec('quit qat').index", "5"),
        ("/(?<=\\$)\\d+/.exec('cost $42')[0]", "'42'"),
        ("/(?<!\\$)\\b\\d+/.exec('$42 and 17')[0]", "'17'"),
        ("/(a*)*b/.test('aaaac')", "false"),
        ("/(a|b)*c/.exec('ababc')[1]", "'b'"),
        ("/(z)((a+)?(b+)?(c))*/.exec('zaacbbbcac').slice(0)", "[ 'zaacbbbcac', 'z', 'ac', 'a', undefined, 'c' ]"),
        ("/\\u{1F600}/u.test('😀')", "true"),
        ("/^.$/u.test('😀')", "true"),
        ("/^.$/.test('😀')", "false"),
        ("/\\p{Lu}/u.test('É')", "true"),
        ("/[\\u0041-\\u005A]/.test('Q')", "true"),
        ("/\\x41/.test('A')", "true"),
        ("/\\//.test('a/b')", "true"),
        ("/[/]/.test('/')", "true"),
        ("let re = /o/g; let r = []; let m; while ((m = re.exec('foo boo'))) r.push(m.index); r", "[ 1, 2, 5, 6 ]"),
        ("let re = /a/g; re.test('aa'); re.lastIndex", "1"),
        ("let re = /a/y; [re.test('ba'), re.lastIndex]", "[ false, 0 ]"),
        ("[...'a1b2c3'.matchAll(/[a-z](\\d)/g)].map(m => m[1])", "[ '1', '2', '3' ]"),
        ("'aaa'.replaceAll(/a/g, 'b')", "'bbb'"),
        ("'aaa'.replaceAll(/a/, 'b')", "throws TypeError*"),
        ("new RegExp('a+', 'g').flags", "'g'"),
        ("new RegExp('/').source", "'\\/'"),
        ("String(/a\\/b/gi)", "'/a\\/b/gi'"),
        ("new RegExp('(')", "throws SyntaxError: Invalid regular expression: unterminated group"),
        ("new RegExp('a', 'gg')", "throws SyntaxError*"),
        ("RegExp('x') instanceof RegExp", "true"),
        ("let r = /x/; RegExp(r) === r", "true"),
        ("Object.prototype.toString.call(/x/)", "'[object RegExp]'"),
        ("/(?:ab)+/.exec('ababx')[0]", "'abab'"),
        ("/a|b|c/.exec('xcx')[0]", "'c'"),
        ("/[]/.test('a')", "false"),
        ("/[^]/.test('\\n')", "true"),
        ("/\\s+/.exec('a \\t\\n b')[0].length", "4"),
        ("'CamelCaseString'.replace(/([A-Z])/g, ' $1').trim()", "'Camel Case String'"),
        ("'  trim me  '.replace(/^\\s+|\\s+$/g, '')", "'trim me'"),
        ("'x=1&y=2'.split('&').map(p => p.split('=')).map(([k, v]) => k + v).join()", "'x1,y2'"),
        ("/^[\\w.+-]+@[\\w-]+\\.[\\w.]+$/.test('user.name+tag@example.co.uk')", "true"),
        ("/(\\d)(?=(\\d{3})+$)/g[Symbol.replace]('1234567', '$1,')", "'1,234,567'"),
        ("let n = 0; for (let i = 0; i < 1000; i++) if (/^item-\\d+$/.test('item-' + i)) n++; n", "1000"),
    ]);
}

#[test]
fn generators() {
    check(&[
        ("function* g() { yield 1; yield 2; return 3; } [...g()]", "[ 1, 2 ]"),
        ("function* g() { yield 1; return 3; } let it = g(); [it.next(), it.next(), it.next()]", "[ { value: 1, done: false }, { value: 3, done: true }, { value: undefined, done: true } ]"),
        ("function* g() { let x = yield 1; yield x * 2; } let it = g(); it.next(); it.next(21).value", "42"),
        ("function* nat() { let n = 0; while (true) yield n++; } let out = []; for (const n of nat()) { if (n > 4) break; out.push(n); } out", "[ 0, 1, 2, 3, 4 ]"),
        ("function* g() { try { yield 1; } catch (e) { yield 'caught ' + e; } } let it = g(); it.next(); it.throw('boom').value", "'caught boom'"),
        ("function* g() { yield 1; } let it = g(); it.throw(new Error('x'))", "throws Error: x"),
        ("function* g() { yield 1; yield 2; } let it = g(); it.next(); [it.return(9), it.next()]", "[ { value: 9, done: true }, { value: undefined, done: true } ]"),
        ("function* inner() { yield 'a'; yield 'b'; return 'r'; } function* outer() { const r = yield* inner(); yield r; } [...outer()]", "[ 'a', 'b', 'r' ]"),
        ("function* g() { yield* [1, 2]; yield* 'xy'; } [...g()]", "[ 1, 2, 'x', 'y' ]"),
        ("function* g() { let x = 1; const f = () => x; yield f(); x = 5; yield f(); } [...g()]", "[ 1, 5 ]"),
        ("function* g() { let x = 1; const inc = () => ++x; yield 0; inc(); yield x; } [...g()]", "[ 0, 2 ]"),
        ("function* g() { for (let i = 0; i < 3; i++) yield () => i; } [...g()].map(f => f())", "[ 0, 1, 2 ]"),
        ("function* fib() { let [a, b] = [0, 1]; for (;;) { yield a; [a, b] = [b, a + b]; } } let r = []; for (const f of fib()) { if (f > 50) break; r.push(f); } r.join()", "'0,1,1,2,3,5,8,13,21,34'"),
        ("function* g() { yield this.v; } g.call({v: 7}).next().value", "7"),
        ("let o = { *items() { yield* this.list; }, list: [1, 2] }; [...o.items()]", "[ 1, 2 ]"),
        ("class C { *[Symbol.iterator]() { yield 'c'; } } [...new C()]", "[ 'c' ]"),
        ("function* g() {} Object.getPrototypeOf(g()) === g.prototype", "true"),
        ("function* g() {} g() instanceof g", "true"),
        ("function* g() {} new g()", "throws TypeError*"),
        ("function* g() {} g()[Symbol.iterator]() instanceof g", "true"),
        ("function* g() { yield 1; } String(g())", "'[object Generator]'"),
        ("function* g(a, b = a + 1) { yield a + b; } g(1).next().value", "3"),
        ("function* g() { const x = yield; return x; } let it = g(); it.next(); it.next('sent')", "{ value: 'sent', done: true }"),
        ("function* g() { yield 1; } let it = g(); it.next(); it.next(); it.next()", "{ value: undefined, done: true }"),
        ("function* g() { try { yield 1; } finally { log.push('cleanup'); } } var log = []; for (const x of g()) break; log", "[ 'cleanup' ]*"),
        ("function* take(n, it) { for (const x of it) { if (n-- <= 0) return; yield x; } } function* nat() { let i = 0; while (true) yield i++; } [...take(3, nat())]", "[ 0, 1, 2 ]"),
        ("function* g() { yield [yield 1, yield 2]; } let it = g(); it.next(); it.next('a'); it.next('b').value", "[ 'a', 'b' ]"),
        ("let n = 0; function* g() { while (true) { yield {n: n++}; } } let it = g(); for (let i = 0; i < 50000; i++) it.next(); it.next().value.n", "50000"),
    ]);
}

#[test]
fn promises_and_async() {
    check_async(&[
        ("var result; Promise.resolve(5).then(v => result = v * 2);", "10"),
        ("var result = []; Promise.resolve().then(() => result.push('micro')); result.push('sync');", "[ 'sync', 'micro' ]"),
        ("var result; new Promise((res) => res('ok')).then(v => { result = v; });", "'ok'"),
        ("var result; new Promise((_, rej) => rej(new Error('bad'))).catch(e => { result = e.message; });", "'bad'"),
        ("var result; new Promise(() => { throw 'thrown'; }).catch(e => { result = e; });", "'thrown'"),
        ("var result; Promise.reject(1).then(() => 'no', e => 'handled ' + e).then(v => result = v);", "'handled 1'"),
        ("var result; Promise.resolve(1).then(v => v + 1).then(v => v * 10).then(v => result = v);", "20"),
        ("var result; Promise.resolve(1).then(v => Promise.resolve(v + 1)).then(v => result = v);", "2"),
        ("var result = []; Promise.resolve(1).finally(() => result.push('f')).then(v => result.push(v));", "[ 'f', 1 ]"),
        ("var result; Promise.reject('e').finally(() => {}).catch(e => result = 'still ' + e);", "'still e'"),
        ("var result; Promise.all([1, Promise.resolve(2), new Promise(r => r(3))]).then(v => result = v);", "[ 1, 2, 3 ]"),
        ("var result; Promise.all([]).then(v => result = v);", "[]"),
        ("var result; Promise.all([1, Promise.reject('no')]).catch(e => result = e);", "'no'"),
        ("var result; Promise.allSettled([1, Promise.reject('x')]).then(v => result = v.map(s => s.status));", "[ 'fulfilled', 'rejected' ]"),
        ("var result; Promise.race([new Promise(() => {}), Promise.resolve('fast')]).then(v => result = v);", "'fast'"),
        ("var result; Promise.any([Promise.reject(1), Promise.resolve(2)]).then(v => result = v);", "2"),
        ("var result; Promise.any([Promise.reject(1)]).catch(e => result = e.errors);", "[ 1 ]"),
        ("var result; const {promise, resolve} = Promise.withResolvers(); promise.then(v => result = v); resolve('wr');", "'wr'"),
        ("var result; ({ then(r) { r('thenable'); } }); Promise.resolve({ then(r) { r('thenable'); } }).then(v => result = v);", "'thenable'"),
        ("var result = []; setTimeoutLike = f => Promise.resolve().then(f); Promise.resolve().then(() => result.push(1)).then(() => result.push(3)); Promise.resolve().then(() => result.push(2)).then(() => result.push(4));", "[ 1, 2, 3, 4 ]"),
        ("var result; async function f() { return 42; } f().then(v => result = v);", "42"),
        ("var result; async function f() { return await Promise.resolve(7) * 2; } f().then(v => result = v);", "14"),
        ("var result; async function f() { throw new Error('async fail'); } f().catch(e => result = e.message);", "'async fail'"),
        ("var result; async function f() { try { await Promise.reject('r'); } catch (e) { return 'caught ' + e; } } f().then(v => result = v);", "'caught r'"),
        ("var result = []; async function f() { result.push('a'); await null; result.push('c'); } f(); result.push('b');", "[ 'a', 'b', 'c' ]"),
        ("var result; async function f() { let s = 0; for (let i = 0; i < 5; i++) s += await i; return s; } f().then(v => result = v);", "10"),
        ("var result; const f = async (x) => x + (await 1); f(1).then(v => result = v);", "2"),
        ("var result; class A { async m() { return this.v; } } let a = new A(); a.v = 'mv'; a.m().then(v => result = v);", "'mv'"),
        ("var result; async function inner() { await null; return 'in'; } async function outer() { return 'out+' + await inner(); } outer().then(v => result = v);", "'out+in'"),
        ("var result; async function f() { const [a, b] = await Promise.all([1, 2]); return a + b; } f().then(v => result = v);", "3"),
        ("var result; async function f() { let x = 1; const g = () => x; await null; x = 9; return g(); } f().then(v => result = v);", "9"),
        ("var result; function delay(v) { return new Promise(r => Promise.resolve().then(() => r(v))); } async function f() { let out = []; for (const v of [1, 2, 3]) out.push(await delay(v)); return out; } f().then(v => result = v);", "[ 1, 2, 3 ]"),
        ("var result; async function f() { try { await null; throw 'late'; } finally { result = 'finally ran'; } } f().catch(() => {});", "'finally ran'"),
        ("var result = typeof (async function() {})().then;", "'function'"),
        ("var result = Object.prototype.toString.call(Promise.resolve());", "'[object Promise]'"),
        ("var result; Promise.resolve().then(() => { throw new TypeError('in then'); }).catch(e => result = e instanceof TypeError);", "true"),
        ("var result = 0; for (let i = 0; i < 1000; i++) Promise.resolve(i).then(v => result += v);", "499500"),
        ("var result; async function* ag() { yield 5; } ag().next().then(r => result = r.value);", "5"),
        ("var result; queueMicrotask(() => result = 'queued');", "'queued'"),
        ("var result; new Promise(r => r()).then(() => new Promise(r => r('nested'))).then(v => result = v);", "'nested'"),
        ("var result; const p = Promise.resolve(); result = p.then() instanceof Promise;", "true"),
        ("var result; let p = new Promise(r => r(1)); p.then(v => { result = v; }); p.then(v => { result += v; });", "2"),
    ]);
}

#[test]
fn dates() {
    check(&[
        ("new Date(0).toISOString()", "'1970-01-01T00:00:00.000Z'"),
        ("new Date(Date.UTC(2024, 1, 29, 12, 30, 15, 250)).toISOString()", "'2024-02-29T12:30:15.250Z'"),
        ("new Date('2024-03-10').getTime()", "1710028800000"),
        ("new Date('2024-03-10T08:05:03Z').getUTCHours()", "8"),
        ("new Date('2024-03-10T08:05:03.5+02:00').toISOString()", "'2024-03-10T06:05:03.500Z'"),
        ("Date.parse('Tue Mar 05 2024 10:00:00 GMT+0000')", "1709632800000"),
        ("Date.parse('March 5, 2024')", "1709596800000"),
        ("Date.parse('2024/03/05')", "1709596800000"),
        ("Date.parse('Tue, 05 Mar 2024 10:00:00 GMT')", "1709632800000"),
        ("Date.parse('not a date')", "NaN"),
        ("let d = new Date(2020, 0, 31); d.setMonth(1); d.getDate()", "2"),
        ("let d = new Date(2024, 0, 1); [d.getFullYear(), d.getMonth(), d.getDate(), d.getDay()]", "[ 2024, 0, 1, 1 ]"),
        ("new Date(2024, 11, 32).getMonth()", "0"),
        ("new Date(99, 0).getFullYear()", "1999"),
        ("new Date(1e20).getTime()", "NaN"),
        ("String(new Date(NaN))", "'Invalid Date'"),
        ("new Date(NaN).toISOString()", "throws RangeError: Invalid time value"),
        ("new Date(0).toString()", "'Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)'"),
        ("new Date(0).toUTCString()", "'Thu, 01 Jan 1970 00:00:00 GMT'"),
        ("JSON.stringify({d: new Date(0)})", "'{\"d\":\"1970-01-01T00:00:00.000Z\"}'"),
        ("new Date(0) - new Date(1000)", "-1000"),
        ("typeof (new Date() + 1)", "'string'"),
        ("new Date(0) < new Date(1)", "true"),
        ("let t = Date.now(); typeof t === 'number' && t > 1.6e12", "true"),
        ("new Date(new Date(5)).getTime()", "5"),
        ("new Date(-1).toISOString()", "'1969-12-31T23:59:59.999Z'"),
        ("new Date('2000-02-29T00:00:00Z').getUTCDate()", "29"),
        ("let d = new Date(0); d.setHours(25); d.toISOString()", "'1970-01-02T01:00:00.000Z'"),
        ("Object.prototype.toString.call(new Date())", "'[object Date]'"),
        ("typeof Date()", "'string'"),
    ]);
}

#[test]
fn typed_arrays() {
    check(&[
        ("new Uint8Array(3)", "Uint8Array(3) [ 0, 0, 0 ]"),
        ("let a = new Uint8Array([1, 256, -1]); a", "Uint8Array(3) [ 1, 0, 255 ]"),
        ("new Int8Array([200])[0]", "-56"),
        ("new Uint8ClampedArray([300, -5, 1.5, 2.5])", "Uint8ClampedArray(4) [ 255, 0, 2, 2 ]"),
        ("new Float32Array([0.1])[0]", "0.10000000149011612"),
        ("new Float64Array([0.1])[0]", "0.1"),
        ("new Uint32Array([-1])[0]", "4294967295"),
        ("let b = new ArrayBuffer(8); let f = new Float64Array(b); f[0] = 1; Array.from(new Uint8Array(b))", "[ 0, 0, 0, 0, 0, 0, 240, 63 ]"),
        ("let b = new ArrayBuffer(8); new Uint16Array(b, 2, 2).length", "2"),
        ("new Uint16Array(new ArrayBuffer(8), 1)", "throws RangeError*"),
        ("let a = new Int16Array(4); a[10] = 5; [a[10], a.length]", "[ undefined, 4 ]"),
        ("let a = new Int32Array([5, 1, 4]); a.sort(); Array.from(a)", "[ 1, 4, 5 ]"),
        ("Array.from(new Uint8Array([1, 2, 3]).map(x => x * 2))", "[ 2, 4, 6 ]"),
        ("new Uint8Array([1, 2, 3]).filter(x => x > 1).length", "2"),
        ("new Uint8Array([1, 2, 3]).reduce((a, b) => a + b)", "6"),
        ("new Uint8Array([1, 2, 3]).join('-')", "'1-2-3'"),
        ("let a = new Uint8Array([1, 2, 3, 4]); let s = a.subarray(1, 3); s[0] = 9; Array.from(a)", "[ 1, 9, 3, 4 ]"),
        ("let a = new Uint8Array([1, 2, 3, 4]); let s = a.slice(1, 3); s[0] = 9; Array.from(a)", "[ 1, 2, 3, 4 ]"),
        ("let a = new Uint8Array(4); a.set([7, 8], 2); Array.from(a)", "[ 0, 0, 7, 8 ]"),
        ("[...new Uint8Array([4, 5])]", "[ 4, 5 ]"),
        ("let t = 0; for (const x of new Float64Array([1.5, 2.5])) t += x; t", "4"),
        ("Uint8Array.BYTES_PER_ELEMENT + Float64Array.BYTES_PER_ELEMENT", "9"),
        ("Object.prototype.toString.call(new Uint8Array(1))", "'[object Uint8Array]'"),
        ("new Uint8Array(2).buffer.byteLength", "2"),
        ("ArrayBuffer.isView(new DataView(new ArrayBuffer(1)))", "true"),
        ("let v = new DataView(new ArrayBuffer(4)); v.setUint16(0, 0x1234); [v.getUint8(0), v.getUint16(0, true)]", "[ 18, 13330 ]"),
        ("let v = new DataView(new ArrayBuffer(8)); v.setFloat64(0, Math.PI, true); v.getFloat64(0, true) === Math.PI", "true"),
        ("new DataView(new ArrayBuffer(2)).getUint32(0)", "throws RangeError*"),
        ("Uint8Array.from([1, 2], x => x * 3)", "Uint8Array(2) [ 3, 6 ]"),
        ("Int16Array.of(1, -2)", "Int16Array(2) [ 1, -2 ]"),
        ("new Uint8Array(new ArrayBuffer(4).slice(1, 3)).length", "2"),
        ("Object.keys(new Uint8Array(2))", "[ '0', '1' ]"),
        ("let a = new Float64Array(100000); for (let i = 0; i < a.length; i++) a[i] = i * 0.5; let s = 0; for (let i = 0; i < a.length; i++) s += a[i]; s", "2499975000"),
        ("new Uint8Array([3, 1]) instanceof Uint8Array", "true"),
        ("Object.getPrototypeOf(Uint8Array) === Object.getPrototypeOf(Int8Array)", "true"),
    ]);
}

#[test]
fn proxies() {
    check(&[
        ("let p = new Proxy({}, { get: (t, k) => 'got ' + String(k) }); p.foo", "'got foo'"),
        ("let log = []; let p = new Proxy({}, { set(t, k, v) { log.push(k + '=' + v); t[k] = v; return true; } }); p.a = 1; p['b'] = 2; [log, p.a]", "[ [ 'a=1', 'b=2' ], 1 ]"),
        ("let p = new Proxy({x: 1}, { has: (t, k) => k === 'magic' }); ['magic' in p, 'x' in p]", "[ true, false ]"),
        ("let p = new Proxy({a: 1, b: 2}, { deleteProperty(t, k) { return k !== 'a' && delete t[k]; } }); [delete p.a, delete p.b, Object.keys(p)]", "[ false, true, [ 'a' ] ]"),
        ("let p = new Proxy({}, { ownKeys: () => ['z', 'y'], getOwnPropertyDescriptor: () => ({ value: 1, enumerable: true, configurable: true }) }); Object.keys(p)", "[ 'z', 'y' ]"),
        ("let target = {v: 5}; let p = new Proxy(target, {}); p.v = 6; [p.v, target.v]", "[ 6, 6 ]"),
        ("let f = new Proxy(function (a) { return a * 2; }, { apply: (t, self, args) => t(...args) + 1 }); [typeof f, f(10)]", "[ 'function', 21 ]"),
        ("class A { constructor(x) { this.x = x; } } let P = new Proxy(A, { construct: (t, args) => new t(args[0] * 10) }); new P(4).x", "40"),
        ("let {proxy, revoke} = Proxy.revocable({}, {}); revoke(); proxy.x", "throws TypeError*"),
        ("Array.isArray(new Proxy([], {}))", "true"),
        ("let p = new Proxy([1, 2, 3], {}); [p.length, p[1], JSON.stringify(p)]", "[ 3, 2, '[1,2,3]' ]"),
        ("let p = new Proxy({}, { getPrototypeOf: () => Array.prototype }); Object.getPrototypeOf(p) === Array.prototype", "true"),
        ("let seen = []; let p = new Proxy({a: 1}, { get(t, k, r) { seen.push(typeof k === 'symbol' ? 'sym' : k); return Reflect.get(t, k, r); } }); p.a; `${p.a}`; seen", "[ 'a', 'a' ]"),
        ("let deps = new Set(); const reactive = o => new Proxy(o, { get(t, k) { deps.add(k); return t[k]; } }); const s = reactive({n: 1, m: 2}); s.n + s.n; [...deps]", "[ 'n' ]"),
        ("let p = new Proxy({}, { defineProperty(t, k, d) { t[k] = 'defined ' + d.value; return true; } }); Object.defineProperty(p, 'q', {value: 1}); p.q", "'defined 1'"),
        ("let p = new Proxy({a: 1}, {}); let ks = []; for (const k in p) ks.push(k); ks", "[ 'a' ]"),
        ("new Proxy(1, {})", "throws TypeError*"),
        ("Reflect.ownKeys(new Proxy({b: 1, a: 2}, {}))", "[ 'b', 'a' ]"),
    ]);
}

#[test]
fn with_statement() {
    check(&[
        ("var o = {a: 1}; with (o) { a + 1 }", "2"),
        ("var o = {a: 1}; var a = 'outer'; with (o) { a = 5; } [o.a, a]", "[ 5, 'outer' ]"),
        ("var o = {}; var b = 'outer'; with (o) { b = 'set'; } [o.b, b]", "[ undefined, 'set' ]"),
        ("function f(obj) { var x = 'local'; with (obj) { return x; } } [f({}), f({x: 'prop'})]", "[ 'local', 'prop' ]"),
        ("var o = {m() { return this === o; }}; with (o) { m() }", "true"),
        ("with ({x: 1}) { with ({y: 2}) { x + y } }", "3"),
        ("with (null) {}", "throws TypeError*"),
        ("'use strict'; with ({}) {}", "throws SyntaxError*"),
        ("var o = {v: 1, [Symbol.unscopables]: {v: true}}; var v = 'global v'; with (o) { v }", "'global v'"),
        ("function tpl(data) { var out = ''; with (data) { out += 'Hi ' + name + '!'; } return out; } tpl({name: 'Ann'})", "'Hi Ann!'"),
        ("new Function('obj', 'with (obj) { return typeof missing + typeof x; }')({x: 1})", "'undefinednumber'"),
    ]);
}

#[test]
fn uri_functions() {
    check(&[
        ("encodeURIComponent('a b&c/d?é€😀')", "'a%20b%26c%2Fd%3F%C3%A9%E2%82%AC%F0%9F%98%80'"),
        ("encodeURI('http://x.y/a b?q=1&r=é#h')", "'http://x.y/a%20b?q=1&r=%C3%A9#h'"),
        ("decodeURIComponent('a%20b%26c%2Fd%3F%C3%A9%E2%82%AC%F0%9F%98%80')", "'a b&c/d?é€😀'"),
        ("decodeURI('a%20b%26c%2F')", "'a b%26c%2F'"),
        ("decodeURIComponent('%')", "throws URIError*"),
        ("decodeURIComponent('%C3')", "throws URIError*"),
        ("encodeURIComponent('\\uD800')", "throws URIError*"),
        ("escape('a b+ü\\u0100')", "'a%20b+%FC%u0100'"),
        ("unescape('a%20b+%FC%u0100%zz')", "'a b+üĀ%zz'"),
    ]);
}

#[test]
fn async_generators() {
    check_async(&[
        (
            "var result; async function* g() { yield 1; yield await Promise.resolve(2); return 3; }
             (async () => { const it = g(); const out = []; for (let i = 0; i < 4; i++) { const r = await it.next(); out.push(r.value + ':' + r.done); } result = out.join(' '); })();",
            "'1:false 2:false 3:true undefined:true'",
        ),
        (
            "var result; async function* g() { for (let i = 0; i < 3; i++) yield i * 10; }
             (async () => { let s = 0; for await (const v of g()) s += v; result = s; })();",
            "30",
        ),
        (
            "var result; (async () => { const out = []; for await (const v of [1, Promise.resolve(2), 3]) out.push(v); result = out.join(); })();",
            "'1,2,3'",
        ),
        (
            "var result; const src = { [Symbol.asyncIterator]() { let i = 0; return { next: () => Promise.resolve({ value: i, done: i++ >= 2 }) }; } };
             (async () => { const out = []; for await (const v of src) out.push(v); result = out.join(); })();",
            "'0,1'",
        ),
        (
            "var result, log = []; async function* g() { try { yield 1; yield 2; } finally { log.push('cleanup'); } }
             (async () => { for await (const v of g()) { log.push(v); break; } result = log.join(); })();",
            "'1,cleanup'",
        ),
        (
            "var result; async function* inner() { yield 'a'; yield 'b'; return 'r'; } async function* outer() { const r = yield* inner(); yield r; yield* ['x', 'y']; }
             (async () => { const out = []; for await (const v of outer()) out.push(v); result = out.join(); })();",
            "'a,b,r,x,y'",
        ),
        (
            "var result; async function* g() { yield 1; yield 2; }
             (async () => { const it = g(); const a = it.next(), b = it.next(), c = it.next();
               const rs = await Promise.all([a, b, c]); result = rs.map(r => r.value + ':' + r.done).join(' '); })();",
            "'1:false 2:false undefined:true'",
        ),
        (
            "var result; async function* g() { try { yield 1; } catch (e) { yield 'caught ' + e; } }
             (async () => { const it = g(); await it.next(); const r = await it.throw('boom'); result = r.value; })();",
            "'caught boom'",
        ),
        (
            "var result; async function* g() { yield 1; throw new Error('bad'); }
             (async () => { const it = g(); await it.next(); try { await it.next(); } catch (e) { result = e.message + ' ' + (await it.next()).done; } })();",
            "'bad true'",
        ),
        (
            "var result; async function* g() { yield 1; }
             (async () => { const it = g(); const r = await it.return(Promise.resolve('early')); result = [r.value, r.done, (await it.next()).done].join(); })();",
            "'early,true,true'",
        ),
        (
            "var result; class C { async *vals() { yield this.n; } constructor() { this.n = 7; } } const o = { async *m() { yield 'o'; } };
             (async () => { const a = []; for await (const v of new C().vals()) a.push(v); for await (const v of o.m()) a.push(v); result = a.join(); })();",
            "'7,o'",
        ),
        (
            "var result; async function* g() {} const it = g();
             result = [Object.prototype.toString.call(it), typeof it[Symbol.asyncIterator], it[Symbol.asyncIterator]() === it, typeof it.next().then].join();",
            "'[object AsyncGenerator],function,true,function'",
        ),
        (
            "var result; async function* g() { return await new Promise(r => setTimeoutLike(r)); } function setTimeoutLike(f) { Promise.resolve().then(() => f('late')); }
             g().next().then(r => result = r.value + ' ' + r.done);",
            "'late true'",
        ),
        (
            "var result; (async () => { try { for await (const v of 5) {} } catch (e) { result = e instanceof TypeError; } })();",
            "true",
        ),
        (
            "var result; async function* g() { const x = yield 1; yield x * 2; }
             (async () => { const it = g(); await it.next(); result = (await it.next(21)).value; })();",
            "42",
        ),
        ("function f() { for await (const x of []) {} }", "throws SyntaxError*"),
    ]);
}

#[test]
fn huge_functions_compile() {
    // More property accesses than inline caches fit in an instruction
    let mut src = String::from("var o = {a: 1, b: 2}, s = 0; (function () {\n");
    for i in 0..70_000 {
        src.push_str(if i % 2 == 0 { "s += o.a;\n" } else { "o.b = s;\n" });
    }
    src.push_str("})(); [s, o.b]");
    assert_eq!(run(&src), "[ 35000, 35000 ]");
}

#[test]
fn class_bindings() {
    check(&[
        // Declarations bind like `let` (decorators reassign them)
        ("{ class A {} A = 1; A }", "1"),
        ("function f() { class B { static k = 2 } B = B.k; return B; } f()", "2"),
        ("class G {} G = 3; G", "3"),
        // The class's own name is immutable inside its body
        ("class C { static m() { C = 1; } } C.m()", "throws TypeError: Assignment to constant variable."),
        ("(class D { static m() { D = 1; } }).m()", "throws TypeError: Assignment to constant variable."),
        ("{ new E(); class E {} }", "throws ReferenceError: Cannot access 'E' before initialization*"),
    ]);
}

#[test]
fn regex_unicode_properties() {
    check(&[
        ("/^\\p{ID_Start}\\p{ID_Continue}*$/u.test('ñame_1')", "true"),
        ("/^\\p{ID_Start}/u.test('1a')", "false"),
        ("'a+b=$5 ©→😀'.match(/\\p{S}/gu).join('')", "'+=$©→😀'"),
        ("'a+b=$5'.match(/\\p{Sc}/gu).join('')", "'$'"),
        ("/\\p{Sm}/u.test('∑')", "true"),
        ("'a\\u0000b\\u200dc\\u3000d'.replace(/[\\p{Cc}\\p{Cf}\\p{Zs}]/gu, '_')", "'a_b_c_d'"),
        ("'(x)-[y]'.replace(/[\\p{Ps}\\p{Pe}\\p{Pd}]/gu, '')", "'xy'"),
        ("'a\\u200bb\\ufe0fc'.replace(/\\p{Default_Ignorable_Code_Point}/gu, '')", "'abc'"),
        ("/^\\p{RI}\\p{RI}$/u.test('🇪🇸')", "true"),
    ]);
}

#[test]
fn bigint() {
    check(&[
        // Literals and arithmetic
        ("typeof 10n", "'bigint'"),
        ("0x1fn + 0o7n + 0b11n", "41n"),
        ("2n ** 64n", "18446744073709551616n"),
        ("(2n ** 100n) / 3n", "422550200076076467165567735125n"),
        ("-7n / 2n", "-3n"),
        ("-7n % 2n", "-1n"),
        ("(-5n) & 3n", "3n"),
        ("(-5n) | 3n", "-5n"),
        ("5n ^ -3n", "-8n"),
        ("~5n", "-6n"),
        ("-9n >> 1n", "-5n"),
        ("1n << 70n", "1180591620717411303424n"),
        ("let x = 9007199254740993n; x++; x", "9007199254740994n"),
        ("let y = 1n; y--; -y", "0n"),
        ("'n=' + 12345678901234567890n", "'n=12345678901234567890'"),
        ("`${-1n}`", "'-1'"),
        // Errors
        ("1n + 1", "throws TypeError: Cannot mix BigInt and other types, use explicit conversions"),
        ("1n / 0n", "throws RangeError: Division by zero"),
        ("2n ** -1n", "throws RangeError*"),
        ("1n >>> 0n", "throws TypeError*"),
        ("+1n", "throws TypeError: Cannot convert a BigInt value to a number"),
        ("Math.max(1n)", "throws TypeError*"),
        ("JSON.stringify({a: 1n})", "throws TypeError: Do not know how to serialize a BigInt"),
        ("new BigInt(1)", "throws TypeError: BigInt is not a constructor"),
        ("BigInt(1.5)", "throws RangeError*"),
        ("BigInt('1.5')", "throws SyntaxError*"),
        ("BigInt(undefined)", "throws TypeError*"),
        // Equality and comparison
        ("[1n === 1n, 1n == 1, 1n == '1', 2n > 1, 1n < 1.5, 2n > '1', 1n == 1.5, 0n == '', 1n < NaN, 10n > 9.99]", "[ true, true, true, true, true, true, false, true, false, true ]"),
        ("[Object.is(0n, -0n), [1n, 2n].includes(2n), 0n ? 'y' : 'n', !!1n]", "[ true, true, 'n', true ]"),
        ("const m = new Map([[10n, 'a']]); m.get(10n) + m.has(BigInt(10)) + new Set([1n, 1n, 2n]).size", "'atrue2'"),
        // Conversions and builtins
        ("[BigInt(42), BigInt('0x10'), BigInt(' -12 '), BigInt(true), BigInt(1e21)]", "[ 42n, 16n, -12n, 1n, 1000000000000000000000n ]"),
        ("[Number(2n ** 64n), Number(-5n), parseInt('12n'), String(7n)]", "[ 18446744073709552000, -5, 12, '7' ]"),
        ("[(255n).toString(16), (-255n).toString(2), (1234567n).toLocaleString(), Object(3n) + 1n]", "[ 'ff', '-11111111', '1,234,567', 4n ]"),
        ("[BigInt.asIntN(8, 255n), BigInt.asUintN(64, -1n), BigInt.asIntN(64, 2n ** 63n)]", "[ -1n, 18446744073709551615n, -9223372036854775808n ]"),
        ("({[10n]: 'k'})['10']", "'k'"),
        ("Object.prototype.toString.call(1n)", "'[object BigInt]'"),
        ("let big = 1n; for (let i = 0; i < 100; i++) big *= 3n; big % 1000000007n", "886041711n"),
    ]);
}

#[test]
fn intl() {
    check(&[
        // Built on first use, then an ordinary property
        ("[typeof Intl, Object.getOwnPropertyDescriptor(globalThis, 'Intl').get === undefined, String(Intl)]", "[ 'object', true, '[object Intl]' ]"),
        ("Intl = 5; Intl", "5"),
        // NumberFormat, DateTimeFormat and Collator work without `new`; the others throw
        ("[Intl.DateTimeFormat().resolvedOptions().timeZone, Intl.NumberFormat('en', { style: 'percent' }).format(0.5), Intl.Collator().compare('a', 'b'), Intl.DateTimeFormat() instanceof Intl.DateTimeFormat, Intl.NumberFormat.name, typeof Intl.Collator.supportedLocalesOf, Intl.NumberFormat.prototype.constructor === Intl.NumberFormat]", "[ 'UTC', '50%', -1, true, 'NumberFormat', 'function', true ]"),
        ("try { Intl.PluralRules(); } catch (e) { e.name }", "'TypeError'"),
        // NumberFormat
        ("new Intl.NumberFormat().format(1234567.891)", "'1,234,567.891'"),
        ("new Intl.NumberFormat('de-DE').format(-0.5)", "'-0.5'"),
        ("new Intl.NumberFormat('en-US', { style: 'currency', currency: 'USD' }).format(1234.5)", "'$1,234.50'"),
        ("new Intl.NumberFormat('en', { style: 'currency', currency: 'JPY' }).format(1234.5)", "'¥1,235'"),
        ("new Intl.NumberFormat('en', { style: 'currency', currency: 'EUR', currencyDisplay: 'code' }).format(3)", "'EUR 3.00'"),
        ("new Intl.NumberFormat('en', { style: 'percent' }).format(0.256)", "'26%'"),
        ("new Intl.NumberFormat('en', { maximumFractionDigits: 1 }).format(2.25)", "'2.3'"),
        ("new Intl.NumberFormat('en', { minimumFractionDigits: 2 }).format(5)", "'5.00'"),
        ("new Intl.NumberFormat('en', { maximumSignificantDigits: 3 }).format(123456)", "'123,000'"),
        ("[1234, 15300, 2500000, 999].map(n => new Intl.NumberFormat('en', { notation: 'compact' }).format(n)).join(' ')", "'1.2K 15K 2.5M 999'"),
        ("new Intl.NumberFormat('en', { notation: 'compact', compactDisplay: 'long' }).format(2500000)", "'2.5 million'"),
        ("new Intl.NumberFormat('en', { signDisplay: 'always' }).format(3)", "'+3'"),
        ("new Intl.NumberFormat('en', { style: 'unit', unit: 'kilometer' }).format(12)", "'12 km'"),
        ("new Intl.NumberFormat().format(12345678901234567890n)", "'12,345,678,901,234,567,890'"),
        ("new Intl.NumberFormat('en', { useGrouping: false }).format(12345)", "'12345'"),
        ("new Intl.NumberFormat().formatToParts(-1234.5).map(p => p.type).join()", "'minusSign,integer,group,integer,decimal,fraction'"),
        ("const f = new Intl.NumberFormat().format; [1, 2000].map(f).join('|')", "'1|2,000'"),
        ("new Intl.NumberFormat('en', { style: 'currency' })", "throws TypeError*"),
        // DateTimeFormat (UTC)
        ("new Intl.DateTimeFormat('en-US').format(Date.UTC(2024, 0, 5))", "'1/5/2024'"),
        ("new Intl.DateTimeFormat('en', { dateStyle: 'medium' }).format(Date.UTC(2024, 6, 4, 15, 30))", "'Jul 4, 2024'"),
        ("new Intl.DateTimeFormat('en', { dateStyle: 'full' }).format(Date.UTC(2024, 6, 4))", "'Thursday, July 4, 2024'"),
        ("new Intl.DateTimeFormat('en', { hour: 'numeric', minute: '2-digit' }).format(Date.UTC(2024, 6, 4, 15, 5))", "'3:05 PM'"),
        ("new Intl.DateTimeFormat('en', { timeStyle: 'short' }).format(Date.UTC(2024, 6, 4, 0, 7))", "'12:07 AM'"),
        ("new Intl.DateTimeFormat('en', { month: 'short', day: 'numeric' }).format(Date.UTC(2024, 11, 25))", "'Dec 25'"),
        ("new Intl.DateTimeFormat('en', { hour: '2-digit', minute: '2-digit', hour12: false }).format(Date.UTC(2024, 0, 1, 9, 3))", "'09:03'"),
        ("new Intl.DateTimeFormat().resolvedOptions().timeZone", "'UTC'"),
        // PluralRules, RelativeTimeFormat, ListFormat
        ("[0, 1, 2].map(n => new Intl.PluralRules('en').select(n)).join()", "'other,one,other'"),
        ("[1, 2, 3, 4, 11, 22].map(n => new Intl.PluralRules('en', { type: 'ordinal' }).select(n)).join()", "'one,two,few,other,other,two'"),
        ("const r = new Intl.RelativeTimeFormat('en'); [r.format(-3, 'day'), r.format(1, 'hours'), r.format(1, 'year')].join('|')", "'3 days ago|in 1 hour|in 1 year'"),
        ("new Intl.RelativeTimeFormat('en', { numeric: 'auto' }).format(-1, 'day')", "'yesterday'"),
        ("new Intl.ListFormat('en').format(['a', 'b', 'c'])", "'a, b, and c'"),
        ("new Intl.ListFormat('en', { type: 'disjunction' }).format(['a', 'b'])", "'a or b'"),
        // Collator, Segmenter, the rest
        ("['b', 'a', 'C'].sort(new Intl.Collator().compare).join('')", "'abC'"),
        ("['item10', 'item2', 'item1'].sort(new Intl.Collator('en', { numeric: true }).compare).join()", "'item1,item2,item10'"),
        ("new Intl.Collator('en', { sensitivity: 'base' }).compare('a', 'Á')", "0"),
        ("[...new Intl.Segmenter().segment('e\\u0301👍🏽🇪🇸x')].map(s => s.segment).length", "4"),
        ("[...new Intl.Segmenter('en', { granularity: 'word' }).segment('Hi, you!')].filter(s => s.isWordLike).map(s => s.segment).join('|')", "'Hi|you'"),
        ("Intl.getCanonicalLocales(['EN-us', 'es-mx', 'en-US'])", "[ 'en-US', 'es-MX' ]"),
        ("new Intl.DisplayNames(['en'], { type: 'region' }).of('ES')", "'Spain'"),
        ("new Intl.Locale('es-Latn-MX').region", "'MX'"),
        ("Intl.getCanonicalLocales('not a locale!')", "throws RangeError*"),
    ]);
}

#[test]
fn normalize_and_locale_compare() {
    check(&[
        ("['Á'.normalize('NFD').length, 'A\\u0301'.normalize() === 'Á', 'ệ'.normalize('NFD').length, 'x'.normalize('NFD')]", "[ 2, true, 3, 'x' ]"),
        ("'a'.normalize('bad')", "throws RangeError*"),
        ("['b', 'a', 'B', 'á', 'A'].sort((x, y) => x.localeCompare(y)).join('')", "'aAábB'"),
        ("['a'.localeCompare('Á', undefined, { sensitivity: 'base' }), 'a'.localeCompare('A', undefined, { sensitivity: 'accent' }), 'a'.localeCompare('A')]", "[ 0, 0, -1 ]"),
        ("'v10'.localeCompare('v9', undefined, { numeric: true })", "1"),
    ]);
}

/// Weak references let go of their targets once nothing else holds them
#[test]
fn weak_references() {
    let mut vm = Vm::new();
    let eval = |vm: &mut Vm, src: &str| -> String {
        match vm.eval(src) {
            Ok(v) => vm.display(v),
            Err(e) => format!("throws {}", vm.display(e)),
        }
    };
    let setup = r#"
        var log = [];
        var registry = new FinalizationRegistry(held => log.push(held));
        var kept = { name: 'kept' };
        var refKept = new WeakRef(kept);
        var refLost = new WeakRef({ name: 'lost' });
        (function () {
            const a = {}, b = {}, c = {};
            registry.register(a, 'a');
            registry.register(b, 'b', b);
            registry.register(c, 'c', kept);
            registry.unregister(kept);
        })();
        registry.register(kept, 'never');
        // An ephemeron: the value refers back to its key, and nothing else
        // holds either; the map must not keep both alive
        var wm = new WeakMap();
        var probe = new WeakRef((() => { const k = {}; wm.set(k, { k }); return k; })());
        // A value reachable only through a live key stays
        var liveKey = {};
        wm.set(liveKey, { tag: 'value' });
        var valueRef = new WeakRef(wm.get(liveKey));
        var sym = Symbol('weak');
        var symRef = new WeakRef(sym);
        typeof refLost.deref()
    "#;
    // A new WeakRef's target survives the script (until the job queue,
    // drained after each script, is empty)
    assert_eq!(eval(&mut vm, setup), "object");
    vm.collect_garbage();
    vm.run_jobs();
    assert_eq!(eval(&mut vm, "[refKept.deref() === kept, refLost.deref(), probe.deref(), valueRef.deref().tag, symRef.deref() === sym]"), "[ true, undefined, undefined, 'value', true ]");
    // The cleanup callback ran for a and b, but not c (unregistered) or kept
    assert_eq!(eval(&mut vm, "log.sort().join()"), "a,b");
    let errors = [
        ("new WeakRef(1)", "throws TypeError: WeakRef: target must be an object or non-registered symbol"),
        ("new WeakRef(Symbol.for('x'))", "throws TypeError: WeakRef: target must be an object or non-registered symbol"),
        ("WeakRef({})", "throws TypeError: Constructor requires 'new'"),
        ("new FinalizationRegistry(1)", "throws TypeError: FinalizationRegistry: cleanup must be callable"),
        ("var o = {}; registry.register(o, o)", "throws TypeError: FinalizationRegistry.prototype.register: target and holdings must not be same"),
        ("registry.unregister(1)", "throws TypeError: Invalid unregisterToken ('1')"),
        ("registry.register({}, 1, 2)", "throws TypeError: FinalizationRegistry.prototype.register: invalid unregister token"),
        ("[registry.register({}, 1), registry.unregister({})]", "[ undefined, false ]"),
        ("Object.prototype.toString.call(refKept) + Object.prototype.toString.call(registry)", "[object WeakRef][object FinalizationRegistry]"),
        ("var s = Symbol(); new WeakMap([[s, 1]]).get(s)", "1"),
        ("new WeakSet().add(Symbol.for('y'))", "throws TypeError: Invalid value used as weak map key"),
    ];
    for (src, want) in errors {
        assert_eq!(eval(&mut vm, src), want, "{src}");
    }
}

/// Stack traces name each frame's function and source location
#[test]
fn stack_trace_locations() {
    let mut vm = Vm::new();
    let src = "function outer() {\n  return inner();\n}\nfunction inner() {\n  const o = {};\n  return o.missing.deep;\n}\ntry { outer(); } catch (e) { e.stack }";
    let stack = vm.eval_named(src, "https://example.com/app.js").map(|v| vm.display(v)).unwrap();
    assert_eq!(
        stack,
        "TypeError: Cannot read properties of undefined (reading 'deep')\n    at inner (https://example.com/app.js:6:20)\n    at outer (https://example.com/app.js:2:10)\n    at https://example.com/app.js:8:7"
    );
    // Errors made by `new Error` and `throw`, calls of non-functions, and
    // lazily compiled functions on one long line
    let src = "var f = () => { throw new Error('x'); }; var g = function named() { undefinedFn(); }; var h = () => { ({}).nope(); };\nvar r = [];\nfor (const fn of [f, g, h]) { try { fn(); } catch (e) { r.push(e.stack.split('\\n')[1]); } }\nr.join('|')";
    let got = vm.eval_named(src, "t.js").map(|v| vm.display(v)).unwrap();
    assert_eq!(got, "    at f (t.js:1:23)|    at named (t.js:1:69)|    at h (t.js:1:108)");
    // Syntax errors point at the offending token
    let err = vm.eval_named("let a = 1;\nlet b = ;", "bad.js").unwrap_err();
    let stack = vm.eval_named("x => x.stack", "").and_then(|f| vm.call(f, fos_jsvm::Value::UNDEFINED, &[err])).map(|v| vm.display(v)).unwrap();
    assert_eq!(stack, "SyntaxError: unexpected ';': expected expression\n    at bad.js:2:9");
    // The stack is formatted when first read, from the message then; it
    // can be replaced; captureStackTrace leaves out the constructor's frames
    let src = r#"
        function MyError(msg) { this.message = msg; Error.captureStackTrace(this, MyError); }
        function make() { return new MyError('custom'); }
        var e = new Error('first');
        var before = Object.getOwnPropertyNames(e).includes('stack');
        e.message = 'second';
        var head = e.stack.split('\n')[0];
        var replaced = new TypeError('t'); replaced.stack = 'mine';
        [before, Object.getOwnPropertyNames(e).includes('stack'), head, replaced.stack, make().stack.split('\n').slice(1).join('|'),
         Error.stackTraceLimit, typeof Object.getOwnPropertyDescriptor(Error.prototype, 'stack').get, Object.create(Error.prototype).stack]
    "#;
    let got = vm.eval_named(src, "c.js").map(|v| vm.display(v)).unwrap();
    assert_eq!(got, "[ false, true, 'Error: second', 'mine', '    at make (c.js:3:34)|    at c.js:9:89', 10, 'function', undefined ]");
    // A getter's caller is located at the property access
    let src = "var o = { get g() { return new Error('g').stack; } };\nfunction f() { return [1, o.g][1]; }\nf().split('\\n').slice(1, 3).join('|')";
    let got = vm.eval_named(src, "g.js").map(|v| vm.display(v)).unwrap();
    assert_eq!(got, "    at get g (g.js:1:28)|    at f (g.js:2:29)");
}

/// Functions take their names from computed keys and accessor kinds
#[test]
fn computed_function_names() {
    check(&[
        ("const n = 'dyn'; ({ [n]: function () {} })[n].name", "'dyn'"),
        ("({ [Symbol.iterator]: () => 0 })[Symbol.iterator].name", "'[Symbol.iterator]'"),
        ("({ [Symbol()]: () => 0 })[Object.getOwnPropertySymbols({ [Symbol()]: 1 }).length - 1] === undefined", "true"),
        ("const s = Symbol(); Object.getOwnPropertyDescriptor({ [s]() {} }, s).value.name", "''"),
        ("({ [1 + 1]() {} })[2].name", "'2'"),
        ("({ ['m' + 1]() {} }).m1.name", "'m1'"),
        ("Object.getOwnPropertyDescriptor({ get ['a' + 'b']() { return 1; } }, 'ab').get.name", "'get ab'"),
        ("Object.getOwnPropertyDescriptor({ get x() { return 1; }, set x(v) {} }, 'x').set.name", "'set x'"),
        ("Object.getOwnPropertyDescriptor(class { static get y() { return 1; } }, 'y').get.name", "'get y'"),
        ("class A { [Symbol.toPrimitive]() {} } A.prototype[Symbol.toPrimitive].name", "'[Symbol.toPrimitive]'"),
        ("const k = 'c'; ({ [k]: class {} }).c.name", "'c'"),
        ("const k2 = 'c'; ({ [k2]: class { static name() {} } }).c.name.call === Function.prototype.call", "true"),
        ("const k3 = 'f'; ({ [k3]: function named() {} }).f.name", "'named'"),
    ]);
}

/// Redefining a property with a partial descriptor keeps what it omits
#[test]
fn partial_property_redefinition() {
    check(&[
        // React's input value tracker
        ("var o = {}, cur; Object.defineProperty(o, 'v', { configurable: true, get() { return 1; }, set(x) { cur = x; } }); Object.defineProperty(o, 'v', { enumerable: false }); (function () { 'use strict'; o.v = 5; })(); var d = Object.getOwnPropertyDescriptor(o, 'v'); [cur, typeof d.get, typeof d.set, 'value' in d, d.configurable]", "[ 5, 'function', 'function', false, true ]"),
        ("var o = {}; Object.defineProperty(o, 'v', { configurable: true, get() { return 1; }, set(x) {} }); Object.defineProperty(o, 'v', { get() { return 2; } }); [o.v, typeof Object.getOwnPropertyDescriptor(o, 'v').set]", "[ 2, 'function' ]"),
        ("var o = { v: 1 }; Object.defineProperty(o, 'v', { enumerable: false }); var d = Object.getOwnPropertyDescriptor(o, 'v'); [d.value, d.writable, d.enumerable]", "[ 1, true, false ]"),
        ("var o = {}; Object.defineProperty(o, 'v', { configurable: true, get() { return 1; } }); Object.defineProperty(o, 'v', { value: 3 }); var d = Object.getOwnPropertyDescriptor(o, 'v'); [d.value, d.writable, 'get' in d]", "[ 3, false, false ]"),
        ("var o = {}; Object.defineProperty(o, 'v', { get() { return 1; } }); try { Object.defineProperty(o, 'v', { enumerable: true }); 'no' } catch (e) { e.constructor.name }", "'TypeError'"),
    ]);
}

/// Frozen arrays, and arrays whose length is read-only
#[test]
fn frozen_arrays() {
    check(&[
        // DOMPurify's addToSet only writes to arrays that are not frozen
        ("[Object.isFrozen(Object.freeze(['A'])), Object.isFrozen(Object.freeze([])), Object.isFrozen(Object.seal([1])), Object.isSealed(Object.freeze([1]))]", "[ true, true, false, true ]"),
        ("Object.getOwnPropertyDescriptor(Object.freeze([1]), 'length').writable", "false"),
        ("var a = Object.freeze([1, 2]); a.length = 0; a[5] = 1; a[0] = 9; [a.length, a[0], a[5]]", "[ 2, 1, undefined ]"),
        ("var a = Object.freeze([1]); [() => a.push(2), () => a.pop(), () => a.shift(), () => a.unshift(0), () => a.splice(0, 1)].map(f => { try { f(); return 'ok'; } catch (e) { return e.constructor.name; } })", "[ 'TypeError', 'TypeError', 'TypeError', 'TypeError', 'TypeError' ]"),
        ("var b = [1, 2, 3]; Object.defineProperty(b, 'length', { writable: false }); var r = []; try { b.push(4); } catch (e) { r.push(e.constructor.name); } b[0] = 7; b[3] = 1; [r[0], b.length, b[0], b[3], Object.isFrozen(b)]", "[ 'TypeError', 3, 7, undefined, false ]"),
        ("(function () { 'use strict'; var a = Object.freeze([1]); try { a.length = 0; return 'no'; } catch (e) { return e.constructor.name; } })()", "'TypeError'"),
        ("var b = [1]; Object.defineProperty(b, 'length', { writable: false }); try { Object.defineProperty(b, 'length', { writable: true }); 'no' } catch (e) { e.constructor.name }", "'TypeError'"),
    ]);
}

/// instanceof and isPrototypeOf see a proxy's [[GetPrototypeOf]]
#[test]
fn proxy_prototype_chain() {
    check(&[
        // Function.prototype[@@hasInstance], fixed and reachable (as Symbol[Symbol.hasInstance])
        ("const d = Object.getOwnPropertyDescriptor(Function.prototype, Symbol.hasInstance); [typeof d.value, d.writable, d.configurable, Symbol[Symbol.hasInstance].call(Array, []), Function.prototype[Symbol.hasInstance].call({}, []), [] instanceof Array, ({ [Symbol.hasInstance]: () => true }) instanceof Object]", "[ 'function', false, false, true, false, true, true ]"),
        ("class A { static [Symbol.hasInstance](v) { return v === 1; } } [1 instanceof A, new A() instanceof A]", "[ true, false ]"),
        ("class A {} [new Proxy(new A(), {}) instanceof A, A.prototype.isPrototypeOf(new Proxy(new A(), {}))]", "[ true, true ]"),
        ("class B {} new Proxy({}, { getPrototypeOf() { return B.prototype; } }) instanceof B", "true"),
        ("class C {} const p = new Proxy(Object.create(C.prototype), {}); Object.create(p) instanceof C", "true"),
        ("var log = []; class D {} new Proxy({}, { getPrototypeOf(t) { log.push('trap'); return null; } }) instanceof D; log.join()", "'trap'"),
        // Prototype and extensibility operations reach the target or the traps
        ("function C() {} var t = {}; Object.setPrototypeOf(new Proxy(t, {}), C.prototype); Object.getPrototypeOf(t) === C.prototype", "true"),
        ("var log = []; var p = new Proxy({}, { setPrototypeOf(t, v) { log.push('set'); return Reflect.setPrototypeOf(t, v); }, preventExtensions(t) { log.push('pe'); return Reflect.preventExtensions(t); }, isExtensible(t) { log.push('ie'); return Reflect.isExtensible(t); } }); Object.setPrototypeOf(p, null); Object.preventExtensions(p); [Object.isExtensible(p), log.join()]", "[ false, 'set,pe,ie' ]"),
        ("var t = {}; Object.preventExtensions(new Proxy(t, {})); [Object.isExtensible(t), Reflect.preventExtensions(new Proxy({}, { preventExtensions() { return false; } }))]", "[ false, false ]"),
        ("try { Object.preventExtensions(new Proxy({}, { preventExtensions() { return false; } })); 'no' } catch (e) { e.constructor.name }", "'TypeError'"),
        ("var t = { a: 1, get g() { return 1; } }; var p = Object.freeze(new Proxy(t, {})); [Object.isFrozen(t), Object.isFrozen(p), Object.isSealed(p), Object.getOwnPropertyDescriptor(t, 'a').writable]", "[ true, true, true, false ]"),
        ("var t = { a: 1 }; Object.seal(new Proxy(t, {})); [Object.isSealed(t), Object.isFrozen(t), Object.isFrozen(new Proxy({}, {}))]", "[ true, false, false ]"),
    ]);
}
