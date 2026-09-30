//! Error and its subclasses

use crate::gc::Gc;
use crate::object::*;
use crate::string::atoms;
use crate::value::Value;
use crate::vm::{JsResult, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let base = vm.realm.error_proto;
    let error_ctor = vm.def_ctor("Error", 1, error_call, Some(error_construct), base);
    let empty = vm.atom_value(atoms::empty);
    vm.def_value(base, "message", empty, PropFlags::HIDDEN);
    let n = vm.str_value("Error");
    vm.def_value(base, "name", n, PropFlags::HIDDEN);
    vm.def_method(base, "toString", 0, to_string);
    let subclasses = [
        ("TypeError", vm.realm.type_error_proto),
        ("RangeError", vm.realm.range_error_proto),
        ("ReferenceError", vm.realm.reference_error_proto),
        ("SyntaxError", vm.realm.syntax_error_proto),
        ("EvalError", vm.realm.eval_error_proto),
        ("URIError", vm.realm.uri_error_proto),
    ];
    for (name, proto) in subclasses {
        let c = vm.def_ctor(name, 1, error_call, Some(error_construct), proto);
        c.get_mut().proto = Some(error_ctor);
        error_ctor.get_mut().is_prototype = true;
        let n = vm.str_value(name);
        vm.def_value(proto, "name", n, PropFlags::HIDDEN);
        vm.def_value(proto, "message", empty, PropFlags::HIDDEN);
    }
}

fn error_call(vm: &mut Vm, _this: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    error_construct(vm, Value::object(callee), args, callee)
}

fn error_construct(vm: &mut Vm, new_target: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    let nt = if new_target.is_undefined() { Value::object(callee) } else { new_target };
    let proto = vm.prototype_for(nt, |r| r.error_proto)?;
    let o = vm.new_object_with(Some(proto), ObjectKind::Error);
    let msg = arg(args, 0);
    if !msg.is_undefined() {
        let s = vm.to_string(msg)?;
        vm.define_value(o, PropertyKey::Atom(atoms::message), Value::string(s), PropFlags::HIDDEN);
    }
    let opts = arg(args, 1);
    if let Some(opts) = opts.as_object() {
        if vm.has_property(opts, PropertyKey::Atom(atoms::cause)) {
            let cause = vm.get(Value::object(opts), PropertyKey::Atom(atoms::cause))?;
            vm.define_value(o, PropertyKey::Atom(atoms::cause), cause, PropFlags::HIDDEN);
        }
    }
    // V8's format: the error's string form, then one line per frame
    let header = error_to_string(vm, Value::object(o)).unwrap_or_default();
    let stack = format!("{header}\n{}", vm.stack_trace());
    let stack = vm.str_value(&stack);
    vm.define_value(o, PropertyKey::Atom(atoms::stack), stack, PropFlags::HIDDEN);
    Ok(Value::object(o))
}

pub(crate) fn error_to_string(vm: &mut Vm, this: Value) -> JsResult<String> {
    if !this.is_object() {
        return Err(vm.type_error("Error.prototype.toString called on non-object"));
    }
    let name = vm.get(this, PropertyKey::Atom(atoms::name))?;
    let name = if name.is_undefined() { "Error".to_string() } else { vm.to_rust_string(name)? };
    let msg = vm.get(this, PropertyKey::Atom(atoms::message))?;
    let msg = if msg.is_undefined() { String::new() } else { vm.to_rust_string(msg)? };
    Ok(match (name.is_empty(), msg.is_empty()) {
        (true, _) => msg,
        (_, true) => name,
        _ => format!("{name}: {msg}"),
    })
}

fn to_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = error_to_string(vm, this)?;
    Ok(vm.str_value(&s))
}
