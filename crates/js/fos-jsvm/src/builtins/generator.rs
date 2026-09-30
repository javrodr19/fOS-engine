//! Generator objects (%GeneratorPrototype%)

use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::generator::ResumeMode;
use crate::vm::{JsResult, Vm};

use super::arg;
use super::array::iter_result;

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.generator_proto;
    let next_fn = vm.realm.generator_next;
    if let ObjectKind::Native(n) = &mut next_fn.get_mut().kind {
        n.call = next;
        n.name = vm.intern("next");
        n.length = 1;
    }
    next_fn.get_mut().lazy = LAZY_LENGTH | LAZY_NAME;
    vm.def_value(proto, "next", Value::object(next_fn), PropFlags::HIDDEN);
    vm.def_method(proto, "return", 1, gen_return);
    vm.def_method(proto, "throw", 1, gen_throw);
    let tag = vm.sym.to_string_tag;
    let s = vm.str_value("Generator");
    vm.define_value(proto, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));
}

fn this_gen(vm: &mut Vm, this: Value) -> JsResult<Gc<JsObject>> {
    match this.as_object() {
        Some(o) if matches!(&o.get().kind, ObjectKind::Generator(g) if g.promise.is_none()) => Ok(o),
        _ => Err(vm.type_error("next method called on incompatible receiver")),
    }
}

fn step(vm: &mut Vm, this: Value, mode: ResumeMode, v: Value) -> JsResult<Value> {
    let g = this_gen(vm, this)?;
    let (value, done) = vm.resume(g, mode, v)?;
    Ok(iter_result(vm, value, done))
}

fn next(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    step(vm, this, ResumeMode::Next, arg(args, 0))
}

fn gen_return(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    step(vm, this, ResumeMode::Return, arg(args, 0))
}

fn gen_throw(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    step(vm, this, ResumeMode::Throw, arg(args, 0))
}
