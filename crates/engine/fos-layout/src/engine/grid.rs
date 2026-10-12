//! CSS Grid layout (CSS Grid Layout 1 §7-§11, simplified where noted):
//! explicit tracks with repeat()/auto-fill/auto-fit and line names,
//! template areas, item placement by lines, names, areas and spans,
//! auto-placement (row or column flow, sparse or dense) with implicit
//! tracks, track sizing (fixed, intrinsic, minmax, fit-content, fr),
//! content distribution and self-alignment

use std::sync::Arc;

use fos_css::grid::{Breadth, GridLine, GridStyle, RepeatCount, TemplateItem, TrackSize};
use fos_css::style::{AlignContent, AlignItems, AlignSelf, JustifyContent, Style};

use super::block::{intrinsic_outer, layout_sized, placeholder, relative_offset, Forced, LayoutCtx, Sizing};
use super::box_tree::LayoutBox;
use super::fragment::{Fragment, Rect};

pub struct GridLayout {
    pub frags: Vec<Fragment>,
    pub height: f32,
}

/// One axis's explicit tracks and line names
struct Explicit {
    tracks: Vec<TrackSize>,
    /// Names of each line (tracks + 1)
    names: Vec<Vec<Arc<str>>>,
}

/// Expand a track list for an axis `avail` long (when definite)
fn expand(template: &[TemplateItem], avail: Option<f32>, gap: f32) -> Explicit {
    let mut tracks = Vec::new();
    let mut names: Vec<Vec<Arc<str>>> = vec![Vec::new()];
    fn push_items(items: &[TemplateItem], tracks: &mut Vec<TrackSize>, names: &mut Vec<Vec<Arc<str>>>) {
        for item in items {
            match item {
                TemplateItem::Names(n) => names.last_mut().expect("a line").extend(n.iter().cloned()),
                TemplateItem::Track(t) => {
                    tracks.push(*t);
                    names.push(Vec::new());
                }
                TemplateItem::Repeat(..) => {}
            }
        }
    }
    let fixed = |b: Breadth, basis: f32| match b {
        Breadth::Length(l) => Some(l.resolve(basis)),
        _ => None,
    };
    for item in template {
        match item {
            TemplateItem::Repeat(count, inner) => {
                let n = match count {
                    RepeatCount::Count(n) => *n as usize,
                    RepeatCount::AutoFill | RepeatCount::AutoFit => {
                        // As many repetitions as fit (at least one)
                        let inner_tracks: Vec<&TrackSize> = inner.iter().filter_map(|i| if let TemplateItem::Track(t) = i { Some(t) } else { None }).collect();
                        match avail {
                            Some(avail) => {
                                let basis = avail;
                                let size: Option<f32> = inner_tracks.iter().map(|t| fixed(t.max, basis).or_else(|| fixed(t.min, basis))).sum();
                                match size {
                                    Some(s) if s > 0.0 => {
                                        let per = s + gap * inner_tracks.len() as f32;
                                        (((avail + gap) / per).floor() as usize).clamp(1, 1000)
                                    }
                                    _ => 1,
                                }
                            }
                            None => 1,
                        }
                    }
                };
                for _ in 0..n {
                    push_items(inner, &mut tracks, &mut names);
                }
            }
            other => push_items(std::slice::from_ref(other), &mut tracks, &mut names),
        }
    }
    Explicit { tracks, names }
}

/// A resolved span in one axis: start line (0-based, may be negative
/// for implicit tracks before the explicit grid) and end line
#[derive(Clone, Copy, Debug)]
enum Place {
    Definite(i32, i32),
    /// Auto-placed, spanning this many tracks
    Auto(u32),
}

/// The line named `name`, nth occurrence (negative counts from the end)
fn named_line(names: &[Vec<Arc<str>>], name: &str, nth: i32) -> Option<i32> {
    let found: Vec<i32> = names.iter().enumerate().filter(|(_, n)| n.iter().any(|x| &**x == name)).map(|(i, _)| i as i32).collect();
    if found.is_empty() {
        return None;
    }
    if nth > 0 {
        found.get(nth as usize - 1).copied().or(found.last().copied())
    } else {
        let i = found.len() as i32 + nth;
        found.get(i.max(0) as usize).copied()
    }
}

/// Resolve an item's start and end lines in one axis
fn resolve(start: &GridLine, end: &GridLine, names: &[Vec<Arc<str>>], areas: &[(Arc<str>, u32, u32)], explicit: i32) -> Place {
    let line = |l: &GridLine, is_start: bool| -> Option<i32> {
        match l {
            GridLine::Line(n, None) => Some(if *n > 0 { n - 1 } else { explicit + 1 + n }),
            GridLine::Line(n, Some(name)) => named_line(names, name, *n),
            GridLine::Ident(name) => {
                if let Some(a) = areas.iter().find(|a| &*a.0 == &**name) {
                    return Some(if is_start { a.1 as i32 } else { a.2 as i32 });
                }
                let suffixed = format!("{name}-{}", if is_start { "start" } else { "end" });
                named_line(names, &suffixed, 1).or_else(|| named_line(names, name, 1))
            }
            _ => None,
        }
    };
    let span_of = |l: &GridLine| if let GridLine::Span(n, _) = l { Some(*n as i32) } else { None };
    match (line(start, true), line(end, false)) {
        (Some(s), Some(e)) => {
            if e > s {
                Place::Definite(s, e)
            } else if e < s {
                Place::Definite(e, s)
            } else {
                Place::Definite(s, s + 1)
            }
        }
        (Some(s), None) => Place::Definite(s, s + span_of(end).unwrap_or(1)),
        (None, Some(e)) => Place::Definite(e - span_of(start).unwrap_or(1), e),
        (None, None) => Place::Auto(span_of(start).or(span_of(end)).unwrap_or(1).max(1) as u32),
    }
}

/// An item's area: column start..end, row start..end (0-based tracks of
/// the final grid)
#[derive(Clone, Copy, Debug)]
struct Area {
    c0: usize,
    c1: usize,
    r0: usize,
    r1: usize,
}

/// Occupied cells, growing as needed
struct Occupancy {
    cols: usize,
    rows: Vec<Vec<bool>>,
}

impl Occupancy {
    fn fits(&self, r0: usize, c0: usize, rs: usize, cs: usize) -> bool {
        (r0..r0 + rs).all(|r| (c0..c0 + cs).all(|c| !self.rows.get(r).is_some_and(|row| row.get(c).copied().unwrap_or(false))))
    }

    fn mark(&mut self, r0: usize, c0: usize, rs: usize, cs: usize) {
        if self.rows.len() < r0 + rs {
            self.rows.resize(r0 + rs, Vec::new());
        }
        self.cols = self.cols.max(c0 + cs);
        for r in r0..r0 + rs {
            let row = &mut self.rows[r];
            if row.len() < c0 + cs {
                row.resize(c0 + cs, false);
            }
            for cell in &mut row[c0..c0 + cs] {
                *cell = true;
            }
        }
    }
}

/// Place the items (CSS Grid §8.5); returns their areas in the "major"
/// (flow) and "minor" axes plus the track counts
fn place(items: &[(&LayoutBox, Place, Place)], explicit_cols: usize, column_flow: bool, dense: bool) -> (Vec<Area>, usize, usize) {
    // Work in flow terms: "rows" are the axis auto-placement advances in
    let (major, minor): (Vec<Place>, Vec<Place>) = items.iter().map(|(_, c, r)| if column_flow { (*c, *r) } else { (*r, *c) }).unzip();
    // Implicit tracks before the explicit grid shift every line
    let shift = |p: &Place| if let Place::Definite(s, _) = p { (-s).max(0) } else { 0 };
    let major_shift = major.iter().map(shift).max().unwrap_or(0);
    let minor_shift = minor.iter().map(shift).max().unwrap_or(0);
    let def = |p: &Place, sh: i32| if let Place::Definite(s, e) = p { Some(((s + sh) as usize, (e + sh) as usize)) } else { None };
    let mut occ = Occupancy { cols: explicit_cols + minor_shift as usize, rows: Vec::new() };
    let mut out: Vec<Option<(usize, usize, usize, usize)>> = vec![None; items.len()];
    // Items fixed in both axes, then the minor size every item needs
    for i in 0..items.len() {
        if let (Some((r0, r1)), Some((c0, c1))) = (def(&major[i], major_shift), def(&minor[i], minor_shift)) {
            occ.mark(r0, c0, r1 - r0, c1 - c0);
            out[i] = Some((r0, r1, c0, c1));
        }
    }
    for p in &minor {
        match p {
            Place::Auto(span) => occ.cols = occ.cols.max(*span as usize),
            Place::Definite(..) => {
                if let Some((_, c1)) = def(p, minor_shift) {
                    occ.cols = occ.cols.max(c1);
                }
            }
        }
    }
    // Items locked to a row (in the flow axis)
    for i in 0..items.len() {
        if out[i].is_some() {
            continue;
        }
        if let (Some((r0, r1)), Place::Auto(span)) = (def(&major[i], major_shift), minor[i]) {
            let span = span as usize;
            let c0 = (0..).find(|&c| c + span > occ.cols || occ.fits(r0, c, r1 - r0, span)).unwrap_or(0);
            occ.mark(r0, c0, r1 - r0, span);
            out[i] = Some((r0, r1, c0, c0 + span));
        }
    }
    // The rest, with a cursor (dense placement restarts from the top)
    let (mut cr, mut cc) = (0usize, 0usize);
    for i in 0..items.len() {
        if out[i].is_some() {
            continue;
        }
        let rs = if let Place::Auto(s) = major[i] { s as usize } else { 1 };
        if dense {
            cr = 0;
            cc = 0;
        }
        match def(&minor[i], minor_shift) {
            Some((c0, c1)) => {
                if !dense && c0 < cc {
                    cr += 1;
                }
                while !occ.fits(cr, c0, rs, c1 - c0) {
                    cr += 1;
                }
                occ.mark(cr, c0, rs, c1 - c0);
                out[i] = Some((cr, cr + rs, c0, c1));
                cc = c1;
            }
            None => {
                let cs = if let Place::Auto(s) = minor[i] { s as usize } else { 1 };
                let cols = occ.cols.max(cs);
                loop {
                    if cc + cs > cols {
                        cr += 1;
                        cc = 0;
                        continue;
                    }
                    if occ.fits(cr, cc, rs, cs) {
                        break;
                    }
                    cc += 1;
                }
                occ.mark(cr, cc, rs, cs);
                out[i] = Some((cr, cr + rs, cc, cc + cs));
                cc += cs;
            }
        }
    }
    let rows = occ.rows.len();
    let cols = occ.cols;
    let areas = out
        .into_iter()
        .map(|o| {
            let (r0, r1, c0, c1) = o.expect("placed");
            if column_flow {
                Area { c0: r0, c1: r1, r0: c0, r1: c1 }
            } else {
                Area { c0, c1, r0, r1 }
            }
        })
        .collect();
    if column_flow {
        (areas, rows, cols)
    } else {
        (areas, cols, rows)
    }
}

/// A track being sized
#[derive(Clone, Copy, Debug)]
struct Track {
    size: TrackSize,
    base: f32,
    limit: f32,
}

impl Track {
    fn flex(&self) -> Option<f32> {
        if let Breadth::Fr(f) = self.size.max {
            Some(f)
        } else {
            None
        }
    }
}

/// Size the tracks of one axis (§11, simplified): `contrib[i]` is item
/// i's (min-content, max-content) outer size in this axis and `span` its
/// tracks
fn size_tracks(tracks: &mut [Track], spans: &[(usize, usize)], contrib: &[(f32, f32)], avail: Option<f32>, gap: f32, stretch: bool) {
    let basis = avail.unwrap_or(0.0);
    for t in tracks.iter_mut() {
        t.base = match t.size.min {
            Breadth::Length(l) => l.resolve(basis),
            _ => 0.0,
        };
        t.limit = match t.size.max {
            Breadth::Length(l) => l.resolve(basis).max(t.base),
            _ => f32::INFINITY,
        };
    }
    let mut max_seen = vec![0.0f32; tracks.len()];
    // Items in one track
    for (&(s, e), &(cmin, cmax)) in spans.iter().zip(contrib) {
        if e - s != 1 {
            continue;
        }
        let t = &mut tracks[s];
        match t.size.min {
            Breadth::Auto | Breadth::MinContent => t.base = t.base.max(cmin),
            Breadth::MaxContent => t.base = t.base.max(cmax),
            _ => {}
        }
        if t.flex().is_none() {
            let want = match t.size.max {
                Breadth::MinContent => cmin,
                Breadth::Auto | Breadth::MaxContent => cmax,
                _ => continue,
            };
            max_seen[s] = max_seen[s].max(want);
        } else {
            max_seen[s] = max_seen[s].max(cmax);
        }
    }
    for (i, t) in tracks.iter_mut().enumerate() {
        if t.limit.is_infinite() && t.flex().is_none() {
            t.limit = max_seen[i];
        }
        if let Some(fc) = t.size.fit_content {
            t.limit = t.limit.min(fc.resolve(basis));
        }
        t.limit = t.limit.max(t.base);
    }
    // Spanning items: what they need beyond their tracks goes evenly to
    // the intrinsic ones. Items spanning a flexible track are left to the
    // flexible tracks (they would otherwise inflate the auto tracks a
    // later `fr` track should have grown into)
    for (&(s, e), &(cmin, cmax)) in spans.iter().zip(contrib) {
        if e - s < 2 || tracks[s..e].iter().any(|t| t.flex().is_some()) {
            continue;
        }
        let gaps = gap * (e - s - 1) as f32;
        let intrinsic: Vec<usize> = (s..e).filter(|&i| tracks[i].size.min.is_intrinsic()).collect();
        if intrinsic.is_empty() {
            continue;
        }
        let have: f32 = tracks[s..e].iter().map(|t| t.base).sum::<f32>() + gaps;
        if cmin > have {
            let each = (cmin - have) / intrinsic.len() as f32;
            for &i in &intrinsic {
                tracks[i].base += each;
                tracks[i].limit = tracks[i].limit.max(tracks[i].base);
            }
        }
        let have_limit: f32 = tracks[s..e].iter().map(|t| if t.limit.is_finite() { t.limit } else { t.base }).sum::<f32>() + gaps;
        if cmax > have_limit {
            let growable: Vec<usize> = intrinsic.iter().copied().filter(|&i| tracks[i].flex().is_none()).collect();
            if !growable.is_empty() {
                let each = (cmax - have_limit) / growable.len() as f32;
                for i in growable {
                    tracks[i].limit += each;
                }
            }
        }
    }
    let gaps = gap * tracks.len().saturating_sub(1) as f32;
    // Grow tracks toward their limits with the free space
    if let Some(avail) = avail {
        let mut free = avail - tracks.iter().map(|t| t.base).sum::<f32>() - gaps;
        for _ in 0..4 {
            let growing: Vec<usize> = (0..tracks.len()).filter(|&i| tracks[i].flex().is_none() && tracks[i].limit > tracks[i].base + 0.01).collect();
            if free <= 0.01 || growing.is_empty() {
                break;
            }
            let each = free / growing.len() as f32;
            for i in growing {
                let add = each.min(tracks[i].limit - tracks[i].base);
                tracks[i].base += add;
                free -= add;
            }
        }
    } else {
        for t in tracks.iter_mut().filter(|t| t.flex().is_none()) {
            t.base = t.limit.max(t.base);
        }
    }
    // Flexible tracks share what is left (or, without a definite size,
    // get what their content needs per fr)
    let flex_sum: f32 = tracks.iter().filter_map(|t| t.flex()).sum();
    if flex_sum > 0.0 {
        let fr = match avail {
            Some(avail) => {
                let mut inflexible: Vec<bool> = tracks.iter().map(|t| t.flex().is_none()).collect();
                let mut fr = 0.0;
                for _ in 0..tracks.len() + 1 {
                    let used: f32 = tracks.iter().zip(&inflexible).filter(|(_, inf)| **inf).map(|(t, _)| t.base).sum();
                    let flex: f32 = tracks.iter().zip(&inflexible).filter(|(_, inf)| !**inf).filter_map(|(t, _)| t.flex()).sum();
                    fr = ((avail - used - gaps) / flex.max(1.0)).max(0.0);
                    let mut changed = false;
                    for (i, t) in tracks.iter().enumerate() {
                        if !inflexible[i] && t.flex().is_some_and(|f| t.base > fr * f) {
                            inflexible[i] = true;
                            changed = true;
                        }
                    }
                    if !changed {
                        break;
                    }
                }
                fr
            }
            None => {
                let single = tracks.iter().enumerate().filter_map(|(i, t)| t.flex().map(|f| max_seen[i].max(t.base) / f.max(1.0))).fold(0.0, f32::max);
                // Items spanning flexible tracks: what they need beyond
                // their other tracks, per fr of the flexible ones
                let spanning = spans.iter().zip(contrib).filter(|((s, e), _)| e - s > 1).filter_map(|(&(s, e), &(_, cmax))| {
                    let flex: f32 = tracks[s..e].iter().filter_map(|t| t.flex()).sum();
                    let fixed: f32 = tracks[s..e].iter().filter(|t| t.flex().is_none()).map(|t| t.base).sum();
                    (flex > 0.0).then(|| (cmax - fixed - gap * (e - s - 1) as f32).max(0.0) / flex.max(1.0))
                });
                spanning.fold(single, f32::max)
            }
        };
        for t in tracks.iter_mut() {
            if let Some(f) = t.flex() {
                t.base = t.base.max(fr * f);
            }
        }
    } else if stretch {
        // Auto tracks stretch over the free space
        if let Some(avail) = avail {
            let free = avail - tracks.iter().map(|t| t.base).sum::<f32>() - gaps;
            let autos: Vec<usize> = (0..tracks.len()).filter(|&i| tracks[i].size.max == Breadth::Auto).collect();
            if free > 0.0 && !autos.is_empty() {
                for &i in &autos {
                    tracks[i].base += free / autos.len() as f32;
                }
            }
        }
    }
}

/// Offsets of tracks with content distribution: (start, extra between)
fn distribute(free: f32, n: usize, how: JustifyContent) -> (f32, f32) {
    if free <= 0.0 || n == 0 {
        return (0.0, 0.0);
    }
    let n = n as f32;
    match how {
        JustifyContent::Center => (free / 2.0, 0.0),
        JustifyContent::End | JustifyContent::FlexEnd | JustifyContent::Right => (free, 0.0),
        JustifyContent::SpaceBetween if n > 1.0 => (0.0, free / (n - 1.0)),
        JustifyContent::SpaceAround => (free / n / 2.0, free / n),
        JustifyContent::SpaceEvenly => (free / (n + 1.0), free / (n + 1.0)),
        _ => (0.0, 0.0),
    }
}

fn align_content_as_justify(a: AlignContent) -> JustifyContent {
    match a {
        AlignContent::Center => JustifyContent::Center,
        AlignContent::End | AlignContent::FlexEnd => JustifyContent::End,
        AlignContent::SpaceBetween => JustifyContent::SpaceBetween,
        AlignContent::SpaceAround => JustifyContent::SpaceAround,
        AlignContent::SpaceEvenly => JustifyContent::SpaceEvenly,
        AlignContent::Stretch => JustifyContent::Stretch,
        _ => JustifyContent::Normal,
    }
}

/// Self-alignment in one axis: (stretch, offset fraction of free space)
fn self_align(value: AlignItems) -> (bool, f32) {
    match value {
        AlignItems::Normal | AlignItems::Stretch => (true, 0.0),
        AlignItems::Center => (false, 0.5),
        AlignItems::End | AlignItems::FlexEnd | AlignItems::SelfEnd => (false, 1.0),
        _ => (false, 0.0),
    }
}

fn self_value(own: AlignSelf, container: AlignItems) -> AlignItems {
    match own {
        AlignSelf::Auto => container,
        AlignSelf::Normal => AlignItems::Normal,
        AlignSelf::Stretch => AlignItems::Stretch,
        AlignSelf::FlexStart => AlignItems::FlexStart,
        AlignSelf::FlexEnd => AlignItems::FlexEnd,
        AlignSelf::Center => AlignItems::Center,
        AlignSelf::Baseline => AlignItems::Baseline,
        AlignSelf::Start => AlignItems::Start,
        AlignSelf::End => AlignItems::End,
        AlignSelf::SelfStart => AlignItems::SelfStart,
        AlignSelf::SelfEnd => AlignItems::SelfEnd,
    }
}

/// Prepared grid: in-flow items with their areas and the track sizing
/// functions of the final grid
struct Prepared<'a> {
    items: Vec<&'a LayoutBox>,
    areas: Vec<Area>,
    cols: Vec<TrackSize>,
    rows: Vec<TrackSize>,
}

fn prepare<'a>(style: &Style, items: &'a [LayoutBox], width: Option<f32>, height: Option<f32>, gaps: (f32, f32)) -> Prepared<'a> {
    let g: &GridStyle = &style.box_.grid;
    let mut ec = expand(&g.template_columns, width, gaps.0);
    let mut er = expand(&g.template_rows, height, gaps.1);
    let mut col_areas = Vec::new();
    let mut row_areas = Vec::new();
    if let Some(a) = &g.areas {
        while ec.tracks.len() < a.columns as usize {
            ec.tracks.push(g.auto_columns[0]);
            ec.names.push(Vec::new());
        }
        while er.tracks.len() < a.rows as usize {
            er.tracks.push(g.auto_rows[0]);
            er.names.push(Vec::new());
        }
        for (name, r0, c0, r1, c1) in a.areas.iter() {
            col_areas.push((name.clone(), *c0, *c1));
            row_areas.push((name.clone(), *r0, *r1));
        }
    }
    let mut order: Vec<usize> = (0..items.len()).filter(|&i| !items[i].style.is_out_of_flow()).collect();
    order.sort_by_key(|&i| items[i].style.box_.order);
    let placed: Vec<(&LayoutBox, Place, Place)> = order
        .iter()
        .map(|&i| {
            let b = &items[i];
            let s = &b.style.box_.grid;
            let c = resolve(&s.column_start, &s.column_end, &ec.names, &col_areas, ec.tracks.len() as i32);
            let r = resolve(&s.row_start, &s.row_end, &er.names, &row_areas, er.tracks.len() as i32);
            (b, c, r)
        })
        .collect();
    let col_shift = placed.iter().filter_map(|p| if let Place::Definite(s, _) = p.1 { Some((-s).max(0)) } else { None }).max().unwrap_or(0) as usize;
    let row_shift = placed.iter().filter_map(|p| if let Place::Definite(s, _) = p.2 { Some((-s).max(0)) } else { None }).max().unwrap_or(0) as usize;
    let (areas, ncols, nrows) = place(&placed, ec.tracks.len(), g.flow_column, g.flow_dense);
    // Implicit tracks before (from negative lines) and after the explicit
    let track_list = |explicit: &[TrackSize], auto: &[TrackSize], before: usize, total: usize| -> Vec<TrackSize> {
        (0..total.max(explicit.len() + before))
            .map(|i| if i >= before && i - before < explicit.len() { explicit[i - before] } else { auto[i % auto.len().max(1)] })
            .collect()
    };
    let cols = track_list(&ec.tracks, &g.auto_columns, col_shift, ncols);
    let rows = track_list(&er.tracks, &g.auto_rows, row_shift, nrows);
    Prepared { items: placed.iter().map(|p| p.0).collect(), areas, cols, rows }
}

fn gaps(style: &Style, width: f32, height: Option<f32>) -> (f32, f32) {
    let b = &style.box_;
    (b.column_gap.map_or(0.0, |g| g.resolve(width)), b.row_gap.map_or(0.0, |g| g.resolve(height.unwrap_or(0.0))))
}

fn new_tracks(sizes: &[TrackSize]) -> Vec<Track> {
    sizes.iter().map(|&size| Track { size, base: 0.0, limit: 0.0 }).collect()
}

/// Lay out a grid container's items in its content box
pub fn layout_grid(ctx: &mut LayoutCtx, style: &Style, items: &[LayoutBox], width: f32, height: Option<f32>) -> GridLayout {
    let (cgap, rgap) = gaps(style, width, height);
    let p = prepare(style, items, Some(width), height, (cgap, rgap));
    let mut frags = Vec::new();
    for b in items.iter().filter(|b| b.style.is_out_of_flow()) {
        frags.push(Fragment::Box(placeholder(b, Rect::default())));
    }

    // Columns from the items' min/max-content widths
    let col_spans: Vec<(usize, usize)> = p.areas.iter().map(|a| (a.c0, a.c1)).collect();
    let col_contrib: Vec<(f32, f32)> = p.items.iter().map(|b| intrinsic_outer(ctx, b)).collect();
    let mut cols = new_tracks(&p.cols);
    let justify = style.box_.justify_content;
    size_tracks(&mut cols, &col_spans, &col_contrib, Some(width), cgap, matches!(justify, JustifyContent::Normal | JustifyContent::Stretch));
    let span_size = |tracks: &[Track], s: usize, e: usize, gap: f32| tracks[s..e].iter().map(|t| t.base).sum::<f32>() + gap * (e - s).saturating_sub(1) as f32;

    // Items at their column widths, for row heights
    let justify_items = AlignItems::from_u8(style.box_.grid.justify_items).unwrap_or(AlignItems::Normal);
    let mut laid = Vec::with_capacity(p.items.len());
    let mut row_contrib = Vec::with_capacity(p.items.len());
    for (b, a) in p.items.iter().zip(&p.areas) {
        let area_w = span_size(&cols, a.c0, a.c1, cgap);
        let jself = AlignSelf::from_u8(b.style.box_.grid.justify_self).unwrap_or(AlignSelf::Auto);
        let (stretch, _) = self_align(self_value(jself, justify_items));
        let m = b.style.box_.margin.map(|m| m.resolve(area_w));
        let forced_w = (stretch && b.style.box_.width.is_auto() && m[1].is_some() && m[3].is_some() && !matches!(b.kind, super::box_tree::BoxKind::Replaced(_)))
            .then(|| (area_w - m[1].unwrap_or(0.0) - m[3].unwrap_or(0.0)).max(0.0));
        let l = layout_sized(ctx, b, area_w, None, Sizing::Shrink, false, Forced { width: forced_w, height: None, root: true });
        let outer = l.mt.size() + l.frag.border_box.h + l.mb.size();
        row_contrib.push((outer, outer));
        laid.push(l);
    }
    let row_spans: Vec<(usize, usize)> = p.areas.iter().map(|a| (a.r0, a.r1)).collect();
    let mut rows = new_tracks(&p.rows);
    let align_content = style.box_.align_content;
    size_tracks(&mut rows, &row_spans, &row_contrib, height, rgap, matches!(align_content, AlignContent::Normal | AlignContent::Stretch));

    // Track positions with content distribution
    let total = |tracks: &[Track], gap: f32| tracks.iter().map(|t| t.base).sum::<f32>() + gap * tracks.len().saturating_sub(1) as f32;
    let (cw, rh) = (total(&cols, cgap), total(&rows, rgap));
    let (cx0, cbetween) = distribute(width - cw, cols.len(), justify);
    let content_h = height.unwrap_or(rh);
    let (ry0, rbetween) = distribute(content_h - rh, rows.len(), align_content_as_justify(align_content));
    let positions = |tracks: &[Track], start: f32, gap: f32| -> Vec<f32> {
        let mut at = start;
        tracks
            .iter()
            .map(|t| {
                let p = at;
                at += t.base + gap;
                p
            })
            .collect()
    };
    let col_x = positions(&cols, cx0, cgap + cbetween);
    let row_y = positions(&rows, ry0, rgap + rbetween);

    let align_items = style.box_.align_items;
    for ((b, a), mut l) in p.items.iter().zip(&p.areas).zip(laid) {
        let area_w = span_size(&cols, a.c0, a.c1, cgap + cbetween);
        let area_h = span_size(&rows, a.r0, a.r1, rgap + rbetween);
        // Stretch in the block axis: lay out again at the row height
        let (vstretch, vfrac) = self_align(self_value(b.style.box_.align_self, align_items));
        let m = b.style.box_.margin;
        let (mt, mb) = (m[0].resolve(area_w), m[2].resolve(area_w));
        if vstretch && b.style.box_.height.is_auto() && mt.is_some() && mb.is_some() && !matches!(b.kind, super::box_tree::BoxKind::Replaced(_)) {
            let target = (area_h - mt.unwrap_or(0.0) - mb.unwrap_or(0.0)).max(0.0);
            if (target - l.frag.border_box.h).abs() > 0.01 {
                l = layout_sized(ctx, b, area_w, Some(area_h), Sizing::Shrink, false, Forced { width: Some(l.frag.border_box.w), height: Some(target), root: true });
            }
        }
        let jself = AlignSelf::from_u8(b.style.box_.grid.justify_self).unwrap_or(AlignSelf::Auto);
        let (_, hfrac) = self_align(self_value(jself, justify_items));
        let outer_w = l.ml + l.frag.border_box.w + l.mr;
        let outer_h = l.mt.size() + l.frag.border_box.h + l.mb.size();
        // Auto margins center within the area
        let hfree = (area_w - outer_w).max(0.0);
        let vfree = (area_h - outer_h).max(0.0);
        let (ml_auto, mr_auto) = (m[3].is_auto(), m[1].is_auto());
        let hoff = if ml_auto && mr_auto { hfree / 2.0 } else if ml_auto { hfree } else if mr_auto { 0.0 } else { hfree * hfrac };
        let (mt_auto, mb_auto) = (m[0].is_auto(), m[2].is_auto());
        let voff = if mt_auto && mb_auto { vfree / 2.0 } else if mt_auto { vfree } else if mb_auto { 0.0 } else { vfree * vfrac };
        let mut x = col_x.get(a.c0).copied().unwrap_or(0.0) + hoff + l.ml;
        // Columns run from the right in right-to-left containers
        if style.inherited.direction == fos_css::style::Direction::Rtl {
            x = width - x - l.frag.border_box.w;
        }
        let y = row_y.get(a.r0).copied().unwrap_or(0.0) + voff + l.mt.size();
        let rel = relative_offset(&b.style, width, height);
        let (dx, dy) = (x - l.frag.border_box.x + rel.0, y - l.frag.border_box.y + rel.1);
        l.frag.translate(dx, dy);
        frags.push(Fragment::Box(l.frag));
    }
    GridLayout { frags, height: if height.is_some() { content_h } else { rh } }
}

/// A grid container's min- and max-content widths
pub fn intrinsic_grid(ctx: &mut LayoutCtx, style: &Style, items: &[LayoutBox]) -> (f32, f32) {
    let (cgap, _) = gaps(style, 0.0, None);
    let p = prepare(style, items, None, None, (cgap, 0.0));
    let spans: Vec<(usize, usize)> = p.areas.iter().map(|a| (a.c0, a.c1)).collect();
    let contrib: Vec<(f32, f32)> = p.items.iter().map(|b| intrinsic_outer(ctx, b)).collect();
    let mins: Vec<(f32, f32)> = contrib.iter().map(|c| (c.0, c.0)).collect();
    let mut min_tracks = new_tracks(&p.cols);
    size_tracks(&mut min_tracks, &spans, &mins, None, cgap, false);
    let mut max_tracks = new_tracks(&p.cols);
    size_tracks(&mut max_tracks, &spans, &contrib, None, cgap, false);
    let total = |t: &[Track]| t.iter().map(|t| t.base).sum::<f32>() + cgap * t.len().saturating_sub(1) as f32;
    let (min, max) = (total(&min_tracks), total(&max_tracks));
    (min, max.max(min))
}
