//! String

use crate::gc::Gc;
use crate::object::*;
use crate::string::{JsString, Units, atoms};
use crate::value::Value;
use crate::vm::{JsResult, NativeFn, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.string_proto;
    let c = vm.def_ctor("String", 1, string_call, Some(string_construct), proto);
    vm.def_method(c, "fromCharCode", 1, from_char_code);
    vm.def_method(c, "fromCodePoint", 1, from_code_point);
    vm.def_method(c, "raw", 1, raw);
    let methods: &[(&str, u32, NativeFn)] = &[
        ("toString", 0, value_of),
        ("valueOf", 0, value_of),
        ("charAt", 1, char_at),
        ("charCodeAt", 1, char_code_at),
        ("codePointAt", 1, code_point_at),
        ("at", 1, at),
        ("indexOf", 1, index_of),
        ("lastIndexOf", 1, last_index_of),
        ("includes", 1, includes),
        ("startsWith", 1, starts_with),
        ("endsWith", 1, ends_with),
        ("slice", 2, slice),
        ("substring", 2, substring),
        ("substr", 2, substr),
        ("toUpperCase", 0, to_upper_case),
        ("toLowerCase", 0, to_lower_case),
        ("toLocaleUpperCase", 0, to_upper_case),
        ("toLocaleLowerCase", 0, to_lower_case),
        ("trim", 0, trim),
        ("trimStart", 0, trim_start),
        ("trimEnd", 0, trim_end),
        ("padStart", 2, pad_start),
        ("padEnd", 2, pad_end),
        ("repeat", 1, repeat),
        ("concat", 1, concat),
        ("localeCompare", 1, locale_compare),
        ("normalize", 0, normalize),
        ("split", 2, split),
        ("replace", 2, replace),
        ("replaceAll", 2, replace_all),
        ("match", 1, super::regexp::string_match),
        ("matchAll", 1, super::regexp::string_match_all),
        ("search", 1, super::regexp::string_search),
    ];
    for &(name, len, f) in methods {
        vm.def_method(proto, name, len, f);
    }
    let it = vm.sym.iterator;
    vm.def_method_sym(proto, it, "[Symbol.iterator]", 0, iterator);
    // `trimLeft`/`trimRight` are aliases
    let tl = vm.get_str(Value::object(proto), "trimStart").unwrap();
    vm.def_value(proto, "trimLeft", tl, PropFlags::HIDDEN);
    let tr = vm.get_str(Value::object(proto), "trimEnd").unwrap();
    vm.def_value(proto, "trimRight", tr, PropFlags::HIDDEN);
}

fn string_call(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if args.is_empty() {
        return Ok(vm.atom_value(atoms::empty));
    }
    if let Some(s) = args[0].as_symbol() {
        let text = super::symbol::symbol_descriptive_string(s);
        return Ok(vm.str_value(&text));
    }
    Ok(Value::string(vm.to_string(args[0])?))
}

fn string_construct(vm: &mut Vm, new_target: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = if args.is_empty() { vm.atoms.string(atoms::empty) } else { vm.to_string(args[0])? };
    let proto = vm.prototype_for(new_target, |r| r.string_proto)?;
    Ok(Value::object(vm.new_object_with(Some(proto), ObjectKind::String(s))))
}

fn this_str(vm: &mut Vm, this: Value, name: &str) -> JsResult<Gc<JsString>> {
    vm.this_string(this, name)
}

fn value_of(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if this.is_string() {
        return Ok(this);
    }
    if let Some(o) = this.as_object() {
        if let ObjectKind::String(s) = o.get().kind {
            return Ok(Value::string(s));
        }
    }
    Err(vm.type_error("String.prototype.valueOf requires that 'this' be a String"))
}

fn from_char_code(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let mut units = Vec::with_capacity(args.len());
    for &a in args {
        units.push(vm.to_uint32(a)? as u16);
    }
    Ok(Value::string(vm.new_string_units(&units)))
}

fn from_code_point(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let mut units = Vec::with_capacity(args.len());
    for &a in args {
        let n = vm.to_number(a)?;
        if n.fract() != 0.0 || !(0.0..=1114111.0).contains(&n) {
            return Err(vm.range_error(&format!("Invalid code point {n}")));
        }
        let cp = n as u32;
        if cp < 0x10000 {
            units.push(cp as u16);
        } else {
            let c = cp - 0x10000;
            units.push(0xD800 + (c >> 10) as u16);
            units.push(0xDC00 + (c & 0x3FF) as u16);
        }
    }
    Ok(Value::string(vm.new_string_units(&units)))
}

fn raw(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let strings = vm.to_object(arg(args, 0))?;
    let raw = vm.get(Value::object(strings), PropertyKey::Atom(atoms::raw))?;
    let raw_o = vm.to_object(raw)?;
    let len = vm.get(Value::object(raw_o), PropertyKey::Atom(atoms::length))?;
    let len = vm.to_length(len)? as usize;
    let mut units = Vec::new();
    for i in 0..len {
        let seg = vm.get(Value::object(raw_o), PropertyKey::Index(i as u32))?;
        let s = vm.to_string(seg)?;
        units.extend(s.get().units().iter());
        if i + 1 < len && i + 1 < args.len() {
            let sub = vm.to_string(args[i + 1])?;
            units.extend(sub.get().units().iter());
        }
    }
    Ok(Value::string(vm.new_string_units(&units)))
}

fn char_at(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "charAt")?;
    let i = vm.to_integer(arg(args, 0))?;
    if i < 0.0 || i >= s.get().len() as f64 {
        return Ok(vm.atom_value(atoms::empty));
    }
    let u = s.get().code_unit_at(i as u32).unwrap();
    Ok(Value::string(vm.new_string_units(&[u])))
}

fn char_code_at(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "charCodeAt")?;
    let a = arg(args, 0);
    let i = match a.as_int() {
        Some(i) => i as f64,
        None => vm.to_integer(a)?,
    };
    if i < 0.0 || i >= s.get().len() as f64 {
        return Ok(Value::NAN);
    }
    Ok(Value::int(s.get().code_unit_at(i as u32).unwrap() as i32))
}

fn code_point_at(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "codePointAt")?;
    let i = vm.to_integer(arg(args, 0))?;
    let st = s.get();
    if i < 0.0 || i >= st.len() as f64 {
        return Ok(Value::UNDEFINED);
    }
    let i = i as u32;
    let u = st.code_unit_at(i).unwrap();
    if (0xD800..0xDC00).contains(&u) {
        if let Some(u2) = st.code_unit_at(i + 1) {
            if (0xDC00..0xE000).contains(&u2) {
                return Ok(Value::int(0x10000 + (((u as i32) - 0xD800) << 10) + (u2 as i32 - 0xDC00)));
            }
        }
    }
    Ok(Value::int(u as i32))
}

fn at(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "at")?;
    let len = s.get().len() as f64;
    let n = vm.to_integer(arg(args, 0))?;
    let k = if n < 0.0 { len + n } else { n };
    if k < 0.0 || k >= len {
        return Ok(Value::UNDEFINED);
    }
    let u = s.get().code_unit_at(k as u32).unwrap();
    Ok(Value::string(vm.new_string_units(&[u])))
}

/// Position of `needle` in `hay` at or after `from`
pub(crate) fn find_units(hay: &Units, needle: &Units, from: usize) -> Option<usize> {
    let (n, m) = (hay.len(), needle.len());
    if m == 0 {
        return if from <= n { Some(from) } else { None };
    }
    if m > n {
        return None;
    }
    if let (Units::Latin1(h), Units::Latin1(nd)) = (hay, needle) {
        return h[from.min(n)..].windows(m).position(|w| w == *nd).map(|p| p + from);
    }
    let first = needle.at(0);
    let mut i = from;
    while i + m <= n {
        if hay.at(i) == first && (1..m).all(|j| hay.at(i + j) == needle.at(j)) {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn rfind_units(hay: &Units, needle: &Units, from: usize) -> Option<usize> {
    let (n, m) = (hay.len(), needle.len());
    if m > n {
        return None;
    }
    let mut i = from.min(n - m);
    loop {
        if (0..m).all(|j| hay.at(i + j) == needle.at(j)) {
            return Some(i);
        }
        if i == 0 {
            return None;
        }
        i -= 1;
    }
}

fn index_of(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "indexOf")?;
    let needle = vm.to_string(arg(args, 0))?;
    let from = vm.to_integer(arg(args, 1))?.clamp(0.0, s.get().len() as f64) as usize;
    let r = find_units(&s.get().units(), &needle.get().units(), from);
    Ok(Value::number(r.map(|p| p as f64).unwrap_or(-1.0)))
}

fn last_index_of(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "lastIndexOf")?;
    let needle = vm.to_string(arg(args, 0))?;
    let n = vm.to_number(arg(args, 1))?;
    let from = if n.is_nan() { usize::MAX } else { n.max(0.0) as usize };
    let r = rfind_units(&s.get().units(), &needle.get().units(), from);
    Ok(Value::number(r.map(|p| p as f64).unwrap_or(-1.0)))
}

fn not_regexp(vm: &mut Vm, v: Value, method: &str) -> JsResult<()> {
    if v.as_object().is_some_and(|o| super::regexp::is_regexp(o)) {
        return Err(vm.type_error(&format!("First argument to String.prototype.{method} must not be a regular expression")));
    }
    Ok(())
}

fn includes(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "includes")?;
    not_regexp(vm, arg(args, 0), "includes")?;
    let needle = vm.to_string(arg(args, 0))?;
    let from = vm.to_integer(arg(args, 1))?.clamp(0.0, s.get().len() as f64) as usize;
    Ok(Value::bool(find_units(&s.get().units(), &needle.get().units(), from).is_some()))
}

fn starts_with(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "startsWith")?;
    not_regexp(vm, arg(args, 0), "startsWith")?;
    let needle = vm.to_string(arg(args, 0))?;
    let from = vm.to_integer(arg(args, 1))?.clamp(0.0, s.get().len() as f64) as usize;
    let (h, n) = (s.get().units(), needle.get().units());
    if from + n.len() > h.len() {
        return Ok(Value::FALSE);
    }
    Ok(Value::bool((0..n.len()).all(|j| h.at(from + j) == n.at(j))))
}

fn ends_with(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "endsWith")?;
    not_regexp(vm, arg(args, 0), "endsWith")?;
    let needle = vm.to_string(arg(args, 0))?;
    let len = s.get().len() as f64;
    let end = if arg(args, 1).is_undefined() { len } else { vm.to_integer(arg(args, 1))?.clamp(0.0, len) } as usize;
    let (h, n) = (s.get().units(), needle.get().units());
    if n.len() > end {
        return Ok(Value::FALSE);
    }
    let start = end - n.len();
    Ok(Value::bool((0..n.len()).all(|j| h.at(start + j) == n.at(j))))
}

fn clamp_rel(vm: &mut Vm, v: Value, len: f64, default: f64) -> JsResult<f64> {
    if v.is_undefined() {
        return Ok(default);
    }
    let n = vm.to_integer(v)?;
    Ok(if n < 0.0 { (len + n).max(0.0) } else { n.min(len) })
}

fn slice(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "slice")?;
    let len = s.get().len() as f64;
    let start = clamp_rel(vm, arg(args, 0), len, 0.0)?;
    let end = clamp_rel(vm, arg(args, 1), len, len)?;
    if start >= end {
        return Ok(vm.atom_value(atoms::empty));
    }
    Ok(Value::string(vm.substring(s, start as u32, end as u32)))
}

fn substring(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "substring")?;
    let len = s.get().len() as f64;
    let a = vm.to_integer(arg(args, 0))?.clamp(0.0, len);
    let b = if arg(args, 1).is_undefined() { len } else { vm.to_integer(arg(args, 1))?.clamp(0.0, len) };
    let (start, end) = if a < b { (a, b) } else { (b, a) };
    if start == end {
        return Ok(vm.atom_value(atoms::empty));
    }
    Ok(Value::string(vm.substring(s, start as u32, end as u32)))
}

fn substr(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "substr")?;
    let len = s.get().len() as f64;
    let start = clamp_rel(vm, arg(args, 0), len, 0.0)?;
    let count = if arg(args, 1).is_undefined() { len - start } else { vm.to_integer(arg(args, 1))?.clamp(0.0, len - start) };
    if count <= 0.0 {
        return Ok(vm.atom_value(atoms::empty));
    }
    Ok(Value::string(vm.substring(s, start as u32, (start + count) as u32)))
}

fn map_chars(vm: &mut Vm, s: Gc<JsString>, upper: bool) -> Value {
    if let Units::Latin1(b) = s.get().units() {
        if b.is_ascii() {
            let out: Vec<u8> = if upper { b.to_ascii_uppercase() } else { b.to_ascii_lowercase() };
            if out == b {
                return Value::string(s);
            }
            return Value::string(vm.new_string_latin1(out));
        }
    }
    let text = s.get().to_rust_string();
    let out = if upper { text.to_uppercase() } else { text.to_lowercase() };
    vm.str_value(&out)
}

fn to_upper_case(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "toUpperCase")?;
    Ok(map_chars(vm, s, true))
}

fn to_lower_case(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "toLowerCase")?;
    Ok(map_chars(vm, s, false))
}

pub(crate) fn is_js_whitespace(u: u16) -> bool {
    matches!(u, 0x09..=0x0D | 0x20 | 0xA0 | 0x1680 | 0x2000..=0x200A | 0x2028 | 0x2029 | 0x202F | 0x205F | 0x3000 | 0xFEFF)
}

fn trim_impl(vm: &mut Vm, this: Value, start: bool, end: bool) -> JsResult<Value> {
    let s = this_str(vm, this, "trim")?;
    let u = s.get().units();
    let (mut a, mut b) = (0, u.len());
    if start {
        while a < b && is_js_whitespace(u.at(a)) {
            a += 1;
        }
    }
    if end {
        while b > a && is_js_whitespace(u.at(b - 1)) {
            b -= 1;
        }
    }
    Ok(Value::string(vm.substring(s, a as u32, b as u32)))
}

fn trim(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    trim_impl(vm, this, true, true)
}

fn trim_start(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    trim_impl(vm, this, true, false)
}

fn trim_end(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    trim_impl(vm, this, false, true)
}

fn pad(vm: &mut Vm, this: Value, args: &[Value], at_start: bool) -> JsResult<Value> {
    let s = this_str(vm, this, "padStart")?;
    let target = vm.to_length(arg(args, 0))? as usize;
    let len = s.get().len() as usize;
    if target <= len {
        return Ok(Value::string(s));
    }
    let filler: Vec<u16> = if arg(args, 1).is_undefined() { vec![b' ' as u16] } else { vm.to_string(arg(args, 1))?.get().units().to_vec() };
    if filler.is_empty() {
        return Ok(Value::string(s));
    }
    if target > (1 << 28) {
        return Err(vm.range_error("Invalid string length"));
    }
    let fill: Vec<u16> = filler.iter().copied().cycle().take(target - len).collect();
    let mut out = Vec::with_capacity(target);
    let own = s.get().units().to_vec();
    if at_start {
        out.extend(fill);
        out.extend(own);
    } else {
        out.extend(own);
        out.extend(fill);
    }
    Ok(Value::string(vm.new_string_units(&out)))
}

fn pad_start(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    pad(vm, this, args, true)
}

fn pad_end(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    pad(vm, this, args, false)
}

fn repeat(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "repeat")?;
    let n = vm.to_integer(arg(args, 0))?;
    if n < 0.0 || n.is_infinite() {
        return Err(vm.range_error("Invalid count value"));
    }
    let total = n * s.get().len() as f64;
    if total > (1 << 29) as f64 {
        return Err(vm.range_error("Invalid string length"));
    }
    let units = s.get().units().to_vec();
    let mut out = Vec::with_capacity(total as usize);
    for _ in 0..n as usize {
        out.extend_from_slice(&units);
    }
    Ok(Value::string(vm.new_string_units(&out)))
}

fn concat(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let mut s = this_str(vm, this, "concat")?;
    for &a in args {
        let t = vm.to_string(a)?;
        s = vm.concat(s, t);
    }
    Ok(Value::string(s))
}

fn locale_compare(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "localeCompare")?;
    let t = vm.to_string(arg(args, 0))?;
    let (a, b) = (s.get().to_rust_string(), t.get().to_rust_string());
    Ok(Value::int(match a.cmp(&b) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }))
}

fn normalize(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::string(this_str(vm, this, "normalize")?))
}

fn split(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if this.is_nullish() {
        return Err(vm.type_error("String.prototype.split called on null or undefined"));
    }
    let sep = arg(args, 0);
    if let Some(o) = sep.as_object() {
        if super::regexp::is_regexp(o) {
            return super::regexp::regexp_split(vm, o, this, arg(args, 1));
        }
    }
    let s = vm.to_string(this)?;
    let limit = if arg(args, 1).is_undefined() { u32::MAX } else { vm.to_uint32(arg(args, 1))? };
    if limit == 0 {
        return Ok(super::array::create_array(vm, Vec::new()));
    }
    if sep.is_undefined() {
        return Ok(super::array::create_array(vm, vec![Value::string(s)]));
    }
    let sep = vm.to_string(sep)?;
    let (h, n) = (s.get().units(), sep.get().units());
    let mut parts = Vec::new();
    if n.is_empty() {
        for i in 0..h.len().min(limit as usize) {
            parts.push(Value::string(vm.substring(s, i as u32, i as u32 + 1)));
        }
        return Ok(super::array::create_array(vm, parts));
    }
    let mut start = 0;
    while let Some(p) = find_units(&h, &n, start) {
        parts.push(Value::string(vm.substring(s, start as u32, p as u32)));
        if parts.len() as u32 >= limit {
            return Ok(super::array::create_array(vm, parts));
        }
        start = p + n.len();
    }
    parts.push(Value::string(vm.substring(s, start as u32, h.len() as u32)));
    Ok(super::array::create_array(vm, parts))
}

/// Expand `$` patterns in a replacement string
pub(crate) fn expand_replacement(
    out: &mut Vec<u16>,
    replacement: &Units,
    matched: &[u16],
    subject: &Units,
    position: usize,
    captures: &[Option<Vec<u16>>],
) {
    let n = replacement.len();
    let mut i = 0;
    while i < n {
        let c = replacement.at(i);
        if c != b'$' as u16 || i + 1 >= n {
            out.push(c);
            i += 1;
            continue;
        }
        let d = replacement.at(i + 1);
        match d {
            0x24 => {
                out.push(b'$' as u16);
                i += 2;
            }
            0x26 => {
                out.extend_from_slice(matched);
                i += 2;
            }
            0x60 => {
                out.extend((0..position).map(|k| subject.at(k)));
                i += 2;
            }
            0x27 => {
                out.extend((position + matched.len()..subject.len()).map(|k| subject.at(k)));
                i += 2;
            }
            0x30..=0x39 => {
                let d1 = (d - 0x30) as usize;
                let two = if i + 2 < n && (0x30..=0x39).contains(&replacement.at(i + 2)) {
                    Some(d1 * 10 + (replacement.at(i + 2) - 0x30) as usize)
                } else {
                    None
                };
                match two {
                    Some(k) if k >= 1 && k <= captures.len() => {
                        if let Some(c) = &captures[k - 1] {
                            out.extend_from_slice(c);
                        }
                        i += 3;
                    }
                    _ if d1 >= 1 && d1 <= captures.len() => {
                        if let Some(c) = &captures[d1 - 1] {
                            out.extend_from_slice(c);
                        }
                        i += 2;
                    }
                    _ => {
                        out.push(c);
                        i += 1;
                    }
                }
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
}

fn replace_impl(vm: &mut Vm, this: Value, args: &[Value], all: bool) -> JsResult<Value> {
    if this.is_nullish() {
        return Err(vm.type_error("String.prototype.replace called on null or undefined"));
    }
    let pattern = arg(args, 0);
    if let Some(o) = pattern.as_object() {
        if super::regexp::is_regexp(o) {
            if all && !super::regexp::is_global(o) {
                return Err(vm.type_error("replaceAll must be called with a global RegExp"));
            }
            return super::regexp::regexp_replace(vm, o, this, arg(args, 1));
        }
    }
    let s = vm.to_string(this)?;
    let pat = vm.to_string(pattern)?;
    let replacement = arg(args, 1);
    let functional = vm.is_callable(replacement);
    let rep_str = if functional { None } else { Some(vm.to_string(replacement)?) };
    let (h, p) = (s.get().units(), pat.get().units());
    let mut positions = Vec::new();
    let mut from = 0;
    while let Some(pos) = find_units(&h, &p, from) {
        positions.push(pos);
        if !all {
            break;
        }
        from = pos + p.len().max(1);
        if from > h.len() {
            break;
        }
    }
    if positions.is_empty() {
        return Ok(Value::string(s));
    }
    let hv = h.to_vec();
    let pv = p.to_vec();
    let mut out = Vec::with_capacity(hv.len());
    let mut last = 0;
    for pos in positions {
        out.extend_from_slice(&hv[last..pos]);
        match rep_str {
            Some(r) => expand_replacement(&mut out, &r.get().units(), &pv, &Units::Utf16(&hv), pos, &[]),
            None => {
                let r = vm.call(replacement, Value::UNDEFINED, &[Value::string(pat), Value::number(pos as f64), Value::string(s)])?;
                let rs = vm.to_string(r)?;
                out.extend(rs.get().units().iter());
            }
        }
        last = pos + pv.len();
    }
    out.extend_from_slice(&hv[last..]);
    Ok(Value::string(vm.new_string_units(&out)))
}

fn replace(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    replace_impl(vm, this, args, false)
}

fn replace_all(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    replace_impl(vm, this, args, true)
}

fn iterator(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_str(vm, this, "[Symbol.iterator]")?;
    let proto = vm.realm.string_iterator_proto;
    let it = vm.new_object_with(Some(proto), ObjectKind::StringIterator { string: s, pos: 0 });
    Ok(Value::object(it))
}
