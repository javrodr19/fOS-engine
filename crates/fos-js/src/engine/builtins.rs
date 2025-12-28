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
    
    pub fn replace_all(s: &str, from: &str, to: &str) -> JsVal {
        JsVal::String(s.replace(from, to).into())
    }
    
    pub fn pad_start(s: &str, target_length: usize, pad_string: &str) -> JsVal {
        let current_len = s.chars().count();
        if current_len >= target_length {
            return JsVal::String(s.into());
        }
        let pad_len = target_length - current_len;
        let mut padding = String::new();
        while padding.chars().count() < pad_len {
            padding.push_str(pad_string);
        }
        let padding: String = padding.chars().take(pad_len).collect();
        JsVal::String(format!("{}{}", padding, s).into())
    }
    
    pub fn pad_end(s: &str, target_length: usize, pad_string: &str) -> JsVal {
        let current_len = s.chars().count();
        if current_len >= target_length {
            return JsVal::String(s.into());
        }
        let pad_len = target_length - current_len;
        let mut padding = String::new();
        while padding.chars().count() < pad_len {
            padding.push_str(pad_string);
        }
        let padding: String = padding.chars().take(pad_len).collect();
        JsVal::String(format!("{}{}", s, padding).into())
    }
    
    pub fn repeat(s: &str, count: usize) -> JsVal {
        JsVal::String(s.repeat(count).into())
    }
    
    pub fn last_index_of(s: &str, search: &str) -> JsVal {
        JsVal::Number(s.rfind(search).map(|i| i as f64).unwrap_or(-1.0))
    }
    
    pub fn char_code_at(s: &str, index: usize) -> JsVal {
        s.chars().nth(index)
            .map(|c| JsVal::Number(c as u32 as f64))
            .unwrap_or(JsVal::Number(f64::NAN))
    }
    
    pub fn concat(strings: &[&str]) -> JsVal {
        JsVal::String(strings.join("").into())
    }
    
    pub fn slice(s: &str, start: i32, end: Option<i32>) -> JsVal {
        let len = s.chars().count() as i32;
        let start = if start < 0 { (len + start).max(0) as usize } else { start as usize };
        let end = end.map(|e| if e < 0 { (len + e).max(0) as usize } else { e as usize }).unwrap_or(len as usize);
        let result: String = s.chars().skip(start).take(end.saturating_sub(start)).collect();
        JsVal::String(result.into())
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

/// Object built-in methods
pub struct ObjectMethods;

impl ObjectMethods {
    /// Object.keys(obj) - get own enumerable property names
    pub fn keys(obj: &JsObject) -> Vec<JsVal> {
        obj.keys().map(|k| JsVal::String(k.into())).collect()
    }
    
    /// Object.values(obj) - get own enumerable property values
    pub fn values(obj: &JsObject) -> Vec<JsVal> {
        obj.keys().filter_map(|k| obj.get(k).cloned()).collect()
    }
    
    /// Object.entries(obj) - get [key, value] pairs
    pub fn entries(obj: &JsObject) -> Vec<(JsVal, JsVal)> {
        obj.keys()
            .filter_map(|k| obj.get(k).cloned().map(|v| (JsVal::String(k.into()), v)))
            .collect()
    }
    
    /// Object.assign(target, ...sources) - copy properties
    pub fn assign(target: &mut JsObject, source: &JsObject) {
        for key in source.keys() {
            if let Some(val) = source.get(key) {
                target.set(key, val.clone());
            }
        }
    }
    
    /// Object.hasOwn(obj, prop) - check if has own property
    pub fn has_own(obj: &JsObject, prop: &str) -> bool {
        obj.has(prop)
    }
}

/// Function built-in methods
pub struct FunctionMethods;

impl FunctionMethods {
    /// Function.prototype.bind context
    pub fn bind_context(func_id: u32, this_arg: JsVal) -> BoundFunction {
        BoundFunction { func_id, this_arg, bound_args: Vec::new() }
    }
    
    /// Function.prototype.bind with args
    pub fn bind_with_args(func_id: u32, this_arg: JsVal, args: Vec<JsVal>) -> BoundFunction {
        BoundFunction { func_id, this_arg, bound_args: args }
    }
}

/// Bound function representation
#[derive(Debug, Clone)]
pub struct BoundFunction {
    pub func_id: u32,
    pub this_arg: JsVal,
    pub bound_args: Vec<JsVal>,
}

/// Extended Array methods
impl ArrayMethods {
    /// Array.isArray(value)
    pub fn is_array(value: &JsVal) -> bool {
        matches!(value, JsVal::Array(_))
    }
    
    /// Array.from(arrayLike) - simplified
    pub fn from(len: usize) -> JsArray {
        let mut arr = JsArray::new();
        for _ in 0..len {
            arr.push(JsVal::Undefined);
        }
        arr
    }
    
    /// Array.of(...items)
    pub fn of(items: Vec<JsVal>) -> JsArray {
        let mut arr = JsArray::new();
        for item in items {
            arr.push(item);
        }
        arr
    }
    
    /// Array.prototype.flat(depth) - simplified (depth=1)
    pub fn flat(arr: &JsArray) -> JsArray {
        let mut result = JsArray::new();
        for i in 0..arr.len() {
            let val = arr.get(i);
            // Only flatten one level
            result.push(val);
        }
        result
    }
    
    /// Array.prototype.find
    pub fn find(arr: &JsArray, predicate: impl Fn(&JsVal) -> bool) -> JsVal {
        for i in 0..arr.len() {
            let val = arr.get(i);
            if predicate(&val) {
                return val;
            }
        }
        JsVal::Undefined
    }
    
    /// Array.prototype.findIndex
    pub fn find_index(arr: &JsArray, predicate: impl Fn(&JsVal) -> bool) -> JsVal {
        for i in 0..arr.len() {
            let val = arr.get(i);
            if predicate(&val) {
                return JsVal::Number(i as f64);
            }
        }
        JsVal::Number(-1.0)
    }
    
    /// Array.prototype.every
    pub fn every(arr: &JsArray, predicate: impl Fn(&JsVal) -> bool) -> bool {
        for i in 0..arr.len() {
            if !predicate(&arr.get(i)) {
                return false;
            }
        }
        true
    }
    
    /// Array.prototype.some
    pub fn some(arr: &JsArray, predicate: impl Fn(&JsVal) -> bool) -> bool {
        for i in 0..arr.len() {
            if predicate(&arr.get(i)) {
                return true;
            }
        }
        false
    }
    
    /// Array.prototype.fill
    pub fn fill(arr: &mut JsArray, value: JsVal) {
        for i in 0..arr.len() {
            arr.set(i, value.clone());
        }
    }
    
    /// Array.prototype.concat
    pub fn concat(arr1: &JsArray, arr2: &JsArray) -> JsArray {
        let mut result = JsArray::new();
        for i in 0..arr1.len() {
            result.push(arr1.get(i));
        }
        for i in 0..arr2.len() {
            result.push(arr2.get(i));
        }
        result
    }
    
    /// Array.prototype.slice
    pub fn slice(arr: &JsArray, start: usize, end: usize) -> JsArray {
        let mut result = JsArray::new();
        let end = end.min(arr.len());
        for i in start..end {
            result.push(arr.get(i));
        }
        result
    }
    
    /// Array.prototype.splice
    pub fn splice(arr: &mut JsArray, start: usize, delete_count: usize, items: Vec<JsVal>) -> JsArray {
        let mut deleted = JsArray::new();
        let len = arr.len();
        let start = start.min(len);
        let end = (start + delete_count).min(len);
        
        // Collect deleted items
        for i in start..end {
            deleted.push(arr.get(i));
        }
        
        // For simplicity, just return deleted items (full impl would modify arr)
        deleted
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
