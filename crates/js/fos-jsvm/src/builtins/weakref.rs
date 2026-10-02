//! WeakRef and FinalizationRegistry
//!
//! Neither keeps its targets alive: the collector clears a WeakRef whose
//! target dies, and queues a registry's cleanup callback (as a job) with
//! the held value of each cell whose target died.

use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::{JsResult, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let tag = vm.sym.to_string_tag;

    let wr = vm.realm.weakref_proto;
    vm.def_ctor("WeakRef", 1, requires_new, Some(weakref_construct), wr);
    vm.def_method(wr, "deref", 0, weakref_deref);
    let s = vm.str_value("WeakRef");
    vm.define_value(wr, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));

    let fr = vm.realm.finalization_registry_proto;
    vm.def_ctor("FinalizationRegistry", 1, requires_new, Some(registry_construct), fr);
    vm.def_method(fr, "register", 2, registry_register);
    vm.def_method(fr, "unregister", 1, registry_unregister);
    let s = vm.str_value("FinalizationRegistry");
    vm.define_value(fr, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));
}

fn requires_new(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("Constructor requires 'new'"))
}

impl Vm {
    /// CanBeHeldWeakly: objects and symbols not in the global registry
    pub(crate) fn can_be_held_weakly(&self, v: Value) -> bool {
        v.is_object() || v.as_symbol().is_some_and(|s| !self.symbol_registry.values().any(|&r| r == s))
    }
}

fn weakref_construct(vm: &mut Vm, nt: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let target = arg(args, 0);
    if !vm.can_be_held_weakly(target) {
        return Err(vm.type_error("WeakRef: target must be an object or non-registered symbol"));
    }
    let proto = vm.prototype_for(nt, |r| r.weakref_proto)?;
    let o = vm.new_object_with(Some(proto), ObjectKind::WeakRef(target));
    vm.weak_refs.borrow_mut().push(o);
    // A new WeakRef's target survives the current job (AddToKeptObjects)
    vm.kept_alive.push(target);
    Ok(Value::object(o))
}

fn weakref_deref(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let target = match this.as_object().map(|o| &o.get_mut_detached().kind) {
        Some(ObjectKind::WeakRef(t)) => *t,
        _ => return Err(vm.type_error("WeakRef.prototype.deref called on incompatible receiver")),
    };
    if !target.is_undefined() {
        vm.kept_alive.push(target);
    }
    Ok(target)
}

fn registry_data<'a>(vm: &mut Vm, this: Value, method: &str) -> JsResult<&'a mut FinalizationData> {
    if let Some(o) = this.as_object() {
        if let ObjectKind::FinalizationRegistry(f) = &mut o.get_mut_detached().kind {
            return Ok(f);
        }
    }
    Err(vm.type_error(&format!("FinalizationRegistry.prototype.{method} called on incompatible receiver")))
}

fn registry_construct(vm: &mut Vm, nt: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let cleanup = arg(args, 0);
    if !vm.is_callable(cleanup) {
        return Err(vm.type_error("FinalizationRegistry: cleanup must be callable"));
    }
    let proto = vm.prototype_for(nt, |r| r.finalization_registry_proto)?;
    let o = vm.new_object_with(Some(proto), ObjectKind::FinalizationRegistry(Box::new(FinalizationData { cleanup, cells: Vec::new() })));
    vm.finalization_registries.borrow_mut().push(o);
    Ok(Value::object(o))
}

fn registry_register(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (target, held, token) = (arg(args, 0), arg(args, 1), arg(args, 2));
    registry_data(vm, this, "register")?;
    if !vm.can_be_held_weakly(target) {
        return Err(vm.type_error("FinalizationRegistry.prototype.register: invalid target"));
    }
    if crate::vm::ops::same_value(target, held) {
        return Err(vm.type_error("FinalizationRegistry.prototype.register: target and holdings must not be same"));
    }
    if !token.is_undefined() && !vm.can_be_held_weakly(token) {
        return Err(vm.type_error("FinalizationRegistry.prototype.register: invalid unregister token"));
    }
    registry_data(vm, this, "register")?.cells.push(FinalizationCell { target, held, token });
    Ok(Value::UNDEFINED)
}

fn registry_unregister(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let token = arg(args, 0);
    registry_data(vm, this, "unregister")?;
    if !vm.can_be_held_weakly(token) {
        let shown = vm.display(token);
        return Err(vm.type_error(&format!("Invalid unregisterToken ('{shown}')")));
    }
    let f = registry_data(vm, this, "unregister")?;
    let before = f.cells.len();
    f.cells.retain(|c| c.token.raw() != token.raw());
    Ok(Value::bool(f.cells.len() != before))
}
