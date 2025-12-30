//! JavaScript Values - NaN-boxed with backward-compatible API
//!
//! 8-byte values using IEEE-754 NaN space. API compatible with old enum.

use std::fmt;
use std::hash::{Hash, Hasher};

// NaN-boxing constants
const QNAN: u64 = 0x7FFC_0000_0000_0000;
const SIGN_BIT: u64 = 0x8000_0000_0000_0000;
const TAG_MASK: u64 = 0x0003_0000_0000_0000;
const VAL_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;

const TAG_UNDEF: u64 = 0x0000_0000_0000_0000;
const TAG_NULL: u64 = 0x0001_0000_0000_0000;
const TAG_BOOL: u64 = 0x0002_0000_0000_0000;
const TAG_STRING: u64 = 0x0003_0000_0000_0000;

const TAG_OBJECT: u64 = SIGN_BIT | QNAN | 0x0000_0000_0000_0000;
const TAG_FUNCTION: u64 = SIGN_BIT | QNAN | 0x0001_0000_0000_0000;
const TAG_ARRAY: u64 = SIGN_BIT | QNAN | 0x0002_0000_0000_0000;

/// NaN-boxed JavaScript value (8 bytes)
/// Compatible with old enum API via associated functions
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct JsVal(u64);

/// Enum for pattern matching (mirrors old API)
#[derive(Debug, Clone, PartialEq)]
pub enum JsValKind {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Box<str>),
    Object(u32),
    Function(u32),
    Array(u32),
}

// === Backward-compatible constructors (capital case like old enum) ===
#[allow(non_snake_case)]
impl JsVal {
    pub const Undefined: Self = JsVal(QNAN | TAG_UNDEF);
    pub const Null: Self = JsVal(QNAN | TAG_NULL);
    
    #[inline(always)]
    pub fn Number(n: f64) -> Self { JsVal(n.to_bits()) }
    
    #[inline(always)]
    pub fn Bool(b: bool) -> Self { 
        if b { JsVal(QNAN | TAG_BOOL | 1) } else { JsVal(QNAN | TAG_BOOL | 0) }
    }
    
    #[inline(always)]
    pub fn String(s: Box<str>) -> Self {
        // Store string in thread-local pool and return ID
        STRING_POOL.with(|pool| {
            let mut pool = pool.borrow_mut();
            let id = pool.intern_boxed(s);
            JsVal(QNAN | TAG_STRING | id as u64)
        })
    }
    
    #[inline(always)]
    pub fn Object(id: u32) -> Self { JsVal(TAG_OBJECT | id as u64) }
    
    #[inline(always)]
    pub fn Function(id: u32) -> Self { JsVal(TAG_FUNCTION | id as u64) }
    
    #[inline(always)]
    pub fn Array(id: u32) -> Self { JsVal(TAG_ARRAY | id as u64) }
}

// Thread-local string pool
use std::cell::RefCell;
thread_local! {
    static STRING_POOL: RefCell<StringPool> = RefCell::new(StringPool::new());
}

/// String interning pool
#[derive(Default)]
pub struct StringPool {
    strings: Vec<Box<str>>,
}

impl StringPool {
    pub fn new() -> Self { Self { strings: Vec::new() } }
    
    pub fn intern(&mut self, s: &str) -> u32 {
        for (i, existing) in self.strings.iter().enumerate() {
            if &**existing == s { return i as u32; }
        }
        let id = self.strings.len() as u32;
        self.strings.push(s.into());
        id
    }
    
    pub fn intern_boxed(&mut self, s: Box<str>) -> u32 {
        for (i, existing) in self.strings.iter().enumerate() {
            if &**existing == &*s { return i as u32; }
        }
        let id = self.strings.len() as u32;
        self.strings.push(s);
        id
    }
    
    pub fn get(&self, id: u32) -> Option<&str> {
        self.strings.get(id as usize).map(|s| &**s)
    }
}

impl JsVal {
    // Type checks
    #[inline(always)]
    fn is_boxed(&self) -> bool { (self.0 & QNAN) == QNAN }
    
    #[inline(always)]
    pub fn is_number(&self) -> bool { !self.is_boxed() || self.0 == f64::NAN.to_bits() }
    
    #[inline(always)]
    pub fn is_undefined(&self) -> bool { self.0 == Self::Undefined.0 }
    
    #[inline(always)]
    pub fn is_null(&self) -> bool { self.0 == Self::Null.0 }
    
    #[inline(always)]
    pub fn is_bool(&self) -> bool { 
        self.is_boxed() && (self.0 & TAG_MASK) == TAG_BOOL && (self.0 & SIGN_BIT) == 0
    }
    
    #[inline(always)]
    pub fn is_string(&self) -> bool { 
        self.is_boxed() && (self.0 & TAG_MASK) == TAG_STRING && (self.0 & SIGN_BIT) == 0
    }
    
    #[inline(always)]
    pub fn is_object(&self) -> bool { (self.0 & (SIGN_BIT | QNAN | TAG_MASK)) == TAG_OBJECT }
    
    #[inline(always)]
    pub fn is_function(&self) -> bool { (self.0 & (SIGN_BIT | QNAN | TAG_MASK)) == TAG_FUNCTION }
    
    #[inline(always)]
    pub fn is_array(&self) -> bool { (self.0 & (SIGN_BIT | QNAN | TAG_MASK)) == TAG_ARRAY }
    
    // Value extraction
    #[inline(always)]
    pub fn as_number(&self) -> Option<f64> {
        if self.is_number() { Some(f64::from_bits(self.0)) } else { None }
    }
    
    #[inline(always)]
    pub fn as_bool(&self) -> Option<bool> {
        if self.is_bool() { Some((self.0 & 1) != 0) } else { None }
    }
    
    /// Get string value (looks up in pool)
    pub fn as_string(&self) -> Option<Box<str>> {
        if self.is_string() {
            let id = (self.0 & VAL_MASK) as u32;
            STRING_POOL.with(|pool| {
                pool.borrow().get(id).map(|s| s.into())
            })
        } else { None }
    }
    
    #[inline(always)]
    pub fn as_object_id(&self) -> Option<u32> {
        if self.is_object() { Some((self.0 & VAL_MASK) as u32) } else { None }
    }
    
    #[inline(always)]
    pub fn as_function_id(&self) -> Option<u32> {
        if self.is_function() { Some((self.0 & VAL_MASK) as u32) } else { None }
    }
    
    #[inline(always)]
    pub fn as_array_id(&self) -> Option<u32> {
        if self.is_array() { Some((self.0 & VAL_MASK) as u32) } else { None }
    }
    
    /// Convert to enum for pattern matching
    pub fn kind(&self) -> JsValKind {
        if self.is_undefined() { JsValKind::Undefined }
        else if self.is_null() { JsValKind::Null }
        else if let Some(b) = self.as_bool() { JsValKind::Bool(b) }
        else if let Some(n) = self.as_number() { JsValKind::Number(n) }
        else if let Some(s) = self.as_string() { JsValKind::String(s) }
        else if let Some(id) = self.as_object_id() { JsValKind::Object(id) }
        else if let Some(id) = self.as_function_id() { JsValKind::Function(id) }
        else if let Some(id) = self.as_array_id() { JsValKind::Array(id) }
        else { JsValKind::Undefined }
    }
    
    // Runtime operations
    #[inline]
    pub fn is_truthy(&self) -> bool {
        if self.is_undefined() || self.is_null() { return false; }
        if let Some(b) = self.as_bool() { return b; }
        if let Some(n) = self.as_number() { return n != 0.0 && !n.is_nan(); }
        if self.is_string() { return self.as_string().map(|s| !s.is_empty()).unwrap_or(false); }
        true
    }
    
    #[inline]
    pub fn type_of(&self) -> &'static str {
        if self.is_undefined() { "undefined" }
        else if self.is_null() { "object" }
        else if self.is_bool() { "boolean" }
        else if self.is_number() { "number" }
        else if self.is_string() { "string" }
        else if self.is_function() { "function" }
        else { "object" }
    }
    
    #[inline]
    pub fn to_number(&self) -> f64 {
        if let Some(n) = self.as_number() { n }
        else if self.is_undefined() { f64::NAN }
        else if self.is_null() { 0.0 }
        else if let Some(b) = self.as_bool() { if b { 1.0 } else { 0.0 } }
        else { f64::NAN }
    }
    
    pub fn to_string_val(&self) -> String {
        match self.kind() {
            JsValKind::Undefined => "undefined".into(),
            JsValKind::Null => "null".into(),
            JsValKind::Bool(b) => b.to_string(),
            JsValKind::Number(n) => n.to_string(),
            JsValKind::String(s) => s.to_string(),
            JsValKind::Object(_) => "[object Object]".into(),
            JsValKind::Function(_) => "[Function]".into(),
            JsValKind::Array(_) => "[Array]".into(),
        }
    }
}

impl Default for JsVal {
    fn default() -> Self { Self::Undefined }
}

impl PartialEq for JsVal {
    fn eq(&self, other: &Self) -> bool {
        if self.is_number() && other.is_number() {
            let a = f64::from_bits(self.0);
            let b = f64::from_bits(other.0);
            if a.is_nan() && b.is_nan() { return false; }
            a == b
        } else {
            self.0 == other.0
        }
    }
}

impl Eq for JsVal {}

impl Hash for JsVal {
    fn hash<H: Hasher>(&self, state: &mut H) { self.0.hash(state); }
}

impl fmt::Debug for JsVal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            JsValKind::Undefined => write!(f, "undefined"),
            JsValKind::Null => write!(f, "null"),
            JsValKind::Bool(b) => write!(f, "{}", b),
            JsValKind::Number(n) => write!(f, "{}", n),
            JsValKind::String(s) => write!(f, "\"{}\"", s),
            JsValKind::Object(id) => write!(f, "[Object:{}]", id),
            JsValKind::Function(id) => write!(f, "[Function:{}]", id),
            JsValKind::Array(id) => write!(f, "[Array:{}]", id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_size() { assert_eq!(std::mem::size_of::<JsVal>(), 8); }
    
    #[test]
    fn test_undefined_null() {
        assert!(JsVal::Undefined.is_undefined());
        assert!(JsVal::Null.is_null());
    }
    
    #[test]
    fn test_bool() {
        assert!(JsVal::Bool(true).is_truthy());
        assert!(!JsVal::Bool(false).is_truthy());
    }
    
    #[test]
    fn test_number() {
        let v = JsVal::Number(42.5);
        assert!(v.is_number());
        assert_eq!(v.as_number(), Some(42.5));
    }
    
    #[test]
    fn test_string() {
        let v = JsVal::String("hello".into());
        assert!(v.is_string());
        assert_eq!(v.as_string(), Some("hello".into()));
    }
    
    #[test]
    fn test_objects() {
        let obj = JsVal::Object(123);
        assert!(obj.is_object());
        assert_eq!(obj.as_object_id(), Some(123));
        
        let func = JsVal::Function(456);
        assert!(func.is_function());
        assert_eq!(func.as_function_id(), Some(456));
    }
}
