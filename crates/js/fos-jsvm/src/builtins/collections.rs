//! Map, Set, WeakMap, WeakSet

use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::{JsResult, NativeFn, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let map_proto = vm.realm.map_proto;
    vm.def_ctor("Map", 0, requires_new, Some(map_construct), map_proto);
    let methods: &[(&str, u32, NativeFn)] = &[
        ("get", 1, map_get),
        ("set", 2, map_set),
        ("has", 1, has),
        ("delete", 1, delete),
        ("clear", 0, clear),
        ("forEach", 1, for_each),
        ("keys", 0, keys),
        ("values", 0, values),
    ];
    for &(name, len, f) in methods {
        vm.def_method(map_proto, name, len, f);
    }
    let entries_fn = vm.def_method(map_proto, "entries", 0, entries);
    vm.def_getter(map_proto, "size", size);
    let it = vm.sym.iterator;
    vm.define_value(map_proto, PropertyKey::Symbol(it), Value::object(entries_fn), PropFlags::HIDDEN);

    let set_proto = vm.realm.set_proto;
    vm.def_ctor("Set", 0, requires_new, Some(set_construct), set_proto);
    let methods: &[(&str, u32, NativeFn)] = &[("add", 1, set_add), ("has", 1, has), ("delete", 1, delete), ("clear", 0, clear), ("forEach", 1, for_each), ("entries", 0, entries)];
    for &(name, len, f) in methods {
        vm.def_method(set_proto, name, len, f);
    }
    let values_fn = vm.def_method(set_proto, "values", 0, values);
    vm.def_value(set_proto, "keys", Value::object(values_fn), PropFlags::HIDDEN);
    vm.define_value(set_proto, PropertyKey::Symbol(it), Value::object(values_fn), PropFlags::HIDDEN);
    vm.def_getter(set_proto, "size", size);

    let wm = vm.realm.weakmap_proto;
    vm.def_ctor("WeakMap", 0, requires_new, Some(weakmap_construct), wm);
    vm.def_method(wm, "get", 1, map_get);
    vm.def_method(wm, "set", 2, map_set);
    vm.def_method(wm, "has", 1, has);
    vm.def_method(wm, "delete", 1, delete);
    let ws = vm.realm.weakset_proto;
    vm.def_ctor("WeakSet", 0, requires_new, Some(weakset_construct), ws);
    vm.def_method(ws, "add", 1, set_add);
    vm.def_method(ws, "has", 1, has);
    vm.def_method(ws, "delete", 1, delete);

    let tag = vm.sym.to_string_tag;
    for (p, name) in [(map_proto, "Map"), (set_proto, "Set"), (wm, "WeakMap"), (ws, "WeakSet")] {
        let s = vm.str_value(name);
        vm.define_value(p, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));
    }
}

fn requires_new(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("Constructor requires 'new'"))
}

/// SameValueZero key
pub(crate) fn map_key(v: Value) -> MapKey {
    if let Some(s) = v.as_string() {
        return MapKey::String(s.get().units().to_vec().into_boxed_slice());
    }
    if let Some(n) = v.as_number() {
        // Normalize: -0 and 0, int and double forms, all NaNs
        let canonical = if n == 0.0 { Value::int(0) } else { Value::number(n) };
        return MapKey::Bits(canonical.raw());
    }
    MapKey::Bits(v.raw())
}

fn new_collection(vm: &mut Vm, new_target: Value, default: fn(&crate::vm::Realm) -> Gc<JsObject>, kind: ObjectKind) -> JsResult<Gc<JsObject>> {
    let proto = vm.prototype_for(new_target, default)?;
    Ok(vm.new_object_with(Some(proto), kind))
}

fn fill_from(vm: &mut Vm, o: Gc<JsObject>, iterable: Value, adder: &str) -> JsResult<()> {
    if iterable.is_nullish() {
        return Ok(());
    }
    let add = vm.get_str(Value::object(o), adder)?;
    if !vm.is_callable(add) {
        return Err(vm.type_error(&format!("'{adder}' is not a function")));
    }
    let is_map = adder == "set";
    let it = vm.get_iterator(iterable)?;
    let mark = vm.temp_mark();
    while let Some(entry) = vm.iter_step(it)? {
        if is_map {
            if !entry.is_object() {
                vm.iter_close(it)?;
                return Err(vm.type_error("Iterator value is not an entry object"));
            }
            let k = vm.get(entry, PropertyKey::Index(0))?;
            let v = vm.get(entry, PropertyKey::Index(1))?;
            vm.call(add, Value::object(o), &[k, v])?;
        } else {
            vm.call(add, Value::object(o), &[entry])?;
        }
        vm.temp_reset(mark);
    }
    Ok(())
}

fn map_construct(vm: &mut Vm, nt: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = new_collection(vm, nt, |r| r.map_proto, ObjectKind::Map(Box::default()))?;
    fill_from(vm, o, arg(args, 0), "set")?;
    Ok(Value::object(o))
}

fn set_construct(vm: &mut Vm, nt: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = new_collection(vm, nt, |r| r.set_proto, ObjectKind::Set(Box::default()))?;
    fill_from(vm, o, arg(args, 0), "add")?;
    Ok(Value::object(o))
}

fn weakmap_construct(vm: &mut Vm, nt: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = new_collection(vm, nt, |r| r.weakmap_proto, ObjectKind::WeakMap(Box::default()))?;
    vm.weak_maps.borrow_mut().push(o);
    fill_from(vm, o, arg(args, 0), "set")?;
    Ok(Value::object(o))
}

fn weakset_construct(vm: &mut Vm, nt: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = new_collection(vm, nt, |r| r.weakset_proto, ObjectKind::WeakSet(Box::default()))?;
    vm.weak_maps.borrow_mut().push(o);
    fill_from(vm, o, arg(args, 0), "add")?;
    Ok(Value::object(o))
}

/// The collection data of `this`; `weak` selects which kinds are accepted
fn data<'a>(vm: &mut Vm, this: Value, method: &str) -> JsResult<(&'a mut MapData, bool)> {
    if let Some(o) = this.as_object() {
        match &mut o.get_mut_detached().kind {
            ObjectKind::Map(d) | ObjectKind::Set(d) => return Ok((d, false)),
            ObjectKind::WeakMap(d) | ObjectKind::WeakSet(d) => return Ok((d, true)),
            _ => {}
        }
    }
    Err(vm.type_error(&format!("Method {method} called on incompatible receiver")))
}

fn check_weak_key(vm: &mut Vm, weak: bool, k: Value) -> JsResult<()> {
    if weak && !k.is_object() {
        return Err(vm.type_error("Invalid value used as weak map key"));
    }
    Ok(())
}

fn map_get(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (d, _) = data(vm, this, "get")?;
    Ok(match d.index.get(&map_key(arg(args, 0))) {
        Some(&i) => d.entries[i as usize].map(|(_, v)| v).unwrap_or(Value::UNDEFINED),
        None => Value::UNDEFINED,
    })
}

fn insert(d: &mut MapData, k: Value, v: Value) {
    let key = map_key(k);
    if let Some(&i) = d.index.get(&key) {
        if let Some(e) = &mut d.entries[i as usize] {
            e.1 = v;
            return;
        }
    }
    // -0 is stored as +0
    let k = if k.as_number() == Some(0.0) { Value::int(0) } else { k };
    d.index.insert(key, d.entries.len() as u32);
    d.entries.push(Some((k, v)));
    d.size += 1;
}

fn map_set(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (d, weak) = data(vm, this, "set")?;
    check_weak_key(vm, weak, arg(args, 0))?;
    insert(d, arg(args, 0), arg(args, 1));
    vm.heap.note_growth(32);
    Ok(this)
}

fn set_add(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (d, weak) = data(vm, this, "add")?;
    let v = arg(args, 0);
    check_weak_key(vm, weak, v)?;
    insert(d, v, v);
    vm.heap.note_growth(32);
    Ok(this)
}

fn has(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (d, _) = data(vm, this, "has")?;
    Ok(Value::bool(d.index.contains_key(&map_key(arg(args, 0)))))
}

fn delete(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (d, _) = data(vm, this, "delete")?;
    match d.index.remove(&map_key(arg(args, 0))) {
        Some(i) => {
            d.entries[i as usize] = None;
            d.size -= 1;
            Ok(Value::TRUE)
        }
        None => Ok(Value::FALSE),
    }
}

fn clear(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (d, _) = data(vm, this, "clear")?;
    d.index.clear();
    for e in d.entries.iter_mut() {
        *e = None;
    }
    d.size = 0;
    Ok(Value::UNDEFINED)
}

fn size(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (d, weak) = data(vm, this, "size")?;
    if weak {
        return Err(vm.type_error("size called on incompatible receiver"));
    }
    Ok(Value::number(d.size as f64))
}

fn for_each(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let f = arg(args, 0);
    if !vm.is_callable(f) {
        return Err(vm.type_error("forEach callback is not a function"));
    }
    let t = arg(args, 1);
    let mut i = 0;
    let mark = vm.temp_mark();
    loop {
        let (d, _) = data(vm, this, "forEach")?;
        if i >= d.entries.len() {
            break;
        }
        let e = d.entries[i];
        i += 1;
        if let Some((k, v)) = e {
            vm.call(f, t, &[v, k, this])?;
            vm.temp_reset(mark);
        }
    }
    Ok(Value::UNDEFINED)
}

fn make_iter(vm: &mut Vm, this: Value, kind: u8) -> JsResult<Value> {
    let (_, weak) = data(vm, this, "iterator")?;
    if weak {
        return Err(vm.type_error("not iterable"));
    }
    let o = this.as_object().unwrap();
    let proto = if matches!(o.get().kind, ObjectKind::Map(_)) { vm.realm.map_iterator_proto } else { vm.realm.set_iterator_proto };
    let it = vm.new_object_with(Some(proto), ObjectKind::MapIterator { map: o, pos: 0, kind });
    Ok(Value::object(it))
}

fn keys(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    make_iter(vm, this, 0)
}

fn values(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    make_iter(vm, this, 1)
}

fn entries(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    make_iter(vm, this, 2)
}
