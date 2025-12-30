//! fos-engine Integration
//!
//! Local implementations of utilities inspired by fos-engine crate,
//! avoiding cyclic dependencies. These are self-contained versions
//! of StringInterner, BumpAllocator, Fixed16, and Cow for the JS engine.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// ============================================================================
// String Interning
// ============================================================================

/// Interned string reference
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct InternedString {
    id: u32,
}

impl InternedString {
    pub fn id(&self) -> u32 { self.id }
}

impl std::fmt::Debug for InternedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Interned({})", self.id)
    }
}

/// String interner for deduplicating strings
#[derive(Debug, Default)]
pub struct StringInterner {
    strings: Vec<Arc<str>>,
    lookup: HashMap<Arc<str>, u32>,
}

impl StringInterner {
    pub fn new() -> Self { Self::default() }
    
    pub fn intern(&mut self, s: &str) -> InternedString {
        if let Some(&id) = self.lookup.get(s) {
            return InternedString { id };
        }
        let id = self.strings.len() as u32;
        let arc: Arc<str> = s.into();
        self.strings.push(arc.clone());
        self.lookup.insert(arc, id);
        InternedString { id }
    }
    
    pub fn get(&self, interned: &InternedString) -> Option<&str> {
        self.strings.get(interned.id as usize).map(|s| s.as_ref())
    }
    
    pub fn len(&self) -> usize { self.strings.len() }
    pub fn is_empty(&self) -> bool { self.strings.is_empty() }
}

// ============================================================================
// JavaScript String Interner (with pre-interned keywords)
// ============================================================================

/// JavaScript String Interner with pre-interned keywords
pub struct JsInterner {
    interner: Mutex<StringInterner>,
}

impl std::fmt::Debug for JsInterner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JsInterner")
            .field("len", &self.len())
            .finish()
    }
}

impl Default for JsInterner {
    fn default() -> Self { Self::new() }
}

impl JsInterner {
    pub fn new() -> Self {
        let mut interner = StringInterner::new();
        for keyword in JS_KEYWORDS { interner.intern(keyword); }
        for ident in COMMON_IDENTIFIERS { interner.intern(ident); }
        Self { interner: Mutex::new(interner) }
    }
    
    pub fn intern(&self, s: &str) -> InternedString {
        self.interner.lock().unwrap().intern(s)
    }
    
    pub fn get(&self, interned: &InternedString) -> Option<String> {
        self.interner.lock().unwrap().get(interned).map(|s| s.to_string())
    }
    
    pub fn len(&self) -> usize { self.interner.lock().unwrap().len() }
    pub fn is_empty(&self) -> bool { self.interner.lock().unwrap().is_empty() }
}

const JS_KEYWORDS: &[&str] = &[
    "await", "break", "case", "catch", "class", "const", "continue",
    "debugger", "default", "delete", "do", "else", "export", "extends",
    "false", "finally", "for", "function", "if", "import", "in",
    "instanceof", "let", "new", "null", "of", "return", "super",
    "switch", "this", "throw", "true", "try", "typeof", "undefined",
    "var", "void", "while", "with", "yield", "async",
];

const COMMON_IDENTIFIERS: &[&str] = &[
    "length", "push", "pop", "map", "filter", "reduce", "forEach",
    "console", "log", "document", "window", "Math", "JSON", "Object",
    "Array", "String", "Number", "Boolean", "Date", "Error", "Promise",
];

// ============================================================================
// Fixed-Point Arithmetic (16.16 format)
// ============================================================================

/// 16.16 fixed-point number for deterministic math
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Fixed16(i32);

impl Fixed16 {
    pub const FRAC_BITS: u32 = 16;
    pub const SCALE: i32 = 1 << Self::FRAC_BITS;
    pub const ZERO: Fixed16 = Fixed16(0);
    pub const ONE: Fixed16 = Fixed16(Self::SCALE);
    
    pub const fn from_bits(bits: i32) -> Self { Self(bits) }
    pub const fn to_bits(self) -> i32 { self.0 }
    pub const fn from_i32(value: i32) -> Self { Self(value << Self::FRAC_BITS) }
    pub const fn to_i32(self) -> i32 { self.0 >> Self::FRAC_BITS }
    
    pub fn from_f32(value: f32) -> Self { Self((value * Self::SCALE as f32) as i32) }
    pub fn to_f32(self) -> f32 { self.0 as f32 / Self::SCALE as f32 }
    pub fn from_f64(value: f64) -> Self { Self((value * Self::SCALE as f64) as i32) }
    pub fn to_f64(self) -> f64 { self.0 as f64 / Self::SCALE as f64 }
}

impl std::ops::Add for Fixed16 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self { Self(self.0.saturating_add(rhs.0)) }
}

impl std::ops::Sub for Fixed16 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self { Self(self.0.saturating_sub(rhs.0)) }
}

impl std::ops::Mul for Fixed16 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self(((self.0 as i64 * rhs.0 as i64) >> Self::FRAC_BITS) as i32)
    }
}

impl std::ops::Div for Fixed16 {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 { return Self(0); }
        Self((((self.0 as i64) << Self::FRAC_BITS) / rhs.0 as i64) as i32)
    }
}

/// Wrapper for JS deterministic fixed-point
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct JsFixed(Fixed16);

impl JsFixed {
    pub fn from_f64(value: f64) -> Self { Self(Fixed16::from_f64(value)) }
    pub fn to_f64(self) -> f64 { self.0.to_f64() }
    pub fn add(self, other: Self) -> Self { Self(self.0 + other.0) }
    pub fn sub(self, other: Self) -> Self { Self(self.0 - other.0) }
    pub fn mul(self, other: Self) -> Self { Self(self.0 * other.0) }
    pub fn div(self, other: Self) -> Self { Self(self.0 / other.0) }
}

// ============================================================================
// Copy-on-Write
// ============================================================================

/// Copy-on-Write wrapper
#[derive(Debug)]
pub struct Cow<T> {
    inner: Arc<T>,
}

impl<T: Clone> Cow<T> {
    pub fn new(value: T) -> Self { Self { inner: Arc::new(value) } }
    pub fn get(&self) -> &T { &self.inner }
    pub fn get_mut(&mut self) -> &mut T { Arc::make_mut(&mut self.inner) }
    pub fn is_unique(&self) -> bool { Arc::strong_count(&self.inner) == 1 }
}

impl<T> Clone for Cow<T> {
    fn clone(&self) -> Self { Self { inner: Arc::clone(&self.inner) } }
}

impl<T> std::ops::Deref for Cow<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target { &self.inner }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_string_interner() {
        let mut interner = StringInterner::new();
        let id1 = interner.intern("hello");
        let id2 = interner.intern("hello");
        let id3 = interner.intern("world");
        assert_eq!(id1.id(), id2.id());
        assert_ne!(id1.id(), id3.id());
    }
    
    #[test]
    fn test_js_interner() {
        let interner = JsInterner::new();
        let if_id = interner.intern("if");
        let if_id2 = interner.intern("if");
        assert_eq!(if_id.id(), if_id2.id());
    }
    
    #[test]
    fn test_fixed16() {
        let a = Fixed16::from_f64(10.5);
        let b = Fixed16::from_f64(2.25);
        let c = a + b;
        assert!((c.to_f64() - 12.75).abs() < 0.01);
    }
    
    #[test]
    fn test_cow() {
        let cow1: Cow<Vec<i32>> = Cow::new(vec![1, 2, 3]);
        let mut cow2 = cow1.clone();
        assert!(!cow1.is_unique());
        cow2.get_mut().push(4);
        assert!(cow1.is_unique());
        assert_eq!(cow1.get().len(), 3);
        assert_eq!(cow2.get().len(), 4);
    }
}
