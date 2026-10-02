//! The HTML canvas 2D context, drawn with tiny-skia
//!
//! `Canvas2D` holds a premultiplied RGBA bitmap (allocated on the first
//! drawing operation, so canvases used only to measure text cost nothing),
//! the drawing state and its stack, and the current default path. Paths
//! are kept in device space: the HTML standard applies the current
//! transform as each point is added. Styles, compositing, shadows and
//! clipping follow the standard; text comes from `text2d`.

use std::cell::RefCell;
use std::f32::consts::{PI, TAU};
use std::rc::Rc;

use tiny_skia as sk;

use crate::text2d::{self, FontSpec, TextMetrics2D};

/// The largest canvas side and area we allocate (as browsers cap them)
const MAX_SIDE: u32 = 32_767;
const MAX_AREA: u64 = 268_435_456;

/// A fill or stroke style
#[derive(Clone)]
pub enum Style {
    Color(sk::Color),
    Gradient(Rc<RefCell<Gradient>>),
    Pattern(Rc<CanvasPattern>),
}

#[derive(Clone, Debug)]
pub enum GradientKind {
    Linear { x0: f32, y0: f32, x1: f32, y1: f32 },
    Radial { x0: f32, y0: f32, r0: f32, x1: f32, y1: f32, r1: f32 },
    Conic { angle: f32, x: f32, y: f32 },
}

/// A CanvasGradient: its geometry and color stops (in insertion order,
/// sorted stably by offset when used)
#[derive(Clone, Debug)]
pub struct Gradient {
    pub kind: GradientKind,
    pub stops: Vec<(f32, sk::Color)>,
}

impl Gradient {
    pub fn new(kind: GradientKind) -> Self {
        Gradient { kind, stops: Vec::new() }
    }

    pub fn add_color_stop(&mut self, offset: f32, color: sk::Color) {
        // Stops at equal offsets keep their order (a hard edge)
        let i = self.stops.partition_point(|s| s.0 <= offset);
        self.stops.insert(i, (offset, color));
    }

    /// Color at `t` in 0..=1, interpolating in unpremultiplied space
    fn color_at(&self, t: f32) -> sk::Color {
        let s = &self.stops;
        if s.is_empty() {
            return sk::Color::TRANSPARENT;
        }
        if t <= s[0].0 {
            return s[0].1;
        }
        for w in s.windows(2) {
            let ((a, ca), (b, cb)) = (w[0], w[1]);
            if t <= b {
                let f = if b > a { (t - a) / (b - a) } else { 1.0 };
                let mix = |x: f32, y: f32| x + (y - x) * f;
                return sk::Color::from_rgba(mix(ca.red(), cb.red()), mix(ca.green(), cb.green()), mix(ca.blue(), cb.blue()), mix(ca.alpha(), cb.alpha()))
                    .unwrap_or(ca);
            }
        }
        s[s.len() - 1].1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repetition {
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
}

/// A CanvasPattern: a copy of its source image
pub struct CanvasPattern {
    pub pixmap: sk::Pixmap,
    pub repetition: Repetition,
    pub transform: RefCell<sk::Transform>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Start,
    End,
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextBaseline {
    Top,
    Hanging,
    Middle,
    Alphabetic,
    Ideographic,
    Bottom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Ltr,
    Rtl,
    /// The canvas element's direction (left-to-right here)
    Inherit,
}

/// `imageSmoothingQuality`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SmoothingQuality {
    Low,
    Medium,
    High,
}

impl SmoothingQuality {
    fn filter(self) -> sk::FilterQuality {
        match self {
            SmoothingQuality::Low => sk::FilterQuality::Bilinear,
            SmoothingQuality::Medium | SmoothingQuality::High => sk::FilterQuality::Bicubic,
        }
    }
}

/// globalCompositeOperation values
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Composite(pub sk::BlendMode);

impl Composite {
    pub fn parse(s: &str) -> Option<Composite> {
        use sk::BlendMode::*;
        Some(Composite(match s {
            "source-over" => SourceOver,
            "source-in" => SourceIn,
            "source-out" => SourceOut,
            "source-atop" => SourceAtop,
            "destination-over" => DestinationOver,
            "destination-in" => DestinationIn,
            "destination-out" => DestinationOut,
            "destination-atop" => DestinationAtop,
            "lighter" => Plus,
            "copy" => Source,
            "xor" => Xor,
            "multiply" => Multiply,
            "screen" => Screen,
            "overlay" => Overlay,
            "darken" => Darken,
            "lighten" => Lighten,
            "color-dodge" => ColorDodge,
            "color-burn" => ColorBurn,
            "hard-light" => HardLight,
            "soft-light" => SoftLight,
            "difference" => Difference,
            "exclusion" => Exclusion,
            "hue" => Hue,
            "saturation" => Saturation,
            "color" => Color,
            "luminosity" => Luminosity,
            _ => return None,
        }))
    }

    pub fn name(self) -> &'static str {
        use sk::BlendMode::*;
        match self.0 {
            SourceIn => "source-in",
            SourceOut => "source-out",
            SourceAtop => "source-atop",
            DestinationOver => "destination-over",
            DestinationIn => "destination-in",
            DestinationOut => "destination-out",
            DestinationAtop => "destination-atop",
            Plus => "lighter",
            Source => "copy",
            Xor => "xor",
            Multiply => "multiply",
            Screen => "screen",
            Overlay => "overlay",
            Darken => "darken",
            Lighten => "lighten",
            ColorDodge => "color-dodge",
            ColorBurn => "color-burn",
            HardLight => "hard-light",
            SoftLight => "soft-light",
            Difference => "difference",
            Exclusion => "exclusion",
            Hue => "hue",
            Saturation => "saturation",
            Color => "color",
            Luminosity => "luminosity",
            _ => "source-over",
        }
    }

    /// Operations that also change pixels outside the shape (the shape is
    /// composited as a whole layer)
    fn unbounded(self) -> bool {
        matches!(self.0, sk::BlendMode::SourceIn | sk::BlendMode::SourceOut | sk::BlendMode::DestinationIn | sk::BlendMode::DestinationAtop | sk::BlendMode::Source)
    }
}

/// The drawing state (`save()` / `restore()`)
#[derive(Clone)]
pub struct State {
    pub transform: sk::Transform,
    pub fill: Style,
    pub stroke: Style,
    pub line_width: f32,
    pub line_cap: sk::LineCap,
    pub line_join: sk::LineJoin,
    pub miter_limit: f32,
    pub dash: Vec<f32>,
    pub dash_offset: f32,
    pub global_alpha: f32,
    pub composite: Composite,
    pub shadow_offset: (f32, f32),
    pub shadow_blur: f32,
    pub shadow_color: sk::Color,
    pub font: FontSpec,
    pub text_align: TextAlign,
    pub text_baseline: TextBaseline,
    pub direction: Direction,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    /// `fontKerning`, `fontStretch`, `fontVariantCaps`, `textRendering`
    /// (kept for scripts; shaping uses the font's defaults)
    pub font_kerning: &'static str,
    pub font_stretch: &'static str,
    pub font_variant_caps: &'static str,
    pub text_rendering: &'static str,
    /// `filter` as set (CSS filters are not applied yet)
    pub filter: Rc<str>,
    pub image_smoothing: bool,
    pub smoothing_quality: SmoothingQuality,
    pub clip: Option<Rc<sk::Mask>>,
}

impl Default for State {
    fn default() -> Self {
        State {
            transform: sk::Transform::identity(),
            fill: Style::Color(sk::Color::BLACK),
            stroke: Style::Color(sk::Color::BLACK),
            line_width: 1.0,
            line_cap: sk::LineCap::Butt,
            line_join: sk::LineJoin::Miter,
            miter_limit: 10.0,
            dash: Vec::new(),
            dash_offset: 0.0,
            global_alpha: 1.0,
            composite: Composite(sk::BlendMode::SourceOver),
            shadow_offset: (0.0, 0.0),
            shadow_blur: 0.0,
            shadow_color: sk::Color::TRANSPARENT,
            font: FontSpec::default(),
            text_align: TextAlign::Start,
            text_baseline: TextBaseline::Alphabetic,
            direction: Direction::Inherit,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            font_kerning: "auto",
            font_stretch: "normal",
            font_variant_caps: "normal",
            text_rendering: "auto",
            filter: Rc::from("none"),
            image_smoothing: true,
            smoothing_quality: SmoothingQuality::Low,
            clip: None,
        }
    }
}

/// A path being built, in device space
#[derive(Clone, Default)]
pub struct PathData {
    builder: sk::PathBuilder,
    /// Current point (device space)
    current: Option<(f32, f32)>,
    /// Start of the current subpath
    start: (f32, f32),
}

/// A recorded path command (Path2D keeps its commands in its own space)
#[derive(Clone, Debug)]
pub enum PathCmd {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CubicTo(f32, f32, f32, f32, f32, f32),
    ArcTo(f32, f32, f32, f32, f32),
    Ellipse { x: f32, y: f32, rx: f32, ry: f32, rotation: f32, start: f32, end: f32, ccw: bool },
    Rect(f32, f32, f32, f32),
    RoundRect(f32, f32, f32, f32, [(f32, f32); 4]),
    Close,
    /// Another path's commands under a transform (`addPath`)
    Path(Rc<Vec<PathCmd>>, sk::Transform),
}

fn map(t: &sk::Transform, x: f32, y: f32) -> (f32, f32) {
    (t.sx * x + t.kx * y + t.tx, t.ky * x + t.sy * y + t.ty)
}

impl PathData {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.builder.is_empty()
    }

    fn move_to_device(&mut self, (x, y): (f32, f32)) {
        self.builder.move_to(x, y);
        self.current = Some((x, y));
        self.start = (x, y);
    }

    fn line_to_device(&mut self, p: (f32, f32)) {
        if self.current.is_none() {
            self.move_to_device(p);
        }
        self.builder.line_to(p.0, p.1);
        self.current = Some(p);
    }

    /// Apply one command, mapping its points through `t`
    pub fn apply(&mut self, cmd: &PathCmd, t: &sk::Transform) {
        let finite = |v: &[f32]| v.iter().all(|x| x.is_finite());
        match *cmd {
            PathCmd::MoveTo(x, y) if finite(&[x, y]) => self.move_to_device(map(t, x, y)),
            PathCmd::LineTo(x, y) if finite(&[x, y]) => self.line_to_device(map(t, x, y)),
            PathCmd::QuadTo(cx, cy, x, y) if finite(&[cx, cy, x, y]) => {
                if self.current.is_none() {
                    self.move_to_device(map(t, cx, cy));
                }
                let (c, p) = (map(t, cx, cy), map(t, x, y));
                self.builder.quad_to(c.0, c.1, p.0, p.1);
                self.current = Some(p);
            }
            PathCmd::CubicTo(c1x, c1y, c2x, c2y, x, y) if finite(&[c1x, c1y, c2x, c2y, x, y]) => {
                if self.current.is_none() {
                    self.move_to_device(map(t, c1x, c1y));
                }
                let (a, b, p) = (map(t, c1x, c1y), map(t, c2x, c2y), map(t, x, y));
                self.builder.cubic_to(a.0, a.1, b.0, b.1, p.0, p.1);
                self.current = Some(p);
            }
            PathCmd::ArcTo(x1, y1, x2, y2, r) if finite(&[x1, y1, x2, y2, r]) => self.arc_to(t, x1, y1, x2, y2, r),
            PathCmd::Ellipse { x, y, rx, ry, rotation, start, end, ccw } if finite(&[x, y, rx, ry, rotation, start, end]) => {
                self.ellipse(t, x, y, rx, ry, rotation, start, end, ccw)
            }
            PathCmd::Rect(x, y, w, h) if finite(&[x, y, w, h]) => {
                self.move_to_device(map(t, x, y));
                self.line_to_device(map(t, x + w, y));
                self.line_to_device(map(t, x + w, y + h));
                self.line_to_device(map(t, x, y + h));
                self.close();
                self.move_to_device(map(t, x, y));
            }
            PathCmd::RoundRect(x, y, w, h, radii) if finite(&[x, y, w, h]) => self.round_rect(t, x, y, w, h, radii),
            PathCmd::Close => self.close(),
            PathCmd::Path(ref cmds, ref sub) => {
                let combined = t.pre_concat(*sub);
                for c in cmds.iter() {
                    self.apply(c, &combined);
                }
            }
            _ => {}
        }
    }

    pub fn close(&mut self) {
        if self.current.is_some() {
            self.builder.close();
            self.current = Some(self.start);
        }
    }

    /// Arc as cubic Béziers (each spanning at most a quarter turn)
    #[allow(clippy::too_many_arguments)]
    fn ellipse(&mut self, t: &sk::Transform, cx: f32, cy: f32, rx: f32, ry: f32, rotation: f32, start: f32, end: f32, ccw: bool) {
        if rx < 0.0 || ry < 0.0 {
            return;
        }
        let mut sweep = end - start;
        if !ccw && sweep >= TAU || ccw && -sweep >= TAU {
            sweep = if ccw { -TAU } else { TAU };
        } else {
            sweep %= TAU;
            if !ccw && sweep < 0.0 {
                sweep += TAU;
            } else if ccw && sweep > 0.0 {
                sweep -= TAU;
            }
        }
        let (sr, cr) = rotation.sin_cos();
        let point = |a: f32| {
            let (s, c) = a.sin_cos();
            let (px, py) = (rx * c, ry * s);
            map(t, cx + px * cr - py * sr, cy + px * sr + py * cr)
        };
        let first = point(start);
        if self.current.is_some() {
            self.line_to_device(first);
        } else {
            self.move_to_device(first);
        }
        let segments = ((sweep.abs() / (PI / 2.0)).ceil() as usize).max(1);
        let step = sweep / segments as f32;
        let k = 4.0 / 3.0 * (step / 4.0).tan();
        let mut a = start;
        for _ in 0..segments {
            let b = a + step;
            let (sa, ca) = a.sin_cos();
            let (sb, cb) = b.sin_cos();
            // Control points of the unit-circle arc, scaled and rotated
            let ctrl = |px: f32, py: f32| map(t, cx + (rx * px) * cr - (ry * py) * sr, cy + (rx * px) * sr + (ry * py) * cr);
            let c1 = ctrl(ca - k * sa, sa + k * ca);
            let c2 = ctrl(cb + k * sb, sb - k * cb);
            let p = ctrl(cb, sb);
            self.builder.cubic_to(c1.0, c1.1, c2.0, c2.1, p.0, p.1);
            self.current = Some(p);
            a = b;
        }
    }

    fn arc_to(&mut self, t: &sk::Transform, x1: f32, y1: f32, x2: f32, y2: f32, r: f32) {
        if r < 0.0 {
            return;
        }
        let Some(cur) = self.current else {
            self.move_to_device(map(t, x1, y1));
            return;
        };
        // The current point in user space
        let Some(inv) = t.invert() else { return };
        let (x0, y0) = map(&inv, cur.0, cur.1);
        let (dx0, dy0, dx2, dy2) = (x0 - x1, y0 - y1, x2 - x1, y2 - y1);
        let cross = dx0 * dy2 - dy0 * dx2;
        if (x0 == x1 && y0 == y1) || (x1 == x2 && y1 == y2) || r == 0.0 || cross.abs() < 1e-6 {
            self.line_to_device(map(t, x1, y1));
            return;
        }
        let (l0, l2) = ((dx0 * dx0 + dy0 * dy0).sqrt(), (dx2 * dx2 + dy2 * dy2).sqrt());
        let cos = (dx0 * dx2 + dy0 * dy2) / (l0 * l2);
        let angle = cos.clamp(-1.0, 1.0).acos();
        let dist = r / (angle / 2.0).tan();
        let (t0x, t0y) = (x1 + dx0 / l0 * dist, y1 + dy0 / l0 * dist);
        let (t2x, t2y) = (x1 + dx2 / l2 * dist, y1 + dy2 / l2 * dist);
        // Center: perpendicular from the first tangent point
        let ccw = cross > 0.0;
        let (nx, ny) = if ccw { (dy0 / l0, -dx0 / l0) } else { (-dy0 / l0, dx0 / l0) };
        let (cx, cy) = (t0x + nx * r, t0y + ny * r);
        let a0 = (t0y - cy).atan2(t0x - cx);
        let a2 = (t2y - cy).atan2(t2x - cx);
        self.line_to_device(map(t, t0x, t0y));
        self.ellipse(t, cx, cy, r, r, 0.0, a0, a2, ccw);
    }

    fn round_rect(&mut self, t: &sk::Transform, x: f32, y: f32, w: f32, h: f32, radii: [(f32, f32); 4]) {
        // Normalize negative sizes: flip the rectangle and the corners
        let (mut x, mut y, mut w, mut h, mut r) = (x, y, w, h, radii);
        if w < 0.0 {
            x += w;
            w = -w;
            r = [r[1], r[0], r[3], r[2]];
        }
        if h < 0.0 {
            y += h;
            h = -h;
            r = [r[3], r[2], r[1], r[0]];
        }
        // Scale radii down so adjacent corners fit
        let sums = [r[0].0 + r[1].0, r[3].0 + r[2].0, r[0].1 + r[3].1, r[1].1 + r[2].1];
        let lengths = [w, w, h, h];
        let mut scale = 1.0f32;
        for (s, l) in sums.iter().zip(lengths) {
            if *s > l && *s > 0.0 {
                scale = scale.min(l / s);
            }
        }
        let r = r.map(|(a, b)| (a * scale, b * scale));
        let [tl, tr, br, bl] = r;
        self.move_to_device(map(t, x + tl.0, y));
        self.line_to_device(map(t, x + w - tr.0, y));
        self.ellipse(t, x + w - tr.0, y + tr.1, tr.0, tr.1, 0.0, -PI / 2.0, 0.0, false);
        self.line_to_device(map(t, x + w, y + h - br.1));
        self.ellipse(t, x + w - br.0, y + h - br.1, br.0, br.1, 0.0, 0.0, PI / 2.0, false);
        self.line_to_device(map(t, x + bl.0, y + h));
        self.ellipse(t, x + bl.0, y + h - bl.1, bl.0, bl.1, 0.0, PI / 2.0, PI, false);
        self.line_to_device(map(t, x, y + tl.1));
        self.ellipse(t, x + tl.0, y + tl.1, tl.0, tl.1, 0.0, PI, PI * 1.5, false);
        self.close();
        self.move_to_device(map(t, x, y));
    }

    /// The path built so far (device space)
    pub fn to_path(&self) -> Option<sk::Path> {
        self.builder.clone().finish()
    }
}

/// Build a device-space path from recorded commands
pub fn build_path(cmds: &[PathCmd], t: &sk::Transform) -> Option<sk::Path> {
    let mut p = PathData::new();
    for c in cmds {
        p.apply(c, t);
    }
    p.to_path()
}

/// What is being drawn: a filled or stroked path, or an image
enum Draw<'a> {
    Fill(&'a sk::Path, sk::FillRule),
    Stroke(&'a sk::Path),
}

pub struct Canvas2D {
    width: u32,
    height: u32,
    /// Allocated on first draw
    pixmap: Option<sk::Pixmap>,
    pub state: State,
    stack: Vec<State>,
    pub path: PathData,
    /// Whether pixels changed since the last `take_dirty`
    dirty: bool,
    /// Opaque (`alpha: false`): starts and clears to black
    opaque: bool,
}

impl Canvas2D {
    pub fn new(width: u32, height: u32) -> Self {
        Canvas2D { width, height, pixmap: None, state: State::default(), stack: Vec::new(), path: PathData::new(), dirty: false, opaque: false }
    }

    /// A canvas without alpha (`getContext('2d', { alpha: false })`)
    pub fn new_opaque(width: u32, height: u32) -> Self {
        Canvas2D { opaque: true, ..Canvas2D::new(width, height) }
    }

    pub fn is_opaque(&self) -> bool {
        self.opaque
    }

    fn blank(&self) -> Option<sk::Pixmap> {
        let mut p = sk::Pixmap::new(self.width.max(1), self.height.max(1))?;
        if self.opaque {
            p.fill(sk::Color::BLACK);
        }
        Some(p)
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Setting `width` or `height` clears the bitmap and resets the state
    pub fn resize(&mut self, width: u32, height: u32) {
        *self = Canvas2D { opaque: self.opaque, ..Canvas2D::new(width, height) };
        self.dirty = true;
    }

    /// The bitmap, if anything was drawn (premultiplied RGBA)
    pub fn pixmap(&self) -> Option<&sk::Pixmap> {
        self.pixmap.as_ref()
    }

    /// Whether the bitmap changed since the last call
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    fn surface(&mut self) -> Option<&mut sk::Pixmap> {
        if self.pixmap.is_none() {
            if self.width == 0 || self.height == 0 || self.width > MAX_SIDE || self.height > MAX_SIDE || self.width as u64 * self.height as u64 > MAX_AREA {
                return None;
            }
            self.pixmap = sk::Pixmap::new(self.width, self.height);
            if self.opaque {
                if let Some(p) = &mut self.pixmap {
                    p.fill(sk::Color::BLACK);
                }
            }
        }
        self.dirty = true;
        self.pixmap.as_mut()
    }

    // ---- state ----

    pub fn save(&mut self) {
        // Browsers cap the stack; a runaway save() loop must not exhaust memory
        if self.stack.len() < 1024 {
            self.stack.push(self.state.clone());
        }
    }

    pub fn restore(&mut self) {
        if let Some(s) = self.stack.pop() {
            self.state = s;
        }
    }

    /// A new transparent bitmap, keeping the state (`transferToImageBitmap`)
    pub fn clear_bitmap(&mut self) {
        self.pixmap = None;
        self.dirty = true;
    }

    /// `reset()`: clear the bitmap, the path and the state
    pub fn reset(&mut self) {
        let (w, h) = (self.width, self.height);
        *self = Canvas2D { opaque: self.opaque, ..Canvas2D::new(w, h) };
        self.dirty = true;
    }

    // ---- transforms ----

    pub fn set_transform(&mut self, t: sk::Transform) {
        self.state.transform = t;
    }

    pub fn transform(&mut self, t: sk::Transform) {
        let all = [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty];
        if all.iter().all(|v| v.is_finite()) {
            self.state.transform = self.state.transform.pre_concat(t);
        }
    }

    // ---- drawing ----

    /// The image a style's shader reads, built before the paint borrows it
    fn shader_image(&self, style: &Style, bounds: Option<sk::Rect>) -> ShaderImage {
        match style {
            Style::Gradient(g) => match g.borrow().kind {
                GradientKind::Conic { angle, x, y } => conic_image(&g.borrow(), angle, x, y, self.state.global_alpha, &self.state.transform, bounds).map_or(ShaderImage::Nothing, |(p, t)| ShaderImage::Owned(p, t)),
                _ => ShaderImage::None,
            },
            Style::Pattern(p) => ShaderImage::Pattern(p.clone()),
            Style::Color(_) => ShaderImage::None,
        }
    }

    /// The paint for a style, with globalAlpha (shaders are in user space)
    fn paint_for<'a>(&self, style: &Style, image: &'a ShaderImage) -> Option<sk::Paint<'a>> {
        let mut paint = sk::Paint { anti_alias: true, ..Default::default() };
        let alpha = self.state.global_alpha;
        match (style, image) {
            (_, ShaderImage::Nothing) => return None,
            (_, ShaderImage::Owned(p, t)) => paint.shader = sk::Pattern::new(p.as_ref(), sk::SpreadMode::Pad, sk::FilterQuality::Nearest, 1.0, *t),
            (_, ShaderImage::Pattern(p)) => {
                let mode = if p.repetition == Repetition::NoRepeat { sk::SpreadMode::Pad } else { sk::SpreadMode::Repeat };
                let quality = if self.state.image_smoothing { sk::FilterQuality::Bilinear } else { sk::FilterQuality::Nearest };
                paint.shader = sk::Pattern::new(p.pixmap.as_ref(), mode, quality, alpha, *p.transform.borrow());
            }
            (Style::Color(c), _) => {
                let mut c = *c;
                c.apply_opacity(alpha);
                paint.set_color(c);
            }
            (Style::Gradient(g), _) => paint.shader = gradient_shader(&g.borrow(), alpha)?,
            (Style::Pattern(_), _) => return None,
        }
        Some(paint)
    }

    fn stroke_props(&self) -> sk::Stroke {
        let s = &self.state;
        let mut stroke = sk::Stroke { width: s.line_width, miter_limit: s.miter_limit, line_cap: s.line_cap, line_join: s.line_join, dash: None };
        if !s.dash.is_empty() {
            let mut dash = s.dash.clone();
            if dash.len() % 2 == 1 {
                dash.extend_from_within(..);
            }
            stroke.dash = sk::StrokeDash::new(dash, s.dash_offset);
        }
        stroke
    }

    /// Draw `what` with `style`: shadow, then the shape, composited and clipped
    fn draw(&mut self, what: Draw, style: &Style) {
        let t = self.state.transform;
        let Some(inv) = t.invert() else { return };
        // Paths are device-space; draw them in user space under the
        // transform so strokes and shaders get the transform's shape
        let path = match &what {
            Draw::Fill(p, _) | Draw::Stroke(p) => (*p).clone().transform(inv),
        };
        let Some(path) = path else { return };
        let image = self.shader_image(style, Some(path.bounds()));
        let Some(paint) = self.paint_for(style, &image) else { return };
        let stroke = matches!(what, Draw::Stroke(_)).then(|| self.stroke_props());
        let rule = match what {
            Draw::Fill(_, r) => r,
            Draw::Stroke(_) => sk::FillRule::Winding,
        };
        let render = |pixmap: &mut sk::Pixmap, paint: &sk::Paint, mask: Option<&sk::Mask>| match &stroke {
            Some(s) => pixmap.stroke_path(&path, paint, s, t, mask),
            None => pixmap.fill_path(&path, paint, rule, t, mask),
        };
        self.paint_shape(paint, render);
    }

    /// Shadow, compositing and clipping around a render callback
    fn paint_shape(&mut self, mut paint: sk::Paint<'_>, render: impl Fn(&mut sk::Pixmap, &sk::Paint, Option<&sk::Mask>)) {
        let (w, h) = (self.width, self.height);
        let composite = self.state.composite;
        let clip = self.state.clip.clone();
        let shadow = self.shadow_params();
        let Some(_) = self.surface() else { return };
        let needs_layer = composite.unbounded() || shadow.is_some();
        if !needs_layer {
            paint.blend_mode = composite.0;
            let pixmap = self.pixmap.as_mut().unwrap();
            render(pixmap, &paint, clip.as_deref());
            return;
        }
        // Draw the shape alone, then composite the layer
        let Some(mut layer) = sk::Pixmap::new(w, h) else { return };
        paint.blend_mode = sk::BlendMode::SourceOver;
        render(&mut layer, &paint, None);
        let pixmap = self.pixmap.as_mut().unwrap();
        if let Some((color, blur, (ox, oy))) = shadow {
            if let Some(shadow_layer) = shadow_of(&layer, color, blur) {
                let pp = sk::PixmapPaint { opacity: 1.0, blend_mode: composite.0, quality: sk::FilterQuality::Nearest };
                pixmap.draw_pixmap(0, 0, shadow_layer.as_ref(), &pp, sk::Transform::from_translate(ox, oy), clip.as_deref());
            }
        }
        let pp = sk::PixmapPaint { opacity: 1.0, blend_mode: composite.0, quality: sk::FilterQuality::Nearest };
        pixmap.draw_pixmap(0, 0, layer.as_ref(), &pp, sk::Transform::identity(), clip.as_deref());
    }

    fn shadow_params(&self) -> Option<(sk::Color, f32, (f32, f32))> {
        let s = &self.state;
        let visible = s.shadow_color.alpha() > 0.0 && (s.shadow_blur > 0.0 || s.shadow_offset.0 != 0.0 || s.shadow_offset.1 != 0.0);
        visible.then_some((s.shadow_color, s.shadow_blur, s.shadow_offset))
    }

    pub fn fill_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if let Some(p) = rect_path(&self.state.transform, x, y, w, h) {
            let style = self.state.fill.clone();
            self.draw(Draw::Fill(&p, sk::FillRule::Winding), &style);
        }
    }

    pub fn stroke_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite()) {
            return;
        }
        let t = self.state.transform;
        let path = if w == 0.0 || h == 0.0 {
            // A degenerate rectangle strokes as a line
            let mut p = PathData::new();
            p.apply(&PathCmd::MoveTo(x, y), &t);
            p.apply(&PathCmd::LineTo(x + w, y + h), &t);
            p.to_path()
        } else {
            rect_path(&t, x, y, w, h)
        };
        if let Some(p) = path {
            let style = self.state.stroke.clone();
            self.draw(Draw::Stroke(&p), &style);
        }
    }

    pub fn clear_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        if self.pixmap.is_none() {
            return;
        }
        let t = self.state.transform;
        let Some(p) = rect_path(&t, x, y, w, h) else { return };
        let clip = self.state.clip.clone();
        let opaque = self.opaque;
        let Some(pixmap) = self.surface() else { return };
        // A whole-canvas clear (the usual start of an animation frame)
        let b = p.bounds();
        let background = if opaque { sk::Color::BLACK } else { sk::Color::TRANSPARENT };
        if clip.is_none() && t.is_identity() && b.left() <= 0.0 && b.top() <= 0.0 && b.right() >= pixmap.width() as f32 && b.bottom() >= pixmap.height() as f32 {
            pixmap.fill(background);
            return;
        }
        let mut paint = sk::Paint { blend_mode: sk::BlendMode::Clear, anti_alias: true, ..Default::default() };
        if opaque {
            paint.set_color(background);
            paint.blend_mode = sk::BlendMode::Source;
        }
        pixmap.fill_path(&p, &paint, sk::FillRule::Winding, sk::Transform::identity(), clip.as_deref());
    }

    pub fn fill_path(&mut self, path: Option<&sk::Path>, rule: sk::FillRule) {
        let owned;
        let p = match path {
            Some(p) => p,
            None => match self.path.to_path() {
                Some(p) => {
                    owned = p;
                    &owned
                }
                None => return,
            },
        };
        let style = self.state.fill.clone();
        self.draw(Draw::Fill(p, rule), &style);
    }

    pub fn stroke_path(&mut self, path: Option<&sk::Path>) {
        let owned;
        let p = match path {
            Some(p) => p,
            None => match self.path.to_path() {
                Some(p) => {
                    owned = p;
                    &owned
                }
                None => return,
            },
        };
        let style = self.state.stroke.clone();
        self.draw(Draw::Stroke(p), &style);
    }

    pub fn clip(&mut self, path: Option<&sk::Path>, rule: sk::FillRule) {
        let owned;
        let p = match path {
            Some(p) => Some(p),
            None => {
                owned = self.path.to_path();
                owned.as_ref()
            }
        };
        let mask = match (p, &self.state.clip) {
            (None, _) => sk::Mask::new(self.width.max(1), self.height.max(1)),
            (Some(p), None) => sk::Mask::new(self.width.max(1), self.height.max(1)).map(|mut m| {
                m.fill_path(p, rule, true, sk::Transform::identity());
                m
            }),
            (Some(p), Some(old)) => {
                let mut m = (**old).clone();
                m.intersect_path(p, rule, true, sk::Transform::identity());
                Some(m)
            }
        };
        self.state.clip = mask.map(Rc::new);
    }

    /// Clip to an empty path: nothing shows until `restore()`
    pub fn clip_to_nothing(&mut self) {
        self.state.clip = sk::Mask::new(self.width.max(1), self.height.max(1)).map(Rc::new);
    }

    /// `isPointInPath` (point in canvas coordinates, path in device space)
    pub fn is_point_in_path(&self, path: Option<&sk::Path>, x: f32, y: f32, rule: sk::FillRule) -> bool {
        let owned;
        let p = match path {
            Some(p) => p,
            None => match self.path.to_path() {
                Some(p) => {
                    owned = p;
                    &owned
                }
                None => return false,
            },
        };
        point_in_path(p, x, y, rule)
    }

    pub fn is_point_in_stroke(&self, path: Option<&sk::Path>, x: f32, y: f32) -> bool {
        let owned;
        let p = match path {
            Some(p) => p,
            None => match self.path.to_path() {
                Some(p) => {
                    owned = p;
                    &owned
                }
                None => return false,
            },
        };
        let t = self.state.transform;
        let Some(inv) = t.invert() else { return false };
        let Some(user) = p.clone().transform(inv) else { return false };
        let Some(outline) = user.stroke(&self.stroke_props(), 1.0) else { return false };
        let Some(device) = outline.transform(t) else { return false };
        point_in_path(&device, x, y, sk::FillRule::Winding)
    }

    // ---- text ----

    pub fn fill_text(&mut self, text: &str, x: f32, y: f32, max_width: Option<f32>) {
        self.draw_text(text, x, y, max_width, false);
    }

    pub fn stroke_text(&mut self, text: &str, x: f32, y: f32, max_width: Option<f32>) {
        self.draw_text(text, x, y, max_width, true);
    }

    fn draw_text(&mut self, text: &str, x: f32, y: f32, max_width: Option<f32>, stroke: bool) {
        if !(x.is_finite() && y.is_finite()) || max_width.is_some_and(|m| !(m > 0.0)) {
            return;
        }
        let s = &self.state;
        let Some(layout) = text2d::layout(&s.font, text, s.letter_spacing) else { return };
        let mut dx = match (s.text_align, s.direction) {
            (TextAlign::Left, _) | (TextAlign::Start, Direction::Ltr | Direction::Inherit) | (TextAlign::End, Direction::Rtl) => 0.0,
            (TextAlign::Right, _) | (TextAlign::End, Direction::Ltr | Direction::Inherit) | (TextAlign::Start, Direction::Rtl) => -layout.advance,
            (TextAlign::Center, _) => -layout.advance / 2.0,
        };
        let dy = match s.text_baseline {
            TextBaseline::Alphabetic => 0.0,
            TextBaseline::Top => layout.ascent,
            TextBaseline::Hanging => layout.ascent * 0.8,
            TextBaseline::Middle => (layout.ascent - layout.descent) / 2.0,
            TextBaseline::Ideographic | TextBaseline::Bottom => -layout.descent,
        };
        // Squeeze horizontally to fit maxWidth
        let sx = match max_width {
            Some(m) if layout.advance > m => m / layout.advance,
            _ => 1.0,
        };
        dx *= sx;
        let Some(path) = layout.path else { return };
        let placement = sk::Transform::from_row(sx, 0.0, 0.0, 1.0, x + dx, y + dy);
        let Some(device) = path.transform(self.state.transform.pre_concat(placement)) else { return };
        if stroke {
            let style = self.state.stroke.clone();
            self.draw(Draw::Stroke(&device), &style);
        } else {
            let style = self.state.fill.clone();
            self.draw(Draw::Fill(&device, sk::FillRule::Winding), &style);
        }
    }

    pub fn measure_text(&self, text: &str) -> TextMetrics2D {
        text2d::measure(&self.state.font, text, self.state.letter_spacing, self.state.text_align, self.state.text_baseline)
    }

    // ---- images ----

    /// `drawImage`: the `(sx, sy, sw, sh)` part of `image` into `(dx, dy, dw, dh)`
    #[allow(clippy::too_many_arguments)]
    pub fn draw_image(&mut self, image: &sk::Pixmap, sx: f32, sy: f32, sw: f32, sh: f32, dx: f32, dy: f32, dw: f32, dh: f32) {
        let vals = [sx, sy, sw, sh, dx, dy, dw, dh];
        if !vals.iter().all(|v| v.is_finite()) || sw == 0.0 || sh == 0.0 || dw == 0.0 || dh == 0.0 {
            return;
        }
        // Normalize negative sizes
        let (sx, sw) = if sw < 0.0 { (sx + sw, -sw) } else { (sx, sw) };
        let (sy, sh) = if sh < 0.0 { (sy + sh, -sh) } else { (sy, sh) };
        let (dx, dw) = if dw < 0.0 { (dx + dw, -dw) } else { (dx, dw) };
        let (dy, dh) = if dh < 0.0 { (dy + dh, -dh) } else { (dy, dh) };
        // Clip the source rectangle to the image, shrinking the destination alike
        let (iw, ih) = (image.width() as f32, image.height() as f32);
        let (cx0, cy0) = (sx.max(0.0), sy.max(0.0));
        let (cx1, cy1) = ((sx + sw).min(iw), (sy + sh).min(ih));
        if cx1 <= cx0 || cy1 <= cy0 {
            return;
        }
        let (scale_x, scale_y) = (dw / sw, dh / sh);
        let (dx, dy) = (dx + (cx0 - sx) * scale_x, dy + (cy0 - sy) * scale_y);
        let (dw, dh) = ((cx1 - cx0) * scale_x, (cy1 - cy0) * scale_y);
        let src_rect = sk::IntRect::from_ltrb(cx0.floor() as i32, cy0.floor() as i32, cx1.ceil() as i32, cy1.ceil() as i32);
        let Some(sub) = src_rect.and_then(|r| image.clone_rect(r)) else { return };
        let quality = if !self.state.image_smoothing { sk::FilterQuality::Nearest } else { self.state.smoothing_quality.filter() };
        let pattern_t = sk::Transform::from_row(scale_x, 0.0, 0.0, scale_y, dx - (cx0 - cx0.floor()) * scale_x, dy - (cy0 - cy0.floor()) * scale_y);
        let shader = sk::Pattern::new(sub.as_ref(), sk::SpreadMode::Pad, quality, self.state.global_alpha, pattern_t);
        let Some(rect) = sk::Rect::from_xywh(dx, dy, dw, dh) else { return };
        let path = sk::PathBuilder::from_rect(rect);
        let t = self.state.transform;
        let paint = sk::Paint { shader, anti_alias: true, ..Default::default() };
        self.paint_shape(paint, |pixmap, paint, mask| pixmap.fill_path(&path, paint, sk::FillRule::Winding, t, mask));
    }

    // ---- pixels ----

    /// `getImageData`: unpremultiplied RGBA of a rectangle (outside the
    /// canvas reads as transparent black)
    pub fn get_image_data(&self, x: i32, y: i32, w: u32, h: u32) -> Vec<u8> {
        let mut out = vec![0u8; w as usize * h as usize * 4];
        let Some(pixmap) = &self.pixmap else { return out };
        // i64: offsets near i32::MAX must not overflow
        let (pw, ph) = (pixmap.width() as i64, pixmap.height() as i64);
        let (x, y, w) = (x as i64, y as i64, w as i64);
        let data = pixmap.data();
        // Only the rows and columns inside the canvas
        let (row0, row1) = ((-y).clamp(0, h as i64), (ph - y).clamp(0, h as i64));
        let (col0, col1) = ((-x).clamp(0, w), (pw - x).clamp(0, w));
        for row in row0..row1 {
            let sy = y + row;
            for col in col0..col1 {
                let sx = x + col;
                let s = ((sy * pw + sx) * 4) as usize;
                let d = ((row * w + col) * 4) as usize;
                let a = data[s + 3];
                if a == 0 {
                    continue;
                }
                for c in 0..3 {
                    out[d + c] = ((data[s + c] as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
                }
                out[d + 3] = a;
            }
        }
        out
    }

    /// `putImageData`: copy unpremultiplied RGBA (`w`×`h`) to `(x, y)`,
    /// limited to the dirty rectangle; ignores the state
    #[allow(clippy::too_many_arguments)]
    pub fn put_image_data(&mut self, data: &[u8], w: u32, h: u32, x: i32, y: i32, dirty: (i32, i32, i32, i32)) {
        let (mut dx0, mut dy0, mut dw, mut dh) = dirty;
        if dw < 0 {
            dx0 += dw;
            dw = -dw;
        }
        if dh < 0 {
            dy0 += dh;
            dh = -dh;
        }
        // i64 throughout: offsets near i32::MAX must not overflow
        let (dx0, dy0, dw, dh) = (dx0 as i64, dy0 as i64, dw as i64, dh as i64);
        let (x0, y0) = (dx0.max(0), dy0.max(0));
        let (x1, y1) = ((dx0 + dw).min(w as i64), (dy0 + dh).min(h as i64));
        if x1 <= x0 || y1 <= y0 || data.len() < w as usize * h as usize * 4 {
            return;
        }
        let Some(pixmap) = self.surface() else { return };
        let (pw, ph) = (pixmap.width() as i64, pixmap.height() as i64);
        let (x, y) = (x as i64, y as i64);
        // Only the part landing inside the canvas
        let (y0, y1) = (y0.max(-y), y1.min(ph - y));
        let (x0, x1) = (x0.max(-x), x1.min(pw - x));
        let dst = pixmap.data_mut();
        for row in y0..y1 {
            let ty = y + row;
            for col in x0..x1 {
                let tx = x + col;
                let s = ((row * w as i64 + col) * 4) as usize;
                let d = ((ty * pw + tx) * 4) as usize;
                let a = data[s + 3] as u32;
                for c in 0..3 {
                    dst[d + c] = ((data[s + c] as u32 * a + 127) / 255) as u8;
                }
                dst[d + 3] = a as u8;
            }
        }
    }

    /// The bitmap as PNG (all-transparent when nothing was drawn)
    pub fn to_png(&self) -> Option<Vec<u8>> {
        match &self.pixmap {
            Some(p) => p.encode_png().ok(),
            None => self.blank()?.encode_png().ok(),
        }
    }

    /// A copy of the bitmap (for drawImage from a canvas, or patterns)
    pub fn snapshot(&self) -> Option<sk::Pixmap> {
        match &self.pixmap {
            Some(p) => Some(p.clone()),
            None => self.blank(),
        }
    }
}

fn rect_path(t: &sk::Transform, x: f32, y: f32, w: f32, h: f32) -> Option<sk::Path> {
    if !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite()) || w == 0.0 || h == 0.0 {
        return None;
    }
    let mut p = PathData::new();
    p.apply(&PathCmd::MoveTo(x, y), t);
    p.apply(&PathCmd::LineTo(x + w, y), t);
    p.apply(&PathCmd::LineTo(x + w, y + h), t);
    p.apply(&PathCmd::LineTo(x, y + h), t);
    p.close();
    p.to_path()
}

/// A gradient shader in user space (the draw applies the transform)
fn gradient_shader(g: &Gradient, alpha: f32) -> Option<sk::Shader<'static>> {
    if g.stops.is_empty() {
        // A gradient without stops paints transparent black
        return Some(sk::Shader::SolidColor(sk::Color::TRANSPARENT));
    }
    let stops = |remap: &dyn Fn(f32) -> f32| -> Vec<sk::GradientStop> {
        g.stops
            .iter()
            .map(|&(o, mut c)| {
                c.apply_opacity(alpha);
                sk::GradientStop::new(remap(o), c)
            })
            .collect()
    };
    match g.kind {
        GradientKind::Linear { x0, y0, x1, y1 } => {
            if x0 == x1 && y0 == y1 {
                return None; // nothing painted
            }
            sk::LinearGradient::new(sk::Point::from_xy(x0, y0), sk::Point::from_xy(x1, y1), stops(&|o| o), sk::SpreadMode::Pad, sk::Transform::identity())
        }
        GradientKind::Radial { x0, y0, r0, x1, y1, r1 } => {
            if r1 <= 0.0 || (x0 == x1 && y0 == y1 && r0 == r1) {
                return None;
            }
            // tiny-skia's two-point gradient starts from a point: fold the
            // start radius into the stops (exact for concentric circles)
            let k = r0 / r1;
            sk::RadialGradient::new(sk::Point::from_xy(x0, y0), sk::Point::from_xy(x1, y1), r1, stops(&|o| k + o * (1.0 - k)), sk::SpreadMode::Pad, sk::Transform::identity())
        }
        // Built as an image (`conic_image`)
        GradientKind::Conic { .. } => None,
    }
}

/// The image behind a shader, owned by the draw that uses it
enum ShaderImage {
    /// The style needs none
    None,
    /// The style paints nothing (e.g. an empty conic area)
    Nothing,
    Pattern(Rc<CanvasPattern>),
    /// An image in user space, placed by the transform
    Owned(sk::Pixmap, sk::Transform),
}

/// tiny-skia has no sweep gradient: render one as an image over the
/// drawn area (in device pixels) and place it back in user space
fn conic_image(g: &Gradient, angle: f32, cx: f32, cy: f32, alpha: f32, t: &sk::Transform, bounds: Option<sk::Rect>) -> Option<(sk::Pixmap, sk::Transform)> {
    if g.stops.is_empty() {
        return None;
    }
    let b = bounds?.transform(*t)?;
    let (x0, y0) = (b.left().floor().max(-4096.0), b.top().floor().max(-4096.0));
    let (w, h) = ((b.right().ceil() - x0).clamp(1.0, 4096.0) as u32, (b.bottom().ceil() - y0).clamp(1.0, 4096.0) as u32);
    let inv = t.invert()?;
    let lut: Vec<sk::PremultipliedColorU8> = (0..256)
        .map(|i| {
            let mut c = g.color_at(i as f32 / 255.0);
            c.apply_opacity(alpha);
            c.premultiply().to_color_u8()
        })
        .collect();
    let mut pixmap = sk::Pixmap::new(w, h)?;
    let pixels = pixmap.pixels_mut();
    for py in 0..h {
        for px in 0..w {
            let (ux, uy) = map(&inv, x0 + px as f32 + 0.5, y0 + py as f32 + 0.5);
            // Angles from the start angle, clockwise from the +x axis
            let a = ((uy - cy).atan2(ux - cx) - angle).rem_euclid(TAU);
            pixels[(py * w + px) as usize] = lut[((a / TAU) * 255.0).round() as usize];
        }
    }
    // Device pixels at (x0, y0); the draw applies `t`, so undo it
    Some((pixmap, inv.pre_translate(x0, y0)))
}

/// The shadow of a layer: its alpha, blurred, in `color`
fn shadow_of(layer: &sk::Pixmap, color: sk::Color, blur: f32) -> Option<sk::Pixmap> {
    let (w, h) = (layer.width(), layer.height());
    let mut alpha: Vec<u8> = layer.pixels().iter().map(|p| p.alpha()).collect();
    if blur > 0.0 {
        // Past the canvas size more blur changes nothing visible
        let sigma = (blur / 2.0).min(w.max(h) as f32);
        box_blur(&mut alpha, w as usize, h as usize, sigma);
    }
    let mut out = sk::Pixmap::new(w, h)?;
    let c = color.premultiply().to_color_u8();
    for (dst, &a) in out.pixels_mut().iter_mut().zip(&alpha) {
        let scale = |v: u8| ((v as u32 * a as u32 + 127) / 255) as u8;
        *dst = sk::PremultipliedColorU8::from_rgba(scale(c.red()), scale(c.green()), scale(c.blue()), scale(c.alpha())).unwrap_or(*dst);
    }
    Some(out)
}

/// Three box blurs approximating a Gaussian of `sigma`
fn box_blur(data: &mut [u8], w: usize, h: usize, sigma: f32) {
    let d = ((sigma * 3.0 * (2.0 * PI).sqrt() / 4.0) + 0.5).floor().max(1.0) as usize;
    let mut tmp = vec![0u8; data.len()];
    for _ in 0..3 {
        blur_pass(data, &mut tmp, w, h, d, true);
        blur_pass(&tmp, data, w, h, d, false);
    }
}

fn blur_pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, d: usize, horizontal: bool) {
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    let r = d / 2;
    let idx = |line: usize, i: usize| if horizontal { line * w + i } else { i * w + line };
    for line in 0..lines {
        let mut sum: u32 = 0;
        // Window [i - r, i + r], edges treated as transparent
        for i in 0..=r.min(len - 1) {
            sum += src[idx(line, i)] as u32;
        }
        let size = (2 * r + 1) as u32;
        for i in 0..len {
            dst[idx(line, i)] = (sum / size) as u8;
            if i + r + 1 < len {
                sum += src[idx(line, i + r + 1)] as u32;
            }
            if i >= r {
                sum -= src[idx(line, i - r)] as u32;
            }
        }
    }
}

/// Winding test of a point against a path (flattening curves)
fn point_in_path(path: &sk::Path, x: f32, y: f32, rule: sk::FillRule) -> bool {
    if !(x.is_finite() && y.is_finite()) {
        return false;
    }
    let mut winding = 0i32;
    let mut on_edge = false;
    let mut edge = |a: (f32, f32), b: (f32, f32)| {
        // Points on the boundary count as inside, as in browsers
        let cross = (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0);
        if cross.abs() < 1e-3 && x >= a.0.min(b.0) - 1e-3 && x <= a.0.max(b.0) + 1e-3 && y >= a.1.min(b.1) - 1e-3 && y <= a.1.max(b.1) + 1e-3 {
            on_edge = true;
        }
        if a.1 <= y {
            if b.1 > y && cross > 0.0 {
                winding += 1;
            }
        } else if b.1 <= y && cross < 0.0 {
            winding -= 1;
        }
    };
    let (mut start, mut cur) = ((0.0, 0.0), (0.0, 0.0));
    let flat = |p0: (f32, f32), f: &dyn Fn(f32) -> (f32, f32), edge: &mut dyn FnMut((f32, f32), (f32, f32))| {
        let mut prev = p0;
        for i in 1..=16 {
            let p = f(i as f32 / 16.0);
            edge(prev, p);
            prev = p;
        }
    };
    for seg in path.segments() {
        match seg {
            sk::PathSegment::MoveTo(p) => {
                if cur != start {
                    edge(cur, start);
                }
                start = (p.x, p.y);
                cur = start;
            }
            sk::PathSegment::LineTo(p) => {
                edge(cur, (p.x, p.y));
                cur = (p.x, p.y);
            }
            sk::PathSegment::QuadTo(c, p) => {
                let p0 = cur;
                flat(p0, &|t| {
                    let u = 1.0 - t;
                    (u * u * p0.0 + 2.0 * u * t * c.x + t * t * p.x, u * u * p0.1 + 2.0 * u * t * c.y + t * t * p.y)
                }, &mut edge);
                cur = (p.x, p.y);
            }
            sk::PathSegment::CubicTo(c1, c2, p) => {
                let p0 = cur;
                flat(p0, &|t| {
                    let u = 1.0 - t;
                    (
                        u * u * u * p0.0 + 3.0 * u * u * t * c1.x + 3.0 * u * t * t * c2.x + t * t * t * p.x,
                        u * u * u * p0.1 + 3.0 * u * u * t * c1.y + 3.0 * u * t * t * c2.y + t * t * t * p.y,
                    )
                }, &mut edge);
                cur = (p.x, p.y);
            }
            sk::PathSegment::Close => {
                edge(cur, start);
                cur = start;
            }
        }
    }
    if cur != start {
        edge(cur, start);
    }
    on_edge
        || match rule {
            sk::FillRule::Winding => winding != 0,
            sk::FillRule::EvenOdd => winding % 2 != 0,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(c: &Canvas2D, x: i32, y: i32) -> [u8; 4] {
        let d = c.get_image_data(x, y, 1, 1);
        [d[0], d[1], d[2], d[3]]
    }

    fn rgba(r: u8, g: u8, b: u8, a: u8) -> Style {
        Style::Color(sk::Color::from_rgba8(r, g, b, a))
    }

    #[test]
    fn rects_transforms_and_state() {
        let mut c = Canvas2D::new(20, 20);
        assert!(c.pixmap().is_none(), "no bitmap before drawing");
        c.state.fill = rgba(255, 0, 0, 255);
        c.fill_rect(0.0, 0.0, 10.0, 10.0);
        assert_eq!(px(&c, 5, 5), [255, 0, 0, 255]);
        assert_eq!(px(&c, 15, 15), [0, 0, 0, 0]);
        c.save();
        c.transform(sk::Transform::from_translate(10.0, 10.0));
        c.state.fill = rgba(0, 0, 255, 255);
        c.fill_rect(0.0, 0.0, 5.0, 5.0);
        c.restore();
        assert_eq!(px(&c, 12, 12), [0, 0, 255, 255]);
        assert!(matches!(c.state.fill, Style::Color(col) if col.red() == 1.0), "restore brings back the fill");
        c.clear_rect(0.0, 0.0, 5.0, 5.0);
        assert_eq!(px(&c, 2, 2), [0, 0, 0, 0]);
        assert_eq!(px(&c, 7, 7), [255, 0, 0, 255]);
        c.clear_rect(0.0, 0.0, 20.0, 20.0);
        assert_eq!(px(&c, 12, 12), [0, 0, 0, 0]);
        // Out-of-canvas reads are transparent
        assert_eq!(c.get_image_data(-5, -5, 2, 2), vec![0; 16]);
    }

    #[test]
    fn paths_arcs_and_hit_testing() {
        let mut c = Canvas2D::new(40, 40);
        let t = c.state.transform;
        c.path.apply(&PathCmd::Ellipse { x: 20.0, y: 20.0, rx: 10.0, ry: 10.0, rotation: 0.0, start: 0.0, end: TAU, ccw: false }, &t);
        c.fill_path(None, sk::FillRule::Winding);
        assert_eq!(px(&c, 20, 20)[3], 255);
        assert_eq!(px(&c, 2, 2)[3], 0);
        assert_eq!(px(&c, 20, 12)[3], 255, "inside near the top");
        assert_eq!(px(&c, 20, 8)[3], 0, "outside above");
        assert!(c.is_point_in_path(None, 20.0, 20.0, sk::FillRule::Winding));
        assert!(!c.is_point_in_path(None, 31.0, 31.0, sk::FillRule::Winding));
        c.state.line_width = 4.0;
        assert!(c.is_point_in_stroke(None, 30.0, 20.0));
        assert!(!c.is_point_in_stroke(None, 20.0, 20.0));
        // Even-odd: a rectangle inside a rectangle has a hole
        let cmds = [PathCmd::Rect(0.0, 0.0, 30.0, 30.0), PathCmd::Rect(10.0, 10.0, 10.0, 10.0)];
        let p = build_path(&cmds, &sk::Transform::identity());
        assert!(!c.is_point_in_path(p.as_ref(), 15.0, 15.0, sk::FillRule::EvenOdd));
        assert!(c.is_point_in_path(p.as_ref(), 15.0, 15.0, sk::FillRule::Winding));
        assert!(c.is_point_in_path(p.as_ref(), 5.0, 5.0, sk::FillRule::EvenOdd));
        // arcTo rounds a corner: the corner itself stays outside
        let mut p = PathData::new();
        let id = sk::Transform::identity();
        for cmd in [PathCmd::MoveTo(0.0, 0.0), PathCmd::ArcTo(30.0, 0.0, 30.0, 30.0, 10.0), PathCmd::LineTo(30.0, 30.0), PathCmd::LineTo(0.0, 30.0), PathCmd::Close] {
            p.apply(&cmd, &id);
        }
        let p = p.to_path();
        assert!(!c.is_point_in_path(p.as_ref(), 29.0, 1.0, sk::FillRule::Winding));
        assert!(c.is_point_in_path(p.as_ref(), 15.0, 15.0, sk::FillRule::Winding));
    }

    #[test]
    fn clipping_alpha_and_compositing() {
        let mut c = Canvas2D::new(20, 20);
        c.save();
        let clip = build_path(&[PathCmd::Rect(0.0, 0.0, 10.0, 20.0)], &sk::Transform::identity());
        c.clip(clip.as_ref(), sk::FillRule::Winding);
        c.state.fill = rgba(0, 255, 0, 255);
        c.fill_rect(0.0, 0.0, 20.0, 20.0);
        c.restore();
        assert_eq!(px(&c, 5, 5), [0, 255, 0, 255]);
        assert_eq!(px(&c, 15, 5), [0, 0, 0, 0], "clipped away");
        // globalAlpha
        c.state.global_alpha = 0.5;
        c.state.fill = rgba(0, 0, 255, 255);
        c.fill_rect(10.0, 0.0, 10.0, 10.0);
        let p = px(&c, 15, 5);
        assert!(p[3].abs_diff(128) <= 1 && p[2] == 255, "{p:?}");
        c.state.global_alpha = 1.0;
        // destination-out erases where drawn
        c.state.composite = Composite::parse("destination-out").unwrap();
        c.fill_rect(0.0, 0.0, 5.0, 5.0);
        assert_eq!(px(&c, 2, 2)[3], 0);
        assert_eq!(px(&c, 7, 7), [0, 255, 0, 255]);
        // source-in is unbounded: it clears outside the shape too
        c.state.composite = Composite::parse("source-in").unwrap();
        c.state.fill = rgba(255, 0, 0, 255);
        c.fill_rect(5.0, 5.0, 4.0, 4.0);
        assert_eq!(px(&c, 6, 6), [255, 0, 0, 255]);
        assert_eq!(px(&c, 2, 12)[3], 0, "outside the shape is cleared");
        assert_eq!(Composite::parse("lighter").unwrap().name(), "lighter");
        assert!(Composite::parse("bogus").is_none());
    }

    #[test]
    fn gradients_and_patterns() {
        let mut c = Canvas2D::new(100, 10);
        let mut g = Gradient::new(GradientKind::Linear { x0: 0.0, y0: 0.0, x1: 100.0, y1: 0.0 });
        g.add_color_stop(0.0, sk::Color::from_rgba8(255, 0, 0, 255));
        g.add_color_stop(1.0, sk::Color::from_rgba8(0, 0, 255, 255));
        c.state.fill = Style::Gradient(Rc::new(RefCell::new(g)));
        c.fill_rect(0.0, 0.0, 100.0, 10.0);
        let (l, m, r) = (px(&c, 1, 5), px(&c, 50, 5), px(&c, 98, 5));
        assert!(l[0] > 240 && l[2] < 15, "{l:?}");
        assert!(r[2] > 240 && r[0] < 15, "{r:?}");
        assert!(m[0].abs_diff(127) < 10 && m[2].abs_diff(127) < 10, "{m:?}");
        // A conic gradient, starting at the top (angle -π/2), red to blue
        let mut c = Canvas2D::new(40, 40);
        let mut g = Gradient::new(GradientKind::Conic { angle: -PI / 2.0, x: 20.0, y: 20.0 });
        g.add_color_stop(0.0, sk::Color::from_rgba8(255, 0, 0, 255));
        g.add_color_stop(1.0, sk::Color::from_rgba8(0, 0, 255, 255));
        c.state.fill = Style::Gradient(Rc::new(RefCell::new(g)));
        c.fill_rect(0.0, 0.0, 40.0, 40.0);
        let (right, below) = (px(&c, 38, 20), px(&c, 20, 38));
        assert!(right[0] > 170 && right[2] < 85, "a quarter turn: {right:?}");
        assert!(below[0].abs_diff(127) < 12, "half a turn: {below:?}");
        // A repeating 2x2 checker pattern
        let mut tile = sk::Pixmap::new(2, 2).unwrap();
        tile.pixels_mut()[0] = sk::PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();
        tile.pixels_mut()[3] = sk::PremultipliedColorU8::from_rgba(0, 0, 0, 255).unwrap();
        let mut c = Canvas2D::new(8, 8);
        c.state.image_smoothing = false;
        c.state.fill = Style::Pattern(Rc::new(CanvasPattern { pixmap: tile, repetition: Repetition::Repeat, transform: RefCell::new(sk::Transform::identity()) }));
        c.fill_rect(0.0, 0.0, 8.0, 8.0);
        assert_eq!(px(&c, 4, 4)[3], 255);
        assert_eq!(px(&c, 5, 4)[3], 0);
        assert_eq!(px(&c, 5, 5)[3], 255);
    }

    #[test]
    fn shadows() {
        let mut c = Canvas2D::new(40, 40);
        c.state.shadow_color = sk::Color::from_rgba8(0, 0, 0, 255);
        c.state.shadow_offset = (10.0, 10.0);
        c.state.fill = rgba(255, 0, 0, 255);
        c.fill_rect(5.0, 5.0, 10.0, 10.0);
        assert_eq!(px(&c, 10, 10), [255, 0, 0, 255], "the shape over its shadow");
        assert_eq!(px(&c, 22, 22), [0, 0, 0, 255], "the offset shadow");
        assert_eq!(px(&c, 35, 5)[3], 0);
        // A blurred shadow fades out at its edges
        let mut c = Canvas2D::new(60, 60);
        c.state.shadow_color = sk::Color::from_rgba8(0, 0, 0, 255);
        c.state.shadow_blur = 8.0;
        c.state.shadow_offset = (0.0, 30.0);
        c.fill_rect(20.0, 5.0, 20.0, 20.0);
        let (center, edge) = (px(&c, 30, 45)[3], px(&c, 30, 35)[3]);
        assert!(center > 200 && edge > 40 && edge < 220, "{center} {edge}");
    }

    #[test]
    fn image_data_and_draw_image() {
        let mut c = Canvas2D::new(4, 4);
        let mut data = vec![0u8; 2 * 2 * 4];
        data[..4].copy_from_slice(&[255, 128, 0, 255]);
        data[4..8].copy_from_slice(&[0, 0, 255, 128]);
        c.put_image_data(&data, 2, 2, 1, 1, (0, 0, 2, 2));
        assert_eq!(px(&c, 1, 1), [255, 128, 0, 255]);
        let half = px(&c, 2, 1);
        assert_eq!(half[3], 128);
        assert!(half[2] >= 254, "unpremultiplied on the way out: {half:?}");
        // Only the dirty rectangle is written
        let mut c2 = Canvas2D::new(4, 4);
        c2.put_image_data(&data, 2, 2, 0, 0, (1, 0, 1, 1));
        assert_eq!(px(&c2, 0, 0)[3], 0);
        assert_eq!(px(&c2, 1, 0)[3], 128);
        // drawImage scales the source rectangle into the destination
        let mut src = Canvas2D::new(2, 2);
        src.state.fill = rgba(0, 128, 0, 255);
        src.fill_rect(0.0, 0.0, 1.0, 2.0);
        let image = src.snapshot().unwrap();
        let mut c = Canvas2D::new(20, 20);
        c.state.image_smoothing = false;
        c.draw_image(&image, 0.0, 0.0, 2.0, 2.0, 0.0, 0.0, 20.0, 20.0);
        assert_eq!(px(&c, 5, 10), [0, 128, 0, 255]);
        assert_eq!(px(&c, 15, 10)[3], 0);
        // Source rectangles past the image are clipped, not stretched
        let mut c = Canvas2D::new(20, 20);
        c.state.image_smoothing = false;
        c.draw_image(&image, 0.0, 0.0, 4.0, 4.0, 0.0, 0.0, 20.0, 20.0);
        assert_eq!(px(&c, 2, 2), [0, 128, 0, 255]);
        assert_eq!(px(&c, 12, 12)[3], 0);
        let png = c.to_png().unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }

    #[test]
    fn degenerate_input_is_ignored() {
        let mut c = Canvas2D::new(10, 10);
        c.fill_rect(f32::NAN, 0.0, 5.0, 5.0);
        c.fill_rect(0.0, 0.0, f32::INFINITY, 5.0);
        c.transform(sk::Transform::from_row(f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0));
        c.fill_rect(0.0, 0.0, 5.0, 5.0);
        assert_eq!(px(&c, 2, 2)[3], 255, "the NaN transform was ignored");
        c.set_transform(sk::Transform::from_row(0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
        c.fill_rect(5.0, 5.0, 5.0, 5.0);
        assert_eq!(px(&c, 7, 7)[3], 0, "a singular transform draws nothing");
        for _ in 0..5000 {
            c.save();
        }
        assert_eq!(c.stack.len(), 1024);
        let mut huge = Canvas2D::new(100_000, 100_000);
        huge.fill_rect(0.0, 0.0, 1.0, 1.0);
        assert!(huge.pixmap().is_none(), "over the size cap: never allocated");
        // Offsets at the ends of the i32 range
        c.state.fill = rgba(1, 2, 3, 255);
        assert_eq!(c.get_image_data(i32::MAX, i32::MAX, 3, 1), vec![0; 12]);
        assert_eq!(c.get_image_data(i32::MIN, 0, 2, 1), vec![0; 8]);
        c.put_image_data(&[9; 16], 2, 2, i32::MAX, i32::MAX, (0, 0, 2, 2));
        c.put_image_data(&[9; 16], 2, 2, i32::MIN, 0, (i32::MAX, i32::MAX, i32::MAX, i32::MAX));
        c.state.shadow_color = sk::Color::BLACK;
        c.state.shadow_blur = 1e30;
        c.fill_rect(0.0, 0.0, 2.0, 2.0);
        let mut empty = Canvas2D::new(0, 0);
        empty.fill_rect(0.0, 0.0, 1.0, 1.0);
        assert!(empty.to_png().is_some());
    }

    #[test]
    fn text_draws_when_fonts_exist() {
        let mut c = Canvas2D::new(100, 30);
        c.state.font = text2d::parse_font("20px sans-serif").unwrap();
        let m = c.measure_text("Hello");
        assert!(m.width > 20.0, "{}", m.width);
        if text2d::layout(&c.state.font, "Hello", 0.0).is_some_and(|l| l.path.is_some()) {
            c.fill_text("Hello", 5.0, 22.0, None);
            let ink = c.get_image_data(0, 0, 100, 30).chunks(4).filter(|p| p[3] > 0).count();
            assert!(ink > 50, "{ink}");
            // maxWidth squeezes the text
            let mut narrow = Canvas2D::new(100, 30);
            narrow.state.font = c.state.font.clone();
            narrow.fill_text("Hello", 0.0, 22.0, Some(10.0));
            assert_eq!(narrow.get_image_data(20, 0, 80, 30).iter().skip(3).step_by(4).filter(|&&a| a > 0).count(), 0);
        }
    }
}
