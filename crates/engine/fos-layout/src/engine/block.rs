//! Block formatting (CSS 2.1 §9.4.1, §10.3, §10.6): used widths and
//! heights, margin collapsing, replaced elements, and intrinsic sizes for
//! shrink-to-fit boxes

use std::sync::Arc;

use fos_css::properties::Color;
use fos_css::style::{BoxSizing, Float, LpAuto, Overflow, Position, Style};
use fos_dom::NodeId;

use super::box_tree::{BoxKind, LayoutBox, Replaced, ReplacedWhat};
use super::fonts::FontContext;
use super::fragment::{BoxFragment, BoxFragmentKind, Fragment, Rect, ReplacedPaint, TextFragment};
use super::inline;

pub struct LayoutCtx<'a> {
    pub fonts: &'a mut FontContext,
    pub viewport: (f32, f32),
}

/// Adjoining margins: the largest positive and the most negative
#[derive(Clone, Copy, Debug, Default)]
pub struct Margin {
    pos: f32,
    neg: f32,
}

impl Margin {
    pub fn new(m: f32) -> Margin {
        if m >= 0.0 {
            Margin { pos: m, neg: 0.0 }
        } else {
            Margin { pos: 0.0, neg: m }
        }
    }

    pub fn adjoin(self, o: Margin) -> Margin {
        Margin { pos: self.pos.max(o.pos), neg: self.neg.min(o.neg) }
    }

    /// The collapsed margin's size
    pub fn size(self) -> f32 {
        self.pos + self.neg
    }
}

/// How an auto width is found
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sizing {
    /// Fill the containing block (in-flow blocks)
    Stretch,
    /// Shrink to fit the content (inline-blocks, floats, absolutes)
    Shrink,
}

/// Used margins (auto resolved later), borders and padding
#[derive(Clone, Copy)]
struct Edges {
    margin: [Option<f32>; 4],
    border: [f32; 4],
    padding: [f32; 4],
}

fn edges(style: &Style, cb_w: f32) -> Edges {
    let b = &style.box_;
    Edges {
        margin: b.margin.map(|m| m.resolve(cb_w)),
        border: style.border.width,
        padding: b.padding.map(|p| p.resolve(cb_w).max(0.0)),
    }
}

impl Edges {
    fn horizontal_bp(&self) -> f32 {
        self.border[1] + self.border[3] + self.padding[1] + self.padding[3]
    }

    fn vertical_bp(&self) -> f32 {
        self.border[0] + self.border[2] + self.padding[0] + self.padding[2]
    }
}

/// A content-box size from a specified one (box-sizing)
fn content_size(style: &Style, specified: f32, bp: f32) -> f32 {
    match style.box_.box_sizing {
        BoxSizing::ContentBox => specified,
        BoxSizing::BorderBox => (specified - bp).max(0.0),
    }
}

/// Clamp a content width by min/max-width
fn clamp_width(style: &Style, w: f32, cb_w: f32, bp: f32) -> f32 {
    let b = &style.box_;
    let max = content_size(style, b.max_width.resolve(cb_w), bp);
    let min = b.min_width.resolve(cb_w).map_or(0.0, |m| content_size(style, m, bp));
    w.min(max).max(min)
}

/// Clamp a content height by min/max-height (percentages need a definite
/// containing block height)
fn clamp_height(style: &Style, h: f32, cb_h: Option<f32>, bp: f32) -> f32 {
    let b = &style.box_;
    let max = match b.max_height {
        fos_css::style::MaxSize::None => f32::INFINITY,
        fos_css::style::MaxSize::Lp(l) if l.has_percent() => cb_h.map_or(f32::INFINITY, |c| l.resolve(c)),
        fos_css::style::MaxSize::Lp(l) => l.px,
    };
    let max = if max.is_finite() { content_size(style, max, bp) } else { max };
    let min = b.min_height.resolve_definite(cb_h).map_or(0.0, |m| content_size(style, m, bp));
    h.min(max).max(min)
}

/// A laid-out block-level box
pub struct Laid {
    /// Border box at (margin-left, 0)
    pub frag: BoxFragment,
    /// The box's top margin, adjoined with any it collapses with from
    /// inside; and the bottom
    pub mt: Margin,
    pub mb: Margin,
    /// Its top and bottom margins collapse through it (empty boxes); then
    /// `mt` holds them all
    pub through: bool,
    pub ml: f32,
    pub mr: f32,
}

impl Laid {
    pub fn margin_box_width(&self) -> f32 {
        self.ml + self.frag.border_box.w + self.mr
    }
}

/// Lay out the root element's box in the initial containing block
pub fn layout_root(ctx: &mut LayoutCtx, root: &LayoutBox) -> (BoxFragment, f32) {
    let (vw, vh) = ctx.viewport;
    let mut laid = layout_block_level(ctx, root, vw, Some(vh), Sizing::Stretch, true);
    // The root's margins never collapse with anything
    let top = laid.mt.size();
    laid.frag.translate(0.0, top);
    let bottom = laid.frag.border_box.bottom() + laid.mb.size();
    (laid.frag, bottom)
}

/// Lay out a block-level box (or an atomic inline) for a containing block
/// `cb_w` wide (and `cb_h` tall, when definite)
pub fn layout_block_level(ctx: &mut LayoutCtx, b: &LayoutBox, cb_w: f32, cb_h: Option<f32>, sizing: Sizing, is_root: bool) -> Laid {
    let style = &b.style;
    let e = edges(style, cb_w);
    let hbp = e.horizontal_bp();
    let vbp = e.vertical_bp();

    if let BoxKind::Replaced(r) = &b.kind {
        return layout_replaced(ctx, b.node, style, r, e, cb_w, cb_h, sizing);
    }

    // Width (§10.3.3), or shrink-to-fit (§10.3.5)
    let specified = style.box_.width.resolve(cb_w).map(|w| content_size(style, w, hbp));
    let (ml_auto, mr_auto) = (e.margin[3].is_none(), e.margin[1].is_none());
    let (mut ml, mut mr) = (e.margin[3].unwrap_or(0.0), e.margin[1].unwrap_or(0.0));
    let width = match specified {
        Some(w) => clamp_width(style, w, cb_w, hbp),
        None => {
            let avail = (cb_w - ml - mr - hbp).max(0.0);
            let w = match sizing {
                Sizing::Stretch => avail,
                Sizing::Shrink => {
                    let (min, max) = intrinsic_content(ctx, b);
                    max.min(avail).max(min)
                }
            };
            clamp_width(style, w, cb_w, hbp)
        }
    };
    if sizing == Sizing::Stretch {
        // Auto margins take the free space; overconstrained, the right
        // margin gives (ltr)
        let free = cb_w - width - hbp - ml - mr;
        match (ml_auto, mr_auto) {
            (true, true) => {
                ml = free.max(0.0) / 2.0;
                mr = free - ml;
            }
            (true, false) => ml = free,
            (false, true) => mr = free,
            (false, false) => mr += free,
        }
    }

    // Height: specified (percentages of a definite containing block) or
    // the content's
    let specified_h = style.box_.height.resolve_definite(cb_h).map(|h| content_size(style, h, vbp));
    let inner_cb_h = specified_h.map(|h| clamp_height(style, h, cb_h, vbp)).or(if is_root { cb_h.map(|h| (h - vbp).max(0.0)) } else { None });

    let bfc = is_root || b.is_bfc_root();
    let collapse_top = !bfc && e.border[0] == 0.0 && e.padding[0] == 0.0;
    let collapse_bottom = !bfc && e.border[2] == 0.0 && e.padding[2] == 0.0 && style.box_.height.is_auto() && style.box_.min_height.resolve(0.0).unwrap_or(0.0) <= 0.0;

    let mut frag = BoxFragment {
        node: b.node,
        kind: BoxFragmentKind::Block,
        style: style.clone(),
        border_box: Rect::default(),
        border: e.border,
        padding: e.padding,
        children: Vec::new(),
        ink: Rect::default(),
        marker: None,
        replaced: None,
    };
    let (content_x, content_y) = (ml + e.border[3] + e.padding[3], e.border[0] + e.padding[0]);
    let mut escaped_top = Margin::default();
    let mut escaped_bottom = Margin::default();
    let mut through = false;
    let content_h = match &b.kind {
        BoxKind::Block(children) => {
            let flow = layout_flow(ctx, children, width, inner_cb_h, collapse_top, collapse_bottom);
            escaped_top = flow.top;
            escaped_bottom = flow.bottom;
            through = flow.through;
            frag.children = flow.frags;
            flow.height
        }
        BoxKind::Inline(content) => {
            let lines = inline::layout_inline(ctx, content, width, inner_cb_h);
            frag.children = lines.frags;
            lines.height
        }
        BoxKind::Replaced(_) => unreachable!(),
    };
    let height = clamp_height(style, specified_h.unwrap_or(content_h), cb_h, vbp);
    let through = through && collapse_top && collapse_bottom && height <= 0.0;
    for c in &mut frag.children {
        c.translate(content_x, content_y);
    }
    frag.border_box = Rect::new(ml, 0.0, width + hbp, height + vbp);

    // An outside list marker sits left of the first line
    if let Some(m) = &b.marker {
        let font = ctx.fonts.resolve(&m.style);
        let text = m.text.trim_end();
        let word = ctx.fonts.shape(&font, text);
        let gap = ctx.fonts.shape(&font, " ").width;
        let baseline = frag.first_baseline().unwrap_or(content_y + font.ascent());
        let x = content_x - word.width - gap;
        frag.marker = Some(TextFragment {
            node: NodeId::NONE,
            rect: Rect::new(x, baseline - font.ascent(), word.width, font.ascent() + font.descent()),
            baseline,
            font,
            color: m.style.color(),
            words: vec![(word, x)],
            decoration: 0,
            decoration_color: Color::BLACK,
            visible: m.style.inherited.visibility == fos_css::style::Visibility::Visible,
        });
    }
    frag.update_ink();

    let own_top = Margin::new(e.margin[0].unwrap_or(0.0));
    let own_bottom = Margin::new(e.margin[2].unwrap_or(0.0));
    if through {
        Laid { frag, mt: own_top.adjoin(own_bottom).adjoin(escaped_top), mb: Margin::default(), through: true, ml, mr }
    } else {
        Laid {
            frag,
            mt: if collapse_top { own_top.adjoin(escaped_top) } else { own_top },
            mb: if collapse_bottom { own_bottom.adjoin(escaped_bottom) } else { own_bottom },
            through: false,
            ml,
            mr,
        }
    }
}

/// Block-level children laid out top to bottom
struct Flow {
    frags: Vec<Fragment>,
    height: f32,
    /// Margins escaping through the container's top and bottom
    top: Margin,
    bottom: Margin,
    through: bool,
}

fn layout_flow(ctx: &mut LayoutCtx, children: &[LayoutBox], width: f32, cb_h: Option<f32>, collapse_top: bool, collapse_bottom: bool) -> Flow {
    let mut frags = Vec::with_capacity(children.len());
    let mut y = 0.0f32;
    let mut pending = Margin::default();
    let mut top = Margin::default();
    let mut placed = false;
    for child in children {
        let out_of_flow = child.style.is_out_of_flow();
        let sizing = if out_of_flow || child.style.box_.float != Float::None { Sizing::Shrink } else { Sizing::Stretch };
        let mut laid = layout_block_level(ctx, child, width, cb_h, sizing, false);
        if out_of_flow {
            // At its static position, taking no room (positioning comes
            // with stage C)
            let at = y + pending.adjoin(laid.mt).size();
            laid.frag.translate(0.0, at);
            frags.push(Fragment::Box(laid.frag));
            continue;
        }
        let rel = relative_offset(&child.style, width, cb_h);
        if laid.through {
            let at = y + pending.adjoin(laid.mt).size();
            pending = pending.adjoin(laid.mt);
            laid.frag.translate(rel.0, at + rel.1);
            frags.push(Fragment::Box(laid.frag));
            continue;
        }
        let m = pending.adjoin(laid.mt);
        let child_y = if !placed && collapse_top {
            top = m;
            y
        } else {
            y + m.size()
        };
        let h = laid.frag.border_box.h;
        laid.frag.translate(rel.0, child_y + rel.1);
        frags.push(Fragment::Box(laid.frag));
        y = child_y + h;
        pending = laid.mb;
        placed = true;
    }
    if !placed {
        return if collapse_top {
            Flow { frags, height: 0.0, top: pending, bottom: Margin::default(), through: true }
        } else if collapse_bottom {
            Flow { frags, height: 0.0, top, bottom: pending, through: false }
        } else {
            Flow { frags, height: pending.size().max(0.0), top, bottom: Margin::default(), through: false }
        };
    }
    if collapse_bottom {
        Flow { frags, height: y, top, bottom: pending, through: false }
    } else {
        Flow { frags, height: (y + pending.size()).max(y.min(0.0)), top, bottom: Margin::default(), through: false }
    }
}

/// `position: relative` offsets
pub fn relative_offset(style: &Style, cb_w: f32, cb_h: Option<f32>) -> (f32, f32) {
    if style.box_.position != Position::Relative && style.box_.position != Position::Sticky {
        return (0.0, 0.0);
    }
    if style.box_.position == Position::Sticky {
        return (0.0, 0.0);
    }
    let i = &style.box_.inset;
    let dx = match (i[3].resolve(cb_w), i[1].resolve(cb_w)) {
        (Some(l), _) => l,
        (None, Some(r)) => -r,
        _ => 0.0,
    };
    let dy = match (i[0].resolve_definite(cb_h), i[2].resolve_definite(cb_h)) {
        (Some(t), _) => t,
        (None, Some(b)) => -b,
        _ => 0.0,
    };
    (dx, dy)
}

/// A replaced element's default size when it has no natural one
fn default_replaced_size(ctx: &mut LayoutCtx, style: &Style, r: &Replaced) -> (f32, f32) {
    let font = ctx.fonts.resolve(style);
    let line = style.inherited.line_height.resolve(font.size).max(font.ascent() + font.descent());
    match &r.what {
        ReplacedWhat::TextField(..) => (font.size * 11.25, line),
        ReplacedWhat::Select(text) => (ctx.fonts.shape(&font, text).width + font.size * 1.5, line),
        _ => (300.0, 150.0),
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_replaced(ctx: &mut LayoutCtx, node: NodeId, style: &Style, r: &Replaced, e: Edges, cb_w: f32, cb_h: Option<f32>, sizing: Sizing) -> Laid {
    let (hbp, vbp) = (e.horizontal_bp(), e.vertical_bp());
    let (w, h) = replaced_content_size(ctx, style, r, cb_w, cb_h, hbp, vbp);
    let (mut ml, mut mr) = (e.margin[3].unwrap_or(0.0), e.margin[1].unwrap_or(0.0));
    if sizing == Sizing::Stretch && style.box_.float == Float::None {
        let free = cb_w - w - hbp - ml - mr;
        match (e.margin[3].is_none(), e.margin[1].is_none()) {
            (true, true) if free > 0.0 => {
                ml = free / 2.0;
                mr = free / 2.0;
            }
            (true, false) => ml = free,
            (false, true) => mr = free,
            _ => {}
        }
    }
    let mut frag = BoxFragment {
        node,
        kind: BoxFragmentKind::Replaced,
        style: style.clone(),
        border_box: Rect::new(ml, 0.0, w + hbp, h + vbp),
        border: e.border,
        padding: e.padding,
        children: Vec::new(),
        ink: Rect::default(),
        marker: None,
        replaced: None,
    };
    let content = frag.content_box();
    let paint = match &r.what {
        ReplacedWhat::Image | ReplacedWhat::Canvas | ReplacedWhat::Video | ReplacedWhat::Svg => ReplacedPaint::Bitmap,
        ReplacedWhat::Frame => ReplacedPaint::Empty,
        ReplacedWhat::Check { radio, checked } => ReplacedPaint::Check { radio: *radio, checked: *checked },
        ReplacedWhat::TextField(text, placeholder) => {
            if !text.is_empty() {
                let color = if *placeholder { Color::rgba(117, 117, 117, 255) } else { style.color() };
                frag.children.push(Fragment::Text(control_text(ctx, style, text, content, color)));
            }
            ReplacedPaint::Empty
        }
        ReplacedWhat::Select(text) => {
            if !text.is_empty() {
                frag.children.push(Fragment::Text(control_text(ctx, style, text, content, style.color())));
            }
            ReplacedPaint::Select
        }
    };
    frag.replaced = Some(Arc::new(paint));
    frag.update_ink();
    let mt = Margin::new(e.margin[0].unwrap_or(0.0));
    let mb = Margin::new(e.margin[2].unwrap_or(0.0));
    Laid { frag, mt, mb, through: false, ml, mr }
}

/// One line of a form control's text, vertically centered and clipped to
/// the content box
fn control_text(ctx: &mut LayoutCtx, style: &Style, text: &str, content: Rect, color: Color) -> TextFragment {
    let font = ctx.fonts.resolve(style);
    let line: String = text.lines().next().unwrap_or("").to_string();
    let word = ctx.fonts.shape(&font, &line);
    let glyph_h = font.ascent() + font.descent();
    let baseline = content.y + (content.h - glyph_h) / 2.0 + font.ascent();
    TextFragment {
        node: NodeId::NONE,
        rect: Rect::new(content.x, baseline - font.ascent(), word.width.min(content.w.max(0.0)), glyph_h),
        baseline,
        font,
        color,
        words: vec![(word, content.x)],
        decoration: 0,
        decoration_color: color,
        visible: style.inherited.visibility == fos_css::style::Visibility::Visible,
    }
}

/// A replaced element's used content size (§10.3.2, §10.6.2)
fn replaced_content_size(ctx: &mut LayoutCtx, style: &Style, r: &Replaced, cb_w: f32, cb_h: Option<f32>, hbp: f32, vbp: f32) -> (f32, f32) {
    let sw = style.box_.width.resolve(cb_w).map(|w| content_size(style, w, hbp));
    let sh = style.box_.height.resolve_definite(cb_h).map(|h| content_size(style, h, vbp));
    let natural = r.natural.unwrap_or_else(|| default_replaced_size(ctx, style, r));
    let ratio = r.natural.filter(|n| n.0 > 0.0 && n.1 > 0.0).map(|n| n.0 / n.1);
    let (w, h) = match (sw, sh) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, ratio.map_or(natural.1, |r| w / r)),
        (None, Some(h)) => (ratio.map_or(natural.0, |r| h * r), h),
        (None, None) => natural,
    };
    let cw = clamp_width(style, w, cb_w, hbp);
    let ch = clamp_height(style, h, cb_h, vbp);
    // Keep the ratio when only one dimension was clamped
    match (sw, sh, ratio) {
        (None, None, Some(r)) if cw != w && ch == h => (cw, clamp_height(style, cw / r, cb_h, vbp)),
        (None, None, Some(r)) if ch != h && cw == w => (clamp_width(style, ch * r, cb_w, hbp), ch),
        _ => (cw, ch),
    }
}

/// The min-content and max-content widths of a box's content box
pub fn intrinsic_content(ctx: &mut LayoutCtx, b: &LayoutBox) -> (f32, f32) {
    match &b.kind {
        BoxKind::Inline(content) => inline::intrinsic_inline(ctx, content),
        BoxKind::Block(children) => {
            let (mut min, mut max) = (0.0f32, 0.0f32);
            for c in children {
                if c.style.is_out_of_flow() {
                    continue;
                }
                let (cmin, cmax) = intrinsic_outer(ctx, c);
                min = min.max(cmin);
                max = max.max(cmax);
            }
            (min, max)
        }
        BoxKind::Replaced(r) => {
            let e = edges(&b.style, 0.0);
            let (w, _) = replaced_content_size(ctx, &b.style, r, 0.0, None, e.horizontal_bp(), e.vertical_bp());
            (w, w)
        }
    }
}

/// A box's min-content and max-content contributions: its margin box
/// (percentages count as zero)
pub fn intrinsic_outer(ctx: &mut LayoutCtx, b: &LayoutBox) -> (f32, f32) {
    let style = &b.style;
    let e = edges(style, 0.0);
    let hbp = e.horizontal_bp();
    let margins = e.margin[1].unwrap_or(0.0) + e.margin[3].unwrap_or(0.0);
    let fixed = match style.box_.width {
        LpAuto::Lp(l) if !l.has_percent() => Some(content_size(style, l.px, hbp)),
        _ => None,
    };
    let (min, max) = match (fixed, &b.kind) {
        (Some(w), _) => (w, w),
        (None, _) => intrinsic_content(ctx, b),
    };
    let clamp = |w: f32| {
        let b = &style.box_;
        let maxw = match b.max_width {
            fos_css::style::MaxSize::Lp(l) if !l.has_percent() => content_size(style, l.px, hbp),
            _ => f32::INFINITY,
        };
        let minw = match b.min_width {
            LpAuto::Lp(l) if !l.has_percent() => content_size(style, l.px, hbp),
            _ => 0.0,
        };
        w.min(maxw).max(minw)
    };
    (clamp(min) + hbp + margins, clamp(max) + hbp + margins)
}

/// Whether a box clips its overflow (an inline-block's baseline is then its
/// bottom margin edge)
pub fn clips(style: &Style) -> bool {
    style.box_.overflow_x != Overflow::Visible || style.box_.overflow_y != Overflow::Visible
}
