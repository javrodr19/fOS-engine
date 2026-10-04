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
    /// Loaded images (CSS backgrounds and masks), and the URL relative
    /// ones resolve against
    images: Option<&'a std::collections::HashMap<String, std::sync::Arc<crate::image_loader::LoadedImage>>>,
    base: &'a str,
    /// Canvas elements' bitmaps
    canvases: Option<&'a std::collections::HashMap<NodeId, std::sync::Arc<crate::image_loader::LoadedImage>>>,
    /// Scroll offsets of scroll containers
    box_scroll: Option<&'a std::collections::HashMap<NodeId, (f32, f32)>>,
    /// Horizontal shift from document to canvas (inside scrolled boxes)
    dx: f32,
    /// The visible part (top, height) of the scroll container being
    /// painted, which sticky boxes stick within
    view: (f32, f32),
    /// The box whose transform is being applied (painted plainly)
    untransformed: usize,
    /// The box being painted offscreen for its shape clip (painted plainly)
    shaped: usize,
}

/// Where painting is: origin, dx and view
type At = (f32, f32, (f32, f32));

/// A positioned box to paint as a layer: z-index, box, clip, opacity, and
/// the position state in effect where it sits
type Layer<'t> = (i32, &'t BoxFragment, Clip, f32, At);

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
        Painter { canvas, text, origin, mask: None, canvas_background_box, scroll, find_fixed: false, images: None, base: "", canvases: None, box_scroll: None, dx: 0.0, view: (scroll, 0.0), untransformed: 0, shaped: 0 }
    }

    /// Images for CSS `url()`s
    pub fn with_images(mut self, images: &'a crate::image_loader::Images, base: &'a str) -> Self {
        self.images = Some(&**images);
        self.base = base;
        self
    }

    /// Scroll positions of scroll containers
    pub fn with_box_scroll(mut self, offsets: &'a std::collections::HashMap<NodeId, (f32, f32)>) -> Self {
        self.box_scroll = Some(offsets);
        self
    }

    /// Bitmaps of canvas elements
    pub fn with_canvases(mut self, canvases: &'a std::collections::HashMap<NodeId, std::sync::Arc<crate::image_loader::LoadedImage>>) -> Self {
        self.canvases = Some(canvases);
        self
    }

    /// The page has fixed boxes
    pub fn with_fixed(mut self, has_fixed: bool) -> Self {
        self.find_fixed = has_fixed;
        self
    }

    pub fn paint(&mut self, tree: &FragmentTree) {
        let Some(root) = &tree.root else { return };
        self.view = (self.scroll, tree.viewport_height);
        let clip = Clip([0.0, 0.0, self.canvas.width() as f32, self.canvas.height() as f32]);
        self.paint_layer(root, clip, 1.0);
    }

    /// Document rectangle to device
    fn dev(&self, r: Rect) -> Rect {
        Rect::new(r.x + self.dx, r.y - self.origin, r.w, r.h)
    }

    /// Shift for the contents of `b` if it is a scrolled box; returns what
    /// to restore
    fn enter_scroll(&mut self, b: &BoxFragment) -> At {
        let saved = self.at();
        if b.scroll_extent.is_some() {
            let (ox, oy) = self.box_scroll.and_then(|m| m.get(&b.node)).copied().unwrap_or((0.0, 0.0));
            self.origin += oy;
            self.dx -= ox;
            let pad = b.padding_box();
            self.view = (pad.y + oy, pad.h);
        }
        saved
    }

    fn at(&self) -> At {
        (self.origin, self.dx, self.view)
    }

    fn go(&mut self, (origin, dx, view): At) {
        (self.origin, self.dx, self.view) = (origin, dx, view);
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

    /// Clip for a box and its contents: `clip` and `clip-path: inset()`
    fn own_clip(&self, b: &BoxFragment, clip: Clip) -> Clip {
        let r = b.border_box;
        match b.style.own_clip((r.x, r.y, r.w, r.h)) {
            Some((x, y, w, h)) => clip.intersect(self.dev(Rect::new(x, y, w, h))),
            None => clip,
        }
    }

    /// Clip for a box's contents
    fn inner_clip(&self, b: &BoxFragment, clip: Clip) -> Clip {
        if b.style.clips() {
            clip.intersect(self.dev(b.padding_box()))
        } else {
            clip
        }
    }

    /// A positioned or transformed box paints as a layer of its stacking
    /// context
    fn is_layer(b: &BoxFragment) -> bool {
        (b.style.is_positioned() || b.style.has_transform()) && b.kind != BoxFragmentKind::Placeholder
    }

    /// Paint `b` through its transform `m` (document coordinates): moved,
    /// when it only translates; otherwise drawn offscreen and mapped
    fn paint_transformed(&mut self, b: &BoxFragment, m: fos_css::transform::Matrix, clip: Clip, alpha: f32) {
        use fos_css::transform::multiply;
        let key = b as *const BoxFragment as usize;
        let prev = std::mem::replace(&mut self.untransformed, key);
        if m[..4] == [1.0, 0.0, 0.0, 1.0] {
            let saved = (self.origin, self.dx);
            self.origin -= m[5];
            self.dx += m[4];
            self.paint_layer(b, clip, alpha);
            (self.origin, self.dx) = saved;
            self.untransformed = prev;
            return;
        }
        // Device space: document x + dx, y - origin
        let to_dev = [1.0, 0.0, 0.0, 1.0, self.dx, -self.origin];
        let from_dev = [1.0, 0.0, 0.0, 1.0, -self.dx, self.origin];
        let d = multiply(to_dev, multiply(m, from_dev));
        let ink = self.dev(b.ink);
        let painted = self.dev(b.transformed_ink());
        let (w, h) = (ink.w.ceil(), ink.h.ceil());
        let off_screen = painted.bottom() < clip.0[1] || painted.y > clip.0[3] || painted.right() < clip.0[0] || painted.x > clip.0[2];
        // Huge or degenerate boxes are painted untransformed
        if off_screen || !(w >= 1.0 && h >= 1.0) || w * h > 16_000_000.0 || w > 8192.0 || h > 8192.0 {
            if !off_screen {
                self.paint_layer(b, clip, alpha);
            }
            self.untransformed = prev;
            return;
        }
        let Some(mut off) = Canvas::new(w as u32, h as u32) else {
            self.untransformed = prev;
            return;
        };
        {
            let mut p = Painter {
                canvas: &mut off,
                text: &mut *self.text,
                origin: b.ink.y,
                mask: None,
                canvas_background_box: self.canvas_background_box,
                scroll: self.scroll,
                find_fixed: self.find_fixed,
                images: self.images,
                base: self.base,
                canvases: self.canvases,
                box_scroll: self.box_scroll,
                dx: -b.ink.x,
                view: self.view,
                untransformed: key,
                shaped: self.shaped,
            };
            p.paint_layer(b, Clip([0.0, 0.0, w, h]), alpha);
        }
        let t = Transform::from_row(d[0], d[1], d[2], d[3], d[4], d[5]).pre_translate(ink.x, ink.y);
        let full = clip.contains(Rect::new(0.0, 0.0, self.canvas.width() as f32, self.canvas.height() as f32));
        let mask = if full { None } else { self.mask_for(clip).cloned() };
        let paint = tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() };
        if let Some(mut pm) = self.canvas.pixmap_mut() {
            pm.draw_pixmap(0, 0, off.pixmap(), &paint, t, mask.as_ref());
        }
        self.untransformed = prev;
    }

    /// Paint a box as a layer (CSS 2.1 Appendix E, simplified): its
    /// background and borders, positioned descendants with negative
    /// z-index, the in-flow content, then positioned descendants by
    /// z-index (tree order among equals)
    fn paint_layer(&mut self, b: &BoxFragment, clip: Clip, alpha: f32) {
        if b.style.box_.position == fos_css::style::Position::Fixed && self.scroll != 0.0 {
            // Laid out against the viewport at the top of the page: drawn
            // where the viewport is now
            let (saved, view) = (self.origin, self.view);
            self.origin -= self.scroll;
            self.view = (0.0, view.1);
            let full = Clip([0.0, 0.0, self.canvas.width() as f32, self.canvas.height() as f32]);
            self.scroll = 0.0;
            self.paint_layer(b, full, alpha);
            self.scroll = saved - self.origin;
            (self.origin, self.view) = (saved, view);
            return;
        }
        if self.untransformed != b as *const BoxFragment as usize {
            if let Some(m) = b.transform() {
                return self.paint_transformed(b, m, clip, alpha);
            }
        }
        let clip = self.own_clip(b, clip);
        if clip.is_empty() || self.culled(b, clip) {
            return;
        }
        if self.shaped != b as *const BoxFragment as usize {
            if let Some(shape) = self.shape_clip(b) {
                return self.paint_shaped(b, shape, clip, alpha, true);
            }
        }
        let alpha = alpha * b.style.box_.opacity;
        if alpha <= 0.0 {
            return;
        }
        self.paint_own(b, clip, alpha);
        let inner = self.inner_clip(b, clip);
        let saved = self.enter_scroll(b);
        let mut layers: Vec<Layer> = Vec::new();
        self.collect_layers(b, inner, alpha, &mut layers);
        layers.sort_by_key(|l| l.0);
        let here = self.at();
        for &(_, l, c, a, at) in layers.iter().filter(|l| l.0 < 0) {
            self.go(at);
            self.paint_layer(l, c, a);
        }
        self.go(here);
        self.paint_flow(b, inner, alpha);
        for &(_, l, c, a, at) in layers.iter().filter(|l| l.0 >= 0) {
            self.go(at);
            self.paint_layer(l, c, a);
        }
        self.go(saved);
        self.scroll_thumbs(b, clip);
        if b.style.inherited.visibility == Visibility::Visible {
            self.outline(b, clip, alpha);
        }
    }

    /// The positioned boxes under `b` that are not inside another layer
    fn collect_layers<'t>(&mut self, b: &'t BoxFragment, clip: Clip, alpha: f32, out: &mut Vec<Layer<'t>>) {
        for c in &b.children {
            let Fragment::Box(cb) = c else { continue };
            if Self::is_layer(cb) {
                // A sticky box is drawn moved to stay in view
                let mut at = self.at();
                at.0 -= cb.sticky_offset(b.content_box(), self.view);
                out.push((cb.style.box_.z_index.unwrap_or(0), cb, clip, alpha, at));
            } else if self.find_fixed || !self.culled(cb, clip) {
                let inner = self.inner_clip(cb, clip);
                let saved = self.enter_scroll(cb);
                self.collect_layers(cb, inner, alpha * cb.style.box_.opacity, out);
                self.go(saved);
            }
        }
    }

    /// Overlay scroll thumbs on a box whose content overflows it
    fn scroll_thumbs(&mut self, b: &BoxFragment, clip: Clip) {
        use fos_css::style::Overflow;
        let Some((ew, eh)) = b.scroll_extent else { return };
        let user = |o: Overflow| matches!(o, Overflow::Auto | Overflow::Scroll);
        let pad = self.dev(b.padding_box());
        let (ox, oy) = self.box_scroll.and_then(|m| m.get(&b.node)).copied().unwrap_or((0.0, 0.0));
        let color = tiny_skia::Color::from_rgba8(0, 0, 0, 90);
        let mut thumb = |r: Rect, painter: &mut Self| {
            if let Some(p) = rounded_path(r, [(r.w.min(r.h) / 2.0, r.w.min(r.h) / 2.0); 4]) {
                let mut paint = Paint::default();
                paint.set_color(color);
                paint.anti_alias = true;
                painter.fill(&p, &paint, FillRule::Winding, clip.intersect(pad));
            }
        };
        if user(b.style.box_.overflow_y) && eh > pad.h + 0.5 && pad.h > 16.0 {
            let len = (pad.h * pad.h / eh).max(16.0);
            let y = pad.y + (pad.h - len) * (oy / (eh - pad.h)).clamp(0.0, 1.0);
            thumb(Rect::new(pad.right() - 6.0, y + 2.0, 4.0, len - 4.0), self);
        }
        if user(b.style.box_.overflow_x) && ew > pad.w + 0.5 && pad.w > 16.0 {
            let len = (pad.w * pad.w / ew).max(16.0);
            let x = pad.x + (pad.w - len) * (ox / (ew - pad.w)).clamp(0.0, 1.0);
            thumb(Rect::new(x + 2.0, pad.bottom() - 6.0, len - 4.0, 4.0), self);
        }
    }

    /// Background, borders and replaced content of one box
    fn paint_own(&mut self, b: &BoxFragment, clip: Clip, alpha: f32) {
        if b.style.inherited.visibility != Visibility::Visible {
            return;
        }
        if let Some(url) = &b.style.background.mask {
            // Only what the mask lets through shows
            self.masked_background(b, url, clip, alpha);
            return;
        }
        let shadows = b.style.box_.box_shadow.is_some();
        if shadows {
            self.shadows(b, clip, alpha, false);
        }
        if b.kind == BoxFragmentKind::InlinePart || self.canvas_background_box != Some(b.node) {
            self.background(b, clip, alpha);
        } else {
            // The color went to the canvas; gradients still paint here
            self.background_images(b, clip, alpha);
        }
        if shadows {
            self.shadows(b, clip, alpha, true);
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
                    self.paint_in_flow(cb, clip, alpha);
                }
            }
        }
    }

    /// An in-flow (not layered) box and its contents
    fn paint_in_flow(&mut self, cb: &BoxFragment, clip: Clip, alpha: f32) {
        let a = alpha * cb.style.box_.opacity;
        let clip = self.own_clip(cb, clip);
        if a <= 0.0 || clip.is_empty() {
            return;
        }
        if self.shaped != cb as *const BoxFragment as usize {
            if let Some(shape) = self.shape_clip(cb) {
                return self.paint_shaped(cb, shape, clip, alpha, false);
            }
        }
        self.paint_own(cb, clip, a);
        let inner = self.inner_clip(cb, clip);
        let saved = self.enter_scroll(cb);
        self.paint_flow(cb, inner, a);
        self.go(saved);
        self.scroll_thumbs(cb, clip);
        if cb.style.inherited.visibility == Visibility::Visible {
            self.outline(cb, clip, a);
        }
    }

    /// The shape (device space) a box and its contents are cut to when it
    /// is not a rectangle: a `clip-path` circle, ellipse or polygon, or
    /// the rounded border box of a box that clips its overflow
    fn shape_clip(&self, b: &BoxFragment) -> Option<(Path, FillRule)> {
        use fos_css::style::ClipShape;
        let r = self.dev(b.border_box);
        if let Some(shape) = &b.style.box_.clip_shape {
            return match &**shape {
                ClipShape::Ellipse { .. } => {
                    let (cx, cy, rx, ry) = shape.ellipse_in(r.w, r.h)?;
                    // An empty shape hides everything
                    let oval = tiny_skia::Rect::from_xywh(r.x + cx - rx, r.y + cy - ry, (2.0 * rx).max(0.01), (2.0 * ry).max(0.01))?;
                    Some((PathBuilder::from_oval(oval)?, FillRule::Winding))
                }
                ClipShape::Polygon(points, evenodd) => {
                    let mut pb = PathBuilder::new();
                    for (i, (x, y)) in points.iter().enumerate() {
                        let (px, py) = (r.x + x.resolve(r.w), r.y + y.resolve(r.h));
                        if i == 0 {
                            pb.move_to(px, py);
                        } else {
                            pb.line_to(px, py);
                        }
                    }
                    pb.close();
                    Some((pb.finish()?, if *evenodd { FillRule::EvenOdd } else { FillRule::Winding }))
                }
            };
        }
        if b.style.clips() && b.kind != BoxFragmentKind::InlinePart {
            let rad = radii(b, r);
            if rad.iter().any(|c| c.0 > 0.0) {
                return Some((rounded_path(r, rad)?, FillRule::Winding));
            }
        }
        None
    }

    /// Paint `b` (a layer, or in flow) offscreen, then onto the canvas
    /// through its shape
    fn paint_shaped(&mut self, b: &BoxFragment, shape: (Path, FillRule), clip: Clip, alpha: f32, layer: bool) {
        let key = b as *const BoxFragment as usize;
        let sb = shape.0.bounds();
        let (x0, y0) = (sb.left().max(clip.0[0]).floor(), sb.top().max(clip.0[1]).floor());
        let (x1, y1) = (sb.right().min(clip.0[2]).ceil(), sb.bottom().min(clip.0[3]).ceil());
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let prev = std::mem::replace(&mut self.shaped, key);
        // Huge areas are painted unshaped
        let off = if w * h > 16_000_000.0 { None } else { Canvas::new(w as u32, h as u32) };
        let Some(mut off) = off else {
            if layer {
                self.paint_layer(b, clip, alpha);
            } else {
                self.paint_in_flow(b, clip, alpha);
            }
            self.shaped = prev;
            return;
        };
        {
            let mut p = Painter {
                canvas: &mut off,
                text: &mut *self.text,
                origin: self.origin + y0,
                mask: None,
                canvas_background_box: self.canvas_background_box,
                scroll: self.scroll,
                find_fixed: self.find_fixed,
                images: self.images,
                base: self.base,
                canvases: self.canvases,
                box_scroll: self.box_scroll,
                dx: self.dx - x0,
                view: self.view,
                untransformed: self.untransformed,
                shaped: key,
            };
            let c = Clip([0.0, 0.0, w, h]);
            if layer {
                p.paint_layer(b, c, alpha);
            } else {
                p.paint_in_flow(b, c, alpha);
            }
        }
        self.shaped = prev;
        let Some(mut mask) = Mask::new(self.canvas.width(), self.canvas.height()) else { return };
        mask.fill_path(&shape.0, shape.1, true, Transform::identity());
        if let Some(mut pm) = self.canvas.pixmap_mut() {
            pm.draw_pixmap(x0 as i32, y0 as i32, off.pixmap(), &tiny_skia::PixmapPaint::default(), Transform::identity(), Some(&mask));
        }
    }

    /// `box-shadow`s: outer ones (outside the border box, under the
    /// background) or inset ones (inside the padding box, above it); the
    /// first in the list is on top
    fn shadows(&mut self, b: &BoxFragment, clip: Clip, alpha: f32, inset: bool) {
        let Some(list) = b.style.box_.box_shadow.clone() else { return };
        let border = self.dev(b.border_box);
        let rad = radii(b, b.border_box);
        let (w, h) = (self.canvas.width(), self.canvas.height());
        let clip_path = |c: Clip| tiny_skia::Rect::from_ltrb(c.0[0], c.0[1], c.0[2], c.0[3]).map(PathBuilder::from_rect);
        for s in list.iter().rev().filter(|s| s.inset == inset) {
            let color = skia_color(s.color.unwrap_or(b.style.color()), alpha);
            if color.alpha() == 0.0 {
                continue;
            }
            let grow = |r: [(f32, f32); 4], d: f32| r.map(|(x, y)| if x > 0.0 && y > 0.0 { ((x + d).max(0.0), (y + d).max(0.0)) } else { (0.0, 0.0) });
            // Where the shadow may show
            let Some(mut area) = Mask::new(w, h) else { return };
            let (shape, shape_radii, area_rect) = if !inset {
                let Some(p) = rounded_path(border, rad) else { continue };
                area.fill_path(&p, FillRule::Winding, true, Transform::identity());
                area.invert();
                let shape = Rect::new(border.x + s.x - s.spread, border.y + s.y - s.spread, border.w + 2.0 * s.spread, border.h + 2.0 * s.spread);
                (shape, grow(rad, s.spread), shape)
            } else {
                let pad = self.dev(b.padding_box());
                let prad = inset_radii(rad, b.border);
                let Some(p) = rounded_path(pad, prad) else { continue };
                area.fill_path(&p, FillRule::Winding, true, Transform::identity());
                let hole = Rect::new(pad.x + s.x + s.spread, pad.y + s.y + s.spread, pad.w - 2.0 * s.spread, pad.h - 2.0 * s.spread);
                (hole, grow(prad, -s.spread), pad)
            };
            if let Some(cp) = clip_path(clip) {
                area.intersect_path(&cp, FillRule::Winding, false, Transform::identity());
            }
            // The region to compute: the shape (or, inset, the box) with
            // room for the blur, within the clip
            let margin = (s.blur * 1.5).ceil() + 1.0;
            let x0 = (area_rect.x - margin).max(clip.0[0]).max(0.0).floor();
            let y0 = (area_rect.y - margin).max(clip.0[1]).max(0.0).floor();
            let x1 = (area_rect.right() + margin).min(clip.0[2]).min(w as f32).ceil();
            let y1 = (area_rect.bottom() + margin).min(clip.0[3]).min(h as f32).ceil();
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let (rw, rh) = ((x1 - x0) as u32, (y1 - y0) as u32);
            let Some(mut cover) = Mask::new(rw, rh) else { continue };
            let local = Rect::new(shape.x - x0, shape.y - y0, shape.w, shape.h);
            if shape.w > 0.0 && shape.h > 0.0 {
                if let Some(p) = rounded_path(local, shape_radii) {
                    cover.fill_path(&p, FillRule::Winding, true, Transform::identity());
                }
            }
            if inset {
                // The shadow is everything outside the hole
                cover.invert();
            }
            if s.blur > 0.0 {
                box_blur(cover.data_mut(), rw as usize, rh as usize, s.blur / 2.0);
            }
            // The shadow color through its coverage, drawn where it may show
            let Some(mut layer) = tiny_skia::Pixmap::new(rw, rh) else { continue };
            let c = color.premultiply().to_color_u8();
            for (px, &a) in layer.pixels_mut().iter_mut().zip(cover.data()) {
                if a > 0 {
                    let f = |v: u8| ((v as u32 * a as u32 + 127) / 255) as u8;
                    *px = tiny_skia::PremultipliedColorU8::from_rgba(f(c.red()), f(c.green()), f(c.blue()), f(c.alpha())).unwrap_or(*px);
                }
            }
            if let Some(mut pm) = self.canvas.pixmap_mut() {
                pm.draw_pixmap(x0 as i32, y0 as i32, layer.as_ref(), &tiny_skia::PixmapPaint::default(), Transform::identity(), Some(&area));
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
    /// A loaded CSS image by its (possibly relative) URL
    fn css_image(&self, url: &str) -> Option<&'a crate::image_loader::LoadedImage> {
        let images = self.images?;
        images.get(url).or_else(|| images.get(&fos_net::url_util::resolve(self.base, url))).map(|i| &**i)
    }

    /// Background layers (gradients and images), bottom first
    fn background_images(&mut self, b: &BoxFragment, clip: Clip, alpha: f32) {
        let bg = &b.style.background;
        if bg.images.is_empty() {
            return;
        }
        let border_box = self.dev(b.border_box);
        let area = self.dev(b.padding_box());
        let rad = radii(b, border_box);
        let shape = rad.iter().any(|c| c.0 > 0.0).then(|| rounded_path(border_box, rad)).flatten();
        let n = bg.images.len();
        for i in (0..n).rev() {
            let image = &bg.images[i];
            let loaded = match image {
                Image::None => continue,
                Image::Url(u) => match self.css_image(u) {
                    Some(img) => Some(img),
                    None => continue,
                },
                _ => None,
            };
            let size = bg.size.get(i % bg.size.len().max(1)).copied().unwrap_or_default();
            let pos = bg.position.get(i % bg.position.len().max(1)).copied().unwrap_or((Lp::ZERO, Lp::ZERO));
            let repeat = bg.repeat.get(i % bg.repeat.len().max(1)).copied().unwrap_or((BackgroundRepeat::Repeat, BackgroundRepeat::Repeat));
            let (tw, th) = tile_size(size, area, loaded.map(|l| l.natural));
            if tw < 0.5 || th < 0.5 {
                continue;
            }
            let (xs, ys) = tile_positions(area, border_box, (tw, th), pos, repeat);
            let painting_clip = clip.intersect(border_box);
            for &ty in &ys {
                if ty > painting_clip.0[3] || ty + th < painting_clip.0[1] {
                    continue;
                }
                for &tx in &xs {
                    let tile = Rect::new(tx, ty, tw, th);
                    let shader = match loaded {
                        Some(img) => {
                            let pm = &img.pixmap;
                            let t = Transform::from_row(tw / pm.width() as f32, 0.0, 0.0, th / pm.height() as f32, tx, ty);
                            tiny_skia::Pattern::new(pm.as_ref(), SpreadMode::Pad, tiny_skia::FilterQuality::Bilinear, alpha, t)
                        }
                        None => match gradient_shader(image, tile, alpha) {
                            Some(s) => s,
                            None => continue,
                        },
                    };
                    let mut paint = Paint::default();
                    paint.shader = shader;
                    paint.anti_alias = true;
                    let tile_clip = painting_clip.intersect(tile);
                    match &shape {
                        Some(path) => self.fill(path, &paint, FillRule::Winding, tile_clip),
                        None => {
                            if let Some(path) = rounded_path(tile, [(0.0, 0.0); 4]) {
                                self.fill(&path, &paint, FillRule::Winding, tile_clip);
                            }
                        }
                    }
                }
            }
        }
    }

    /// A masked box's background color shaped by its mask image (icons);
    /// false when the mask is not loaded (the box is then invisible)
    fn masked_background(&mut self, b: &BoxFragment, url: &str, clip: Clip, alpha: f32) -> bool {
        let Some(img) = self.css_image(url) else { return false };
        let bg = &b.style.background;
        let color = if bg.color.a > 0 { bg.color } else { return true };
        let border_box = self.dev(b.border_box);
        let area = self.dev(b.padding_box());
        let (tw, th) = tile_size(bg.mask_size, area, Some(img.natural));
        if tw < 0.5 || th < 0.5 || tw > 4096.0 || th > 4096.0 {
            return true;
        }
        // The tile: the mask's alpha as coverage of the color
        let (pw, ph) = (tw.ceil() as u32, th.ceil() as u32);
        let Some(mut tile_pm) = tiny_skia::Pixmap::new(pw, ph) else { return true };
        let pm = &img.pixmap;
        let scale = Transform::from_scale(tw / pm.width() as f32, th / pm.height() as f32);
        tile_pm.draw_pixmap(0, 0, pm.as_ref(), &tiny_skia::PixmapPaint { quality: tiny_skia::FilterQuality::Bilinear, ..Default::default() }, scale, None);
        let c = skia_color(color, alpha).premultiply().to_color_u8();
        for px in tile_pm.pixels_mut() {
            let a = px.alpha() as u32;
            let f = |v: u8| ((v as u32 * a + 127) / 255) as u8;
            *px = tiny_skia::PremultipliedColorU8::from_rgba(f(c.red()), f(c.green()), f(c.blue()), f(c.alpha())).unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
        }
        let (xs, ys) = tile_positions(area, border_box, (tw, th), bg.mask_position, bg.mask_repeat);
        let painting_clip = clip.intersect(border_box);
        for &ty in &ys {
            for &tx in &xs {
                let tile = Rect::new(tx, ty, tw, th);
                let vis = painting_clip.intersect(tile);
                if vis.is_empty() {
                    continue;
                }
                let t = Transform::from_row(tw / pw as f32, 0.0, 0.0, th / ph as f32, tx, ty);
                let mut paint = Paint::default();
                paint.shader = tiny_skia::Pattern::new(tile_pm.as_ref(), SpreadMode::Pad, tiny_skia::FilterQuality::Bilinear, 1.0, t);
                if let (Some(rect), Some(mut canvas)) = (tiny_skia::Rect::from_ltrb(vis.0[0], vis.0[1], vis.0[2], vis.0[3]), self.canvas.pixmap_mut()) {
                    canvas.fill_rect(rect, &paint, Transform::identity(), None);
                }
            }
        }
        true
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
            ReplacedPaint::Image(handle) => {
                if let Some(img) = handle.0.downcast_ref::<crate::image_loader::LoadedImage>() {
                    self.image(b, img, c, clip, alpha);
                }
            }
            ReplacedPaint::Bitmap => {
                if let Some(img) = self.canvases.and_then(|c| c.get(&b.node)).cloned() {
                    self.image(b, &img, c, clip, alpha);
                }
            }
            ReplacedPaint::Empty => {}
        }
    }

    /// An image in a content box `c` (device space), fitted per
    /// `object-fit` and placed per `object-position`
    fn image(&mut self, b: &BoxFragment, img: &crate::image_loader::LoadedImage, c: Rect, clip: Clip, alpha: f32) {
        use fos_css::style::ObjectFit;
        let (nw, nh) = img.natural;
        if nw <= 0.0 || nh <= 0.0 || c.w <= 0.0 || c.h <= 0.0 {
            return;
        }
        let (sx, sy) = (c.w / nw, c.h / nh);
        let (dw, dh) = match b.style.box_.object_fit {
            ObjectFit::Fill => (c.w, c.h),
            ObjectFit::Contain => {
                let s = sx.min(sy);
                (nw * s, nh * s)
            }
            ObjectFit::Cover => {
                let s = sx.max(sy);
                (nw * s, nh * s)
            }
            ObjectFit::None => (nw, nh),
            ObjectFit::ScaleDown => {
                let s = sx.min(sy).min(1.0);
                (nw * s, nh * s)
            }
        };
        // object-position (50% 50% by default) places it in the box
        let (px, py) = b.style.box_.object_position;
        let dest = Rect::new(c.x + px.resolve(c.w - dw), c.y + py.resolve(c.h - dh), dw, dh);
        let visible = clip.intersect(c).intersect(dest);
        if visible.is_empty() {
            return;
        }
        let pm = &img.pixmap;
        let transform = Transform::from_row(dw / pm.width() as f32, 0.0, 0.0, dh / pm.height() as f32, dest.x, dest.y);
        let scale = (dw / pm.width() as f32).min(dh / pm.height() as f32);
        // Smooth when scaling, exact at 1:1
        let quality = if (scale - 1.0).abs() < 0.01 { tiny_skia::FilterQuality::Nearest } else { tiny_skia::FilterQuality::Bilinear };
        let shader = tiny_skia::Pattern::new(pm.as_ref(), SpreadMode::Pad, quality, alpha, transform);
        let mut paint = Paint::default();
        paint.shader = shader;
        // Rounded corners cut the image when it fills its box
        let border = self.dev(b.border_box);
        let rad = radii(b, border);
        let covers = dest.x <= c.x + 0.5 && dest.y <= c.y + 0.5 && dest.right() >= c.right() - 0.5 && dest.bottom() >= c.bottom() - 0.5;
        if covers && rad.iter().any(|r| r.0 > 0.0) {
            let inner = inset_radii(rad, [c.y - border.y, border.right() - c.right(), border.bottom() - c.bottom(), c.x - border.x]);
            if let Some(path) = rounded_path(c, inner) {
                self.fill(&path, &paint, FillRule::Winding, clip);
                return;
            }
        }
        if let (Some(rect), Some(mut canvas)) = (tiny_skia::Rect::from_ltrb(visible.0[0], visible.0[1], visible.0[2], visible.0[3]), self.canvas.pixmap_mut()) {
            canvas.fill_rect(rect, &paint, Transform::identity(), None);
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
            let x = *x + self.dx;
            if x + word.width < clip.0[0] || x > clip.0[2] {
                continue;
            }
            if word.fallback.is_empty() {
                self.text.draw_glyph_run(self.canvas, font, word.size, t.font.synthesis, color, word.glyphs.iter().map(|g| (g.id, x + g.x, baseline - g.y)), pixel_clip);
            } else {
                // Characters drawn from fallback faces, glyph by glyph
                for (face, g) in word.glyph_fonts() {
                    if let Some(face) = face {
                        self.text.draw_glyph_run(self.canvas, face, word.size, t.font.synthesis, color, std::iter::once((g.id, x + g.x, baseline - g.y)), pixel_clip);
                    }
                }
            }
        }
        if t.decoration != 0 {
            let size = t.font.size;
            let thickness = (size / 14.0).max(1.0);
            let dc = skia_color(t.decoration_color, alpha);
            let line = |y: f32, painter: &mut Self| painter.fill_rect(Rect::new(r.x, y, r.w, thickness), dc, clip);
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
/// Approximate a Gaussian blur of standard deviation `sigma` on an alpha
/// buffer with three box blurs each way
fn box_blur(data: &mut [u8], w: usize, h: usize, sigma: f32) {
    // Box widths for three passes (W3C filter effects' approximation)
    let d = ((sigma * 3.0 * (2.0 * std::f32::consts::PI).sqrt() / 4.0) + 0.5).floor().max(1.0) as usize;
    let r = d / 2;
    let mut tmp = vec![0u8; data.len()];
    for _ in 0..3 {
        blur_pass(data, &mut tmp, w, h, r, true);
        blur_pass(&tmp, data, w, h, r, false);
    }
}

/// One box blur of radius `r`, along rows (`horizontal`) or columns
fn blur_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, r: usize, horizontal: bool) {
    let (lines, len, step, stride) = if horizontal { (h, w, 1, w) } else { (w, h, w, 1) };
    let div = (2 * r + 1) as u32;
    for line in 0..lines {
        let base = line * stride;
        let at = |i: isize| -> u32 {
            if i < 0 || i as usize >= len {
                0
            } else {
                src[base + i as usize * step] as u32
            }
        };
        let mut sum: u32 = (-(r as isize)..=r as isize).map(at).sum();
        for i in 0..len {
            dst[base + i * step] = ((sum + div / 2) / div) as u8;
            sum += at(i as isize + r as isize + 1);
            sum -= at(i as isize - r as isize);
        }
    }
}

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

/// A background layer's tile size in a positioning `area`: images of
/// natural size `natural` keep their ratio; gradients fill the area
fn tile_size(size: BackgroundSize, area: Rect, natural: Option<(f32, f32)>) -> (f32, f32) {
    match natural {
        None => match size {
            BackgroundSize::Auto | BackgroundSize::Cover | BackgroundSize::Contain => (area.w, area.h),
            BackgroundSize::Explicit(w, h) => (w.resolve(area.w).unwrap_or(area.w), h.resolve(area.h).unwrap_or(area.h)),
        },
        Some((nw, nh)) if nw > 0.0 && nh > 0.0 => match size {
            BackgroundSize::Auto => (nw, nh),
            BackgroundSize::Cover => {
                let s = (area.w / nw).max(area.h / nh);
                (nw * s, nh * s)
            }
            BackgroundSize::Contain => {
                let s = (area.w / nw).min(area.h / nh);
                (nw * s, nh * s)
            }
            BackgroundSize::Explicit(w, h) => match (w.resolve(area.w), h.resolve(area.h)) {
                (Some(w), Some(h)) => (w, h),
                (Some(w), None) => (w, w * nh / nw),
                (None, Some(h)) => (h * nw / nh, h),
                (None, None) => (nw, nh),
            },
        },
        Some(_) => (0.0, 0.0),
    }
}

/// Where a layer's tiles go: x and y positions covering `paint_area`
fn tile_positions(area: Rect, paint_area: Rect, (tw, th): (f32, f32), pos: (Lp, Lp), repeat: (BackgroundRepeat, BackgroundRepeat)) -> (Vec<f32>, Vec<f32>) {
    let x0 = area.x + pos.0.resolve(area.w - tw);
    let y0 = area.y + pos.1.resolve(area.h - th);
    let tiles = |start: f32, len: f32, lo: f32, hi: f32, rep: bool| -> Vec<f32> {
        if !rep || len <= 0.0 {
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
    (tiles(x0, tw, paint_area.x, paint_area.right(), rx), tiles(y0, th, paint_area.y, paint_area.bottom(), ry))
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
