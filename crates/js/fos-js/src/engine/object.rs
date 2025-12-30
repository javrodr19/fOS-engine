//! JavaScript Objects - Optimized
//!
//! Uses SmallVec for stack allocation of small objects.

use super::value::JsVal;

/// Stack-allocated small vector (inline up to N elements)
#[derive(Debug, Clone)]
pub struct SmallVec<T, const N: usize> {
    inline: [Option<T>; N],
    overflow: Option<Vec<T>>,
    len: usize,
}

impl<T: Clone, const N: usize> Default for SmallVec<T, N> {
    fn default() -> Self {
        Self {
            inline: std::array::from_fn(|_| None),
            overflow: None,
            len: 0,
        }
    }
}

impl<T: Clone, const N: usize> SmallVec<T, N> {
    pub fn new() -> Self { Self::default() }
    
    pub fn push(&mut self, val: T) {
        if self.len < N {
            self.inline[self.len] = Some(val);
        } else {
            if self.overflow.is_none() {
                self.overflow = Some(Vec::new());
            }
            self.overflow.as_mut().unwrap().push(val);
        }
        self.len += 1;
    }
    
    pub fn get(&self, idx: usize) -> Option<&T> {
        if idx < N {
            self.inline[idx].as_ref()
        } else {
            self.overflow.as_ref()?.get(idx - N)
        }
    }
    
    pub fn get_mut(&mut self, idx: usize) -> Option<&mut T> {
        if idx < N {
            self.inline[idx].as_mut()
        } else {
            self.overflow.as_mut()?.get_mut(idx - N)
        }
    }
    
    pub fn len(&self) -> usize { self.len }
    pub fn is_empty(&self) -> bool { self.len == 0 }
    
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.inline.iter()
            .take(self.len.min(N))
            .filter_map(|x| x.as_ref())
            .chain(self.overflow.iter().flat_map(|v| v.iter()))
    }
}

/// Property entry for SmallVec storage
#[derive(Debug, Clone)]
struct Property {
    key: Box<str>,
    value: JsVal,
}

/// JavaScript object - uses SmallVec for ≤4 properties (zero heap)
#[derive(Debug, Clone, Default)]
pub struct JsObject {
    properties: SmallVec<Property, 4>,  // Stack-allocated for ≤4 props
    prototype: Option<u32>,
}

impl JsObject {
    pub fn new() -> Self { Self::default() }
    
    pub fn with_prototype(proto: u32) -> Self {
        Self { properties: SmallVec::new(), prototype: Some(proto) }
    }
    
    pub fn get(&self, key: &str) -> Option<&JsVal> {
        for i in 0..self.properties.len() {
            if let Some(prop) = self.properties.get(i) {
                if &*prop.key == key {
                    return Some(&prop.value);
                }
            }
        }
        None
    }
    
    pub fn set(&mut self, key: &str, value: JsVal) {
        // Check if key exists
        for i in 0..self.properties.len() {
            if let Some(prop) = self.properties.get_mut(i) {
                if &*prop.key == key {
                    prop.value = value;
                    return;
                }
            }
        }
        // Add new property
        self.properties.push(Property { key: key.into(), value });
    }
    
    pub fn delete(&mut self, key: &str) -> bool {
        // Note: simplified - doesn't compact, just marks as undefined
        for i in 0..self.properties.len() {
            if let Some(prop) = self.properties.get(i) {
                if &*prop.key == key {
                    if let Some(p) = self.properties.get_mut(i) {
                        p.value = JsVal::Undefined;
                    }
                    return true;
                }
            }
        }
        false
    }
    
    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.properties.iter().map(|p| &*p.key)
    }
    
    pub fn prototype(&self) -> Option<u32> { self.prototype }
    pub fn set_prototype(&mut self, proto: Option<u32>) { self.prototype = proto; }
}

/// JavaScript array - pre-allocated with capacity
#[derive(Debug, Clone, Default)]
pub struct JsArray {
    elements: Vec<JsVal>,
}

impl JsArray {
    pub fn new() -> Self { Self::default() }
    pub fn with_capacity(cap: usize) -> Self { Self { elements: Vec::with_capacity(cap) } }
    
    pub fn push(&mut self, val: JsVal) { self.elements.push(val); }
    pub fn pop(&mut self) -> JsVal { self.elements.pop().unwrap_or(JsVal::Undefined) }
    pub fn get(&self, idx: usize) -> JsVal { self.elements.get(idx).cloned().unwrap_or(JsVal::Undefined) }
    pub fn set(&mut self, idx: usize, val: JsVal) {
        if idx >= self.elements.len() { self.elements.resize(idx + 1, JsVal::Undefined); }
        self.elements[idx] = val;
    }
    pub fn len(&self) -> usize { self.elements.len() }
    pub fn is_empty(&self) -> bool { self.elements.is_empty() }
    
    pub fn shift(&mut self) -> JsVal {
        if self.elements.is_empty() { JsVal::Undefined }
        else { self.elements.remove(0) }
    }
    
    pub fn reverse(&mut self) { self.elements.reverse(); }
}

/// JavaScript function
#[derive(Debug, Clone)]
pub struct JsFunction {
    pub name: Option<Box<str>>,
    pub params: Vec<Box<str>>,
    pub bytecode_id: u32,
}

impl JsFunction {
    pub fn new(name: Option<Box<str>>, params: Vec<Box<str>>, bytecode_id: u32) -> Self {
        Self { name, params, bytecode_id }
    }
}

// === COPY-ON-WRITE ARRAY ===

use std::rc::Rc;
use std::cell::RefCell;

/// Copy-on-Write array wrapper
/// Clones only on mutation, allows cheap sharing
#[derive(Debug, Clone)]
pub struct CowArray {
    data: Rc<RefCell<Vec<JsVal>>>,
}

impl Default for CowArray {
    fn default() -> Self { Self::new() }
}

impl CowArray {
    pub fn new() -> Self {
        Self { data: Rc::new(RefCell::new(Vec::new())) }
    }
    
    pub fn with_capacity(cap: usize) -> Self {
        Self { data: Rc::new(RefCell::new(Vec::with_capacity(cap))) }
    }
    
    /// Check if we need to clone before mutation
    fn ensure_unique(&mut self) {
        if Rc::strong_count(&self.data) > 1 {
            // Clone the data before mutating - get clone first to avoid borrow conflict
            let cloned = (*self.data.borrow()).clone();
            self.data = Rc::new(RefCell::new(cloned));
        }
    }
    
    pub fn push(&mut self, val: JsVal) {
        self.ensure_unique();
        self.data.borrow_mut().push(val);
    }
    
    pub fn get(&self, idx: usize) -> JsVal {
        self.data.borrow().get(idx).cloned().unwrap_or(JsVal::Undefined)
    }
    
    pub fn set(&mut self, idx: usize, val: JsVal) {
        self.ensure_unique();
        let mut data = self.data.borrow_mut();
        if idx >= data.len() { data.resize(idx + 1, JsVal::Undefined); }
        data[idx] = val;
    }
    
    pub fn len(&self) -> usize { self.data.borrow().len() }
    pub fn is_empty(&self) -> bool { self.data.borrow().is_empty() }
}

// === STRING ROPE for O(1) concatenation ===

/// Rope structure for efficient string concatenation
#[derive(Debug, Clone)]
pub enum StringRope {
    Leaf(Box<str>),
    Concat(Rc<StringRope>, Rc<StringRope>),
}

impl StringRope {
    pub fn new(s: &str) -> Self { StringRope::Leaf(s.into()) }
    
    /// O(1) concatenation
    pub fn concat(left: Rc<StringRope>, right: Rc<StringRope>) -> Self {
        StringRope::Concat(left, right)
    }
    
    /// Flatten to string (O(n) but deferred)
    pub fn to_string(&self) -> String {
        let mut buf = String::new();
        self.collect(&mut buf);
        buf
    }
    
    fn collect(&self, buf: &mut String) {
        match self {
            StringRope::Leaf(s) => buf.push_str(s),
            StringRope::Concat(left, right) => {
                left.collect(buf);
                right.collect(buf);
            }
        }
    }
    
    pub fn len(&self) -> usize {
        match self {
            StringRope::Leaf(s) => s.len(),
            StringRope::Concat(left, right) => left.len() + right.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_smallvec_inline() {
        let mut sv: SmallVec<i32, 4> = SmallVec::new();
        sv.push(1); sv.push(2); sv.push(3); sv.push(4);
        assert_eq!(sv.len(), 4);
        assert!(sv.overflow.is_none()); // All inline!
    }
    
    #[test]
    fn test_smallvec_overflow() {
        let mut sv: SmallVec<i32, 4> = SmallVec::new();
        for i in 0..10 { sv.push(i); }
        assert_eq!(sv.len(), 10);
        assert!(sv.overflow.is_some());
    }
    
    #[test]
    fn test_object_small() {
        let mut obj = JsObject::new();
        obj.set("a", JsVal::Number(1.0));
        obj.set("b", JsVal::Number(2.0));
        assert!(obj.properties.overflow.is_none()); // Stack-allocated!
        assert_eq!(obj.get("a").and_then(|v| v.as_number()), Some(1.0));
    }
}
