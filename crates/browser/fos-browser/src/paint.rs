//! Painting a fragment tree: backgrounds (colors and gradients), borders
//! (all styles, rounded corners), text with decorations, form controls,
//! outlines, overflow clipping and opacity
//!
//! Boxes are painted in tree order, each box's background and borders
//! before its contents, skipping everything outside the painted band.

use fos_canvas::tiny_skia::{
    self, FillRule, GradientStop, LinearGradient, Mask, Paint, Path, PathBuilder, Point, RadialGradient, Shader, SpreadMode, Stroke,
    StrokeDash, Transform,
};
use fos_css::properties::Color as CssColor;
use fos_css::style::decoration;
use fos_css::style::{
    BackgroundRepeat, BackgroundSize, BorderStyle, ColorStop, GradientDirection, Image, Lp, RadialSize, Visibility,
};
use fos_layout::engine::{BoxFragment, BoxFragmentKind, Fragment, FragmentTree, Rect, ReplacedPaint, TextFragment};
use fos_dom::NodeId;
use fos_render::{Canvas, TextRenderer};

/// Paints into a canvas showing the document band starting at `origin`
pub struct Painter<'a> {
    canvas: &'a mut Canvas,
    text: &'a mut TextRenderer,
    origin: f32,
    /// The last clip mask built, by clip rectangle
    mask: Option<([i32; 4], Mask)>,
    /// The element whose background became the canvas's (not painted
    /// again)
    canvas_background_box: Option<NodeId>,
    /// How far the page is scrolled (fixed boxes ignore it)
    scroll: f32,
    /// Fixed boxes may sit inside boxes scrolled out of view: layers are
    /// then looked for everywhere
    find_fixed: bool,
}

/// A device-space clip rectangle: x0, y0, x1, y1
#[derive(Clone, Copy, PartialEq)]
struct Clip([f32; 4]);

impl Clip {
    fn intersect(self, r: Rect) -> Clip {
        let c = self.0;
        Clip([c[0].max(r.x), c[1].max(r.y), c[2].min(r.right()), c[3].min(r.bottom())])
    }

    fn is_empty(self) -> bool {
        self.0[0] >= self.0[2] || self.0[1] >= self.0[3]
    }

    fn contains(self, r: Rect) -> bool {
        r.x >= self.0[0] && r.y >= self.0[1] && r.right() <= self.0[2] && r.bottom() <= self.0[3]
    }

    fn pixels(self) -> [i32; 4] {
        [self.0[0].floor() as i32, self.0[1].floor() as i32, self.0[2].ceil() as i32, self.0[3].ceil() as i32]
    }
}

fn skia_color(c: CssColor, alpha: f32) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(c.r, c.g, c.b, (c.a as f32 * alpha).round().clamp(0.0, 255.0) as u8)
}

fn render_color(c: CssColor, alpha: f32) -> fos_render::Color {
    fos_render::Color::rgba(c.r, c.g, c.b, (c.a as f32 * alpha).round().clamp(0.0, 255.0) as u8)
}

fn shade(c: CssColor, f: f32) -> CssColor {
    CssColor { r: (c.r as f32 * f) as u8, g: (c.g as f32 * f) as u8, b: (c.b as f32 * f) as u8, a: c.a }
}

/// Corner radii (horizontal, vertical): top-left, top-right, bottom-right,
/// bottom-left, reduced so adjacent curves never overlap
fn radii(b: &BoxFragment, r: Rect) -> [(f32, f32); 4] {
    let spec = &b.style.border.radius;
    let mut out = [(0.0, 0.0); 4];
    for i in 0..4 {
        let (h, v): (Lp, Lp) = spec[i];
        out[i] = (h.resolve(r.w).max(0.0), v.resolve(r.h).max(0.0));
        if out[i].0 == 0.0 || out[i].1 == 0.0 {
            out[i] = (0.0, 0.0);
        }
    }
    let mut f = 1.0f32;
    let fit = |len: f32, a: f32, b: f32| if a + b > len && a + b > 0.0 { len / (a + b) } else { 1.0 };
    f = f.min(fit(r.w, out[0].0, out[1].0)).min(fit(r.w, out[3].0, out[2].0));
    f = f.min(fit(r.h, out[0].1, out[3].1)).min(fit(r.h, out[1].1, out[2].1));
    if f < 1.0 {
        for c in &mut out {
            c.0 *= f;
            c.1 *= f;
        }
    }
    out
}

/// Radii shrunk by edge widths (top, right, bottom, left): the curve of an
/// inner edge
fn inset_radii(r: [(f32, f32); 4], e: [f32; 4]) -> [(f32, f32); 4] {
    [
        ((r[0].0 - e[3]).max(0.0), (r[0].1 - e[0]).max(0.0)),
        ((r[1].0 - e[1]).max(0.0), (r[1].1 - e[0]).max(0.0)),
        ((r[2].0 - e[1]).max(0.0), (r[2].1 - e[2]).max(0.0)),
        ((r[3].0 - e[3]).max(0.0), (r[3].1 - e[2]).max(0.0)),
    ]
}

/// A rectangle with elliptical corners
fn push_rounded(pb: &mut PathBuilder, r: Rect, rad: [(f32, f32); 4]) {
    const K: f32 = 0.552_284_8;
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    pb.move_to(x0 + rad[0].0, y0);
    pb.line_to(x1 - rad[1].0, y0);
    if rad[1].0 > 0.0 {
        pb.cubic_to(x1 - rad[1].0 * (1.0 - K), y0, x1, y0 + rad[1].1 * (1.0 - K), x1, y0 + rad[1].1);
    }
    pb.line_to(x1, y1 - rad[2].1);
    if rad[2].0 > 0.0 {
        pb.cubic_to(x1, y1 - rad[2].1 * (1.0 - K), x1 - rad[2].0 * (1.0 - K), y1, x1 - rad[2].0, y1);
    }
    pb.line_to(x0 + rad[3].0, y1);
    if rad[3].0 > 0.0 {
        pb.cubic_to(x0 + rad[3].0 * (1.0 - K), y1, x0, y1 - rad[3].1 * (1.0 - K), x0, y1 - rad[3].1);
    }
    pb.line_to(x0, y0 + rad[0].1);
    if rad[0].0 > 0.0 {
        pb.cubic_to(x0, y0 + rad[0].1 * (1.0 - K), x0 + rad[0].0 * (1.0 - K), y0, x0 + rad[0].0, y0);
    }
    pb.close();
}

fn rounded_path(r: Rect, rad: [(f32, f32); 4]) -> Option<Path> {
    if r.w <= 0.0 || r.h <= 0.0 {
        return None;
    }
    let mut pb = PathBuilder::new();
    if rad.iter().all(|c| c.0 == 0.0) {
        pb.push_rect(tiny_skia::Rect::from_xywh(r.x, r.y, r.w, r.h)?);
    } else {
        push_rounded(&mut pb, r, rad);
    }
    pb.finish()
}

/// The background color of the canvas: the root's, or the body's when the
/// root has none (CSS Backgrounds §2.11.2)
pub fn canvas_background(tree: &FragmentTree, is_body: impl Fn(&BoxFragment) -> bool) -> (CssColor, Option<NodeId>) {
    let Some(root) = &tree.root else { return (CssColor::WHITE, None) };
    if root.style.background.color.a > 0 {
        return (root.style.background.color, Some(root.node));
    }
    for c in &root.children {
        if let Fragment::Box(b) = c {
            if is_body(b) {
                if b.style.background.color.a > 0 {
                    return (b.style.background.color, Some(b.node));
                }
                break;
            }
        }
    }
    (CssColor::WHITE, None)
}

impl<'a> Painter<'a> {
    /// `origin` is the document y of the canvas's first row; `scroll` the
    /// page's scroll position (the canvas may be a band below its top)
    pub fn new(canvas: &'a mut Canvas, text: &'a mut TextRenderer, origin: f32, scroll: f32, canvas_background_box: Option<NodeId>) -> Self {
        Painter { canvas, text, origin, mask: None, canvas_background_box, scroll, find_fixed: false }
    }

    /// The page has fixed boxes
    pub fn with_fixed(mut self, has_fixed: bool) -> Self {
        self.find_fixed = has_fixed;
        self
    }

    pub fn paint(&mut self, tree: &FragmentTree) {
        let Some(root) = &tree.root else { return };
        let clip = Clip([0.0, 0.0, self.canvas.width() as f32, self.canvas.height() as f32]);
        self.paint_layer(root, clip, 1.0);
    }

    /// Document rectangle to device
    fn dev(&self, r: Rect) -> Rect {
        Rect::new(r.x, r.y - self.origin, r.w, r.h)
    }

    fn mask_for(&mut self, clip: Clip) -> Option<&Mask> {
        let px = clip.pixels();
        if self.mask.as_ref().is_none_or(|(k, _)| *k != px) {
            let mut mask = Mask::new(self.canvas.width(), self.canvas.height())?;
            let mut pb = PathBuilder::new();
            pb.push_rect(tiny_skia::Rect::from_ltrb(clip.0[0], clip.0[1], clip.0[2], clip.0[3])?);
            mask.fill_path(&pb.finish()?, FillRule::Winding, false, Transform::identity());
            self.mask = Some((px, mask));
        }
        self.mask.as_ref().map(|m| &m.1)
    }

    /// Fill a path (device space) clipped to `clip`
    fn fill(&mut self, path: &Path, paint: &Paint, rule: FillRule, clip: Clip) {
        let b = path.bounds();
        let bounds = Rect::new(b.x(), b.y(), b.width(), b.height());
        if clip.is_empty() || bounds.right() < clip.0[0] || bounds.x > clip.0[2] || bounds.bottom() < clip.0[1] || bounds.y > clip.0[3] {
            return;
        }
        if clip.contains(bounds) {
            if let Some(mut pm) = self.canvas.pixmap_mut() {
                pm.fill_path(path, paint, rule, Transform::identity(), None);
            }
            return;
        }
        // Partly clipped: a plain rectangle is intersected, anything else
        // is masked
        if let Some(rect) = path.compute_tight_bounds().filter(|_| is_axis_rect(path)) {
            let r = Clip(clip.0).intersect(Rect::new(rect.x(), rect.y(), rect.width(), rect.height()));
            if r.is_empty() {
                return;
            }
            if let (Some(rect), Some(mut pm)) = (tiny_skia::Rect::from_ltrb(r.0[0], r.0[1], r.0[2], r.0[3]), self.canvas.pixmap_mut()) {
                pm.fill_rect(rect, paint, Transform::identity(), None);
            }
            return;
        }
        let Some(mask) = self.mask_for(clip).cloned() else { return };
        if let Some(mut pm) = self.canvas.pixmap_mut() {
            pm.fill_path(path, paint, rule, Transform::identity(), Some(&mask));
        }
    }

    fn fill_rect(&mut self, r: Rect, color: tiny_skia::Color, clip: Clip) {
        let c = clip.intersect(r);
        if c.is_empty() || color.alpha() == 0.0 {
            return;
        }
        let mut paint = Paint::default();
        paint.set_color(color);
        if let (Some(rect), Some(mut pm)) = (tiny_skia::Rect::from_ltrb(c.0[0], c.0[1], c.0[2], c.0[3]), self.canvas.pixmap_mut()) {
            pm.fill_rect(rect, &paint, Transform::identity(), None);
        }
    }

    fn culled(&self, b: &BoxFragment, clip: Clip) -> bool {
        let ink = self.dev(b.ink);
        ink.bottom() < clip.0[1] || ink.y > clip.0[3]
    }

    /// Clip for a box's contents
    fn inner_clip(&self, b: &BoxFragment, clip: Clip) -> Clip {
        if b.style.clips() {
            clip.intersect(self.dev(b.padding_box()))
        } else {
            clip
        }
    }

    /// A positioned box paints as a layer of its stacking context
    fn is_layer(b: &BoxFragment) -> bool {
        b.style.is_positioned() && b.kind != BoxFragmentKind::Placeholder
    }

    /// Paint a box as a layer (CSS 2.1 Appendix E, simplified): its
    /// background and borders, positioned descendants with negative
    /// z-index, the in-flow content, then positioned descendants by
    /// z-index (tree order among equals)
    fn paint_layer(&mut self, b: &BoxFragment, clip: Clip, alpha: f32) {
        if b.style.box_.position == fos_css::style::Position::Fixed && self.scroll != 0.0 {
            // Laid out against the viewport at the top of the page: drawn
            // where the viewport is now
            let saved = self.origin;
            self.origin -= self.scroll;
            let full = Clip([0.0, 0.0, self.canvas.width() as f32, self.canvas.height() as f32]);
            self.scroll = 0.0;
            self.paint_layer(b, full, alpha);
            self.scroll = saved - self.origin;
            self.origin = saved;
            return;
        }
        if self.culled(b, clip) {
            return;
        }
        let alpha = alpha * b.style.box_.opacity;
        if alpha <= 0.0 {
            return;
        }
        self.paint_own(b, clip, alpha);
        let inner = self.inner_clip(b, clip);
        let mut layers: Vec<(i32, &BoxFragment, Clip, f32)> = Vec::new();
        self.collect_layers(b, inner, alpha, &mut layers);
        layers.sort_by_key(|l| l.0);
        for &(_, l, c, a) in layers.iter().filter(|l| l.0 < 0) {
            self.paint_layer(l, c, a);
        }
        self.paint_flow(b, inner, alpha);
        for &(_, l, c, a) in layers.iter().filter(|l| l.0 >= 0) {
            self.paint_layer(l, c, a);
        }
        if b.style.inherited.visibility == Visibility::Visible {
            self.outline(b, clip, alpha);
        }
    }

    /// The positioned boxes under `b` that are not inside another layer
    fn collect_layers<'t>(&self, b: &'t BoxFragment, clip: Clip, alpha: f32, out: &mut Vec<(i32, &'t BoxFragment, Clip, f32)>) {
        for c in &b.children {
            let Fragment::Box(cb) = c else { continue };
            if Self::is_layer(cb) {
                out.push((cb.style.box_.z_index.unwrap_or(0), cb, clip, alpha));
            } else if self.find_fixed || !self.culled(cb, clip) {
                self.collect_layers(cb, self.inner_clip(cb, clip), alpha * cb.style.box_.opacity, out);
            }
        }
    }

    /// Background, borders and replaced content of one box
    fn paint_own(&mut self, b: &BoxFragment, clip: Clip, alpha: f32) {
        if b.style.inherited.visibility != Visibility::Visible {
            return;
        }
        if b.kind == BoxFragmentKind::InlinePart || self.canvas_background_box != Some(b.node) {
            self.background(b, clip, alpha);
        } else {
            // The color went to the canvas; gradients still paint here
            self.background_images(b, clip, alpha);
        }
        self.border(b, clip, alpha);
        if let Some(r) = &b.replaced {
            self.replaced(b, r, clip, alpha);
        }
    }

    /// The in-flow contents of `b` (its children that are not layers)
    fn paint_flow(&mut self, b: &BoxFragment, clip: Clip, alpha: f32) {
        if clip.is_empty() {
            return;
        }
        if let Some(m) = &b.marker {
            self.text(m, clip, alpha);
        }
        for c in &b.children {
            match c {
                Fragment::Text(t) => self.text(t, clip, alpha),
                Fragment::Box(cb) => {
                    if Self::is_layer(cb) || self.culled(cb, clip) {
                        continue;
                    }
                    let a = alpha * cb.style.box_.opacity;
                    if a <= 0.0 {
                        continue;
                    }
                    self.paint_own(cb, clip, a);
                    self.paint_flow(cb, self.inner_clip(cb, clip), a);
                    if cb.style.inherited.visibility == Visibility::Visible {
                        self.outline(cb, clip, a);
                    }
                }
            }
        }
    }

    fn background(&mut self, b: &BoxFragment, clip: Clip, alpha: f32) {
        let bg = &b.style.background;
        if !bg.is_visible() {
            return;
        }
        let r = self.dev(b.border_box);
        if bg.color.a > 0 {
            let rad = radii(b, r);
            if rad.iter().all(|c| c.0 == 0.0) {
                self.fill_rect(r, skia_color(bg.color, alpha), clip);
            } else if let Some(path) = rounded_path(r, rad) {
                let mut paint = Paint::default();
                paint.set_color(skia_color(bg.color, alpha));
                paint.anti_alias = true;
                self.fill(&path, &paint, FillRule::Winding, clip);
            }
        }
        self.background_images(b, clip, alpha);
    }

    /// Gradient layers, bottom first (`url()` images come with image
    /// loading)
    fn background_images(&mut self, b: &BoxFragment, clip: Clip, alpha: f32) {
        let bg = &b.style.background;
        if bg.images.is_empty() {
            return;
        }
        let border_box = self.dev(b.border_box);
        let area = {
            let p = b.padding_box();
            self.dev(p)
        };
        let rad = radii(b, border_box);
        let shape = rounded_path(border_box, rad);
        let n = bg.images.len();
        for i in (0..n).rev() {
            let image = &bg.images[i];
            if matches!(image, Image::None | Image::Url(_)) {
                continue;
            }
            let size = bg.size.get(i % bg.size.len().max(1)).copied().unwrap_or_default();
            let pos = bg.position.get(i % bg.position.len().max(1)).copied().unwrap_or((Lp::ZERO, Lp::ZERO));
            let repeat = bg.repeat.get(i % bg.repeat.len().max(1)).copied().unwrap_or((BackgroundRepeat::Repeat, BackgroundRepeat::Repeat));
            let (tw, th) = match size {
                BackgroundSize::Auto | BackgroundSize::Cover | BackgroundSize::Contain => (area.w, area.h),
                BackgroundSize::Explicit(w, h) => (w.resolve(area.w).unwrap_or(area.w), h.resolve(area.h).unwrap_or(area.h)),
            };
            if tw < 0.5 || th < 0.5 {
                continue;
            }
            let x0 = area.x + pos.0.resolve(area.w - tw);
            let y0 = area.y + pos.1.resolve(area.h - th);
            let tiles = |start: f32, len: f32, lo: f32, hi: f32, rep: BackgroundRepeat| -> Vec<f32> {
                if rep == BackgroundRepeat::NoRepeat || len <= 0.0 {
                    return vec![start];
                }
                let first = start - ((start - lo) / len).ceil() * len;
                let mut v = Vec::new();
                let mut p = first;
                while p < hi && v.len() < 512 {
                    v.push(p);
                    p += len;
                }
                v
            };
            let rx = matches!(repeat.0, BackgroundRepeat::Repeat | BackgroundRepeat::RepeatX | BackgroundRepeat::Space | BackgroundRepeat::Round);
            let ry = matches!(repeat.1, BackgroundRepeat::Repeat | BackgroundRepeat::RepeatY | BackgroundRepeat::Space | BackgroundRepeat::Round);
            let xs = tiles(x0, tw, border_box.x, border_box.right(), if rx { BackgroundRepeat::Repeat } else { BackgroundRepeat::NoRepeat });
            let ys = tiles(y0, th, border_box.y, border_box.bottom(), if ry { BackgroundRepeat::Repeat } else { BackgroundRepeat::NoRepeat });
            let painting_clip = clip.intersect(border_box);
            for &ty in &ys {
                if ty > painting_clip.0[3] || ty + th < painting_clip.0[1] {
                    continue;
                }
                for &tx in &xs {
                    let tile = Rect::new(tx, ty, tw, th);
                    let Some(shader) = gradient_shader(image, tile, alpha) else { continue };
                    let mut paint = Paint::default();
                    paint.shader = shader;
                    paint.anti_alias = true;
                    let tile_clip = painting_clip.intersect(tile);
                    match &shape {
                        Some(path) if rad.iter().any(|c| c.0 > 0.0) => self.fill(path, &paint, FillRule::Winding, tile_clip),
                        _ => {
                            if let Some(path) = rounded_path(tile, [(0.0, 0.0); 4]) {
                                self.fill(&path, &paint, FillRule::Winding, tile_clip);
                            }
                        }
                    }
                }
            }
        }
    }

    fn border(&mut self, b: &BoxFragment, clip: Clip, alpha: f32) {
        let bs = &b.style.border;
        let w = b.border;
        if w.iter().all(|&x| x <= 0.0) {
            return;
        }
        let color_of = |i: usize| bs.color[i].unwrap_or(b.style.color());
        let outer = self.dev(b.border_box);
        let rad = radii(b, outer);
        let uniform = (1..4).all(|i| w[i] == w[0] && bs.style[i] == bs.style[0] && color_of(i) == color_of(0));
        let style0 = bs.style[0];
        if uniform && matches!(style0, BorderStyle::Solid) {
            // One ring: outer shape minus inner shape
            let inner = Rect::new(outer.x + w[3], outer.y + w[0], (outer.w - w[1] - w[3]).max(0.0), (outer.h - w[0] - w[2]).max(0.0));
            let mut pb = PathBuilder::new();
            push_rounded(&mut pb, outer, rad);
            if inner.w > 0.0 && inner.h > 0.0 {
                push_rounded(&mut pb, inner, inset_radii(rad, w));
            }
            if let Some(path) = pb.finish() {
                let mut paint = Paint::default();
                paint.set_color(skia_color(color_of(0), alpha));
                paint.anti_alias = true;
                self.fill(&path, &paint, FillRule::EvenOdd, clip);
            }
            return;
        }
        for side in 0..4 {
            if w[side] <= 0.0 {
                continue;
            }
            let base = color_of(side);
            match bs.style[side] {
                BorderStyle::None | BorderStyle::Hidden => {}
                BorderStyle::Dashed | BorderStyle::Dotted => self.dashed_side(outer, w, side, base, bs.style[side] == BorderStyle::Dotted, clip, alpha),
                BorderStyle::Double if w[side] >= 3.0 => {
                    self.side(outer, w, side, 0.0, 1.0 / 3.0, base, clip, alpha);
                    self.side(outer, w, side, 2.0 / 3.0, 1.0, base, clip, alpha);
                }
                BorderStyle::Inset | BorderStyle::Outset => {
                    let dark_side = (side == 0 || side == 3) == (bs.style[side] == BorderStyle::Inset);
                    let c = if dark_side { shade(base, 0.6) } else { base };
                    self.side(outer, w, side, 0.0, 1.0, c, clip, alpha);
                }
                BorderStyle::Groove | BorderStyle::Ridge => {
                    let top_left = side == 0 || side == 3;
                    let (a, bcol) = if (bs.style[side] == BorderStyle::Groove) == top_left { (shade(base, 0.6), base) } else { (base, shade(base, 0.6)) };
                    self.side(outer, w, side, 0.0, 0.5, a, clip, alpha);
                    self.side(outer, w, side, 0.5, 1.0, bcol, clip, alpha);
                }
                _ => self.side(outer, w, side, 0.0, 1.0, base, clip, alpha),
            }
        }
    }

    /// One side's band from fraction `t0` to `t1` of its width, as a
    /// trapezoid mitered at the corners
    #[allow(clippy::too_many_arguments)]
    fn side(&mut self, r: Rect, w: [f32; 4], side: usize, t0: f32, t1: f32, color: CssColor, clip: Clip, alpha: f32) {
        let at = |t: f32| (r.x + w[3] * t, r.y + w[0] * t, r.right() - w[1] * t, r.bottom() - w[2] * t);
        let (ox0, oy0, ox1, oy1) = at(t0);
        let (ix0, iy0, ix1, iy1) = at(t1);
        let pts = match side {
            0 => [(ox0, oy0), (ox1, oy0), (ix1, iy0), (ix0, iy0)],
            1 => [(ox1, oy0), (ox1, oy1), (ix1, iy1), (ix1, iy0)],
            2 => [(ox1, oy1), (ox0, oy1), (ix0, iy1), (ix1, iy1)],
            _ => [(ox0, oy1), (ox0, oy0), (ix0, iy0), (ix0, iy1)],
        };
        let mut pb = PathBuilder::new();
        pb.move_to(pts[0].0, pts[0].1);
        for p in &pts[1..] {
            pb.line_to(p.0, p.1);
        }
        pb.close();
        if let Some(path) = pb.finish() {
            let mut paint = Paint::default();
            paint.set_color(skia_color(color, alpha));
            paint.anti_alias = true;
            self.fill(&path, &paint, FillRule::Winding, clip);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn dashed_side(&mut self, r: Rect, w: [f32; 4], side: usize, color: CssColor, dotted: bool, clip: Clip, alpha: f32) {
        let width = w[side];
        let half = width / 2.0;
        let (a, b) = match side {
            0 => ((r.x, r.y + half), (r.right(), r.y + half)),
            1 => ((r.right() - half, r.y), (r.right() - half, r.bottom())),
            2 => ((r.right(), r.bottom() - half), (r.x, r.bottom() - half)),
            _ => ((r.x + half, r.bottom()), (r.x + half, r.y)),
        };
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        let Some(line) = pb.finish() else { return };
        let dash = if dotted { vec![width.max(0.5), width.max(0.5)] } else { vec![width * 3.0, width * 2.0] };
        // Path::stroke ignores dashes: dash the line first
        let Some(dashed) = StrokeDash::new(dash, 0.0).and_then(|d| line.dash(&d, 1.0)) else { return };
        let stroke = Stroke { width, line_cap: if dotted { tiny_skia::LineCap::Round } else { tiny_skia::LineCap::Butt }, ..Stroke::default() };
        let Some(path) = dashed.stroke(&stroke, 1.0) else { return };
        let mut paint = Paint::default();
        paint.set_color(skia_color(color, alpha));
        paint.anti_alias = true;
        self.fill(&path, &paint, FillRule::Winding, clip);
    }

    fn outline(&mut self, b: &BoxFragment, clip: Clip, alpha: f32) {
        let bs = &b.style.border;
        let w = bs.outline_width;
        if w <= 0.0 || matches!(bs.outline_style, BorderStyle::None | BorderStyle::Hidden) {
            return;
        }
        let o = bs.outline_offset + w;
        let r = self.dev(b.border_box);
        let outer = Rect::new(r.x - o, r.y - o, r.w + 2.0 * o, r.h + 2.0 * o);
        let color = bs.outline_color.unwrap_or(b.style.color());
        for side in 0..4 {
            self.side(outer, [w; 4], side, 0.0, 1.0, color, clip, alpha);
        }
    }

    /// Form controls drawn by the browser
    fn replaced(&mut self, b: &BoxFragment, r: &ReplacedPaint, clip: Clip, alpha: f32) {
        let c = self.dev(b.content_box());
        match r {
            ReplacedPaint::Check { radio, checked } => {
                let size = c.w.min(c.h);
                let bx = Rect::new(c.x + (c.w - size) / 2.0, c.y + (c.h - size) / 2.0, size, size);
                let accent = CssColor::rgba(0, 117, 255, 255);
                let (fill, stroke) = if *checked { (accent, accent) } else { (CssColor::WHITE, CssColor::rgba(118, 118, 118, 255)) };
                let rad = if *radio { size / 2.0 } else { 2.0 };
                if let Some(p) = rounded_path(bx, [(rad, rad); 4]) {
                    let mut paint = Paint::default();
                    paint.anti_alias = true;
                    paint.set_color(skia_color(fill, alpha));
                    self.fill(&p, &paint, FillRule::Winding, clip);
                    if let Some(ring) = p.stroke(&Stroke { width: 1.0, ..Stroke::default() }, 1.0) {
                        paint.set_color(skia_color(stroke, alpha));
                        self.fill(&ring, &paint, FillRule::Winding, clip);
                    }
                }
                if *checked {
                    let mut paint = Paint::default();
                    paint.anti_alias = true;
                    paint.set_color(skia_color(CssColor::WHITE, alpha));
                    if *radio {
                        let d = size * 0.4;
                        if let Some(p) = rounded_path(Rect::new(bx.x + (size - d) / 2.0, bx.y + (size - d) / 2.0, d, d), [(d / 2.0, d / 2.0); 4]) {
                            self.fill(&p, &paint, FillRule::Winding, clip);
                        }
                    } else {
                        let mut pb = PathBuilder::new();
                        pb.move_to(bx.x + size * 0.22, bx.y + size * 0.52);
                        pb.line_to(bx.x + size * 0.42, bx.y + size * 0.72);
                        pb.line_to(bx.x + size * 0.78, bx.y + size * 0.3);
                        if let Some(tick) = pb.finish().and_then(|p| p.stroke(&Stroke { width: (size / 7.0).max(1.5), ..Stroke::default() }, 1.0)) {
                            self.fill(&tick, &paint, FillRule::Winding, clip);
                        }
                    }
                }
            }
            ReplacedPaint::Select => {
                let s = (c.h * 0.3).clamp(3.0, 6.0);
                let (cx, cy) = (c.right() - s - 2.0, c.y + c.h / 2.0);
                let mut pb = PathBuilder::new();
                pb.move_to(cx - s, cy - s / 2.0);
                pb.line_to(cx + s, cy - s / 2.0);
                pb.line_to(cx, cy + s / 2.0);
                pb.close();
                if let Some(p) = pb.finish() {
                    let mut paint = Paint::default();
                    paint.anti_alias = true;
                    paint.set_color(skia_color(b.style.color(), alpha));
                    self.fill(&p, &paint, FillRule::Winding, clip);
                }
            }
            ReplacedPaint::Bitmap | ReplacedPaint::Empty => {}
        }
    }

    fn text(&mut self, t: &TextFragment, clip: Clip, alpha: f32) {
        if !t.visible || t.color.a == 0 {
            return;
        }
        let r = self.dev(t.rect);
        // Glyphs may reach past the font's ascent and descent
        let slack = t.font.size * 0.5;
        if r.bottom() + slack < clip.0[1] || r.y - slack > clip.0[3] || r.x > clip.0[2] || r.right() < clip.0[0] {
            return;
        }
        let color = render_color(t.color, alpha);
        let baseline = t.baseline - self.origin;
        let pixel_clip = clip.pixels();
        for (word, x) in &t.words {
            let Some(font) = word.font else { continue };
            if x + word.width < clip.0[0] || *x > clip.0[2] {
                continue;
            }
            let x = *x;
            self.text.draw_glyph_run(self.canvas, font, word.size, t.font.synthesis, color, word.glyphs.iter().map(|g| (g.id, x + g.x, baseline - g.y)), pixel_clip);
        }
        if t.decoration != 0 {
            let size = t.font.size;
            let thickness = (size / 14.0).max(1.0);
            let dc = skia_color(t.decoration_color, alpha);
            let mut line = |y: f32, painter: &mut Self| painter.fill_rect(Rect::new(r.x, y, r.w, thickness), dc, clip);
            if t.decoration & decoration::UNDERLINE != 0 {
                line(baseline + (t.font.descent() * 0.45).max(1.0), self);
            }
            if t.decoration & decoration::OVERLINE != 0 {
                line(baseline - t.font.ascent(), self);
            }
            if t.decoration & decoration::LINE_THROUGH != 0 {
                line(baseline - size * 0.3, self);
            }
        }
    }
}

/// Whether a path is an axis-aligned rectangle (four corner points)
fn is_axis_rect(path: &Path) -> bool {
    let pts = path.points();
    pts.len() == 4 && {
        let (xs, ys): (Vec<f32>, Vec<f32>) = pts.iter().map(|p| (p.x, p.y)).unzip();
        let distinct = |v: &[f32]| {
            let mut d: Vec<f32> = Vec::new();
            for &x in v {
                if !d.iter().any(|&y| (y - x).abs() < 1e-3) {
                    d.push(x);
                }
            }
            d.len()
        };
        let corners = (0..4).all(|i| (0..i).all(|j| (pts[i].x - pts[j].x).abs() > 1e-3 || (pts[i].y - pts[j].y).abs() > 1e-3));
        distinct(&xs) == 2 && distinct(&ys) == 2 && corners
    }
}

/// Stops as fractions along a gradient line of length `len`, positions
/// implied by neighbors filled in and kept in order
fn stop_fractions(stops: &[ColorStop], len: f32) -> Vec<f32> {
    let n = stops.len();
    let mut pos: Vec<Option<f32>> = stops.iter().map(|s| s.position.map(|p| if len > 0.0 { p.resolve(len) / len } else { 0.0 })).collect();
    if n == 0 {
        return Vec::new();
    }
    if pos[0].is_none() {
        pos[0] = Some(0.0);
    }
    if pos[n - 1].is_none() {
        pos[n - 1] = Some(1.0);
    }
    let mut last = pos[0].unwrap();
    for p in pos.iter_mut() {
        if let Some(v) = p {
            if *v < last {
                *v = last;
            }
            last = *v;
        }
    }
    let mut i = 0;
    while i < n {
        if pos[i].is_none() {
            let start = i - 1;
            let mut end = i;
            while pos[end].is_none() {
                end += 1;
            }
            let (a, b) = (pos[start].unwrap(), pos[end].unwrap());
            for (k, p) in pos.iter_mut().enumerate().take(end).skip(i) {
                *p = Some(a + (b - a) * (k - start) as f32 / (end - start) as f32);
            }
            i = end;
        }
        i += 1;
    }
    pos.into_iter().map(|p| p.unwrap_or(0.0)).collect()
}

/// Normalize stop fractions to 0..1, returning the stops and the start and
/// end fractions they now map to
fn normalized(stops: &[ColorStop], fr: &[f32], alpha: f32) -> (Vec<GradientStop>, f32, f32) {
    let (lo, hi) = (fr.first().copied().unwrap_or(0.0).min(0.0), fr.last().copied().unwrap_or(1.0).max(1.0));
    let span = (hi - lo).max(1e-6);
    let gs = stops.iter().zip(fr).map(|(s, &f)| GradientStop::new((f - lo) / span, skia_color(s.color, alpha))).collect();
    (gs, lo, hi)
}

fn gradient_shader(image: &Image, tile: Rect, alpha: f32) -> Option<Shader<'static>> {
    match image {
        Image::Linear { direction, stops, repeating } => {
            let (vx, vy) = match *direction {
                GradientDirection::Angle(deg) => {
                    let a = deg.to_radians();
                    (a.sin(), -a.cos())
                }
                GradientDirection::Corner(sx, sy) => {
                    let (x, y) = (sx as f32 * tile.h, sy as f32 * tile.w);
                    let l = (x * x + y * y).sqrt().max(1e-6);
                    (x / l, y / l)
                }
            };
            let len = (tile.w * vx).abs() + (tile.h * vy).abs();
            let (cx, cy) = (tile.x + tile.w / 2.0, tile.y + tile.h / 2.0);
            let fr = stop_fractions(stops, len);
            if *repeating {
                let (a, b) = (fr.first().copied()?, fr.last().copied()?);
                if b - a <= 1e-3 {
                    return None;
                }
                let gs = stops.iter().zip(&fr).map(|(s, &f)| GradientStop::new((f - a) / (b - a), skia_color(s.color, alpha))).collect();
                let p = |t: f32| Point::from_xy(cx + vx * len * (t - 0.5), cy + vy * len * (t - 0.5));
                return LinearGradient::new(p(a), p(b), gs, SpreadMode::Repeat, Transform::identity());
            }
            let (gs, lo, hi) = normalized(stops, &fr, alpha);
            let p = |t: f32| Point::from_xy(cx + vx * len * (t - 0.5), cy + vy * len * (t - 0.5));
            LinearGradient::new(p(lo), p(hi), gs, SpreadMode::Pad, Transform::identity())
        }
        Image::Radial { circle, size, center, stops, repeating } => {
            let (cx, cy) = (center.0.resolve(tile.w), center.1.resolve(tile.h));
            let (dl, dr, dt, db) = (cx.abs(), (tile.w - cx).abs(), cy.abs(), (tile.h - cy).abs());
            let (mut rx, mut ry) = match size {
                RadialSize::ClosestSide => (dl.min(dr), dt.min(db)),
                RadialSize::FarthestSide => (dl.max(dr), dt.max(db)),
                RadialSize::ClosestCorner => (dl.min(dr) * std::f32::consts::SQRT_2, dt.min(db) * std::f32::consts::SQRT_2),
                RadialSize::FarthestCorner => (dl.max(dr) * std::f32::consts::SQRT_2, dt.max(db) * std::f32::consts::SQRT_2),
                RadialSize::Explicit(a, b) => (a.resolve(tile.w), b.resolve(tile.h)),
            };
            if *circle {
                let r = match size {
                    RadialSize::ClosestSide => rx.min(ry),
                    RadialSize::FarthestSide => rx.max(ry),
                    RadialSize::ClosestCorner => (dl.min(dr).powi(2) + dt.min(db).powi(2)).sqrt(),
                    RadialSize::FarthestCorner => (dl.max(dr).powi(2) + dt.max(db).powi(2)).sqrt(),
                    RadialSize::Explicit(a, _) => a.resolve(tile.w),
                };
                rx = r;
                ry = r;
            }
            if rx <= 0.0 || ry <= 0.0 {
                return None;
            }
            let fr = stop_fractions(stops, rx);
            let (gs, mode, radius) = if *repeating {
                let (a, b) = (fr.first().copied()?, fr.last().copied()?);
                if b - a <= 1e-3 {
                    return None;
                }
                (stops.iter().zip(&fr).map(|(s, &f)| GradientStop::new(f / b, skia_color(s.color, alpha))).collect(), SpreadMode::Repeat, rx * b)
            } else {
                let hi = fr.last().copied().unwrap_or(1.0).max(1.0);
                (stops.iter().zip(&fr).map(|(s, &f)| GradientStop::new(f.max(0.0) / hi, skia_color(s.color, alpha))).collect(), SpreadMode::Pad, rx * hi)
            };
            let transform = Transform::from_translate(tile.x + cx, tile.y + cy).pre_scale(1.0, ry / rx);
            RadialGradient::new(Point::zero(), Point::zero(), radius, gs, mode, transform)
        }
        Image::None | Image::Url(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(c: CssColor, p: Option<f32>) -> ColorStop {
        ColorStop { color: c, position: p.map(Lp::pct) }
    }

    #[test]
    fn implied_stop_positions() {
        let s = [stop(CssColor::BLACK, None), stop(CssColor::WHITE, None), stop(CssColor::BLACK, Some(80.0)), stop(CssColor::WHITE, Some(20.0))];
        let f = stop_fractions(&s, 100.0);
        assert_eq!(f, vec![0.0, 0.4, 0.8, 0.8]);
    }

    #[test]
    fn radii_shrink_to_fit() {
        let r = [(10.0f32, 10.0f32), (10.0, 10.0), (0.0, 0.0), (0.0, 0.0)];
        let mut scaled = r;
        let w = 15.0;
        let f = w / 20.0;
        for c in &mut scaled[..2] {
            c.0 *= f;
            c.1 *= f;
        }
        assert_eq!(scaled[0].0, 7.5);
        assert!(is_axis_rect(&rounded_path(Rect::new(0.0, 0.0, 5.0, 5.0), [(0.0, 0.0); 4]).unwrap()));
        assert!(!is_axis_rect(&rounded_path(Rect::new(0.0, 0.0, 50.0, 50.0), r).unwrap()));
    }
}
