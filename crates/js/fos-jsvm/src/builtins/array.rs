//! Array and the built-in iterators
//!
//! Methods are generic over array-likes, with fast paths for dense arrays
//! (every element present, `elements.len() == length`).

use crate::gc::Gc;
use crate::object::*;
use crate::string::{JsString, atoms};
use crate::value::Value;
use crate::vm::ops::{same_value_zero, strict_equals, truthy};
use crate::vm::{JsResult, NativeFn, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.array_proto;
    let c = vm.def_ctor("Array", 1, array_call, Some(array_construct), proto);
    vm.def_method(c, "isArray", 1, is_array);
    vm.def_method(c, "of", 0, of);
    vm.def_method(c, "from", 1, from);
    let species = vm.sym.species;
    let getter = vm.new_native("get [Symbol.species]", 0, species_getter, None);
    vm.define_accessor(c, PropertyKey::Symbol(species), Some(Value::object(getter)), None, PropFlags(PropFlags::CONFIGURABLE));

    let methods: &[(&str, u32, NativeFn)] = &[
        ("push", 1, push),
        ("pop", 0, pop),
        ("shift", 0, shift),
        ("unshift", 1, unshift),
        ("slice", 2, slice),
        ("splice", 2, splice),
        ("concat", 1, concat),
        ("join", 1, join),
        ("toString", 0, to_string),
        ("toLocaleString", 0, to_string),
        ("reverse", 0, reverse),
        ("indexOf", 1, index_of),
        ("lastIndexOf", 1, last_index_of),
        ("includes", 1, includes),
        ("find", 1, find),
        ("findIndex", 1, find_index),
        ("findLast", 1, find_last),
        ("findLastIndex", 1, find_last_index),
        ("filter", 1, filter),
        ("map", 1, map),
        ("forEach", 1, for_each),
        ("some", 1, some),
        ("every", 1, every),
        ("reduce", 1, reduce),
        ("reduceRight", 1, reduce_right),
        ("sort", 1, sort),
        ("fill", 1, fill),
        ("keys", 0, keys),
        ("entries", 0, entries),
        ("flat", 0, flat),
        ("flatMap", 1, flat_map),
        ("at", 1, at),
        ("copyWithin", 2, copy_within),
        ("toReversed", 0, to_reversed),
        ("toSorted", 1, to_sorted),
        ("with", 2, with),
    ];
    for &(name, len, f) in methods {
        vm.def_method(proto, name, len, f);
    }
    // values and @@iterator are the same function
    let values_fn = vm.realm.array_values;
    if let ObjectKind::Native(n) = &mut values_fn.get_mut().kind {
        n.call = values;
        n.name = vm.intern("values");
    }
    values_fn.get_mut().lazy = LAZY_LENGTH | LAZY_NAME;
    vm.def_value(proto, "values", Value::object(values_fn), PropFlags::HIDDEN);
    let it = vm.sym.iterator;
    vm.define_value(proto, PropertyKey::Symbol(it), Value::object(values_fn), PropFlags::HIDDEN);

    // Iterator prototypes
    let ip = vm.realm.iterator_proto;
    vm.def_method_sym(ip, it, "[Symbol.iterator]", 0, return_this);
    let tag = vm.sym.to_string_tag;
    for (p, name) in [
        (vm.realm.array_iterator_proto, "Array Iterator"),
        (vm.realm.string_iterator_proto, "String Iterator"),
        (vm.realm.map_iterator_proto, "Map Iterator"),
        (vm.realm.set_iterator_proto, "Set Iterator"),
    ] {
        vm.def_method(p, "next", 0, iterator_next);
        let s = vm.str_value(name);
        vm.define_value(p, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));
    }
}

fn return_this(_vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(this)
}

fn species_getter(_vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(this)
}

/// `next()` of the built-in iterators
fn iterator_next(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let ok = this.as_object().is_some_and(|o| matches!(o.get().kind, ObjectKind::ArrayIterator { .. } | ObjectKind::StringIterator { .. } | ObjectKind::MapIterator { .. }));
    if !ok {
        return Err(vm.type_error("next method called on incompatible receiver"));
    }
    let r = vm.iter_step(this)?;
    Ok(iter_result(vm, r.unwrap_or(Value::UNDEFINED), r.is_none()))
}

pub(crate) fn iter_result(vm: &mut Vm, value: Value, done: bool) -> Value {
    let o = vm.new_object();
    vm.define_value(o, PropertyKey::Atom(atoms::value), value, PropFlags::DEFAULT);
    vm.define_value(o, PropertyKey::Atom(atoms::done), Value::bool(done), PropFlags::DEFAULT);
    Value::object(o)
}

// ---- helpers ----

/// The length of an array-like
fn len_of(vm: &mut Vm, o: Gc<JsObject>) -> JsResult<u64> {
    if let ObjectKind::Array { length } = o.get().kind {
        return Ok(length as u64);
    }
    let l = vm.get(Value::object(o), PropertyKey::Atom(atoms::length))?;
    Ok(vm.to_length(l)? as u64)
}

fn index_key(i: u64) -> PropertyKey {
    if i < u32::MAX as u64 { PropertyKey::Index(i as u32) } else { PropertyKey::Atom(crate::string::Atom(u32::MAX)) }
}

fn key_for(vm: &mut Vm, i: u64) -> PropertyKey {
    if i < u32::MAX as u64 { PropertyKey::Index(i as u32) } else { PropertyKey::Atom(vm.intern(&i.to_string())) }
}

#[inline]
fn get_idx(vm: &mut Vm, o: Gc<JsObject>, i: u64) -> JsResult<Value> {
    if let Some(&e) = o.get().elements.get(i as usize) {
        if !e.is_hole() {
            return Ok(e);
        }
    }
    let k = key_for(vm, i);
    vm.get(Value::object(o), k)
}

fn has_idx(vm: &mut Vm, o: Gc<JsObject>, i: u64) -> bool {
    if let Some(&e) = o.get().elements.get(i as usize) {
        if !e.is_hole() {
            return true;
        }
    }
    let k = key_for(vm, i);
    vm.has_property(o, k)
}

fn set_idx(vm: &mut Vm, o: Gc<JsObject>, i: u64, v: Value) -> JsResult<()> {
    let k = key_for(vm, i);
    vm.set(Value::object(o), k, v, true)
}

fn set_len(vm: &mut Vm, o: Gc<JsObject>, n: u64) -> JsResult<()> {
    vm.set(Value::object(o), PropertyKey::Atom(atoms::length), Value::number(n as f64), true)
}

fn delete_idx(vm: &mut Vm, o: Gc<JsObject>, i: u64) -> JsResult<()> {
    let k = key_for(vm, i);
    vm.delete_property(Value::object(o), k, true)?;
    Ok(())
}

/// Dense arrays: elements are exactly the array
fn dense(o: Gc<JsObject>) -> bool {
    let ob = o.get();
    match ob.kind {
        ObjectKind::Array { length } => ob.elements.len() == length as usize && ob.dict.is_none() && !ob.elements.iter().any(|e| e.is_hole()),
        _ => false,
    }
}

/// Relative index argument (`slice`, `at`...) clamped to [0, len]
fn relative(vm: &mut Vm, v: Value, len: u64, default: u64) -> JsResult<u64> {
    if v.is_undefined() {
        return Ok(default);
    }
    let n = vm.to_integer(v)?;
    Ok(if n < 0.0 { (len as f64 + n).max(0.0) as u64 } else { n.min(len as f64) as u64 })
}

fn callback(vm: &mut Vm, args: &[Value], method: &str) -> JsResult<Value> {
    let f = arg(args, 0);
    if !vm.is_callable(f) {
        let d = vm.describe(f);
        return Err(vm.type_error(&format!("{d} is not a function (in Array.prototype.{method})")));
    }
    Ok(f)
}

pub(crate) fn create_array(vm: &mut Vm, values: Vec<Value>) -> Value {
    Value::object(vm.new_array(values))
}

// ---- constructor ----

fn array_call(vm: &mut Vm, _this: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    array_construct(vm, Value::object(callee), args, callee)
}

fn array_construct(vm: &mut Vm, new_target: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    let a = if args.len() == 1 && args[0].is_number() {
        let n = args[0].number_unchecked();
        if n < 0.0 || n.fract() != 0.0 || n >= 4294967296.0 {
            return Err(vm.range_error("Invalid array length"));
        }
        let a = vm.new_array(Vec::new());
        if let ObjectKind::Array { length } = &mut a.get_mut().kind {
            *length = n as u32;
        }
        a
    } else {
        vm.new_array(args.to_vec())
    };
    if !new_target.is_undefined() && new_target != Value::object(callee) {
        let proto = vm.prototype_for(new_target, |r| r.array_proto)?;
        a.get_mut().proto = Some(proto);
        proto.get_mut().is_prototype = true;
    }
    Ok(Value::object(a))
}

fn is_array(_vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(arg(args, 0).as_object().is_some_and(|o| o.get().is_array())))
}

fn of(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(create_array(vm, args.to_vec()))
}

fn from(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let src = arg(args, 0);
    let f = arg(args, 1);
    let this_arg = arg(args, 2);
    let mapping = !f.is_undefined();
    if mapping && !vm.is_callable(f) {
        return Err(vm.type_error("Array.from: when provided, the second argument must be a function"));
    }
    if src.is_nullish() {
        return Err(vm.type_error("Array.from requires an array-like or iterable"));
    }
    let iter_method = vm.get(src, PropertyKey::Symbol(vm.sym.iterator))?;
    let mut out = Vec::new();
    if !iter_method.is_nullish() {
        let it = vm.get_iterator(src)?;
        let mut i = 0;
        while let Some(v) = vm.iter_step(it)? {
            let v = if mapping { vm.call(f, this_arg, &[v, Value::number(i as f64)])? } else { v };
            out.push(v);
            i += 1;
        }
    } else {
        let o = vm.to_object(src)?;
        let len = len_of(vm, o)?;
        for i in 0..len {
            let v = get_idx(vm, o, i)?;
            let v = if mapping { vm.call(f, this_arg, &[v, Value::number(i as f64)])? } else { v };
            out.push(v);
        }
    }
    Ok(create_array(vm, out))
}

// ---- mutators ----

fn push(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    {
        let ob = o.get_mut();
        if let ObjectKind::Array { length } = &mut ob.kind {
            if *length as usize == ob.elements.len() && ob.extensible && (*length as u64 + args.len() as u64) < u32::MAX as u64 {
                *length += args.len() as u32;
                let len = *length;
                ob.elements.extend_from_slice(args);
                return Ok(Value::number(len as f64));
            }
        }
    }
    let mut len = len_of(vm, o)?;
    for &a in args {
        set_idx(vm, o, len, a)?;
        len += 1;
    }
    set_len(vm, o, len)?;
    Ok(Value::number(len as f64))
}

fn pop(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    {
        let ob = o.get_mut();
        if let ObjectKind::Array { length } = &mut ob.kind {
            if *length as usize == ob.elements.len() && *length > 0 {
                let v = ob.elements.pop().unwrap();
                if !v.is_hole() {
                    *length -= 1;
                    return Ok(v);
                }
                ob.elements.push(v);
            }
        }
    }
    let len = len_of(vm, o)?;
    if len == 0 {
        set_len(vm, o, 0)?;
        return Ok(Value::UNDEFINED);
    }
    let v = get_idx(vm, o, len - 1)?;
    delete_idx(vm, o, len - 1)?;
    set_len(vm, o, len - 1)?;
    Ok(v)
}

fn shift(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    if dense(o) && o.get().extensible {
        let ob = o.get_mut();
        if ob.elements.is_empty() {
            return Ok(Value::UNDEFINED);
        }
        let v = ob.elements.remove(0);
        if let ObjectKind::Array { length } = &mut ob.kind {
            *length -= 1;
        }
        return Ok(v);
    }
    let len = len_of(vm, o)?;
    if len == 0 {
        set_len(vm, o, 0)?;
        return Ok(Value::UNDEFINED);
    }
    let first = get_idx(vm, o, 0)?;
    for i in 1..len {
        if has_idx(vm, o, i) {
            let v = get_idx(vm, o, i)?;
            set_idx(vm, o, i - 1, v)?;
        } else {
            delete_idx(vm, o, i - 1)?;
        }
    }
    delete_idx(vm, o, len - 1)?;
    set_len(vm, o, len - 1)?;
    Ok(first)
}

fn unshift(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    if dense(o) && o.get().extensible {
        let ob = o.get_mut();
        ob.elements.splice(0..0, args.iter().copied());
        let n = ob.elements.len() as u32;
        if let ObjectKind::Array { length } = &mut ob.kind {
            *length = n;
        }
        return Ok(Value::number(n as f64));
    }
    let len = len_of(vm, o)?;
    let n = args.len() as u64;
    for i in (0..len).rev() {
        if has_idx(vm, o, i) {
            let v = get_idx(vm, o, i)?;
            set_idx(vm, o, i + n, v)?;
        } else {
            delete_idx(vm, o, i + n)?;
        }
    }
    for (j, &a) in args.iter().enumerate() {
        set_idx(vm, o, j as u64, a)?;
    }
    set_len(vm, o, len + n)?;
    Ok(Value::number((len + n) as f64))
}

fn splice(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    let start = relative(vm, arg(args, 0), len, 0)?;
    let delete_count = if args.is_empty() {
        0
    } else if args.len() == 1 {
        len - start
    } else {
        let d = vm.to_integer(args[1])?;
        (d.max(0.0) as u64).min(len - start)
    };
    let items: Vec<Value> = args.iter().skip(2).copied().collect();
    if dense(o) && o.get().extensible {
        let ob = o.get_mut();
        let removed: Vec<Value> = ob.elements.splice(start as usize..(start + delete_count) as usize, items).collect();
        let n = ob.elements.len() as u32;
        if let ObjectKind::Array { length } = &mut ob.kind {
            *length = n;
        }
        return Ok(create_array(vm, removed));
    }
    let mut removed = Vec::with_capacity(delete_count as usize);
    for i in 0..delete_count {
        removed.push(if has_idx(vm, o, start + i) { get_idx(vm, o, start + i)? } else { Value::HOLE });
    }
    let item_count = items.len() as u64;
    if item_count < delete_count {
        for k in start..len - delete_count {
            let from = k + delete_count;
            let to = k + item_count;
            if has_idx(vm, o, from) {
                let v = get_idx(vm, o, from)?;
                set_idx(vm, o, to, v)?;
            } else {
                delete_idx(vm, o, to)?;
            }
        }
        for k in (len - delete_count + item_count..len).rev() {
            delete_idx(vm, o, k)?;
        }
    } else if item_count > delete_count {
        for k in (start..len - delete_count).rev() {
            let from = k + delete_count;
            let to = k + item_count;
            if has_idx(vm, o, from) {
                let v = get_idx(vm, o, from)?;
                set_idx(vm, o, to, v)?;
            } else {
                delete_idx(vm, o, to)?;
            }
        }
    }
    for (j, &v) in items.iter().enumerate() {
        set_idx(vm, o, start + j as u64, v)?;
    }
    set_len(vm, o, len - delete_count + item_count)?;
    Ok(create_array(vm, removed))
}

fn reverse(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    if dense(o) {
        o.get_mut().elements.reverse();
        return Ok(Value::object(o));
    }
    let len = len_of(vm, o)?;
    let (mut lo, mut hi) = (0u64, len.saturating_sub(1));
    while lo < hi {
        let (lh, hh) = (has_idx(vm, o, lo), has_idx(vm, o, hi));
        let lv = if lh { get_idx(vm, o, lo)? } else { Value::UNDEFINED };
        let hv = if hh { get_idx(vm, o, hi)? } else { Value::UNDEFINED };
        if hh { set_idx(vm, o, lo, hv)? } else { delete_idx(vm, o, lo)? }
        if lh { set_idx(vm, o, hi, lv)? } else { delete_idx(vm, o, hi)? }
        lo += 1;
        hi -= 1;
    }
    Ok(Value::object(o))
}

fn fill(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    let v = arg(args, 0);
    let start = relative(vm, arg(args, 1), len, 0)?;
    let end = relative(vm, arg(args, 2), len, len)?;
    for i in start..end {
        set_idx(vm, o, i, v)?;
    }
    Ok(Value::object(o))
}

fn copy_within(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    let to = relative(vm, arg(args, 0), len, 0)?;
    let from = relative(vm, arg(args, 1), len, 0)?;
    let end = relative(vm, arg(args, 2), len, len)?;
    let count = (end.saturating_sub(from)).min(len - to);
    let values: Vec<Option<Value>> =
        (0..count).map(|i| if has_idx(vm, o, from + i) { get_idx(vm, o, from + i).ok() } else { None }).collect();
    for (i, v) in values.into_iter().enumerate() {
        match v {
            Some(v) => set_idx(vm, o, to + i as u64, v)?,
            None => delete_idx(vm, o, to + i as u64)?,
        }
    }
    Ok(Value::object(o))
}

// ---- accessors ----

fn slice(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    let start = relative(vm, arg(args, 0), len, 0)?;
    let end = relative(vm, arg(args, 1), len, len)?;
    if start >= end {
        return Ok(create_array(vm, Vec::new()));
    }
    if dense(o) {
        let v = o.get().elements[start as usize..end as usize].to_vec();
        return Ok(create_array(vm, v));
    }
    let mut out = Vec::with_capacity((end - start) as usize);
    for i in start..end {
        out.push(if has_idx(vm, o, i) { get_idx(vm, o, i)? } else { Value::HOLE });
    }
    Ok(create_array(vm, out))
}

fn is_concat_spreadable(vm: &mut Vm, v: Value) -> JsResult<bool> {
    let Some(o) = v.as_object() else { return Ok(false) };
    let s = vm.get(v, PropertyKey::Symbol(vm.sym.is_concat_spreadable))?;
    if !s.is_undefined() {
        return Ok(truthy(s));
    }
    Ok(o.get().is_array())
}

fn concat(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let mut out = Vec::new();
    let items = std::iter::once(Value::object(o)).chain(args.iter().copied()).collect::<Vec<_>>();
    for item in items {
        if is_concat_spreadable(vm, item)? {
            let io = item.as_object().unwrap();
            if dense(io) {
                out.extend_from_slice(&io.get().elements);
                continue;
            }
            let len = len_of(vm, io)?;
            for i in 0..len {
                out.push(if has_idx(vm, io, i) { get_idx(vm, io, i)? } else { Value::HOLE });
            }
        } else {
            out.push(item);
        }
    }
    Ok(create_array(vm, out))
}

pub(crate) fn join_values(vm: &mut Vm, o: Gc<JsObject>, sep: &str) -> JsResult<Gc<JsString>> {
    let len = len_of(vm, o)?;
    let mut units: Vec<u16> = Vec::new();
    let sep16: Vec<u16> = sep.encode_utf16().collect();
    for i in 0..len {
        if i > 0 {
            units.extend_from_slice(&sep16);
        }
        let v = get_idx(vm, o, i)?;
        if v.is_nullish() {
            continue;
        }
        let s = vm.to_string(v)?;
        units.extend(s.get().units().iter());
    }
    Ok(vm.new_string_units(&units))
}

fn join(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let sep = arg(args, 0);
    let sep = if sep.is_undefined() { ",".to_string() } else { vm.to_rust_string(sep)? };
    Ok(Value::string(join_values(vm, o, &sep)?))
}

fn to_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let j = vm.get(Value::object(o), PropertyKey::Atom(atoms::join))?;
    if vm.is_callable(j) {
        return vm.call(j, Value::object(o), &[]);
    }
    super::object::to_string(vm, this, &[], o)
}

fn index_of(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    let target = arg(args, 0);
    let start = relative(vm, arg(args, 1), len, 0)?;
    if dense(o) {
        let pos = o.get().elements[start as usize..].iter().position(|&e| strict_equals(e, target));
        return Ok(Value::number(pos.map(|p| (p as u64 + start) as f64).unwrap_or(-1.0)));
    }
    for i in start..len {
        if has_idx(vm, o, i) && strict_equals(get_idx(vm, o, i)?, target) {
            return Ok(Value::number(i as f64));
        }
    }
    Ok(Value::int(-1))
}

fn last_index_of(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    if len == 0 {
        return Ok(Value::int(-1));
    }
    let target = arg(args, 0);
    let from = if args.len() > 1 {
        let n = vm.to_integer(args[1])?;
        if n < 0.0 { len as f64 + n } else { n.min(len as f64 - 1.0) }
    } else {
        len as f64 - 1.0
    };
    let mut i = from;
    while i >= 0.0 {
        let k = i as u64;
        if has_idx(vm, o, k) && strict_equals(get_idx(vm, o, k)?, target) {
            return Ok(Value::number(i));
        }
        i -= 1.0;
    }
    Ok(Value::int(-1))
}

fn includes(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    let target = arg(args, 0);
    let start = relative(vm, arg(args, 1), len, 0)?;
    if dense(o) {
        return Ok(Value::bool(o.get().elements[start as usize..].iter().any(|&e| same_value_zero(e, target))));
    }
    for i in start..len {
        if same_value_zero(get_idx(vm, o, i)?, target) {
            return Ok(Value::TRUE);
        }
    }
    Ok(Value::FALSE)
}

fn at(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)? as f64;
    let n = vm.to_integer(arg(args, 0))?;
    let k = if n < 0.0 { len + n } else { n };
    if k < 0.0 || k >= len {
        return Ok(Value::UNDEFINED);
    }
    get_idx(vm, o, k as u64)
}

// ---- iteration methods ----

/// Visit present elements in order, calling `f(value, index)`; stops when
/// `f` returns Some
fn visit(
    vm: &mut Vm,
    o: Gc<JsObject>,
    reverse: bool,
    skip_holes: bool,
    mut f: impl FnMut(&mut Vm, Value, u64) -> JsResult<Option<Value>>,
) -> JsResult<Option<Value>> {
    let len = len_of(vm, o)?;
    let mark = vm.temp_mark();
    for n in 0..len {
        let i = if reverse { len - 1 - n } else { n };
        if skip_holes && !has_idx(vm, o, i) {
            continue;
        }
        let v = get_idx(vm, o, i)?;
        if let Some(r) = f(vm, v, i)? {
            return Ok(Some(r));
        }
        vm.temp_reset(mark);
    }
    Ok(None)
}

fn for_each(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let f = callback(vm, args, "forEach")?;
    let t = arg(args, 1);
    visit(vm, o, false, true, |vm, v, i| {
        vm.call(f, t, &[v, Value::number(i as f64), Value::object(o)])?;
        Ok(None)
    })?;
    Ok(Value::UNDEFINED)
}

fn map(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let f = callback(vm, args, "map")?;
    let t = arg(args, 1);
    let len = len_of(vm, o)?;
    let out = vm.new_array(vec![Value::HOLE; len.min(1 << 24) as usize]);
    if let ObjectKind::Array { length } = &mut out.get_mut().kind {
        *length = len as u32;
    }
    visit(vm, o, false, true, |vm, v, i| {
        let r = vm.call(f, t, &[v, Value::number(i as f64), Value::object(o)])?;
        let ob = out.get_mut();
        if (i as usize) < ob.elements.len() {
            ob.elements[i as usize] = r;
        } else {
            set_idx(vm, out, i, r)?;
        }
        Ok(None)
    })?;
    Ok(Value::object(out))
}

fn filter(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let f = callback(vm, args, "filter")?;
    let t = arg(args, 1);
    let out = vm.new_array(Vec::new());
    visit(vm, o, false, true, |vm, v, i| {
        let r = vm.call(f, t, &[v, Value::number(i as f64), Value::object(o)])?;
        if truthy(r) {
            let ob = out.get_mut();
            ob.elements.push(v);
            if let ObjectKind::Array { length } = &mut ob.kind {
                *length += 1;
            }
        }
        Ok(None)
    })?;
    Ok(Value::object(out))
}

fn find_impl(vm: &mut Vm, this: Value, args: &[Value], reverse: bool, want_index: bool, name: &str) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let f = callback(vm, args, name)?;
    let t = arg(args, 1);
    let found = visit(vm, o, reverse, false, |vm, v, i| {
        let r = vm.call(f, t, &[v, Value::number(i as f64), Value::object(o)])?;
        Ok(if truthy(r) { Some(if want_index { Value::number(i as f64) } else { v }) } else { None })
    })?;
    Ok(found.unwrap_or(if want_index { Value::int(-1) } else { Value::UNDEFINED }))
}

fn find(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    find_impl(vm, this, args, false, false, "find")
}

fn find_index(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    find_impl(vm, this, args, false, true, "findIndex")
}

fn find_last(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    find_impl(vm, this, args, true, false, "findLast")
}

fn find_last_index(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    find_impl(vm, this, args, true, true, "findLastIndex")
}

fn some(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let f = callback(vm, args, "some")?;
    let t = arg(args, 1);
    let r = visit(vm, o, false, true, |vm, v, i| {
        let r = vm.call(f, t, &[v, Value::number(i as f64), Value::object(o)])?;
        Ok(if truthy(r) { Some(Value::TRUE) } else { None })
    })?;
    Ok(Value::bool(r.is_some()))
}

fn every(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let f = callback(vm, args, "every")?;
    let t = arg(args, 1);
    let r = visit(vm, o, false, true, |vm, v, i| {
        let r = vm.call(f, t, &[v, Value::number(i as f64), Value::object(o)])?;
        Ok(if truthy(r) { None } else { Some(Value::FALSE) })
    })?;
    Ok(Value::bool(r.is_none()))
}

fn reduce_impl(vm: &mut Vm, this: Value, args: &[Value], reverse: bool) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let f = callback(vm, args, if reverse { "reduceRight" } else { "reduce" })?;
    let mut acc = if args.len() >= 2 { Some(args[1]) } else { None };
    visit(vm, o, reverse, true, |vm, v, i| {
        acc = Some(match acc {
            None => v,
            Some(a) => {
                let r = vm.call(f, Value::UNDEFINED, &[a, v, Value::number(i as f64), Value::object(o)])?;
                // Keep the accumulator alive across the temp reset
                vm.temp_roots.push(r);
                r
            }
        });
        Ok(None)
    })?;
    match acc {
        Some(a) => Ok(a),
        None => Err(vm.type_error("Reduce of empty array with no initial value")),
    }
}

fn reduce(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    reduce_impl(vm, this, args, false)
}

fn reduce_right(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    reduce_impl(vm, this, args, true)
}

/// Stable merge sort with a fallible comparator
fn merge_sort(vm: &mut Vm, v: &mut Vec<Value>, cmp: &mut dyn FnMut(&mut Vm, Value, Value) -> JsResult<bool>) -> JsResult<()> {
    let n = v.len();
    if n < 2 {
        return Ok(());
    }
    // Insertion sort for small runs, then merge
    const RUN: usize = 16;
    for start in (0..n).step_by(RUN) {
        let end = (start + RUN).min(n);
        for i in start + 1..end {
            let mut j = i;
            while j > start && cmp(vm, v[j], v[j - 1])? {
                v.swap(j, j - 1);
                j -= 1;
            }
        }
    }
    let mut width = RUN;
    let mut buf = v.clone();
    while width < n {
        let mut i = 0;
        while i < n {
            let mid = (i + width).min(n);
            let end = (i + 2 * width).min(n);
            let (mut a, mut b, mut k) = (i, mid, i);
            while a < mid && b < end {
                // Take from the right only if strictly less (stability)
                if cmp(vm, v[b], v[a])? {
                    buf[k] = v[b];
                    b += 1;
                } else {
                    buf[k] = v[a];
                    a += 1;
                }
                k += 1;
            }
            buf[k..k + mid - a].copy_from_slice(&v[a..mid]);
            k += mid - a;
            buf[k..k + end - b].copy_from_slice(&v[b..end]);
            i += 2 * width;
        }
        std::mem::swap(v, &mut buf);
        width *= 2;
    }
    Ok(())
}

/// Sort values with the comparator semantics of Array.prototype.sort
/// (undefined last)
fn sort_values(vm: &mut Vm, values: &mut Vec<Value>, comparefn: Value) -> JsResult<()> {
    let undefined_count = values.iter().filter(|v| v.is_undefined()).count();
    values.retain(|v| !v.is_undefined());
    if comparefn.is_undefined() {
        // Compare string forms (computed once)
        let mut keyed: Vec<(Vec<u16>, Value)> = Vec::with_capacity(values.len());
        let all_ints = values.iter().all(|v| v.is_int());
        if all_ints {
            let mut ints: Vec<(Vec<u16>, Value)> = values.iter().map(|v| (v.as_int().unwrap().to_string().encode_utf16().collect(), *v)).collect();
            ints.sort_by(|a, b| a.0.cmp(&b.0));
            *values = ints.into_iter().map(|(_, v)| v).collect();
        } else {
            for &v in values.iter() {
                let s = vm.to_string(v)?;
                keyed.push((s.get().units().to_vec(), v));
            }
            keyed.sort_by(|a, b| a.0.cmp(&b.0));
            *values = keyed.into_iter().map(|(_, v)| v).collect();
        }
    } else {
        let mark = vm.temp_mark();
        let mut cmp = |vm: &mut Vm, a: Value, b: Value| -> JsResult<bool> {
            let r = vm.call(comparefn, Value::UNDEFINED, &[a, b])?;
            let n = vm.to_number(r)?;
            vm.temp_reset(mark);
            Ok(n < 0.0)
        };
        merge_sort(vm, values, &mut cmp)?;
    }
    values.extend(std::iter::repeat_n(Value::UNDEFINED, undefined_count));
    Ok(())
}

fn sort(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let comparefn = arg(args, 0);
    if !comparefn.is_undefined() && !vm.is_callable(comparefn) {
        return Err(vm.type_error("The comparison function must be either a function or undefined"));
    }
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    let mut values = Vec::new();
    let mut holes = 0u64;
    for i in 0..len {
        if has_idx(vm, o, i) {
            values.push(get_idx(vm, o, i)?);
        } else {
            holes += 1;
        }
    }
    vm.temp_roots.extend_from_slice(&values);
    sort_values(vm, &mut values, comparefn)?;
    let n = values.len() as u64;
    if dense(o) && holes == 0 {
        o.get_mut().elements = values;
        return Ok(Value::object(o));
    }
    for (i, v) in values.into_iter().enumerate() {
        set_idx(vm, o, i as u64, v)?;
    }
    for i in n..n + holes {
        delete_idx(vm, o, i)?;
    }
    Ok(Value::object(o))
}

fn to_sorted(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let comparefn = arg(args, 0);
    if !comparefn.is_undefined() && !vm.is_callable(comparefn) {
        return Err(vm.type_error("The comparison function must be either a function or undefined"));
    }
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    let mut values = Vec::with_capacity(len as usize);
    for i in 0..len {
        values.push(get_idx(vm, o, i)?);
    }
    vm.temp_roots.extend_from_slice(&values);
    sort_values(vm, &mut values, comparefn)?;
    Ok(create_array(vm, values))
}

fn to_reversed(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    let mut values = Vec::with_capacity(len as usize);
    for i in (0..len).rev() {
        values.push(get_idx(vm, o, i)?);
    }
    Ok(create_array(vm, values))
}

fn with(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let len = len_of(vm, o)?;
    let n = vm.to_integer(arg(args, 0))?;
    let k = if n < 0.0 { len as f64 + n } else { n };
    if k < 0.0 || k >= len as f64 {
        return Err(vm.range_error("Invalid index"));
    }
    let mut values = Vec::with_capacity(len as usize);
    for i in 0..len {
        values.push(if i == k as u64 { arg(args, 1) } else { get_idx(vm, o, i)? });
    }
    Ok(create_array(vm, values))
}

fn flatten_into(vm: &mut Vm, out: &mut Vec<Value>, src: Gc<JsObject>, depth: f64) -> JsResult<()> {
    let len = len_of(vm, src)?;
    for i in 0..len {
        if !has_idx(vm, src, i) {
            continue;
        }
        let v = get_idx(vm, src, i)?;
        match v.as_object() {
            Some(inner) if depth > 0.0 && inner.get().is_array() => flatten_into(vm, out, inner, depth - 1.0)?,
            _ => out.push(v),
        }
    }
    Ok(())
}

fn flat(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let d = arg(args, 0);
    let depth = if d.is_undefined() { 1.0 } else { vm.to_integer(d)? };
    let mut out = Vec::new();
    flatten_into(vm, &mut out, o, depth)?;
    Ok(create_array(vm, out))
}

fn flat_map(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let f = callback(vm, args, "flatMap")?;
    let t = arg(args, 1);
    let len = len_of(vm, o)?;
    let mut out = Vec::new();
    for i in 0..len {
        if !has_idx(vm, o, i) {
            continue;
        }
        let v = get_idx(vm, o, i)?;
        let r = vm.call(f, t, &[v, Value::number(i as f64), Value::object(o)])?;
        match r.as_object() {
            Some(inner) if inner.get().is_array() => flatten_into(vm, &mut out, inner, 0.0)?,
            _ => out.push(r),
        }
    }
    Ok(create_array(vm, out))
}

fn make_iter(vm: &mut Vm, this: Value, kind: u8) -> JsResult<Value> {
    let o = vm.to_object(this)?;
    let proto = vm.realm.array_iterator_proto;
    let it = vm.new_object_with(Some(proto), ObjectKind::ArrayIterator { target: Value::object(o), index: 0, kind });
    Ok(Value::object(it))
}

fn values(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    make_iter(vm, this, 0)
}

fn keys(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    make_iter(vm, this, 1)
}

fn entries(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    make_iter(vm, this, 2)
}

#[allow(dead_code)]
fn unused(_: PropertyKey) -> PropertyKey {
    index_key(0)
}
