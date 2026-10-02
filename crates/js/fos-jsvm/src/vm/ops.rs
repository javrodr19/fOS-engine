//! Type conversions and operators

use crate::bigint::BigInt;
use crate::gc::Gc;
use crate::number::{number_to_string, string_to_number};
use crate::object::*;
use crate::string::{Atom, JsString, Units, atoms};
use crate::value::Value;

use super::{JsResult, Vm};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arith {
    Sub,
    Mul,
    Div,
    Mod,
    Exp,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Sar,
    Shr,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hint {
    Default,
    Number,
    String,
}

/// ToBoolean
#[inline]
pub fn truthy(v: Value) -> bool {
    if let Some(b) = v.as_bool() {
        return b;
    }
    if let Some(i) = v.as_int() {
        return i != 0;
    }
    if v.is_double() {
        let n = v.number_unchecked();
        return !(n == 0.0 || n.is_nan());
    }
    if v.is_nullish() || v.is_hole() {
        return false;
    }
    if let Some(s) = v.as_string() {
        return !s.get().is_empty();
    }
    if let Some(b) = v.as_bigint() {
        return !b.get().is_zero();
    }
    true
}

/// `%` on numbers (sign of the dividend, like C's fmod)
#[inline]
pub fn js_rem(a: f64, b: f64) -> f64 {
    a % b
}

/// IsStrictlyEqual
#[inline]
pub fn strict_equals(a: Value, b: Value) -> bool {
    if a.raw() == b.raw() {
        return !(a.is_double() && a.number_unchecked().is_nan());
    }
    if a.is_number() && b.is_number() {
        return a.number_unchecked() == b.number_unchecked();
    }
    if let (Some(x), Some(y)) = (a.as_string(), b.as_string()) {
        return x.get().equals(y.get());
    }
    if let (Some(x), Some(y)) = (a.as_bigint(), b.as_bigint()) {
        return x.get() == y.get();
    }
    false
}

/// SameValueZero (Map keys, includes)
pub fn same_value_zero(a: Value, b: Value) -> bool {
    if a.is_number() && b.is_number() {
        let (x, y) = (a.number_unchecked(), b.number_unchecked());
        return x == y || (x.is_nan() && y.is_nan());
    }
    strict_equals(a, b)
}

/// SameValue (Object.is)
pub fn same_value(a: Value, b: Value) -> bool {
    if a.is_number() && b.is_number() {
        let (x, y) = (a.number_unchecked(), b.number_unchecked());
        if x.is_nan() && y.is_nan() {
            return true;
        }
        return x == y && x.is_sign_negative() == y.is_sign_negative();
    }
    strict_equals(a, b)
}

/// ToInt32 of a number
pub fn f64_to_int32(n: f64) -> i32 {
    if n.is_finite() && n.abs() < 2147483648.0 {
        return n as i32;
    }
    if !n.is_finite() {
        return 0;
    }
    let m = n.trunc() % 4294967296.0;
    let m = if m < 0.0 { m + 4294967296.0 } else { m };
    m as u32 as i32
}

/// A numeric value: a number or a BigInt (ToNumeric)
pub enum Numeric {
    Number(f64),
    BigInt(Gc<BigInt>),
}

impl Vm {
    /// ToNumeric
    pub fn to_numeric(&mut self, v: Value) -> JsResult<Numeric> {
        if let Some(n) = v.as_number() {
            return Ok(Numeric::Number(n));
        }
        if let Some(b) = v.as_bigint() {
            return Ok(Numeric::BigInt(b));
        }
        let p = self.to_primitive(v, Hint::Number)?;
        if let Some(b) = p.as_bigint() {
            return Ok(Numeric::BigInt(b));
        }
        Ok(Numeric::Number(self.to_number(p)?))
    }

    fn mix_error(&mut self) -> Value {
        self.type_error("Cannot mix BigInt and other types, use explicit conversions")
    }

    pub fn to_number(&mut self, v: Value) -> JsResult<f64> {
        if let Some(n) = v.as_number() {
            return Ok(n);
        }
        if let Some(b) = v.as_bool() {
            return Ok(if b { 1.0 } else { 0.0 });
        }
        if v.is_undefined() || v.is_hole() {
            return Ok(f64::NAN);
        }
        if v.is_null() {
            return Ok(0.0);
        }
        if let Some(s) = v.as_string() {
            return Ok(string_to_number_units(&s.get().units()));
        }
        if v.is_symbol() {
            return Err(self.type_error("Cannot convert a Symbol value to a number"));
        }
        if v.is_bigint() {
            return Err(self.type_error("Cannot convert a BigInt value to a number"));
        }
        let p = self.to_primitive(v, Hint::Number)?;
        self.to_number(p)
    }

    pub fn to_int32(&mut self, v: Value) -> JsResult<i32> {
        if let Some(i) = v.as_int() {
            return Ok(i);
        }
        Ok(f64_to_int32(self.to_number(v)?))
    }

    pub fn to_uint32(&mut self, v: Value) -> JsResult<u32> {
        Ok(self.to_int32(v)? as u32)
    }

    /// ToIntegerOrInfinity
    pub fn to_integer(&mut self, v: Value) -> JsResult<f64> {
        if let Some(i) = v.as_int() {
            return Ok(i as f64);
        }
        let n = self.to_number(v)?;
        Ok(if n.is_nan() { 0.0 } else { n.trunc() + 0.0 })
    }

    /// ToLength
    pub fn to_length(&mut self, v: Value) -> JsResult<f64> {
        let n = self.to_integer(v)?;
        Ok(n.clamp(0.0, 9007199254740991.0))
    }

    pub fn number_to_js_string(&mut self, n: f64) -> Gc<JsString> {
        if n >= 0.0 && n < 10.0 && n.fract() == 0.0 && !(n == 0.0 && n.is_sign_negative()) {
            return self.char_strings[b'0' as usize + n as usize];
        }
        if n.is_nan() {
            return self.atoms.string(atoms::NaN);
        }
        let s = number_to_string(n);
        self.new_string_latin1(s.into_bytes())
    }

    pub fn to_string(&mut self, v: Value) -> JsResult<Gc<JsString>> {
        if let Some(s) = v.as_string() {
            return Ok(s);
        }
        if let Some(n) = v.as_number() {
            return Ok(self.number_to_js_string(n));
        }
        if let Some(b) = v.as_bigint() {
            let text = b.get().to_string_radix(10);
            return Ok(self.new_string(&text));
        }
        let atom = match v {
            Value::UNDEFINED | Value::HOLE => atoms::undefined,
            Value::NULL => atoms::null,
            Value::TRUE => atoms::true_,
            Value::FALSE => atoms::false_,
            _ => {
                if v.is_symbol() {
                    return Err(self.type_error("Cannot convert a Symbol value to a string"));
                }
                let p = self.to_primitive(v, Hint::String)?;
                return self.to_string(p);
            }
        };
        Ok(self.atoms.string(atom))
    }

    /// Rust string of a value (ToString)
    pub fn to_rust_string(&mut self, v: Value) -> JsResult<String> {
        Ok(self.to_string(v)?.get().to_rust_string())
    }

    pub fn to_primitive(&mut self, v: Value, hint: Hint) -> JsResult<Value> {
        let Some(o) = v.as_object() else { return Ok(v) };
        let exotic = self.get(v, PropertyKey::Symbol(self.sym.to_primitive))?;
        if !exotic.is_nullish() {
            let h = match hint {
                Hint::Default => atoms::default_,
                Hint::Number => atoms::number,
                Hint::String => atoms::string,
            };
            let hv = self.atom_value(h);
            let r = self.call(exotic, v, &[hv])?;
            if r.is_object() {
                return Err(self.type_error("Cannot convert object to primitive value"));
            }
            return Ok(r);
        }
        let hint = if hint == Hint::Default && matches!(o.get().kind, ObjectKind::Date(_)) { Hint::String } else { hint };
        let order = if hint == Hint::String { [atoms::toString, atoms::valueOf] } else { [atoms::valueOf, atoms::toString] };
        for name in order {
            let f = self.get(v, PropertyKey::Atom(name))?;
            if f.as_object().is_some_and(|f| f.get().is_callable()) {
                let r = self.call(f, v, &[])?;
                if !r.is_object() {
                    return Ok(r);
                }
            }
        }
        Err(self.type_error("Cannot convert object to primitive value"))
    }

    pub fn to_object(&mut self, v: Value) -> JsResult<Gc<JsObject>> {
        if let Some(o) = v.as_object() {
            return Ok(o);
        }
        let (proto, kind) = if let Some(s) = v.as_string() {
            (self.realm.string_proto, ObjectKind::String(s))
        } else if let Some(n) = v.as_number() {
            (self.realm.number_proto, ObjectKind::Number(n))
        } else if let Some(b) = v.as_bool() {
            (self.realm.boolean_proto, ObjectKind::Boolean(b))
        } else if let Some(s) = v.as_symbol() {
            (self.realm.symbol_proto, ObjectKind::Symbol(s))
        } else if let Some(b) = v.as_bigint() {
            (self.realm.bigint_proto, ObjectKind::BigInt(b))
        } else {
            return Err(self.type_error(&format!("Cannot convert {v:?} to object")));
        };
        Ok(self.new_object_with(Some(proto), kind))
    }

    pub fn typeof_atom(&self, v: Value) -> Atom {
        if v.is_number() {
            atoms::number
        } else if v.is_string() {
            atoms::string
        } else if v.is_bool() {
            atoms::boolean
        } else if v.is_undefined() || v.is_hole() {
            atoms::undefined
        } else if v.is_symbol() {
            atoms::symbol
        } else if v.is_bigint() {
            atoms::bigint
        } else if let Some(o) = v.as_object() {
            if o.get().is_callable() { atoms::function } else { atoms::object }
        } else {
            atoms::object
        }
    }

    pub fn loose_equals(&mut self, a: Value, b: Value) -> JsResult<bool> {
        if a.is_number() && b.is_number() {
            return Ok(a.number_unchecked() == b.number_unchecked());
        }
        if a.is_nullish() || b.is_nullish() {
            return Ok(a.is_nullish() && b.is_nullish());
        }
        // BigInt against BigInt, number or string
        if let Some(x) = a.as_bigint().or_else(|| b.as_bigint()) {
            let other = if a.is_bigint() { b } else { a };
            if let Some(y) = other.as_bigint() {
                return Ok(x.get() == y.get());
            }
            if let Some(n) = other.as_number() {
                return Ok(x.get().cmp_f64(n) == Some(std::cmp::Ordering::Equal));
            }
            if let Some(s) = other.as_string() {
                let parsed = BigInt::parse(&s.get().to_rust_string());
                return Ok(parsed.is_some_and(|y| &y == x.get()));
            }
        }
        if (a.is_string() && b.is_string()) || (a.is_object() && b.is_object()) || (a.is_symbol() && b.is_symbol()) || (a.is_bool() && b.is_bool()) {
            return Ok(strict_equals(a, b));
        }
        if a.is_number() && b.is_string() {
            let n = self.to_number(b)?;
            return Ok(a.number_unchecked() == n);
        }
        if a.is_string() && b.is_number() {
            let n = self.to_number(a)?;
            return Ok(n == b.number_unchecked());
        }
        if a.is_bool() {
            let n = self.to_number(a)?;
            return self.loose_equals(Value::number(n), b);
        }
        if b.is_bool() {
            let n = self.to_number(b)?;
            return self.loose_equals(a, Value::number(n));
        }
        if a.is_object() && !b.is_object() {
            let p = self.to_primitive(a, Hint::Default)?;
            return self.loose_equals(p, b);
        }
        if b.is_object() && !a.is_object() {
            let p = self.to_primitive(b, Hint::Default)?;
            return self.loose_equals(a, p);
        }
        Ok(false)
    }

    pub fn add_slow(&mut self, a: Value, b: Value) -> JsResult<Value> {
        let pa = self.to_primitive(a, Hint::Default)?;
        let pb = self.to_primitive(b, Hint::Default)?;
        if pa.is_string() || pb.is_string() {
            let sa = self.to_string(pa)?;
            let sb = self.to_string(pb)?;
            return Ok(Value::string(self.concat(sa, sb)));
        }
        if pa.is_bigint() || pb.is_bigint() {
            return match (pa.as_bigint(), pb.as_bigint()) {
                (Some(x), Some(y)) => {
                    let r = x.get().add(y.get());
                    Ok(self.new_bigint(r))
                }
                _ => Err(self.mix_error()),
            };
        }
        let x = self.to_number(pa)?;
        let y = self.to_number(pb)?;
        Ok(Value::number(x + y))
    }

    /// A binary operator on two BigInts
    fn bigint_arith(&mut self, op: Arith, x: &BigInt, y: &BigInt) -> JsResult<Value> {
        // Shifts past this many bits are certainly too large
        const MAX_SHIFT: i64 = 1 << 30;
        let shift = |y: &BigInt| y.to_i64().map(|s| s.clamp(-MAX_SHIFT - 1, MAX_SHIFT + 1)).unwrap_or(if y.is_negative() { -MAX_SHIFT - 1 } else { MAX_SHIFT + 1 });
        let r = match op {
            Arith::Sub => x.sub(y),
            Arith::Mul => x.mul(y),
            Arith::Div | Arith::Mod => match x.divrem(y) {
                Some((q, r)) => if op == Arith::Div { q } else { r },
                None => return Err(self.range_error("Division by zero")),
            },
            Arith::Exp => {
                if y.is_negative() {
                    return Err(self.range_error("Exponent must be non-negative"));
                }
                match x.pow(y) {
                    Some(r) => r,
                    None => return Err(self.range_error("Maximum BigInt size exceeded")),
                }
            }
            Arith::BitAnd => x.and(y),
            Arith::BitOr => x.or(y),
            Arith::BitXor => x.xor(y),
            Arith::Shl | Arith::Sar => {
                let s = if op == Arith::Shl { shift(y) } else { -shift(y) };
                if s > MAX_SHIFT {
                    if x.is_zero() {
                        return Ok(self.new_bigint(BigInt::zero()));
                    }
                    return Err(self.range_error("Maximum BigInt size exceeded"));
                }
                x.shl(s.max(-MAX_SHIFT - 1))
            }
            Arith::Shr => return Err(self.type_error("BigInts have no unsigned right shift, use >> instead")),
        };
        Ok(self.new_bigint(r))
    }

    pub fn arith_slow(&mut self, op: Arith, a: Value, b: Value) -> JsResult<Value> {
        if !(a.is_number() && b.is_number()) && (!a.is_primitive_number_like() || !b.is_primitive_number_like()) {
            let x = self.to_numeric(a)?;
            let y = self.to_numeric(b)?;
            return match (x, y) {
                (Numeric::BigInt(x), Numeric::BigInt(y)) => self.bigint_arith(op, x.get(), y.get()),
                (Numeric::Number(x), Numeric::Number(y)) => self.arith_slow(op, Value::number(x), Value::number(y)),
                _ => Err(self.mix_error()),
            };
        }
        match op {
            Arith::BitAnd | Arith::BitOr | Arith::BitXor | Arith::Shl | Arith::Sar | Arith::Shr => {
                let x = self.to_int32(a)?;
                let y = self.to_int32(b)?;
                Ok(match op {
                    Arith::BitAnd => Value::int(x & y),
                    Arith::BitOr => Value::int(x | y),
                    Arith::BitXor => Value::int(x ^ y),
                    Arith::Shl => Value::int(x.wrapping_shl(y as u32 & 31)),
                    Arith::Sar => Value::int(x >> (y as u32 & 31)),
                    _ => Value::number(((x as u32) >> (y as u32 & 31)) as f64),
                })
            }
            _ => {
                let x = self.to_number(a)?;
                let y = self.to_number(b)?;
                Ok(Value::number(match op {
                    Arith::Sub => x - y,
                    Arith::Mul => x * y,
                    Arith::Div => x / y,
                    Arith::Mod => js_rem(x, y),
                    _ => js_pow(x, y),
                }))
            }
        }
    }

    pub fn compare_slow(&mut self, a: Value, b: Value, op: Cmp) -> JsResult<bool> {
        // Operands are converted left to right; `>` and `<=` swap them
        let pa = self.to_primitive(a, Hint::Number)?;
        let pb = self.to_primitive(b, Hint::Number)?;
        if let (Some(x), Some(y)) = (pa.as_string(), pb.as_string()) {
            let ord = x.get().units().cmp_units(&y.get().units());
            return Ok(match op {
                Cmp::Lt => ord.is_lt(),
                Cmp::Le => ord.is_le(),
                Cmp::Gt => ord.is_gt(),
                Cmp::Ge => ord.is_ge(),
            });
        }
        if pa.is_bigint() || pb.is_bigint() {
            use std::cmp::Ordering;
            let numeric = |vm: &mut Vm, v: Value| -> JsResult<Result<BigInt, f64>> {
                if let Some(b) = v.as_bigint() {
                    return Ok(Ok(b.get().clone()));
                }
                if let Some(s) = v.as_string() {
                    // An invalid string compares as NaN
                    return Ok(BigInt::parse(&s.get().to_rust_string()).ok_or(f64::NAN));
                }
                Ok(Err(vm.to_number(v)?))
            };
            let (x, y) = (numeric(self, pa)?, numeric(self, pb)?);
            let ord: Option<Ordering> = match (&x, &y) {
                (Ok(x), Ok(y)) => Some(x.cmp(y)),
                (Ok(x), Err(n)) => x.cmp_f64(*n),
                (Err(n), Ok(y)) => y.cmp_f64(*n).map(Ordering::reverse),
                (Err(m), Err(n)) => m.partial_cmp(n),
            };
            let Some(ord) = ord else { return Ok(false) };
            return Ok(match op {
                Cmp::Lt => ord.is_lt(),
                Cmp::Le => ord.is_le(),
                Cmp::Gt => ord.is_gt(),
                Cmp::Ge => ord.is_ge(),
            });
        }
        let x = self.to_number(pa)?;
        let y = self.to_number(pb)?;
        Ok(match op {
            Cmp::Lt => x < y,
            Cmp::Le => x <= y,
            Cmp::Gt => x > y,
            Cmp::Ge => x >= y,
        })
    }

    /// `key in obj`
    pub fn has_in(&mut self, key: Value, obj: Value) -> JsResult<bool> {
        let Some(o) = obj.as_object() else {
            return Err(self.type_error("Cannot use 'in' operator to search for a key in a non-object"));
        };
        let k = self.to_property_key(key)?;
        self.has_property_js(o, k)
    }

    pub fn instance_of(&mut self, v: Value, target: Value) -> JsResult<bool> {
        let Some(t) = target.as_object() else {
            return Err(self.type_error("Right-hand side of 'instanceof' is not an object"));
        };
        let custom = self.get(target, PropertyKey::Symbol(self.sym.has_instance))?;
        if !custom.is_nullish() {
            let r = self.call(custom, target, &[v])?;
            return Ok(truthy(r));
        }
        if !t.get().is_callable() {
            return Err(self.type_error("Right-hand side of 'instanceof' is not callable"));
        }
        self.ordinary_has_instance(t, v)
    }

    pub(crate) fn ordinary_has_instance(&mut self, t: Gc<JsObject>, v: Value) -> JsResult<bool> {
        if let ObjectKind::Bound(b) = &t.get().kind {
            let target = b.target;
            return self.instance_of(v, Value::object(target));
        }
        let Some(o) = v.as_object() else { return Ok(false) };
        let p = self.get(Value::object(t), PropertyKey::Atom(atoms::prototype))?;
        let Some(p) = p.as_object() else {
            return Err(self.type_error("Function has non-object prototype in instanceof check"));
        };
        let mut cur = self.prototype_of(o)?;
        while let Some(c) = cur {
            if c == p {
                return Ok(true);
            }
            cur = self.prototype_of(c)?;
        }
        Ok(false)
    }

    /// [[GetPrototypeOf]]: a proxy's comes from its handler (or target)
    pub(crate) fn prototype_of(&mut self, o: Gc<JsObject>) -> JsResult<Option<Gc<JsObject>>> {
        if crate::builtins::proxy::is_proxy(o) {
            return Ok(self.proxy_get_prototype(o)?.as_object());
        }
        Ok(o.get().proto)
    }

    pub fn is_callable(&self, v: Value) -> bool {
        v.as_object().is_some_and(|o| o.get().is_callable())
    }

    pub fn is_constructor(&self, v: Value) -> bool {
        v.as_object().is_some_and(|o| match &o.get().kind {
            ObjectKind::Function(c) => c.proto.is_constructor,
            ObjectKind::Native(n) => n.construct.is_some(),
            ObjectKind::Bound(b) => self.is_constructor(Value::object(b.target)),
            ObjectKind::Proxy(p) => p.constructor,
            _ => false,
        })
    }
}

/// Number ** Number with the ECMAScript special cases
pub fn js_pow(x: f64, y: f64) -> f64 {
    if y.is_nan() {
        return f64::NAN;
    }
    if y == 0.0 {
        return 1.0;
    }
    if (x == 1.0 || x == -1.0) && y.is_infinite() {
        return f64::NAN;
    }
    x.powf(y)
}

pub fn string_to_number_units(u: &Units) -> f64 {
    match u {
        Units::Latin1(b) if b.is_ascii() => string_to_number(std::str::from_utf8(b).unwrap()),
        _ => string_to_number(&u.to_rust_string()),
    }
}
