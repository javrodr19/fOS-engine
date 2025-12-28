//! Map and Set Collections
//!
//! JavaScript Map, Set, WeakMap, WeakSet implementations.

use super::value::JsVal;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// JavaScript Map
#[derive(Debug, Clone, Default)]
pub struct JsMap {
    entries: Vec<(JsVal, JsVal)>,
}

impl JsMap {
    pub fn new() -> Self { Self::default() }
    
    pub fn get(&self, key: &JsVal) -> Option<&JsVal> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
    
    pub fn set(&mut self, key: JsVal, value: JsVal) {
        if let Some(entry) = self.entries.iter_mut().find(|(k, _)| k == &key) {
            entry.1 = value;
        } else {
            self.entries.push((key, value));
        }
    }
    
    pub fn has(&self, key: &JsVal) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }
    
    pub fn delete(&mut self, key: &JsVal) -> bool {
        if let Some(idx) = self.entries.iter().position(|(k, _)| k == key) {
            self.entries.remove(idx);
            true
        } else { false }
    }
    
    pub fn clear(&mut self) { self.entries.clear(); }
    pub fn size(&self) -> usize { self.entries.len() }
    pub fn keys(&self) -> impl Iterator<Item = &JsVal> { self.entries.iter().map(|(k, _)| k) }
    pub fn values(&self) -> impl Iterator<Item = &JsVal> { self.entries.iter().map(|(_, v)| v) }
    pub fn entries(&self) -> impl Iterator<Item = (&JsVal, &JsVal)> { self.entries.iter().map(|(k, v)| (k, v)) }
}

/// JavaScript Set
#[derive(Debug, Clone, Default)]
pub struct JsSet {
    values: Vec<JsVal>,
}

impl JsSet {
    pub fn new() -> Self { Self::default() }
    
    pub fn add(&mut self, value: JsVal) {
        if !self.has(&value) { self.values.push(value); }
    }
    
    pub fn has(&self, value: &JsVal) -> bool {
        self.values.contains(value)
    }
    
    pub fn delete(&mut self, value: &JsVal) -> bool {
        if let Some(idx) = self.values.iter().position(|v| v == value) {
            self.values.remove(idx);
            true
        } else { false }
    }
    
    pub fn clear(&mut self) { self.values.clear(); }
    pub fn size(&self) -> usize { self.values.len() }
    pub fn values(&self) -> impl Iterator<Item = &JsVal> { self.values.iter() }
}

/// JavaScript WeakMap (simplified - uses object IDs)
#[derive(Debug, Default)]
pub struct JsWeakMap {
    entries: HashMap<u32, JsVal>, // Object ID -> Value
}

impl JsWeakMap {
    pub fn new() -> Self { Self::default() }
    
    pub fn get(&self, key: u32) -> Option<&JsVal> { self.entries.get(&key) }
    pub fn set(&mut self, key: u32, value: JsVal) { self.entries.insert(key, value); }
    pub fn has(&self, key: u32) -> bool { self.entries.contains_key(&key) }
    pub fn delete(&mut self, key: u32) -> bool { self.entries.remove(&key).is_some() }
}

/// JavaScript WeakSet (simplified - uses object IDs)
#[derive(Debug, Default)]
pub struct JsWeakSet {
    values: HashSet<u32>, // Object IDs
}

impl JsWeakSet {
    pub fn new() -> Self { Self::default() }
    
    pub fn add(&mut self, value: u32) { self.values.insert(value); }
    pub fn has(&self, value: u32) -> bool { self.values.contains(&value) }
    pub fn delete(&mut self, value: u32) -> bool { self.values.remove(&value) }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_map() {
        let mut map = JsMap::new();
        map.set(JsVal::String("key".into()), JsVal::Number(42.0));
        assert_eq!(map.get(&JsVal::String("key".into())), Some(&JsVal::Number(42.0)));
        assert!(map.has(&JsVal::String("key".into())));
        assert_eq!(map.size(), 1);
    }
    
    #[test]
    fn test_set() {
        let mut set = JsSet::new();
        set.add(JsVal::Number(1.0));
        set.add(JsVal::Number(2.0));
        set.add(JsVal::Number(1.0)); // Duplicate
        assert_eq!(set.size(), 2);
        assert!(set.has(&JsVal::Number(1.0)));
    }
    
    #[test]
    fn test_weak_map() {
        let mut wm = JsWeakMap::new();
        wm.set(123, JsVal::String("value".into()));
        assert!(wm.has(123));
        wm.delete(123);
        assert!(!wm.has(123));
    }
}
