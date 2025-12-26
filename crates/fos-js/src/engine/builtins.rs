//! Built-in Objects
//!
//! JavaScript standard library objects.

use super::value::JsVal;
use super::object::JsObject;

/// Install console built-in
pub fn create_console() -> JsObject {
    let mut console = JsObject::new();
    // Methods would be implemented as native functions
    console
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

/// Install JSON built-in
pub fn create_json() -> JsObject { JsObject::new() }

/// Install Object prototype
pub fn create_object_prototype() -> JsObject { JsObject::new() }

/// Install Array prototype
pub fn create_array_prototype() -> JsObject { JsObject::new() }

/// Install String prototype
pub fn create_string_prototype() -> JsObject { JsObject::new() }

/// Install Number prototype
pub fn create_number_prototype() -> JsObject { JsObject::new() }

/// Install global object
pub fn create_global() -> JsObject {
    let mut global = JsObject::new();
    global.set("undefined", JsVal::Undefined);
    global.set("NaN", JsVal::Number(f64::NAN));
    global.set("Infinity", JsVal::Number(f64::INFINITY));
    global
}
