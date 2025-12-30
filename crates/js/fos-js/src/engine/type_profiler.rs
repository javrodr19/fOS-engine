//! Type Profiler
//!
//! Runtime type profiling for optimization decisions.
//! Tracks types seen at call sites and property accesses.

use super::value::JsVal;
use std::collections::HashMap;

/// Type observed at runtime
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObservedType {
    Undefined,
    Null,
    Boolean,
    Integer,    // f64 that is actually an integer
    Float,
    String,
    Object,
    Array,
    Function,
    Mixed,      // Multiple types seen
}

impl ObservedType {
    pub fn from_jsval(val: &JsVal) -> Self {
        if val.is_undefined() { ObservedType::Undefined }
        else if val.is_null() { ObservedType::Null }
        else if val.as_bool().is_some() { ObservedType::Boolean }
        else if let Some(n) = val.as_number() {
            if n.fract() == 0.0 && n.abs() < i32::MAX as f64 {
                ObservedType::Integer
            } else {
                ObservedType::Float
            }
        }
        else if val.as_string().is_some() { ObservedType::String }
        else if val.as_object_id().is_some() { ObservedType::Object }
        else if val.as_array_id().is_some() { ObservedType::Array }
        else if val.as_function_id().is_some() { ObservedType::Function }
        else { ObservedType::Mixed }
    }
    
    pub fn merge(self, other: Self) -> Self {
        if self == other { self }
        else { ObservedType::Mixed }
    }
}

/// Type profile for a bytecode location
#[derive(Debug, Clone, Default)]
pub struct TypeProfile {
    pub types_seen: Vec<ObservedType>,
    pub sample_count: u32,
    pub is_monomorphic: bool,
}

impl TypeProfile {
    pub fn new() -> Self { Self::default() }
    
    pub fn record(&mut self, ty: ObservedType) {
        self.sample_count += 1;
        if !self.types_seen.contains(&ty) {
            self.types_seen.push(ty);
        }
        self.is_monomorphic = self.types_seen.len() == 1;
    }
    
    /// Get the dominant type if monomorphic
    pub fn dominant_type(&self) -> Option<ObservedType> {
        if self.is_monomorphic {
            self.types_seen.first().copied()
        } else {
            None
        }
    }
}

/// Type profiler manager
#[derive(Debug, Default)]
pub struct TypeProfiler {
    profiles: HashMap<u32, TypeProfile>,  // bytecode offset -> profile
    total_samples: u64,
}

impl TypeProfiler {
    pub fn new() -> Self { Self::default() }
    
    /// Record type observation at bytecode offset
    #[inline]
    pub fn record(&mut self, offset: u32, val: &JsVal) {
        let ty = ObservedType::from_jsval(val);
        let profile = self.profiles.entry(offset).or_default();
        profile.record(ty);
        self.total_samples += 1;
    }
    
    /// Get profile for offset
    pub fn get(&self, offset: u32) -> Option<&TypeProfile> {
        self.profiles.get(&offset)
    }
    
    /// Check if site is monomorphic (optimization candidate)
    pub fn is_monomorphic(&self, offset: u32) -> bool {
        self.profiles.get(&offset).map(|p| p.is_monomorphic).unwrap_or(false)
    }
    
    /// Get dominant type for offset
    pub fn dominant_type(&self, offset: u32) -> Option<ObservedType> {
        self.profiles.get(&offset).and_then(|p| p.dominant_type())
    }
    
    /// Stats
    pub fn total_samples(&self) -> u64 { self.total_samples }
    pub fn profile_count(&self) -> usize { self.profiles.len() }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_type_detection() {
        assert_eq!(ObservedType::from_jsval(&JsVal::Number(42.0)), ObservedType::Integer);
        assert_eq!(ObservedType::from_jsval(&JsVal::Number(3.14)), ObservedType::Float);
        assert_eq!(ObservedType::from_jsval(&JsVal::Bool(true)), ObservedType::Boolean);
    }
    
    #[test]
    fn test_monomorphic() {
        let mut profiler = TypeProfiler::new();
        
        // All integers at offset 0
        profiler.record(0, &JsVal::Number(1.0));
        profiler.record(0, &JsVal::Number(2.0));
        profiler.record(0, &JsVal::Number(3.0));
        
        assert!(profiler.is_monomorphic(0));
        assert_eq!(profiler.dominant_type(0), Some(ObservedType::Integer));
    }
    
    #[test]
    fn test_polymorphic() {
        let mut profiler = TypeProfiler::new();
        
        profiler.record(0, &JsVal::Number(1.0));
        profiler.record(0, &JsVal::String("hello".into()));
        
        assert!(!profiler.is_monomorphic(0));
    }
}
