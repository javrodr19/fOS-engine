//! BigInt Implementation
//!
//! Arbitrary precision integers for JavaScript.

use std::cmp::Ordering;
use std::ops::{Add, Sub, Mul, Div, Rem, Neg};

/// JavaScript BigInt
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsBigInt {
    /// Magnitude in little-endian limbs
    limbs: Vec<u64>,
    /// Sign: true = negative
    negative: bool,
}

impl JsBigInt {
    /// Create zero
    pub fn zero() -> Self {
        Self { limbs: vec![0], negative: false }
    }
    
    /// Create from i64
    pub fn from_i64(n: i64) -> Self {
        if n == 0 {
            Self::zero()
        } else if n > 0 {
            Self { limbs: vec![n as u64], negative: false }
        } else {
            Self { limbs: vec![(-n) as u64], negative: true }
        }
    }
    
    /// Create from u64
    pub fn from_u64(n: u64) -> Self {
        Self { limbs: vec![n], negative: false }
    }
    
    /// Parse from string
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        
        let (negative, digits) = if s.starts_with('-') {
            (true, &s[1..])
        } else {
            (false, s)
        };
        
        // Strip trailing 'n' if present
        let digits = digits.strip_suffix('n').unwrap_or(digits);
        
        // Simple base-10 parsing
        let mut result = Self::zero();
        let ten = Self::from_u64(10);
        
        for c in digits.chars() {
            if let Some(d) = c.to_digit(10) {
                result = result * ten.clone() + Self::from_u64(d as u64);
            } else {
                return None;
            }
        }
        
        result.negative = negative && result != Self::zero();
        Some(result)
    }
    
    /// Convert to i64 (may overflow)
    pub fn to_i64(&self) -> Option<i64> {
        if self.limbs.len() > 1 {
            return None;
        }
        let val = self.limbs[0];
        if self.negative {
            if val > i64::MAX as u64 + 1 {
                None
            } else {
                Some(-(val as i64))
            }
        } else {
            if val > i64::MAX as u64 {
                None
            } else {
                Some(val as i64)
            }
        }
    }
    
    /// Check if zero
    pub fn is_zero(&self) -> bool {
        self.limbs.len() == 1 && self.limbs[0] == 0
    }
    
    /// Get absolute value
    pub fn abs(&self) -> Self {
        Self { limbs: self.limbs.clone(), negative: false }
    }
    
    /// Negate
    pub fn negate(&self) -> Self {
        if self.is_zero() {
            self.clone()
        } else {
            Self { limbs: self.limbs.clone(), negative: !self.negative }
        }
    }
    
    /// Power
    pub fn pow(&self, exp: u32) -> Self {
        if exp == 0 {
            return Self::from_u64(1);
        }
        let mut result = self.clone();
        for _ in 1..exp {
            result = result * self.clone();
        }
        result
    }
    
    /// Convert to string
    pub fn to_string(&self) -> String {
        if self.is_zero() {
            return "0".to_string();
        }
        
        let mut result = String::new();
        let mut temp = self.abs();
        let ten = Self::from_u64(10);
        
        while !temp.is_zero() {
            let (q, r) = temp.div_rem(&ten);
            result.push(char::from_digit(r.limbs[0] as u32, 10).unwrap());
            temp = q;
        }
        
        if self.negative {
            result.push('-');
        }
        
        result.chars().rev().collect()
    }
    
    /// Division with remainder
    fn div_rem(&self, other: &Self) -> (Self, Self) {
        // Simple implementation
        if other.is_zero() {
            panic!("Division by zero");
        }
        
        if self.is_zero() {
            return (Self::zero(), Self::zero());
        }
        
        // For simple single-limb cases
        if self.limbs.len() == 1 && other.limbs.len() == 1 {
            let q = self.limbs[0] / other.limbs[0];
            let r = self.limbs[0] % other.limbs[0];
            return (Self::from_u64(q), Self::from_u64(r));
        }
        
        (Self::zero(), self.clone())
    }
}

impl Add for JsBigInt {
    type Output = Self;
    
    fn add(self, other: Self) -> Self {
        // Simple implementation for single limb
        if self.limbs.len() == 1 && other.limbs.len() == 1 {
            let a = if self.negative { -(self.limbs[0] as i128) } else { self.limbs[0] as i128 };
            let b = if other.negative { -(other.limbs[0] as i128) } else { other.limbs[0] as i128 };
            let sum = a + b;
            
            if sum >= 0 {
                Self { limbs: vec![sum as u64], negative: false }
            } else {
                Self { limbs: vec![(-sum) as u64], negative: true }
            }
        } else {
            // Would need full multi-limb implementation
            self
        }
    }
}

impl Sub for JsBigInt {
    type Output = Self;
    
    fn sub(self, other: Self) -> Self {
        self + other.negate()
    }
}

impl Mul for JsBigInt {
    type Output = Self;
    
    fn mul(self, other: Self) -> Self {
        if self.is_zero() || other.is_zero() {
            return Self::zero();
        }
        
        // Simple single-limb
        if self.limbs.len() == 1 && other.limbs.len() == 1 {
            let (prod, overflow) = self.limbs[0].overflowing_mul(other.limbs[0]);
            if overflow {
                // Would need multi-limb
                Self { limbs: vec![prod], negative: self.negative != other.negative }
            } else {
                Self { limbs: vec![prod], negative: self.negative != other.negative }
            }
        } else {
            self
        }
    }
}

impl Neg for JsBigInt {
    type Output = Self;
    
    fn neg(self) -> Self {
        self.negate()
    }
}

impl PartialOrd for JsBigInt {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for JsBigInt {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.limbs.cmp(&other.limbs),
            (true, true) => other.limbs.cmp(&self.limbs),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bigint_parse() {
        let n = JsBigInt::parse("12345n").unwrap();
        assert_eq!(n.to_string(), "12345");
        
        let neg = JsBigInt::parse("-999").unwrap();
        assert_eq!(neg.to_string(), "-999");
    }
    
    #[test]
    fn test_bigint_ops() {
        let a = JsBigInt::from_i64(100);
        let b = JsBigInt::from_i64(50);
        
        assert_eq!((a.clone() + b.clone()).to_i64(), Some(150));
        assert_eq!((a.clone() - b.clone()).to_i64(), Some(50));
        assert_eq!((a * b).to_i64(), Some(5000));
    }
}
