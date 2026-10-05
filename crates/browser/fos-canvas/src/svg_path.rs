//! SVG path data (`new Path2D("M 0 0 L 10 10 Z")`) as canvas path commands
//!
//! Follows the SVG 2 grammar: implicit repeated commands (and `L` after
//! `M`), numbers without separators (`1-2`, `.5.5`), packed arc flags, and
//! rendering up to the first error.

use std::f32::consts::{PI, TAU};

use crate::canvas2d::PathCmd;

struct Scanner<'a> {
    s: &'a [u8],
    i: usize,
}

impl Scanner<'_> {
    fn skip_separators(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }

    fn skip_space(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn number(&mut self) -> Option<f32> {
        self.skip_separators();
        let start = self.i;
        let s = self.s;
        let mut i = self.i;
        if i < s.len() && matches!(s[i], b'+' | b'-') {
            i += 1;
        }
        let digits = |i: &mut usize| {
            let from = *i;
            while *i < s.len() && s[*i].is_ascii_digit() {
                *i += 1;
            }
            *i > from
        };
        let int = digits(&mut i);
        let mut frac = false;
        if i < s.len() && s[i] == b'.' {
            i += 1;
            frac = digits(&mut i);
        }
        if !int && !frac {
            return None;
        }
        if i < s.len() && matches!(s[i], b'e' | b'E') {
            let mut j = i + 1;
            if j < s.len() && matches!(s[j], b'+' | b'-') {
                j += 1;
            }
            if digits(&mut j) {
                i = j;
            }
        }
        let v: f32 = std::str::from_utf8(&s[start..i]).ok()?.parse().ok()?;
        self.i = i;
        v.is_finite().then_some(v)
    }

    fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        let f = match self.s.get(self.i)? {
            b'0' => false,
            b'1' => true,
            _ => return None,
        };
        self.i += 1;
        Some(f)
    }

    /// Whether another number of the same command follows
    fn number_ahead(&mut self) -> bool {
        self.skip_separators();
        self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.'))
    }
}

/// The commands of SVG path data, up to the first error
pub fn parse_svg_path(d: &str) -> Vec<PathCmd> {
    let mut out = Vec::new();
    let mut sc = Scanner { s: d.as_bytes(), i: 0 };
    let (mut cur, mut start) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    // Last control point, for S/T reflections
    let mut last_cubic: Option<(f32, f32)> = None;
    let mut last_quad: Option<(f32, f32)> = None;
    sc.skip_space();
    let mut cmd = match sc.s.first() {
        Some(b'M' | b'm') => 0u8,
        _ => return out,
    };
    loop {
        sc.skip_space();
        if sc.i < sc.s.len() && sc.s[sc.i].is_ascii_alphabetic() {
            cmd = sc.s[sc.i];
            sc.i += 1;
        } else if sc.i >= sc.s.len() || cmd == 0 {
            break;
        } else if !sc.number_ahead() || matches!(cmd, b'Z' | b'z') {
            break;
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { cur } else { (0.0, 0.0) };
        let ok = (|| -> Option<()> {
            match cmd.to_ascii_uppercase() {
                b'M' => {
                    let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                    out.push(PathCmd::MoveTo(x, y));
                    cur = (x, y);
                    start = cur;
                    // Further pairs are line-tos
                    cmd = if rel { b'l' } else { b'L' };
                    last_cubic = None;
                    last_quad = None;
                }
                b'L' => {
                    let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                    out.push(PathCmd::LineTo(x, y));
                    cur = (x, y);
                    last_cubic = None;
                    last_quad = None;
                }
                b'H' => {
                    let x = sc.number()? + ox;
                    out.push(PathCmd::LineTo(x, cur.1));
                    cur.0 = x;
                    last_cubic = None;
                    last_quad = None;
                }
                b'V' => {
                    let y = sc.number()? + oy;
                    out.push(PathCmd::LineTo(cur.0, y));
                    cur.1 = y;
                    last_cubic = None;
                    last_quad = None;
                }
                b'C' => {
                    let (x1, y1) = (sc.number()? + ox, sc.number()? + oy);
                    let (x2, y2) = (sc.number()? + ox, sc.number()? + oy);
                    let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                    out.push(PathCmd::CubicTo(x1, y1, x2, y2, x, y));
                    cur = (x, y);
                    last_cubic = Some((x2, y2));
                    last_quad = None;
                }
                b'S' => {
                    let (x1, y1) = match last_cubic {
                        Some((cx, cy)) => (2.0 * cur.0 - cx, 2.0 * cur.1 - cy),
                        None => cur,
                    };
                    let (x2, y2) = (sc.number()? + ox, sc.number()? + oy);
                    let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                    out.push(PathCmd::CubicTo(x1, y1, x2, y2, x, y));
                    cur = (x, y);
                    last_cubic = Some((x2, y2));
                    last_quad = None;
                }
                b'Q' => {
                    let (x1, y1) = (sc.number()? + ox, sc.number()? + oy);
                    let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                    out.push(PathCmd::QuadTo(x1, y1, x, y));
                    cur = (x, y);
                    last_quad = Some((x1, y1));
                    last_cubic = None;
                }
                b'T' => {
                    let (x1, y1) = match last_quad {
                        Some((cx, cy)) => (2.0 * cur.0 - cx, 2.0 * cur.1 - cy),
                        None => cur,
                    };
                    let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                    out.push(PathCmd::QuadTo(x1, y1, x, y));
                    cur = (x, y);
                    last_quad = Some((x1, y1));
                    last_cubic = None;
                }
                b'A' => {
                    let (rx, ry, rot) = (sc.number()?, sc.number()?, sc.number()?);
                    let (large, sweep) = (sc.flag()?, sc.flag()?);
                    let (x, y) = (sc.number()? + ox, sc.number()? + oy);
                    if let Some(c) = arc(cur, rx, ry, rot, large, sweep, (x, y)) {
                        out.push(c);
                    }
                    cur = (x, y);
                    last_cubic = None;
                    last_quad = None;
                }
                b'Z' => {
                    out.push(PathCmd::Close);
                    cur = start;
                    last_cubic = None;
                    last_quad = None;
                }
                _ => return None,
            }
            Some(())
        })();
        if ok.is_none() {
            break;
        }
    }
    out
}

/// An SVG endpoint arc as a center-parameterized ellipse (SVG 2, B.2.4)
fn arc((x1, y1): (f32, f32), rx: f32, ry: f32, rotation_deg: f32, large: bool, sweep: bool, (x2, y2): (f32, f32)) -> Option<PathCmd> {
    if x1 == x2 && y1 == y2 {
        return None;
    }
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        return Some(PathCmd::LineTo(x2, y2));
    }
    let phi = rotation_deg.rem_euclid(360.0) * PI / 180.0;
    let (sin, cos) = phi.sin_cos();
    let (dx, dy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
    let (x1p, y1p) = (cos * dx + sin * dy, -sin * dx + cos * dy);
    // Radii too small to reach: scale them up
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = if den > 0.0 { (num / den).max(0.0).sqrt() } else { 0.0 };
    if large == sweep {
        coef = -coef;
    }
    let (cxp, cyp) = (coef * rx * y1p / ry, -coef * ry * x1p / rx);
    let (cx, cy) = (cos * cxp - sin * cyp + (x1 + x2) / 2.0, sin * cxp + cos * cyp + (y1 + y2) / 2.0);
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let (ux, uy) = ((x1p - cxp) / rx, (y1p - cyp) / ry);
    let (vx, vy) = ((-x1p - cxp) / rx, (-y1p - cyp) / ry);
    let start = angle(1.0, 0.0, ux, uy);
    let mut delta = angle(ux, uy, vx, vy);
    if !sweep && delta > 0.0 {
        delta -= TAU;
    } else if sweep && delta < 0.0 {
        delta += TAU;
    }
    Some(PathCmd::Ellipse { x: cx, y: cy, rx, ry, rotation: phi, start, end: start + delta, ccw: !sweep })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas2d::{build_path, Canvas2D};
    use tiny_skia as sk;

    fn summary(d: &str) -> String {
        parse_svg_path(d)
            .iter()
            .map(|c| match c {
                PathCmd::MoveTo(x, y) => format!("M{x},{y}"),
                PathCmd::LineTo(x, y) => format!("L{x},{y}"),
                PathCmd::QuadTo(a, b, x, y) => format!("Q{a},{b},{x},{y}"),
                PathCmd::CubicTo(a, b, c, d, x, y) => format!("C{a},{b},{c},{d},{x},{y}"),
                PathCmd::Ellipse { .. } => "A".to_string(),
                PathCmd::Close => "Z".to_string(),
                _ => "?".to_string(),
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    #[test]
    fn grammar() {
        assert_eq!(summary("M10 20L30,40"), "M10,20 L30,40");
        // Implicit line-tos after a move, relative commands, H/V, close
        assert_eq!(summary("m1 1 2 2 h3 v-1 z l1 1"), "M1,1 L3,3 L6,3 L6,2 Z L2,2");
        // Numbers without separators
        assert_eq!(summary("M.5.5-1-1"), "M0.5,0.5 L-1,-1");
        assert_eq!(summary("M1e1 2E-1"), "M10,0.2");
        // Smooth curves reflect the previous control point
        assert_eq!(summary("M0 0C0 10 10 10 10 0S20 -10 20 0"), "M0,0 C0,10,10,10,10,0 C10,-10,20,-10,20,0");
        assert_eq!(summary("M0 0Q5 5 10 0T20 0"), "M0,0 Q5,5,10,0 Q15,-5,20,0");
        // Packed arc flags, and zero radii drawing a line
        assert_eq!(summary("M0 0a5 5 0 1110 0A0 5 0 0 1 20 0"), "M0,0 A L20,0");
        // Rendering stops at the first error
        assert_eq!(summary("M0 0 L10 10 L20 x L30 30"), "M0,0 L10,10");
        assert_eq!(summary("L10 10"), "");
        assert_eq!(summary(""), "");
        assert_eq!(summary("M0 0 L1 1 Z Z"), "M0,0 L1,1 Z Z");
    }

    #[test]
    fn arcs_reach_their_end() {
        // A half circle from (0,10) to (20,10) bulging up, then filled
        let cmds = parse_svg_path("M0 10 A10 10 0 0 1 20 10 Z");
        let p = build_path(&cmds, &sk::Transform::identity()).unwrap();
        let b = p.bounds();
        assert!((b.left() - 0.0).abs() < 0.01 && (b.right() - 20.0).abs() < 0.01, "{b:?}");
        assert!((b.top() - 0.0).abs() < 0.05 && (b.bottom() - 10.0).abs() < 0.01, "{b:?}");
        // Radii too small are scaled up to reach the end point
        let cmds = parse_svg_path("M0 0 A1 1 0 0 0 20 0");
        let p = build_path(&cmds, &sk::Transform::identity()).unwrap();
        assert!((p.bounds().bottom() - 10.0).abs() < 0.05, "{:?}", p.bounds());
        let mut c = Canvas2D::new(30, 30);
        c.fill_path(Some(&build_path(&parse_svg_path("M5 5h20v20h-20z"), &sk::Transform::identity()).unwrap()), sk::FillRule::Winding);
        assert_eq!(c.get_image_data(15, 15, 1, 1)[3], 255);
    }
}
