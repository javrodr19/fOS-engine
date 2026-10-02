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
}

impl BoxFragment {
    pub fn padding_box(&self) -> Rect {
        let (b, r) = (self.border, self.border_box);
        Rect::new(r.x + b[3], r.y + b[0], (r.w - b[1] - b[3]).max(0.0), (r.h - b[0] - b[2]).max(0.0))
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

    /// Recompute the ink rectangle from the children
    pub fn update_ink(&mut self) {
        let mut ink = self.border_box;
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
    /// An image, canvas or other bitmap the embedder supplies by node
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

    pub fn ink(&self) -> Rect {
        match self {
            Fragment::Box(b) => b.ink,
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
            if b.node.is_valid() {
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
        fn hits(b: &BoxFragment) -> bool {
            b.node.is_valid() && b.style.inherited.pointer_events != fos_css::style::PointerEvents::None && b.kind != BoxFragmentKind::Placeholder
        }
        fn walk(b: &BoxFragment, x: f32, y: f32) -> Option<NodeId> {
            if !b.ink.contains(x, y) {
                return None;
            }
            let mut layers: Vec<&BoxFragment> = b
                .children
                .iter()
                .filter_map(|c| match c {
                    Fragment::Box(cb) if cb.style.is_positioned() => Some(cb),
                    _ => None,
                })
                .collect();
            // Highest z-index first; later in tree order first among equals
            layers.reverse();
            layers.sort_by_key(|l| std::cmp::Reverse(l.style.box_.z_index.unwrap_or(0)));
            for l in layers {
                if let Some(n) = walk(l, x, y) {
                    return Some(n);
                }
            }
            for c in b.children.iter().rev() {
                match c {
                    Fragment::Box(cb) if cb.style.is_positioned() => {}
                    Fragment::Box(cb) => {
                        if let Some(n) = walk(cb, x, y) {
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
            (hits(b) && b.border_box.contains(x, y)).then_some(b.node)
        }
        walk(self.root.as_ref()?, x, y)
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
        }
    }
}
