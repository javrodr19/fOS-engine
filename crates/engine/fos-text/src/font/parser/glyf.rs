//! Glyph outline parsing (glyf/loca tables)

use super::reader::FontReader;
use super::outline::OutlineBuilder;

/// Maximum nesting of compound glyphs. Real fonts stay well below this;
/// the limit stops malicious fonts with cyclic components from recursing
/// until the stack overflows.
const MAX_COMPONENT_DEPTH: u32 = 8;

/// Get glyph offset from loca table
pub fn get_glyph_offset(loca_data: &[u8], glyph_index: u16, index_format: u16) -> Option<u32> {
    let mut reader = FontReader::new(loca_data);

    if index_format == 0 {
        // Short format (u16, multiply by 2)
        reader.skip((glyph_index as usize) * 2).ok()?;
        Some(reader.read_u16().ok()? as u32 * 2)
    } else {
        // Long format (u32)
        reader.skip((glyph_index as usize) * 4).ok()?;
        Some(reader.read_u32().ok()?)
    }
}

/// 2D affine transform: `x' = a*x + c*y + e`, `y' = b*x + d*y + f`
#[derive(Debug, Clone, Copy, PartialEq)]
struct Transform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Transform {
    const IDENTITY: Transform = Transform { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }

    /// Transform that applies `inner` first, then `self`
    fn compose(&self, inner: &Transform) -> Transform {
        Transform {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            e: self.a * inner.e + self.c * inner.f + self.e,
            f: self.b * inner.e + self.d * inner.f + self.f,
        }
    }
}

/// Outline builder adaptor applying a transform to every point
struct Transformed<'b, B: OutlineBuilder> {
    inner: &'b mut B,
    transform: Transform,
}

impl<B: OutlineBuilder> Transformed<'_, B> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform.apply(x, y);
        self.inner.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.transform.apply(x, y);
        self.inner.line_to(x, y);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (x1, y1) = self.transform.apply(x1, y1);
        let (x, y) = self.transform.apply(x, y);
        self.inner.quad_to(x1, y1, x, y);
    }

    fn close(&mut self) {
        self.inner.close();
    }
}

/// Outline a glyph
pub fn outline_glyph<B: OutlineBuilder>(
    glyf_data: &[u8],
    loca_data: &[u8],
    glyph_index: u16,
    index_format: u16,
    builder: &mut B,
) -> Option<()> {
    outline_glyph_transformed(glyf_data, loca_data, glyph_index, index_format, builder, &Transform::IDENTITY, 0)
}

fn outline_glyph_transformed<B: OutlineBuilder>(
    glyf_data: &[u8],
    loca_data: &[u8],
    glyph_index: u16,
    index_format: u16,
    builder: &mut B,
    transform: &Transform,
    depth: u32,
) -> Option<()> {
    if depth > MAX_COMPONENT_DEPTH {
        return None;
    }

    let offset = get_glyph_offset(loca_data, glyph_index, index_format)?;
    let next_offset = get_glyph_offset(loca_data, glyph_index.checked_add(1)?, index_format)?;

    if offset == next_offset {
        return Some(()); // Empty glyph
    }

    let glyph_data = glyf_data.get(offset as usize..next_offset as usize)?;
    let mut reader = FontReader::new(glyph_data);

    let num_contours = reader.read_i16().ok()?;
    let _x_min = reader.read_i16().ok()?;
    let _y_min = reader.read_i16().ok()?;
    let _x_max = reader.read_i16().ok()?;
    let _y_max = reader.read_i16().ok()?;

    if num_contours >= 0 {
        let mut target = Transformed { inner: builder, transform: *transform };
        outline_simple_glyph(glyph_data, num_contours as u16, &mut target)
    } else {
        outline_compound_glyph(glyf_data, loca_data, glyph_data, index_format, builder, transform, depth)
    }
}

/// Parse simple glyph outline
fn outline_simple_glyph<B: OutlineBuilder>(
    glyph_data: &[u8],
    num_contours: u16,
    builder: &mut Transformed<'_, B>,
) -> Option<()> {
    if num_contours == 0 {
        return Some(());
    }

    let mut reader = FontReader::new(glyph_data);
    reader.skip(10).ok()?; // Skip header

    // Read end points of contours; they must be strictly increasing
    let mut end_points = Vec::with_capacity(num_contours as usize);
    for _ in 0..num_contours {
        let end = reader.read_u16().ok()? as usize;
        if end_points.last().is_some_and(|&prev| end <= prev) {
            return None;
        }
        end_points.push(end);
    }

    let num_points = end_points.last().copied().unwrap_or(0) + 1;

    // Skip instructions
    let instruction_length = reader.read_u16().ok()? as usize;
    reader.skip(instruction_length).ok()?;

    // Read flags
    let mut flags = Vec::with_capacity(num_points);
    while flags.len() < num_points {
        let flag = reader.read_u8().ok()?;
        flags.push(flag);

        if flag & 0x08 != 0 {
            // Repeat flag
            let repeat_count = reader.read_u8().ok()? as usize;
            for _ in 0..repeat_count {
                flags.push(flag);
            }
        }
    }
    flags.truncate(num_points);

    // Read x coordinates
    let mut x_coords = Vec::with_capacity(num_points);
    let mut x = 0i32;
    for &flag in &flags {
        let is_short = flag & 0x02 != 0;
        let is_same_or_positive = flag & 0x10 != 0;

        if is_short {
            let dx = reader.read_u8().ok()? as i32;
            x += if is_same_or_positive { dx } else { -dx };
        } else if !is_same_or_positive {
            x += reader.read_i16().ok()? as i32;
        }
        // else: same as previous (x unchanged)

        x_coords.push(x as f32);
    }

    // Read y coordinates
    let mut y_coords = Vec::with_capacity(num_points);
    let mut y = 0i32;
    for &flag in &flags {
        let is_short = flag & 0x04 != 0;
        let is_same_or_positive = flag & 0x20 != 0;

        if is_short {
            let dy = reader.read_u8().ok()? as i32;
            y += if is_same_or_positive { dy } else { -dy };
        } else if !is_same_or_positive {
            y += reader.read_i16().ok()? as i32;
        }

        y_coords.push(y as f32);
    }

    // Build outline, one contour at a time
    let mut contour_start = 0usize;
    for &contour_end in &end_points {
        let len = contour_end + 1 - contour_start;
        let point = |i: usize| {
            let idx = contour_start + i % len;
            (x_coords[idx], y_coords[idx], flags[idx] & 0x01 != 0)
        };

        if len >= 2 {
            // Start at the first on-curve point. A contour made only of
            // off-curve points starts at the implied on-curve midpoint
            // between its last and first points.
            let first_on = (0..len).find(|&i| point(i).2);
            let (start, begin, count) = match first_on {
                Some(i) => {
                    let (px, py, _) = point(i);
                    ((px, py), i + 1, len - 1)
                }
                None => {
                    let (lx, ly, _) = point(len - 1);
                    let (fx, fy, _) = point(0);
                    (((lx + fx) / 2.0, (ly + fy) / 2.0), 0, len)
                }
            };

            builder.move_to(start.0, start.1);
            let mut pending_off: Option<(f32, f32)> = None;

            for k in 0..count {
                let (px, py, on_curve) = point(begin + k);
                if on_curve {
                    match pending_off.take() {
                        Some((cx, cy)) => builder.quad_to(cx, cy, px, py),
                        None => builder.line_to(px, py),
                    }
                } else {
                    if let Some((cx, cy)) = pending_off {
                        // Two off-curve points imply an on-curve midpoint
                        builder.quad_to(cx, cy, (cx + px) / 2.0, (cy + py) / 2.0);
                    }
                    pending_off = Some((px, py));
                }
            }

            if let Some((cx, cy)) = pending_off {
                builder.quad_to(cx, cy, start.0, start.1);
            }
            builder.close();
        }

        contour_start = contour_end + 1;
    }

    Some(())
}

/// Parse compound glyph outline, placing each component with its offset and
/// scale (accented letters are usually a base glyph plus a shifted accent)
fn outline_compound_glyph<B: OutlineBuilder>(
    glyf_data: &[u8],
    loca_data: &[u8],
    glyph_data: &[u8],
    index_format: u16,
    builder: &mut B,
    parent: &Transform,
    depth: u32,
) -> Option<()> {
    let mut reader = FontReader::new(glyph_data);
    reader.skip(10).ok()?; // Skip header

    const ARG_1_AND_2_ARE_WORDS: u16 = 0x0001;
    const ARGS_ARE_XY_VALUES: u16 = 0x0002;
    const WE_HAVE_A_SCALE: u16 = 0x0008;
    const MORE_COMPONENTS: u16 = 0x0020;
    const WE_HAVE_AN_X_AND_Y_SCALE: u16 = 0x0040;
    const WE_HAVE_A_TWO_BY_TWO: u16 = 0x0080;
    const SCALED_COMPONENT_OFFSET: u16 = 0x0800;

    let read_f2dot14 = |reader: &mut FontReader| -> Option<f32> {
        Some(reader.read_i16().ok()? as f32 / 16384.0)
    };

    let mut more = true;
    while more {
        let flags = reader.read_u16().ok()?;
        let glyph_index = reader.read_u16().ok()?;

        let (arg1, arg2) = if flags & ARG_1_AND_2_ARE_WORDS != 0 {
            (reader.read_i16().ok()? as f32, reader.read_i16().ok()? as f32)
        } else {
            (reader.read_u8().ok()? as i8 as f32, reader.read_u8().ok()? as i8 as f32)
        };
        // Point-matching placement (args are point numbers) is not supported
        let (dx, dy) = if flags & ARGS_ARE_XY_VALUES != 0 { (arg1, arg2) } else { (0.0, 0.0) };

        let mut component = Transform::IDENTITY;
        if flags & WE_HAVE_A_SCALE != 0 {
            let scale = read_f2dot14(&mut reader)?;
            component.a = scale;
            component.d = scale;
        } else if flags & WE_HAVE_AN_X_AND_Y_SCALE != 0 {
            component.a = read_f2dot14(&mut reader)?;
            component.d = read_f2dot14(&mut reader)?;
        } else if flags & WE_HAVE_A_TWO_BY_TWO != 0 {
            component.a = read_f2dot14(&mut reader)?;
            component.b = read_f2dot14(&mut reader)?;
            component.c = read_f2dot14(&mut reader)?;
            component.d = read_f2dot14(&mut reader)?;
        }

        if flags & SCALED_COMPONENT_OFFSET != 0 {
            component.e = component.a * dx + component.c * dy;
            component.f = component.b * dx + component.d * dy;
        } else {
            component.e = dx;
            component.f = dy;
        }

        let combined = parent.compose(&component);
        outline_glyph_transformed(glyf_data, loca_data, glyph_index, index_format, builder, &combined, depth + 1)?;

        more = flags & MORE_COMPONENTS != 0;
    }

    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::outline::{GlyphOutline, OutlineCommand};

    /// Simple glyph: one triangle contour with on-curve points (0,0) (100,0) (50,100)
    fn triangle_glyph() -> Vec<u8> {
        let mut g = Vec::new();
        for v in [1i16, 0, 0, 100, 100] {
            g.extend_from_slice(&v.to_be_bytes()); // contours + bbox
        }
        g.extend_from_slice(&2u16.to_be_bytes()); // end point of contour 0
        g.extend_from_slice(&0u16.to_be_bytes()); // no instructions
        g.extend_from_slice(&[0x01, 0x01, 0x01]); // on-curve, long coordinates
        for dx in [0i16, 100, -50] {
            g.extend_from_slice(&dx.to_be_bytes());
        }
        for dy in [0i16, 0, 100] {
            g.extend_from_slice(&dy.to_be_bytes());
        }
        g
    }

    /// Compound glyph referencing glyph 0 with an offset of (+10, +20)
    fn compound_glyph() -> Vec<u8> {
        let mut g = Vec::new();
        for v in [-1i16, 0, 0, 110, 120] {
            g.extend_from_slice(&v.to_be_bytes());
        }
        g.extend_from_slice(&(0x0001u16 | 0x0002).to_be_bytes()); // words, xy values
        g.extend_from_slice(&0u16.to_be_bytes()); // component glyph 0
        g.extend_from_slice(&10i16.to_be_bytes());
        g.extend_from_slice(&20i16.to_be_bytes());
        g
    }

    fn tables(glyphs: &[Vec<u8>]) -> (Vec<u8>, Vec<u8>) {
        let mut glyf = Vec::new();
        let mut loca = Vec::new();
        for g in glyphs {
            loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
            glyf.extend_from_slice(g);
        }
        loca.extend_from_slice(&(glyf.len() as u32).to_be_bytes());
        (glyf, loca)
    }

    fn points(outline: &GlyphOutline) -> Vec<(f32, f32)> {
        outline.commands.iter().filter_map(|c| match *c {
            OutlineCommand::MoveTo(x, y) | OutlineCommand::LineTo(x, y) => Some((x, y)),
            OutlineCommand::QuadTo(_, _, x, y) => Some((x, y)),
            _ => None,
        }).collect()
    }

    #[test]
    fn test_simple_glyph_outline() {
        let (glyf, loca) = tables(&[triangle_glyph()]);
        let mut outline = GlyphOutline::default();
        outline_glyph(&glyf, &loca, 0, 1, &mut outline).unwrap();
        assert_eq!(points(&outline), vec![(0.0, 0.0), (100.0, 0.0), (50.0, 100.0)]);
        assert!(matches!(outline.commands.last(), Some(OutlineCommand::Close)));
    }

    #[test]
    fn test_compound_glyph_applies_offset() {
        let (glyf, loca) = tables(&[triangle_glyph(), compound_glyph()]);
        let mut outline = GlyphOutline::default();
        outline_glyph(&glyf, &loca, 1, 1, &mut outline).unwrap();
        assert_eq!(points(&outline), vec![(10.0, 20.0), (110.0, 20.0), (60.0, 120.0)]);
    }

    #[test]
    fn test_cyclic_compound_glyph_terminates() {
        // Glyph 0 references itself
        let mut g = Vec::new();
        for v in [-1i16, 0, 0, 0, 0] {
            g.extend_from_slice(&v.to_be_bytes());
        }
        g.extend_from_slice(&0x0003u16.to_be_bytes());
        g.extend_from_slice(&0u16.to_be_bytes());
        g.extend_from_slice(&[0, 0, 0, 0]);
        let (glyf, loca) = tables(&[g]);

        let mut outline = GlyphOutline::default();
        assert!(outline_glyph(&glyf, &loca, 0, 1, &mut outline).is_none());
    }

    #[test]
    fn test_malformed_glyphs_do_not_panic() {
        // Decreasing contour end points
        let mut g = Vec::new();
        for v in [2i16, 0, 0, 10, 10] {
            g.extend_from_slice(&v.to_be_bytes());
        }
        g.extend_from_slice(&5u16.to_be_bytes());
        g.extend_from_slice(&1u16.to_be_bytes());
        let (glyf, loca) = tables(&[g]);
        let mut outline = GlyphOutline::default();
        assert!(outline_glyph(&glyf, &loca, 0, 1, &mut outline).is_none());

        // Glyph offsets beyond the glyf table, and the last glyph ID
        let loca: Vec<u8> = [0u32, 1000].iter().flat_map(|v| v.to_be_bytes()).collect();
        assert!(outline_glyph(&[0; 10], &loca, 0, 1, &mut outline).is_none());
        assert!(outline_glyph(&[0; 10], &loca, u16::MAX, 1, &mut outline).is_none());
    }

    #[test]
    fn test_transform_compose() {
        let scale = Transform { a: 2.0, d: 2.0, ..Transform::IDENTITY };
        let shift = Transform { e: 5.0, f: -1.0, ..Transform::IDENTITY };
        // Scale first, then shift
        assert_eq!(shift.compose(&scale).apply(1.0, 1.0), (7.0, 1.0));
        // Shift first, then scale
        assert_eq!(scale.compose(&shift).apply(1.0, 1.0), (12.0, 0.0));
    }
}
