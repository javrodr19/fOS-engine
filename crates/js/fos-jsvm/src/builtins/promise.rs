//! Promise

use crate::gc::Gc;
use crate::object::*;
use crate::string::atoms;
use crate::value::Value;
use crate::vm::{JsResult, NativeFn, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.promise_proto;
    let c = vm.def_ctor("Promise", 1, requires_new, Some(construct), proto);
    let statics: &[(&str, u32, NativeFn)] = &[
        ("resolve", 1, resolve),
        ("reject", 1, reject),
        ("all", 1, all),
        ("allSettled", 1, all_settled),
        ("race", 1, race),
        ("any", 1, any),
        ("withResolvers", 0, with_resolvers),
    ];
    for &(name, len, f) in statics {
        vm.def_method(c, name, len, f);
    }
    vm.def_method(proto, "then", 2, then);
    vm.def_method(proto, "catch", 1, catch);
    vm.def_method(proto, "finally", 1, finally);
    let tag = vm.sym.to_string_tag;
    let s = vm.str_value("Promise");
    vm.define_value(proto, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));
}

fn requires_new(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("Promise constructor cannot be invoked without 'new'"))
}

fn construct(vm: &mut Vm, new_target: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let executor = arg(args, 0);
    if !vm.is_callable(executor) {
        return Err(vm.type_error("Promise resolver is not a function"));
    }
    let p = vm.new_promise();
    let proto = vm.prototype_for(new_target, |r| r.promise_proto)?;
    p.get_mut().proto = Some(proto);
    let (res, rej) = vm.resolving_functions(p);
    if let Err(e) = vm.call(executor, Value::UNDEFINED, &[res, rej]) {
        vm.call(rej, Value::UNDEFINED, &[e])?;
    }
    Ok(Value::object(p))
}

/// The promise and "already resolved" flag of a resolving function
fn record(callee: Gc<JsObject>) -> Option<(Gc<JsObject>, Gc<JsObject>)> {
    let ObjectKind::Native(n) = &callee.get().kind else { return None };
    let rec = n.data.as_object()?;
    let p = rec.get().elements.first()?.as_object()?;
    Some((rec, p))
}

pub(crate) fn resolve_fn(vm: &mut Vm, _this: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    if let Some((rec, p)) = record(callee) {
        if rec.get().elements[1] == Value::FALSE {
            rec.get_mut().elements[1] = Value::TRUE;
            vm.resolve_promise(p, arg(args, 0));
        }
    }
    Ok(Value::UNDEFINED)
}

pub(crate) fn reject_fn(vm: &mut Vm, _this: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    if let Some((rec, p)) = record(callee) {
        if rec.get().elements[1] == Value::FALSE {
            rec.get_mut().elements[1] = Value::TRUE;
            vm.reject_promise(p, arg(args, 0));
        }
    }
    Ok(Value::UNDEFINED)
}

fn this_promise(vm: &mut Vm, this: Value) -> JsResult<Gc<JsObject>> {
    match this.as_object() {
        Some(o) if matches!(o.get().kind, ObjectKind::Promise(_)) => Ok(o),
        _ => Err(vm.type_error("Method Promise.prototype.then called on incompatible receiver")),
    }
}

/// `p.then(f, r)` returning the derived promise
pub(crate) fn perform_then(vm: &mut Vm, p: Gc<JsObject>, on_fulfilled: Value, on_rejected: Value) -> Gc<JsObject> {
    let derived = vm.new_promise();
    let f = if vm.is_callable(on_fulfilled) { on_fulfilled } else { Value::UNDEFINED };
    let r = if vm.is_callable(on_rejected) { on_rejected } else { Value::UNDEFINED };
    vm.add_reaction(p, Reaction { kind: ReactionKind::Then, on_fulfilled: f, on_rejected: r, derived: Some(derived) });
    derived
}

fn then(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let p = this_promise(vm, this)?;
    Ok(Value::object(perform_then(vm, p, arg(args, 0), arg(args, 1))))
}

fn invoke_then(vm: &mut Vm, this: Value, f: Value, r: Value) -> JsResult<Value> {
    let then = vm.get(this, PropertyKey::Atom(atoms::then))?;
    vm.call(then, this, &[f, r])
}

fn catch(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    invoke_then(vm, this, Value::UNDEFINED, arg(args, 0))
}

/// Native closure state: [callback, kind] in `data`
fn finally_reaction(vm: &mut Vm, _this: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    let ObjectKind::Native(n) = &callee.get().kind else { unreachable!() };
    let data = n.data.as_object().unwrap();
    let (on_finally, rejected) = (data.get().elements[0], data.get().elements[1] == Value::TRUE);
    let v = arg(args, 0);
    let r = vm.call(on_finally, Value::UNDEFINED, &[])?;
    let p = vm.promise_resolve(r);
    // Pass the original outcome through once the callback's promise settles
    let pass = vm.new_native("", 0, finally_pass, None);
    let d = vm.new_object_with(None, ObjectKind::Ordinary);
    d.get_mut().elements = vec![v, Value::bool(rejected)];
    if let ObjectKind::Native(n) = &mut pass.get_mut().kind {
        n.data = Value::object(d);
    }
    Ok(Value::object(perform_then(vm, p, Value::object(pass), Value::UNDEFINED)))
}

fn finally_pass(_vm: &mut Vm, _this: Value, _args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    let ObjectKind::Native(n) = &callee.get().kind else { unreachable!() };
    let d = n.data.as_object().unwrap();
    let (v, rejected) = (d.get().elements[0], d.get().elements[1] == Value::TRUE);
    if rejected { Err(v) } else { Ok(v) }
}

fn finally(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let on_finally = arg(args, 0);
    if !vm.is_callable(on_finally) {
        return invoke_then(vm, this, on_finally, on_finally);
    }
    let mut fns = [Value::UNDEFINED; 2];
    for (i, rejected) in [false, true].into_iter().enumerate() {
        let f = vm.new_native("", 1, finally_reaction, None);
        let d = vm.new_object_with(None, ObjectKind::Ordinary);
        d.get_mut().elements = vec![on_finally, Value::bool(rejected)];
        if let ObjectKind::Native(n) = &mut f.get_mut().kind {
            n.data = Value::object(d);
        }
        fns[i] = Value::object(f);
    }
    invoke_then(vm, this, fns[0], fns[1])
}

fn resolve(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::object(vm.promise_resolve(arg(args, 0))))
}

fn reject(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let p = vm.new_promise();
    vm.reject_promise(p, arg(args, 0));
    Ok(Value::object(p))
}

fn with_resolvers(vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let p = vm.new_promise();
    let (res, rej) = vm.resolving_functions(p);
    let o = vm.new_object();
    let k = vm.key_from_str("promise");
    vm.define_value(o, k, Value::object(p), PropFlags::DEFAULT);
    let k = vm.key_from_str("resolve");
    vm.define_value(o, k, res, PropFlags::DEFAULT);
    let k = vm.key_from_str("reject");
    vm.define_value(o, k, rej, PropFlags::DEFAULT);
    Ok(Value::object(o))
}

/// Combinator state shared by per-element callbacks: elements are
/// [result promise, values array, remaining count, mode]
const MODE_ALL: i32 = 0;
const MODE_ALL_SETTLED_OK: i32 = 1;
const MODE_ALL_SETTLED_ERR: i32 = 2;
const MODE_ANY: i32 = 3;

fn element_fn(vm: &mut Vm, _this: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    let ObjectKind::Native(n) = &callee.get().kind else { unreachable!() };
    let d = n.data.as_object().unwrap();
    // [state object, index, mode, called]
    let el = d.get().elements.clone();
    if el[3] == Value::TRUE {
        return Ok(Value::UNDEFINED);
    }
    d.get_mut().elements[3] = Value::TRUE;
    let state = el[0].as_object().unwrap();
    let index = el[1].as_int().unwrap() as usize;
    let mode = el[2].as_int().unwrap();
    let v = arg(args, 0);
    let stored = match mode {
        MODE_ALL_SETTLED_OK | MODE_ALL_SETTLED_ERR => {
            let o = vm.new_object();
            let (status, key) = if mode == MODE_ALL_SETTLED_OK { ("fulfilled", "value") } else { ("rejected", "reason") };
            let s = vm.str_value(status);
            let k = vm.key_from_str("status");
            vm.define_value(o, k, s, PropFlags::DEFAULT);
            let k = vm.key_from_str(key);
            vm.define_value(o, k, v, PropFlags::DEFAULT);
            Value::object(o)
        }
        _ => v,
    };
    let st = state.get().elements.clone();
    let values = st[1].as_object().unwrap();
    values.get_mut().elements[index] = stored;
    let remaining = st[2].as_int().unwrap() - 1;
    state.get_mut().elements[2] = Value::int(remaining);
    if remaining == 0 {
        let result = st[0].as_object().unwrap();
        if mode == MODE_ANY {
            let e = vm.make_error(crate::vm::ErrorKind::Error, "All promises were rejected");
            let k = vm.key_from_str("errors");
            vm.define_value(e.as_object().unwrap(), k, Value::object(values), PropFlags::HIDDEN);
            let k = vm.key_from_str("name");
            let n = vm.str_value("AggregateError");
            vm.define_value(e.as_object().unwrap(), k, n, PropFlags::HIDDEN);
            vm.reject_promise(result, e);
        } else {
            vm.resolve_promise(result, Value::object(values));
        }
    }
    Ok(Value::UNDEFINED)
}

fn combinator(vm: &mut Vm, iterable: Value, mode: i32) -> JsResult<Value> {
    let result = vm.new_promise();
    let (res_fn, rej_fn) = vm.resolving_functions(result);
    let values = vm.new_array(Vec::new());
    let state = vm.new_object_with(None, ObjectKind::Ordinary);
    state.get_mut().elements = vec![Value::object(result), Value::object(values), Value::int(1), Value::int(mode)];
    let run = (|| -> JsResult<()> {
        let it = vm.get_iterator(iterable)?;
        let mut index = 0;
        while let Some(v) = vm.iter_step(it)? {
            {
                let vals = values.get_mut();
                vals.elements.push(Value::UNDEFINED);
                if let ObjectKind::Array { length } = &mut vals.kind {
                    *length += 1;
                }
            }
            state.get_mut().elements[2] = Value::int(state.get().elements[2].as_int().unwrap() + 1);
            let p = vm.promise_resolve(v);
            let make = |vm: &mut Vm, m: i32| {
                let f = vm.new_native("", 1, element_fn, None);
                let d = vm.new_object_with(None, ObjectKind::Ordinary);
                d.get_mut().elements = vec![Value::object(state), Value::int(index), Value::int(m), Value::FALSE];
                if let ObjectKind::Native(n) = &mut f.get_mut().kind {
                    n.data = Value::object(d);
                }
                Value::object(f)
            };
            let (on_f, on_r) = match mode {
                MODE_ALL => (make(vm, MODE_ALL), rej_fn),
                MODE_ANY => (res_fn, make(vm, MODE_ANY)),
                _ => (make(vm, MODE_ALL_SETTLED_OK), make(vm, MODE_ALL_SETTLED_ERR)),
            };
            invoke_then(vm, Value::object(p), on_f, on_r)?;
            index += 1;
        }
        Ok(())
    })();
    if let Err(e) = run {
        vm.reject_promise(result, e);
        return Ok(Value::object(result));
    }
    // Drop the initial count of 1
    let remaining = state.get().elements[2].as_int().unwrap() - 1;
    state.get_mut().elements[2] = Value::int(remaining);
    if remaining == 0 {
        if mode == MODE_ANY {
            let e = vm.make_error(crate::vm::ErrorKind::Error, "All promises were rejected");
            vm.reject_promise(result, e);
        } else {
            vm.resolve_promise(result, Value::object(values));
        }
    }
    Ok(Value::object(result))
}

fn all(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    combinator(vm, arg(args, 0), MODE_ALL)
}

fn all_settled(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    combinator(vm, arg(args, 0), MODE_ALL_SETTLED_OK)
}

fn any(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    combinator(vm, arg(args, 0), MODE_ANY)
}

fn race(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let result = vm.new_promise();
    let (res, rej) = vm.resolving_functions(result);
    let it = match vm.get_iterator(arg(args, 0)) {
        Ok(it) => it,
        Err(e) => {
            vm.reject_promise(result, e);
            return Ok(Value::object(result));
        }
    };
    loop {
        match vm.iter_step(it) {
            Ok(Some(v)) => {
                let p = vm.promise_resolve(v);
                invoke_then(vm, Value::object(p), res, rej)?;
            }
            Ok(None) => break,
            Err(e) => {
                vm.reject_promise(result, e);
                break;
            }
        }
    }
    Ok(Value::object(result))
}
