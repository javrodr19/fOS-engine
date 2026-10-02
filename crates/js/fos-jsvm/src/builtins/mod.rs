//! Built-in objects

pub(crate) mod array;
mod bigint;
mod intl;
mod collections;
mod weakref;
mod date;
mod error;
mod function;
pub(crate) mod generator;
pub(crate) mod promise;
pub(crate) mod json;
mod math;
mod number;
mod object;
pub(crate) mod proxy;
mod reflect;
pub(crate) mod regexp;
mod string;
mod symbol;
mod uri;
pub mod typedarray;

use crate::gc::Gc;
use crate::object::*;
use crate::string::{JsString, atoms};
use crate::value::Value;
use crate::vm::{JsResult, NativeFn, Vm};

pub(crate) fn init(vm: &mut Vm) {
    object::init(vm);
    function::init(vm);
    error::init(vm);
    symbol::init(vm);
    array::init(vm);
    string::init(vm);
    number::init(vm);
    bigint::init(vm);
    math::init(vm);
    json::init(vm);
    collections::init(vm);
    weakref::init(vm);
    reflect::init(vm);
    proxy::init(vm);
    regexp::init(vm);
    promise::init(vm);
    date::init(vm);
    typedarray::init(vm);
    generator::init(vm);
    uri::init(vm);
    intl::init(vm);

    let g = vm.global;
    vm.def_value(g, "globalThis", Value::object(g), PropFlags::HIDDEN);
    vm.def_value(g, "NaN", Value::NAN, PropFlags::FROZEN);
    vm.def_value(g, "Infinity", Value::number(f64::INFINITY), PropFlags::FROZEN);
    vm.def_value(g, "undefined", Value::UNDEFINED, PropFlags::FROZEN);
    vm.def_method(g, "parseInt", 2, number::parse_int);
    vm.def_method(g, "parseFloat", 1, number::parse_float);
    vm.def_method(g, "isNaN", 1, global_is_nan);
    vm.def_method(g, "isFinite", 1, global_is_finite);
    vm.def_method(g, "eval", 1, global_eval);
    vm.def_method(g, "queueMicrotask", 1, queue_microtask);

    let console = vm.new_object();
    for name in ["log", "info", "warn", "error", "debug", "trace"] {
        vm.def_method(console, name, 0, console_log);
    }
    vm.def_value(g, "console", Value::object(console), PropFlags::HIDDEN);

    vm.temp_roots.clear();
}

#[inline]
pub fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).copied().unwrap_or(Value::UNDEFINED)
}

impl Vm {
    pub fn def_value(&mut self, obj: Gc<JsObject>, name: &str, v: Value, flags: PropFlags) {
        let key = self.key_from_str(name);
        self.define_value(obj, key, v, flags);
    }

    pub fn def_method(&mut self, obj: Gc<JsObject>, name: &str, length: u32, f: NativeFn) -> Gc<JsObject> {
        let func = self.new_native(name, length, f, None);
        self.def_value(obj, name, Value::object(func), PropFlags::HIDDEN);
        func
    }

    pub fn def_method_sym(&mut self, obj: Gc<JsObject>, sym: Gc<Symbol>, name: &str, length: u32, f: NativeFn) -> Gc<JsObject> {
        let func = self.new_native(name, length, f, None);
        self.define_value(obj, PropertyKey::Symbol(sym), Value::object(func), PropFlags::HIDDEN);
        func
    }

    pub fn def_getter(&mut self, obj: Gc<JsObject>, name: &str, f: NativeFn) {
        let func = self.new_native(&format!("get {name}"), 0, f, None);
        let key = self.key_from_str(name);
        self.define_accessor(obj, key, Some(Value::object(func)), None, PropFlags(PropFlags::CONFIGURABLE));
    }

    /// An accessor property with native getter and optional setter
    pub fn def_accessor(&mut self, obj: Gc<JsObject>, name: &str, get: NativeFn, set: Option<NativeFn>) {
        let g = self.new_native(&format!("get {name}"), 0, get, None);
        let s = set.map(|f| Value::object(self.new_native(&format!("set {name}"), 1, f, None)));
        let key = self.key_from_str(name);
        self.define_accessor(obj, key, Some(Value::object(g)), s, PropFlags(PropFlags::CONFIGURABLE));
    }

    /// A constructor with its prototype object, installed as a global
    pub fn def_ctor(&mut self, name: &str, length: u32, call: NativeFn, construct: Option<NativeFn>, proto: Gc<JsObject>) -> Gc<JsObject> {
        let c = self.new_native(name, length, call, construct);
        self.define_value(c, PropertyKey::Atom(atoms::prototype), Value::object(proto), PropFlags::FROZEN);
        self.define_value(proto, PropertyKey::Atom(atoms::constructor), Value::object(c), PropFlags::HIDDEN);
        let g = self.global;
        self.def_value(g, name, Value::object(c), PropFlags::HIDDEN);
        c
    }

    /// RequireObjectCoercible(this) then ToString
    pub(crate) fn this_string(&mut self, this: Value, method: &str) -> JsResult<Gc<JsString>> {
        if this.is_nullish() {
            return Err(self.type_error(&format!("String.prototype.{method} called on null or undefined")));
        }
        self.to_string(this)
    }

    /// Call `f` for its value without keeping temporaries of earlier
    /// iterations alive (loops over many callbacks)
    pub fn temp_mark(&self) -> usize {
        self.temp_roots.len()
    }

    pub fn temp_reset(&mut self, mark: usize) {
        self.temp_roots.truncate(mark);
    }

    /// Readable representation (console.log)
    pub fn display(&mut self, v: Value) -> String {
        let mut out = String::new();
        self.display_into(v, &mut out, 0, true);
        out
    }

    fn display_into(&mut self, v: Value, out: &mut String, depth: usize, top: bool) {
        if let Some(s) = v.as_string() {
            let s = s.get().to_rust_string();
            if top {
                out.push_str(&s);
            } else {
                out.push('\'');
                out.push_str(&s.replace('\'', "\\'"));
                out.push('\'');
            }
            return;
        }
        if let Some(n) = v.as_number() {
            if n == 0.0 && n.is_sign_negative() {
                out.push_str("-0");
            } else {
                out.push_str(&crate::number::number_to_string(n));
            }
            return;
        }
        if let Some(b) = v.as_bigint() {
            out.push_str(&b.get().to_string_radix(10));
            out.push('n');
            return;
        }
        if let Some(s) = v.as_symbol() {
            match s.get().description {
                Some(d) => out.push_str(&format!("Symbol({})", d.get().to_rust_string())),
                None => out.push_str("Symbol()"),
            }
            return;
        }
        let Some(o) = v.as_object() else {
            out.push_str(&format!("{v:?}"));
            return;
        };
        if o.get().is_callable() {
            let name = self.get(v, PropertyKey::Atom(atoms::name)).ok().and_then(|n| n.as_string()).map(|s| s.get().to_rust_string());
            let is_class = matches!(&o.get().kind, ObjectKind::Function(c) if c.proto.is_class_constructor);
            let kind = if is_class { "class" } else { "Function" };
            match name {
                Some(n) if !n.is_empty() => out.push_str(&format!("[{kind}: {n}]")),
                _ => out.push_str(&format!("[{kind} (anonymous)]")),
            }
            return;
        }
        if matches!(o.get().kind, ObjectKind::Error(_)) {
            let s = error::error_to_string(self, v).unwrap_or_default();
            out.push_str(&s);
            return;
        }
        if depth > 2 {
            out.push_str(if o.get().is_array() { "[Array]" } else { "[Object]" });
            return;
        }
        match &o.get().kind {
            ObjectKind::Array { length } => {
                let len = *length;
                if len == 0 {
                    out.push_str("[]");
                    return;
                }
                out.push_str("[ ");
                for i in 0..len.min(100) {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    match o.get().elements.get(i as usize) {
                        Some(&e) if !e.is_hole() => self.display_into(e, out, depth + 1, false),
                        _ => out.push_str("<empty>"),
                    }
                }
                if len > 100 {
                    out.push_str(&format!(", ... {} more items", len - 100));
                }
                out.push_str(" ]");
                return;
            }
            ObjectKind::Map(m) | ObjectKind::Set(m) => {
                let is_map = matches!(o.get().kind, ObjectKind::Map(_));
                let entries: Vec<(Value, Value)> = m.entries.iter().flatten().copied().collect();
                out.push_str(if is_map { "Map(" } else { "Set(" });
                out.push_str(&format!("{}) {{", entries.len()));
                for (i, (k, val)) in entries.into_iter().enumerate() {
                    out.push_str(if i == 0 { " " } else { ", " });
                    self.display_into(k, out, depth + 1, false);
                    if is_map {
                        out.push_str(" => ");
                        self.display_into(val, out, depth + 1, false);
                    }
                }
                out.push_str(" }");
                return;
            }
            ObjectKind::TypedArray(t) => {
                let (name, len) = (t.kind.name(), t.length);
                out.push_str(&format!("{name}({len}) ["));
                for i in 0..len.min(100) {
                    out.push_str(if i == 0 { " " } else { ", " });
                    let v = typedarray::ta_get(o.get(), i).unwrap_or(Value::UNDEFINED);
                    self.display_into(v, out, depth + 1, false);
                }
                out.push_str(if len == 0 { "]" } else { " ]" });
                return;
            }
            ObjectKind::ArrayBuffer(b) => {
                out.push_str(&format!("ArrayBuffer {{ byteLength: {} }}", b.len()));
                return;
            }
            ObjectKind::String(s) => {
                out.push_str(&format!("[String: '{}']", s.get().to_rust_string()));
                return;
            }
            ObjectKind::Number(n) => {
                out.push_str(&format!("[Number: {}]", crate::number::number_to_string(*n)));
                return;
            }
            ObjectKind::Boolean(b) => {
                out.push_str(&format!("[Boolean: {b}]"));
                return;
            }
            _ => {}
        }
        let keys = self.enumerable_own_keys(o);
        if keys.is_empty() {
            out.push_str("{}");
            return;
        }
        out.push_str("{ ");
        for (i, k) in keys.into_iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            let name = self.key_display(k);
            let plain = name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') && !name.starts_with(|c: char| c.is_ascii_digit());
            if plain || matches!(k, PropertyKey::Index(_)) {
                out.push_str(&name);
            } else {
                out.push_str(&format!("'{name}'"));
            }
            out.push_str(": ");
            let val = self.get(v, k).unwrap_or(Value::UNDEFINED);
            self.display_into(val, out, depth + 1, false);
        }
        out.push_str(" }");
    }
}

fn console_log(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let parts: Vec<String> = args.iter().map(|&a| vm.display(a)).collect();
    let line = parts.join(" ");
    (vm.print)(&line);
    Ok(Value::UNDEFINED)
}

fn global_is_nan(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(vm.to_number(arg(args, 0))?.is_nan()))
}

fn global_is_finite(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(vm.to_number(arg(args, 0))?.is_finite()))
}

/// Indirect eval: runs in the global scope
fn global_eval(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let src = arg(args, 0);
    let Some(s) = src.as_string() else { return Ok(src) };
    let text = s.get().to_rust_string();
    vm.eval(&text)
}

fn queue_microtask(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let f = arg(args, 0);
    if !vm.is_callable(f) {
        return Err(vm.type_error("queueMicrotask requires a function"));
    }
    vm.jobs.push_back(crate::vm::Job::Call(f, Value::UNDEFINED));
    Ok(Value::UNDEFINED)
}
