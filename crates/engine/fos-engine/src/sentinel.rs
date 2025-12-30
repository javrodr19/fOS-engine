//! Sentinel Values (Phase 24.1)
//!
//! Use NaN/MAX as "none" for numerics - 4 bytes instead of 8 (Option<f32>).
//! Macro to wrap/unwrap sentinels for 50% savings on optional numerics.

use std::ops::{Add, Sub, Mul, Div};

/// Sentinel marker trait
pub trait Sentinel: Copy {
    /// The sentinel value representing "none"
    const SENTINEL: Self;
    
    /// Check if this is the sentinel value
    fn is_sentinel(self) -> bool;
    
    /// Convert to Option
    fn to_option(self) -> Option<Self> {
        if self.is_sentinel() {
            None
        } else {
            Some(self)
        }
    }
}

impl Sentinel for f32 {
    const SENTINEL: f32 = f32::NAN;
    
    #[inline]
    fn is_sentinel(self) -> bool {
        self.is_nan()
    }
}

impl Sentinel for f64 {
    const SENTINEL: f64 = f64::NAN;
    
    #[inline]
    fn is_sentinel(self) -> bool {
        self.is_nan()
    }
}

impl Sentinel for u32 {
    const SENTINEL: u32 = u32::MAX;
    
    #[inline]
    fn is_sentinel(self) -> bool {
        self == Self::SENTINEL
    }
}

impl Sentinel for u16 {
    const SENTINEL: u16 = u16::MAX;
    
    #[inline]
    fn is_sentinel(self) -> bool {
        self == Self::SENTINEL
    }
}

impl Sentinel for i32 {
    const SENTINEL: i32 = i32::MIN;
    
    #[inline]
    fn is_sentinel(self) -> bool {
        self == Self::SENTINEL
    }
}

/// Optional value using sentinel encoding (4 bytes for f32 vs 8 for Option<f32>)
#[derive(Clone, Copy)]
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

impl<T: Sentinel + PartialEq> PartialEq for Opt<T> {
    fn eq(&self, other: &Self) -> bool {
        match (self.get(), other.get()) {
            (None, None) => true,
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }
}

impl<T: Sentinel> Default for Opt<T> {
    fn default() -> Self {
        Self::none()
    }
}

impl<T: Sentinel> Opt<T> {
    /// Create a "none" value
    #[inline]
    pub const fn none() -> Self {
        Opt(T::SENTINEL)
    }
    
    /// Create a "some" value
    #[inline]
    pub fn some(value: T) -> Self {
        debug_assert!(!value.is_sentinel(), "Cannot store sentinel value as Some");
        Opt(value)
    }
    
    /// Check if none
    #[inline]
    pub fn is_none(self) -> bool {
        self.0.is_sentinel()
    }
    
    /// Check if some
    #[inline]
    pub fn is_some(self) -> bool {
        !self.is_none()
    }
    
    /// Get the value if present
    #[inline]
    pub fn get(self) -> Option<T> {
        self.0.to_option()
    }
    
    /// Get the value or a default
    #[inline]
    pub fn unwrap_or(self, default: T) -> T {
        if self.is_some() { self.0 } else { default }
    }
    
    /// Get the value or compute a default
    #[inline]
    pub fn unwrap_or_else<F: FnOnce() -> T>(self, f: F) -> T {
        if self.is_some() { self.0 } else { f() }
    }
    
    /// Map the value if present
    #[inline]
    pub fn map<U: Sentinel, F: FnOnce(T) -> U>(self, f: F) -> Opt<U> {
        match self.get() {
            Some(v) => Opt::some(f(v)),
            None => Opt::none(),
        }
    }
    
    /// Get the raw value (including sentinel)
    #[inline]
    pub fn raw(self) -> T {
        self.0
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

impl<T: Sentinel> From<Opt<T>> for Option<T> {
    fn from(opt: Opt<T>) -> Option<T> {
        opt.get()
    }
}

// Arithmetic operations for Opt<f32>
impl<T: Sentinel + Add<Output = T>> Add for Opt<T> {
    type Output = Opt<T>;
    
    fn add(self, rhs: Self) -> Self::Output {
        match (self.get(), rhs.get()) {
            (Some(a), Some(b)) => Opt::some(a + b),
            _ => Opt::none(),
        }
    }
}

impl<T: Sentinel + Sub<Output = T>> Sub for Opt<T> {
    type Output = Opt<T>;
    
    fn sub(self, rhs: Self) -> Self::Output {
        match (self.get(), rhs.get()) {
            (Some(a), Some(b)) => Opt::some(a - b),
            _ => Opt::none(),
        }
    }
}

impl<T: Sentinel + Mul<Output = T>> Mul for Opt<T> {
    type Output = Opt<T>;
    
    fn mul(self, rhs: Self) -> Self::Output {
        match (self.get(), rhs.get()) {
            (Some(a), Some(b)) => Opt::some(a * b),
            _ => Opt::none(),
        }
    }
}

impl<T: Sentinel + Div<Output = T>> Div for Opt<T> {
    type Output = Opt<T>;
    
    fn div(self, rhs: Self) -> Self::Output {
        match (self.get(), rhs.get()) {
            (Some(a), Some(b)) => Opt::some(a / b),
            _ => Opt::none(),
        }
    }
}

/// Type aliases for common sentinel types
pub type OptF32 = Opt<f32>;
pub type OptF64 = Opt<f64>;
pub type OptU32 = Opt<u32>;
pub type OptU16 = Opt<u16>;
pub type OptI32 = Opt<i32>;

/// Optional layout dimension (using sentinel)
#[derive(Clone, Copy, Debug, Default)]
pub struct OptDimension {
    pub width: OptF32,
    pub height: OptF32,
}

impl OptDimension {
    pub const fn none() -> Self {
        Self {
            width: OptF32::none(),
            height: OptF32::none(),
        }
    }
    
    pub fn new(width: Option<f32>, height: Option<f32>) -> Self {
        Self {
            width: width.into(),
            height: height.into(),
        }
    }
    
    pub fn both(width: f32, height: f32) -> Self {
        Self {
            width: OptF32::some(width),
            height: OptF32::some(height),
        }
    }
    
    /// Memory size: 8 bytes (vs 16 for Option<f32> x 2)
    pub const fn memory_size() -> usize {
        std::mem::size_of::<Self>()
    }
}

/// Optional box edges (margin, padding, etc.)
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
            top: OptF32::some(value),
            right: OptF32::some(value),
            bottom: OptF32::some(value),
            left: OptF32::some(value),
        }
    }
    
    pub fn symmetric(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: OptF32::some(vertical),
            right: OptF32::some(horizontal),
            bottom: OptF32::some(vertical),
            left: OptF32::some(horizontal),
        }
    }
    
    /// Memory size: 16 bytes (vs 32 for Option<f32> x 4)
    pub const fn memory_size() -> usize {
        std::mem::size_of::<Self>()
    }
    
    /// Resolve with defaults
    pub fn resolve(&self, default: f32) -> (f32, f32, f32, f32) {
        (
            self.top.unwrap_or(default),
            self.right.unwrap_or(default),
            self.bottom.unwrap_or(default),
            self.left.unwrap_or(default),
        )
    }
}

/// Calculate memory savings
pub fn memory_savings_report() -> String {
    format!(
        "Sentinel Value Memory Savings:\n\
         - OptF32: {} bytes (vs {} for Option<f32>)\n\
         - OptDimension: {} bytes (vs {} for 2x Option<f32>)\n\
         - OptEdges: {} bytes (vs {} for 4x Option<f32>)\n\
         Total savings: 50%",
        std::mem::size_of::<OptF32>(),
        std::mem::size_of::<Option<f32>>(),
        OptDimension::memory_size(),
        std::mem::size_of::<Option<f32>>() * 2,
        OptEdges::memory_size(),
        std::mem::size_of::<Option<f32>>() * 4,
    )
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
        assert_eq!(none.get(), None);
    }
    
    #[test]
    fn test_opt_u32() {
        let none: OptU32 = Opt::none();
        let some = Opt::some(100u32);
        
        assert!(none.is_none());
        assert_eq!(some.unwrap_or(0), 100);
        assert_eq!(none.unwrap_or(999), 999);
    }
    
    #[test]
    fn test_arithmetic() {
        let a = Opt::some(10.0f32);
        let b = Opt::some(5.0f32);
        let none: OptF32 = Opt::none();
        
        assert_eq!((a + b).get(), Some(15.0));
        assert_eq!((a - b).get(), Some(5.0));
        assert_eq!((a * b).get(), Some(50.0));
        assert_eq!((a / b).get(), Some(2.0));
        
        // None propagation
        assert!((a + none).is_none());
    }
    
    #[test]
    fn test_memory_size() {
        // Opt<f32> should be 4 bytes
        assert_eq!(std::mem::size_of::<OptF32>(), 4);
        
        // Option<f32> is 8 bytes (due to discriminant + alignment)
        assert_eq!(std::mem::size_of::<Option<f32>>(), 8);
        
        // 50% savings!
        assert_eq!(std::mem::size_of::<OptF32>() * 2, std::mem::size_of::<Option<f32>>());
        
        // OptDimension: 8 bytes vs 16
        assert_eq!(OptDimension::memory_size(), 8);
        
        // OptEdges: 16 bytes vs 32
        assert_eq!(OptEdges::memory_size(), 16);
    }
    
    #[test]
    fn test_opt_edges() {
        let edges = OptEdges::symmetric(10.0, 20.0);
        
        assert_eq!(edges.top.get(), Some(10.0));
        assert_eq!(edges.right.get(), Some(20.0));
        
        let (t, r, b, l) = edges.resolve(0.0);
        assert_eq!((t, r, b, l), (10.0, 20.0, 10.0, 20.0));
        
        // None resolution
        let partial = OptEdges {
            top: Opt::some(5.0),
            ..OptEdges::none()
        };
        let (t, r, b, l) = partial.resolve(1.0);
        assert_eq!((t, r, b, l), (5.0, 1.0, 1.0, 1.0));
    }
    
    #[test]
    fn test_from_option() {
        let some: OptF32 = Some(3.14f32).into();
        let none: OptF32 = None.into();
        
        assert_eq!(some.get(), Some(3.14));
        assert_eq!(none.get(), None);
        
        // And back
        let opt: Option<f32> = some.into();
        assert_eq!(opt, Some(3.14));
    }
    
    #[test]
    fn test_map() {
        let some = Opt::some(10.0f32);
        let doubled = some.map(|x| x * 2.0);
        assert_eq!(doubled.get(), Some(20.0));
        
        let none: OptF32 = Opt::none();
        let doubled_none = none.map(|x| x * 2.0);
        assert!(doubled_none.is_none());
    }
}
