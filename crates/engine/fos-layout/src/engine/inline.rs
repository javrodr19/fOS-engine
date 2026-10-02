//! Inline formatting (CSS 2.1 §9.4.2, §10.8; CSS Text 3): breaking an
//! inline formatting context's content into line boxes, aligning it
//! horizontally (text-align, text-indent) and vertically (line-height,
//! vertical-align)
//!
//! Content is first turned into measured pieces (words, spaces, inline box
//! edges, atomic inlines, forced breaks); lines are chosen greedily over the
//! pieces; then each line is positioned and turned into fragments.

use std::sync::Arc;

use fos_css::properties::Color;
use fos_css::style::{Direction, Style, TextAlign, VerticalAlign, Visibility, WordBreak};
use fos_dom::NodeId;

use super::block::{clips, intrinsic_outer, layout_block_level, relative_offset, Laid, LayoutCtx, Sizing};
use super::box_tree::{InlineContent, InlineItem};
use super::fonts::{Glyph, ResolvedFont, ShapedWord};
use super::fragment::{BoxFragment, BoxFragmentKind, Fragment, Rect, TextFragment};

pub struct InlineLayout {
    pub frags: Vec<Fragment>,
    pub height: f32,
}

enum PieceKind {
    Word { style: u32, node: NodeId, word: Arc<ShapedWord> },
    /// Spaces (`collapsible` ones vanish at line starts)
    Space { collapsible: bool },
    Open { style: u32, node: NodeId },
    Close,
    Atomic(usize),
    Break,
}

struct Piece {
    kind: PieceKind,
    width: f32,
    /// A soft wrap opportunity right before this piece
    break_before: bool,
}

/// Horizontal margin + border + padding at an inline box's start and end
fn inline_edges(style: &Style, cb_w: f32) -> (f32, f32) {
    let b = &style.box_;
    let m = |i: usize| b.margin[i].resolve(cb_w).unwrap_or(0.0);
    let p = |i: usize| b.padding[i].resolve(cb_w).max(0.0);
    (m(3) + style.border.width[3] + p(3), m(1) + style.border.width[1] + p(1))
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x2E80..=0x9FFF | 0xAC00..=0xD7AF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

/// A word with letter-spacing added after each glyph
fn spaced(word: &ShapedWord, ls: f32) -> Arc<ShapedWord> {
    let glyphs: Box<[Glyph]> = word.glyphs.iter().enumerate().map(|(i, g)| Glyph { x: g.x + ls * i as f32, ..*g }).collect();
    let n = word.glyphs.len().max(1) as f32;
    Arc::new(ShapedWord { font: word.font, size: word.size, width: word.width + ls * n, glyphs })
}

/// Measure the content into pieces; `atomic_widths` are the atomic
/// inlines' margin-box widths
fn build_pieces(ctx: &mut LayoutCtx, ic: &InlineContent, fonts: &[ResolvedFont], cb_w: f32, atomic_widths: &[f32]) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::with_capacity(ic.items.len() * 4);
    let mut opportunity = false;
    let mut stack: Vec<u32> = Vec::new();
    let mut atomic = 0;
    for item in &ic.items {
        match item {
            InlineItem::Text { start, end, style, node } => {
                let st = &ic.styles[*style as usize];
                let inh = &st.inherited;
                let font = &fonts[*style as usize];
                let wraps = inh.white_space.wraps();
                let collapsible = inh.white_space.collapses_spaces();
                let break_all = inh.word_break == WordBreak::BreakAll;
                let text = &ic.text[*start..*end];
                let mut rest = text;
                while !rest.is_empty() {
                    let spaces = rest.len() - rest.trim_start_matches(' ').len();
                    if spaces > 0 {
                        let n = spaces as f32;
                        let w = ctx.fonts.shape(font, " ").width * n + (inh.word_spacing + inh.letter_spacing) * n;
                        out.push(Piece { kind: PieceKind::Space { collapsible }, width: w, break_before: false });
                        opportunity = wraps;
                        rest = &rest[spaces..];
                        continue;
                    }
                    let word_len = rest.find(' ').unwrap_or(rest.len());
                    let word = &rest[..word_len];
                    rest = &rest[word_len..];
                    // Break opportunities inside the word
                    let mut chunk_start = 0;
                    let mut prev: Option<char> = None;
                    let mut chunks: Vec<&str> = Vec::new();
                    for (i, c) in word.char_indices() {
                        if let Some(p) = prev {
                            let split = wraps && (break_all || is_cjk(c) || is_cjk(p) || (p == '-' && c.is_alphanumeric() && i - chunk_start > 2));
                            if split {
                                chunks.push(&word[chunk_start..i]);
                                chunk_start = i;
                            }
                        }
                        prev = Some(c);
                    }
                    chunks.push(&word[chunk_start..]);
                    for (ci, chunk) in chunks.into_iter().enumerate() {
                        let mut shaped = ctx.fonts.shape(font, chunk);
                        if inh.letter_spacing != 0.0 {
                            shaped = spaced(&shaped, inh.letter_spacing);
                        }
                        let width = shaped.width;
                        out.push(Piece { kind: PieceKind::Word { style: *style, node: *node, word: shaped }, width, break_before: if ci == 0 { opportunity } else { true } });
                        opportunity = false;
                    }
                }
            }
            InlineItem::Open { style, node } => {
                let (left, _) = inline_edges(&ic.styles[*style as usize], cb_w);
                out.push(Piece { kind: PieceKind::Open { style: *style, node: *node }, width: left, break_before: opportunity });
                opportunity = false;
                stack.push(*style);
            }
            InlineItem::Close => {
                let right = stack.pop().map_or(0.0, |s| inline_edges(&ic.styles[s as usize], cb_w).1);
                out.push(Piece { kind: PieceKind::Close, width: right, break_before: false });
            }
            InlineItem::Atomic(_) => {
                let wraps = ic.styles[stack.last().copied().unwrap_or(0) as usize].inherited.white_space.wraps();
                out.push(Piece { kind: PieceKind::Atomic(atomic), width: atomic_widths.get(atomic).copied().unwrap_or(0.0), break_before: wraps });
                atomic += 1;
                opportunity = wraps;
            }
            InlineItem::Break => {
                out.push(Piece { kind: PieceKind::Break, width: 0.0, break_before: false });
                opportunity = false;
            }
        }
    }
    out
}

/// The end of the line starting at piece `start` (greedy: as many
/// pieces as fit in `avail`, breaking at the last opportunity)
fn next_line(pieces: &[Piece], start: usize, avail: f32, indent: f32) -> usize {
    let mut x = indent;
    let mut last_opportunity: Option<usize> = None;
    let mut content = false;
    let mut i = start;
    while i < pieces.len() {
        let p = &pieces[i];
        if p.break_before && i > start && content {
            last_opportunity = Some(i);
        }
        match p.kind {
            PieceKind::Break => return i + 1,
            // Spaces hang at line ends: they never overflow
            PieceKind::Space { collapsible } => {
                if content || !collapsible {
                    x += p.width;
                }
            }
            PieceKind::Word { .. } | PieceKind::Atomic(_) => {
                x += p.width;
                if x > avail + 0.01 {
                    if let Some(o) = last_opportunity {
                        return o;
                    }
                }
                content = true;
            }
            PieceKind::Open { .. } | PieceKind::Close => x += p.width,
        }
        i += 1;
    }
    pieces.len()
}

/// The width of the unbreakable run starting a line at `start`
fn first_run_width(pieces: &[Piece], start: usize) -> f32 {
    let mut w = 0.0;
    let mut content = false;
    for p in &pieces[start..] {
        if content && p.break_before {
            break;
        }
        match p.kind {
            PieceKind::Break => break,
            PieceKind::Space { .. } => {
                if content {
                    break;
                }
            }
            PieceKind::Word { .. } | PieceKind::Atomic(_) => {
                w += p.width;
                content = true;
            }
            _ => w += p.width,
        }
    }
    w
}

/// An inline box open on the line being built
struct OpenBox {
    style: u32,
    node: NodeId,
    /// Its baseline, relative to the line's root baseline (down positive)
    shift: f32,
    /// Where its margin box starts, and whether its start edge is on this
    /// line
    x: f32,
    has_start: bool,
    children: Vec<Fragment>,
    /// The style of the last text fragment in `children`, to extend it
    last_text: Option<(u32, NodeId)>,
    decoration: (u8, Color),
}

fn half_leading_extents(font: &ResolvedFont, line_height: f32, shift: f32) -> (f32, f32) {
    let (a, d) = (font.ascent(), font.descent());
    let half = (line_height - (a + d)) / 2.0;
    (shift - a - half, shift + d + half)
}

/// The baseline shift of a box with `font` inside a parent box with
/// `parent` font at `parent_shift`; `top_to_baseline` and `height` describe
/// atomic inlines' margin boxes (ascent and ascent+descent for inline
/// boxes)
fn baseline_shift(va: VerticalAlign, parent: &ResolvedFont, parent_shift: f32, top_to_baseline: f32, height: f32, line_height: f32) -> f32 {
    let size = parent.size;
    match va {
        VerticalAlign::Baseline => parent_shift,
        VerticalAlign::Sub => parent_shift + size * 0.2,
        VerticalAlign::Super => parent_shift - size * 0.34,
        // The box's middle at the parent's baseline plus half an x-height
        VerticalAlign::Middle => parent_shift - size * 0.25 - height / 2.0 + top_to_baseline,
        VerticalAlign::TextTop | VerticalAlign::Top => parent_shift - parent.ascent() + top_to_baseline,
        VerticalAlign::TextBottom | VerticalAlign::Bottom => parent_shift + parent.descent() - (height - top_to_baseline),
        VerticalAlign::Length(l) => parent_shift - l.resolve(line_height),
    }
}

/// An atomic inline's distance from its margin-box top to its baseline
fn atomic_baseline(laid: &Laid) -> f32 {
    let f = &laid.frag;
    let mt = laid.mt.size();
    let bottom = mt + f.border_box.h + laid.mb.size();
    match f.kind {
        BoxFragmentKind::Replaced => f.first_baseline().map_or(bottom, |b| mt + b),
        _ if clips(&f.style) => bottom,
        _ => f.last_baseline().map_or(bottom, |b| mt + b),
    }
}

/// Lay out inline content in a box `avail` wide whose content box starts
/// at `base` in its formatting context (floats shorten lines)
pub fn layout_inline(ctx: &mut LayoutCtx, ic: &InlineContent, avail: f32, cb_h: Option<f32>, base: (f32, f32)) -> InlineLayout {
    let fonts: Vec<ResolvedFont> = ic.styles.iter().map(|s| ctx.fonts.resolve(s)).collect();
    // Atomic inlines are laid out first: their widths drive line breaking
    let mut atomics: Vec<Option<Laid>> = Vec::new();
    for item in &ic.items {
        if let InlineItem::Atomic(b) = item {
            atomics.push(Some(layout_block_level(ctx, b, avail, cb_h, Sizing::Shrink, false)));
        }
    }
    let atomic_styles: Vec<&Style> = ic.items.iter().filter_map(|i| if let InlineItem::Atomic(b) = i { Some(&b.style) } else { None }).collect();
    let widths: Vec<f32> = atomics.iter().map(|a| a.as_ref().map_or(0.0, |a| a.margin_box_width())).collect();
    let pieces = build_pieces(ctx, ic, &fonts, avail, &widths);

    let container = &ic.styles[0];
    let indent = container.inherited.text_indent.resolve(avail);
    let root_font = fonts[0];
    let root_lh = container.inherited.line_height.resolve(root_font.size);
    let rtl = container.inherited.direction == Direction::Rtl;
    let align = container.inherited.text_align;

    let mut frags: Vec<Fragment> = Vec::new();
    let mut y = 0.0f32;
    // Inline boxes open across line ends: (style, node)
    let mut carried: Vec<(u32, NodeId)> = Vec::new();
    let full_avail = avail;
    let mut start = 0;
    let mut li = 0;
    loop {
        if li > 0 && start >= pieces.len() {
            break;
        }
        let first_indent = if li == 0 { indent } else { 0.0 };
        // Floats beside this line narrow it; when even the first word
        // does not fit, the line moves below them
        let (mut line_x, mut avail) = (0.0, full_avail);
        if !ctx.floats.is_empty() {
            let top = base.1 + y;
            let bottom = top + root_lh.max(1.0);
            let (l, r) = ctx.floats.available(top, bottom, base.0, base.0 + full_avail);
            if r - l < full_avail {
                if first_run_width(&pieces, start) + first_indent > r - l {
                    if let Some(next) = ctx.floats.next_bottom(top, bottom).filter(|&n| n > top) {
                        y = next - base.1;
                        continue;
                    }
                }
                line_x = l - base.0;
                avail = r - l;
            }
        }
        let a = start;
        let b = next_line(&pieces, start, avail, first_indent).max((start + 1).min(pieces.len()));
        start = b;
        li += 1;
        let line = &pieces[a..b];
        let start_x = line_x + first_indent;
        let ends_forced = matches!(line.last().map(|p| &p.kind), Some(PieceKind::Break));
        let last_line = b >= pieces.len();

        // Pass 1: the line's width and justification opportunities
        let (mut x, mut content, mut pending, mut pending_n, mut opportunities) = (start_x, false, 0.0f32, 0usize, 0usize);
        let mut has_content = false;
        let mut end = start_x;
        for p in line {
            match &p.kind {
                PieceKind::Space { collapsible } => {
                    if content || !collapsible {
                        pending += p.width;
                        pending_n += 1;
                    }
                }
                PieceKind::Word { .. } | PieceKind::Atomic(_) => {
                    x += pending;
                    opportunities += pending_n;
                    pending = 0.0;
                    pending_n = 0;
                    x += p.width;
                    end = x;
                    content = true;
                    has_content = true;
                }
                PieceKind::Open { .. } => {
                    if content {
                        x += pending;
                        opportunities += pending_n;
                    }
                    pending = 0.0;
                    pending_n = 0;
                    x += p.width;
                    end = x;
                    has_content |= p.width > 0.0;
                }
                PieceKind::Close => {
                    x += p.width;
                    end = x;
                    has_content |= p.width > 0.0;
                }
                PieceKind::Break => has_content = true,
            }
        }
        // Inline boxes with vertical borders or padding show even when empty
        let has_content = has_content
            || line.iter().any(|p| matches!(p.kind, PieceKind::Open { style, .. } if { let s = &ic.styles[style as usize]; s.border.has_border() || s.box_.padding.iter().any(|p| !p.is_zero()) }));
        let free = line_x + avail - end;
        let justify = align == TextAlign::Justify && !last_line && !ends_forced && opportunities > 0 && free > 0.0;
        let offset = if free <= 0.0 {
            0.0
        } else {
            match (align, rtl) {
                (TextAlign::Center, _) => free / 2.0,
                (TextAlign::Right, _) | (TextAlign::End, false) | (TextAlign::Start, true) | (TextAlign::MatchParent, true) => free,
                (TextAlign::Justify, true) if !justify => free,
                _ => 0.0,
            }
        };
        let extra = if justify { free / opportunities as f32 } else { 0.0 };

        // Pass 2: fragments, relative to the line's root baseline
        let mut stack: Vec<OpenBox> = vec![OpenBox {
            style: 0,
            node: NodeId::NONE,
            shift: 0.0,
            x: 0.0,
            has_start: false,
            children: Vec::new(),
            last_text: None,
            decoration: ic.decoration,
        }];
        let (root_top, root_bottom) = half_leading_extents(&root_font, root_lh, 0.0);
        let (mut top, mut bottom) = (root_top, root_bottom);
        // Atomic inlines placed on this line: (index, x, baseline shift)
        let mut placed_atomics: Vec<(usize, f32, f32)> = Vec::new();
        let mut x = start_x + offset;
        for &(style, node) in &carried {
            open_box(&mut stack, ic, &fonts, style, node, x, false, &mut top, &mut bottom);
        }
        let mut content = false;
        let mut pending = 0.0f32;
        let mut pending_n = 0usize;
        for p in line {
            match &p.kind {
                PieceKind::Space { collapsible } => {
                    if content || !collapsible {
                        pending += p.width;
                        pending_n += 1;
                    }
                }
                PieceKind::Word { style, node, word } => {
                    x += pending + extra * pending_n as f32;
                    pending = 0.0;
                    pending_n = 0;
                    let top_box = stack.last_mut().expect("root box");
                    let font = fonts[*style as usize];
                    let st = &ic.styles[*style as usize];
                    let extend = top_box.last_text == Some((*style, *node));
                    if let (true, Some(Fragment::Text(t))) = (extend, top_box.children.last_mut()) {
                        t.rect.w = x + word.width - t.rect.x;
                        t.words.push((word.clone(), x));
                    } else {
                        let shift = top_box.shift;
                        top_box.children.push(Fragment::Text(TextFragment {
                            node: *node,
                            rect: Rect::new(x, shift - font.ascent(), word.width, font.ascent() + font.descent()),
                            baseline: shift,
                            font,
                            color: st.color(),
                            words: vec![(word.clone(), x)],
                            decoration: top_box.decoration.0,
                            decoration_color: top_box.decoration.1,
                            visible: st.inherited.visibility == Visibility::Visible,
                        }));
                        top_box.last_text = Some((*style, *node));
                    }
                    x += word.width;
                    content = true;
                }
                PieceKind::Atomic(i) => {
                    x += pending + extra * pending_n as f32;
                    pending = 0.0;
                    pending_n = 0;
                    if let Some(laid) = &atomics[*i] {
                        let parent = stack.last().expect("root box");
                        let pfont = fonts[parent.style as usize];
                        let st = atomic_styles[*i];
                        let h = laid.mt.size() + laid.frag.border_box.h + laid.mb.size();
                        let bl = atomic_baseline(laid);
                        let shift = baseline_shift(st.box_.vertical_align, &pfont, parent.shift, bl, h, st.inherited.line_height.resolve(st.font_size()));
                        top = top.min(shift - bl);
                        bottom = bottom.max(shift - bl + h);
                        placed_atomics.push((*i, x, shift));
                        stack.last_mut().expect("root box").last_text = None;
                    }
                    x += p.width;
                    content = true;
                }
                PieceKind::Open { style, node } => {
                    if content {
                        x += pending + extra * pending_n as f32;
                    }
                    pending = 0.0;
                    pending_n = 0;
                    open_box(&mut stack, ic, &fonts, *style, *node, x, true, &mut top, &mut bottom);
                    x += p.width;
                }
                PieceKind::Close => {
                    x += p.width;
                    close_box(&mut stack, ic, &fonts, x, true, avail);
                }
                PieceKind::Break => {}
            }
        }
        // Boxes still open continue on the next line
        carried = stack[1..].iter().map(|o| (o.style, o.node)).collect();
        while stack.len() > 1 {
            close_box(&mut stack, ic, &fonts, x, false, avail);
        }
        let root = stack.pop().expect("root box");
        if !has_content {
            continue;
        }
        let baseline = y - top;
        let mut line_frags = root.children;
        for f in &mut line_frags {
            f.translate(0.0, baseline);
        }
        for (i, ax, shift) in placed_atomics {
            if let Some(mut laid) = atomics[i].take() {
                let bl = atomic_baseline(&laid);
                let st = atomic_styles[i];
                let rel = relative_offset(st, avail, cb_h);
                // The fragment's border box is at (margin-left, 0)
                laid.frag.translate(ax + rel.0, baseline + shift - bl + laid.mt.size() + rel.1);
                line_frags.push(Fragment::Box(laid.frag));
            }
        }
        frags.extend(line_frags);
        y += bottom - top;
    }
    InlineLayout { frags, height: y }
}

#[allow(clippy::too_many_arguments)]
fn open_box(stack: &mut Vec<OpenBox>, ic: &InlineContent, fonts: &[ResolvedFont], style: u32, node: NodeId, x: f32, has_start: bool, top: &mut f32, bottom: &mut f32) {
    let parent = stack.last().expect("root box");
    let st = &ic.styles[style as usize];
    let font = fonts[style as usize];
    let pfont = fonts[parent.style as usize];
    let lh = st.inherited.line_height.resolve(font.size);
    let shift = baseline_shift(st.box_.vertical_align, &pfont, parent.shift, font.ascent(), font.ascent() + font.descent(), lh);
    let (t, b) = half_leading_extents(&font, lh, shift);
    *top = top.min(t);
    *bottom = bottom.max(b);
    let own = st.box_.text_decoration_line;
    let decoration = if own != 0 { (parent.decoration.0 | own, st.box_.text_decoration_color.unwrap_or(st.color())) } else { parent.decoration };
    stack.push(OpenBox { style, node, shift, x, has_start, children: Vec::new(), last_text: None, decoration });
}

/// Close the innermost inline box at `x` (the end of its margin box) into
/// a fragment in its parent
fn close_box(stack: &mut Vec<OpenBox>, ic: &InlineContent, fonts: &[ResolvedFont], x: f32, has_end: bool, cb_w: f32) {
    let o = stack.pop().expect("open box");
    let st = &ic.styles[o.style as usize];
    let font = fonts[o.style as usize];
    let b = &st.box_;
    let m = |i: usize| b.margin[i].resolve(cb_w).unwrap_or(0.0);
    let p = |i: usize| b.padding[i].resolve(cb_w).max(0.0);
    let bw = st.border.width;
    let border = [bw[0], if has_end { bw[1] } else { 0.0 }, bw[2], if o.has_start { bw[3] } else { 0.0 }];
    let padding = [p(0), if has_end { p(1) } else { 0.0 }, p(2), if o.has_start { p(3) } else { 0.0 }];
    let left = o.x + if o.has_start { m(3) } else { 0.0 };
    let right = x - if has_end { m(1) } else { 0.0 };
    let top = o.shift - font.ascent() - padding[0] - border[0];
    let bottom = o.shift + font.descent() + padding[2] + border[2];
    let mut frag = BoxFragment {
        node: o.node,
        kind: BoxFragmentKind::InlinePart,
        style: st.clone(),
        border_box: Rect::new(left, top, (right - left).max(0.0), bottom - top),
        border,
        padding,
        children: o.children,
        ink: Rect::default(),
        marker: None,
        replaced: None,
    };
    frag.update_ink();
    let parent = stack.last_mut().expect("root box");
    parent.children.push(Fragment::Box(frag));
    parent.last_text = None;
}

/// The min-content and max-content widths of inline content
pub fn intrinsic_inline(ctx: &mut LayoutCtx, ic: &InlineContent) -> (f32, f32) {
    let fonts: Vec<ResolvedFont> = ic.styles.iter().map(|s| ctx.fonts.resolve(s)).collect();
    let mut mins = Vec::new();
    let mut maxs = Vec::new();
    for item in &ic.items {
        if let InlineItem::Atomic(b) = item {
            let (min, max) = intrinsic_outer(ctx, b);
            mins.push(min);
            maxs.push(max);
        }
    }
    let indent = ic.styles[0].inherited.text_indent.resolve(0.0);
    let measure = |pieces: &[Piece], whole_lines: bool| -> f32 {
        let (mut best, mut x, mut pending, mut content) = (0.0f32, indent, 0.0f32, false);
        for p in pieces {
            if !whole_lines && p.break_before {
                x = 0.0;
                pending = 0.0;
                content = false;
            }
            match p.kind {
                PieceKind::Break => {
                    x = 0.0;
                    pending = 0.0;
                    content = false;
                }
                PieceKind::Space { collapsible } => {
                    if whole_lines && (content || !collapsible) {
                        pending += p.width;
                    }
                }
                _ => {
                    x += pending + p.width;
                    pending = 0.0;
                    content |= matches!(p.kind, PieceKind::Word { .. } | PieceKind::Atomic(_));
                }
            }
            best = best.max(x);
        }
        best
    };
    let min_pieces = build_pieces(ctx, ic, &fonts, 0.0, &mins);
    let min = measure(&min_pieces, false);
    let max_pieces = build_pieces(ctx, ic, &fonts, 0.0, &maxs);
    let max = measure(&max_pieces, true);
    (min, max.max(min))
}
