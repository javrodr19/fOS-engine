//! Flexbox (CSS Flexible Box Layout 1, §9): base sizes, line collection,
//! resolving flexible lengths, main-axis justification with auto margins,
//! cross-axis alignment (stretch, baselines), multi-line alignment, order
//! and reversed directions

use fos_css::style::{AlignContent, AlignItems, AlignSelf, FlexWrap, JustifyContent, LpAuto, MaxSize, Overflow, Style};

use super::block::{content_size, intrinsic_content, intrinsic_outer, layout_sized, relative_offset, Forced, Laid, LayoutCtx, Sizing};
use super::box_tree::LayoutBox;
use super::fragment::Fragment;

pub struct FlexLayout {
    pub frags: Vec<Fragment>,
    /// Content height of the container
    pub height: f32,
}

struct Item<'a> {
    b: &'a LayoutBox,
    /// Margins (auto as 0): top, right, bottom, left
    margin: [f32; 4],
    auto_margin: [bool; 4],
    /// Border-box sizes along the main axis
    base: f32,
    hypo: f32,
    min: f32,
    max: f32,
    target: f32,
    frozen: bool,
    laid: Option<Laid>,
    /// Border-box cross size, once laid out
    cross: f32,
}

impl Item<'_> {
    fn main_margins(&self, row: bool) -> f32 {
        if row {
            self.margin[1] + self.margin[3]
        } else {
            self.margin[0] + self.margin[2]
        }
    }

    fn cross_margins(&self, row: bool) -> f32 {
        if row {
            self.margin[0] + self.margin[2]
        } else {
            self.margin[1] + self.margin[3]
        }
    }
}

/// Horizontal and vertical border + padding
fn bp(style: &Style, cb_w: f32) -> (f32, f32) {
    let b = &style.border.width;
    let p = style.box_.padding.map(|p| p.resolve(cb_w).max(0.0));
    (b[1] + b[3] + p[1] + p[3], b[0] + b[2] + p[0] + p[2])
}

fn align_of(item: &Style, container: &Style) -> AlignItems {
    match item.box_.align_self {
        AlignSelf::Auto => container.box_.align_items,
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

/// Lay out a flex container's items in its content box (`width` wide,
/// `height` tall when definite)
pub fn layout_flex(ctx: &mut LayoutCtx, style: &Style, items: &[LayoutBox], width: f32, height: Option<f32>, _cb_w: f32) -> FlexLayout {
    let bx = &style.box_;
    let row = bx.flex_direction.is_row();
    let reverse = bx.flex_direction.is_reverse();
    let wrap = bx.flex_wrap != FlexWrap::Nowrap;
    let main_size = if row { Some(width) } else { height };
    let cross_size = if row { height } else { Some(width) };
    let (row_gap, col_gap) = (bx.row_gap.map_or(0.0, |g| g.resolve(height.unwrap_or(0.0))), bx.column_gap.map_or(0.0, |g| g.resolve(width)));
    let (gap_main, gap_cross) = if row { (col_gap, row_gap) } else { (row_gap, col_gap) };

    let mut frags = Vec::new();
    let mut order: Vec<usize> = (0..items.len()).collect();
    order.sort_by_key(|&i| items[i].style.box_.order);

    // Base and hypothetical main sizes
    let mut flex: Vec<Item> = Vec::new();
    for &i in &order {
        let b = &items[i];
        if b.style.is_out_of_flow() {
            // Its static position is the container's content start
            frags.push(Fragment::Box(super::block::placeholder(b, super::fragment::Rect::default())));
            continue;
        }
        let s = &b.style;
        let (hbp, vbp) = bp(s, width);
        let (bp_main, _) = if row { (hbp, vbp) } else { (vbp, hbp) };
        let m = s.box_.margin;
        let auto_margin = m.map(|x| x.is_auto());
        let margin = m.map(|x| x.resolve(width).unwrap_or(0.0));
        let border_box = |l: f32| content_size(s, l, bp_main) + bp_main;
        let main_prop = if row { s.box_.width } else { s.box_.height };
        let definite = |v: LpAuto| v.resolve_definite(main_size).map(border_box);
        let mut content_laid: Option<Laid> = None;
        let align = align_of(s, style);
        let stretch_cross = matches!(align, AlignItems::Stretch | AlignItems::Normal) && (if row { s.box_.height } else { s.box_.width }).is_auto();
        let layout_column = |ctx: &mut LayoutCtx| -> Laid {
            // A column item's height at its cross size
            let cross_w = if stretch_cross { Some((width - margin[1] - margin[3]).max(0.0)) } else { None };
            layout_sized(ctx, b, width, height, Sizing::Shrink, false, Forced { width: cross_w, height: None })
        };
        let base = match (s.box_.flex_basis, definite(s.box_.flex_basis)) {
            (LpAuto::Lp(_), Some(v)) => v,
            _ => match definite(main_prop) {
                Some(v) => v,
                None if row => intrinsic_content(ctx, b).1 + bp_main,
                None => {
                    let laid = layout_column(ctx);
                    let h = laid.frag.border_box.h;
                    content_laid = Some(laid);
                    h
                }
            },
        };
        let clips = s.box_.overflow_x != Overflow::Visible || s.box_.overflow_y != Overflow::Visible;
        let min_prop = if row { s.box_.min_width } else { s.box_.min_height };
        let min = match min_prop.resolve_definite(main_size) {
            Some(v) => border_box(v),
            None if min_prop.is_auto() && !clips => {
                // Automatic minimum: the content size, capped by a specified
                // size
                let content = if row {
                    intrinsic_content(ctx, b).0 + bp_main
                } else {
                    match &content_laid {
                        Some(l) => l.frag.border_box.h,
                        None => {
                            let laid = layout_column(ctx);
                            let h = laid.frag.border_box.h;
                            content_laid = Some(laid);
                            h
                        }
                    }
                };
                definite(main_prop).map_or(content, |v| v.min(content))
            }
            None => 0.0,
        };
        let max_prop = if row { s.box_.max_width } else { s.box_.max_height };
        let max = match max_prop {
            MaxSize::None => f32::INFINITY,
            MaxSize::Lp(l) if l.has_percent() => main_size.map_or(f32::INFINITY, |m| border_box(l.resolve(m))),
            MaxSize::Lp(l) => border_box(l.px),
        };
        let hypo = base.min(max).max(min);
        let _ = content_laid;
        flex.push(Item { b, margin, auto_margin, base, hypo, min, max, target: hypo, frozen: false, laid: None, cross: 0.0 });
    }

    // Lines
    let avail_main = main_size.unwrap_or(f32::INFINITY);
    let mut lines: Vec<std::ops::Range<usize>> = Vec::new();
    let mut start = 0;
    let mut used = 0.0;
    for (i, it) in flex.iter().enumerate() {
        let outer = it.hypo + it.main_margins(row);
        let with_gap = if i > start { used + gap_main + outer } else { outer };
        if wrap && i > start && with_gap > avail_main + 0.01 {
            lines.push(start..i);
            start = i;
            used = outer;
        } else {
            used = with_gap;
        }
    }
    if start < flex.len() || lines.is_empty() {
        lines.push(start..flex.len());
    }

    // Resolve flexible lengths (§9.7)
    for line in &lines {
        let items = &mut flex[line.clone()];
        let gaps = gap_main * items.len().saturating_sub(1) as f32;
        let used: f32 = items.iter().map(|it| it.hypo + it.main_margins(row)).sum::<f32>() + gaps;
        let Some(avail) = main_size else { continue };
        let grow = avail > used;
        for it in items.iter_mut() {
            let s = &it.b.style.box_;
            let factor = if grow { s.flex_grow } else { s.flex_shrink };
            it.frozen = factor == 0.0 || (grow && it.base > it.hypo) || (!grow && it.base < it.hypo);
            it.target = it.hypo;
        }
        let initial_free = avail - items.iter().map(|it| if it.frozen { it.target } else { it.base } + it.main_margins(row)).sum::<f32>() - gaps;
        for _ in 0..=items.len() {
            if items.iter().all(|it| it.frozen) {
                break;
            }
            let mut free = avail - items.iter().map(|it| if it.frozen { it.target } else { it.base } + it.main_margins(row)).sum::<f32>() - gaps;
            let unfrozen = || items.iter().filter(|it| !it.frozen);
            let sum_factors: f32 = unfrozen().map(|it| if grow { it.b.style.box_.flex_grow } else { it.b.style.box_.flex_shrink }).sum();
            if sum_factors < 1.0 {
                let scaled = initial_free * sum_factors;
                if scaled.abs() < free.abs() {
                    free = scaled;
                }
            }
            let sum_scaled: f32 = unfrozen().map(|it| it.b.style.box_.flex_shrink * it.base).sum();
            for it in items.iter_mut().filter(|it| !it.frozen) {
                let s = &it.b.style.box_;
                it.target = if grow {
                    if sum_factors > 0.0 { it.base + free * s.flex_grow / sum_factors } else { it.base }
                } else if sum_scaled > 0.0 {
                    it.base + free * s.flex_shrink * it.base / sum_scaled
                } else {
                    it.base
                };
            }
            // Clamp, then freeze the violators of the overall direction
            let mut total = 0.0;
            let mut clamped = Vec::with_capacity(items.len());
            for it in items.iter() {
                let c = if it.frozen { it.target } else { it.target.min(it.max).max(it.min).max(0.0) };
                total += c - it.target;
                clamped.push(c);
            }
            for (it, c) in items.iter_mut().zip(clamped) {
                if it.frozen {
                    continue;
                }
                let freeze = total.abs() < 0.01 || (total > 0.0 && c > it.target) || (total < 0.0 && c < it.target);
                if freeze {
                    it.frozen = true;
                }
                it.target = c;
            }
        }
    }

    // Lay out each item at its main size; cross sizes from content
    for it in flex.iter_mut() {
        let s = &it.b.style;
        let forced = if row {
            Forced { width: Some(it.target), height: None }
        } else {
            let stretch = matches!(align_of(s, style), AlignItems::Stretch | AlignItems::Normal) && s.box_.width.is_auto() && !it.auto_margin[1] && !it.auto_margin[3];
            Forced { width: stretch.then(|| (width - it.margin[1] - it.margin[3]).max(0.0)), height: Some(it.target) }
        };
        let laid = layout_sized(ctx, it.b, width, height, Sizing::Shrink, false, forced);
        it.cross = if row { laid.frag.border_box.h } else { laid.frag.border_box.w };
        it.laid = Some(laid);
    }

    // Line cross sizes
    let mut line_cross: Vec<f32> = lines.iter().map(|l| flex[l.clone()].iter().map(|it| it.cross + it.cross_margins(row)).fold(0.0, f32::max)).collect();
    // Baseline-aligned items in a row line need room for their alignment
    if row {
        for (li, l) in lines.iter().enumerate() {
            let (mut above, mut below) = (0.0f32, 0.0f32);
            for it in &flex[l.clone()] {
                if align_of(&it.b.style, style) == AlignItems::Baseline {
                    let f = &it.laid.as_ref().unwrap().frag;
                    let bl = f.first_baseline().map_or(f.border_box.h, |b| b - f.border_box.y) + it.margin[0];
                    above = above.max(bl);
                    below = below.max(it.cross + it.cross_margins(row) - bl);
                }
            }
            line_cross[li] = line_cross[li].max(above + below);
        }
    }
    if !wrap {
        if let Some(c) = cross_size {
            line_cross[0] = c;
        }
    } else if let Some(c) = cross_size {
        // align-content: stretch (and normal) share leftover space
        let total: f32 = line_cross.iter().sum::<f32>() + gap_cross * (lines.len().saturating_sub(1)) as f32;
        if matches!(bx.align_content, AlignContent::Normal | AlignContent::Stretch) && c > total {
            let extra = (c - total) / lines.len() as f32;
            for lc in &mut line_cross {
                *lc += extra;
            }
        }
    }

    // Stretched items get the line's cross size
    for (li, l) in lines.iter().enumerate() {
        for it in &mut flex[l.clone()] {
            let s = &it.b.style;
            let auto_cross = if row { s.box_.height.is_auto() } else { s.box_.width.is_auto() };
            let cross_auto_margins = if row { it.auto_margin[0] || it.auto_margin[2] } else { it.auto_margin[1] || it.auto_margin[3] };
            if row && auto_cross && !cross_auto_margins && matches!(align_of(s, style), AlignItems::Stretch | AlignItems::Normal) {
                let mut target = (line_cross[li] - it.cross_margins(row)).max(0.0);
                let (_, vbp) = bp(s, width);
                target = super::block::clamp_height(s, (target - vbp).max(0.0), height, vbp) + vbp;
                if (target - it.cross).abs() > 0.01 {
                    let laid = layout_sized(ctx, it.b, width, height, Sizing::Shrink, false, Forced { width: Some(it.target), height: Some(target) });
                    it.cross = target;
                    it.laid = Some(laid);
                }
            }
        }
    }

    // Cross positions of lines
    let lines_total: f32 = line_cross.iter().sum::<f32>() + gap_cross * (lines.len().saturating_sub(1)) as f32;
    let container_cross = cross_size.unwrap_or(lines_total);
    let free_cross = container_cross - lines_total;
    let n = lines.len() as f32;
    let (mut cross_pos, cross_between) = if !wrap || free_cross <= 0.0 {
        (0.0, 0.0)
    } else {
        match bx.align_content {
            AlignContent::FlexEnd | AlignContent::End => (free_cross, 0.0),
            AlignContent::Center => (free_cross / 2.0, 0.0),
            AlignContent::SpaceBetween if n > 1.0 => (0.0, free_cross / (n - 1.0)),
            AlignContent::SpaceAround => (free_cross / n / 2.0, free_cross / n),
            AlignContent::SpaceEvenly => (free_cross / (n + 1.0), free_cross / (n + 1.0)),
            _ => (0.0, 0.0),
        }
    };
    let main_total = main_size.unwrap_or_else(|| {
        lines.iter().map(|l| flex[l.clone()].iter().map(|it| it.target + it.main_margins(row)).sum::<f32>() + gap_main * l.len().saturating_sub(1) as f32).fold(0.0, f32::max)
    });

    for (li, l) in lines.iter().enumerate() {
        let lc = line_cross[li];
        let items = &mut flex[l.clone()];
        let count = items.len() as f32;
        let used: f32 = items.iter().map(|it| it.target + it.main_margins(row)).sum::<f32>() + gap_main * (items.len().saturating_sub(1)) as f32;
        let mut free = main_total - used;
        // Auto margins take free space first
        let (ms, me) = if row { (3, 1) } else { (0, 2) };
        let autos = items.iter().map(|it| it.auto_margin[ms] as u32 + it.auto_margin[me] as u32).sum::<u32>();
        let auto_share = if autos > 0 && free > 0.0 {
            let s = free / autos as f32;
            free = 0.0;
            s
        } else {
            0.0
        };
        let (mut pos, between) = if free <= 0.0 && !matches!(bx.justify_content, JustifyContent::Center | JustifyContent::FlexEnd | JustifyContent::End | JustifyContent::Right) {
            (0.0, 0.0)
        } else {
            match bx.justify_content {
                JustifyContent::FlexEnd | JustifyContent::End | JustifyContent::Right => (free, 0.0),
                JustifyContent::Center => (free / 2.0, 0.0),
                JustifyContent::SpaceBetween if count > 1.0 => (0.0, free / (count - 1.0)),
                JustifyContent::SpaceAround => (free / count / 2.0, free / count),
                JustifyContent::SpaceEvenly => (free / (count + 1.0), free / (count + 1.0)),
                _ => (0.0, 0.0),
            }
        };
        // Baselines of the line's baseline-aligned items
        let max_baseline = if row {
            items
                .iter()
                .filter(|it| align_of(&it.b.style, style) == AlignItems::Baseline)
                .map(|it| {
                    let f = &it.laid.as_ref().unwrap().frag;
                    f.first_baseline().map_or(f.border_box.h, |b| b - f.border_box.y) + it.margin[0]
                })
                .fold(0.0, f32::max)
        } else {
            0.0
        };
        for it in items.iter_mut() {
            let lead = it.margin[ms] + if it.auto_margin[ms] { auto_share } else { 0.0 };
            let trail = it.margin[me] + if it.auto_margin[me] { auto_share } else { 0.0 };
            let main_start = pos + lead;
            pos += lead + it.target + trail + gap_main + between;
            // Cross alignment within the line
            let (cs, ce) = if row { (0, 2) } else { (3, 1) };
            let outer_cross = it.cross + it.cross_margins(row);
            let cross_free = lc - outer_cross;
            let cross_offset = if it.auto_margin[cs] && it.auto_margin[ce] {
                it.margin[cs] + cross_free.max(0.0) / 2.0
            } else if it.auto_margin[cs] {
                it.margin[cs] + cross_free.max(0.0)
            } else if it.auto_margin[ce] {
                it.margin[cs]
            } else {
                it.margin[cs]
                    + match align_of(&it.b.style, style) {
                        AlignItems::FlexEnd | AlignItems::End | AlignItems::SelfEnd => cross_free,
                        AlignItems::Center => cross_free / 2.0,
                        AlignItems::Baseline if row => {
                            let f = &it.laid.as_ref().unwrap().frag;
                            max_baseline - (f.first_baseline().map_or(f.border_box.h, |b| b - f.border_box.y) + it.margin[0])
                        }
                        _ => 0.0,
                    }
            };
            let mut main = main_start;
            if reverse {
                main = main_total - main_start - it.target;
            }
            let mut cross = cross_pos + cross_offset;
            if bx.flex_wrap == FlexWrap::WrapReverse {
                cross = container_cross - cross - it.cross;
            }
            let (x, y) = if row { (main, cross) } else { (cross, main) };
            let mut laid = it.laid.take().expect("laid out");
            let rel = relative_offset(&it.b.style, width, height);
            let (dx, dy) = (x - laid.frag.border_box.x + rel.0, y - laid.frag.border_box.y + rel.1);
            laid.frag.translate(dx, dy);
            frags.push(Fragment::Box(laid.frag));
        }
        cross_pos += lc + gap_cross + cross_between;
    }

    let height = if row { lines_total } else { main_total };
    FlexLayout { frags, height: if row && !wrap { height.max(line_cross.first().copied().unwrap_or(0.0)) } else { height } }
}

/// A flex container's min-content and max-content widths
pub fn intrinsic_flex(ctx: &mut LayoutCtx, style: &Style, items: &[LayoutBox]) -> (f32, f32) {
    let row = style.box_.flex_direction.is_row();
    let gap = style.box_.column_gap.map_or(0.0, |g| g.resolve(0.0));
    let (mut min, mut max) = (0.0f32, 0.0f32);
    let mut n = 0;
    for b in items.iter().filter(|b| !b.style.is_out_of_flow()) {
        let (bmin, bmax) = intrinsic_outer(ctx, b);
        if row {
            max += bmax;
            if style.box_.flex_wrap == FlexWrap::Nowrap {
                min += bmin;
            } else {
                min = min.max(bmin);
            }
        } else {
            min = min.max(bmin);
            max = max.max(bmax);
        }
        n += 1;
    }
    if row && n > 1 {
        max += gap * (n - 1) as f32;
        if style.box_.flex_wrap == FlexWrap::Nowrap {
            min += gap * (n - 1) as f32;
        }
    }
    (min, max)
}
