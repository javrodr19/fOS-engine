//! JavaScript values
//!
//! A value is 64 bits. Doubles are stored as themselves; everything else
//! lives in NaN space that doubles never use (the top 16 bits are 0xFFF9
//! or above, and every NaN is canonicalized to 0x7FF8_0000_0000_0000):
//!
//! | top 16 bits | payload                               |
//! |-------------|---------------------------------------|
//! | 0xFFF9      | int32 (low 32 bits)                   |
//! | 0xFFFA      | undefined, null, booleans, hole       |
//! | 0xFFFB      | string cell address                   |
//! | 0xFFFC      | object cell address                   |
//! | 0xFFFD      | symbol cell address                   |
//!
//! Integers in int32 range are stored as int32 (except -0), which gives
//! arithmetic and array indexing an integer fast path.

use crate::gc::Gc;
use crate::object::{JsObject, Symbol};
use crate::string::JsString;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Value(u64);

const TAG_SHIFT: u32 = 48;
const TAG_INT: u64 = 0xFFF9;
const TAG_MISC: u64 = 0xFFFA;
const TAG_STRING: u64 = 0xFFFB;
const TAG_OBJECT: u64 = 0xFFFC;
const TAG_SYMBOL: u64 = 0xFFFD;
const PAYLOAD: u64 = 0x0000_FFFF_FFFF_FFFF;
const CANONICAL_NAN: u64 = 0x7FF8_0000_0000_0000;

const fn misc(n: u64) -> u64 {
    (TAG_MISC << TAG_SHIFT) | n
}

impl Value {
    pub const UNDEFINED: Value = Value(misc(0));
    pub const NULL: Value = Value(misc(1));
    pub const FALSE: Value = Value(misc(2));
    pub const TRUE: Value = Value(misc(3));
    /// Missing array element / uninitialized binding (never user-visible)
    pub const HOLE: Value = Value(misc(4));
    pub const NAN: Value = Value(CANONICAL_NAN);

    #[inline(always)]
    fn tag(self) -> u64 {
        self.0 >> TAG_SHIFT
    }

    #[inline(always)]
    pub fn raw(self) -> u64 {
        self.0
    }

    #[inline(always)]
    pub fn bool(b: bool) -> Value {
        if b { Value::TRUE } else { Value::FALSE }
    }

    #[inline(always)]
    pub fn int(i: i32) -> Value {
        Value((TAG_INT << TAG_SHIFT) | (i as u32 as u64))
    }

    /// A number, stored as int32 when that is exact
    #[inline(always)]
    pub fn number(n: f64) -> Value {
        let i = n as i32;
        if i as f64 == n && !(i == 0 && n.is_sign_negative()) {
            return Value::int(i);
        }
        Value::double(n)
    }

    /// A number always stored as a double (NaN canonicalized)
    #[inline(always)]
    pub fn double(n: f64) -> Value {
        if n.is_nan() { Value::NAN } else { Value(n.to_bits()) }
    }

    #[inline(always)]
    pub fn string(s: Gc<JsString>) -> Value {
        Value((TAG_STRING << TAG_SHIFT) | s.addr() as u64)
    }

    #[inline(always)]
    pub fn object(o: Gc<JsObject>) -> Value {
        Value((TAG_OBJECT << TAG_SHIFT) | o.addr() as u64)
    }

    #[inline(always)]
    pub fn symbol(s: Gc<Symbol>) -> Value {
        Value((TAG_SYMBOL << TAG_SHIFT) | s.addr() as u64)
    }

    // ---- type tests ----

    #[inline(always)]
    pub fn is_int(self) -> bool {
        self.tag() == TAG_INT
    }

    #[inline(always)]
    pub fn is_double(self) -> bool {
        self.tag() < TAG_INT
    }

    #[inline(always)]
    pub fn is_number(self) -> bool {
        self.tag() <= TAG_INT
    }

    #[inline(always)]
    pub fn is_undefined(self) -> bool {
        self == Value::UNDEFINED
    }

    #[inline(always)]
    pub fn is_null(self) -> bool {
        self == Value::NULL
    }

    #[inline(always)]
    pub fn is_nullish(self) -> bool {
        self == Value::UNDEFINED || self == Value::NULL
    }

    #[inline(always)]
    pub fn is_bool(self) -> bool {
        self == Value::TRUE || self == Value::FALSE
    }

    #[inline(always)]
    pub fn is_hole(self) -> bool {
        self == Value::HOLE
    }

    #[inline(always)]
    pub fn is_string(self) -> bool {
        self.tag() == TAG_STRING
    }

    #[inline(always)]
    pub fn is_object(self) -> bool {
        self.tag() == TAG_OBJECT
    }

    #[inline(always)]
    pub fn is_symbol(self) -> bool {
        self.tag() == TAG_SYMBOL
    }

    // ---- extraction ----

    #[inline(always)]
    pub fn as_int(self) -> Option<i32> {
        if self.is_int() { Some(self.0 as u32 as i32) } else { None }
    }

    /// The numeric value, if this is a number
    #[inline(always)]
    pub fn as_number(self) -> Option<f64> {
        match self.tag() {
            TAG_INT => Some(self.0 as u32 as i32 as f64),
            t if t < TAG_INT => Some(f64::from_bits(self.0)),
            _ => None,
        }
    }

    /// Numeric value of a value known to be a number
    #[inline(always)]
    pub fn number_unchecked(self) -> f64 {
        if self.is_int() { self.0 as u32 as i32 as f64 } else { f64::from_bits(self.0) }
    }

    #[inline(always)]
    pub fn as_bool(self) -> Option<bool> {
        match self {
            Value::TRUE => Some(true),
            Value::FALSE => Some(false),
            _ => None,
        }
    }

    #[inline(always)]
    pub fn as_string(self) -> Option<Gc<JsString>> {
        if self.is_string() { Some(unsafe { Gc::from_addr((self.0 & PAYLOAD) as usize) }) } else { None }
    }

    #[inline(always)]
    pub fn as_object(self) -> Option<Gc<JsObject>> {
        if self.is_object() { Some(unsafe { Gc::from_addr((self.0 & PAYLOAD) as usize) }) } else { None }
    }

    #[inline(always)]
    pub fn as_symbol(self) -> Option<Gc<Symbol>> {
        if self.is_symbol() { Some(unsafe { Gc::from_addr((self.0 & PAYLOAD) as usize) }) } else { None }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::bool(b)
    }
}

impl From<i32> for Value {
    fn from(i: i32) -> Value {
        Value::int(i)
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Value {
        Value::number(n)
    }
}

impl From<Gc<JsObject>> for Value {
    fn from(o: Gc<JsObject>) -> Value {
        Value::object(o)
    }
}

impl From<Gc<JsString>> for Value {
    fn from(s: Gc<JsString>) -> Value {
        Value::string(s)
    }
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(n) = self.as_number() {
            return write!(f, "{}", crate::number::number_to_string(n));
        }
        match *self {
            Value::UNDEFINED => write!(f, "undefined"),
            Value::NULL => write!(f, "null"),
            Value::TRUE => write!(f, "true"),
            Value::FALSE => write!(f, "false"),
            Value::HOLE => write!(f, "<hole>"),
            _ => {
                if let Some(s) = self.as_string() {
                    write!(f, "{:?}", s.get().to_rust_string())
                } else if self.is_object() {
                    write!(f, "[object]")
                } else {
                    write!(f, "Symbol()")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numbers() {
        assert_eq!(Value::number(3.0).as_int(), Some(3));
        assert!(Value::number(3.5).is_double());
        assert!(Value::number(-0.0).is_double(), "-0 is not an int32");
        assert!(Value::number(4294967296.0).is_double());
        assert_eq!(Value::number(-7.0).as_number(), Some(-7.0));
        assert!(Value::number(f64::NAN).as_number().unwrap().is_nan());
        assert_eq!(Value::number(f64::NAN), Value::NAN);
        // A NaN with a payload in the tag space must not become a pointer
        let evil = f64::from_bits(0xFFFC_0000_1234_5678);
        assert!(Value::number(evil).is_number());
        assert!(Value::number(f64::INFINITY).is_number());
        assert!(Value::number(f64::NEG_INFINITY).is_number());
        let neg_nan = f64::from_bits(0xFFF8_0000_0000_0000);
        assert!(Value::double(neg_nan).is_number());
    }

    #[test]
    fn test_misc() {
        assert!(Value::UNDEFINED.is_nullish() && Value::NULL.is_nullish());
        assert!(!Value::FALSE.is_nullish());
        assert_eq!(Value::bool(true).as_bool(), Some(true));
        assert!(!Value::UNDEFINED.is_number());
        assert!(!Value::HOLE.is_object());
    }
}
