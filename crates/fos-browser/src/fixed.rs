//! Fixed-Point Math Integration
//!
//! Deterministic, cross-platform fixed-point arithmetic for layout.

use std::ops::{Add, Sub, Mul, Div};

/// 16.16 fixed-point number
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Fixed16(i32);

impl Fixed16 {
    pub const FRAC_BITS: u32 = 16;
    pub const SCALE: i32 = 1 << Self::FRAC_BITS;
    pub const ZERO: Fixed16 = Fixed16(0);
    pub const ONE: Fixed16 = Fixed16(Self::SCALE);
    
    /// Create from integer
    pub const fn from_i32(value: i32) -> Self {
        Self(value << Self::FRAC_BITS)
    }
    
    /// Convert to integer
    pub const fn to_i32(self) -> i32 {
        self.0 >> Self::FRAC_BITS
    }
    
    /// Create from f32
    pub fn from_f32(value: f32) -> Self {
        Self((value * Self::SCALE as f32) as i32)
    }
    
    /// Convert to f32
    pub fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }
    
    /// Round to nearest integer
    pub const fn round(self) -> i32 {
        (self.0 + (Self::SCALE / 2)) >> Self::FRAC_BITS
    }
    
    /// Floor
    pub const fn floor(self) -> i32 {
        self.0 >> Self::FRAC_BITS
    }
    
    /// Ceiling
    pub const fn ceil(self) -> i32 {
        (self.0 + Self::SCALE - 1) >> Self::FRAC_BITS
    }
    
    /// Absolute value
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }
    
    /// Minimum
    pub const fn min(self, other: Self) -> Self {
        if self.0 < other.0 { self } else { other }
    }
    
    /// Maximum
    pub const fn max(self, other: Self) -> Self {
        if self.0 > other.0 { self } else { other }
    }
}

impl Add for Fixed16 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Fixed16 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl Mul for Fixed16 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        let result = (self.0 as i64 * rhs.0 as i64) >> Self::FRAC_BITS;
        Self(result as i32)
    }
}

impl Div for Fixed16 {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 {
            return Self(i32::MAX);
        }
        let result = ((self.0 as i64) << Self::FRAC_BITS) / rhs.0 as i64;
        Self(result as i32)
    }
}

/// Fixed-point rectangle
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FixedRect {
    pub x: Fixed16,
    pub y: Fixed16,
    pub width: Fixed16,
    pub height: Fixed16,
}

impl FixedRect {
    pub fn new(x: Fixed16, y: Fixed16, width: Fixed16, height: Fixed16) -> Self {
        Self { x, y, width, height }
    }
    
    pub fn from_f32(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x: Fixed16::from_f32(x),
            y: Fixed16::from_f32(y),
            width: Fixed16::from_f32(width),
            height: Fixed16::from_f32(height),
        }
    }
    
    pub fn right(&self) -> Fixed16 {
        self.x + self.width
    }
    
    pub fn bottom(&self) -> Fixed16 {
        self.y + self.height
    }
}

/// Fixed-point math utilities
pub struct FixedMath;

impl FixedMath {
    /// Linear interpolation
    pub fn lerp(a: Fixed16, b: Fixed16, t: Fixed16) -> Fixed16 {
        a + (b - a) * t
    }
    
    /// Clamp value
    pub fn clamp(value: Fixed16, min: Fixed16, max: Fixed16) -> Fixed16 {
        value.max(min).min(max)
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
    }
    
    #[test]
    fn test_fixed_rect() {
        let rect = FixedRect::from_f32(10.0, 20.0, 100.0, 50.0);
        
        assert!((rect.right().to_f32() - 110.0).abs() < 0.01);
        assert!((rect.bottom().to_f32() - 70.0).abs() < 0.01);
    }
}
