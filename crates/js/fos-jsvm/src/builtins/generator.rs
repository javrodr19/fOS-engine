//! Generator objects (%GeneratorPrototype%), async generators
//! (%AsyncGeneratorPrototype%) and async iteration helpers

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

    // %AsyncIteratorPrototype%[Symbol.asyncIterator]() returns this
    let aip = vm.realm.async_iterator_proto;
    let sym = vm.sym.async_iterator;
    vm.def_method_sym(aip, sym, "[Symbol.asyncIterator]", 0, return_this);

    let agp = vm.realm.async_generator_proto;
    vm.def_method(agp, "next", 1, async_next);
    vm.def_method(agp, "return", 1, async_return);
    vm.def_method(agp, "throw", 1, async_throw);
    let s = vm.str_value("AsyncGenerator");
    vm.define_value(agp, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));

    let afs = vm.realm.async_from_sync_iterator_proto;
    vm.def_method(afs, "next", 1, from_sync_next);
    vm.def_method(afs, "return", 1, from_sync_return);
    vm.def_method(afs, "throw", 1, from_sync_throw);
}

fn return_this(_: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(this)
}

/// `v` -> `{ value: v, done: true }` (a promise reaction)
pub(crate) fn done_result(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(iter_result(vm, arg(args, 0), true))
}

/// `v` -> `{ value: v, done: false }` (a promise reaction)
fn value_result(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(iter_result(vm, arg(args, 0), false))
}

// ---- %AsyncGeneratorPrototype% ----

fn async_step(vm: &mut Vm, this: Value, mode: ResumeMode, v: Value) -> JsResult<Value> {
    match this.as_object() {
        Some(o) if matches!(&o.get().kind, ObjectKind::Generator(g) if g.queue.is_some()) => {
            Ok(Value::object(vm.async_gen_enqueue(o, mode, v)))
        }
        _ => {
            // Async methods report errors through their promise
            let p = vm.new_promise();
            let e = vm.type_error("AsyncGenerator method called on incompatible receiver");
            vm.reject_promise(p, e);
            Ok(Value::object(p))
        }
    }
}

fn async_next(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    async_step(vm, this, ResumeMode::Next, arg(args, 0))
}

fn async_return(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    async_step(vm, this, ResumeMode::Return, arg(args, 0))
}

fn async_throw(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    async_step(vm, this, ResumeMode::Throw, arg(args, 0))
}

// ---- %AsyncFromSyncIteratorPrototype% ----

/// The sync iterator (record) a from-sync wrapper holds
fn sync_of(this: Value) -> Option<Value> {
    let o = this.as_object()?;
    o.get().elements.first().copied()
}

/// A promise for `{ value: await value, done }`
fn continuation(vm: &mut Vm, value: Value, done: bool) -> Value {
    let p = vm.new_promise();
    let inner = vm.promise_resolve(value);
    let f = if done { done_result } else { value_result };
    let wrap = vm.new_native("", 1, f, None);
    vm.add_reaction(inner, Reaction { kind: ReactionKind::Then, on_fulfilled: Value::object(wrap), on_rejected: Value::UNDEFINED, derived: Some(p) });
    Value::object(p)
}

fn rejected(vm: &mut Vm, e: Value) -> Value {
    let p = vm.new_promise();
    vm.reject_promise(p, e);
    Value::object(p)
}

/// Settle a from-sync step from the sync iterator's result object
fn from_result(vm: &mut Vm, r: JsResult<Value>) -> Value {
    let step = r.and_then(|r| {
        let done = vm.get(r, PropertyKey::Atom(crate::string::atoms::done))?;
        let value = vm.get(r, PropertyKey::Atom(crate::string::atoms::value))?;
        Ok((value, crate::vm::truthy(done)))
    });
    match step {
        Ok((value, done)) => continuation(vm, value, done),
        Err(e) => rejected(vm, e),
    }
}

fn from_sync_next(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(sync) = sync_of(this) else { return Ok(rejected(vm, Value::UNDEFINED)) };
    let r = vm.iter_send(sync, arg(args, 0));
    Ok(from_result(vm, r))
}

/// The underlying iterator object's method `name`, if it has one
fn sync_method(vm: &mut Vm, sync: Value, name: crate::string::Atom) -> JsResult<Option<(Value, Value)>> {
    let Some(rec) = sync.as_object() else { return Ok(None) };
    let iter = match rec.get().kind {
        ObjectKind::IterRecord { iter, .. } => iter,
        // Built-in iterators (arrays, strings, maps) have no return/throw
        _ => return Ok(None),
    };
    let m = vm.get(iter, PropertyKey::Atom(name))?;
    Ok((!m.is_nullish()).then_some((m, iter)))
}

fn from_sync_return(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let v = arg(args, 0);
    let Some(sync) = sync_of(this) else { return Ok(rejected(vm, Value::UNDEFINED)) };
    match sync_method(vm, sync, crate::string::atoms::return_) {
        Ok(Some((m, iter))) => {
            let r = vm.call(m, iter, &[v]);
            Ok(from_result(vm, r))
        }
        Ok(None) => Ok(continuation(vm, v, true)),
        Err(e) => Ok(rejected(vm, e)),
    }
}

fn from_sync_throw(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let v = arg(args, 0);
    let Some(sync) = sync_of(this) else { return Ok(rejected(vm, Value::UNDEFINED)) };
    let throw = vm.intern("throw");
    match sync_method(vm, sync, throw) {
        Ok(Some((m, iter))) => {
            let r = vm.call(m, iter, &[v]);
            Ok(from_result(vm, r))
        }
        Ok(None) => Ok(rejected(vm, v)),
        Err(e) => Ok(rejected(vm, e)),
    }
}

fn this_gen(vm: &mut Vm, this: Value) -> JsResult<Gc<JsObject>> {
    match this.as_object() {
        Some(o) if matches!(&o.get().kind, ObjectKind::Generator(g) if g.promise.is_none() && g.queue.is_none()) => Ok(o),
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
