//! Reflect

use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::{JsResult, NativeFn, Vm};

use super::arg;
use super::object::{define_from_descriptor, from_descriptor, set_proto, to_descriptor};

pub(super) fn init(vm: &mut Vm) {
    let r = vm.new_object();
    let fns: &[(&str, u32, NativeFn)] = &[
        ("apply", 3, apply),
        ("construct", 2, construct),
        ("defineProperty", 3, define_property),
        ("deleteProperty", 2, delete_property),
        ("get", 2, get),
        ("getOwnPropertyDescriptor", 2, get_own_property_descriptor),
        ("getPrototypeOf", 1, get_prototype_of),
        ("has", 2, has),
        ("isExtensible", 1, is_extensible),
        ("ownKeys", 1, own_keys),
        ("preventExtensions", 1, prevent_extensions),
        ("set", 3, set),
        ("setPrototypeOf", 2, set_prototype_of),
    ];
    for &(name, len, f) in fns {
        vm.def_method(r, name, len, f);
    }
    let g = vm.global;
    vm.def_value(g, "Reflect", Value::object(r), PropFlags::HIDDEN);
}

fn target(vm: &mut Vm, args: &[Value]) -> JsResult<Gc<JsObject>> {
    match arg(args, 0).as_object() {
        Some(o) => Ok(o),
        None => Err(vm.type_error("Reflect method called on non-object")),
    }
}

fn apply(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let list = super::function::list_from_array_like(vm, arg(args, 2))?;
    vm.call(arg(args, 0), arg(args, 1), &list)
}

fn construct(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let f = arg(args, 0);
    if !vm.is_constructor(f) {
        return Err(vm.type_error("Reflect.construct target is not a constructor"));
    }
    let nt = if args.len() > 2 { args[2] } else { f };
    if !vm.is_constructor(nt) {
        return Err(vm.type_error("Reflect.construct newTarget is not a constructor"));
    }
    let list = super::function::list_from_array_like(vm, arg(args, 1))?;
    vm.construct(f, &list, nt)
}

fn define_property(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    let key = vm.to_property_key(arg(args, 1))?;
    let desc = to_descriptor(vm, arg(args, 2))?;
    Ok(Value::bool(define_from_descriptor(vm, o, key, &desc)?))
}

fn delete_property(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    let key = vm.to_property_key(arg(args, 1))?;
    Ok(Value::bool(vm.delete_property(Value::object(o), key, false)?))
}

fn get(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    let key = vm.to_property_key(arg(args, 1))?;
    let receiver = if args.len() > 2 { args[2] } else { Value::object(o) };
    vm.get_from(o, key, receiver)
}

fn get_own_property_descriptor(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    let key = vm.to_property_key(arg(args, 1))?;
    from_descriptor(vm, o, key)
}

fn get_prototype_of(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    super::object::proto_of(vm, Value::object(o))
}

fn has(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    let key = vm.to_property_key(arg(args, 1))?;
    Ok(Value::bool(vm.has_property_js(o, key)?))
}

fn is_extensible(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    Ok(Value::bool(o.get().extensible))
}

fn own_keys(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    let keys: Vec<PropertyKey> = vm.own_keys_js(o)?.into_iter().map(|(k, _)| k).collect();
    let values: Vec<Value> = keys.into_iter().map(|k| vm.key_value(k)).collect();
    Ok(Value::object(vm.new_array(values)))
}

fn prevent_extensions(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    o.get_mut().extensible = false;
    o.get_mut().to_dictionary(&vm.shapes);
    Ok(Value::TRUE)
}

fn set(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    let key = vm.to_property_key(arg(args, 1))?;
    let receiver = if args.len() > 3 { args[3] } else { Value::object(o) };
    Ok(Value::bool(vm.set_on(o, key, arg(args, 2), receiver)?))
}

fn set_prototype_of(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let o = target(vm, args)?;
    let p = arg(args, 1);
    if !p.is_object() && !p.is_null() {
        return Err(vm.type_error("Object prototype may only be an Object or null"));
    }
    Ok(Value::bool(set_proto(vm, o, p)))
}
