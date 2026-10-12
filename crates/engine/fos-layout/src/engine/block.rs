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
    /// Floats of the block formatting context being laid out, in its
    /// coordinates
    pub floats: FloatList,
    /// Where the box about to be laid out starts in the formatting
    /// context: its containing block's content left, its border-box top
    pub origin: (f32, f32),
    /// Min- and max-content widths by box (addresses in the box tree,
    /// which lives unchanged through one layout)
    intrinsic: std::collections::HashMap<usize, (f32, f32)>,
    /// The box about to be laid out is in a block whose text-align is
    /// -webkit-center (<center>, align=center): it is centered
    pub center_in_parent: bool,
    /// The box about to be laid out is aligned in its block container by
    /// `justify-self` (the container's `justify-items` for `auto`), not
    /// stretched: where in the free space it goes (0 start, 1 end)
    pub justify_in_parent: Option<f32>,
    /// The document is in quirks mode
    pub quirks: bool,
}

impl<'a> LayoutCtx<'a> {
    pub fn new(fonts: &'a mut FontContext, viewport: (f32, f32)) -> Self {
        LayoutCtx { fonts, viewport, floats: FloatList::default(), origin: (0.0, 0.0), intrinsic: Default::default(), center_in_parent: false, justify_in_parent: None, quirks: false }
    }
}

/// The floats placed so far in a block formatting context (margin boxes)
#[derive(Debug, Default)]
pub struct FloatList {
    items: Vec<(Rect, bool)>,
    /// A float's top may not be above an earlier one's
    last_top: f32,
}

impl FloatList {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The free span of `[left, right)` beside floats between `top` and
    /// `bottom`
    pub fn available(&self, top: f32, bottom: f32, left: f32, right: f32) -> (f32, f32) {
        let (mut l, mut r) = (left, right);
        for (f, is_left) in &self.items {
            if f.bottom() > top && f.y < bottom && f.h > 0.0 {
                if *is_left {
                    l = l.max(f.right());
                } else {
                    r = r.min(f.x);
                }
            }
        }
        (l, r.max(l))
    }

    /// The lowest bottom of floats overlapping `[top, bottom)` (to move
    /// below them), if any
    pub fn next_bottom(&self, top: f32, bottom: f32) -> Option<f32> {
        self.items.iter().filter(|(f, _)| f.bottom() > top && f.y < bottom && f.h > 0.0).map(|(f, _)| f.bottom()).min_by(f32::total_cmp)
    }

    /// Place a `w` × `h` margin box as far up (from `y_min`) and to its
    /// side as it fits within `[left, right)`
    pub fn place(&mut self, w: f32, h: f32, is_left: bool, y_min: f32, left: f32, right: f32) -> (f32, f32) {
        let mut y = y_min.max(self.last_top);
        let hh = h.max(0.01);
        let x = loop {
            let (l, r) = self.available(y, y + hh, left, right);
            if r - l >= w - 0.01 || (l <= left && r >= right) {
                break if is_left { l } else { r - w };
            }
            match self.next_bottom(y, y + hh) {
                Some(next) if next > y => y = next,
                _ => break if is_left { l } else { r - w },
            }
        };
        self.items.push((Rect::new(x, y, w, h), is_left));
        self.last_top = y;
        (x, y)
    }

    /// Where content clearing `clear` floats may start
    pub fn clear_y(&self, clear: fos_css::style::Clear) -> Option<f32> {
        use fos_css::style::Clear;
        self.items
            .iter()
            .filter(|(_, l)| match clear {
                Clear::None => false,
                Clear::Left => *l,
                Clear::Right => !*l,
                Clear::Both => true,
            })
            .map(|(f, _)| f.bottom())
            .max_by(f32::total_cmp)
    }

    /// The bottom of the lowest float
    pub fn bottom(&self) -> f32 {
        self.items.iter().map(|(f, _)| f.bottom()).fold(0.0, f32::max)
    }
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
pub(crate) struct Edges {
    pub margin: [Option<f32>; 4],
    pub border: [f32; 4],
    pub padding: [f32; 4],
}

pub(crate) fn edges(style: &Style, cb_w: f32) -> Edges {
    let b = &style.box_;
    Edges {
        margin: b.margin.map(|m| m.resolve(cb_w)),
        border: style.border.width,
        padding: b.padding.map(|p| p.resolve(cb_w).max(0.0)),
    }
}

impl Edges {
    pub fn horizontal_bp(&self) -> f32 {
        self.border[1] + self.border[3] + self.padding[1] + self.padding[3]
    }

    pub fn vertical_bp(&self) -> f32 {
        self.border[0] + self.border[2] + self.padding[0] + self.padding[2]
    }
}

/// A content-box size from a specified one (box-sizing)
pub(crate) fn content_size(style: &Style, specified: f32, bp: f32) -> f32 {
    match style.box_.box_sizing {
        BoxSizing::ContentBox => specified,
        BoxSizing::BorderBox => (specified - bp).max(0.0),
    }
}

/// Clamp a content width by min/max-width
pub(crate) fn clamp_width(style: &Style, w: f32, cb_w: f32, bp: f32) -> f32 {
    let b = &style.box_;
    let max = content_size(style, b.max_width.resolve(cb_w), bp);
    let min = b.min_width.resolve(cb_w).map_or(0.0, |m| content_size(style, m, bp));
    w.min(max).max(min)
}

/// Clamp a content height by min/max-height (percentages need a definite
/// containing block height)
pub(crate) fn clamp_height(style: &Style, h: f32, cb_h: Option<f32>, bp: f32) -> f32 {
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
    /// The height of the box's content as laid out, whatever height the
    /// box was given (table cells align their content by it)
    pub content_h: f32,
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
    layout_sized(ctx, b, cb_w, cb_h, sizing, is_root, Forced::default())
}

/// Border-box sizes a parent formatting context decided (flex items)
#[derive(Clone, Copy, Debug, Default)]
pub struct Forced {
    pub width: Option<f32>,
    pub height: Option<f32>,
    /// The box is a flex or grid item or a table cell: it establishes a
    /// formatting context whatever its own style says
    pub root: bool,
}

/// [`layout_block_level`] with sizes imposed by the parent (auto margins
/// are then left to the parent too)
#[allow(clippy::too_many_arguments)]
pub fn layout_sized(ctx: &mut LayoutCtx, b: &LayoutBox, cb_w: f32, cb_h: Option<f32>, sizing: Sizing, is_root: bool, forced: Forced) -> Laid {
    let style = &b.style;
    let e = edges(style, cb_w);
    let hbp = e.horizontal_bp();
    let vbp = e.vertical_bp();
    let sizing = if forced.width.is_some() { Sizing::Shrink } else { sizing };
    let centered = std::mem::take(&mut ctx.center_in_parent);
    let justify = std::mem::take(&mut ctx.justify_in_parent).filter(|_| sizing == Sizing::Stretch && forced.width.is_none());

    if let BoxKind::Replaced(r) = &b.kind {
        let mut laid = layout_replaced(ctx, b.node, style, r, e, cb_w, cb_h, sizing);
        if forced.width.is_some() || forced.height.is_some() {
            let bb = &mut laid.frag.border_box;
            bb.w = forced.width.unwrap_or(bb.w).max(hbp);
            bb.h = forced.height.unwrap_or(bb.h).max(vbp);
            laid.frag.children.clear();
            laid.frag.update_ink();
        }
        return laid;
    }

    // Width (§10.3.3), or shrink-to-fit (§10.3.5)
    let (ml_auto, mr_auto) = (e.margin[3].is_none(), e.margin[1].is_none());
    let (mut ml, mut mr) = (e.margin[3].unwrap_or(0.0), e.margin[1].unwrap_or(0.0));
    let intrinsic = style.box_.intrinsic_width;
    let avail_w = (cb_w - ml - mr - hbp).max(0.0);
    let specified = match forced.width {
        Some(w) => Some((w - hbp).max(0.0)),
        None => style.box_.width.resolve(cb_w).map(|w| content_size(style, w, hbp)).or_else(|| intrinsic_size(ctx, b, intrinsic[0], avail_w)),
    };
    // Tables are as wide as their content needs (up to the space there is)
    let is_table = matches!(b.kind, BoxKind::Table(_));
    // aspect-ratio applies to the box-sizing box: a shrink-to-fit box with
    // a definite height gets its width from it
    let ratio = style.box_.aspect_ratio.filter(|_| !is_table);
    let border_box = style.box_.box_sizing == fos_css::style::BoxSizing::BorderBox;
    let ratio_width = ratio.filter(|_| specified.is_none() && sizing != Sizing::Stretch).and_then(|r| {
        let h = match forced.height {
            Some(h) => (h - vbp).max(0.0),
            None => style.box_.height.resolve_definite(cb_h).map(|h| content_size(style, h, vbp))?,
        };
        Some(if border_box { ((h + vbp) * r - hbp).max(0.0) } else { h * r })
    });
    let mut width = match specified {
        Some(w) if forced.width.is_some() => w,
        Some(w) => clamp_width(style, w, cb_w, hbp),
        None if ratio_width.is_some() => clamp_width(style, ratio_width.unwrap_or(0.0), cb_w, hbp),
        None => {
            let avail = (cb_w - ml - mr - hbp).max(0.0);
            let w = match sizing {
                // Aligned rather than stretched: as wide as its content
                Sizing::Stretch if !is_table && justify.is_none() => avail,
                _ => {
                    let (min, max) = intrinsic_content(ctx, b);
                    max.min(avail).max(min)
                }
            };
            clamp_width(style, w, cb_w, hbp)
        }
    };
    // min-width and max-width given as intrinsic sizes (min wins)
    if forced.width.is_none() {
        if let Some(max) = intrinsic_size(ctx, b, intrinsic[2], avail_w) {
            width = width.min(max);
        }
        if let Some(min) = intrinsic_size(ctx, b, intrinsic[1], avail_w) {
            width = width.max(min);
        }
    }
    if sizing == Sizing::Stretch {
        // Auto margins take the free space; overconstrained, the right
        // margin gives (the left one in right-to-left content)
        let free = cb_w - width - hbp - ml - mr;
        match (ml_auto, mr_auto) {
            (true, true) => {
                ml = free.max(0.0) / 2.0;
                mr = free - ml;
            }
            (true, false) => ml = free,
            (false, true) => mr = free,
            // Aligned by justify-self, or centered inside <center> (and
            // align=center)
            (false, false) if free > 0.0 && justify.is_some() => {
                let at = justify.unwrap_or(0.0);
                let at = if style.inherited.direction == fos_css::style::Direction::Rtl { 1.0 - at } else { at };
                ml += free * at;
                mr += free * (1.0 - at);
            }
            (false, false) if free > 0.0 && centered => {
                ml += free / 2.0;
                mr += free / 2.0;
            }
            // (its own direction stands in for the containing block's)
            (false, false) if style.inherited.direction == fos_css::style::Direction::Rtl => ml += free,
            (false, false) => mr += free,
        }
    }

    // Height: specified (percentages of a definite containing block) or
    // the content's
    let specified_h = match forced.height {
        Some(h) => Some((h - vbp).max(0.0)),
        None => style.box_.height.resolve_definite(cb_h).map(|h| content_size(style, h, vbp)).or_else(|| {
            // An automatic height follows the width through aspect-ratio
            let r = ratio.filter(|_| style.box_.height.is_auto())?;
            Some(if border_box { ((width + hbp) / r - vbp).max(0.0) } else { width / r })
        }),
    };
    let inner_cb_h = specified_h.map(|h| if forced.height.is_some() { h } else { clamp_height(style, h, cb_h, vbp) }).or(if is_root { cb_h.map(|h| (h - vbp).max(0.0)) } else { None });

    let bfc = is_root || forced.root || b.is_bfc_root();
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
        scroll_extent: None,
        line_baselines: None,
    };
    let (content_x, content_y) = (ml + e.border[3] + e.padding[3], e.border[0] + e.padding[0]);
    // A new formatting context has its own floats and coordinates
    let base = if bfc { (0.0, 0.0) } else { (ctx.origin.0 + content_x, ctx.origin.1 + content_y) };
    let outer_floats = bfc.then(|| std::mem::take(&mut ctx.floats));
    let mut escaped_top = Margin::default();
    let mut escaped_bottom = Margin::default();
    let mut through = false;
    let content_h = match &b.kind {
        BoxKind::Block(children) => {
            let center = style.inherited.text_align == fos_css::style::TextAlign::WebkitCenter;
            let justify_items = fos_css::style::AlignItems::from_u8(style.box_.grid.justify_items).unwrap_or(fos_css::style::AlignItems::Normal);
            let flow = layout_flow(ctx, children, width, inner_cb_h, collapse_top, collapse_bottom, base, (center, justify_items));
            escaped_top = flow.top;
            escaped_bottom = flow.bottom;
            through = flow.through;
            frag.children = flow.frags;
            flow.height
        }
        BoxKind::Inline(content) => {
            let lines = inline::layout_inline(ctx, content, width, inner_cb_h, base);
            frag.children = lines.frags;
            frag.line_baselines = lines.baselines;
            lines.height
        }
        BoxKind::Flex(items) => {
            let bounds = (clamp_height(style, 0.0, cb_h, vbp), clamp_height(style, f32::INFINITY, cb_h, vbp));
            let flex = super::flex::layout_flex(ctx, style, items, width, inner_cb_h, bounds);
            frag.children = flex.frags;
            flex.height
        }
        BoxKind::Grid(items) => {
            let grid = super::grid::layout_grid(ctx, style, items, width, inner_cb_h);
            frag.children = grid.frags;
            grid.height
        }
        BoxKind::Table(t) => {
            let table = super::table::layout_table(ctx, style, t, width, specified.is_none());
            frag.children = table.frags;
            width = width.max(table.width);
            table.height
        }
        BoxKind::Replaced(_) => unreachable!(),
    };
    // Formatting context roots grow to contain their floats
    let content_h = match outer_floats {
        Some(outer) => {
            let floats_bottom = ctx.floats.bottom();
            ctx.floats = outer;
            content_h.max(floats_bottom)
        }
        None => content_h,
    };
    let through = through && content_h <= 0.0;
    let height = match forced.height {
        Some(_) => specified_h.unwrap_or(content_h),
        None => clamp_height(style, specified_h.unwrap_or(content_h), cb_h, vbp),
    };
    let through = through && collapse_top && collapse_bottom && height <= 0.0;
    for c in &mut frag.children {
        c.translate(content_x, content_y);
    }
    if let Some((first, last)) = &mut frag.line_baselines {
        *first += content_y;
        *last += content_y;
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
        Laid { frag, mt: own_top.adjoin(own_bottom).adjoin(escaped_top), mb: Margin::default(), through: true, ml, mr, content_h }
    } else {
        Laid {
            frag,
            mt: if collapse_top { own_top.adjoin(escaped_top) } else { own_top },
            mb: if collapse_bottom { own_bottom.adjoin(escaped_bottom) } else { own_bottom },
            through: false,
            ml,
            mr,
            content_h,
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

/// The order to lay `children` out in: their own, except that floats
/// right after a run of text that fits on one line beside them come
/// first. A float sits at the line it is in, and the line flows beside it
/// (floats are block children here, after the text before them).
fn flow_order(ctx: &mut LayoutCtx, children: &[LayoutBox], width: f32) -> Vec<usize> {
    use fos_css::style::Clear;
    let text_run = |b: &LayoutBox| !b.node.is_valid() && matches!(&b.kind, BoxKind::Inline(c) if !c.items.iter().any(|i| matches!(i, super::box_tree::InlineItem::Break)));
    let float = |b: &LayoutBox| b.style.box_.float != Float::None && b.style.box_.clear == Clear::None && !b.style.is_out_of_flow();
    let mut order: Vec<usize> = (0..children.len()).collect();
    if !children.windows(2).any(|w| text_run(&w[0]) && float(&w[1])) {
        return order;
    }
    order.clear();
    let mut i = 0;
    while i < children.len() {
        if text_run(&children[i]) {
            let end = i + 1 + children[i + 1..].iter().take_while(|c| float(c)).count();
            if end > i + 1 {
                let text = intrinsic_content(ctx, &children[i]).1;
                let floats: f32 = children[i + 1..end].iter().map(|f| intrinsic_outer(ctx, f).1).sum();
                if text + floats <= width {
                    order.extend(i + 1..end);
                    order.push(i);
                    i = end;
                    continue;
                }
            }
        }
        order.push(i);
        i += 1;
    }
    order
}

#[allow(clippy::too_many_arguments)]
/// `align`: whether the container centers its blocks (`<center>`), and its
/// `justify-items`
fn layout_flow(ctx: &mut LayoutCtx, children: &[LayoutBox], width: f32, cb_h: Option<f32>, collapse_top: bool, collapse_bottom: bool, base: (f32, f32), align: (bool, fos_css::style::AlignItems)) -> Flow {
    let (center, justify_items) = align;
    let mut frags = Vec::with_capacity(children.len());
    let mut y = 0.0f32;
    let mut pending = Margin::default();
    let mut top = Margin::default();
    let mut placed = false;
    let order = flow_order(ctx, children, width);
    for child in order.iter().map(|&i| &children[i]) {
        if child.style.is_out_of_flow() {
            // Positioned later, from its static position
            frags.push(Fragment::Box(placeholder(child, Rect::new(0.0, y + pending.size(), 0.0, 0.0))));
            continue;
        }
        let rel = relative_offset(&child.style, width, cb_h);
        let float = child.style.box_.float;
        let clear_at = ctx.floats.clear_y(child.style.box_.clear).map(|c| c - base.1);
        if float != Float::None {
            // Beside earlier floats, as high as the current position
            let mut laid = layout_block_level(ctx, child, width, cb_h, Sizing::Shrink, false);
            let (w, h) = (laid.margin_box_width(), laid.mt.size() + laid.frag.border_box.h + laid.mb.size());
            let mut y_min = base.1 + y + if placed { pending.size() } else { 0.0 };
            if let Some(c) = clear_at {
                y_min = y_min.max(c + base.1);
            }
            let (fx, fy) = ctx.floats.place(w, h, float == Float::Left, y_min, base.0, base.0 + width);
            laid.frag.translate(fx - base.0 + rel.0, fy - base.1 + laid.mt.size() + rel.1);
            frags.push(Fragment::Box(laid.frag));
            continue;
        }
        // Where the child's border box will (most likely) start
        let own_mt = Margin::new(child.style.box_.margin[0].resolve(width).unwrap_or(0.0));
        let mut est_y = if !placed && collapse_top { y } else { y + pending.adjoin(own_mt).size() };
        let cleared = clear_at.filter(|&c| c > est_y);
        if let Some(c) = cleared {
            est_y = c;
        }
        ctx.origin = (base.0, base.1 + est_y);
        // Formatting context roots (overflow: hidden...) sit beside floats
        let (mut cw, mut shift) = (width, 0.0);
        if !ctx.floats.is_empty() && child.is_bfc_root() {
            let (l, r) = ctx.floats.available(base.1 + est_y, base.1 + est_y + 1.0, base.0, base.0 + width);
            cw = r - l;
            shift = l - base.0;
        }
        ctx.center_in_parent = center;
        ctx.justify_in_parent = super::grid::block_justify(&child.style, justify_items);
        let mut laid = layout_block_level(ctx, child, cw, cb_h, Sizing::Stretch, false);
        laid.frag.translate(shift, 0.0);
        if let Some(c) = cleared {
            // Clearance: the child starts below the floats; margins above
            // it no longer collapse through
            let h = laid.frag.border_box.h;
            laid.frag.translate(rel.0, c + rel.1);
            frags.push(Fragment::Box(laid.frag));
            y = c + h;
            pending = laid.mb;
            placed = true;
            continue;
        }
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

/// The stand-in for an absolutely positioned box at its static position
pub fn placeholder(b: &LayoutBox, at: Rect) -> BoxFragment {
    BoxFragment {
        node: b.node,
        kind: BoxFragmentKind::Placeholder,
        style: b.style.clone(),
        border_box: at,
        border: [0.0; 4],
        padding: [0.0; 4],
        children: Vec::new(),
        ink: at,
        marker: None,
        replaced: None,
        scroll_extent: None,
        line_baselines: None,
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

/// The character width text fields are sized by, and how much wider than
/// it their widest glyph is (as Blink: the OS/2 average, rounded, for a
/// proportional face; the "0"'s width for a fixed-pitch one)
fn field_char_width(ctx: &mut LayoutCtx, font: &super::fonts::ResolvedFont) -> (f32, Option<f32>) {
    let m = font.metrics;
    let mut zero = || ctx.fonts.shape(font, "0").width;
    if m.avg_char_width <= 0.0 {
        return (zero(), None);
    }
    let avg = if m.fixed_pitch { zero() } else { (m.avg_char_width * font.size).round() };
    (avg, Some((m.max_char_width * font.size).round() - avg))
}

/// The room a drop-down's arrow takes after its text, and the padding
/// before the text and above and below it (none when not drawn natively)
fn select_chrome(style: &Style) -> (f32, f32, f32) {
    if style.box_.appearance == fos_css::style::Appearance::Auto {
        (16.0, 4.0, 1.0)
    } else {
        (0.0, 0.0, 0.0)
    }
}

/// How far a scrollbar takes from a box that may scroll
const SCROLLBAR: f32 = 15.0;

/// A replaced element's default size when it has no natural one
fn default_replaced_size(ctx: &mut LayoutCtx, style: &Style, r: &Replaced) -> (f32, f32) {
    use fos_css::style::Overflow;
    let font = ctx.fonts.resolve(style);
    let line = font.line_height(style.inherited.line_height).max(font.ascent() + font.descent());
    let text_w = |ctx: &mut LayoutCtx, t: &str| ctx.fonts.shape(&font, t).width;
    match &r.what {
        // `size` characters, and the widest glyph's excess
        ReplacedWhat::TextField(_, _, size) => {
            let (cw, extra) = field_char_width(ctx, &font);
            ((cw * *size as f32).ceil() + extra.unwrap_or(0.0), line)
        }
        // `cols` characters and a scrollbar, `rows` lines
        ReplacedWhat::TextArea { cols, rows, .. } => {
            let (cw, _) = field_char_width(ctx, &font);
            let bar = if matches!(style.box_.overflow_y, Overflow::Auto | Overflow::Scroll) { SCROLLBAR } else { 0.0 };
            ((cw * *cols as f32).ceil() + bar, line * *rows as f32)
        }
        // As wide as the widest option: a drop-down with its arrow, a list
        // box with its rows' padding
        ReplacedWhat::Select { options, rows, .. } => {
            let widest = options.iter().map(|o| text_w(ctx, o)).fold(0.0f32, f32::max);
            match rows {
                Some(n) => (widest + 4.0, (font.ascent() + font.descent() + 2.0) * *n as f32),
                None => {
                    let (arrow, start, vpad) = select_chrome(style);
                    (widest.ceil() + start + arrow, font.ascent() + font.descent() + 2.0 * vpad)
                }
            }
        }
        ReplacedWhat::Button(label) => (text_w(ctx, label), line),
        // Room for 34 digits (its button and "No file chosen" fit in that)
        ReplacedWhat::File => ((text_w(ctx, "0") * 34.0).ceil(), line + 6.0),
        ReplacedWhat::Range(_) => (129.0, 16.0),
        ReplacedWhat::ColorWell(_) => (44.0, 23.0),
        ReplacedWhat::Gauge { meter, .. } => (if *meter { 80.0 } else { 160.0 }, 16.0),
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
        scroll_extent: None,
        line_baselines: None,
    };
    let content = frag.content_box();
    let paint = match &r.what {
        ReplacedWhat::Image => r.image.clone().map_or(ReplacedPaint::Empty, ReplacedPaint::Image),
        // Frames show their documents' pictures, supplied like canvases'
        ReplacedWhat::Canvas | ReplacedWhat::Video | ReplacedWhat::Svg | ReplacedWhat::Frame => ReplacedPaint::Bitmap,
        ReplacedWhat::Check { radio, checked } => ReplacedPaint::Check { radio: *radio, checked: *checked },
        ReplacedWhat::TextField(text, placeholder, _) => {
            if !text.is_empty() {
                let color = if *placeholder { PLACEHOLDER } else { style.color() };
                frag.children.push(Fragment::Text(control_text(ctx, style, text, content, color)));
            }
            ReplacedPaint::Empty
        }
        ReplacedWhat::TextArea { text, placeholder, .. } => {
            let color = if *placeholder { PLACEHOLDER } else { style.color() };
            control_lines(ctx, style, text, content, color, &mut frag.children);
            ReplacedPaint::Empty
        }
        ReplacedWhat::Select { shown, options, rows } => match rows {
            None => {
                let (_, start, _) = select_chrome(style);
                if !shown.is_empty() {
                    let inner = Rect::new(content.x + start, content.y, (content.w - start).max(0.0), content.h);
                    frag.children.push(Fragment::Text(control_text(ctx, style, shown, inner, style.color())));
                }
                if start > 0.0 { ReplacedPaint::Select } else { ReplacedPaint::Empty }
            }
            // A list box's options, a row each
            Some(n) => {
                let font = ctx.fonts.resolve(style);
                let row = font.ascent() + font.descent() + 2.0;
                for (i, o) in options.iter().take(*n as usize).enumerate() {
                    let r = Rect::new(content.x + 2.0, content.y + i as f32 * row, (content.w - 4.0).max(0.0), row);
                    frag.children.push(Fragment::Text(control_text(ctx, style, o, r, style.color())));
                }
                ReplacedPaint::Empty
            }
        },
        ReplacedWhat::Button(label) => {
            if !label.is_empty() {
                let font = ctx.fonts.resolve(style);
                let w = ctx.fonts.shape(&font, label).width;
                let x = content.x + ((content.w - w) / 2.0).max(0.0);
                frag.children.push(Fragment::Text(control_text(ctx, style, label, Rect::new(x, content.y, (content.right() - x).max(0.0), content.h), style.color())));
            }
            ReplacedPaint::Empty
        }
        // A button (its label padded 6px across, 1px down, in a 2px
        // border), then the note that no file is chosen
        ReplacedWhat::File => {
            let font = ctx.fonts.resolve(style);
            let label = "Choose File";
            let w = (ctx.fonts.shape(&font, label).width + 16.0).min(content.w);
            let button = Rect::new(content.x, content.y, w, content.h);
            frag.children.push(Fragment::Text(control_text(ctx, style, label, Rect::new(button.x + 8.0, button.y, (w - 8.0).max(0.0), button.h), style.color())));
            let x = button.right() + 4.0;
            frag.children.push(Fragment::Text(control_text(ctx, style, "No file chosen", Rect::new(x, content.y, (content.right() - x).max(0.0), content.h), style.color())));
            ReplacedPaint::File { button_w: w }
        }
        ReplacedWhat::Range(at) => ReplacedPaint::Range(*at),
        ReplacedWhat::ColorWell([r, g, b, a]) => ReplacedPaint::ColorWell(Color::rgba(*r, *g, *b, *a)),
        ReplacedWhat::Gauge { value, meter } => ReplacedPaint::Gauge { value: *value, meter: *meter },
    };
    // One-line controls have their text's baseline, text or not (a list
    // box, its first row's)
    let one_line = match &r.what {
        ReplacedWhat::TextField(..) | ReplacedWhat::Button(_) | ReplacedWhat::File | ReplacedWhat::ColorWell(_) => Some(content),
        ReplacedWhat::Select { rows: None, .. } => Some(content),
        ReplacedWhat::Select { rows: Some(_), .. } => {
            let font = ctx.fonts.resolve(style);
            Some(Rect::new(content.x, content.y, content.w, font.ascent() + font.descent() + 2.0))
        }
        _ => None,
    };
    if let Some(line) = one_line {
        let font = ctx.fonts.resolve(style);
        let baseline = line.y + (line.h - font.ascent() - font.descent()) / 2.0 + font.ascent();
        frag.line_baselines = Some((baseline, baseline));
    }
    // Checkboxes, radio buttons and sliders sit on their border box's
    // bottom (their bottom margin hangs below the baseline)
    if matches!(r.what, ReplacedWhat::Check { .. } | ReplacedWhat::Range(_)) {
        let bottom = frag.border_box.bottom();
        frag.line_baselines = Some((bottom, bottom));
    }
    frag.replaced = Some(Arc::new(paint));
    frag.update_ink();
    let mt = Margin::new(e.margin[0].unwrap_or(0.0));
    let mb = Margin::new(e.margin[2].unwrap_or(0.0));
    let content_h = frag.content_box().h;
    Laid { frag, mt, mb, through: false, ml, mr, content_h }
}

/// Placeholder text's color
const PLACEHOLDER: Color = Color { r: 117, g: 117, b: 117, a: 255 };

/// A textarea's lines, from the top of its content box (those that fit)
fn control_lines(ctx: &mut LayoutCtx, style: &Style, text: &str, content: Rect, color: Color, out: &mut Vec<Fragment>) {
    let font = ctx.fonts.resolve(style);
    let line = font.line_height(style.inherited.line_height).max(font.ascent() + font.descent());
    for (i, l) in text.lines().enumerate() {
        let y = content.y + i as f32 * line;
        if y + line > content.bottom() + 0.5 {
            break;
        }
        if !l.is_empty() {
            out.push(Fragment::Text(control_text(ctx, style, l, Rect::new(content.x, y, content.w, line), color)));
        }
    }
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
    // Keep the ratio when only one dimension was clamped, or when both
    // were the same way, by the limit that constrains most (CSS 2.1 §10.4)
    match (sw, sh, ratio) {
        (None, None, Some(r)) if cw != w && ch == h => (cw, clamp_height(style, cw / r, cb_h, vbp)),
        (None, None, Some(r)) if ch != h && cw == w => (clamp_width(style, ch * r, cb_w, hbp), ch),
        (None, None, Some(r)) if cw != w && ch != h && w > 0.0 && h > 0.0 && (cw < w) == (ch < h) => {
            let (sx, sy) = (cw / w, ch / h);
            let by_width = if cw < w { sx <= sy } else { sx >= sy };
            if by_width {
                (cw, clamp_height(style, cw / r, cb_h, vbp))
            } else {
                (clamp_width(style, ch * r, cb_w, hbp), ch)
            }
        }
        _ => (cw, ch),
    }
}

/// The content width an intrinsic size keyword gives a box, `avail` wide
/// at most for fit-content (`None` for no keyword)
fn intrinsic_size(ctx: &mut LayoutCtx, b: &LayoutBox, k: fos_css::style::IntrinsicSize, avail: f32) -> Option<f32> {
    use fos_css::style::IntrinsicSize;
    if k == IntrinsicSize::None {
        return None;
    }
    let (min, max) = intrinsic_content(ctx, b);
    Some(match k {
        IntrinsicSize::MinContent => min,
        IntrinsicSize::MaxContent => max,
        _ => max.min(avail).max(min),
    })
}

/// The min-content and max-content widths of a box's content box
pub fn intrinsic_content(ctx: &mut LayoutCtx, b: &LayoutBox) -> (f32, f32) {
    let key = b as *const LayoutBox as usize;
    if let Some(&sizes) = ctx.intrinsic.get(&key) {
        return sizes;
    }
    let sizes = intrinsic_content_uncached(ctx, b);
    ctx.intrinsic.insert(key, sizes);
    sizes
}

fn intrinsic_content_uncached(ctx: &mut LayoutCtx, b: &LayoutBox) -> (f32, f32) {
    match &b.kind {
        BoxKind::Inline(content) => inline::intrinsic_inline(ctx, content),
        BoxKind::Block(children) => {
            use fos_css::style::{Clear, Float};
            let (mut min, mut max) = (0.0f32, 0.0f32);
            // Floats in a row sit side by side at max-content, and beside
            // the inline content around them (the runs of text between
            // floats are one inline formatting context's)
            let (mut float_row, mut inline_row) = (0.0f32, 0.0f32);
            for c in children {
                if c.style.is_out_of_flow() {
                    continue;
                }
                let (cmin, cmax) = intrinsic_outer(ctx, c);
                min = min.max(cmin);
                if c.style.box_.float != Float::None {
                    if c.style.box_.clear != Clear::None {
                        (float_row, inline_row) = (0.0, 0.0);
                    }
                    float_row += cmax;
                    max = max.max(float_row + inline_row);
                } else if !c.node.is_valid() && matches!(c.kind, BoxKind::Inline(_)) {
                    inline_row += cmax;
                    max = max.max(float_row + inline_row);
                } else if cmax > 0.0 {
                    // A box that avoids floats sits beside them; others go
                    // below. (Empty boxes between floats, like the pieces of
                    // an inline element split around them, end no row.)
                    let beside = if c.is_bfc_root() { float_row } else { 0.0 };
                    max = max.max(beside + cmax);
                    (float_row, inline_row) = (0.0, 0.0);
                }
            }
            (min, max)
        }
        BoxKind::Flex(items) => super::flex::intrinsic_flex(ctx, &b.style, items),
        BoxKind::Table(t) => super::table::intrinsic_table(ctx, &b.style, t),
        BoxKind::Grid(items) => super::grid::intrinsic_grid(ctx, &b.style, items),
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
    let (min, max) = match (fixed, style.box_.intrinsic_width[0]) {
        (Some(w), _) => (w, w),
        (None, fos_css::style::IntrinsicSize::MinContent) => {
            let min = intrinsic_content(ctx, b).0;
            (min, min)
        }
        (None, fos_css::style::IntrinsicSize::MaxContent) => {
            let max = intrinsic_content(ctx, b).1;
            (max, max)
        }
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
