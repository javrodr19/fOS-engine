//! Arbitrary-precision integers (BigInt)
//!
//! Sign and magnitude, the magnitude as little-endian 32-bit limbs with no
//! leading zero limbs (zero has no limbs and is never negative). BigInts in
//! web code are mostly 64-bit ids and hashes, so values of a few limbs are
//! the common case; multiplication is schoolbook and division is Knuth's
//! algorithm D, which are the fastest choices at those sizes.

use std::cmp::Ordering;

use crate::gc::{CellKind, Trace, Tracer};

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct BigInt {
    neg: bool,
    mag: Vec<u32>,
}

impl Trace for BigInt {
    const KIND: CellKind = CellKind::BigInt;
    fn trace(&self, _: &mut Tracer) {}
}

fn trim(mag: &mut Vec<u32>) {
    while mag.last() == Some(&0) {
        mag.pop();
    }
}

fn cmp_mag(a: &[u32], b: &[u32]) -> Ordering {
    a.len().cmp(&b.len()).then_with(|| a.iter().rev().cmp(b.iter().rev()))
}

fn add_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    let (a, b) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let mut out = Vec::with_capacity(a.len() + 1);
    let mut carry = 0u64;
    for i in 0..a.len() {
        let s = a[i] as u64 + *b.get(i).unwrap_or(&0) as u64 + carry;
        out.push(s as u32);
        carry = s >> 32;
    }
    if carry != 0 {
        out.push(carry as u32);
    }
    out
}

/// `a - b` for `a >= b`
fn sub_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = Vec::with_capacity(a.len());
    let mut borrow = 0i64;
    for i in 0..a.len() {
        let mut d = a[i] as i64 - *b.get(i).unwrap_or(&0) as i64 - borrow;
        borrow = if d < 0 {
            d += 1 << 32;
            1
        } else {
            0
        };
        out.push(d as u32);
    }
    trim(&mut out);
    out
}

fn mul_mag(a: &[u32], b: &[u32]) -> Vec<u32> {
    if a.is_empty() || b.is_empty() {
        return Vec::new();
    }
    let mut out = vec![0u32; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let t = x as u64 * y as u64 + out[i + j] as u64 + carry;
            out[i + j] = t as u32;
            carry = t >> 32;
        }
        out[i + b.len()] = carry as u32;
    }
    trim(&mut out);
    out
}

/// Divide by a single limb: (quotient, remainder)
fn divrem_small(a: &[u32], d: u32) -> (Vec<u32>, u32) {
    let mut q = vec![0u32; a.len()];
    let mut r = 0u64;
    for i in (0..a.len()).rev() {
        let cur = (r << 32) | a[i] as u64;
        q[i] = (cur / d as u64) as u32;
        r = cur % d as u64;
    }
    trim(&mut q);
    (q, r as u32)
}

/// Knuth's algorithm D: (quotient, remainder) of magnitudes, `b` non-zero
fn divrem_mag(a: &[u32], b: &[u32]) -> (Vec<u32>, Vec<u32>) {
    if cmp_mag(a, b) == Ordering::Less {
        return (Vec::new(), a.to_vec());
    }
    if b.len() == 1 {
        let (q, r) = divrem_small(a, b[0]);
        return (q, if r == 0 { Vec::new() } else { vec![r] });
    }
    // Normalize so the divisor's top limb has its high bit set
    let s = b[b.len() - 1].leading_zeros();
    let v = shl_mag(b, s as usize);
    let mut u = shl_mag(a, s as usize);
    if u.len() == a.len() {
        u.push(0);
    }
    let n = v.len();
    let m = u.len() - n - 1;
    let mut q = vec![0u32; m + 1];
    let b32 = 1u64 << 32;
    for j in (0..=m).rev() {
        let num = ((u[j + n] as u64) << 32) | u[j + n - 1] as u64;
        let mut qhat = num / v[n - 1] as u64;
        let mut rhat = num % v[n - 1] as u64;
        while qhat >= b32 || qhat * v[n - 2] as u64 > ((rhat << 32) | u[j + n - 2] as u64) {
            qhat -= 1;
            rhat += v[n - 1] as u64;
            if rhat >= b32 {
                break;
            }
        }
        // u[j..=j+n] -= qhat * v
        let mut borrow = 0i64;
        let mut carry = 0u64;
        for i in 0..n {
            let p = qhat * v[i] as u64 + carry;
            carry = p >> 32;
            let t = u[i + j] as i64 - borrow - (p & 0xFFFF_FFFF) as i64;
            u[i + j] = t as u32;
            borrow = if t < 0 { 1 } else { 0 };
        }
        let t = u[j + n] as i64 - borrow - carry as i64;
        u[j + n] = t as u32;
        if t < 0 {
            // qhat was one too large: add v back
            qhat -= 1;
            let mut c = 0u64;
            for i in 0..n {
                let s = u[i + j] as u64 + v[i] as u64 + c;
                u[i + j] = s as u32;
                c = s >> 32;
            }
            u[j + n] = u[j + n].wrapping_add(c as u32);
        }
        q[j] = qhat as u32;
    }
    trim(&mut q);
    u.truncate(n);
    let mut r = shr_mag(&u, s as usize);
    trim(&mut r);
    (q, r)
}

fn shl_mag(a: &[u32], bits: usize) -> Vec<u32> {
    if a.is_empty() {
        return Vec::new();
    }
    let (limbs, bits) = (bits / 32, bits % 32);
    let mut out = vec![0u32; limbs];
    if bits == 0 {
        out.extend_from_slice(a);
    } else {
        let mut carry = 0u32;
        for &x in a {
            out.push((x << bits) | carry);
            carry = x >> (32 - bits);
        }
        if carry != 0 {
            out.push(carry);
        }
    }
    out
}

/// Magnitude shifted right (truncating)
fn shr_mag(a: &[u32], bits: usize) -> Vec<u32> {
    let (limbs, bits) = (bits / 32, bits % 32);
    if limbs >= a.len() {
        return Vec::new();
    }
    let a = &a[limbs..];
    let mut out = Vec::with_capacity(a.len());
    for i in 0..a.len() {
        let hi = if bits == 0 { 0 } else { a.get(i + 1).map_or(0, |&h| h << (32 - bits)) };
        out.push((a[i] >> bits) | hi);
    }
    trim(&mut out);
    out
}

impl BigInt {
    pub fn zero() -> BigInt {
        BigInt::default()
    }

    fn from_parts(neg: bool, mut mag: Vec<u32>) -> BigInt {
        trim(&mut mag);
        let neg = neg && !mag.is_empty();
        BigInt { neg, mag }
    }

    pub fn from_i64(v: i64) -> BigInt {
        let m = v.unsigned_abs();
        BigInt::from_parts(v < 0, vec![m as u32, (m >> 32) as u32])
    }

    pub fn from_u64(v: u64) -> BigInt {
        BigInt::from_parts(false, vec![v as u32, (v >> 32) as u32])
    }

    /// Heap bytes of the limbs (for collection scheduling)
    pub fn limb_bytes(&self) -> usize {
        self.mag.capacity() * 4
    }

    pub fn is_zero(&self) -> bool {
        self.mag.is_empty()
    }

    pub fn is_negative(&self) -> bool {
        self.neg
    }

    /// An integral, finite number as a BigInt
    pub fn from_f64(f: f64) -> Option<BigInt> {
        if !f.is_finite() || f.trunc() != f {
            return None;
        }
        let neg = f < 0.0;
        let bits = f.abs().to_bits();
        let exp = ((bits >> 52) & 0x7FF) as i64;
        if exp == 0 {
            return Some(BigInt::zero());
        }
        let mantissa = (bits & ((1 << 52) - 1)) | (1 << 52);
        let shift = exp - 1075;
        let m = BigInt::from_u64(mantissa);
        let mag = if shift >= 0 { shl_mag(&m.mag, shift as usize) } else { shr_mag(&m.mag, (-shift) as usize) };
        Some(BigInt::from_parts(neg, mag))
    }

    fn bit_length(&self) -> usize {
        match self.mag.last() {
            None => 0,
            Some(&top) => self.mag.len() * 32 - top.leading_zeros() as usize,
        }
    }

    /// The nearest number (ties to even)
    pub fn to_f64(&self) -> f64 {
        let len = self.bit_length();
        let mag = if len <= 64 {
            let lo = *self.mag.first().unwrap_or(&0) as u64;
            let hi = *self.mag.get(1).unwrap_or(&0) as u64;
            ((hi << 32) | lo) as f64
        } else {
            // Top 64 bits, with any lower set bit folded into the lowest
            // (below the rounding position, so rounding stays correct)
            let shift = len - 64;
            let top = shr_mag(&self.mag, shift);
            let mut m = (top[0] as u64) | ((*top.get(1).unwrap_or(&0) as u64) << 32);
            let dropped = shl_mag(&top, shift);
            if cmp_mag(&dropped, &self.mag) != Ordering::Equal {
                m |= 1;
            }
            if shift > 1100 {
                f64::INFINITY
            } else {
                (m as f64) * 2f64.powi(shift as i32)
            }
        };
        if self.neg { -mag } else { mag }
    }

    pub fn neg(&self) -> BigInt {
        BigInt::from_parts(!self.neg, self.mag.clone())
    }

    pub fn add(&self, o: &BigInt) -> BigInt {
        if self.neg == o.neg {
            return BigInt::from_parts(self.neg, add_mag(&self.mag, &o.mag));
        }
        match cmp_mag(&self.mag, &o.mag) {
            Ordering::Equal => BigInt::zero(),
            Ordering::Greater => BigInt::from_parts(self.neg, sub_mag(&self.mag, &o.mag)),
            Ordering::Less => BigInt::from_parts(o.neg, sub_mag(&o.mag, &self.mag)),
        }
    }

    pub fn sub(&self, o: &BigInt) -> BigInt {
        self.add(&o.neg())
    }

    pub fn mul(&self, o: &BigInt) -> BigInt {
        BigInt::from_parts(self.neg != o.neg, mul_mag(&self.mag, &o.mag))
    }

    /// Truncating division and remainder (sign of the dividend); None for
    /// division by zero
    pub fn divrem(&self, o: &BigInt) -> Option<(BigInt, BigInt)> {
        if o.is_zero() {
            return None;
        }
        let (q, r) = divrem_mag(&self.mag, &o.mag);
        Some((BigInt::from_parts(self.neg != o.neg, q), BigInt::from_parts(self.neg, r)))
    }

    /// `self ** exp`; None for negative exponents
    pub fn pow(&self, exp: &BigInt) -> Option<BigInt> {
        if exp.neg {
            return None;
        }
        if exp.bit_length() > 32 {
            // Only 0, 1 and -1 stay representable
            return match (self.mag.as_slice(), self.neg) {
                ([], _) | ([1], false) => Some(self.clone()),
                ([1], true) => Some(if exp.mag[0] & 1 == 1 { self.clone() } else { BigInt::from_i64(1) }),
                _ => None,
            };
        }
        let mut e = exp.mag.first().copied().unwrap_or(0);
        let mut base = self.clone();
        let mut out = BigInt::from_i64(1);
        while e > 0 {
            if e & 1 == 1 {
                out = out.mul(&base);
            }
            e >>= 1;
            if e > 0 {
                base = base.mul(&base);
            }
        }
        Some(out)
    }

    /// `self << n` (negative `n` shifts right, rounding toward -infinity)
    pub fn shl(&self, n: i64) -> BigInt {
        if n >= 0 {
            return BigInt::from_parts(self.neg, shl_mag(&self.mag, n as usize));
        }
        let n = n.unsigned_abs() as usize;
        let q = shr_mag(&self.mag, n);
        let mut r = BigInt::from_parts(self.neg, q);
        // Floor for negatives: subtract one if any bit was shifted out
        if self.neg && cmp_mag(&shl_mag(&shr_mag(&self.mag, n), n), &self.mag) != Ordering::Equal {
            r = r.sub(&BigInt::from_i64(1));
        }
        r
    }

    /// Two's complement limbs, `len` long
    fn twos(&self, len: usize) -> Vec<u32> {
        let mut out = self.mag.clone();
        out.resize(len, 0);
        if self.neg {
            let mut carry = 1u64;
            for x in &mut out {
                let s = (!*x) as u64 + carry;
                *x = s as u32;
                carry = s >> 32;
            }
        }
        out
    }

    fn from_twos(mut limbs: Vec<u32>) -> BigInt {
        let neg = limbs.last().is_some_and(|&t| t & 0x8000_0000 != 0);
        if neg {
            let mut carry = 1u64;
            for x in &mut limbs {
                let s = (!*x) as u64 + carry;
                *x = s as u32;
                carry = s >> 32;
            }
        }
        BigInt::from_parts(neg, limbs)
    }

    fn bitwise(&self, o: &BigInt, f: impl Fn(u32, u32) -> u32) -> BigInt {
        let len = self.mag.len().max(o.mag.len()) + 1;
        let (a, b) = (self.twos(len), o.twos(len));
        BigInt::from_twos(a.iter().zip(&b).map(|(&x, &y)| f(x, y)).collect())
    }

    pub fn and(&self, o: &BigInt) -> BigInt {
        self.bitwise(o, |x, y| x & y)
    }

    pub fn or(&self, o: &BigInt) -> BigInt {
        self.bitwise(o, |x, y| x | y)
    }

    pub fn xor(&self, o: &BigInt) -> BigInt {
        self.bitwise(o, |x, y| x ^ y)
    }

    /// `~self` = -self - 1
    pub fn not(&self) -> BigInt {
        self.neg().sub(&BigInt::from_i64(1))
    }

    /// `self mod 2^bits`, as an unsigned value
    pub fn as_uint_n(&self, bits: u64) -> BigInt {
        if bits == 0 {
            return BigInt::zero();
        }
        let limbs = (bits as usize).div_ceil(32);
        let mut t = self.twos(limbs.max(self.mag.len() + 1));
        t.truncate(limbs);
        let extra = limbs * 32 - bits as usize;
        if extra > 0 {
            if let Some(top) = t.last_mut() {
                *top &= u32::MAX >> extra;
            }
        }
        BigInt::from_parts(false, t)
    }

    /// `self mod 2^bits`, as a signed value
    pub fn as_int_n(&self, bits: u64) -> BigInt {
        if bits == 0 {
            return BigInt::zero();
        }
        let u = self.as_uint_n(bits);
        if u.bit_length() as u64 == bits {
            u.sub(&BigInt::from_i64(1).shl(bits as i64))
        } else {
            u
        }
    }

    /// The low 64 bits, two's complement (BigInt64Array)
    pub fn to_u64_wrapping(&self) -> u64 {
        let t = self.twos(2.max(self.mag.len() + 1));
        t[0] as u64 | ((t[1] as u64) << 32)
    }

    pub fn cmp(&self, o: &BigInt) -> Ordering {
        match (self.neg, o.neg) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => cmp_mag(&self.mag, &o.mag),
            (true, true) => cmp_mag(&o.mag, &self.mag),
        }
    }

    /// Compare with a number (None when it is NaN)
    pub fn cmp_f64(&self, f: f64) -> Option<Ordering> {
        if f.is_nan() {
            return None;
        }
        if f.is_infinite() {
            return Some(if f > 0.0 { Ordering::Less } else { Ordering::Greater });
        }
        let floor = f.floor();
        let c = self.cmp(&BigInt::from_f64(floor)?);
        Some(match c {
            Ordering::Equal if floor != f => Ordering::Less,
            c => c,
        })
    }

    pub fn to_string_radix(&self, radix: u32) -> String {
        if self.is_zero() {
            return "0".into();
        }
        // Peel off as many digits as fit in a limb at a time
        let mut chunk = radix;
        let mut per = 1;
        while (chunk as u64) * (radix as u64) <= u32::MAX as u64 {
            chunk *= radix;
            per += 1;
        }
        let mut digits: Vec<u8> = Vec::new();
        let mut cur = self.mag.clone();
        while !cur.is_empty() {
            let (q, mut r) = divrem_small(&cur, chunk);
            for _ in 0..per {
                if q.is_empty() && r == 0 {
                    break;
                }
                digits.push(std::char::from_digit(r % radix, radix).unwrap_or('0') as u8);
                r /= radix;
            }
            cur = q;
        }
        if self.neg {
            digits.push(b'-');
        }
        digits.reverse();
        String::from_utf8(digits).unwrap_or_default()
    }

    /// StringToBigInt: decimal (with sign), or 0x/0o/0b; surrounding white
    /// space is ignored and the empty string is 0. None if invalid.
    pub fn parse(s: &str) -> Option<BigInt> {
        let s = s.trim_matches(|c: char| c.is_whitespace() || c == '\u{FEFF}');
        if s.is_empty() {
            return Some(BigInt::zero());
        }
        let lower = s.get(..2).map(|p| p.to_ascii_lowercase());
        let (radix, digits, neg) = match lower.as_deref() {
            Some("0x") => (16, &s[2..], false),
            Some("0o") => (8, &s[2..], false),
            Some("0b") => (2, &s[2..], false),
            _ => match s.strip_prefix('-') {
                Some(d) => (10, d, true),
                None => (10, s.strip_prefix('+').unwrap_or(s), false),
            },
        };
        BigInt::parse_digits(digits, radix).map(|b| if neg { b.neg() } else { b })
    }

    /// Digits in `radix`, nothing else (None if empty or invalid)
    pub fn parse_digits(digits: &str, radix: u32) -> Option<BigInt> {
        if digits.is_empty() {
            return None;
        }
        let mut mag: Vec<u32> = Vec::new();
        for c in digits.chars() {
            let d = c.to_digit(radix)?;
            // mag = mag * radix + d
            let mut carry = d as u64;
            for x in &mut mag {
                let t = *x as u64 * radix as u64 + carry;
                *x = t as u32;
                carry = t >> 32;
            }
            if carry != 0 {
                mag.push(carry as u32);
            }
        }
        Some(BigInt::from_parts(false, mag))
    }

    /// Value as i64, if it fits
    pub fn to_i64(&self) -> Option<i64> {
        if self.bit_length() > 63 {
            return (self.neg && self.bit_length() == 64 && self.mag == [0, 0x8000_0000]).then_some(i64::MIN);
        }
        let m = self.mag.first().copied().unwrap_or(0) as i64 | ((self.mag.get(1).copied().unwrap_or(0) as i64) << 32);
        Some(if self.neg { -m } else { m })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(s: &str) -> BigInt {
        BigInt::parse(s).unwrap()
    }

    #[test]
    fn arithmetic() {
        let x = b("123456789012345678901234567890");
        let y = b("-987654321098765432109876543210");
        assert_eq!(x.add(&y).to_string_radix(10), "-864197532086419753208641975320");
        assert_eq!(x.sub(&y).to_string_radix(10), "1111111110111111111011111111100");
        assert_eq!(x.mul(&y).to_string_radix(10), "-121932631137021795226185032733622923332237463801111263526900");
        let (q, r) = y.divrem(&x).unwrap();
        assert_eq!((q.to_string_radix(10).as_str(), r.to_string_radix(10).as_str()), ("-8", "-9000000000900000000090"));
        assert!(x.divrem(&BigInt::zero()).is_none());
        assert_eq!(b("2").pow(&b("100")).unwrap().to_string_radix(10), "1267650600228229401496703205376");
        assert_eq!(b("-1").pow(&b("99999999999")).unwrap().to_string_radix(10), "-1");
        assert!(b("2").pow(&b("-1")).is_none());
        assert_eq!(b("255").to_string_radix(16), "ff");
        // A sign only goes with decimal digits
        assert!(BigInt::parse("-0x10").is_none());
    }

    #[test]
    fn division_matches_multiplication() {
        // q*d + r == n, |r| < |d|, over many shapes
        let ns = ["0", "1", "4294967295", "4294967296", "18446744073709551615", "340282366920938463463374607431768211457",
            "-98765432109876543210987654321098765432109876543210", "79228162514264337593543950335"];
        let ds = ["1", "3", "4294967291", "4294967296", "-18446744073709551557", "79228162514264337593543950335", "123456789123456789123"];
        for n in ns {
            for d in ds {
                let (n, d) = (b(n), b(d));
                let (q, r) = n.divrem(&d).unwrap();
                assert_eq!(q.mul(&d).add(&r), n, "{n:?} / {d:?}");
                assert_eq!(cmp_mag(&r.mag, &d.mag), Ordering::Less);
                assert!(r.is_zero() || r.neg == n.neg);
            }
        }
    }

    #[test]
    fn bits_and_conversions() {
        assert_eq!(b("-5").and(&b("3")).to_string_radix(10), "3");
        assert_eq!(b("-5").or(&b("3")).to_string_radix(10), "-5");
        assert_eq!(b("-5").xor(&b("3")).to_string_radix(10), "-8");
        assert_eq!(b("5").not().to_string_radix(10), "-6");
        assert_eq!(b("-9").shl(-1).to_string_radix(10), "-5");
        assert_eq!(b("9").shl(-1).to_string_radix(10), "4");
        assert_eq!(b("1").shl(70).to_string_radix(10), "1180591620717411303424");
        assert_eq!(b("255").as_int_n(8).to_string_radix(10), "-1");
        assert_eq!(b("-1").as_uint_n(64).to_string_radix(10), "18446744073709551615");
        assert_eq!(b("-1").as_uint_n(0).to_string_radix(10), "0");
        assert_eq!(b("12345678901234567890123").to_f64(), 1.2345678901234568e22);
        assert_eq!(b("9007199254740993").to_f64(), 9007199254740992.0);
        assert_eq!(b("9007199254740995").to_f64(), 9007199254740996.0);
        assert_eq!(BigInt::from_f64(1e21).unwrap().to_string_radix(10), "1000000000000000000000");
        assert!(BigInt::from_f64(1.5).is_none());
        assert_eq!(b("3").cmp_f64(3.5), Some(Ordering::Less));
        assert_eq!(b("-3").cmp_f64(-3.5), Some(Ordering::Greater));
        assert_eq!(b("3").cmp_f64(f64::NAN), None);
        assert_eq!(b(" 0x1F ").to_string_radix(10), "31");
        assert!(BigInt::parse("1.5").is_none() && BigInt::parse("1n").is_none());
        assert_eq!(b("-9223372036854775808").to_i64(), Some(i64::MIN));
        assert_eq!(b("-1").to_u64_wrapping(), u64::MAX);
    }
}
