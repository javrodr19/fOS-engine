//! Fixed-Point Arithmetic (Local Copy)
//!
//! Fixed-point types for deterministic variable font axis values.
//! Local copy to avoid cyclic dependency with fos-engine.

use std::ops::{Add, Sub, Mul, Div, Neg};
use std::cmp::Ordering;

/// 16.16 fixed-point number (32-bit total)
/// 
/// - 16 bits for integer part: range -32768 to 32767
/// - 16 bits for fractional part: precision of ~0.00001
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(transparent)]
pub struct Fixed16(i32);

impl Fixed16 {
    pub const FRAC_BITS: u32 = 16;
    pub const SCALE: i32 = 1 << Self::FRAC_BITS;
    
    pub const ZERO: Fixed16 = Fixed16(0);
    pub const ONE: Fixed16 = Fixed16(Self::SCALE);
    pub const HALF: Fixed16 = Fixed16(Self::SCALE / 2);
    pub const MAX: Fixed16 = Fixed16(i32::MAX);
    pub const MIN: Fixed16 = Fixed16(i32::MIN);
    
    /// Create from raw bits
    #[inline]
    pub const fn from_bits(bits: i32) -> Self {
        Self(bits)
    }
    
    /// Get raw bits
    #[inline]
    pub const fn to_bits(self) -> i32 {
        self.0
    }
    
    /// Create from integer
    #[inline]
    pub const fn from_i32(value: i32) -> Self {
        Self(value << Self::FRAC_BITS)
    }
    
    /// Convert to integer (truncate)
    #[inline]
    pub const fn to_i32(self) -> i32 {
        self.0 >> Self::FRAC_BITS
    }
    
    /// Create from f32
    #[inline]
    pub fn from_f32(value: f32) -> Self {
        Self((value * Self::SCALE as f32) as i32)
    }
    
    /// Convert to f32
    #[inline]
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }
    
    /// Absolute value
    #[inline]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }
    
    /// Minimum of two values
    #[inline]
    pub const fn min(self, other: Self) -> Self {
        if self.0 < other.0 { self } else { other }
    }
    
    /// Maximum of two values
    #[inline]
    pub const fn max(self, other: Self) -> Self {
        if self.0 > other.0 { self } else { other }
    }
    
    /// Clamp to range
    #[inline]
    pub const fn clamp(self, min: Self, max: Self) -> Self {
        self.max(min).min(max)
    }
    
    /// Linear interpolation
    #[inline]
    pub fn lerp(self, other: Self, t: Self) -> Self {
        self + (other - self) * t
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
    }
    
    #[test]
    fn test_fixed16_fractions() {
        let a = Fixed16::from_f32(10.5);
        let b = Fixed16::from_f32(2.25);
        
        let sum = a + b;
        assert!((sum.to_f32() - 12.75).abs() < 0.001);
    }
    
    #[test]
    fn test_fixed16_clamp() {
        let value = Fixed16::from_f32(500.0);
        let min = Fixed16::from_f32(100.0);
        let max = Fixed16::from_f32(400.0);
        
        assert_eq!(value.clamp(min, max).to_f32(), 400.0);
    }
}
