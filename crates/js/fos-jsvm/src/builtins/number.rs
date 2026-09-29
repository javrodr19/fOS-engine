//! Number, Boolean, parseInt and parseFloat

use crate::gc::Gc;
use crate::number::number_to_string;
use crate::object::*;
use crate::string::atoms;
use crate::value::Value;
use crate::vm::{JsResult, Vm};

use super::arg;
use super::string::is_js_whitespace;

pub(super) fn init(vm: &mut Vm) {
    let proto = vm.realm.number_proto;
    let c = vm.def_ctor("Number", 1, number_call, Some(number_construct), proto);
    let consts = [
        ("MAX_SAFE_INTEGER", 9007199254740991.0),
        ("MIN_SAFE_INTEGER", -9007199254740991.0),
        ("EPSILON", f64::EPSILON),
        ("MAX_VALUE", f64::MAX),
        ("MIN_VALUE", 5e-324),
        ("POSITIVE_INFINITY", f64::INFINITY),
        ("NEGATIVE_INFINITY", f64::NEG_INFINITY),
        ("NaN", f64::NAN),
    ];
    for (name, v) in consts {
        vm.def_value(c, name, Value::number(v), PropFlags::FROZEN);
    }
    vm.def_method(c, "isInteger", 1, is_integer);
    vm.def_method(c, "isSafeInteger", 1, is_safe_integer);
    vm.def_method(c, "isFinite", 1, is_finite);
    vm.def_method(c, "isNaN", 1, is_nan);
    vm.def_method(c, "parseFloat", 1, parse_float);
    vm.def_method(c, "parseInt", 2, parse_int);
    vm.def_method(proto, "toString", 1, to_string);
    vm.def_method(proto, "toLocaleString", 0, to_locale_string);
    vm.def_method(proto, "valueOf", 0, value_of);
    vm.def_method(proto, "toFixed", 1, to_fixed);
    vm.def_method(proto, "toPrecision", 1, to_precision);
    vm.def_method(proto, "toExponential", 1, to_exponential);

    let bproto = vm.realm.boolean_proto;
    vm.def_ctor("Boolean", 1, boolean_call, Some(boolean_construct), bproto);
    vm.def_method(bproto, "toString", 0, boolean_to_string);
    vm.def_method(bproto, "valueOf", 0, boolean_value_of);
}

fn number_call(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if args.is_empty() {
        return Ok(Value::int(0));
    }
    Ok(Value::number(vm.to_number(args[0])?))
}

fn number_construct(vm: &mut Vm, new_target: Value, args: &[Value], callee: Gc<JsObject>) -> JsResult<Value> {
    let n = number_call(vm, Value::UNDEFINED, args, callee)?.number_unchecked();
    let proto = vm.prototype_for(new_target, |r| r.number_proto)?;
    Ok(Value::object(vm.new_object_with(Some(proto), ObjectKind::Number(n))))
}

fn this_number(vm: &mut Vm, this: Value, method: &str) -> JsResult<f64> {
    if let Some(n) = this.as_number() {
        return Ok(n);
    }
    if let Some(o) = this.as_object() {
        if let ObjectKind::Number(n) = o.get().kind {
            return Ok(n);
        }
    }
    Err(vm.type_error(&format!("Number.prototype.{method} requires that 'this' be a Number")))
}

fn is_integer(_vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(arg(args, 0).as_number().is_some_and(|n| n.is_finite() && n.trunc() == n)))
}

fn is_safe_integer(_vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(arg(args, 0).as_number().is_some_and(|n| n.is_finite() && n.trunc() == n && n.abs() <= 9007199254740991.0)))
}

fn is_finite(_vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(arg(args, 0).as_number().is_some_and(f64::is_finite)))
}

fn is_nan(_vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(arg(args, 0).as_number().is_some_and(f64::is_nan)))
}

/// Number to string in any radix (2..=36)
pub(crate) fn to_radix_string(x: f64, radix: u32) -> String {
    if radix == 10 || x.is_nan() || x.is_infinite() || x == 0.0 {
        return number_to_string(x);
    }
    let neg = x < 0.0;
    let x = x.abs();
    let digits = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut int = x.trunc();
    let mut frac = x - int;
    let mut int_digits = Vec::new();
    if int == 0.0 {
        int_digits.push(b'0');
    }
    while int >= 1.0 {
        let d = (int % radix as f64) as usize;
        int_digits.push(digits[d]);
        int = (int / radix as f64).trunc();
    }
    int_digits.reverse();
    let mut out = String::from_utf8(int_digits).unwrap();
    if frac > 0.0 {
        out.push('.');
        // Enough digits to distinguish the value
        let mut n = 0;
        while frac > 0.0 && n < 52 {
            frac *= radix as f64;
            let d = frac.trunc() as usize;
            out.push(digits[d] as char);
            frac -= d as f64;
            n += 1;
        }
    }
    if neg { format!("-{out}") } else { out }
}

fn to_string(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let x = this_number(vm, this, "toString")?;
    let r = arg(args, 0);
    let radix = if r.is_undefined() { 10.0 } else { vm.to_integer(r)? };
    if !(2.0..=36.0).contains(&radix) {
        return Err(vm.range_error("toString() radix must be between 2 and 36"));
    }
    if radix == 10.0 {
        return Ok(Value::string(vm.number_to_js_string(x)));
    }
    let s = to_radix_string(x, radix as u32);
    Ok(vm.str_value(&s))
}

fn to_locale_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let x = this_number(vm, this, "toLocaleString")?;
    if !x.is_finite() || x.fract() != 0.0 || x.abs() >= 1e21 {
        return Ok(Value::string(vm.number_to_js_string(x)));
    }
    // Group thousands like en-US
    let digits = format!("{}", x.abs() as u64);
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if x < 0.0 {
        out.insert(0, '-');
    }
    Ok(vm.str_value(&out))
}

fn value_of(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(this_number(vm, this, "valueOf")?))
}

/// Round `x` to `digits` fraction digits, ties away from zero on the exact
/// binary value (as Number.prototype.toFixed specifies)
fn fixed(x: f64, digits: usize) -> String {
    let s = format!("{:.*}", digits, x);
    // Rust rounds exact ties to even; check whether this was a tie
    let wide = format!("{:.*}", digits + 30, x);
    let tail = &wide[wide.len() - 30..];
    if tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0') {
        let exact = format!("{:.*}", 1100, x);
        let dot = exact.find('.').unwrap();
        let rest = &exact[dot + 1 + digits..];
        if rest.starts_with('5') && rest[1..].bytes().all(|b| b == b'0') {
            // Tie: round away from zero
            let step = 10f64.powi(-(digits as i32));
            let bumped = if x >= 0.0 { x + step / 2.0 } else { x - step / 2.0 };
            let r = format!("{:.*}", digits, bumped);
            if r != s {
                return r;
            }
            // Decimal carry by hand
            return round_up_decimal(&exact[..dot + 1 + digits], x < 0.0);
        }
    }
    s
}

fn round_up_decimal(s: &str, _neg: bool) -> String {
    let mut bytes: Vec<u8> = s.bytes().collect();
    let mut i = bytes.len();
    loop {
        if i == 0 {
            let at = if bytes[0] == b'-' { 1 } else { 0 };
            bytes.insert(at, b'1');
            break;
        }
        i -= 1;
        match bytes[i] {
            b'.' | b'-' => continue,
            b'9' => bytes[i] = b'0',
            d => {
                bytes[i] = d + 1;
                break;
            }
        }
    }
    let s = String::from_utf8(bytes).unwrap();
    s.trim_end_matches('.').to_string()
}

fn to_fixed(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let x = this_number(vm, this, "toFixed")?;
    let f = vm.to_integer(arg(args, 0))?;
    if !(0.0..=100.0).contains(&f) {
        return Err(vm.range_error("toFixed() digits argument must be between 0 and 100"));
    }
    if !x.is_finite() || x.abs() >= 1e21 {
        return Ok(Value::string(vm.number_to_js_string(x)));
    }
    let mut s = fixed(x, f as usize);
    if s.starts_with("-") && s[1..].bytes().all(|b| b == b'0' || b == b'.') {
        s.remove(0);
    }
    Ok(vm.str_value(&s))
}

/// (digits, exponent) of x rounded to `p` significant digits
fn significant(x: f64, p: usize) -> (String, i32) {
    let s = format!("{:.*e}", p - 1, x.abs());
    let (m, e) = s.split_once('e').unwrap();
    (m.replace('.', ""), e.parse().unwrap())
}

fn to_precision(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let x = this_number(vm, this, "toPrecision")?;
    if arg(args, 0).is_undefined() {
        return Ok(Value::string(vm.number_to_js_string(x)));
    }
    let p = vm.to_integer(arg(args, 0))?;
    if !x.is_finite() {
        return Ok(Value::string(vm.number_to_js_string(x)));
    }
    if !(1.0..=100.0).contains(&p) {
        return Err(vm.range_error("toPrecision() argument must be between 1 and 100"));
    }
    let p = p as usize;
    let sign = if x < 0.0 { "-" } else { "" };
    if x == 0.0 {
        let s = if p == 1 { "0".to_string() } else { format!("0.{}", "0".repeat(p - 1)) };
        return Ok(vm.str_value(&s));
    }
    let (digits, e) = significant(x, p);
    let s = if e < -6 || e >= p as i32 {
        let mut m = digits[..1].to_string();
        if p > 1 {
            m.push('.');
            m.push_str(&digits[1..]);
        }
        format!("{sign}{m}e{}{}", if e >= 0 { "+" } else { "-" }, e.abs())
    } else if e >= 0 {
        let e = e as usize;
        if e + 1 >= p { format!("{sign}{digits}") } else { format!("{sign}{}.{}", &digits[..e + 1], &digits[e + 1..]) }
    } else {
        format!("{sign}0.{}{}", "0".repeat((-e - 1) as usize), digits)
    };
    Ok(vm.str_value(&s))
}

fn to_exponential(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let x = this_number(vm, this, "toExponential")?;
    let fa = arg(args, 0);
    let f = vm.to_integer(fa)?;
    if !x.is_finite() {
        return Ok(Value::string(vm.number_to_js_string(x)));
    }
    if !(0.0..=100.0).contains(&f) {
        return Err(vm.range_error("toExponential() argument must be between 0 and 100"));
    }
    let s = if fa.is_undefined() {
        // As many digits as needed
        let s = format!("{:e}", x);
        s
    } else {
        format!("{:.*e}", f as usize, x)
    };
    let (m, e) = s.split_once('e').unwrap();
    let e: i32 = e.parse().unwrap();
    let out = format!("{m}e{}{}", if e >= 0 { "+" } else { "-" }, e.abs());
    Ok(vm.str_value(&out))
}

pub(crate) fn parse_int(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_string(arg(args, 0))?;
    let units = s.get().units().to_vec();
    let mut i = 0;
    while i < units.len() && is_js_whitespace(units[i]) {
        i += 1;
    }
    let mut sign = 1.0;
    if i < units.len() && (units[i] == b'-' as u16 || units[i] == b'+' as u16) {
        if units[i] == b'-' as u16 {
            sign = -1.0;
        }
        i += 1;
    }
    let mut radix = vm.to_int32(arg(args, 1))?;
    let mut strip_prefix = true;
    if radix != 0 {
        if !(2..=36).contains(&radix) {
            return Ok(Value::NAN);
        }
        if radix != 16 {
            strip_prefix = false;
        }
    } else {
        radix = 10;
    }
    if strip_prefix && i + 1 < units.len() && units[i] == b'0' as u16 && (units[i + 1] | 0x20) == b'x' as u16 {
        i += 2;
        radix = 16;
    }
    let start = i;
    let mut value = 0f64;
    while i < units.len() {
        let c = units[i];
        let d = match c {
            0x30..=0x39 => (c - 0x30) as i32,
            0x61..=0x7A => (c - 0x61) as i32 + 10,
            0x41..=0x5A => (c - 0x41) as i32 + 10,
            _ => break,
        };
        if d >= radix {
            break;
        }
        value = value * radix as f64 + d as f64;
        i += 1;
    }
    if i == start {
        return Ok(Value::NAN);
    }
    if radix == 10 && i - start > 15 {
        // Exact decimal conversion for long digit strings
        let text: String = units[start..i].iter().map(|&u| u as u8 as char).collect();
        value = text.parse().unwrap_or(value);
    }
    Ok(Value::number(sign * value))
}

pub(crate) fn parse_float(vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_string(arg(args, 0))?;
    let text = s.get().to_rust_string();
    let t = text.trim_start_matches(|c: char| c.len_utf16() == 1 && is_js_whitespace(c as u16));
    let b = t.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    if t[i..].starts_with("Infinity") {
        return Ok(Value::number(if b.first() == Some(&b'-') { f64::NEG_INFINITY } else { f64::INFINITY }));
    }
    let digits_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    let mantissa = &t[digits_start..i];
    if mantissa.is_empty() || mantissa == "." {
        return Ok(Value::NAN);
    }
    if i < b.len() && (b[i] | 0x20) == b'e' {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let exp_start = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > exp_start {
            i = j;
        }
    }
    Ok(Value::number(t[..i].parse::<f64>().unwrap_or(f64::NAN)))
}

fn boolean_call(_vm: &mut Vm, _this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(crate::vm::ops::truthy(arg(args, 0))))
}

fn boolean_construct(vm: &mut Vm, new_target: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let b = crate::vm::ops::truthy(arg(args, 0));
    let proto = vm.prototype_for(new_target, |r| r.boolean_proto)?;
    Ok(Value::object(vm.new_object_with(Some(proto), ObjectKind::Boolean(b))))
}

fn this_bool(vm: &mut Vm, this: Value) -> JsResult<bool> {
    if let Some(b) = this.as_bool() {
        return Ok(b);
    }
    if let Some(o) = this.as_object() {
        if let ObjectKind::Boolean(b) = o.get().kind {
            return Ok(b);
        }
    }
    Err(vm.type_error("Boolean.prototype method requires that 'this' be a Boolean"))
}

fn boolean_to_string(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let b = this_bool(vm, this)?;
    Ok(vm.atom_value(if b { atoms::true_ } else { atoms::false_ }))
}

fn boolean_value_of(vm: &mut Vm, this: Value, _args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(this_bool(vm, this)?))
}
