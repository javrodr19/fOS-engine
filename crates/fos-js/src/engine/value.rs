//! JavaScript Values
//!
//! Runtime value representation.

use std::fmt;

/// JavaScript runtime value
#[derive(Clone)]
pub enum JsVal {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Box<str>),
    Object(u32),      // Index into object pool
    Function(u32),    // Index into function pool
    Array(u32),       // Index into array pool
}

impl Default for JsVal {
    fn default() -> Self { JsVal::Undefined }
}

impl fmt::Debug for JsVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsVal::Undefined => write!(f, "undefined"),
            JsVal::Null => write!(f, "null"),
            JsVal::Bool(b) => write!(f, "{}", b),
            JsVal::Number(n) => write!(f, "{}", n),
            JsVal::String(s) => write!(f, "\"{}\"", s),
            JsVal::Object(id) => write!(f, "[Object:{}]", id),
            JsVal::Function(id) => write!(f, "[Function:{}]", id),
            JsVal::Array(id) => write!(f, "[Array:{}]", id),
        }
    }
}

impl JsVal {
    pub fn is_truthy(&self) -> bool {
        match self {
            JsVal::Undefined | JsVal::Null => false,
            JsVal::Bool(b) => *b,
            JsVal::Number(n) => *n != 0.0 && !n.is_nan(),
            JsVal::String(s) => !s.is_empty(),
            JsVal::Object(_) | JsVal::Function(_) | JsVal::Array(_) => true,
        }
    }
    
    pub fn type_of(&self) -> &'static str {
        match self {
            JsVal::Undefined => "undefined",
            JsVal::Null => "object",
            JsVal::Bool(_) => "boolean",
            JsVal::Number(_) => "number",
            JsVal::String(_) => "string",
            JsVal::Object(_) | JsVal::Array(_) => "object",
            JsVal::Function(_) => "function",
        }
    }
    
    pub fn to_number(&self) -> f64 {
        match self {
            JsVal::Undefined => f64::NAN,
            JsVal::Null => 0.0,
            JsVal::Bool(true) => 1.0,
            JsVal::Bool(false) => 0.0,
            JsVal::Number(n) => *n,
            JsVal::String(s) => s.parse().unwrap_or(f64::NAN),
            _ => f64::NAN,
        }
    }
    
    pub fn to_string_val(&self) -> String {
        match self {
            JsVal::Undefined => "undefined".into(),
            JsVal::Null => "null".into(),
            JsVal::Bool(b) => b.to_string(),
            JsVal::Number(n) => n.to_string(),
            JsVal::String(s) => s.to_string(),
            JsVal::Object(_) => "[object Object]".into(),
            JsVal::Function(_) => "[Function]".into(),
            JsVal::Array(_) => "[Array]".into(),
        }
    }
}
