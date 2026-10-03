//! CSS Transforms: `transform` function lists, `transform-origin`, and the
//! individual `translate`, `rotate` and `scale` properties, reduced to 2D
//! affine matrices when painting (3D functions keep their 2D part)

use crate::style::Lp;

/// One transform function, lengths computed (percentages of the border box)
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransformFn {
    Translate(Lp, Lp),
    Scale(f32, f32),
    /// Clockwise, in radians
    Rotate(f32),
    /// Radians
    Skew(f32, f32),
    Matrix([f32; 6]),
}

/// A 2D affine matrix `[a, b, c, d, e, f]`: (x, y) maps to
/// (a·x + c·y + e, b·x + d·y + f)
pub type Matrix = [f32; 6];

pub const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// `m · n` (n applies first)
pub fn multiply(m: Matrix, n: Matrix) -> Matrix {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

pub fn invert(m: Matrix) -> Option<Matrix> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-9 || !det.is_finite() {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])])
}

pub fn apply(m: Matrix, (x, y): (f32, f32)) -> (f32, f32) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

impl TransformFn {
    /// The function's matrix for a box `w` × `h`
    pub fn matrix(self, w: f32, h: f32) -> Matrix {
        match self {
            TransformFn::Translate(x, y) => [1.0, 0.0, 0.0, 1.0, x.resolve(w), y.resolve(h)],
            TransformFn::Scale(x, y) => [x, 0.0, 0.0, y, 0.0, 0.0],
            TransformFn::Rotate(a) => {
                let (s, c) = a.sin_cos();
                [c, s, -s, c, 0.0, 0.0]
            }
            TransformFn::Skew(x, y) => [1.0, y.tan(), x.tan(), 1.0, 0.0, 0.0],
            TransformFn::Matrix(m) => m,
        }
    }
}

/// An angle in radians (`deg`, `rad`, `grad`, `turn`, or unitless `0`)
pub fn angle(v: &str) -> Option<f32> {
    let v = v.trim().to_ascii_lowercase();
    let num = |s: &str| s.trim().parse::<f32>().ok().filter(|n| n.is_finite());
    let a = if let Some(n) = v.strip_suffix("deg") {
        num(n)?.to_radians()
    } else if let Some(n) = v.strip_suffix("grad") {
        num(n)? * std::f32::consts::PI / 200.0
    } else if let Some(n) = v.strip_suffix("rad") {
        num(n)?
    } else if let Some(n) = v.strip_suffix("turn") {
        num(n)? * std::f32::consts::TAU
    } else if num(&v)? == 0.0 {
        0.0
    } else {
        return None;
    };
    Some(a)
}

/// A number, or a percentage as a fraction (for scale)
fn factor(v: &str) -> Option<f32> {
    let v = v.trim();
    let n = match v.strip_suffix('%') {
        Some(p) => p.trim().parse::<f32>().ok()? / 100.0,
        None => v.parse::<f32>().ok()?,
    };
    n.is_finite().then_some(n)
}

/// Split a function's arguments at top-level commas or white space
fn args(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0, 0);
    let b = s.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' | b' ' | b'\t' | b'\n' if depth == 0 => {
                if i > start {
                    out.push(&s[start..i]);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// Each `name(args)` of a function list, or `None` if malformed
fn functions(text: &str) -> Option<Vec<(String, &str)>> {
    let mut out = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let open = rest.find('(')?;
        let name = rest[..open].trim().to_ascii_lowercase();
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return None;
        }
        let mut depth = 0;
        let mut close = None;
        for (i, c) in rest[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + i);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close?;
        out.push((name, &rest[open + 1..close]));
        rest = rest[close + 1..].trim_start();
    }
    Some(out)
}

/// Parse a `transform` value (`none` is an empty list); `len` computes a
/// length or percentage
pub fn parse_list(text: &str, len: &dyn Fn(&str) -> Option<Lp>) -> Option<Vec<TransformFn>> {
    if text.trim().eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    for (name, a) in functions(text)? {
        let a = args(a);
        let n = |i: usize| a.get(i).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite());
        let l = |i: usize| a.get(i).and_then(|v| len(v.trim()));
        let f = match (name.as_str(), a.len()) {
            ("translate", 1) => TransformFn::Translate(l(0)?, Lp::ZERO),
            ("translate", 2) | ("translate3d", 3) => TransformFn::Translate(l(0)?, l(1)?),
            ("translatex", 1) => TransformFn::Translate(l(0)?, Lp::ZERO),
            ("translatey", 1) => TransformFn::Translate(Lp::ZERO, l(0)?),
            ("translatez", 1) => continue,
            ("scale", 1) => {
                let s = factor(a[0])?;
                TransformFn::Scale(s, s)
            }
            ("scale", 2) | ("scale3d", 3) => TransformFn::Scale(factor(a[0])?, factor(a[1])?),
            ("scalex", 1) => TransformFn::Scale(factor(a[0])?, 1.0),
            ("scaley", 1) => TransformFn::Scale(1.0, factor(a[0])?),
            ("scalez", 1) => continue,
            ("rotate" | "rotatez", 1) => TransformFn::Rotate(angle(a[0])?),
            // Rotations about other axes have no 2D part but a squash;
            // approximated as none
            ("rotatex" | "rotatey", 1) | ("rotate3d", 4) | ("perspective", 1) => continue,
            ("skew", 1) => TransformFn::Skew(angle(a[0])?, 0.0),
            ("skew", 2) => TransformFn::Skew(angle(a[0])?, angle(a[1])?),
            ("skewx", 1) => TransformFn::Skew(angle(a[0])?, 0.0),
            ("skewy", 1) => TransformFn::Skew(0.0, angle(a[0])?),
            ("matrix", 6) => TransformFn::Matrix([n(0)?, n(1)?, n(2)?, n(3)?, n(4)?, n(5)?]),
            ("matrix3d", 16) => TransformFn::Matrix([n(0)?, n(1)?, n(4)?, n(5)?, n(12)?, n(13)?]),
            _ => return None,
        };
        out.push(f);
    }
    Some(out)
}

/// `transform-origin`: x and y (a z offset is ignored)
pub fn parse_origin(text: &str, len: &dyn Fn(&str) -> Option<Lp>) -> Option<(Lp, Lp)> {
    let parts: Vec<String> = text.split_whitespace().map(|p| p.to_ascii_lowercase()).collect();
    let pct = |p: f32| Lp { px: 0.0, pct: p };
    let kw = |v: &str| match v {
        "left" | "top" => Some(pct(0.0)),
        "center" => Some(pct(50.0)),
        "right" | "bottom" => Some(pct(100.0)),
        _ => None,
    };
    let (x, y) = match parts.as_slice() {
        [one] => match one.as_str() {
            "top" | "bottom" => (pct(50.0), kw(one)?),
            v => (kw(v).or_else(|| len(v))?, pct(50.0)),
        },
        [a, b, ..] => {
            // Vertical keyword first: swap
            if matches!(a.as_str(), "top" | "bottom") || matches!(b.as_str(), "left" | "right") {
                (kw(b).or_else(|| len(b))?, kw(a).or_else(|| len(a))?)
            } else {
                (kw(a).or_else(|| len(a))?, kw(b).or_else(|| len(b))?)
            }
        }
        [] => return None,
    };
    Some((x, y))
}

/// The individual `translate` property: one or two lengths
pub fn parse_translate(text: &str, len: &dyn Fn(&str) -> Option<Lp>) -> Option<Option<(Lp, Lp)>> {
    let a = args(text.trim());
    match a.as_slice() {
        [none] if none.eq_ignore_ascii_case("none") => Some(None),
        [x] => Some(Some((len(x)?, Lp::ZERO))),
        [x, y] | [x, y, _] => Some(Some((len(x)?, len(y)?))),
        _ => None,
    }
}

/// The individual `scale` property: one or two factors
pub fn parse_scale(text: &str) -> Option<Option<(f32, f32)>> {
    let a = args(text.trim());
    match a.as_slice() {
        [none] if none.eq_ignore_ascii_case("none") => Some(None),
        [s] => factor(s).map(|s| Some((s, s))),
        [x, y] | [x, y, _] => Some(Some((factor(x)?, factor(y)?))),
        _ => None,
    }
}

/// The individual `rotate` property: an angle (about z), or `z <angle>`
pub fn parse_rotate(text: &str) -> Option<Option<f32>> {
    let a = args(text.trim());
    match a.as_slice() {
        [none] if none.eq_ignore_ascii_case("none") => Some(None),
        [v] => angle(v).map(Some),
        [axis, v] if axis.eq_ignore_ascii_case("z") => angle(v).map(Some),
        _ => None,
    }
}

/// Whether a `transform` value parses (lengths checked loosely)
pub fn valid(text: &str) -> bool {
    parse_list(text, &|v| crate::parser::parse_length(v).map(|_| Lp::ZERO).or((v == "0" || v.starts_with("-fos-mix(")).then_some(Lp::ZERO))).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn len(v: &str) -> Option<Lp> {
        if let Some(p) = v.strip_suffix('%') {
            return Some(Lp { px: 0.0, pct: p.parse().ok()? });
        }
        v.trim_end_matches("px").parse().ok().map(|px| Lp { px, pct: 0.0 })
    }

    fn close(a: Matrix, b: Matrix) -> bool {
        a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn functions_compose_in_order() {
        let fns = parse_list("translate(-50%, 10px) rotate(90deg) scale(2)", &len).unwrap();
        let m = fns.iter().fold(IDENTITY, |m, f| multiply(m, f.matrix(100.0, 40.0)));
        // (1, 0): scaled to (2, 0), rotated to (0, 2), moved by (-50, 10)
        let (x, y) = apply(m, (1.0, 0.0));
        assert!((x + 50.0).abs() < 1e-4 && (y - 12.0).abs() < 1e-4, "{x} {y}");
        assert!(close(multiply(m, invert(m).unwrap()), IDENTITY));
        assert_eq!(parse_list("none", &len), Some(vec![]));
        assert_eq!(parse_list("translateX(5px) translateZ(3px)", &len), Some(vec![TransformFn::Translate(len("5px").unwrap(), Lp::ZERO)]));
        assert!(parse_list("bogus(1)", &len).is_none());
        assert!(parse_list("rotate(45)", &len).is_none());
        assert_eq!(angle("0.25turn").map(|a| (a.to_degrees() * 10.0).round()), Some(900.0));
    }

    #[test]
    fn origins_and_individual_properties() {
        let pct = |p| Lp { px: 0.0, pct: p };
        assert_eq!(parse_origin("left top", &len), Some((pct(0.0), pct(0.0))));
        assert_eq!(parse_origin("bottom right", &len), Some((pct(100.0), pct(100.0))));
        assert_eq!(parse_origin("top", &len), Some((pct(50.0), pct(0.0))));
        assert_eq!(parse_origin("10px 20%", &len), Some((len("10px").unwrap(), pct(20.0))));
        assert_eq!(parse_translate("10px", &len), Some(Some((len("10px").unwrap(), Lp::ZERO))));
        assert_eq!(parse_scale("1.5"), Some(Some((1.5, 1.5))));
        assert_eq!(parse_rotate("none"), Some(None));
        assert!(parse_rotate("45deg").unwrap().is_some());
    }
}
