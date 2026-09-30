//! ArrayBuffer, typed arrays and DataView

use crate::gc::Gc;
use crate::object::*;
use crate::string::atoms;
use crate::value::Value;
use crate::vm::ops::f64_to_int32;
use crate::vm::{JsResult, NativeFn, Vm};

use super::arg;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaKind {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Float32,
    Float64,
}

pub const TA_KINDS: [TaKind; 9] = [
    TaKind::Int8,
    TaKind::Uint8,
    TaKind::Uint8Clamped,
    TaKind::Int16,
    TaKind::Uint16,
    TaKind::Int32,
    TaKind::Uint32,
    TaKind::Float32,
    TaKind::Float64,
];

impl TaKind {
    pub fn size(self) -> usize {
        match self {
            TaKind::Int8 | TaKind::Uint8 | TaKind::Uint8Clamped => 1,
            TaKind::Int16 | TaKind::Uint16 => 2,
            TaKind::Int32 | TaKind::Uint32 | TaKind::Float32 => 4,
            TaKind::Float64 => 8,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            TaKind::Int8 => "Int8Array",
            TaKind::Uint8 => "Uint8Array",
            TaKind::Uint8Clamped => "Uint8ClampedArray",
            TaKind::Int16 => "Int16Array",
            TaKind::Uint16 => "Uint16Array",
            TaKind::Int32 => "Int32Array",
            TaKind::Uint32 => "Uint32Array",
            TaKind::Float32 => "Float32Array",
            TaKind::Float64 => "Float64Array",
        }
    }
}

/// Read element bytes (little-endian)
#[inline]
pub fn read_elem(buf: &[u8], kind: TaKind, at: usize) -> Value {
    macro_rules! rd {
        ($t:ty, $n:expr) => {
            <$t>::from_le_bytes(buf[at..at + $n].try_into().unwrap())
        };
    }
    match kind {
        TaKind::Int8 => Value::int(buf[at] as i8 as i32),
        TaKind::Uint8 | TaKind::Uint8Clamped => Value::int(buf[at] as i32),
        TaKind::Int16 => Value::int(rd!(i16, 2) as i32),
        TaKind::Uint16 => Value::int(rd!(u16, 2) as i32),
        TaKind::Int32 => Value::int(rd!(i32, 4)),
        TaKind::Uint32 => Value::number(rd!(u32, 4) as f64),
        TaKind::Float32 => Value::number(rd!(f32, 4) as f64),
        TaKind::Float64 => Value::number(rd!(f64, 8)),
    }
}

/// Write a number as element bytes
#[inline]
pub fn write_elem(buf: &mut [u8], kind: TaKind, at: usize, n: f64) {
    match kind {
        TaKind::Int8 | TaKind::Uint8 => buf[at] = f64_to_int32(n) as u8,
        TaKind::Uint8Clamped => {
            buf[at] = if n.is_nan() || n <= 0.0 {
                0
            } else if n >= 255.0 {
                255
            } else {
                // Round half to even
                let f = n.floor();
                let d = n - f;
                (if d > 0.5 || (d == 0.5 && f % 2.0 != 0.0) { f + 1.0 } else { f }) as u8
            };
        }
        TaKind::Int16 | TaKind::Uint16 => buf[at..at + 2].copy_from_slice(&(f64_to_int32(n) as u16).to_le_bytes()),
        TaKind::Int32 | TaKind::Uint32 => buf[at..at + 4].copy_from_slice(&(f64_to_int32(n) as u32).to_le_bytes()),
        TaKind::Float32 => buf[at..at + 4].copy_from_slice(&(n as f32).to_le_bytes()),
        TaKind::Float64 => buf[at..at + 8].copy_from_slice(&n.to_le_bytes()),
    }
}

/// The byte storage of an ArrayBuffer object
pub fn buffer_bytes(b: Gc<JsObject>) -> &'static mut Vec<u8> {
    match &mut b.get_mut_detached().kind {
        ObjectKind::ArrayBuffer(v) => v,
        _ => unreachable!("not an ArrayBuffer"),
    }
}

/// Element `i` of a typed array, if in bounds
#[inline]
pub fn ta_get(o: &JsObject, i: u32) -> Option<Value> {
    match &o.kind {
        ObjectKind::TypedArray(t) if i < t.length => {
            let buf = buffer_bytes(t.buffer);
            Some(read_elem(buf, t.kind, t.offset as usize + i as usize * t.kind.size()))
        }
        _ => None,
    }
}

/// Store into element `i` (true if `o` is a typed array; out-of-bounds
/// stores are ignored)
#[inline]
pub fn ta_set(o: &JsObject, i: u32, n: f64) -> bool {
    match &o.kind {
        ObjectKind::TypedArray(t) => {
            if i < t.length {
                let buf = buffer_bytes(t.buffer);
                write_elem(buf, t.kind, t.offset as usize + i as usize * t.kind.size(), n);
            }
            true
        }
        _ => false,
    }
}

pub(super) fn init(vm: &mut Vm) {
    // ArrayBuffer
    let ab_proto = vm.new_object();
    let ab = vm.def_ctor("ArrayBuffer", 1, requires_new, Some(array_buffer_construct), ab_proto);
    vm.def_method(ab, "isView", 1, is_view);
    vm.def_getter(ab_proto, "byteLength", buffer_byte_length);
    vm.def_method(ab_proto, "slice", 2, buffer_slice);
    vm.realm_extra.array_buffer_proto = Some(ab_proto);
    let tag = vm.sym.to_string_tag;
    let s = vm.str_value("ArrayBuffer");
    vm.define_value(ab_proto, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));

    // %TypedArray%.prototype
    let ta_proto = vm.new_object();
    let getters: &[(&str, NativeFn)] = &[("length", ta_length), ("byteLength", ta_byte_length), ("byteOffset", ta_byte_offset), ("buffer", ta_buffer)];
    for &(name, f) in getters {
        vm.def_getter(ta_proto, name, f);
    }
    let methods: &[(&str, u32, NativeFn)] = &[
        ("set", 1, ta_set_method),
        ("subarray", 2, subarray),
        ("slice", 2, ta_slice),
        ("map", 1, ta_map),
        ("filter", 1, ta_filter),
        ("fill", 1, ta_fill),
        ("sort", 1, ta_sort),
        ("reverse", 0, ta_reverse),
    ];
    for &(name, len, f) in methods {
        vm.def_method(ta_proto, name, len, f);
    }
    // Generic Array methods work on typed arrays as array-likes
    let ap = vm.realm.array_proto;
    for name in ["forEach", "join", "indexOf", "lastIndexOf", "includes", "every", "some", "find", "findIndex", "findLast", "findLastIndex", "reduce", "reduceRight", "at", "keys", "entries", "toString", "toLocaleString"] {
        let f = vm.get_str(Value::object(ap), name).unwrap();
        vm.def_value(ta_proto, name, f, PropFlags::HIDDEN);
    }
    let values = vm.realm.array_values;
    vm.def_value(ta_proto, "values", Value::object(values), PropFlags::HIDDEN);
    let it = vm.sym.iterator;
    vm.define_value(ta_proto, PropertyKey::Symbol(it), Value::object(values), PropFlags::HIDDEN);
    let getter = vm.new_native("get [Symbol.toStringTag]", 0, ta_tag, None);
    vm.define_accessor(ta_proto, PropertyKey::Symbol(tag), Some(Value::object(getter)), None, PropFlags(PropFlags::CONFIGURABLE));

    let ta_ctor = vm.new_native("TypedArray", 0, requires_new, None);
    vm.define_value(ta_ctor, PropertyKey::Atom(atoms::prototype), Value::object(ta_proto), PropFlags::FROZEN);
    vm.define_value(ta_proto, PropertyKey::Atom(atoms::constructor), Value::object(ta_ctor), PropFlags::HIDDEN);
    vm.def_method(ta_ctor, "from", 1, ta_from);
    vm.def_method(ta_ctor, "of", 0, ta_of);

    for kind in TA_KINDS {
        let proto = vm.new_object_with(Some(ta_proto), ObjectKind::Ordinary);
        let c = vm.def_ctor(kind.name(), 3, requires_new, Some(ta_construct), proto);
        c.get_mut().proto = Some(ta_ctor);
        ta_ctor.get_mut().is_prototype = true;
        vm.def_value(c, "BYTES_PER_ELEMENT", Value::int(kind.size() as i32), PropFlags::FROZEN);
        vm.def_value(proto, "BYTES_PER_ELEMENT", Value::int(kind.size() as i32), PropFlags::FROZEN);
        if let ObjectKind::Native(n) = &mut c.get_mut().kind {
            n.data = Value::int(kind as i32);
        }
        vm.realm_extra.typed_array_protos.push(proto);
    }

    // DataView
    let dv_proto = vm.new_object();
    vm.def_ctor("DataView", 1, requires_new, Some(data_view_construct), dv_proto);
    let dv_getters: &[(&str, NativeFn)] = &[("buffer", dv_buffer), ("byteLength", dv_byte_length), ("byteOffset", dv_byte_offset)];
    for &(name, f) in dv_getters {
        vm.def_getter(dv_proto, name, f);
    }
    macro_rules! dv_methods {
        ($($get:literal, $set:literal => $kind:expr),* $(,)?) => {
            $(
                {
                    fn g(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
                        dv_get(vm, this, args, $kind)
                    }
                    fn s(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
                        dv_set(vm, this, args, $kind)
                    }
                    vm.def_method(dv_proto, $get, 1, g);
                    vm.def_method(dv_proto, $set, 2, s);
                }
            )*
        };
    }
    dv_methods! {
        "getInt8", "setInt8" => TaKind::Int8,
        "getUint8", "setUint8" => TaKind::Uint8,
        "getInt16", "setInt16" => TaKind::Int16,
        "getUint16", "setUint16" => TaKind::Uint16,
        "getInt32", "setInt32" => TaKind::Int32,
        "getUint32", "setUint32" => TaKind::Uint32,
        "getFloat32", "setFloat32" => TaKind::Float32,
        "getFloat64", "setFloat64" => TaKind::Float64,
    }
}

fn requires_new(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("Constructor requires 'new'"))
}

fn to_index(vm: &mut Vm, v: Value, what: &str) -> JsResult<usize> {
    if v.is_undefined() {
        return Ok(0);
    }
    let n = vm.to_integer(v)?;
    if !(0.0..=2147483647.0).contains(&n) {
        return Err(vm.range_error(&format!("Invalid {what}")));
    }
    Ok(n as usize)
}

pub(crate) fn new_buffer(vm: &mut Vm, len: usize) -> Gc<JsObject> {
    let proto = vm.realm_extra.array_buffer_proto;
    let b = vm.new_object_with(proto, ObjectKind::ArrayBuffer(Box::new(vec![0u8; len])));
    vm.heap.note_growth(len);
    b
}

fn array_buffer_construct(vm: &mut Vm, new_target: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let len = to_index(vm, arg(args, 0), "array buffer length")?;
    let b = new_buffer(vm, len);
    if let Some(p) = vm.realm_extra.array_buffer_proto {
        let proto = vm.prototype_for(new_target, |r| r.object_proto)?;
        b.get_mut().proto = Some(if proto == vm.realm.object_proto { p } else { proto });
    }
    Ok(Value::object(b))
}

fn is_view(_vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(arg(args, 0).as_object().is_some_and(|o| matches!(o.get().kind, ObjectKind::TypedArray(_) | ObjectKind::DataView(_)))))
}

fn this_buffer(vm: &mut Vm, this: Value) -> JsResult<Gc<JsObject>> {
    match this.as_object() {
        Some(o) if matches!(o.get().kind, ObjectKind::ArrayBuffer(_)) => Ok(o),
        _ => Err(vm.type_error("not an ArrayBuffer")),
    }
}

fn buffer_byte_length(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let b = this_buffer(vm, this)?;
    Ok(Value::number(buffer_bytes(b).len() as f64))
}

fn buffer_slice(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let b = this_buffer(vm, this)?;
    let len = buffer_bytes(b).len() as f64;
    let rel = |vm: &mut Vm, v: Value, d: f64| -> JsResult<usize> {
        if v.is_undefined() {
            return Ok(d as usize);
        }
        let n = vm.to_integer(v)?;
        Ok(if n < 0.0 { (len + n).max(0.0) } else { n.min(len) } as usize)
    };
    let start = rel(vm, arg(args, 0), 0.0)?;
    let end = rel(vm, arg(args, 1), len)?.max(start);
    let nb = new_buffer(vm, end - start);
    buffer_bytes(nb).copy_from_slice(&buffer_bytes(b)[start..end]);
    Ok(Value::object(nb))
}

fn this_ta(vm: &mut Vm, this: Value) -> JsResult<(Gc<JsObject>, TypedArrayData)> {
    if let Some(o) = this.as_object() {
        if let ObjectKind::TypedArray(t) = &o.get().kind {
            return Ok((o, **t));
        }
    }
    Err(vm.type_error("this is not a typed array."))
}

/// A new typed array of `kind` with `len` zeroed elements
pub(crate) fn new_typed_array(vm: &mut Vm, kind: TaKind, len: usize, proto: Option<Gc<JsObject>>) -> JsResult<Gc<JsObject>> {
    if len > (1 << 30) {
        return Err(vm.range_error("Invalid typed array length"));
    }
    let buffer = new_buffer(vm, len * kind.size());
    let proto = proto.unwrap_or(vm.realm_extra.typed_array_protos[kind as usize]);
    Ok(vm.new_object_with(Some(proto), ObjectKind::TypedArray(Box::new(TypedArrayData { kind, buffer, offset: 0, length: len as u32 }))))
}

fn kind_of_ctor(callee: Gc<JsObject>) -> TaKind {
    match &callee.get().kind {
        ObjectKind::Native(n) => TA_KINDS[n.data.as_int().unwrap_or(1) as usize],
        _ => TaKind::Uint8,
    }
}

fn ta_construct(vm: &mut Vm, new_target: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    let kind = kind_of_ctor(callee);
    let default_proto = vm.realm_extra.typed_array_protos[kind as usize];
    let proto = {
        let p = vm.get(new_target, PropertyKey::Atom(atoms::prototype))?;
        p.as_object().unwrap_or(default_proto)
    };
    let first = arg(args, 0);
    let Some(src) = first.as_object() else {
        let len = to_index(vm, first, "typed array length")?;
        return Ok(Value::object(new_typed_array(vm, kind, len, Some(proto))?));
    };
    // View over a buffer
    if matches!(src.get().kind, ObjectKind::ArrayBuffer(_)) {
        let blen = buffer_bytes(src).len();
        let offset = to_index(vm, arg(args, 1), "offset")?;
        if offset % kind.size() != 0 {
            return Err(vm.range_error(&format!("start offset of {} should be a multiple of {}", kind.name(), kind.size())));
        }
        let length = if arg(args, 2).is_undefined() {
            if blen % kind.size() != 0 || offset > blen {
                return Err(vm.range_error("Invalid typed array length"));
            }
            (blen - offset) / kind.size()
        } else {
            let l = to_index(vm, arg(args, 2), "typed array length")?;
            if offset + l * kind.size() > blen {
                return Err(vm.range_error("Invalid typed array length"));
            }
            l
        };
        let o = vm.new_object_with(Some(proto), ObjectKind::TypedArray(Box::new(TypedArrayData { kind, buffer: src, offset: offset as u32, length: length as u32 })));
        return Ok(Value::object(o));
    }
    // Copy from a typed array, iterable or array-like
    let values: Vec<Value> = if let ObjectKind::TypedArray(t) = &src.get().kind {
        (0..t.length).map(|i| ta_get(src.get(), i).unwrap()).collect()
    } else {
        let iter = vm.get(first, PropertyKey::Symbol(vm.sym.iterator))?;
        if !iter.is_nullish() {
            let it = vm.get_iterator(first)?;
            let mut v = Vec::new();
            while let Some(x) = vm.iter_step(it)? {
                v.push(x);
            }
            v
        } else {
            super::function::list_from_array_like(vm, first)?
        }
    };
    let o = new_typed_array(vm, kind, values.len(), Some(proto))?;
    for (i, v) in values.into_iter().enumerate() {
        let n = vm.to_number(v)?;
        ta_set(o.get(), i as u32, n);
    }
    Ok(Value::object(o))
}

fn ta_length(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(this_ta(vm, this)?.1.length as f64))
}

fn ta_byte_length(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (_, t) = this_ta(vm, this)?;
    Ok(Value::number((t.length as usize * t.kind.size()) as f64))
}

fn ta_byte_offset(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(this_ta(vm, this)?.1.offset as f64))
}

fn ta_buffer(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::object(this_ta(vm, this)?.1.buffer))
}

fn ta_tag(_vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if let Some(o) = this.as_object() {
        if let ObjectKind::TypedArray(t) = &o.get().kind {
            let name = t.kind.name();
            return Ok(_vm.str_value(name));
        }
    }
    Ok(Value::UNDEFINED)
}

fn ta_set_method(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (o, t) = this_ta(vm, this)?;
    let offset = to_index(vm, arg(args, 1), "offset")?;
    let src = arg(args, 0);
    let typed = src.as_object().and_then(|s| match &s.get().kind {
        ObjectKind::TypedArray(st) => Some((s, st.length)),
        _ => None,
    });
    let values: Vec<Value> = match typed {
        Some((s, len)) => (0..len).map(|i| ta_get(s.get(), i).unwrap()).collect(),
        None => super::function::list_from_array_like(vm, src)?,
    };
    if offset + values.len() > t.length as usize {
        return Err(vm.range_error("offset is out of bounds"));
    }
    for (i, v) in values.into_iter().enumerate() {
        let n = vm.to_number(v)?;
        ta_set(o.get(), (offset + i) as u32, n);
    }
    Ok(Value::UNDEFINED)
}

fn rel_index(vm: &mut Vm, v: Value, len: usize, default: usize) -> JsResult<usize> {
    if v.is_undefined() {
        return Ok(default);
    }
    let n = vm.to_integer(v)?;
    Ok(if n < 0.0 { (len as f64 + n).max(0.0) as usize } else { (n as usize).min(len) })
}

fn subarray(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (o, t) = this_ta(vm, this)?;
    let len = t.length as usize;
    let start = rel_index(vm, arg(args, 0), len, 0)?;
    let end = rel_index(vm, arg(args, 1), len, len)?.max(start);
    let proto = o.get().proto;
    let data = TypedArrayData { kind: t.kind, buffer: t.buffer, offset: t.offset + (start * t.kind.size()) as u32, length: (end - start) as u32 };
    Ok(Value::object(vm.new_object_with(proto, ObjectKind::TypedArray(Box::new(data)))))
}

fn ta_slice(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (o, t) = this_ta(vm, this)?;
    let len = t.length as usize;
    let start = rel_index(vm, arg(args, 0), len, 0)?;
    let end = rel_index(vm, arg(args, 1), len, len)?.max(start);
    let n = new_typed_array(vm, t.kind, end - start, o.get().proto)?;
    let sz = t.kind.size();
    let src = buffer_bytes(t.buffer)[t.offset as usize + start * sz..t.offset as usize + end * sz].to_vec();
    if let ObjectKind::TypedArray(nt) = &n.get().kind {
        buffer_bytes(nt.buffer).copy_from_slice(&src);
    }
    Ok(Value::object(n))
}

fn callback_args(vm: &mut Vm, args: &[Value]) -> JsResult<(Value, Value)> {
    let f = arg(args, 0);
    if !vm.is_callable(f) {
        return Err(vm.type_error("callback is not a function"));
    }
    Ok((f, arg(args, 1)))
}

fn ta_map(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (o, t) = this_ta(vm, this)?;
    let (f, this_arg) = callback_args(vm, args)?;
    let n = new_typed_array(vm, t.kind, t.length as usize, o.get().proto)?;
    for i in 0..t.length {
        let v = ta_get(o.get(), i).unwrap_or(Value::UNDEFINED);
        let r = vm.call(f, this_arg, &[v, Value::number(i as f64), this])?;
        let x = vm.to_number(r)?;
        ta_set(n.get(), i, x);
    }
    Ok(Value::object(n))
}

fn ta_filter(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (o, t) = this_ta(vm, this)?;
    let (f, this_arg) = callback_args(vm, args)?;
    let mut kept = Vec::new();
    for i in 0..t.length {
        let v = ta_get(o.get(), i).unwrap_or(Value::UNDEFINED);
        let r = vm.call(f, this_arg, &[v, Value::number(i as f64), this])?;
        if crate::vm::ops::truthy(r) {
            kept.push(v);
        }
    }
    let n = new_typed_array(vm, t.kind, kept.len(), o.get().proto)?;
    for (i, v) in kept.into_iter().enumerate() {
        ta_set(n.get(), i as u32, v.number_unchecked());
    }
    Ok(Value::object(n))
}

fn ta_fill(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (o, t) = this_ta(vm, this)?;
    let v = vm.to_number(arg(args, 0))?;
    let len = t.length as usize;
    let start = rel_index(vm, arg(args, 1), len, 0)?;
    let end = rel_index(vm, arg(args, 2), len, len)?;
    for i in start..end {
        ta_set(o.get(), i as u32, v);
    }
    Ok(this)
}

fn ta_sort(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (o, t) = this_ta(vm, this)?;
    let cmp = arg(args, 0);
    let mut values: Vec<Value> = (0..t.length).map(|i| ta_get(o.get(), i).unwrap()).collect();
    if cmp.is_undefined() {
        values.sort_by(|a, b| {
            let (x, y) = (a.number_unchecked(), b.number_unchecked());
            x.partial_cmp(&y).unwrap_or_else(|| x.is_nan().cmp(&y.is_nan()))
        });
    } else {
        // Insertion into a sorted vector keeps comparator errors simple
        let mut sorted: Vec<Value> = Vec::with_capacity(values.len());
        for v in values {
            let mut lo = 0;
            let mut hi = sorted.len();
            while lo < hi {
                let mid = (lo + hi) / 2;
                let r = vm.call(cmp, Value::UNDEFINED, &[v, sorted[mid]])?;
                if vm.to_number(r)? < 0.0 {
                    hi = mid;
                } else {
                    lo = mid + 1;
                }
            }
            sorted.insert(lo, v);
        }
        values = sorted;
    }
    for (i, v) in values.into_iter().enumerate() {
        ta_set(o.get(), i as u32, v.number_unchecked());
    }
    Ok(this)
}

fn ta_reverse(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (o, t) = this_ta(vm, this)?;
    let values: Vec<Value> = (0..t.length).rev().map(|i| ta_get(o.get(), i).unwrap()).collect();
    for (i, v) in values.into_iter().enumerate() {
        ta_set(o.get(), i as u32, v.number_unchecked());
    }
    Ok(this)
}

fn ctor_kind(vm: &mut Vm, this: Value) -> JsResult<TaKind> {
    match this.as_object() {
        Some(c) if matches!(&c.get().kind, ObjectKind::Native(n) if n.data.is_int()) => Ok(kind_of_ctor(c)),
        _ => Err(vm.type_error("TypedArray.from/of must be called on a typed array constructor")),
    }
}

fn ta_from(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let kind = ctor_kind(vm, this)?;
    let src = super::array::from_values(vm, arg(args, 0), arg(args, 1), arg(args, 2))?;
    let o = new_typed_array(vm, kind, src.len(), None)?;
    for (i, v) in src.into_iter().enumerate() {
        let n = vm.to_number(v)?;
        ta_set(o.get(), i as u32, n);
    }
    Ok(Value::object(o))
}

fn ta_of(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let kind = ctor_kind(vm, this)?;
    let o = new_typed_array(vm, kind, args.len(), None)?;
    for (i, &v) in args.iter().enumerate() {
        let n = vm.to_number(v)?;
        ta_set(o.get(), i as u32, n);
    }
    Ok(Value::object(o))
}

// ---- DataView ----

fn data_view_construct(vm: &mut Vm, new_target: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let b = match arg(args, 0).as_object() {
        Some(b) if matches!(b.get().kind, ObjectKind::ArrayBuffer(_)) => b,
        _ => return Err(vm.type_error("First argument to DataView constructor must be an ArrayBuffer")),
    };
    let blen = buffer_bytes(b).len();
    let offset = to_index(vm, arg(args, 1), "DataView offset")?;
    if offset > blen {
        return Err(vm.range_error("Start offset is outside the bounds of the buffer"));
    }
    let length = if arg(args, 2).is_undefined() { blen - offset } else { to_index(vm, arg(args, 2), "DataView length")? };
    if offset + length > blen {
        return Err(vm.range_error("Invalid DataView length"));
    }
    let proto = vm.prototype_for(new_target, |r| r.object_proto)?;
    let o = vm.new_object_with(Some(proto), ObjectKind::DataView(Box::new(TypedArrayData { kind: TaKind::Uint8, buffer: b, offset: offset as u32, length: length as u32 })));
    Ok(Value::object(o))
}

fn this_dv(vm: &mut Vm, this: Value) -> JsResult<TypedArrayData> {
    if let Some(o) = this.as_object() {
        if let ObjectKind::DataView(d) = &o.get().kind {
            return Ok(**d);
        }
    }
    Err(vm.type_error("not a DataView"))
}

fn dv_buffer(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::object(this_dv(vm, this)?.buffer))
}

fn dv_byte_length(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(this_dv(vm, this)?.length as f64))
}

fn dv_byte_offset(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(this_dv(vm, this)?.offset as f64))
}

fn dv_index(vm: &mut Vm, d: &TypedArrayData, v: Value, kind: TaKind) -> JsResult<usize> {
    let i = to_index(vm, v, "offset")?;
    if i + kind.size() > d.length as usize {
        return Err(vm.range_error("Offset is outside the bounds of the DataView"));
    }
    Ok(d.offset as usize + i)
}

fn dv_get(vm: &mut Vm, this: Value, args: &[Value], kind: TaKind) -> JsResult<Value> {
    let d = this_dv(vm, this)?;
    let at = dv_index(vm, &d, arg(args, 0), kind)?;
    let little = crate::vm::ops::truthy(arg(args, 1));
    let buf = buffer_bytes(d.buffer);
    let mut bytes = buf[at..at + kind.size()].to_vec();
    if !little {
        bytes.reverse();
    }
    Ok(read_elem(&bytes, kind, 0))
}

fn dv_set(vm: &mut Vm, this: Value, args: &[Value], kind: TaKind) -> JsResult<Value> {
    let d = this_dv(vm, this)?;
    let at = dv_index(vm, &d, arg(args, 0), kind)?;
    let n = vm.to_number(arg(args, 1))?;
    let little = crate::vm::ops::truthy(arg(args, 2));
    let mut bytes = vec![0u8; kind.size()];
    write_elem(&mut bytes, kind, 0, n);
    if !little {
        bytes.reverse();
    }
    buffer_bytes(d.buffer)[at..at + kind.size()].copy_from_slice(&bytes);
    Ok(Value::UNDEFINED)
}
