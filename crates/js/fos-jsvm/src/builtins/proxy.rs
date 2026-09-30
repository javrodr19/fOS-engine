//! Proxy
//!
//! Proxy objects have no properties of their own; the property algorithms
//! in `vm::property` divert to the trap helpers here when they meet one.
//! A proxy is stored in dictionary mode with no dictionary, so inline
//! caches can never match it.

use crate::gc::Gc;
use crate::object::*;
use crate::shape::ShapeId;
use crate::string::atoms;
use crate::value::Value;
use crate::vm::{JsResult, Vm};

use super::arg;
use super::object::{Descriptor, from_descriptor, to_descriptor};

pub(super) fn init(vm: &mut Vm) {
    let c = vm.new_native("Proxy", 2, requires_new, Some(construct));
    vm.def_method(c, "revocable", 2, revocable);
    let g = vm.global;
    vm.def_value(g, "Proxy", Value::object(c), PropFlags::HIDDEN);
}

fn requires_new(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("Constructor Proxy requires 'new'"))
}

fn create(vm: &mut Vm, target: Value, handler: Value) -> JsResult<Gc<JsObject>> {
    let (Some(t), true) = (target.as_object(), handler.is_object()) else {
        return Err(vm.type_error("Cannot create proxy with a non-object as target or handler"));
    };
    let callable = t.get().is_callable();
    let constructor = vm.is_constructor(target);
    let p = vm.new_object_with(None, ObjectKind::Proxy(Box::new(ProxyData { target, handler, callable, constructor })));
    p.get_mut().shape = ShapeId::DICT;
    Ok(p)
}

fn construct(vm: &mut Vm, _nt: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::object(create(vm, arg(args, 0), arg(args, 1))?))
}

fn revocable(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let p = create(vm, arg(args, 0), arg(args, 1))?;
    let revoke = vm.new_native("", 0, revoke, None);
    if let ObjectKind::Native(n) = &mut revoke.get_mut().kind {
        n.data = Value::object(p);
    }
    let o = vm.new_object();
    let k = vm.key_from_str("proxy");
    vm.define_value(o, k, Value::object(p), PropFlags::DEFAULT);
    let k = vm.key_from_str("revoke");
    vm.define_value(o, k, Value::object(revoke), PropFlags::DEFAULT);
    Ok(Value::object(o))
}

fn revoke(_vm: &mut Vm, _this: Value, _args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    if let ObjectKind::Native(n) = &callee.get().kind {
        if let Some(p) = n.data.as_object() {
            if let ObjectKind::Proxy(d) = &mut p.get_mut().kind {
                d.target = Value::NULL;
                d.handler = Value::NULL;
            }
        }
    }
    Ok(Value::UNDEFINED)
}

pub(crate) fn is_proxy(o: Gc<JsObject>) -> bool {
    matches!(o.get().kind, ObjectKind::Proxy(_))
}

impl Vm {
    /// Target and the trap `name` (None: no trap, use the target)
    fn trap(&mut self, p: Gc<JsObject>, name: &str) -> JsResult<(Value, Option<Value>, Value)> {
        let (target, handler) = match &p.get().kind {
            ObjectKind::Proxy(d) => (d.target, d.handler),
            _ => unreachable!(),
        };
        if handler.is_null() {
            return Err(self.type_error(&format!("Cannot perform '{name}' on a proxy that has been revoked")));
        }
        let t = self.get_str(handler, name)?;
        if t.is_nullish() {
            return Ok((target, None, handler));
        }
        if !self.is_callable(t) {
            return Err(self.type_error(&format!("proxy trap '{name}' is not a function")));
        }
        Ok((target, Some(t), handler))
    }

    pub(crate) fn proxy_get(&mut self, p: Gc<JsObject>, key: PropertyKey, receiver: Value) -> JsResult<Value> {
        let (target, trap, handler) = self.trap(p, "get")?;
        match trap {
            Some(t) => {
                let k = self.key_value(key);
                self.call(t, handler, &[target, k, receiver])
            }
            None => self.get_from(target.as_object().unwrap(), key, receiver),
        }
    }

    pub(crate) fn proxy_set(&mut self, p: Gc<JsObject>, key: PropertyKey, v: Value, receiver: Value) -> JsResult<bool> {
        let (target, trap, handler) = self.trap(p, "set")?;
        match trap {
            Some(t) => {
                let k = self.key_value(key);
                let r = self.call(t, handler, &[target, k, v, receiver])?;
                Ok(crate::vm::ops::truthy(r))
            }
            None => {
                let receiver = if receiver == Value::object(p) { target } else { receiver };
                self.set_on(target.as_object().unwrap(), key, v, receiver)
            }
        }
    }

    pub(crate) fn proxy_has(&mut self, p: Gc<JsObject>, key: PropertyKey) -> JsResult<bool> {
        let (target, trap, handler) = self.trap(p, "has")?;
        match trap {
            Some(t) => {
                let k = self.key_value(key);
                let r = self.call(t, handler, &[target, k])?;
                Ok(crate::vm::ops::truthy(r))
            }
            None => self.has_property_js(target.as_object().unwrap(), key),
        }
    }

    pub(crate) fn proxy_delete(&mut self, p: Gc<JsObject>, key: PropertyKey) -> JsResult<bool> {
        let (target, trap, handler) = self.trap(p, "deleteProperty")?;
        match trap {
            Some(t) => {
                let k = self.key_value(key);
                let r = self.call(t, handler, &[target, k])?;
                Ok(crate::vm::ops::truthy(r))
            }
            None => self.delete_property(target, key, false),
        }
    }

    pub(crate) fn proxy_own_keys(&mut self, p: Gc<JsObject>) -> JsResult<Vec<PropertyKey>> {
        let (target, trap, handler) = self.trap(p, "ownKeys")?;
        match trap {
            Some(t) => {
                let r = self.call(t, handler, &[target])?;
                let list = super::function::list_from_array_like(self, r)?;
                let mut keys = Vec::with_capacity(list.len());
                for v in list {
                    if !v.is_string() && !v.is_symbol() {
                        return Err(self.type_error("proxy ownKeys trap result must contain only strings and symbols"));
                    }
                    keys.push(self.to_property_key(v)?);
                }
                Ok(keys)
            }
            None => {
                let t = target.as_object().unwrap();
                Ok(self.own_keys_js(t)?.into_iter().map(|(k, _)| k).collect())
            }
        }
    }

    /// The own property descriptor as a descriptor object (or undefined)
    pub(crate) fn proxy_get_own_property(&mut self, p: Gc<JsObject>, key: PropertyKey) -> JsResult<Value> {
        let (target, trap, handler) = self.trap(p, "getOwnPropertyDescriptor")?;
        match trap {
            Some(t) => {
                let k = self.key_value(key);
                self.call(t, handler, &[target, k])
            }
            None => {
                let t = target.as_object().unwrap();
                if is_proxy(t) {
                    return self.proxy_get_own_property(t, key);
                }
                from_descriptor(self, t, key)
            }
        }
    }

    pub(crate) fn proxy_define(&mut self, p: Gc<JsObject>, key: PropertyKey, desc: Value) -> JsResult<bool> {
        let (target, trap, handler) = self.trap(p, "defineProperty")?;
        match trap {
            Some(t) => {
                let k = self.key_value(key);
                let r = self.call(t, handler, &[target, k, desc])?;
                Ok(crate::vm::ops::truthy(r))
            }
            None => {
                let d: Descriptor = to_descriptor(self, desc)?;
                super::object::define_from_descriptor(self, target.as_object().unwrap(), key, &d)
            }
        }
    }

    pub(crate) fn proxy_get_prototype(&mut self, p: Gc<JsObject>) -> JsResult<Value> {
        let (target, trap, handler) = self.trap(p, "getPrototypeOf")?;
        match trap {
            Some(t) => self.call(t, handler, &[target]),
            None => super::object::proto_of(self, target),
        }
    }

    pub(crate) fn proxy_call(&mut self, p: Gc<JsObject>, this: Value, args: &[Value]) -> JsResult<Value> {
        let (target, trap, handler) = self.trap(p, "apply")?;
        match trap {
            Some(t) => {
                let arr = self.new_array(args.to_vec());
                self.call(t, handler, &[target, this, Value::object(arr)])
            }
            None => self.call(target, this, args),
        }
    }

    pub(crate) fn proxy_construct(&mut self, p: Gc<JsObject>, args: &[Value], new_target: Value) -> JsResult<Value> {
        let (target, trap, handler) = self.trap(p, "construct")?;
        let nt = if new_target == Value::object(p) { target } else { new_target };
        match trap {
            Some(t) => {
                let arr = self.new_array(args.to_vec());
                let r = self.call(t, handler, &[target, Value::object(arr), nt])?;
                if !r.is_object() {
                    return Err(self.type_error("proxy [[Construct]] must return an object"));
                }
                Ok(r)
            }
            None => self.construct(target, args, nt),
        }
    }

    /// Own keys with attributes, consulting proxy traps (errors propagate)
    pub(crate) fn own_keys_js(&mut self, o: Gc<JsObject>) -> JsResult<Vec<(PropertyKey, PropFlags)>> {
        if !is_proxy(o) {
            return Ok(self.own_keys(o));
        }
        let keys = self.proxy_own_keys(o)?;
        let mut out = Vec::with_capacity(keys.len());
        for k in keys {
            let d = self.proxy_get_own_property(o, k)?;
            if d.is_undefined() {
                continue;
            }
            let e = self.get(d, PropertyKey::Atom(atoms::enumerable))?;
            let c = self.get(d, PropertyKey::Atom(atoms::configurable))?;
            let w = self.get(d, PropertyKey::Atom(atoms::writable))?;
            let flags = PropFlags::NONE
                .with(PropFlags::ENUMERABLE, crate::vm::ops::truthy(e))
                .with(PropFlags::CONFIGURABLE, crate::vm::ops::truthy(c))
                .with(PropFlags::WRITABLE, crate::vm::ops::truthy(w));
            out.push((k, flags));
        }
        Ok(out)
    }

    /// HasProperty, consulting proxy traps
    pub(crate) fn has_property_js(&mut self, o: Gc<JsObject>, key: PropertyKey) -> JsResult<bool> {
        let mut cur = o;
        loop {
            if is_proxy(cur) {
                return self.proxy_has(cur, key);
            }
            if self.own_prop(cur, key).is_some() {
                return Ok(true);
            }
            match cur.get().proto {
                Some(p) => cur = p,
                None => return Ok(false),
            }
        }
    }
}
