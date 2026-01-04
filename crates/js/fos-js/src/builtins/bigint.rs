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
    
    /// Division with remainder using binary long division
    fn div_rem(&self, other: &Self) -> (Self, Self) {
        if other.is_zero() {
            panic!("Division by zero");
        }
        
        if self.is_zero() {
            return (Self::zero(), Self::zero());
        }
        
        // Handle sign
        let quotient_negative = self.negative != other.negative;
        let remainder_negative = self.negative;
        
        let dividend = self.abs();
        let divisor = other.abs();
        
        // If dividend < divisor, quotient is 0
        if dividend < divisor {
            return (Self::zero(), self.clone());
        }
        
        // For simple single-limb cases (optimization)
        if self.limbs.len() == 1 && other.limbs.len() == 1 {
            let q = self.limbs[0] / other.limbs[0];
            let r = self.limbs[0] % other.limbs[0];
            let q_neg = if q == 0 { false } else { quotient_negative };
            let r_neg = if r == 0 { false } else { remainder_negative };
            return (
                Self { limbs: vec![q], negative: q_neg },
                Self { limbs: vec![r], negative: r_neg }
            );
        }
        
        // Binary long division for multi-limb
        let mut quotient = Vec::new();
        let mut remainder = Self::zero();
        
        // Get total bits in dividend
        let total_bits = dividend.limbs.len() * 64;
        
        for i in (0..total_bits).rev() {
            // Shift remainder left by 1
            remainder = Self {
                limbs: {
                    let mut new_limbs = vec![0u64; remainder.limbs.len() + 1];
                    let mut carry = 0u64;
                    for (j, &limb) in remainder.limbs.iter().enumerate() {
                        let shifted = (limb << 1) | carry;
                        new_limbs[j] = shifted;
                        carry = limb >> 63;
                    }
                    new_limbs[remainder.limbs.len()] = carry;
                    // Trim leading zeros
                    while new_limbs.len() > 1 && *new_limbs.last().unwrap() == 0 {
                        new_limbs.pop();
                    }
                    new_limbs
                },
                negative: false,
            };
            
            // Get bit i of dividend
            let limb_idx = i / 64;
            let bit_idx = i % 64;
            let bit = if limb_idx < dividend.limbs.len() {
                (dividend.limbs[limb_idx] >> bit_idx) & 1
            } else {
                0
            };
            
            // Add bit to remainder
            remainder.limbs[0] |= bit;
            
            // Compare remainder with divisor
            if remainder >= divisor {
                remainder = remainder.clone() - divisor.clone();
                // Set bit in quotient
                let q_limb_idx = i / 64;
                while quotient.len() <= q_limb_idx {
                    quotient.push(0u64);
                }
                quotient[q_limb_idx] |= 1u64 << (i % 64);
            }
        }
        
        // Trim leading zeros
        while quotient.len() > 1 && *quotient.last().unwrap() == 0 {
            quotient.pop();
        }
        if quotient.is_empty() {
            quotient.push(0);
        }
        
        let is_q_zero = quotient.len() == 1 && quotient[0] == 0;
        let is_r_zero = remainder.is_zero();
        
        (
            Self { limbs: quotient, negative: if is_q_zero { false } else { quotient_negative } },
            Self { limbs: remainder.limbs, negative: if is_r_zero { false } else { remainder_negative } }
        )
    }
}

impl Add for JsBigInt {
    type Output = Self;
    
    fn add(self, other: Self) -> Self {
        // Handle sign: if same sign, add magnitudes; if different, subtract
        if self.negative == other.negative {
            // Same sign - add magnitudes
            let mut result = Vec::new();
            let mut carry: u64 = 0;
            let len = std::cmp::max(self.limbs.len(), other.limbs.len());
            
            for i in 0..len {
                let a = self.limbs.get(i).copied().unwrap_or(0);
                let b = other.limbs.get(i).copied().unwrap_or(0);
                
                let (sum1, c1) = a.overflowing_add(b);
                let (sum2, c2) = sum1.overflowing_add(carry);
                
                result.push(sum2);
                carry = (c1 as u64) + (c2 as u64);
            }
            
            if carry > 0 {
                result.push(carry);
            }
            
            Self { limbs: result, negative: self.negative }
        } else {
            // Different signs - subtract smaller from larger
            let (larger, smaller, result_negative) = if self.abs() >= other.abs() {
                (&self, &other, self.negative)
            } else {
                (&other, &self, other.negative)
            };
            
            let mut result = Vec::new();
            let mut borrow: u64 = 0;
            
            for i in 0..larger.limbs.len() {
                let a = larger.limbs.get(i).copied().unwrap_or(0);
                let b = smaller.limbs.get(i).copied().unwrap_or(0);
                
                let (diff1, b1) = a.overflowing_sub(b);
                let (diff2, b2) = diff1.overflowing_sub(borrow);
                
                result.push(diff2);
                borrow = (b1 as u64) + (b2 as u64);
            }
            
            // Remove leading zeros
            while result.len() > 1 && *result.last().unwrap() == 0 {
                result.pop();
            }
            
            let is_zero = result.len() == 1 && result[0] == 0;
            Self { limbs: result, negative: if is_zero { false } else { result_negative } }
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
        
        let result_negative = self.negative != other.negative;
        
        // Schoolbook multiplication with multi-limb support
        let n = self.limbs.len();
        let m = other.limbs.len();
        let mut result = vec![0u64; n + m];
        
        for i in 0..n {
            let mut carry: u64 = 0;
            for j in 0..m {
                // Use u128 for full 64x64 -> 128 multiplication
                let product = (self.limbs[i] as u128) * (other.limbs[j] as u128)
                    + (result[i + j] as u128)
                    + (carry as u128);
                
                result[i + j] = product as u64;
                carry = (product >> 64) as u64;
            }
            result[i + m] = carry;
        }
        
        // Remove leading zeros
        while result.len() > 1 && *result.last().unwrap() == 0 {
            result.pop();
        }
        
        Self { limbs: result, negative: result_negative }
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
