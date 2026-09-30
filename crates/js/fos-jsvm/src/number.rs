//! Number <-> string conversions following the ECMAScript algorithms

/// `Number::toString(x)` in radix 10: the shortest digits that round-trip,
/// laid out as the specification requires (plain up to 1e21, exponential
/// beyond, `0.000001` down to 1e-6)
pub fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x == 0.0 {
        return "0".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity".into() } else { "-Infinity".into() };
    }
    if x < 0.0 {
        return format!("-{}", number_to_string(-x));
    }
    // Integers that fit exactly: fast path
    if x < 1e21 && x.fract() == 0.0 && x < 9007199254740992.0 {
        return format!("{}", x as u64);
    }

    // Rust's `{:e}` gives the shortest round-trip digits: "d.ddde±x"
    let sci = format!("{:e}", x);
    let (mantissa, exp) = sci.split_once('e').unwrap();
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp.parse::<i32>().unwrap() + 1;

    if k <= n && n <= 21 {
        let mut out = digits;
        out.extend(std::iter::repeat('0').take((n - k) as usize));
        out
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{}", "0".repeat((-n) as usize), digits)
    } else {
        let e = n - 1;
        let sign = if e >= 0 { '+' } else { '-' };
        if k == 1 {
            format!("{}e{}{}", digits, sign, e.abs())
        } else {
            format!("{}.{}e{}{}", &digits[..1], &digits[1..], sign, e.abs())
        }
    }
}

/// `StringToNumber`: whitespace-trimmed decimal, `0x`/`0o`/`0b` integers,
/// `Infinity`; anything else is NaN. An empty string is 0.
pub fn string_to_number(s: &str) -> f64 {
    let t = s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    if t.is_empty() {
        return 0.0;
    }
    let radix = |digits: &str, radix: u32| -> f64 {
        if digits.is_empty() {
            return f64::NAN;
        }
        let mut v = 0f64;
        for c in digits.chars() {
            match c.to_digit(radix) {
                Some(d) => v = v * radix as f64 + d as f64,
                None => return f64::NAN,
            }
        }
        v
    };
    if t.len() > 2 && t.as_bytes()[0] == b'0' {
        match t.as_bytes()[1] | 0x20 {
            b'x' => return radix(&t[2..], 16),
            b'o' => return radix(&t[2..], 8),
            b'b' => return radix(&t[2..], 2),
            _ => {}
        }
    }
    let (sign, body) = match t.as_bytes()[0] {
        b'+' => (1.0, &t[1..]),
        b'-' => (-1.0, &t[1..]),
        _ => (1.0, t),
    };
    if body == "Infinity" {
        return sign * f64::INFINITY;
    }
    // Only decimal literal syntax: digits, optional fraction, optional exponent
    let bytes = body.as_bytes();
    let mut i = 0;
    let mut digits = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return f64::NAN;
    }
    if i < bytes.len() && (bytes[i] | 0x20) == b'e' {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return f64::NAN;
        }
    }
    if i != bytes.len() {
        return f64::NAN;
    }
    sign * body.parse::<f64>().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_number_to_string() {
        let cases = [
            (0.0, "0"), (-0.0, "0"), (1.0, "1"), (-1.5, "-1.5"), (0.1, "0.1"), (123.456, "123.456"),
            (1e21, "1e+21"), (1e20, "100000000000000000000"), (1.5e300, "1.5e+300"), (1e-6, "0.000001"),
            (1e-7, "1e-7"), (1.2e-7, "1.2e-7"), (0.1 + 0.2, "0.30000000000000004"), (5e-324, "5e-324"),
            (f64::MAX, "1.7976931348623157e+308"), (f64::NAN, "NaN"), (f64::INFINITY, "Infinity"),
            (9007199254740993.0, "9007199254740992"), (2f64.powi(60), "1152921504606847000"),
        ];
        for (x, want) in cases {
            assert_eq!(number_to_string(x), want, "{x:e}");
        }
    }

    #[test]
    fn test_string_to_number() {
        assert_eq!(string_to_number("  42  "), 42.0);
        assert_eq!(string_to_number(""), 0.0);
        assert_eq!(string_to_number("0x1F"), 31.0);
        assert_eq!(string_to_number("-Infinity"), f64::NEG_INFINITY);
        assert_eq!(string_to_number(".5e1"), 5.0);
        assert!(string_to_number("12px").is_nan());
        assert!(string_to_number("0x").is_nan());
        assert!(string_to_number("1e").is_nan());
        assert!(string_to_number("-0x10").is_nan());
    }
}
