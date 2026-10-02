//! Function

use crate::gc::Gc;
use crate::object::*;
use crate::string::atoms;
use crate::value::Value;
use crate::vm::{JsResult, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.function_proto;
    vm.def_ctor("Function", 1, function_ctor, Some(function_construct), proto);
    vm.def_method(proto, "call", 1, call);
    vm.def_method(proto, "apply", 2, apply);
    vm.def_method(proto, "bind", 1, bind);
    vm.def_method(proto, "toString", 0, to_string);
    // Function.prototype[@@hasInstance]: OrdinaryHasInstance, fixed
    let f = vm.new_native("[Symbol.hasInstance]", 1, has_instance, None);
    let key = PropertyKey::Symbol(vm.sym.has_instance);
    vm.define_value(proto, key, Value::object(f), PropFlags::FROZEN);
    let tte = vm.realm.throw_type_error;
    if let ObjectKind::Native(n) = &mut tte.get_mut().kind {
        n.call = throw_type_error;
    }
}

/// `Function.prototype[Symbol.hasInstance](v)`
pub(crate) fn has_instance(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    match this.as_object().filter(|o| o.get().is_callable()) {
        Some(f) => Ok(Value::bool(vm.ordinary_has_instance(f, arg(args, 0))?)),
        None => Ok(Value::FALSE),
    }
}

fn throw_type_error(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("'caller', 'callee', and 'arguments' properties may not be accessed"))
}

fn function_ctor(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let mut params = Vec::new();
    for &a in args.iter().take(args.len().saturating_sub(1)) {
        params.push(vm.to_rust_string(a)?);
    }
    let body = match args.last() {
        Some(&b) => vm.to_rust_string(b)?,
        None => String::new(),
    };
    let params = params.join(",");
    let func = crate::parser::parse_function_parts(&params, &body).map_err(|e| vm.make_error(crate::vm::ErrorKind::Syntax, &e.message))?;
    let src = format!("(function anonymous({params}\n) {{\n{body}\n}})");
    let proto = crate::compiler::compile_function_object(&vm.heap, &mut vm.atoms, &src, &func)
        .map_err(|e| vm.make_error(crate::vm::ErrorKind::Syntax, &e.message))?;
    let f = vm.new_closure(proto, Box::new([]));
    Ok(Value::object(f))
}

fn function_construct(vm: &mut Vm, _nt: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    function_ctor(vm, Value::UNDEFINED, args, callee)
}

fn call(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if !vm.is_callable(this) {
        return Err(vm.type_error("Function.prototype.call called on a non-function"));
    }
    let (t, rest) = match args.split_first() {
        Some((t, rest)) => (*t, rest),
        None => (Value::UNDEFINED, &[][..]),
    };
    vm.call(this, t, rest)
}

/// The elements of an array-like, as a vector
pub(crate) fn list_from_array_like(vm: &mut Vm, v: Value) -> JsResult<Vec<Value>> {
    let Some(o) = v.as_object() else {
        return Err(vm.type_error("CreateListFromArrayLike called on non-object"));
    };
    if let ObjectKind::Array { length } = o.get().kind {
        if o.get().elements.len() == length as usize && !o.get().elements.iter().any(|e| e.is_hole()) {
            return Ok(o.get().elements.clone());
        }
    }
    let len = vm.get(v, PropertyKey::Atom(atoms::length))?;
    let len = vm.to_length(len)? as usize;
    if len > 10_000_000 {
        return Err(vm.range_error("Too many arguments"));
    }
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        out.push(vm.get(v, PropertyKey::Index(i as u32))?);
    }
    Ok(out)
}

fn apply(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if !vm.is_callable(this) {
        return Err(vm.type_error("Function.prototype.apply was called on a non-function"));
    }
    let list = arg(args, 1);
    let list = if list.is_nullish() { Vec::new() } else { list_from_array_like(vm, list)? };
    vm.call(this, arg(args, 0), &list)
}

fn bind(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(target) = this.as_object().filter(|o| o.get().is_callable()) else {
        return Err(vm.type_error("Bind must be called on a function"));
    };
    let bound_args: Box<[Value]> = args.iter().skip(1).copied().collect();
    let nargs = bound_args.len();
    let proto = target.get().proto;
    let b = vm.new_object_with(proto, ObjectKind::Bound(Box::new(BoundFunction { target, this: arg(args, 0), args: bound_args })));
    let len = vm.get(this, PropertyKey::Atom(atoms::length))?;
    let len = match len.as_number() {
        Some(n) if n.is_finite() => (n.trunc() - nargs as f64).max(0.0),
        Some(n) if n == f64::INFINITY => n,
        _ => 0.0,
    };
    vm.define_value(b, PropertyKey::Atom(atoms::length), Value::number(len), PropFlags::READONLY_HIDDEN);
    let name = vm.get(this, PropertyKey::Atom(atoms::name))?;
    let name = match name.as_string() {
        Some(s) => s.get().to_rust_string(),
        None => String::new(),
    };
    let name = vm.str_value(&format!("bound {name}"));
    vm.define_value(b, PropertyKey::Atom(atoms::name), name, PropFlags::READONLY_HIDDEN);
    Ok(Value::object(b))
}

fn to_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(o) = this.as_object().filter(|o| o.get().is_callable()) else {
        return Err(vm.type_error("Function.prototype.toString requires that 'this' be a Function"));
    };
    let name = vm.get(Value::object(o), PropertyKey::Atom(atoms::name))?;
    let name = match name.as_string() {
        Some(s) => s.get().to_rust_string(),
        None => String::new(),
    };
    let text = match &o.get().kind {
        ObjectKind::Function(c) if c.proto.is_class_constructor => format!("class {name} {{ }}"),
        _ => format!("function {name}() {{ [native code] }}"),
    };
    Ok(vm.str_value(&text))
}
