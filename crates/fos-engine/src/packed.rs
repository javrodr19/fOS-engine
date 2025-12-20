//! Struct Packing & Cache Alignment Utilities
//!
//! Provides utilities for memory-efficient struct layout and cache-aligned types.
//!
//! # Guidelines for Field Ordering
//!
//! Order struct fields from largest to smallest to minimize padding:
//! ```rust
//! // Good: 16 bytes (no padding)
//! struct Good {
//!     a: u64,  // 8 bytes
//!     b: u32,  // 4 bytes
//!     c: u16,  // 2 bytes
//!     d: u8,   // 1 byte
//!     e: u8,   // 1 byte
//! }
//!
//! // Bad: 24 bytes (8 bytes padding)
//! struct Bad {
//!     a: u8,   // 1 byte + 7 padding
//!     b: u64,  // 8 bytes
//!     c: u8,   // 1 byte + 3 padding
//!     d: u32,  // 4 bytes
//! }
//! ```

use std::ops::{Deref, DerefMut};

/// Cache line size (64 bytes on most modern CPUs)
pub const CACHE_LINE_SIZE: usize = 64;

/// Cache-aligned wrapper for types.
///
/// Ensures the inner value is aligned to a cache line boundary (64 bytes).
/// This prevents false sharing when multiple threads access different data.
///
/// # Example
/// ```rust
/// use fos_engine::CacheAligned;
///
/// let aligned = CacheAligned::new(42u64);
/// assert_eq!(*aligned, 42);
/// ```
#[repr(C, align(64))]
#[derive(Debug, Clone, Copy)]
pub struct CacheAligned<T> {
    value: T,
    _pad: [u8; 0], // Zero-sized, just for alignment marker
}

impl<T> CacheAligned<T> {
    /// Create a new cache-aligned value.
    #[inline]
    pub const fn new(value: T) -> Self {
        Self { value, _pad: [] }
    }
    
    /// Get the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T> Deref for CacheAligned<T> {
    type Target = T;
    
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for CacheAligned<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T: Default> Default for CacheAligned<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

/// Packed struct with predictable C layout.
///
/// Use `#[repr(C)]` for structs that need predictable memory layout,
/// especially for FFI or memory-mapped data.
///
/// # Example
/// ```rust
/// use fos_engine::Packed;
///
/// #[repr(C)]
/// struct MyData {
///     a: u32,
///     b: u16,
/// }
///
/// let packed: Packed<MyData> = Packed::new(MyData { a: 1, b: 2 });
/// ```
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Packed<T> {
    value: T,
}

impl<T> Packed<T> {
    /// Create a new packed value.
    #[inline]
    pub const fn new(value: T) -> Self {
        Self { value }
    }
    
    /// Get the inner value.
    #[inline]
    pub fn into_inner(self) -> T {
        self.value
    }
    
    /// Get size in bytes.
    #[inline]
    pub const fn size() -> usize {
        std::mem::size_of::<T>()
    }
    
    /// Get alignment in bytes.
    #[inline]
    pub const fn align() -> usize {
        std::mem::align_of::<T>()
    }
}

impl<T> Deref for Packed<T> {
    type Target = T;
    
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for Packed<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

/// Macro to assert struct size at compile time.
///
/// # Example
/// ```rust
/// use fos_engine::assert_size;
///
/// struct MyStruct {
///     a: u64,
///     b: u32,
/// }
///
/// assert_size!(MyStruct, 16);
/// ```
#[macro_export]
macro_rules! assert_size {
    ($type:ty, $size:expr) => {
        const _: () = {
            if std::mem::size_of::<$type>() != $size {
                panic!(concat!(
                    "Size assertion failed for ",
                    stringify!($type),
                    ": expected ",
                    stringify!($size),
                    " bytes"
                ));
            }
        };
    };
}

/// Macro to assert struct alignment at compile time.
#[macro_export]
macro_rules! assert_align {
    ($type:ty, $align:expr) => {
        const _: () = {
            if std::mem::align_of::<$type>() != $align {
                panic!(concat!(
                    "Alignment assertion failed for ",
                    stringify!($type)
                ));
            }
        };
    };
}

/// Compact pair of values (avoids Option padding).
///
/// Stores two values without the overhead of Option.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactPair<A, B> {
    pub first: A,
    pub second: B,
}

impl<A, B> CompactPair<A, B> {
    #[inline]
    pub const fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}

/// Sentinel value utilities for avoiding Option overhead.
pub mod sentinel {
    /// Check if f32 is a sentinel (NaN).
    #[inline]
    pub fn is_sentinel_f32(value: f32) -> bool {
        value.is_nan()
    }
    
    /// Get f32 sentinel value (NaN).
    #[inline]
    pub const fn sentinel_f32() -> f32 {
        f32::NAN
    }
    
    /// Wrap f32 in Option using NaN as None.
    #[inline]
    pub fn from_sentinel_f32(value: f32) -> Option<f32> {
        if value.is_nan() {
            None
        } else {
            Some(value)
        }
    }
    
    /// Unwrap Option<f32> using NaN as sentinel.
    #[inline]
    pub fn to_sentinel_f32(value: Option<f32>) -> f32 {
        value.unwrap_or(f32::NAN)
    }
    
    /// Check if u32 is a sentinel (MAX).
    #[inline]
    pub fn is_sentinel_u32(value: u32) -> bool {
        value == u32::MAX
    }
    
    /// Get u32 sentinel value.
    #[inline]
    pub const fn sentinel_u32() -> u32 {
        u32::MAX
    }
    
    /// Wrap u32 in Option using MAX as None.
    #[inline]
    pub fn from_sentinel_u32(value: u32) -> Option<u32> {
        if value == u32::MAX {
            None
        } else {
            Some(value)
        }
    }
    
    /// Unwrap Option<u32> using MAX as sentinel.
    #[inline]
    pub fn to_sentinel_u32(value: Option<u32>) -> u32 {
        value.unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_aligned() {
        let aligned = CacheAligned::new(42u64);
        assert_eq!(*aligned, 42);
        
        let ptr = &aligned as *const _ as usize;
        assert_eq!(ptr % CACHE_LINE_SIZE, 0, "Should be cache-line aligned");
    }
    
    #[test]
    fn test_packed() {
        #[repr(C)]
        struct TestStruct {
            a: u32,
            b: u16,
        }
        
        let packed = Packed::new(TestStruct { a: 1, b: 2 });
        assert_eq!(packed.a, 1);
        assert_eq!(packed.b, 2);
    }
    
    #[test]
    fn test_sentinel_f32() {
        use sentinel::*;
        
        assert!(is_sentinel_f32(sentinel_f32()));
        assert!(!is_sentinel_f32(1.0));
        
        assert_eq!(from_sentinel_f32(1.0), Some(1.0));
        assert_eq!(from_sentinel_f32(f32::NAN), None);
    }
    
    #[test]
    fn test_sentinel_u32() {
        use sentinel::*;
        
        assert!(is_sentinel_u32(sentinel_u32()));
        assert!(!is_sentinel_u32(42));
        
        assert_eq!(from_sentinel_u32(42), Some(42));
        assert_eq!(from_sentinel_u32(u32::MAX), None);
    }
}
