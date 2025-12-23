//! Sentinel Values Integration
//!
//! Memory-efficient optional values using sentinel encoding.
//! Uses NaN for floats and MAX for integers - saves 50% vs Option.

use std::ops::{Add, Sub, Mul, Div};

/// Sentinel marker trait
pub trait Sentinel: Copy {
    const SENTINEL: Self;
    fn is_sentinel(self) -> bool;
}

impl Sentinel for f32 {
    const SENTINEL: f32 = f32::NAN;
    fn is_sentinel(self) -> bool { self.is_nan() }
}

impl Sentinel for u32 {
    const SENTINEL: u32 = u32::MAX;
    fn is_sentinel(self) -> bool { self == Self::SENTINEL }
}

impl Sentinel for i32 {
    const SENTINEL: i32 = i32::MIN;
    fn is_sentinel(self) -> bool { self == Self::SENTINEL }
}

/// Optional value using sentinel encoding (4 bytes vs 8 for Option<f32>)
#[derive(Clone, Copy, Default)]
#[repr(transparent)]
pub struct Opt<T: Sentinel>(T);

impl<T: Sentinel + std::fmt::Debug> std::fmt::Debug for Opt<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.get() {
            Some(v) => write!(f, "Some({:?})", v),
            None => write!(f, "None"),
        }
    }
}

impl<T: Sentinel> Opt<T> {
    /// Create a "none" value
    pub const fn none() -> Self { Opt(T::SENTINEL) }
    
    /// Create a "some" value
    pub fn some(value: T) -> Self { Opt(value) }
    
    /// Check if none
    pub fn is_none(self) -> bool { self.0.is_sentinel() }
    
    /// Check if some
    pub fn is_some(self) -> bool { !self.is_none() }
    
    /// Get the value if present
    pub fn get(self) -> Option<T> {
        if self.is_some() { Some(self.0) } else { None }
    }
    
    /// Get the value or a default
    pub fn unwrap_or(self, default: T) -> T {
        if self.is_some() { self.0 } else { default }
    }
}

impl<T: Sentinel> From<Option<T>> for Opt<T> {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => Opt::some(v),
            None => Opt::none(),
        }
    }
}

/// Type aliases
pub type OptF32 = Opt<f32>;
pub type OptU32 = Opt<u32>;
pub type OptI32 = Opt<i32>;

/// Optional dimension (8 bytes vs 16 for 2x Option<f32>)
#[derive(Clone, Copy, Debug, Default)]
pub struct OptDimension {
    pub width: OptF32,
    pub height: OptF32,
}

impl OptDimension {
    pub const fn none() -> Self {
        Self { width: OptF32::none(), height: OptF32::none() }
    }
    
    pub fn both(width: f32, height: f32) -> Self {
        Self { width: Opt::some(width), height: Opt::some(height) }
    }
}

/// Optional edges (16 bytes vs 32 for 4x Option<f32>)
#[derive(Clone, Copy, Debug, Default)]
pub struct OptEdges {
    pub top: OptF32,
    pub right: OptF32,
    pub bottom: OptF32,
    pub left: OptF32,
}

impl OptEdges {
    pub const fn none() -> Self {
        Self {
            top: OptF32::none(),
            right: OptF32::none(),
            bottom: OptF32::none(),
            left: OptF32::none(),
        }
    }
    
    pub fn all(value: f32) -> Self {
        Self {
            top: Opt::some(value),
            right: Opt::some(value),
            bottom: Opt::some(value),
            left: Opt::some(value),
        }
    }
    
    pub fn resolve(&self, default: f32) -> (f32, f32, f32, f32) {
        (
            self.top.unwrap_or(default),
            self.right.unwrap_or(default),
            self.bottom.unwrap_or(default),
            self.left.unwrap_or(default),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_opt_f32() {
        let none: OptF32 = Opt::none();
        let some = Opt::some(42.0f32);
        
        assert!(none.is_none());
        assert!(some.is_some());
        assert_eq!(some.get(), Some(42.0));
    }
    
    #[test]
    fn test_memory_size() {
        // Opt<f32> should be 4 bytes vs 8 for Option<f32>
        assert_eq!(std::mem::size_of::<OptF32>(), 4);
        assert_eq!(std::mem::size_of::<Option<f32>>(), 8);
    }
    
    #[test]
    fn test_opt_edges() {
        let edges = OptEdges::all(10.0);
        let (t, r, b, l) = edges.resolve(0.0);
        assert_eq!((t, r, b, l), (10.0, 10.0, 10.0, 10.0));
    }
}
