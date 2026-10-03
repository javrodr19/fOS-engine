//! Layout's output: a tree of positioned fragments in document
//! coordinates, which painting, hit testing and script geometry read

use std::sync::Arc;

use fos_css::properties::Color;
use fos_css::style::Style;
use fos_dom::NodeId;

use super::fonts::{ResolvedFont, ShapedWord};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x && x < self.right() && y >= self.y && y < self.bottom()
    }

    pub fn intersects_y(&self, top: f32, bottom: f32) -> bool {
        self.bottom() > top && self.y < bottom
    }

    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }

    /// The smallest rectangle containing both (empty ones ignored)
    pub fn union(&self, o: &Rect) -> Rect {
        if o.w <= 0.0 && o.h <= 0.0 {
            return *self;
        }
        if self.w <= 0.0 && self.h <= 0.0 {
            return *o;
        }
        let (x, y) = (self.x.min(o.x), self.y.min(o.y));
        Rect { x, y, w: self.right().max(o.right()) - x, h: self.bottom().max(o.bottom()) - y }
    }

    pub fn translate(&self, dx: f32, dy: f32) -> Rect {
        Rect { x: self.x + dx, y: self.y + dy, ..*self }
    }
}

/// What a box fragment is
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxFragmentKind {
    /// A block container, or an atomic inline (inline-block)
    Block,
    /// The part of an inline element on one line
    InlinePart,
    /// An image, canvas, video, form control...
    Replaced,
    /// Where an absolutely positioned box would have been (its static
    /// position), until positioning replaces it with the box
    Placeholder,
}

/// A box: its border box and what it contains
#[derive(Clone, Debug)]
pub struct BoxFragment {
    /// `NodeId::NONE` for anonymous boxes
    pub node: NodeId,
    pub kind: BoxFragmentKind,
    pub style: Style,
    pub border_box: Rect,
    /// Used border widths and padding: top, right, bottom, left
    pub border: [f32; 4],
    pub padding: [f32; 4],
    pub children: Vec<Fragment>,
    /// The box and everything painted inside it (for culling)
    pub ink: Rect,
    /// The list marker (outside markers sit left of the first line)
    pub marker: Option<TextFragment>,
    /// A replaced element's content to paint, or a form control's text
    pub replaced: Option<Arc<ReplacedPaint>>,
    /// For boxes that clip their overflow: the size of their content
    /// (from the padding box's corner), which scrolling can reveal
    pub scroll_extent: Option<(f32, f32)>,
}

impl BoxFragment {
    pub fn padding_box(&self) -> Rect {
        let (b, r) = (self.border, self.border_box);
        Rect::new(r.x + b[3], r.y + b[0], (r.w - b[1] - b[3]).max(0.0), (r.h - b[0] - b[2]).max(0.0))
    }

    /// How far a `position: sticky` box moves down (or up) to stay in
    /// `view` (top, height: the visible part of its scroll container, in
    /// the same coordinates as the box) without leaving `container` (its
    /// parent's content box)
    pub fn sticky_offset(&self, container: Rect, view: (f32, f32)) -> f32 {
        if self.style.box_.position != fos_css::style::Position::Sticky {
            return 0.0;
        }
        let r = self.border_box;
        let inset = &self.style.box_.inset;
        if let Some(t) = inset[0].resolve(view.1) {
            let room = (container.bottom() - r.bottom()).max(0.0);
            let want = view.0 + t - r.y;
            if want > 0.0 {
                return want.min(room);
            }
        }
        if let Some(b) = inset[2].resolve(view.1) {
            let room = (container.y - r.y).min(0.0);
            let want = view.0 + view.1 - b - r.bottom();
            if want < 0.0 {
                return want.max(room);
            }
        }
        0.0
    }

    pub fn content_box(&self) -> Rect {
        let (p, r) = (self.padding, self.padding_box());
        Rect::new(r.x + p[3], r.y + p[0], (r.w - p[1] - p[3]).max(0.0), (r.h - p[0] - p[2]).max(0.0))
    }

    /// Move the box and its contents
    pub fn translate(&mut self, dx: f32, dy: f32) {
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        self.border_box = self.border_box.translate(dx, dy);
        self.ink = self.ink.translate(dx, dy);
        if let Some(m) = &mut self.marker {
            m.translate(dx, dy);
        }
        for c in &mut self.children {
            c.translate(dx, dy);
        }
    }

    /// The first line's baseline inside the box, if it has lines
    pub fn first_baseline(&self) -> Option<f32> {
        for c in &self.children {
            match c {
                Fragment::Text(t) => return Some(t.baseline),
                Fragment::Box(b) => {
                    if let Some(y) = b.first_baseline() {
                        return Some(y);
                    }
                }
            }
        }
        None
    }

    /// The last line's baseline (inline-block baselines)
    pub fn last_baseline(&self) -> Option<f32> {
        for c in self.children.iter().rev() {
            match c {
                Fragment::Text(t) => return Some(t.baseline),
                Fragment::Box(b) => {
                    if let Some(y) = b.last_baseline() {
                        return Some(y);
                    }
                }
            }
        }
        None
    }

    /// Recompute the ink rectangle from the children (a box that clips
    /// keeps its overflow inside: its ink is its own box)
    /// The box's transform, in document coordinates
    pub fn transform(&self) -> Option<fos_css::transform::Matrix> {
        // Transforms apply to block-level and atomic boxes, not inline ones
        if !self.style.has_transform() || self.kind == BoxFragmentKind::InlinePart {
            return None;
        }
        let r = self.border_box;
        self.style.transform_matrix(r.x, r.y, r.w, r.h)
    }

    /// Its ink as painted: the bounds of its transformed ink
    pub fn transformed_ink(&self) -> Rect {
        let Some(m) = self.transform() else { return self.ink };
        let r = self.ink;
        let pts = [(r.x, r.y), (r.right(), r.y), (r.x, r.bottom()), (r.right(), r.bottom())].map(|p| fos_css::transform::apply(m, p));
        let (x0, x1) = pts.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (y0, y1) = pts.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), p| (a.min(p.1), b.max(p.1)));
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            return r;
        }
        Rect::new(x0, y0, x1 - x0, y1 - y0)
    }

    pub fn update_ink(&mut self) {
        let mut ink = self.border_box;
        // Outer shadows paint beyond the border box
        for s in self.style.box_.box_shadow.iter().flat_map(|l| l.iter()).filter(|s| !s.inset) {
            let grow = s.spread + s.blur * 1.5 + 1.0;
            let r = self.border_box;
            ink = ink.union(&Rect::new(r.x + s.x - grow, r.y + s.y - grow, r.w + 2.0 * grow, r.h + 2.0 * grow));
        }
        if self.style.clips() && self.kind != BoxFragmentKind::InlinePart {
            let pad = self.padding_box();
            let mut content = Rect::new(pad.x, pad.y, 0.0, 0.0);
            for c in &self.children {
                content = content.union(&c.ink());
            }
            let w = (content.right() - pad.x + self.padding[1]).max(pad.w);
            let h = (content.bottom() - pad.y + self.padding[2]).max(pad.h);
            self.scroll_extent = Some((w, h));
            if let Some(m) = &self.marker {
                ink = ink.union(&m.rect);
            }
            self.ink = ink;
            return;
        }
        for c in &self.children {
            ink = ink.union(&c.ink());
        }
        if let Some(m) = &self.marker {
            ink = ink.union(&m.rect);
        }
        self.ink = ink;
    }
}

/// What a replaced element shows
#[derive(Debug)]
pub enum ReplacedPaint {
    /// An image the embedder loaded
    Image(super::box_tree::ImageHandle),
    /// A canvas, video or other bitmap the embedder supplies by node
    Bitmap,
    /// A checkbox or radio button, checked or not
    Check { radio: bool, checked: bool },
    /// A drop-down's arrow (its text is a child fragment)
    Select,
    /// Nothing to show (yet)
    Empty,
}

/// A run of text on one line, in one style
#[derive(Clone, Debug)]
pub struct TextFragment {
    /// The text node's parent element
    pub node: NodeId,
    /// The line-height-independent box of the glyphs (ascent to descent)
    pub rect: Rect,
    /// Baseline y
    pub baseline: f32,
    pub font: ResolvedFont,
    pub color: Color,
    /// Shaped words and their x positions
    pub words: Vec<(Arc<ShapedWord>, f32)>,
    /// text-decoration-line flags, color and thickness
    pub decoration: u8,
    pub decoration_color: Color,
    /// Shadow-free opacity is the box's; hidden text is not painted
    pub visible: bool,
}

impl TextFragment {
    pub fn translate(&mut self, dx: f32, dy: f32) {
        self.rect = self.rect.translate(dx, dy);
        self.baseline += dy;
        for w in &mut self.words {
            w.1 += dx;
        }
    }
}

#[derive(Clone, Debug)]
pub enum Fragment {
    Box(BoxFragment),
    Text(TextFragment),
}

impl Fragment {
    pub fn translate(&mut self, dx: f32, dy: f32) {
        match self {
            Fragment::Box(b) => b.translate(dx, dy),
            Fragment::Text(t) => t.translate(dx, dy),
        }
    }

    /// What the fragment covers as painted (through its transform)
    pub fn ink(&self) -> Rect {
        match self {
            Fragment::Box(b) => b.transformed_ink(),
            Fragment::Text(t) => t.rect,
        }
    }
}

/// The laid-out document
#[derive(Clone, Debug)]
pub struct FragmentTree {
    /// The root element's box (absent for empty documents)
    pub root: Option<BoxFragment>,
    /// The height of the scrollable document
    pub document_height: f32,
    pub document_width: f32,
    /// The viewport height layout used (sticky boxes stick within it)
    pub viewport_height: f32,
}

impl FragmentTree {
    /// Every box and text fragment with its element, in tree order
    pub fn for_each(&self, mut f: impl FnMut(&Fragment)) {
        fn walk(b: &BoxFragment, f: &mut dyn FnMut(&Fragment)) {
            for c in &b.children {
                f(c);
                if let Fragment::Box(cb) = c {
                    walk(cb, f);
                }
            }
        }
        if let Some(root) = &self.root {
            let wrapped = Fragment::Box(BoxFragment { children: Vec::new(), ..root.clone_shallow() });
            f(&wrapped);
            walk(root, &mut f);
        }
    }

    /// Border boxes of elements (inline elements have one per line) and
    /// the rectangles of their text, in tree order
    pub fn element_rects(&self) -> Vec<(NodeId, Rect)> {
        let mut out = Vec::new();
        fn walk(b: &BoxFragment, out: &mut Vec<(NodeId, Rect)>) {
            if b.node.is_valid() && !b.node.is_generated() {
                out.push((b.node, b.border_box));
            }
            for c in &b.children {
                match c {
                    Fragment::Box(cb) => walk(cb, out),
                    Fragment::Text(t) if t.node.is_valid() => out.push((t.node, t.rect)),
                    Fragment::Text(_) => {}
                }
            }
        }
        if let Some(root) = &self.root {
            walk(root, &mut out);
        }
        out
    }

    /// The deepest element at document point (x, y), topmost first:
    /// positioned boxes by z-index, then in-flow content (later siblings
    /// paint over earlier ones); boxes with `pointer-events: none` are
    /// transparent to hits
    pub fn hit_test(&self, x: f32, y: f32) -> Option<NodeId> {
        self.hit_test_scrolled(x, y, 0.0, &|_| (0.0, 0.0))
    }

    /// The boxes under document point (x, y) that the user can scroll
    /// (`overflow: auto/scroll` with more content than room), innermost
    /// first: (element, visible size, content size)
    pub fn scrollers_at(&self, x: f32, y: f32, scroll: f32, offsets: &dyn Fn(NodeId) -> (f32, f32)) -> Vec<(NodeId, (f32, f32), (f32, f32))> {
        fn walk(b: &BoxFragment, x: f32, y: f32, scroll: f32, offsets: &dyn Fn(NodeId) -> (f32, f32), out: &mut Vec<(NodeId, (f32, f32), (f32, f32))>) {
            if b.style.box_.position == fos_css::style::Position::Fixed && scroll != 0.0 {
                return walk(b, x, y - scroll, 0.0, offsets, out);
            }
            if !b.ink.contains(x, y) {
                return;
            }
            let (mut cx, mut cy) = (x, y);
            if let Some(extent) = b.scroll_extent {
                use fos_css::style::Overflow;
                let user = |o: Overflow| matches!(o, Overflow::Auto | Overflow::Scroll);
                let pad = b.padding_box();
                if (user(b.style.box_.overflow_x) && extent.0 > pad.w + 0.5) || (user(b.style.box_.overflow_y) && extent.1 > pad.h + 0.5) {
                    out.push((b.node, (pad.w, pad.h), extent));
                }
                let (ox, oy) = offsets(b.node);
                cx += ox;
                cy += oy;
            }
            for c in &b.children {
                if let Fragment::Box(cb) = c {
                    walk(cb, cx, cy, scroll, offsets, out);
                }
            }
        }
        let mut out = Vec::new();
        if let Some(r) = &self.root {
            walk(r, x, y, scroll, offsets, &mut out);
        }
        out.reverse();
        out
    }

    /// [`Self::hit_test`] with the page scrolled by `scroll` (fixed boxes
    /// stay in the viewport) and scroll containers by `offsets`
    pub fn hit_test_scrolled(&self, x: f32, y: f32, scroll: f32, offsets: &dyn Fn(NodeId) -> (f32, f32)) -> Option<NodeId> {
        fn hits(b: &BoxFragment) -> bool {
            b.node.is_valid() && b.style.inherited.pointer_events != fos_css::style::PointerEvents::None && b.kind != BoxFragmentKind::Placeholder
        }
        fn walk(b: &BoxFragment, x: f32, y: f32, scroll: f32, view: (f32, f32), offsets: &dyn Fn(NodeId) -> (f32, f32)) -> Option<NodeId> {
            if b.style.box_.position == fos_css::style::Position::Fixed && scroll != 0.0 {
                return walk(b, x, y - scroll, 0.0, (0.0, view.1), offsets);
            }
            // A transformed box is hit where it is painted
            let (x, y) = match b.transform().and_then(fos_css::transform::invert) {
                Some(inv) => fos_css::transform::apply(inv, (x, y)),
                None => (x, y),
            };
            if !b.ink.contains(x, y) && !(scroll != 0.0 && b.children.iter().any(|c| matches!(c, Fragment::Box(_)))) {
                return None;
            }
            // Inside a scrolled box, its content has moved
            let (bx, by) = (x, y);
            let (x, y, view) = match b.scroll_extent {
                Some(_) => {
                    let (ox, oy) = offsets(b.node);
                    let pad = b.padding_box();
                    (x + ox, y + oy, (pad.y + oy, pad.h))
                }
                None => (x, y, view),
            };
            let container = b.content_box();
            let mut layers: Vec<&BoxFragment> = b
                .children
                .iter()
                .filter_map(|c| match c {
                    Fragment::Box(cb) if cb.style.is_positioned() || cb.style.has_transform() => Some(cb),
                    _ => None,
                })
                .collect();
            // Highest z-index first; later in tree order first among equals
            layers.reverse();
            layers.sort_by_key(|l| std::cmp::Reverse(l.style.box_.z_index.unwrap_or(0)));
            for l in layers {
                // A sticky box is shown moved
                let y = y - l.sticky_offset(container, view);
                if let Some(n) = walk(l, x, y, scroll, view, offsets) {
                    return Some(n);
                }
            }
            for c in b.children.iter().rev() {
                match c {
                    Fragment::Box(cb) if cb.style.is_positioned() || cb.style.has_transform() => {}
                    Fragment::Box(cb) => {
                        if let Some(n) = walk(cb, x, y, scroll, view, offsets) {
                            return Some(n);
                        }
                    }
                    Fragment::Text(t) => {
                        if t.rect.contains(x, y) && t.node.is_valid() {
                            return Some(t.node);
                        }
                    }
                }
            }
            (hits(b) && b.border_box.contains(bx, by)).then_some(b.node)
        }
        // A hit on a `::before`/`::after` box is a hit on its element
        walk(self.root.as_ref()?, x, y, scroll, (scroll, self.viewport_height), offsets).map(NodeId::originating)
    }
}

impl BoxFragment {
    /// A copy without children (cheap; for visiting)
    pub fn clone_shallow(&self) -> BoxFragment {
        BoxFragment {
            node: self.node,
            kind: self.kind,
            style: self.style.clone(),
            border_box: self.border_box,
            border: self.border,
            padding: self.padding,
            children: Vec::new(),
            ink: self.ink,
            marker: self.marker.clone(),
            replaced: self.replaced.clone(),
            scroll_extent: self.scroll_extent,
        }
    }
}
