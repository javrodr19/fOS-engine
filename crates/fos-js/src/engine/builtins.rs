//! Built-in Objects and Methods
//!
//! JavaScript standard library objects and native functions.

use super::value::JsVal;
use super::object::{JsObject, JsArray};
use std::collections::HashMap;

/// Native function type
pub type NativeFn = fn(&[JsVal]) -> JsVal;

/// Built-in registry for native functions
#[derive(Default)]
pub struct BuiltinRegistry {
    pub functions: HashMap<&'static str, NativeFn>,
}

impl BuiltinRegistry {
    pub fn new() -> Self {
        let mut reg = Self { functions: HashMap::new() };
        reg.register_all();
        reg
    }
    
    fn register_all(&mut self) {
        // Math functions
        self.functions.insert("Math.abs", math_abs);
        self.functions.insert("Math.floor", math_floor);
        self.functions.insert("Math.ceil", math_ceil);
        self.functions.insert("Math.round", math_round);
        self.functions.insert("Math.sqrt", math_sqrt);
        self.functions.insert("Math.pow", math_pow);
        self.functions.insert("Math.min", math_min);
        self.functions.insert("Math.max", math_max);
        self.functions.insert("Math.random", math_random);
        self.functions.insert("Math.sin", math_sin);
        self.functions.insert("Math.cos", math_cos);
        self.functions.insert("Math.tan", math_tan);
        
        // Number functions
        self.functions.insert("Number.isNaN", number_is_nan);
        self.functions.insert("Number.isFinite", number_is_finite);
        self.functions.insert("Number.parseInt", number_parse_int);
        self.functions.insert("Number.parseFloat", number_parse_float);
        
        // String functions
        self.functions.insert("String.fromCharCode", string_from_char_code);
    }
    
    pub fn call(&self, name: &str, args: &[JsVal]) -> Option<JsVal> {
        self.functions.get(name).map(|f| f(args))
    }
}

// ============================================================================
// Math functions
// ============================================================================

fn math_abs(args: &[JsVal]) -> JsVal {
    JsVal::Number(args.first().map(|v| v.to_number().abs()).unwrap_or(f64::NAN))
}

fn math_floor(args: &[JsVal]) -> JsVal {
    JsVal::Number(args.first().map(|v| v.to_number().floor()).unwrap_or(f64::NAN))
}

fn math_ceil(args: &[JsVal]) -> JsVal {
    JsVal::Number(args.first().map(|v| v.to_number().ceil()).unwrap_or(f64::NAN))
}

fn math_round(args: &[JsVal]) -> JsVal {
    JsVal::Number(args.first().map(|v| v.to_number().round()).unwrap_or(f64::NAN))
}

fn math_sqrt(args: &[JsVal]) -> JsVal {
    JsVal::Number(args.first().map(|v| v.to_number().sqrt()).unwrap_or(f64::NAN))
}

fn math_pow(args: &[JsVal]) -> JsVal {
    let base = args.first().map(|v| v.to_number()).unwrap_or(0.0);
    let exp = args.get(1).map(|v| v.to_number()).unwrap_or(0.0);
    JsVal::Number(base.powf(exp))
}

fn math_min(args: &[JsVal]) -> JsVal {
    if args.is_empty() { return JsVal::Number(f64::INFINITY); }
    let min = args.iter().map(|v| v.to_number()).fold(f64::INFINITY, f64::min);
    JsVal::Number(min)
}

fn math_max(args: &[JsVal]) -> JsVal {
    if args.is_empty() { return JsVal::Number(f64::NEG_INFINITY); }
    let max = args.iter().map(|v| v.to_number()).fold(f64::NEG_INFINITY, f64::max);
    JsVal::Number(max)
}

fn math_random(_args: &[JsVal]) -> JsVal {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos() as f64;
    JsVal::Number((nanos / 1_000_000_000.0) % 1.0)
}

fn math_sin(args: &[JsVal]) -> JsVal {
    JsVal::Number(args.first().map(|v| v.to_number().sin()).unwrap_or(f64::NAN))
}

fn math_cos(args: &[JsVal]) -> JsVal {
    JsVal::Number(args.first().map(|v| v.to_number().cos()).unwrap_or(f64::NAN))
}

fn math_tan(args: &[JsVal]) -> JsVal {
    JsVal::Number(args.first().map(|v| v.to_number().tan()).unwrap_or(f64::NAN))
}

// ============================================================================
// Number functions
// ============================================================================

fn number_is_nan(args: &[JsVal]) -> JsVal {
    JsVal::Bool(args.first().map(|v| v.to_number().is_nan()).unwrap_or(false))
}

fn number_is_finite(args: &[JsVal]) -> JsVal {
    JsVal::Bool(args.first().map(|v| v.to_number().is_finite()).unwrap_or(false))
}

fn number_parse_int(args: &[JsVal]) -> JsVal {
    let s = args.first().map(|v| v.to_string_val()).unwrap_or_default();
    let radix = args.get(1).map(|v| v.to_number() as u32).unwrap_or(10);
    match i64::from_str_radix(s.trim(), radix) {
        Ok(n) => JsVal::Number(n as f64),
        Err(_) => JsVal::Number(f64::NAN),
    }
}

fn number_parse_float(args: &[JsVal]) -> JsVal {
    let s = args.first().map(|v| v.to_string_val()).unwrap_or_default();
    match s.trim().parse::<f64>() {
        Ok(n) => JsVal::Number(n),
        Err(_) => JsVal::Number(f64::NAN),
    }
}

fn string_from_char_code(args: &[JsVal]) -> JsVal {
    let s: String = args.iter()
        .map(|v| v.to_number() as u32)
        .filter_map(char::from_u32)
        .collect();
    JsVal::String(s.into())
}

// ============================================================================
// Console functions (for debugging)
// ============================================================================

pub fn console_log(args: &[JsVal]) -> JsVal {
    let msg: Vec<String> = args.iter().map(|v| v.to_string_val()).collect();
    println!("{}", msg.join(" "));
    JsVal::Undefined
}

// ============================================================================
// Object creation helpers
// ============================================================================

/// Install console built-in
pub fn create_console() -> JsObject {
    JsObject::new()
}

/// Install Math built-in
pub fn create_math() -> JsObject {
    let mut math = JsObject::new();
    math.set("PI", JsVal::Number(std::f64::consts::PI));
    math.set("E", JsVal::Number(std::f64::consts::E));
    math.set("LN2", JsVal::Number(std::f64::consts::LN_2));
    math.set("LN10", JsVal::Number(std::f64::consts::LN_10));
    math.set("LOG2E", JsVal::Number(std::f64::consts::LOG2_E));
    math.set("LOG10E", JsVal::Number(std::f64::consts::LOG10_E));
    math.set("SQRT2", JsVal::Number(std::f64::consts::SQRT_2));
    math
}

/// Install global object
pub fn create_global() -> JsObject {
    let mut global = JsObject::new();
    global.set("undefined", JsVal::Undefined);
    global.set("NaN", JsVal::Number(f64::NAN));
    global.set("Infinity", JsVal::Number(f64::INFINITY));
    global
}

// ============================================================================
// String instance methods
// ============================================================================

/// String prototype methods that operate on a string value
pub struct StringMethods;

impl StringMethods {
    pub fn to_upper_case(s: &str) -> JsVal {
        JsVal::String(s.to_uppercase().into())
    }
    
    pub fn to_lower_case(s: &str) -> JsVal {
        JsVal::String(s.to_lowercase().into())
    }
    
    pub fn length(s: &str) -> JsVal {
        JsVal::Number(s.chars().count() as f64)
    }
    
    pub fn char_at(s: &str, index: usize) -> JsVal {
        s.chars().nth(index)
            .map(|c| JsVal::String(c.to_string().into()))
            .unwrap_or(JsVal::String("".into()))
    }
    
    pub fn index_of(s: &str, search: &str) -> JsVal {
        JsVal::Number(s.find(search).map(|i| i as f64).unwrap_or(-1.0))
    }
    
    pub fn substring(s: &str, start: usize, end: Option<usize>) -> JsVal {
        let end = end.unwrap_or(s.len());
        let sub: String = s.chars().skip(start).take(end - start).collect();
        JsVal::String(sub.into())
    }
    
    pub fn split(s: &str, separator: &str) -> Vec<JsVal> {
        s.split(separator).map(|p| JsVal::String(p.to_string().into())).collect()
    }
    
    pub fn trim(s: &str) -> JsVal {
        JsVal::String(s.trim().into())
    }
    
    pub fn starts_with(s: &str, prefix: &str) -> JsVal {
        JsVal::Bool(s.starts_with(prefix))
    }
    
    pub fn ends_with(s: &str, suffix: &str) -> JsVal {
        JsVal::Bool(s.ends_with(suffix))
    }
    
    pub fn includes(s: &str, search: &str) -> JsVal {
        JsVal::Bool(s.contains(search))
    }
    
    pub fn replace(s: &str, from: &str, to: &str) -> JsVal {
        JsVal::String(s.replacen(from, to, 1).into())
    }
}

// ============================================================================
// Array instance methods
// ============================================================================

/// Array prototype methods that operate on a JsArray
pub struct ArrayMethods;

impl ArrayMethods {
    pub fn length(arr: &JsArray) -> JsVal {
        JsVal::Number(arr.len() as f64)
    }
    
    pub fn push(arr: &mut JsArray, value: JsVal) -> JsVal {
        arr.push(value);
        JsVal::Number(arr.len() as f64)
    }
    
    pub fn pop(arr: &mut JsArray) -> JsVal {
        arr.pop()
    }
    
    pub fn shift(arr: &mut JsArray) -> JsVal {
        if arr.len() > 0 {
            arr.shift()
        } else {
            JsVal::Undefined
        }
    }
    
    pub fn join(arr: &JsArray, separator: &str) -> JsVal {
        let parts: Vec<String> = (0..arr.len())
            .map(|i| arr.get(i).to_string_val())
            .collect();
        JsVal::String(parts.join(separator).into())
    }
    
    pub fn reverse(arr: &mut JsArray) -> JsVal {
        arr.reverse();
        JsVal::Array(0) // Returns self-reference, simplified
    }
    
    pub fn includes(arr: &JsArray, value: &JsVal) -> JsVal {
        for i in 0..arr.len() {
            if arr.get(i) == *value {
                return JsVal::Bool(true);
            }
        }
        JsVal::Bool(false)
    }
    
    pub fn index_of(arr: &JsArray, value: &JsVal) -> JsVal {
        for i in 0..arr.len() {
            if arr.get(i) == *value {
                return JsVal::Number(i as f64);
            }
        }
        JsVal::Number(-1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_math_functions() {
        assert!(matches!(math_abs(&[JsVal::Number(-5.0)]), JsVal::Number(n) if n == 5.0));
        assert!(matches!(math_floor(&[JsVal::Number(5.7)]), JsVal::Number(n) if n == 5.0));
        assert!(matches!(math_ceil(&[JsVal::Number(5.1)]), JsVal::Number(n) if n == 6.0));
        assert!(matches!(math_sqrt(&[JsVal::Number(16.0)]), JsVal::Number(n) if n == 4.0));
    }
    
    #[test]
    fn test_string_methods() {
        assert!(matches!(StringMethods::to_upper_case("hello"), JsVal::String(s) if &*s == "HELLO"));
        assert!(matches!(StringMethods::length("hello"), JsVal::Number(n) if n == 5.0));
        assert!(matches!(StringMethods::index_of("hello", "ll"), JsVal::Number(n) if n == 2.0));
    }
}
