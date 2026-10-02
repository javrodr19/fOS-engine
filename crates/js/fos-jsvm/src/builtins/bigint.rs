//! BigInt

use crate::bigint::BigInt;
use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::{JsResult, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.bigint_proto;
    let c = vm.def_ctor("BigInt", 1, bigint_call, Some(not_a_constructor), proto);
    vm.def_method(c, "asIntN", 2, as_int_n);
    vm.def_method(c, "asUintN", 2, as_uint_n);
    vm.def_method(proto, "toString", 0, to_string);
    vm.def_method(proto, "toLocaleString", 0, to_locale_string);
    vm.def_method(proto, "valueOf", 0, value_of);
    let tag = vm.str_value("BigInt");
    vm.define_value(proto, PropertyKey::Symbol(vm.sym.to_string_tag), tag, PropFlags(PropFlags::CONFIGURABLE));
}

fn not_a_constructor(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("BigInt is not a constructor"))
}

/// `BigInt(value)`: numbers must be integers; strings are parsed
fn bigint_call(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let p = vm.to_primitive(arg(args, 0), crate::vm::ops::Hint::Number)?;
    if let Some(n) = p.as_number() {
        return match BigInt::from_f64(n) {
            Some(b) => Ok(vm.new_bigint(b)),
            None => {
                let shown = crate::number::number_to_string(n);
                Err(vm.range_error(&format!("The number {shown} cannot be converted to a BigInt because it is not an integer")))
            }
        };
    }
    to_bigint(vm, p)
}

/// ToBigInt: BigInts, booleans and strings (numbers are a TypeError)
pub(crate) fn to_bigint(vm: &mut Vm, v: Value) -> JsResult<Value> {
    let p = vm.to_primitive(v, crate::vm::ops::Hint::Number)?;
    if p.is_bigint() {
        return Ok(p);
    }
    if let Some(b) = p.as_bool() {
        return Ok(vm.new_bigint(BigInt::from_i64(b as i64)));
    }
    if let Some(s) = p.as_string() {
        let text = s.get().to_rust_string();
        return match BigInt::parse(&text) {
            Some(b) => Ok(vm.new_bigint(b)),
            None => Err(vm.make_error(crate::vm::ErrorKind::Syntax, &format!("Cannot convert {text} to a BigInt"))),
        };
    }
    let shown = vm.describe(p);
    Err(vm.type_error(&format!("Cannot convert {shown} to a BigInt")))
}

fn this_bigint(vm: &mut Vm, this: Value, method: &str) -> JsResult<Gc<BigInt>> {
    if let Some(b) = this.as_bigint() {
        return Ok(b);
    }
    if let Some(o) = this.as_object() {
        if let ObjectKind::BigInt(b) = o.get().kind {
            return Ok(b);
        }
    }
    Err(vm.type_error(&format!("BigInt.prototype.{method} requires that 'this' be a BigInt")))
}

fn to_string(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let b = this_bigint(vm, this, "toString")?;
    let radix = match arg(args, 0) {
        Value::UNDEFINED => 10.0,
        r => vm.to_integer(r)?,
    };
    if !(2.0..=36.0).contains(&radix) {
        return Err(vm.range_error("toString() radix must be between 2 and 36"));
    }
    let text = b.get().to_string_radix(radix as u32);
    Ok(vm.str_value(&text))
}

fn to_locale_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let b = this_bigint(vm, this, "toLocaleString")?;
    // en-US grouping
    let digits = b.get().to_string_radix(10);
    let (sign, digits) = digits.strip_prefix('-').map_or(("", digits.as_str()), |d| ("-", d));
    let mut out = String::from(sign);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    Ok(vm.str_value(&out))
}

fn value_of(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bigint(this_bigint(vm, this, "valueOf")?))
}

fn bits_arg(vm: &mut Vm, v: Value) -> JsResult<u64> {
    let n = vm.to_integer(v)?;
    if !(0.0..=9007199254740991.0).contains(&n) {
        return Err(vm.range_error("Invalid value: not (convertible to) a safe integer"));
    }
    Ok(n as u64)
}

fn as_int_n(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let bits = bits_arg(vm, arg(args, 0))?;
    let b = to_bigint(vm, arg(args, 1))?.as_bigint().unwrap();
    let r = b.get().as_int_n(bits);
    Ok(vm.new_bigint(r))
}

fn as_uint_n(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let bits = bits_arg(vm, arg(args, 0))?;
    let b = to_bigint(vm, arg(args, 1))?.as_bigint().unwrap();
    let r = b.get().as_uint_n(bits);
    Ok(vm.new_bigint(r))
}
