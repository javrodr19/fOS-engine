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
    // `stack` is formatted from the captured frames on first read (as in
    // SpiderMonkey, an accessor on the prototype): errors that are caught
    // and dropped never pay for it
    vm.def_accessor(base, "stack", stack_get, Some(stack_set));
    vm.def_method(error_ctor, "captureStackTrace", 2, capture_stack_trace);
    vm.def_value(error_ctor, "stackTraceLimit", Value::number(crate::vm::STACK_FRAMES as f64), PropFlags::DEFAULT);
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
    let captured = vm.capture_stack();
    let o = vm.new_object_with(Some(proto), ObjectKind::Error(captured));
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

/// The stack in V8's format: the error's string form, then one line per frame
fn format_stack(vm: &mut Vm, error: Value, frames: &[(std::rc::Rc<crate::bytecode::FunctionProto>, u32)]) -> String {
    let header = error_to_string(vm, error).unwrap_or_else(|_| "Error".into());
    header + &vm.format_frames(frames)
}

fn stack_get(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(o) = this.as_object() else { return Ok(Value::UNDEFINED) };
    let captured = match &mut o.get_mut().kind {
        ObjectKind::Error(c) => c.take(),
        _ => None,
    };
    let Some(captured) = captured else { return Ok(Value::UNDEFINED) };
    let stack = format_stack(vm, this, &captured.frames);
    let stack = vm.str_value(&stack);
    vm.define_value(o, PropertyKey::Atom(atoms::stack), stack, PropFlags::HIDDEN);
    Ok(stack)
}

fn stack_set(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if let Some(o) = this.as_object() {
        if let ObjectKind::Error(c) = &mut o.get_mut().kind {
            *c = None;
        }
        vm.define_value(o, PropertyKey::Atom(atoms::stack), arg(args, 0), PropFlags::HIDDEN);
    }
    Ok(Value::UNDEFINED)
}

/// `Error.captureStackTrace(object, constructorOpt)` (V8): gives `object`
/// a `stack` of the current frames, leaving out those above and including
/// the latest call to `constructorOpt`
fn capture_stack_trace(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(o) = arg(args, 0).as_object() else {
        return Err(vm.type_error("Invalid argument"));
    };
    let skip_to = arg(args, 1).as_object();
    let mut frames: Vec<_> = Vec::new();
    for f in vm.frames.iter().rev() {
        if let ObjectKind::Function(c) = &f.func.get().kind {
            frames.push((c.proto.clone(), f.pc, f.func));
        }
    }
    if let Some(stop) = skip_to {
        if let Some(i) = frames.iter().position(|f| f.2 == stop) {
            frames.drain(..=i);
        }
    }
    let frames: Vec<_> = frames.into_iter().take(crate::vm::STACK_FRAMES).map(|(p, pc, _)| (p, pc)).collect();
    let stack = format_stack(vm, Value::object(o), &frames);
    let stack = vm.str_value(&stack);
    vm.define_value(o, PropertyKey::Atom(atoms::stack), stack, PropFlags::HIDDEN);
    Ok(Value::UNDEFINED)
}
