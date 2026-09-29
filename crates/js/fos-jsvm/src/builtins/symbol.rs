//! Symbol

use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::{JsResult, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.symbol_proto;
    let c = vm.def_ctor("Symbol", 0, symbol_call, None, proto);
    vm.def_method(c, "for", 1, symbol_for);
    vm.def_method(c, "keyFor", 1, key_for);
    let wk = [
        ("iterator", vm.sym.iterator),
        ("asyncIterator", vm.sym.async_iterator),
        ("hasInstance", vm.sym.has_instance),
        ("toPrimitive", vm.sym.to_primitive),
        ("toStringTag", vm.sym.to_string_tag),
        ("species", vm.sym.species),
        ("isConcatSpreadable", vm.sym.is_concat_spreadable),
        ("unscopables", vm.sym.unscopables),
        ("match", vm.sym.match_),
        ("matchAll", vm.sym.match_all),
        ("replace", vm.sym.replace),
        ("search", vm.sym.search),
        ("split", vm.sym.split),
    ];
    for (name, s) in wk {
        vm.def_value(c, name, Value::symbol(s), PropFlags::FROZEN);
    }
    vm.def_method(proto, "toString", 0, to_string);
    vm.def_method(proto, "valueOf", 0, value_of);
    vm.def_getter(proto, "description", description);
    let tp = vm.sym.to_primitive;
    vm.def_method_sym(proto, tp, "[Symbol.toPrimitive]", 1, value_of);
    let tag = vm.sym.to_string_tag;
    let s = vm.str_value("Symbol");
    vm.define_value(proto, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));
}

fn symbol_call(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let d = arg(args, 0);
    let desc = if d.is_undefined() { None } else { Some(vm.to_string(d)?) };
    Ok(Value::symbol(vm.new_symbol(desc)))
}

fn symbol_for(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_string(arg(args, 0))?;
    let atom = vm.atoms.intern(s);
    if let Some(&sym) = vm.symbol_registry.get(&atom) {
        return Ok(Value::symbol(sym));
    }
    let d = vm.atoms.string(atom);
    let sym = vm.new_symbol(Some(d));
    sym.get_mut().registered = true;
    vm.symbol_registry.insert(atom, sym);
    Ok(Value::symbol(sym))
}

fn key_for(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let Some(s) = arg(args, 0).as_symbol() else {
        return Err(vm.type_error("Symbol.keyFor requires a symbol"));
    };
    if s.get().registered {
        if let Some(d) = s.get().description {
            return Ok(Value::string(d));
        }
    }
    Ok(Value::UNDEFINED)
}

fn this_symbol(vm: &mut Vm, this: Value) -> JsResult<Gc<Symbol>> {
    if let Some(s) = this.as_symbol() {
        return Ok(s);
    }
    if let Some(o) = this.as_object() {
        if let ObjectKind::Symbol(s) = o.get().kind {
            return Ok(s);
        }
    }
    Err(vm.type_error("not a symbol"))
}

pub(crate) fn symbol_descriptive_string(s: Gc<Symbol>) -> String {
    match s.get().description {
        Some(d) => format!("Symbol({})", d.get().to_rust_string()),
        None => "Symbol()".into(),
    }
}

fn to_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_symbol(vm, this)?;
    let text = symbol_descriptive_string(s);
    Ok(vm.str_value(&text))
}

fn value_of(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::symbol(this_symbol(vm, this)?))
}

fn description(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = this_symbol(vm, this)?;
    Ok(s.get().description.map(Value::string).unwrap_or(Value::UNDEFINED))
}
