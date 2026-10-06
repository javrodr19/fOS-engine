//! Static instances of variable fonts.
//!
//! The rasterizer and shaper read a face's `glyf`, `loca` and `hmtx` as
//! they are, so a variable font would always be drawn at its default
//! instance (GitHub's Mona Sans at its lightest). An instance at a given
//! weight is built instead: every glyph's outline and advance are taken
//! at that `wght` coordinate (ttf-parser applies `gvar`, `HVAR` and
//! `avar`) and written out as a plain TrueType face without the variation
//! tables. Composite glyphs come out flattened into simple ones.

use ttf_parser::{GlyphId, OutlineBuilder, Tag};

/// Tables an instance drops: variations, and data tied to the default
/// instance's outlines (hinting, device metrics, the signature)
const DROPPED: [&[u8; 4]; 15] = [
    b"fvar", b"gvar", b"avar", b"HVAR", b"VVAR", b"MVAR", b"STAT", b"cvar", b"hdmx", b"LTSH", b"VDMX", b"fpgm", b"prep", b"cvt ",
    b"DSIG",
];

/// The `wght` axis of a variable TrueType face: (min, default, max)
pub fn weight_axis(data: &[u8], index: u32) -> Option<(f32, f32, f32)> {
    let face = ttf_parser::Face::parse(data, index).ok()?;
    if !face.is_variable() || face.tables().glyf.is_none() {
        return None;
    }
    let axis = face.variation_axes().into_iter().find(|a| a.tag == Tag::from_bytes(b"wght"))?;
    Some((axis.min_value, axis.def_value, axis.max_value))
}

/// Number of glyphs in a face
pub fn glyph_count(data: &[u8], index: u32) -> u16 {
    ttf_parser::Face::parse(data, index).map_or(0, |f| f.number_of_glyphs())
}

/// The face at `index` of `data` as a static TrueType font at `weight`
/// (clamped to its `wght` axis); `None` for faces without TrueType
/// outlines or a weight axis
pub fn instance_at_weight(data: &[u8], index: u32, weight: f32) -> Option<Vec<u8>> {
    let (min, _, max) = weight_axis(data, index)?;
    let mut face = ttf_parser::Face::parse(data, index).ok()?;
    face.set_variation(Tag::from_bytes(b"wght"), weight.clamp(min, max))?;
    let raw = face.raw_face();
    let count = face.number_of_glyphs();

    // Outlines and metrics at the instance
    let mut glyf: Vec<u8> = Vec::new();
    let mut loca: Vec<u32> = Vec::with_capacity(count as usize + 1);
    let mut hmtx: Vec<u8> = Vec::with_capacity(count as usize * 4);
    let (mut max_points, mut max_contours, mut max_advance) = (0u16, 0u16, 0u16);
    for gid in 0..count {
        let id = GlyphId(gid);
        loca.push(glyf.len() as u32);
        let mut outline = Outline::default();
        let bbox = face.outline_glyph(id, &mut outline).map(|_| outline.encode(&mut glyf));
        let advance = face.glyph_hor_advance(id).unwrap_or(0);
        let lsb = bbox.map_or(0, |b| b.0);
        hmtx.extend_from_slice(&advance.to_be_bytes());
        hmtx.extend_from_slice(&lsb.to_be_bytes());
        max_points = max_points.max(outline.points.len() as u16);
        max_contours = max_contours.max(outline.ends.len() as u16);
        max_advance = max_advance.max(advance);
        glyf.resize(glyf.len().next_multiple_of(4), 0);
    }
    loca.push(glyf.len() as u32);
    let loca: Vec<u8> = loca.iter().flat_map(|o| o.to_be_bytes()).collect();

    let mut tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    for record in raw.table_records {
        let tag = record.tag.to_bytes();
        if DROPPED.contains(&&tag) || matches!(&tag, b"glyf" | b"loca" | b"hmtx") {
            continue;
        }
        let start = record.offset as usize;
        let Some(bytes) = data.get(start..start + record.length as usize) else { continue };
        let mut bytes = bytes.to_vec();
        let put = |bytes: &mut Vec<u8>, at: usize, v: u16| {
            if bytes.len() >= at + 2 {
                bytes[at..at + 2].copy_from_slice(&v.to_be_bytes());
            }
        };
        match &tag {
            // Long offsets, and no checksum to adjust
            b"head" => {
                put(&mut bytes, 50, 1);
                if bytes.len() >= 12 {
                    bytes[8..12].fill(0);
                }
            }
            // Every glyph gets a full metric
            b"hhea" => {
                put(&mut bytes, 10, max_advance);
                put(&mut bytes, 34, count);
            }
            b"maxp" => {
                put(&mut bytes, 6, max_points);
                put(&mut bytes, 8, max_contours);
                put(&mut bytes, 10, 0);
                put(&mut bytes, 12, 0);
                put(&mut bytes, 28, 0);
                put(&mut bytes, 30, 0);
            }
            b"OS/2" => put(&mut bytes, 4, weight.clamp(1.0, 1000.0).round() as u16),
            _ => {}
        }
        tables.push((tag, bytes));
    }
    tables.push((*b"glyf", glyf));
    tables.push((*b"loca", loca));
    tables.push((*b"hmtx", hmtx));
    Some(write_sfnt(tables))
}

/// A TrueType font file holding `tables`
fn write_sfnt(mut tables: Vec<([u8; 4], Vec<u8>)>) -> Vec<u8> {
    tables.sort_by(|a, b| a.0.cmp(&b.0));
    let n = tables.len() as u16;
    let entry_selector = 15 - n.max(1).leading_zeros() as u16;
    let search_range = (1u16 << entry_selector) * 16;
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    for v in [n, search_range, entry_selector, n * 16 - search_range] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    let mut offset = 12 + 16 * tables.len();
    for (tag, bytes) in &tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(bytes).to_be_bytes());
        out.extend_from_slice(&(offset as u32).to_be_bytes());
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        offset += bytes.len().next_multiple_of(4);
    }
    for (_, bytes) in &tables {
        out.extend_from_slice(bytes);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    out
}

fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, c| {
        let mut word = [0u8; 4];
        word[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// A glyph outline as TrueType points: (x, y, on-curve), with the index
/// of each contour's last point
#[derive(Default)]
struct Outline {
    points: Vec<(f32, f32, bool)>,
    ends: Vec<u16>,
    start: usize,
    current: (f32, f32),
}

impl Outline {
    fn point(&mut self, x: f32, y: f32, on: bool) {
        self.points.push((x, y, on));
        if on {
            self.current = (x, y);
        }
    }

    /// Append the glyph to `glyf` as a simple glyph (nothing when it is
    /// empty); its bounding box (x min, y min, x max, y max)
    fn encode(&self, glyf: &mut Vec<u8>) -> (i16, i16, i16, i16) {
        if self.points.is_empty() {
            return (0, 0, 0, 0);
        }
        let pts: Vec<(i16, i16, bool)> = self.points.iter().map(|&(x, y, on)| (x.round() as i16, y.round() as i16, on)).collect();
        let bbox = pts.iter().fold((i16::MAX, i16::MAX, i16::MIN, i16::MIN), |b, &(x, y, _)| (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y)));
        glyf.extend_from_slice(&(self.ends.len() as i16).to_be_bytes());
        for v in [bbox.0, bbox.1, bbox.2, bbox.3] {
            glyf.extend_from_slice(&v.to_be_bytes());
        }
        for e in &self.ends {
            glyf.extend_from_slice(&e.to_be_bytes());
        }
        glyf.extend_from_slice(&0u16.to_be_bytes()); // no instructions
        // Flags: on-curve or not; coordinates as 16-bit deltas
        glyf.extend(pts.iter().map(|p| p.2 as u8));
        let mut prev = 0i16;
        for &(x, _, _) in &pts {
            glyf.extend_from_slice(&x.wrapping_sub(prev).to_be_bytes());
            prev = x;
        }
        prev = 0;
        for &(_, y, _) in &pts {
            glyf.extend_from_slice(&y.wrapping_sub(prev).to_be_bytes());
            prev = y;
        }
        bbox
    }
}

impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.start = self.points.len();
        self.point(x, y, true);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.point(x, y, true);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.point(x1, y1, false);
        self.point(x, y, true);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        // A cubic as two quadratics, one per half
        let (x0, y0) = self.current;
        let mid = |a: f32, b: f32| (a + b) / 2.0;
        let (ax, ay) = (mid(x0, x1), mid(y0, y1));
        let (bx, by) = (mid(x1, x2), mid(y1, y2));
        let (cx, cy) = (mid(x2, x), mid(y2, y));
        let (dx, dy) = (mid(ax, bx), mid(ay, by));
        let (ex, ey) = (mid(bx, cx), mid(by, cy));
        let (sx, sy) = (mid(dx, ex), mid(dy, ey));
        self.quad_to(mid(ax, dx), mid(ay, dy), sx, sy);
        self.quad_to(mid(ex, cx), mid(ey, cy), x, y);
    }

    fn close(&mut self) {
        // A closing point repeating the start is implied
        let Some(&start) = self.points.get(self.start) else { return };
        if self.points.len() > self.start + 1 {
            let last = *self.points.last().unwrap_or(&start);
            if last.2 && (last.0, last.1) == (start.0, start.1) {
                self.points.pop();
            }
        }
        if self.points.len() > self.start {
            self.ends.push((self.points.len() - 1) as u16);
        }
        self.start = self.points.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A variable font with a `wght` axis (100 to 900, default 400) and one
    /// square glyph, 100 units wide with a 500 advance, that grows 100
    /// units wider (advance included) at weight 900
    fn variable_square() -> Vec<u8> {
        let be16 = |v: &mut Vec<u8>, x: u16| v.extend_from_slice(&x.to_be_bytes());
        let be32 = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_be_bytes());
        let mut head = vec![0u8; 54];
        head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes());
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        head[50..52].copy_from_slice(&1u16.to_be_bytes());
        let mut hhea = vec![0u8; 36];
        hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        hhea[4..6].copy_from_slice(&800u16.to_be_bytes());
        hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
        hhea[34..36].copy_from_slice(&2u16.to_be_bytes());
        let mut maxp = Vec::new();
        be32(&mut maxp, 0x0000_5000);
        be16(&mut maxp, 2);
        let mut hmtx = Vec::new();
        for (adv, lsb) in [(500u16, 0u16), (500, 0)] {
            be16(&mut hmtx, adv);
            be16(&mut hmtx, lsb);
        }
        // Glyph 1: a square, as 16-bit coordinate deltas
        let mut glyf = Vec::new();
        for v in [1i16, 0, 0, 100, 100] {
            glyf.extend_from_slice(&v.to_be_bytes());
        }
        be16(&mut glyf, 3); // last point
        be16(&mut glyf, 0); // no instructions
        glyf.extend_from_slice(&[1, 1, 1, 1]);
        for d in [0i16, 0, 100, 0] {
            glyf.extend_from_slice(&d.to_be_bytes());
        }
        for d in [0i16, 100, 0, -100] {
            glyf.extend_from_slice(&d.to_be_bytes());
        }
        let mut loca = Vec::new();
        for o in [0u32, 0, glyf.len() as u32] {
            be32(&mut loca, o);
        }
        let mut fvar = Vec::new();
        for v in [1u16, 0, 16, 2, 1, 20, 0, 8] {
            be16(&mut fvar, v);
        }
        fvar.extend_from_slice(b"wght");
        for v in [100u32, 400, 900] {
            be32(&mut fvar, v << 16);
        }
        be16(&mut fvar, 0);
        be16(&mut fvar, 256);
        // gvar: glyph 1's points and phantom points (left, right, top,
        // bottom) move by these x deltas at the axis maximum
        let mut data = vec![0x47];
        for d in [0i16, 0, 100, 100, 0, 100, 0, 0] {
            data.extend_from_slice(&d.to_be_bytes());
        }
        data.push(0x87);
        let mut gvd = Vec::new();
        be16(&mut gvd, 1);
        be16(&mut gvd, 10);
        be16(&mut gvd, data.len() as u16);
        be16(&mut gvd, 0x8000);
        be16(&mut gvd, 0x4000);
        gvd.extend_from_slice(&data);
        let mut gvar = Vec::new();
        for v in [1u16, 0, 1, 0] {
            be16(&mut gvar, v);
        }
        be32(&mut gvar, 32);
        be16(&mut gvar, 2);
        be16(&mut gvar, 1);
        be32(&mut gvar, 32);
        for o in [0u32, 0, gvd.len() as u32] {
            be32(&mut gvar, o);
        }
        gvar.extend_from_slice(&gvd);
        write_sfnt(vec![
            (*b"head", head),
            (*b"hhea", hhea),
            (*b"maxp", maxp),
            (*b"hmtx", hmtx),
            (*b"loca", loca),
            (*b"glyf", glyf),
            (*b"fvar", fvar),
            (*b"gvar", gvar),
        ])
    }

    fn square(data: &[u8]) -> (ttf_parser::Rect, u16) {
        let face = ttf_parser::Face::parse(data, 0).unwrap();
        let mut outline = Outline::default();
        let bbox = face.outline_glyph(GlyphId(1), &mut outline).unwrap();
        (bbox, face.glyph_hor_advance(GlyphId(1)).unwrap())
    }

    #[test]
    fn instances_take_the_outlines_and_advances_at_their_weight() {
        let font = variable_square();
        assert_eq!(weight_axis(&font, 0), Some((100.0, 400.0, 900.0)));
        for (weight, width, advance) in [(400.0, 100, 500), (900.0, 200, 600), (650.0, 150, 550), (2000.0, 200, 600)] {
            let instance = instance_at_weight(&font, 0, weight).unwrap();
            let face = ttf_parser::Face::parse(&instance, 0).unwrap();
            assert!(!face.is_variable(), "{weight}");
            let (bbox, adv) = square(&instance);
            assert_eq!((bbox.x_max, bbox.y_max, adv), (width, 100, advance), "{weight}");
            // The engine's own parser reads it too
            let parser = crate::font::FontParser::parse_index(&instance, 0).unwrap();
            assert_eq!(parser.units_per_em(), 1000);
        }
        assert_eq!(weight_axis(&instance_at_weight(&font, 0, 700.0).unwrap(), 0), None);
    }
}
