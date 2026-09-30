//! Math

use std::cell::Cell;

use crate::gc::Gc;
use crate::object::*;
use crate::value::Value;
use crate::vm::{JsResult, NativeFn, Vm};

use super::arg;

pub(super) fn init(vm: &mut Vm) {
    let m = vm.new_object();
    let consts = [
        ("E", std::f64::consts::E),
        ("LN10", std::f64::consts::LN_10),
        ("LN2", std::f64::consts::LN_2),
        ("LOG10E", std::f64::consts::LOG10_E),
        ("LOG2E", std::f64::consts::LOG2_E),
        ("PI", std::f64::consts::PI),
        ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
        ("SQRT2", std::f64::consts::SQRT_2),
    ];
    for (name, v) in consts {
        vm.def_value(m, name, Value::number(v), PropFlags::FROZEN);
    }
    macro_rules! unary {
        ($($name:literal => $f:expr),* $(,)?) => {
            $(
                {
                    fn f(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
                        let x = num(vm, arg(args, 0))?;
                        let g: fn(f64) -> f64 = $f;
                        Ok(Value::number(g(x)))
                    }
                    vm.def_method(m, $name, 1, f);
                }
            )*
        };
    }
    unary! {
        "abs" => f64::abs, "acos" => f64::acos, "acosh" => f64::acosh, "asin" => f64::asin,
        "asinh" => f64::asinh, "atan" => f64::atan, "atanh" => f64::atanh, "cbrt" => f64::cbrt,
        "ceil" => f64::ceil, "cos" => f64::cos, "cosh" => f64::cosh, "exp" => f64::exp,
        "expm1" => f64::exp_m1, "log" => f64::ln, "log1p" => f64::ln_1p, "log10" => f64::log10,
        "log2" => f64::log2, "sin" => f64::sin, "sinh" => f64::sinh, "sqrt" => f64::sqrt,
        "tan" => f64::tan, "tanh" => f64::tanh, "trunc" => f64::trunc,
        "fround" => |x| x as f32 as f64,
        "sign" => |x| if x.is_nan() || x == 0.0 { x } else { x.signum() },
    }
    let fns: &[(&str, u32, NativeFn)] = &[
        ("floor", 1, floor),
        ("round", 1, round),
        ("max", 2, max),
        ("min", 2, min),
        ("pow", 2, pow),
        ("atan2", 2, atan2),
        ("hypot", 2, hypot),
        ("imul", 2, imul),
        ("clz32", 1, clz32),
        ("random", 0, random),
    ];
    for &(name, len, f) in fns {
        vm.def_method(m, name, len, f);
    }
    let tag = vm.sym.to_string_tag;
    let s = vm.str_value("Math");
    vm.define_value(m, PropertyKey::Symbol(tag), s, PropFlags(PropFlags::CONFIGURABLE));
    let g = vm.global;
    vm.def_value(g, "Math", Value::object(m), PropFlags::HIDDEN);
}

#[inline]
fn num(vm: &mut Vm, v: Value) -> JsResult<f64> {
    match v.as_number() {
        Some(n) => Ok(n),
        None => vm.to_number(v),
    }
}

fn floor(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let v = arg(args, 0);
    if v.is_int() {
        return Ok(v);
    }
    Ok(Value::number(num(vm, v)?.floor()))
}

fn round(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let v = arg(args, 0);
    if v.is_int() {
        return Ok(v);
    }
    let x = num(vm, v)?;
    if !x.is_finite() || x == 0.0 {
        return Ok(Value::number(x));
    }
    if (-0.5..0.0).contains(&x) {
        return Ok(Value::double(-0.0));
    }
    let f = x.floor();
    Ok(Value::number(if x - f >= 0.5 { f + 1.0 } else { f }))
}

fn max(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if let [a, b] = args {
        if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
            return Ok(Value::int(x.max(y)));
        }
    }
    let mut r = f64::NEG_INFINITY;
    let mut nan = false;
    for &a in args {
        let x = num(vm, a)?;
        if x.is_nan() {
            nan = true;
        } else if x > r || (x == 0.0 && r == 0.0 && !x.is_sign_negative()) {
            r = x;
        }
    }
    Ok(Value::number(if nan { f64::NAN } else { r }))
}

fn min(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if let [a, b] = args {
        if let (Some(x), Some(y)) = (a.as_int(), b.as_int()) {
            return Ok(Value::int(x.min(y)));
        }
    }
    let mut r = f64::INFINITY;
    let mut nan = false;
    for &a in args {
        let x = num(vm, a)?;
        if x.is_nan() {
            nan = true;
        } else if x < r || (x == 0.0 && r == 0.0 && x.is_sign_negative()) {
            r = x;
        }
    }
    Ok(Value::number(if nan { f64::NAN } else { r }))
}

fn pow(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let x = num(vm, arg(args, 0))?;
    let y = num(vm, arg(args, 1))?;
    Ok(Value::number(crate::vm::ops::js_pow(x, y)))
}

fn atan2(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let y = num(vm, arg(args, 0))?;
    let x = num(vm, arg(args, 1))?;
    Ok(Value::number(y.atan2(x)))
}

fn hypot(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let mut values = Vec::with_capacity(args.len());
    for &a in args {
        values.push(num(vm, a)?);
    }
    if values.iter().any(|v| v.is_infinite()) {
        return Ok(Value::number(f64::INFINITY));
    }
    Ok(Value::number(values.iter().map(|v| v * v).sum::<f64>().sqrt()))
}

fn imul(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let a = vm.to_int32(arg(args, 0))?;
    let b = vm.to_int32(arg(args, 1))?;
    Ok(Value::int(a.wrapping_mul(b)))
}

fn clz32(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let a = vm.to_uint32(arg(args, 0))?;
    Ok(Value::int(a.leading_zeros() as i32))
}

thread_local! {
    static RNG: Cell<(u64, u64)> = Cell::new({
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0x1234_5678);
        let a = t ^ 0x9E37_79B9_7F4A_7C15;
        (a | 1, a.rotate_left(29) ^ 0xBF58_476D_1CE4_E5B9)
    });
}

/// xorshift128+
fn random(_vm: &mut Vm, _this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let r = RNG.with(|c| {
        let (mut s1, s0) = c.get();
        s1 ^= s1 << 23;
        s1 ^= s1 >> 17;
        s1 ^= s0 ^ (s0 >> 26);
        c.set((s0, s1));
        s0.wrapping_add(s1)
    });
    Ok(Value::number((r >> 11) as f64 / (1u64 << 53) as f64))
}
