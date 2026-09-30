//! RegExp and the regular-expression methods of String

use std::rc::Rc;

use crate::bytecode::RegexLiteral;
use crate::gc::Gc;
use crate::object::*;
use crate::regex::Regex;
use crate::string::{JsString, Units, atoms};
use crate::value::Value;
use crate::vm::{ErrorKind, JsResult, NativeFn, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.regexp_proto;
    vm.def_ctor("RegExp", 2, regexp_call, Some(regexp_construct), proto);
    let methods: &[(&str, u32, NativeFn)] = &[("exec", 1, exec), ("test", 1, test), ("toString", 0, to_string)];
    for &(name, len, f) in methods {
        vm.def_method(proto, name, len, f);
    }
    let getters: &[(&str, NativeFn)] = &[
        ("source", source),
        ("flags", flags),
        ("global", |vm, t, _, _| flag(vm, t, 'g')),
        ("ignoreCase", |vm, t, _, _| flag(vm, t, 'i')),
        ("multiline", |vm, t, _, _| flag(vm, t, 'm')),
        ("dotAll", |vm, t, _, _| flag(vm, t, 's')),
        ("unicode", |vm, t, _, _| flag(vm, t, 'u')),
        ("sticky", |vm, t, _, _| flag(vm, t, 'y')),
        ("hasIndices", |vm, t, _, _| flag(vm, t, 'd')),
    ];
    for &(name, f) in getters {
        vm.def_getter(proto, name, f);
    }
    let syms: [(Gc<Symbol>, &str, NativeFn); 5] = [
        (vm.sym.match_, "[Symbol.match]", symbol_match),
        (vm.sym.match_all, "[Symbol.matchAll]", symbol_match_all),
        (vm.sym.replace, "[Symbol.replace]", symbol_replace),
        (vm.sym.search, "[Symbol.search]", symbol_search),
        (vm.sym.split, "[Symbol.split]", symbol_split),
    ];
    for (s, name, f) in syms {
        vm.def_method_sym(proto, s, name, 1, f);
    }
}

pub(crate) fn is_regexp(o: Gc<JsObject>) -> bool {
    matches!(o.get().kind, ObjectKind::RegExp(_))
}

pub(crate) fn is_global(o: Gc<JsObject>) -> bool {
    match &o.get().kind {
        ObjectKind::RegExp(r) => r.regex.flags.global,
        _ => false,
    }
}

fn syntax_error(vm: &mut Vm, e: crate::regex::RegexError) -> Value {
    vm.make_error(ErrorKind::Syntax, &e.to_string())
}

/// A new RegExp object (lastIndex 0)
fn make(vm: &mut Vm, proto: Gc<JsObject>, source: Gc<JsString>, flags: Gc<JsString>, regex: Rc<Regex>) -> Gc<JsObject> {
    let o = vm.new_object_with(Some(proto), ObjectKind::RegExp(Box::new(RegExpData { source, flags, regex })));
    vm.define_value(o, PropertyKey::Atom(atoms::lastIndex), Value::int(0), PropFlags(PropFlags::WRITABLE));
    o
}

pub(crate) fn from_literal(vm: &mut Vm, lit: &RegexLiteral) -> JsResult<Value> {
    let regex = match lit.compiled.get() {
        Some(r) => r.clone(),
        None => {
            let r = Rc::new(Regex::new(&lit.pattern, &lit.flags).map_err(|e| syntax_error(vm, e))?);
            let _ = lit.compiled.set(r.clone());
            r
        }
    };
    let source = vm.new_string_units(&lit.pattern);
    let flags = vm.new_string(&lit.flags);
    let proto = vm.realm.regexp_proto;
    Ok(Value::object(make(vm, proto, source, flags, regex)))
}

fn regexp_call(vm: &mut Vm, _this: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    // RegExp(re) with no flags returns re itself
    let p = arg(args, 0);
    if arg(args, 1).is_undefined() {
        if let Some(o) = p.as_object() {
            if is_regexp(o) {
                let c = vm.get(p, PropertyKey::Atom(atoms::constructor))?;
                if c == Value::object(callee) {
                    return Ok(p);
                }
            }
        }
    }
    regexp_construct(vm, Value::object(callee), args, callee)
}

fn regexp_construct(vm: &mut Vm, new_target: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let p = arg(args, 0);
    let f = arg(args, 1);
    let existing = p.as_object().and_then(|o| match &o.get().kind {
        ObjectKind::RegExp(r) => Some((r.source, r.flags)),
        _ => None,
    });
    let (source, mut flags_str) = match existing {
        Some((src, fl)) => (src, Some(fl)),
        None => {
            let s = if p.is_undefined() { vm.atoms.string(atoms::empty) } else { vm.to_string(p)? };
            (s, None)
        }
    };
    if !f.is_undefined() {
        flags_str = Some(vm.to_string(f)?);
    }
    let flags_str = flags_str.unwrap_or_else(|| vm.atoms.string(atoms::empty));
    let pattern = source.get().units().to_vec();
    let flags_rust = flags_str.get().to_rust_string();
    let regex = Rc::new(Regex::new(&pattern, &flags_rust).map_err(|e| syntax_error(vm, e))?);
    let proto = vm.prototype_for(new_target, |r| r.regexp_proto)?;
    Ok(Value::object(make(vm, proto, source, flags_str, regex)))
}

fn this_regexp(vm: &mut Vm, this: Value, method: &str) -> JsResult<Gc<JsObject>> {
    match this.as_object() {
        Some(o) if is_regexp(o) => Ok(o),
        _ => Err(vm.type_error(&format!("RegExp.prototype.{method} requires that 'this' be a RegExp"))),
    }
}

fn regex_of(o: Gc<JsObject>) -> Rc<Regex> {
    match &o.get().kind {
        ObjectKind::RegExp(r) => r.regex.clone(),
        _ => unreachable!(),
    }
}

fn flag(vm: &mut Vm, this: Value, c: char) -> JsResult<Value> {
    let Some(o) = this.as_object() else {
        return Err(vm.type_error("RegExp flag getter called on non-object"));
    };
    match &o.get().kind {
        ObjectKind::RegExp(r) => Ok(Value::bool(r.flags.get().to_rust_string().contains(c))),
        _ if o == vm.realm.regexp_proto => Ok(Value::UNDEFINED),
        _ => Err(vm.type_error("RegExp flag getter called on incompatible receiver")),
    }
}

fn source(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(o) = this.as_object() else { return Err(vm.type_error("not a RegExp")) };
    let src = match &o.get().kind {
        ObjectKind::RegExp(r) => r.source.get().units().to_vec(),
        _ if o == vm.realm.regexp_proto => return Ok(vm.str_value("(?:)")),
        _ => return Err(vm.type_error("not a RegExp")),
    };
    if src.is_empty() {
        return Ok(vm.str_value("(?:)"));
    }
    // Escape '/' outside classes and line terminators
    let mut out = Vec::with_capacity(src.len());
    let (mut in_class, mut escaped) = (false, false);
    for &u in &src {
        match u {
            0x0A => out.extend("\\n".encode_utf16()),
            0x0D => out.extend("\\r".encode_utf16()),
            0x2028 => out.extend("\\u2028".encode_utf16()),
            0x2029 => out.extend("\\u2029".encode_utf16()),
            0x2F if !escaped && !in_class => out.extend("\\/".encode_utf16()),
            _ => out.push(u),
        }
        if !escaped {
            if u == b'[' as u16 {
                in_class = true;
            } else if u == b']' as u16 {
                in_class = false;
            }
        }
        escaped = !escaped && u == b'\\' as u16;
    }
    Ok(Value::string(vm.new_string_units(&out)))
}

fn flags(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if !this.is_object() {
        return Err(vm.type_error("RegExp.prototype.flags getter called on non-object"));
    }
    let mut out = String::new();
    for (name, c) in [("hasIndices", 'd'), ("global", 'g'), ("ignoreCase", 'i'), ("multiline", 'm'), ("dotAll", 's'), ("unicode", 'u'), ("sticky", 'y')] {
        let v = vm.get_str(this, name)?;
        if crate::vm::ops::truthy(v) {
            out.push(c);
        }
    }
    Ok(vm.str_value(&out))
}

fn to_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if !this.is_object() {
        return Err(vm.type_error("RegExp.prototype.toString called on non-object"));
    }
    let s = vm.get_str(this, "source")?;
    let s = vm.to_rust_string(s)?;
    let f = vm.get_str(this, "flags")?;
    let f = vm.to_rust_string(f)?;
    Ok(vm.str_value(&format!("/{s}/{f}")))
}

/// Run the regex on a string's contents
fn run_regex(regex: &Regex, s: Gc<JsString>, start: usize, sticky: bool) -> Option<Vec<Option<(usize, usize)>>> {
    match s.get().units() {
        Units::Latin1(b) => regex.exec(b, start, sticky),
        Units::Utf16(u) => regex.exec(u, start, sticky),
    }
}

fn get_last_index(vm: &mut Vm, re: Gc<JsObject>) -> JsResult<usize> {
    let v = vm.get(Value::object(re), PropertyKey::Atom(atoms::lastIndex))?;
    if let Some(i) = v.as_int() {
        return Ok(i.max(0) as usize);
    }
    Ok(vm.to_length(v)? as usize)
}

fn set_last_index(vm: &mut Vm, re: Gc<JsObject>, v: usize) -> JsResult<()> {
    vm.set(Value::object(re), PropertyKey::Atom(atoms::lastIndex), Value::number(v as f64), true)
}

/// RegExpBuiltinExec: capture positions, updating lastIndex
fn builtin_exec(vm: &mut Vm, re: Gc<JsObject>, s: Gc<JsString>) -> JsResult<Option<Vec<Option<(usize, usize)>>>> {
    let regex = regex_of(re);
    let g = regex.flags.global || regex.flags.sticky;
    let last = if g { get_last_index(vm, re)? } else { 0 };
    if last > s.get().len() as usize {
        if g {
            set_last_index(vm, re, 0)?;
        }
        return Ok(None);
    }
    match run_regex(&regex, s, last, regex.flags.sticky) {
        Some(caps) => {
            if g {
                set_last_index(vm, re, caps[0].unwrap().1)?;
            }
            Ok(Some(caps))
        }
        None => {
            if g {
                set_last_index(vm, re, 0)?;
            }
            Ok(None)
        }
    }
}

/// The exec result array
fn match_result(vm: &mut Vm, regex: &Regex, s: Gc<JsString>, caps: &[Option<(usize, usize)>]) -> Value {
    let values: Vec<Value> = caps
        .iter()
        .map(|c| match c {
            Some((a, b)) => Value::string(vm.substring(s, *a as u32, *b as u32)),
            None => Value::UNDEFINED,
        })
        .collect();
    let arr = vm.new_array(values.clone());
    let index = caps[0].unwrap().0;
    vm.define_value(arr, PropertyKey::Atom(atoms::index), Value::number(index as f64), PropFlags::DEFAULT);
    vm.define_value(arr, PropertyKey::Atom(atoms::input), Value::string(s), PropFlags::DEFAULT);
    let groups = if regex.names.is_empty() {
        Value::UNDEFINED
    } else {
        let g = vm.new_object_with(None, ObjectKind::Ordinary);
        for (name, i) in &regex.names {
            let key = vm.key_from_str(name);
            vm.define_value(g, key, values[*i], PropFlags::DEFAULT);
        }
        Value::object(g)
    };
    vm.define_value(arr, PropertyKey::Atom(atoms::groups), groups, PropFlags::DEFAULT);
    if regex.flags.has_indices {
        let pairs: Vec<Value> = caps
            .iter()
            .map(|c| match c {
                Some((a, b)) => Value::object(vm.new_array(vec![Value::number(*a as f64), Value::number(*b as f64)])),
                None => Value::UNDEFINED,
            })
            .collect();
        let indices = vm.new_array(pairs);
        let key = vm.key_from_str("indices");
        vm.define_value(arr, key, Value::object(indices), PropFlags::DEFAULT);
    }
    Value::object(arr)
}

fn exec(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let re = this_regexp(vm, this, "exec")?;
    let s = vm.to_string(arg(args, 0))?;
    match builtin_exec(vm, re, s)? {
        Some(caps) => {
            let regex = regex_of(re);
            Ok(match_result(vm, &regex, s, &caps))
        }
        None => Ok(Value::NULL),
    }
}

fn test(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let re = this_regexp(vm, this, "test")?;
    let s = vm.to_string(arg(args, 0))?;
    Ok(Value::bool(builtin_exec(vm, re, s)?.is_some()))
}

/// Next index after an empty match
fn advance(s: Gc<JsString>, i: usize, unicode: bool) -> usize {
    if unicode {
        let st = s.get();
        if let (Some(hi), Some(lo)) = (st.code_unit_at(i as u32), st.code_unit_at(i as u32 + 1)) {
            if (0xD800..0xDC00).contains(&hi) && (0xDC00..0xE000).contains(&lo) {
                return i + 2;
            }
        }
    }
    i + 1
}

/// All matches of a global regex (lastIndex reset to 0 afterwards)
fn all_matches(vm: &mut Vm, re: Gc<JsObject>, s: Gc<JsString>) -> JsResult<Vec<Vec<Option<(usize, usize)>>>> {
    let regex = regex_of(re);
    let mut out = Vec::new();
    let mut pos = 0;
    let len = s.get().len() as usize;
    while pos <= len {
        let Some(caps) = run_regex(&regex, s, pos, regex.flags.sticky) else { break };
        let (a, b) = caps[0].unwrap();
        pos = if a == b { advance(s, b, regex.flags.unicode) } else { b };
        out.push(caps);
    }
    set_last_index(vm, re, 0)?;
    Ok(out)
}

fn symbol_match(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let re = this_regexp(vm, this, "[Symbol.match]")?;
    let s = vm.to_string(arg(args, 0))?;
    regexp_match(vm, re, s)
}

fn regexp_match(vm: &mut Vm, re: Gc<JsObject>, s: Gc<JsString>) -> JsResult<Value> {
    if !regex_of(re).flags.global {
        return exec(vm, Value::object(re), &[Value::string(s)], re);
    }
    let matches = all_matches(vm, re, s)?;
    if matches.is_empty() {
        return Ok(Value::NULL);
    }
    let values: Vec<Value> = matches
        .iter()
        .map(|caps| {
            let (a, b) = caps[0].unwrap();
            Value::string(vm.substring(s, a as u32, b as u32))
        })
        .collect();
    Ok(Value::object(vm.new_array(values)))
}

fn symbol_match_all(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let re = this_regexp(vm, this, "[Symbol.matchAll]")?;
    let s = vm.to_string(arg(args, 0))?;
    regexp_match_all(vm, re, s)
}

fn regexp_match_all(vm: &mut Vm, re: Gc<JsObject>, s: Gc<JsString>) -> JsResult<Value> {
    let regex = regex_of(re);
    let results: Vec<Value> = if regex.flags.global {
        let matches = all_matches(vm, re, s)?;
        matches.iter().map(|caps| match_result(vm, &regex, s, caps)).collect()
    } else {
        match run_regex(&regex, s, 0, regex.flags.sticky) {
            Some(caps) => vec![match_result(vm, &regex, s, &caps)],
            None => Vec::new(),
        }
    };
    let arr = vm.new_array(results);
    let proto = vm.realm.array_iterator_proto;
    let it = vm.new_object_with(Some(proto), ObjectKind::ArrayIterator { target: Value::object(arr), index: 0, kind: 0 });
    Ok(Value::object(it))
}

fn symbol_search(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let re = this_regexp(vm, this, "[Symbol.search]")?;
    let s = vm.to_string(arg(args, 0))?;
    regexp_search(re, s)
}

fn regexp_search(re: Gc<JsObject>, s: Gc<JsString>) -> JsResult<Value> {
    let regex = regex_of(re);
    Ok(match run_regex(&regex, s, 0, regex.flags.sticky) {
        Some(caps) => Value::number(caps[0].unwrap().0 as f64),
        None => Value::int(-1),
    })
}

/// Expand a replacement template for one match
fn expand(out: &mut Vec<u16>, template: &[u16], subject: &Units, caps: &[Option<(usize, usize)>], names: &[(String, usize)]) {
    let n = template.len();
    let (ms, me) = caps[0].unwrap();
    let ncaps = caps.len() - 1;
    let push_cap = |out: &mut Vec<u16>, c: Option<(usize, usize)>| {
        if let Some((a, b)) = c {
            out.extend((a..b).map(|k| subject.at(k)));
        }
    };
    let mut i = 0;
    while i < n {
        let c = template[i];
        if c != b'$' as u16 || i + 1 >= n {
            out.push(c);
            i += 1;
            continue;
        }
        let d = template[i + 1];
        match d {
            0x24 => {
                out.push(0x24);
                i += 2;
            }
            0x26 => {
                push_cap(out, Some((ms, me)));
                i += 2;
            }
            0x60 => {
                out.extend((0..ms).map(|k| subject.at(k)));
                i += 2;
            }
            0x27 => {
                out.extend((me..subject.len()).map(|k| subject.at(k)));
                i += 2;
            }
            0x3C if !names.is_empty() => match template[i + 2..].iter().position(|&u| u == b'>' as u16) {
                Some(end) => {
                    let name: String = String::from_utf16_lossy(&template[i + 2..i + 2 + end]);
                    if let Some((_, idx)) = names.iter().find(|(n, _)| *n == name) {
                        push_cap(out, caps[*idx]);
                    }
                    i += 3 + end;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            },
            0x30..=0x39 => {
                let d1 = (d - 0x30) as usize;
                let two = if i + 2 < n && (0x30..=0x39).contains(&template[i + 2]) { Some(d1 * 10 + (template[i + 2] - 0x30) as usize) } else { None };
                match two {
                    Some(k) if k >= 1 && k <= ncaps => {
                        push_cap(out, caps[k]);
                        i += 3;
                    }
                    _ if d1 >= 1 && d1 <= ncaps => {
                        push_cap(out, caps[d1]);
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

fn symbol_replace(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let re = this_regexp(vm, this, "[Symbol.replace]")?;
    regexp_replace(vm, re, arg(args, 0), arg(args, 1))
}

pub(crate) fn regexp_replace(vm: &mut Vm, re: Gc<JsObject>, subject: Value, replacement: Value) -> JsResult<Value> {
    let s = vm.to_string(subject)?;
    let regex = regex_of(re);
    let functional = vm.is_callable(replacement);
    let template = if functional { None } else { Some(vm.to_string(replacement)?.get().units().to_vec()) };
    let matches = if regex.flags.global {
        all_matches(vm, re, s)?
    } else {
        match builtin_exec(vm, re, s)? {
            Some(c) => vec![c],
            None => Vec::new(),
        }
    };
    if matches.is_empty() {
        return Ok(Value::string(s));
    }
    let mut out = Vec::with_capacity(s.get().len() as usize);
    let mut last = 0;
    let mark = vm.temp_mark();
    for caps in &matches {
        let (a, b) = caps[0].unwrap();
        {
            let units = s.get().units();
            out.extend((last..a).map(|k| units.at(k)));
        }
        match &template {
            Some(t) => expand(&mut out, t, &s.get().units(), caps, &regex.names),
            None => {
                let mut fargs: Vec<Value> = caps
                    .iter()
                    .map(|c| match c {
                        Some((x, y)) => Value::string(vm.substring(s, *x as u32, *y as u32)),
                        None => Value::UNDEFINED,
                    })
                    .collect();
                fargs.push(Value::number(a as f64));
                fargs.push(Value::string(s));
                if !regex.names.is_empty() {
                    let g = vm.new_object_with(None, ObjectKind::Ordinary);
                    for (name, i) in &regex.names {
                        let key = vm.key_from_str(name);
                        vm.define_value(g, key, fargs[*i], PropFlags::DEFAULT);
                    }
                    fargs.push(Value::object(g));
                }
                let r = vm.call(replacement, Value::UNDEFINED, &fargs)?;
                let rs = vm.to_string(r)?;
                out.extend(rs.get().units().iter());
                vm.temp_reset(mark);
            }
        }
        last = b;
    }
    {
        let units = s.get().units();
        out.extend((last..units.len()).map(|k| units.at(k)));
    }
    Ok(Value::string(vm.new_string_units(&out)))
}

fn symbol_split(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let re = this_regexp(vm, this, "[Symbol.split]")?;
    regexp_split(vm, re, arg(args, 0), arg(args, 1))
}

pub(crate) fn regexp_split(vm: &mut Vm, re: Gc<JsObject>, subject: Value, limit: Value) -> JsResult<Value> {
    let s = vm.to_string(subject)?;
    let regex = regex_of(re);
    let limit = if limit.is_undefined() { u32::MAX } else { vm.to_uint32(limit)? };
    let mut parts: Vec<Value> = Vec::new();
    if limit == 0 {
        return Ok(Value::object(vm.new_array(parts)));
    }
    let len = s.get().len() as usize;
    if len == 0 {
        if run_regex(&regex, s, 0, true).is_none() {
            parts.push(Value::string(s));
        }
        return Ok(Value::object(vm.new_array(parts)));
    }
    // Leftmost-match search from q is equivalent to the specification's
    // sticky attempt at every position
    let (mut p, mut q) = (0usize, 0usize);
    while q < len {
        let Some(caps) = run_regex(&regex, s, q, false) else { break };
        let (a, e) = caps[0].unwrap();
        if a >= len {
            break;
        }
        if e == p {
            // Empty match where the last piece ended
            q = advance(s, a, regex.flags.unicode);
            continue;
        }
        parts.push(Value::string(vm.substring(s, p as u32, a as u32)));
        if parts.len() as u32 == limit {
            return Ok(Value::object(vm.new_array(parts)));
        }
        for c in &caps[1..] {
            parts.push(match c {
                Some((x, y)) => Value::string(vm.substring(s, *x as u32, *y as u32)),
                None => Value::UNDEFINED,
            });
            if parts.len() as u32 == limit {
                return Ok(Value::object(vm.new_array(parts)));
            }
        }
        p = e;
        q = p;
    }
    parts.push(Value::string(vm.substring(s, p as u32, len as u32)));
    Ok(Value::object(vm.new_array(parts)))
}

/// A string method's argument as a RegExp (creating one from a string)
fn coerce_regexp(vm: &mut Vm, v: Value, flags: &str) -> JsResult<Gc<JsObject>> {
    if let Some(o) = v.as_object() {
        if is_regexp(o) {
            return Ok(o);
        }
    }
    let pattern = if v.is_undefined() { Vec::new() } else { vm.to_string(v)?.get().units().to_vec() };
    let regex = Rc::new(Regex::new(&pattern, flags).map_err(|e| syntax_error(vm, e))?);
    let source = vm.new_string_units(&pattern);
    let f = vm.new_string(flags);
    let proto = vm.realm.regexp_proto;
    Ok(make(vm, proto, source, f, regex))
}

pub(crate) fn string_match(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.this_string(this, "match")?;
    let re = coerce_regexp(vm, arg(args, 0), "")?;
    regexp_match(vm, re, s)
}

pub(crate) fn string_match_all(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.this_string(this, "matchAll")?;
    let a = arg(args, 0);
    if let Some(o) = a.as_object() {
        if is_regexp(o) && !is_global(o) {
            return Err(vm.type_error("String.prototype.matchAll called with a non-global RegExp argument"));
        }
    }
    let re = coerce_regexp(vm, a, "g")?;
    regexp_match_all(vm, re, s)
}

pub(crate) fn string_search(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.this_string(this, "search")?;
    let re = coerce_regexp(vm, arg(args, 0), "")?;
    regexp_search(re, s)
}
