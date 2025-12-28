//! JavaScript Objects
//!
//! Runtime object representation.

use super::value::JsVal;
use std::collections::HashMap;

/// JavaScript object
#[derive(Debug, Clone, Default)]
pub struct JsObject {
    properties: HashMap<Box<str>, JsVal>,
    prototype: Option<u32>,
}

impl JsObject {
    pub fn new() -> Self { Self::default() }
    
    pub fn with_prototype(proto: u32) -> Self {
        Self { properties: HashMap::new(), prototype: Some(proto) }
    }
    
    pub fn get(&self, key: &str) -> Option<&JsVal> { self.properties.get(key) }
    pub fn set(&mut self, key: &str, value: JsVal) { self.properties.insert(key.into(), value); }
    pub fn delete(&mut self, key: &str) -> bool { self.properties.remove(key).is_some() }
    pub fn has(&self, key: &str) -> bool { self.properties.contains_key(key) }
    pub fn keys(&self) -> impl Iterator<Item = &str> { self.properties.keys().map(|k| &**k) }
    pub fn prototype(&self) -> Option<u32> { self.prototype }
    pub fn set_prototype(&mut self, proto: Option<u32>) { self.prototype = proto; }
}

/// JavaScript array
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
    pub arity: u8,
    pub bytecode_offset: u32,
    pub upvalues: Vec<u32>,
}

impl JsFunction {
    pub fn new(name: Option<Box<str>>, arity: u8, offset: u32) -> Self {
        Self { name, arity, bytecode_offset: offset, upvalues: Vec::new() }
    }
}
