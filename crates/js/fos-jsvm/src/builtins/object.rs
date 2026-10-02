//! Object

use crate::gc::Gc;
use crate::object::*;
use crate::string::atoms;
use crate::value::Value;
use crate::vm::property::Own;
use crate::vm::{JsResult, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.object_proto;
    let c = vm.def_ctor("Object", 1, object_call, Some(object_construct), proto);
    vm.def_method(c, "keys", 1, keys);
    vm.def_method(c, "values", 1, values);
    vm.def_method(c, "entries", 1, entries);
    vm.def_method(c, "assign", 2, assign);
    vm.def_method(c, "create", 2, create);
    vm.def_method(c, "defineProperty", 3, define_property);
    vm.def_method(c, "defineProperties", 2, define_properties);
    vm.def_method(c, "getOwnPropertyDescriptor", 2, get_own_property_descriptor);
    vm.def_method(c, "getOwnPropertyDescriptors", 1, get_own_property_descriptors);
    vm.def_method(c, "getOwnPropertyNames", 1, get_own_property_names);
    vm.def_method(c, "getOwnPropertySymbols", 1, get_own_property_symbols);
    vm.def_method(c, "getPrototypeOf", 1, get_prototype_of);
    vm.def_method(c, "setPrototypeOf", 2, set_prototype_of);
    vm.def_method(c, "freeze", 1, freeze);
    vm.def_method(c, "isFrozen", 1, is_frozen);
    vm.def_method(c, "seal", 1, seal);
    vm.def_method(c, "isSealed", 1, is_sealed);
    vm.def_method(c, "preventExtensions", 1, prevent_extensions);
    vm.def_method(c, "isExtensible", 1, is_extensible);
    vm.def_method(c, "fromEntries", 1, from_entries);
    vm.def_method(c, "is", 2, is);
    vm.def_method(c, "hasOwn", 2, has_own);

    vm.def_method(proto, "hasOwnProperty", 1, has_own_property);
    vm.def_method(proto, "isPrototypeOf", 1, is_prototype_of);
    vm.def_method(proto, "propertyIsEnumerable", 1, property_is_enumerable);
    vm.def_method(proto, "toString", 0, to_string);
    vm.def_method(proto, "toLocaleString", 0, to_locale_string);
    vm.def_method(proto, "valueOf", 0, value_of);
    let get = vm.new_native("get __proto__", 0, proto_getter, None);
    let set = vm.new_native("set __proto__", 1, proto_setter, None);
    let key = vm.key_from_str("__proto__");
    vm.define_accessor(proto, key, Some(Value::object(get)), Some(Value::object(set)), PropFlags(PropFlags::CONFIGURABLE));
}

fn object_call(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let v = arg(args, 0);
    if v.is_nullish() {
        return Ok(Value::object(vm.new_object()));
    }
    Ok(Value::object(vm.to_object(v)?))
}

fn object_construct(vm: &mut Vm, new_target: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    if new_target != Value::object(callee) {
        let proto = vm.prototype_for(new_target, |r| r.object_proto)?;
        return Ok(Value::object(vm.new_object_with(Some(proto), ObjectKind::Ordinary)));
    }
    object_call(vm, Value::UNDEFINED, args, callee)
}

fn keys(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(arg(args, 0))?;
    let keys: Vec<PropertyKey> = vm.own_keys_js(o)?.into_iter().filter(|(k, f)| f.enumerable() && !matches!(k, PropertyKey::Symbol(_))).map(|(k, _)| k).collect();
    let values: Vec<Value> = keys.into_iter().map(|k| vm.key_value(k)).collect();
    Ok(Value::object(vm.new_array(values)))
}

fn values(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(arg(args, 0))?;
    let mut out = Vec::new();
    for k in vm.enumerable_own_keys(o) {
        out.push(vm.get(Value::object(o), k)?);
    }
    Ok(Value::object(vm.new_array(out)))
}

fn entries(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(arg(args, 0))?;
    let mut out = Vec::new();
    for k in vm.enumerable_own_keys(o) {
        let v = vm.get(Value::object(o), k)?;
        let kv = vm.key_value(k);
        out.push(Value::object(vm.new_array(vec![kv, v])));
    }
    Ok(Value::object(vm.new_array(out)))
}

fn assign(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let target = vm.to_object(arg(args, 0))?;
    for &src in args.iter().skip(1) {
        if src.is_nullish() {
            continue;
        }
        let from = vm.to_object(src)?;
        for (k, f) in vm.own_keys(from) {
            if !f.enumerable() {
                continue;
            }
            let v = vm.get(Value::object(from), k)?;
            vm.set(Value::object(target), k, v, true)?;
        }
    }
    Ok(Value::object(target))
}

fn create(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let p = arg(args, 0);
    let proto = if p.is_null() {
        None
    } else if let Some(o) = p.as_object() {
        Some(o)
    } else {
        return Err(vm.type_error("Object prototype may only be an Object or null"));
    };
    let o = vm.new_object_with(proto, ObjectKind::Ordinary);
    let props = arg(args, 1);
    if !props.is_undefined() {
        define_properties_from(vm, o, props)?;
    }
    Ok(Value::object(o))
}

/// A property descriptor being applied
#[derive(Default)]
pub(crate) struct Descriptor {
    pub value: Option<Value>,
    pub writable: Option<bool>,
    pub get: Option<Value>,
    pub set: Option<Value>,
    pub enumerable: Option<bool>,
    pub configurable: Option<bool>,
}

pub(crate) fn to_descriptor(vm: &mut Vm, d: Value) -> JsResult<Descriptor> {
    if !d.is_object() {
        return Err(vm.type_error("Property description must be an object"));
    }
    let o = d.as_object().unwrap();
    let mut desc = Descriptor::default();
    let field = |vm: &mut Vm, name| -> JsResult<Option<Value>> {
        let key = PropertyKey::Atom(name);
        if vm.has_property(o, key) { Ok(Some(vm.get(d, key)?)) } else { Ok(None) }
    };
    desc.enumerable = field(vm, atoms::enumerable)?.map(crate::vm::ops::truthy);
    desc.configurable = field(vm, atoms::configurable)?.map(crate::vm::ops::truthy);
    desc.value = field(vm, atoms::value)?;
    desc.writable = field(vm, atoms::writable)?.map(crate::vm::ops::truthy);
    desc.get = field(vm, atoms::get)?;
    desc.set = field(vm, atoms::set)?;
    for f in [desc.get, desc.set].into_iter().flatten() {
        if !f.is_undefined() && !vm.is_callable(f) {
            return Err(vm.type_error("Getter/setter must be a function"));
        }
    }
    if (desc.get.is_some() || desc.set.is_some()) && (desc.value.is_some() || desc.writable.is_some()) {
        return Err(vm.type_error("Invalid property descriptor. Cannot both specify accessors and a value or writable attribute"));
    }
    Ok(desc)
}

/// ValidateAndApplyPropertyDescriptor (without the full set of
/// invariant checks for non-configurable properties)
pub(crate) fn define_from_descriptor(vm: &mut Vm, o: Gc<JsObject>, key: PropertyKey, desc: &Descriptor) -> JsResult<bool> {
    let existing = vm.own_prop(o, key);
    let (old_value, old_flags) = match existing {
        Some(Own::Slot(slot, flags)) => (Some(o.get().read(slot)), Some(flags)),
        Some(Own::Virtual(v, flags)) => (Some(v), Some(flags)),
        None => (None, None),
    };
    if let Some(of) = old_flags {
        if !of.configurable() {
            let changes_kind = (desc.get.is_some() || desc.set.is_some()) != of.is_accessor() && (desc.get.is_some() || desc.set.is_some() || desc.value.is_some());
            let bad = desc.configurable == Some(true)
                || desc.enumerable.is_some_and(|e| e != of.enumerable())
                || changes_kind
                || (!of.is_accessor() && !of.writable() && (desc.writable == Some(true) || desc.value.is_some_and(|v| !old_value.is_some_and(|ov| crate::vm::ops::same_value(ov, v)))));
            if bad {
                return Ok(false);
            }
        }
        if let Some(Own::Virtual(..)) = existing {
            if o.get().is_array() && key == PropertyKey::Atom(atoms::length) {
                if let Some(v) = desc.value {
                    vm.set_array_length(o, v)?;
                }
                if desc.writable == Some(false) {
                    o.get_mut().length_readonly = true;
                }
                return Ok(true);
            }
            return Ok(false);
        }
    } else if !o.get().extensible || crate::vm::property::grows_readonly_length(o, key) {
        return Ok(false);
    }
    let base = old_flags.unwrap_or(PropFlags::NONE);
    let enumerable = desc.enumerable.unwrap_or(base.enumerable());
    let configurable = desc.configurable.unwrap_or(base.configurable());
    // A generic descriptor ({ enumerable } alone) keeps an accessor an
    // accessor, with its getter and setter
    let generic = desc.get.is_none() && desc.set.is_none() && desc.value.is_none() && desc.writable.is_none();
    if desc.get.is_some() || desc.set.is_some() || (generic && old_flags.is_some_and(|f| f.is_accessor())) {
        let mut flags = PropFlags::NONE.with(PropFlags::ENUMERABLE, enumerable).with(PropFlags::CONFIGURABLE, configurable);
        if old_flags.is_some_and(|f| !f.is_accessor()) {
            // Data -> accessor: replace
            flags = flags.with(PropFlags::ACCESSOR, false);
            vm.delete_property(Value::object(o), key, false)?;
        }
        vm.define_accessor(o, key, desc.get, desc.set, flags);
        return Ok(true);
    }
    let writable = desc.writable.unwrap_or(if old_flags.is_some_and(|f| !f.is_accessor()) { base.writable() } else { false });
    let flags = PropFlags::NONE
        .with(PropFlags::WRITABLE, writable)
        .with(PropFlags::ENUMERABLE, enumerable)
        .with(PropFlags::CONFIGURABLE, configurable);
    let value = match desc.value {
        Some(v) => v,
        None => match old_flags {
            Some(f) if !f.is_accessor() => old_value.unwrap(),
            _ => Value::UNDEFINED,
        },
    };
    if old_flags.is_some_and(|f| f.is_accessor()) {
        vm.delete_property(Value::object(o), key, false)?;
    }
    vm.define_value(o, key, value, flags);
    Ok(true)
}

fn define_property(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let target = arg(args, 0);
    let Some(o) = target.as_object() else {
        return Err(vm.type_error("Object.defineProperty called on non-object"));
    };
    let key = vm.to_property_key(arg(args, 1))?;
    if super::proxy::is_proxy(o) {
        if !vm.proxy_define(o, key, arg(args, 2))? {
            return Err(vm.type_error("'defineProperty' on proxy: trap returned falsish"));
        }
        return Ok(target);
    }
    let desc = to_descriptor(vm, arg(args, 2))?;
    if !define_from_descriptor(vm, o, key, &desc)? {
        let k = vm.key_display(key);
        return Err(vm.type_error(&format!("Cannot redefine property: {k}")));
    }
    Ok(target)
}

fn define_properties_from(vm: &mut Vm, o: Gc<JsObject>, props: Value) -> JsResult<()> {
    let p = vm.to_object(props)?;
    let mut descs = Vec::new();
    for (k, f) in vm.own_keys(p) {
        if !f.enumerable() {
            continue;
        }
        let d = vm.get(Value::object(p), k)?;
        descs.push((k, to_descriptor(vm, d)?));
    }
    for (k, d) in descs {
        if !define_from_descriptor(vm, o, k, &d)? {
            let k = vm.key_display(k);
            return Err(vm.type_error(&format!("Cannot redefine property: {k}")));
        }
    }
    Ok(())
}

fn define_properties(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let target = arg(args, 0);
    let Some(o) = target.as_object() else {
        return Err(vm.type_error("Object.defineProperties called on non-object"));
    };
    define_properties_from(vm, o, arg(args, 1))?;
    Ok(target)
}

pub(crate) fn from_descriptor(vm: &mut Vm, o: Gc<JsObject>, key: PropertyKey) -> JsResult<Value> {
    let (value, flags) = match vm.own_prop(o, key) {
        None => return Ok(Value::UNDEFINED),
        Some(Own::Slot(slot, flags)) => (o.get().read(slot), flags),
        Some(Own::Virtual(v, flags)) => (v, flags),
    };
    let d = vm.new_object();
    if flags.is_accessor() {
        let (g, s) = vm.accessor_pair(value);
        vm.define_value(d, PropertyKey::Atom(atoms::get), g, PropFlags::DEFAULT);
        vm.define_value(d, PropertyKey::Atom(atoms::set), s, PropFlags::DEFAULT);
    } else {
        vm.define_value(d, PropertyKey::Atom(atoms::value), value, PropFlags::DEFAULT);
        vm.define_value(d, PropertyKey::Atom(atoms::writable), Value::bool(flags.writable()), PropFlags::DEFAULT);
    }
    vm.define_value(d, PropertyKey::Atom(atoms::enumerable), Value::bool(flags.enumerable()), PropFlags::DEFAULT);
    vm.define_value(d, PropertyKey::Atom(atoms::configurable), Value::bool(flags.configurable()), PropFlags::DEFAULT);
    Ok(Value::object(d))
}

fn get_own_property_descriptor(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(arg(args, 0))?;
    let key = vm.to_property_key(arg(args, 1))?;
    if super::proxy::is_proxy(o) {
        return vm.proxy_get_own_property(o, key);
    }
    from_descriptor(vm, o, key)
}

fn get_own_property_descriptors(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(arg(args, 0))?;
    let out = vm.new_object();
    for (k, _) in vm.own_keys(o) {
        let d = from_descriptor(vm, o, k)?;
        vm.define_value(out, k, d, PropFlags::DEFAULT);
    }
    Ok(Value::object(out))
}

fn get_own_property_names(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(arg(args, 0))?;
    let keys: Vec<PropertyKey> = vm.own_keys(o).into_iter().map(|(k, _)| k).filter(|k| !matches!(k, PropertyKey::Symbol(_))).collect();
    let values: Vec<Value> = keys.into_iter().map(|k| vm.key_value(k)).collect();
    Ok(Value::object(vm.new_array(values)))
}

fn get_own_property_symbols(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(arg(args, 0))?;
    let values: Vec<Value> = vm
        .own_keys(o)
        .into_iter()
        .filter_map(|(k, _)| match k {
            PropertyKey::Symbol(s) => Some(Value::symbol(s)),
            _ => None,
        })
        .collect();
    Ok(Value::object(vm.new_array(values)))
}

pub(crate) fn proto_of(vm: &mut Vm, v: Value) -> JsResult<Value> {
    let o = vm.to_object(v)?;
    if super::proxy::is_proxy(o) {
        return vm.proxy_get_prototype(o);
    }
    Ok(match o.get().proto {
        Some(p) => Value::object(p),
        None => Value::NULL,
    })
}

fn get_prototype_of(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    proto_of(vm, arg(args, 0))
}

/// [[SetPrototypeOf]]; false if not allowed
pub(crate) fn set_proto(vm: &mut Vm, o: Gc<JsObject>, p: Value) -> bool {
    let new = p.as_object();
    if o.get().proto == new {
        return true;
    }
    if !o.get().extensible {
        return false;
    }
    // No cycles
    let mut cur = new;
    while let Some(c) = cur {
        if c == o {
            return false;
        }
        cur = c.get().proto;
    }
    if let Some(n) = new {
        n.get_mut().is_prototype = true;
    }
    if o.get().is_prototype {
        vm.proto_epoch = vm.proto_epoch.wrapping_add(1);
    }
    o.get_mut().proto = new;
    true
}

fn set_prototype_of(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let target = arg(args, 0);
    let p = arg(args, 1);
    if !p.is_object() && !p.is_null() {
        return Err(vm.type_error("Object prototype may only be an Object or null"));
    }
    if target.is_nullish() {
        return Err(vm.type_error("Object.setPrototypeOf called on null or undefined"));
    }
    if let Some(o) = target.as_object() {
        if !vm.set_prototype(o, p)? {
            return Err(vm.type_error("Cyclic __proto__ value or non-extensible object"));
        }
    }
    Ok(target)
}

/// Freeze (or seal) an object: attributes on every own property, and no
/// new properties
pub(crate) fn set_integrity(vm: &mut Vm, o: Gc<JsObject>, frozen: bool) {
    vm.materialize(o);
    let keys = vm.own_keys(o);
    if o.get().is_prototype {
        vm.proto_epoch = vm.proto_epoch.wrapping_add(1);
    }
    for (k, f) in keys {
        let mut nf = f.with(PropFlags::CONFIGURABLE, false);
        if frozen && !f.is_accessor() {
            nf = nf.with(PropFlags::WRITABLE, false);
        }
        if nf != f {
            o.get_mut().set_flags(&vm.shapes, k, nf);
        }
    }
    let ob = o.get_mut();
    ob.extensible = false;
    if frozen && ob.is_array() {
        ob.length_readonly = true;
    }
    // Inline caches never match dictionary objects, so cached stores
    // can't bypass the new attributes
    ob.to_dictionary(&vm.shapes);
}

fn freeze(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    integrity(vm, arg(args, 0), true)
}

fn seal(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    integrity(vm, arg(args, 0), false)
}

fn integrity(vm: &mut Vm, v: Value, frozen: bool) -> JsResult<Value> {
    if let Some(o) = v.as_object() {
        if super::proxy::is_proxy(o) {
            if !proxy_set_integrity(vm, o, frozen)? {
                return Err(vm.type_error("Cannot freeze or seal the proxy"));
            }
        } else {
            set_integrity(vm, o, frozen);
        }
    }
    Ok(v)
}

/// SetIntegrityLevel through a proxy's traps
fn proxy_set_integrity(vm: &mut Vm, o: Gc<JsObject>, frozen: bool) -> JsResult<bool> {
    if !vm.prevent_extensions(o)? {
        return Ok(false);
    }
    for key in vm.proxy_own_keys(o)? {
        let desc = vm.new_object();
        vm.define_value(desc, PropertyKey::Atom(atoms::configurable), Value::FALSE, PropFlags::DEFAULT);
        if frozen {
            let current = vm.proxy_get_own_property(o, key)?;
            if is_data_descriptor(vm, current)? {
                vm.define_value(desc, PropertyKey::Atom(atoms::writable), Value::FALSE, PropFlags::DEFAULT);
            }
        }
        if !vm.proxy_define(o, key, Value::object(desc))? {
            return Err(vm.type_error("'defineProperty' on proxy: trap returned falsish"));
        }
    }
    Ok(true)
}

fn is_data_descriptor(vm: &mut Vm, d: Value) -> JsResult<bool> {
    let Some(d) = d.as_object() else { return Ok(false) };
    Ok(vm.has_property(d, PropertyKey::Atom(atoms::value)) || vm.has_property(d, PropertyKey::Atom(atoms::writable)))
}

fn test_integrity(vm: &mut Vm, v: Value, frozen: bool) -> JsResult<bool> {
    let Some(o) = v.as_object() else { return Ok(true) };
    if super::proxy::is_proxy(o) {
        // TestIntegrityLevel through the traps
        if vm.is_extensible(o)? {
            return Ok(false);
        }
        for key in vm.proxy_own_keys(o)? {
            let d = vm.proxy_get_own_property(o, key)?;
            let Some(obj) = d.as_object() else { continue };
            if crate::vm::ops::truthy(vm.get(Value::object(obj), PropertyKey::Atom(atoms::configurable))?) {
                return Ok(false);
            }
            if frozen && is_data_descriptor(vm, d)? && crate::vm::ops::truthy(vm.get(Value::object(obj), PropertyKey::Atom(atoms::writable))?) {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    if o.get().extensible {
        return Ok(false);
    }
    Ok(vm.own_keys(o).into_iter().all(|(_, f)| !f.configurable() && (!frozen || f.is_accessor() || !f.writable())))
}

fn is_frozen(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(test_integrity(vm, arg(args, 0), true)?))
}

fn is_sealed(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(test_integrity(vm, arg(args, 0), false)?))
}

fn prevent_extensions(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let v = arg(args, 0);
    if let Some(o) = v.as_object() {
        if !vm.prevent_extensions(o)? {
            return Err(vm.type_error("Cannot prevent extensions"));
        }
    }
    Ok(v)
}

fn is_extensible(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    match arg(args, 0).as_object() {
        Some(o) => Ok(Value::bool(vm.is_extensible(o)?)),
        None => Ok(Value::FALSE),
    }
}

fn from_entries(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.new_object();
    let it = vm.get_iterator(arg(args, 0))?;
    while let Some(entry) = vm.iter_step(it)? {
        let k = vm.get(entry, PropertyKey::Index(0))?;
        let v = vm.get(entry, PropertyKey::Index(1))?;
        let key = vm.to_property_key(k)?;
        vm.define_value(o, key, v, PropFlags::DEFAULT);
    }
    Ok(Value::object(o))
}

fn is(_vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(crate::vm::ops::same_value(arg(args, 0), arg(args, 1))))
}

fn has_own(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = vm.to_object(arg(args, 0))?;
    let key = vm.to_property_key(arg(args, 1))?;
    Ok(Value::bool(vm.has_own_property(o, key)))
}

fn has_own_property(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let key = vm.to_property_key(arg(args, 0))?;
    let o = vm.to_object(this)?;
    if super::proxy::is_proxy(o) {
        return Ok(Value::bool(!vm.proxy_get_own_property(o, key)?.is_undefined()));
    }
    Ok(Value::bool(vm.has_own_property(o, key)))
}

fn is_prototype_of(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(v) = arg(args, 0).as_object() else { return Ok(Value::FALSE) };
    let o = vm.to_object(this)?;
    let mut cur = vm.prototype_of(v)?;
    while let Some(c) = cur {
        if c == o {
            return Ok(Value::TRUE);
        }
        cur = vm.prototype_of(c)?;
    }
    Ok(Value::FALSE)
}

fn property_is_enumerable(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let key = vm.to_property_key(arg(args, 0))?;
    let o = vm.to_object(this)?;
    Ok(Value::bool(match vm.own_prop(o, key) {
        Some(Own::Slot(_, f)) | Some(Own::Virtual(_, f)) => f.enumerable(),
        None => false,
    }))
}

pub(crate) fn to_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if this.is_undefined() {
        return Ok(vm.str_value("[object Undefined]"));
    }
    if this.is_null() {
        return Ok(vm.str_value("[object Null]"));
    }
    let o = vm.to_object(this)?;
    let builtin = match &o.get().kind {
        ObjectKind::Array { .. } => "Array",
        ObjectKind::Proxy(_) if super::array::is_array_value(Value::object(o)) => "Array",
        ObjectKind::Proxy(p) if p.callable => "Function",
        ObjectKind::Function(_) | ObjectKind::Native(_) | ObjectKind::Bound(_) => "Function",
        ObjectKind::Error(_) => "Error",
        ObjectKind::Boolean(_) => "Boolean",
        ObjectKind::Number(_) => "Number",
        ObjectKind::String(_) => "String",
        ObjectKind::Date(_) => "Date",
        ObjectKind::Arguments => "Arguments",
        ObjectKind::RegExp(_) => "RegExp",
        _ => "Object",
    };
    let tag = vm.get(Value::object(o), PropertyKey::Symbol(vm.sym.to_string_tag))?;
    let tag = match tag.as_string() {
        Some(s) => s.get().to_rust_string(),
        None => builtin.to_string(),
    };
    Ok(vm.str_value(&format!("[object {tag}]")))
}

fn to_locale_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let f = vm.get(this, PropertyKey::Atom(atoms::toString))?;
    vm.call(f, this, &[])
}

fn value_of(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::object(vm.to_object(this)?))
}

fn proto_getter(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    proto_of(vm, this)
}

fn proto_setter(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let p = arg(args, 0);
    if let Some(o) = this.as_object() {
        if (p.is_object() || p.is_null()) && !set_proto(vm, o, p) {
            return Err(vm.type_error("Cyclic __proto__ value"));
        }
    }
    Ok(Value::UNDEFINED)
}
