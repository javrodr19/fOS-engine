//! Fixed-Point Arithmetic
//!
//! Fixed-point types for deterministic, cross-platform layout calculations.
//!
//! Using fixed-point (integer) arithmetic instead of floating-point:
//! - Deterministic results across all platforms
//! - No floating-point rounding issues
//! - Can be faster on some architectures
//! - Smaller memory footprint than f64

use std::ops::{Add, Sub, Mul, Div, Neg, AddAssign, SubAssign};
use std::cmp::Ordering;

/// 16.16 fixed-point number (32-bit total).
///
/// - 16 bits for integer part: range -32768 to 32767
/// - 16 bits for fractional part: precision of ~0.00001
///
/// # Example
/// ```rust
/// use fos_engine::Fixed16;
///
/// let a = Fixed16::from_f32(10.5);
/// let b = Fixed16::from_f32(2.25);
/// let c = a + b;
/// assert!((c.to_f32() - 12.75).abs() < 0.001);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct Fixed16(i32);

impl Fixed16 {
    /// Number of fractional bits.
    pub const FRAC_BITS: u32 = 16;
    
    /// Scale factor (2^16 = 65536).
    pub const SCALE: i32 = 1 << Self::FRAC_BITS;
    
    /// Zero value.
    pub const ZERO: Fixed16 = Fixed16(0);
    
    /// One value.
    pub const ONE: Fixed16 = Fixed16(Self::SCALE);
    
    /// Half value.
    pub const HALF: Fixed16 = Fixed16(Self::SCALE / 2);
    
    /// Maximum value.
    pub const MAX: Fixed16 = Fixed16(i32::MAX);
    
    /// Minimum value.
    pub const MIN: Fixed16 = Fixed16(i32::MIN);
    
    /// Create from raw bits.
    #[inline]
    pub const fn from_bits(bits: i32) -> Self {
        Self(bits)
    }
    
    /// Get raw bits.
    #[inline]
    pub const fn to_bits(self) -> i32 {
        self.0
    }
    
    /// Create from integer.
    #[inline]
    pub const fn from_i32(value: i32) -> Self {
        Self(value << Self::FRAC_BITS)
    }
    
    /// Convert to integer (truncate towards zero).
    #[inline]
    pub const fn to_i32(self) -> i32 {
        self.0 >> Self::FRAC_BITS
    }
    
    /// Convert to integer (round to nearest).
    #[inline]
    pub const fn round(self) -> i32 {
        (self.0 + (Self::SCALE / 2)) >> Self::FRAC_BITS
    }
    
    /// Convert to integer (floor).
    #[inline]
    pub const fn floor(self) -> i32 {
        self.0 >> Self::FRAC_BITS
    }
    
    /// Convert to integer (ceiling).
    #[inline]
    pub const fn ceil(self) -> i32 {
        (self.0 + Self::SCALE - 1) >> Self::FRAC_BITS
    }
    
    /// Create from f32.
    #[inline]
    pub fn from_f32(value: f32) -> Self {
        Self((value * Self::SCALE as f32) as i32)
    }
    
    /// Convert to f32.
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }
    
    /// Create from f64.
    #[inline]
    pub fn from_f64(value: f64) -> Self {
        Self((value * Self::SCALE as f64) as i32)
    }
    
    /// Convert to f64.
    #[inline]
    pub fn to_f64(self) -> f64 {
        self.0 as f64 / Self::SCALE as f64
    }
    
    /// Absolute value.
    #[inline]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }
    
    /// Minimum of two values.
    #[inline]
    pub const fn min(self, other: Self) -> Self {
        if self.0 < other.0 { self } else { other }
    }
    
    /// Maximum of two values.
    #[inline]
    pub const fn max(self, other: Self) -> Self {
        if self.0 > other.0 { self } else { other }
    }
    
    /// Clamp to range.
    #[inline]
    pub const fn clamp(self, min: Self, max: Self) -> Self {
        self.max(min).min(max)
    }
    
    /// Linear interpolation.
    #[inline]
    pub fn lerp(self, other: Self, t: Self) -> Self {
        self + (other - self) * t
    }
    
    /// Fractional part only.
    #[inline]
    pub const fn fract(self) -> Self {
        Self(self.0 & (Self::SCALE - 1))
    }
}

impl Add for Fixed16 {
    type Output = Self;
    
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Fixed16 {
    type Output = Self;
    
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Mul for Fixed16 {
    type Output = Self;
    
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        // Use i64 to avoid overflow
        let result = (self.0 as i64 * rhs.0 as i64) >> Self::FRAC_BITS;
        Self(result as i32)
    }
}

impl Div for Fixed16 {
    type Output = Self;
    
    #[inline]
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 {
            return if self.0 >= 0 { Self::MAX } else { Self::MIN };
        }
        // Use i64 for precision
        let result = ((self.0 as i64) << Self::FRAC_BITS) / rhs.0 as i64;
        Self(result as i32)
    }
}

impl Neg for Fixed16 {
    type Output = Self;
    
    #[inline]
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl AddAssign for Fixed16 {
    #[inline]
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl SubAssign for Fixed16 {
    #[inline]
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl PartialOrd for Fixed16 {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Fixed16 {
    #[inline]
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.cmp(&other.0)
    }
}

impl From<i32> for Fixed16 {
    #[inline]
    fn from(value: i32) -> Self {
        Self::from_i32(value)
    }
}

impl From<f32> for Fixed16 {
    #[inline]
    fn from(value: f32) -> Self {
        Self::from_f32(value)
    }
}

/// 24.8 fixed-point number (32-bit total).
///
/// - 24 bits for integer part: range -8388608 to 8388607
/// - 8 bits for fractional part: precision of ~0.004
///
/// Useful for larger values with less precision needed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct Fixed8(i32);

impl Fixed8 {
    pub const FRAC_BITS: u32 = 8;
    pub const SCALE: i32 = 1 << Self::FRAC_BITS;
    
    pub const ZERO: Fixed8 = Fixed8(0);
    pub const ONE: Fixed8 = Fixed8(Self::SCALE);
    
    #[inline]
    pub const fn from_bits(bits: i32) -> Self {
        Self(bits)
    }
    
    #[inline]
    pub const fn to_bits(self) -> i32 {
        self.0
    }
    
    #[inline]
    pub const fn from_i32(value: i32) -> Self {
        Self(value << Self::FRAC_BITS)
    }
    
    #[inline]
    pub const fn to_i32(self) -> i32 {
        self.0 >> Self::FRAC_BITS
    }
    
    #[inline]
    pub fn from_f32(value: f32) -> Self {
        Self((value * Self::SCALE as f32) as i32)
    }
    
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }
}

impl Add for Fixed8 {
    type Output = Self;
    
    #[inline]
    fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Fixed8 {
    type Output = Self;
    
    #[inline]
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Mul for Fixed8 {
    type Output = Self;
    
    #[inline]
    fn mul(self, rhs: Self) -> Self {
        let result = (self.0 as i64 * rhs.0 as i64) >> Self::FRAC_BITS;
        Self(result as i32)
    }
}

impl Div for Fixed8 {
    type Output = Self;
    
    #[inline]
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 {
            return Self(i32::MAX);
        }
        let result = ((self.0 as i64) << Self::FRAC_BITS) / rhs.0 as i64;
        Self(result as i32)
    }
}

/// Fixed-point rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FixedRect {
    pub x: Fixed16,
    pub y: Fixed16,
    pub width: Fixed16,
    pub height: Fixed16,
}

impl FixedRect {
    #[inline]
    pub const fn new(x: Fixed16, y: Fixed16, width: Fixed16, height: Fixed16) -> Self {
        Self { x, y, width, height }
    }
    
    #[inline]
    pub const fn zero() -> Self {
        Self {
            x: Fixed16::ZERO,
            y: Fixed16::ZERO,
            width: Fixed16::ZERO,
            height: Fixed16::ZERO,
        }
    }
    
    /// Right edge (x + width).
    #[inline]
    pub fn right(&self) -> Fixed16 {
        self.x + self.width
    }
    
    /// Bottom edge (y + height).
    #[inline]
    pub fn bottom(&self) -> Fixed16 {
        self.y + self.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fixed16_basic() {
        let a = Fixed16::from_i32(10);
        let b = Fixed16::from_i32(3);
        
        assert_eq!((a + b).to_i32(), 13);
        assert_eq!((a - b).to_i32(), 7);
        assert_eq!((a * b).to_i32(), 30);
        assert_eq!((a / b).to_i32(), 3);
    }
    
    #[test]
    fn test_fixed16_fractions() {
        let a = Fixed16::from_f32(10.5);
        let b = Fixed16::from_f32(2.25);
        
        let sum = a + b;
        assert!((sum.to_f32() - 12.75).abs() < 0.001);
        
        let product = a * b;
        assert!((product.to_f32() - 23.625).abs() < 0.01);
    }
    
    #[test]
    fn test_fixed16_round_floor_ceil() {
        let a = Fixed16::from_f32(10.6);
        
        assert_eq!(a.floor(), 10);
        assert_eq!(a.ceil(), 11);
        assert_eq!(a.round(), 11);
        
        let b = Fixed16::from_f32(10.4);
        assert_eq!(b.round(), 10);
    }
    
    #[test]
    fn test_fixed16_lerp() {
        let a = Fixed16::from_f32(0.0);
        let b = Fixed16::from_f32(10.0);
        let t = Fixed16::from_f32(0.5);
        
        let result = a.lerp(b, t);
        assert!((result.to_f32() - 5.0).abs() < 0.01);
    }
}
