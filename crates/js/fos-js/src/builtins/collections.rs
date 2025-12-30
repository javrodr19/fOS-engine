//! JavaScript Collections
//!
//! Map, Set, WeakMap, WeakSet implementations.

use std::collections::{HashMap, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Hash a JsMapKey (free function to avoid borrow issues)
fn hash_js_map_key(key: &JsMapKey) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// JavaScript Map
#[derive(Debug, Clone, Default)]
pub struct JsMap {
    entries: Vec<(JsMapKey, JsValue)>,
    index: HashMap<u64, usize>, // hash -> position
}

/// Map key (can be any value)
#[derive(Debug, Clone)]
pub enum JsMapKey {
    Undefined,
    Null,
    Bool(bool),
    Number(u64), // bits of f64
    String(String),
    Object(u32), // reference ID
    Symbol(u32),
}

impl Hash for JsMapKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Bool(b) => b.hash(state),
            Self::Number(n) => n.hash(state),
            Self::String(s) => s.hash(state),
            Self::Object(id) => id.hash(state),
            Self::Symbol(id) => id.hash(state),
            _ => {}
        }
    }
}

impl PartialEq for JsMapKey {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Undefined, Self::Undefined) => true,
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Number(a), Self::Number(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (Self::Object(a), Self::Object(b)) => a == b,
            (Self::Symbol(a), Self::Symbol(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for JsMapKey {}

/// JavaScript value for collections
#[derive(Debug, Clone)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Object(u32),
}

impl Default for JsValue {
    fn default() -> Self {
        Self::Undefined
    }
}

impl JsMap {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn size(&self) -> usize {
        self.entries.len()
    }
    
    pub fn get(&self, key: &JsMapKey) -> Option<&JsValue> {
        let hash = self.hash_key(key);
        self.index.get(&hash)
            .and_then(|&idx| self.entries.get(idx))
            .map(|(_, v)| v)
    }
    
    pub fn set(&mut self, key: JsMapKey, value: JsValue) {
        let hash = self.hash_key(&key);
        if let Some(&idx) = self.index.get(&hash) {
            self.entries[idx] = (key, value);
        } else {
            let idx = self.entries.len();
            self.index.insert(hash, idx);
            self.entries.push((key, value));
        }
    }
    
    pub fn has(&self, key: &JsMapKey) -> bool {
        let hash = self.hash_key(key);
        self.index.contains_key(&hash)
    }
    
    pub fn delete(&mut self, key: &JsMapKey) -> bool {
        let hash = self.hash_key(key);
        if let Some(&idx) = self.index.get(&hash) {
            self.entries.remove(idx);
            self.index.remove(&hash);
            // Update indices
            for (_, v) in self.index.iter_mut() {
                if *v > idx {
                    *v -= 1;
                }
            }
            true
        } else {
            false
        }
    }
    
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
    }
    
    pub fn keys(&self) -> impl Iterator<Item = &JsMapKey> {
        self.entries.iter().map(|(k, _)| k)
    }
    
    pub fn values(&self) -> impl Iterator<Item = &JsValue> {
        self.entries.iter().map(|(_, v)| v)
    }
    
    fn hash_key(&self, key: &JsMapKey) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

/// JavaScript Set
#[derive(Debug, Clone, Default)]
pub struct JsSet {
    values: Vec<JsMapKey>,
    index: HashSet<u64>,
}

impl JsSet {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn size(&self) -> usize {
        self.values.len()
    }
    
    pub fn add(&mut self, value: JsMapKey) -> bool {
        let hash = self.hash_value(&value);
        if self.index.insert(hash) {
            self.values.push(value);
            true
        } else {
            false
        }
    }
    
    pub fn has(&self, value: &JsMapKey) -> bool {
        let hash = self.hash_value(value);
        self.index.contains(&hash)
    }
    
    pub fn delete(&mut self, value: &JsMapKey) -> bool {
        let hash = hash_js_map_key(value);
        if self.index.remove(&hash) {
            self.values.retain(|v| hash_js_map_key(v) != hash);
            true
        } else {
            false
        }
    }
    
    pub fn clear(&mut self) {
        self.values.clear();
        self.index.clear();
    }
    
    pub fn values(&self) -> impl Iterator<Item = &JsMapKey> {
        self.values.iter()
    }
    
    fn hash_value(&self, value: &JsMapKey) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        hasher.finish()
    }
}

/// WeakMap (values can be garbage collected)
#[derive(Debug, Default)]
pub struct JsWeakMap {
    entries: HashMap<u32, JsValue>, // object ID -> value
}

impl JsWeakMap {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn get(&self, key: u32) -> Option<&JsValue> {
        self.entries.get(&key)
    }
    
    pub fn set(&mut self, key: u32, value: JsValue) {
        self.entries.insert(key, value);
    }
    
    pub fn has(&self, key: u32) -> bool {
        self.entries.contains_key(&key)
    }
    
    pub fn delete(&mut self, key: u32) -> bool {
        self.entries.remove(&key).is_some()
    }
}

/// WeakSet
#[derive(Debug, Default)]
pub struct JsWeakSet {
    objects: HashSet<u32>,
}

impl JsWeakSet {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add(&mut self, obj: u32) -> bool {
        self.objects.insert(obj)
    }
    
    pub fn has(&self, obj: u32) -> bool {
        self.objects.contains(&obj)
    }
    
    pub fn delete(&mut self, obj: u32) -> bool {
        self.objects.remove(&obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_map() {
        let mut map = JsMap::new();
        map.set(JsMapKey::String("key".into()), JsValue::Number(42.0));
        
        assert_eq!(map.size(), 1);
        assert!(map.has(&JsMapKey::String("key".into())));
    }
    
    #[test]
    fn test_set() {
        let mut set = JsSet::new();
        set.add(JsMapKey::Number(1));
        set.add(JsMapKey::Number(2));
        set.add(JsMapKey::Number(1)); // duplicate
        
        assert_eq!(set.size(), 2);
    }
}
