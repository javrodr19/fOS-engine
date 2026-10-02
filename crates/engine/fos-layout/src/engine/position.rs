//! Absolute and fixed positioning (CSS 2.1 §10.3.7, §10.6.4)
//!
//! Flow layout leaves a placeholder where each out-of-flow box would have
//! been (its static position). Once the whole tree is laid out, this pass
//! finds each placeholder's containing block (the padding box of the
//! nearest positioned ancestor, the initial containing block, or the
//! viewport for fixed boxes) and replaces it with the laid-out box.

use std::collections::HashMap;

use fos_css::style::Position;
use fos_dom::NodeId;

use super::block::{clamp_height, clamp_width, content_size, edges, intrinsic_content, layout_sized, Forced, LayoutCtx, Sizing};
use super::box_tree::{BoxKind, LayoutBox};
use super::fragment::{BoxFragment, BoxFragmentKind, Fragment, Rect};

/// Replace the placeholders under `root` with positioned boxes
pub fn place_absolutes(ctx: &mut LayoutCtx, root: &mut BoxFragment, boxes: &LayoutBox) {
    let mut map = HashMap::new();
    collect(boxes, &mut map);
    if map.is_empty() {
        return;
    }
    let icb = Rect::new(0.0, 0.0, ctx.viewport.0, ctx.viewport.1);
    walk(ctx, root, icb, icb, &map);
}

/// Out-of-flow boxes by element
fn collect<'a>(b: &'a LayoutBox, map: &mut HashMap<NodeId, &'a LayoutBox>) {
    if b.style.is_out_of_flow() && b.node.is_valid() {
        map.insert(b.node, b);
    }
    match &b.kind {
        BoxKind::Block(children) | BoxKind::Flex(children) => {
            for c in children {
                collect(c, map);
            }
        }
        BoxKind::Inline(content) => {
            for item in &content.items {
                if let super::box_tree::InlineItem::Atomic(a) = item {
                    collect(a, map);
                }
            }
        }
        BoxKind::Replaced(_) => {}
    }
}

fn walk(ctx: &mut LayoutCtx, f: &mut BoxFragment, cb: Rect, viewport: Rect, map: &HashMap<NodeId, &LayoutBox>) {
    let cb = if f.style.is_positioned() && f.kind != BoxFragmentKind::Placeholder { f.padding_box() } else { cb };
    let mut changed = false;
    for c in f.children.iter_mut() {
        let Fragment::Box(child) = c else { continue };
        if child.kind == BoxFragmentKind::Placeholder {
            let Some(b) = map.get(&child.node) else { continue };
            let containing = if b.style.box_.position == Position::Fixed { viewport } else { cb };
            let mut placed = layout_absolute(ctx, b, containing, (child.border_box.x, child.border_box.y));
            // Its own absolutely positioned descendants
            walk(ctx, &mut placed, containing, viewport, map);
            *child = placed;
            changed = true;
        } else {
            let before = child.ink;
            walk(ctx, child, cb, viewport, map);
            changed |= child.ink != before;
        }
    }
    if changed {
        f.update_ink();
    }
}

/// Lay out an absolutely positioned box against containing block `cb`;
/// `static_pos` is where its margin box would have started in flow
pub fn layout_absolute(ctx: &mut LayoutCtx, b: &LayoutBox, cb: Rect, static_pos: (f32, f32)) -> BoxFragment {
    let s = &b.style;
    let e = edges(s, cb.w);
    let (hbp, vbp) = (e.horizontal_bp(), e.vertical_bp());
    let inset = s.box_.inset;
    let (top, right, bottom, left) = (inset[0].resolve(cb.h), inset[1].resolve(cb.w), inset[2].resolve(cb.h), inset[3].resolve(cb.w));
    let (mut ml, mut mr) = (e.margin[3].unwrap_or(0.0), e.margin[1].unwrap_or(0.0));
    let (mut mt, mb) = (e.margin[0].unwrap_or(0.0), e.margin[2].unwrap_or(0.0));
    let replaced = matches!(b.kind, BoxKind::Replaced(_));

    // Width
    let specified_w = s.box_.width.resolve(cb.w).map(|w| clamp_width(s, content_size(s, w, hbp), cb.w, hbp) + hbp);
    let width = match (specified_w, left, right) {
        (Some(w), _, _) => Some(w),
        _ if replaced => None,
        (None, Some(l), Some(r)) => Some(clamp_width(s, (cb.w - l - r - ml - mr - hbp).max(0.0), cb.w, hbp) + hbp),
        (None, _, _) => {
            let avail = (cb.w - left.unwrap_or(0.0) - right.unwrap_or(0.0) - ml - mr - hbp).max(0.0);
            let (min, max) = intrinsic_content(ctx, b);
            Some(clamp_width(s, max.min(avail).max(min), cb.w, hbp) + hbp)
        }
    };
    // Height: stretched between top and bottom when auto
    let height = match (s.box_.height.is_auto(), top, bottom) {
        (true, Some(t), Some(bt)) if !replaced => Some(clamp_height(s, (cb.h - t - bt - mt - mb - vbp).max(0.0), Some(cb.h), vbp) + vbp),
        _ => None,
    };
    let laid = layout_sized(ctx, b, cb.w, Some(cb.h), Sizing::Shrink, false, Forced { width, height });
    let mut frag = laid.frag;
    let (w, h) = (frag.border_box.w, frag.border_box.h);

    // Auto margins center a box with both insets and a size
    if let (Some(l), Some(r)) = (left, right) {
        let free = cb.w - l - r - w - ml - mr;
        match (e.margin[3].is_none(), e.margin[1].is_none()) {
            (true, true) if free > 0.0 => {
                ml += free / 2.0;
                mr += free / 2.0;
            }
            (true, false) => ml += free,
            _ => {}
        }
    }
    if let (Some(t), Some(bt), false) = (top, bottom, s.box_.height.is_auto()) {
        let free = cb.h - t - bt - h - mt - mb;
        match (e.margin[0].is_none(), e.margin[2].is_none()) {
            (true, true) if free > 0.0 => mt += free / 2.0,
            (true, false) => mt += free,
            _ => {}
        }
    }
    let _ = mr;
    let x = match (left, right) {
        (Some(l), _) => cb.x + l + ml,
        (None, Some(r)) => cb.right() - r - e.margin[1].unwrap_or(0.0) - w,
        (None, None) => static_pos.0 + ml,
    };
    let y = match (top, bottom) {
        (Some(t), _) => cb.y + t + mt,
        (None, Some(bt)) => cb.bottom() - bt - mb - h,
        (None, None) => static_pos.1 + mt,
    };
    frag.translate(x - frag.border_box.x, y - frag.border_box.y);
    frag
}
