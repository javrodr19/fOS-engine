//! Script bindings of the 2D canvas, drawn by fos-canvas:
//! `CanvasRenderingContext2D` (and `OffscreenCanvasRenderingContext2D`),
//! `Path2D`, `CanvasGradient`, `CanvasPattern`, `TextMetrics`,
//! `OffscreenCanvas` and `ImageBitmap`.
//!
//! Contexts, paths, gradients, patterns, offscreen canvases and bitmaps
//! are `HostData` objects: their Rust state (a bitmap can be megabytes)
//! dies with them, and its size counts toward the collector's heap. A
//! canvas element's context is held by `DomHost` as long as the element.
//! `ImageData`, `toBlob` and the promise-returning APIs are in
//! `dom_bootstrap.js`, on top of the `__fosCanvas*` natives here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use fos_canvas::tiny_skia as sk;
use fos_canvas::{
    build_path, parse_font, parse_svg_path, Canvas2D, CanvasPattern, Composite, Direction, Gradient, GradientKind, PathCmd, PathData, Repetition,
    SmoothingQuality, Style, TextAlign, TextBaseline,
};
use fos_dom::NodeId;
use fos_jsvm::builtins::arg;
use fos_jsvm::gc::{Gc, Tracer};
use fos_jsvm::object::{HostData, JsObject, ObjectKind, PropFlags};
use fos_jsvm::vm::{truthy, NativeFn};
use fos_jsvm::{JsResult, Value, Vm};

use crate::dom_bindings::{self as dom, node_id};

/// The largest ImageData / ImageBitmap we allocate (pixels)
const MAX_PIXELS: u64 = 1 << 28;

/// Prototypes of the canvas interfaces
#[derive(Clone, Copy)]
struct Protos {
    context: Gc<JsObject>,
    offscreen_context: Gc<JsObject>,
    gradient: Gc<JsObject>,
    pattern: Gc<JsObject>,
    path: Gc<JsObject>,
    metrics: Gc<JsObject>,
    bitmap: Gc<JsObject>,
    offscreen: Gc<JsObject>,
}

/// Per-page canvas state in `DomHost`
#[derive(Default)]
pub struct CanvasHost {
    protos: Option<Protos>,
    /// Canvas element (node index) -> index in `vm.host_roots` of its context
    contexts: HashMap<u32, usize>,
}

/// A 2D context: the canvas and what scripts see of its styles
pub struct Context2D {
    pub canvas: Canvas2D,
    /// `fillStyle` / `strokeStyle` as set: a color string (serialized) or
    /// the CanvasGradient / CanvasPattern object
    fill: Value,
    stroke: Value,
    /// Their values saved by `save()`, restored with the state
    saved: Vec<(Value, Value)>,
    /// The canvas element or OffscreenCanvas
    owner: Value,
    /// Bitmap bytes counted in the object's size for the collector
    accounted: usize,
    will_read_frequently: bool,
}

impl HostData for Context2D {
    fn trace(&self, t: &mut Tracer) {
        t.mark_value(self.fill);
        t.mark_value(self.stroke);
        t.mark_value(self.owner);
        for &(f, s) in &self.saved {
            t.mark_value(f);
            t.mark_value(s);
        }
    }
}

/// A `Path2D`: its commands in its own coordinates
struct PathObject {
    cmds: Vec<PathCmd>,
}
impl HostData for PathObject {}

struct GradientObject(Rc<RefCell<Gradient>>);
impl HostData for GradientObject {}

struct PatternObject(Rc<CanvasPattern>);
impl HostData for PatternObject {}

/// An `ImageBitmap` (no pixels once closed)
struct BitmapObject {
    pixmap: Option<sk::Pixmap>,
}
impl HostData for BitmapObject {}

/// An `OffscreenCanvas` and its context, once made
struct OffscreenObject {
    width: u32,
    height: u32,
    context: Value,
}
impl HostData for OffscreenObject {
    fn trace(&self, t: &mut Tracer) {
        t.mark_value(self.context);
    }
}

// ---- helpers ----

fn host(vm: &mut Vm) -> &mut CanvasHost {
    &mut dom::host(vm).canvas
}

fn protos(vm: &mut Vm) -> Protos {
    host(vm).protos.expect("canvas bindings not installed")
}

fn new_host_object(vm: &mut Vm, proto: Gc<JsObject>, data: Box<dyn HostData>) -> Gc<JsObject> {
    vm.new_object_with(Some(proto), ObjectKind::HostData(data))
}

fn data<'a, T: HostData>(v: Value) -> Option<&'a mut T> {
    v.as_object()?.get_mut_detached().host_data_mut::<T>()
}

/// The context `this` stands for. Callers convert their arguments first:
/// conversions can run scripts, which must not see the borrow.
fn this_ctx<'a>(vm: &mut Vm, this: Value) -> JsResult<&'a mut Context2D> {
    data::<Context2D>(this).ok_or_else(|| vm.type_error("Illegal invocation"))
}

/// A `DOMException` (as defined by the bootstrap)
fn dom_exception(vm: &mut Vm, name: &str, message: &str) -> Value {
    let g = Value::object(vm.global);
    if let Ok(ctor) = vm.get_str(g, "DOMException") {
        if vm.is_callable(ctor) {
            let (m, n) = (vm.str_value(message), vm.str_value(name));
            if let Ok(e) = vm.construct(ctor, &[m, n], ctor) {
                return e;
            }
        }
    }
    dom::dom_error(vm, name, message)
}

fn construct_global(vm: &mut Vm, name: &str, args: &[Value]) -> JsResult<Value> {
    let g = Value::object(vm.global);
    let ctor = vm.get_str(g, name)?;
    vm.construct(ctor, args, ctor)
}

fn require(vm: &mut Vm, args: &[Value], n: usize, method: &str, interface: &str) -> JsResult<()> {
    if args.len() < n {
        let msg = format!("Failed to execute '{method}' on '{interface}': {n} argument{} required, but only {} present.", if n == 1 { "" } else { "s" }, args.len());
        return Err(vm.type_error(&msg));
    }
    Ok(())
}

fn nums<const N: usize>(vm: &mut Vm, args: &[Value]) -> JsResult<[f32; N]> {
    let mut out = [0.0; N];
    for (i, o) in out.iter_mut().enumerate() {
        *o = vm.to_number(arg(args, i))? as f32;
    }
    Ok(out)
}

/// Arguments typed `double` (not `unrestricted double`): non-finite throws
fn finite_nums<const N: usize>(vm: &mut Vm, args: &[Value]) -> JsResult<[f32; N]> {
    let out = nums::<N>(vm, args)?;
    if out.iter().any(|v| !v.is_finite()) {
        return Err(vm.type_error("The provided double value is non-finite."));
    }
    Ok(out)
}

fn atom(vm: &mut Vm, s: &str) -> Value {
    let a = vm.intern(s);
    vm.atom_value(a)
}

fn number(v: f32) -> Value {
    Value::number(v as f64)
}

/// A CSS color for the canvas (`currentcolor` is black: canvases here
/// have no computed style)
fn parse_color(s: &str) -> Option<sk::Color> {
    if s.trim().eq_ignore_ascii_case("currentcolor") {
        return Some(sk::Color::BLACK);
    }
    let c = fos_css::parse_color(s)?;
    Some(sk::Color::from_rgba8(c.r, c.g, c.b, c.a))
}

/// Colors as canvases serialize them: `#rrggbb` when opaque, else
/// `rgba(r, g, b, a)` with the shortest alpha that round-trips
fn serialize_color(c: sk::Color) -> String {
    let c = c.to_color_u8();
    let (r, g, b, a) = (c.red(), c.green(), c.blue(), c.alpha());
    if a == 255 {
        return format!("#{r:02x}{g:02x}{b:02x}");
    }
    let two = (a as f64 / 255.0 * 100.0).round() / 100.0;
    let alpha = if (two * 255.0).round() as u8 == a { two } else { (a as f64 / 255.0 * 1000.0).round() / 1000.0 };
    format!("rgba({r}, {g}, {b}, {alpha})")
}

fn format_px(v: f32) -> String {
    let mut s = format!("{v}");
    if s == "-0" {
        s = "0".into();
    }
    s + "px"
}

/// A CSS length for letterSpacing / wordSpacing (`em` relative to the font)
fn parse_spacing(s: &str, font_size: f32) -> Option<f32> {
    let s = s.trim().to_ascii_lowercase();
    let unit_at = s.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(s.len());
    let n: f32 = s[..unit_at].parse().ok()?;
    let px = match &s[unit_at..] {
        "px" => n,
        "" if n == 0.0 => 0.0,
        "em" => n * font_size,
        "rem" => n * 16.0,
        "pt" => n * 4.0 / 3.0,
        "pc" => n * 16.0,
        "in" => n * 96.0,
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 96.0 / 25.4,
        _ => return None,
    };
    px.is_finite().then_some(px)
}

/// A DOMMatrix2DInit (or a DOMMatrix): `a`..`f`, else `m11`..`m42`
fn matrix_init(vm: &mut Vm, v: Value) -> JsResult<sk::Transform> {
    if v.is_nullish() {
        return Ok(sk::Transform::identity());
    }
    if !v.is_object() {
        return Err(vm.type_error("The provided value is not of type 'DOMMatrix2DInit'."));
    }
    let mut m = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0];
    for (i, (short, long)) in [("a", "m11"), ("b", "m12"), ("c", "m21"), ("d", "m22"), ("e", "m41"), ("f", "m42")].iter().enumerate() {
        let mut x = vm.get_str(v, short)?;
        if x.is_undefined() {
            x = vm.get_str(v, long)?;
        }
        if !x.is_undefined() {
            m[i] = vm.to_number(x)? as f32;
        }
    }
    if m.iter().any(|x| !x.is_finite()) {
        return Err(vm.type_error("Failed to read the 'DOMMatrix2DInit': the matrix is not finite."));
    }
    Ok(sk::Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5]))
}

fn fill_rule(vm: &mut Vm, v: Value) -> JsResult<sk::FillRule> {
    if v.is_undefined() {
        return Ok(sk::FillRule::Winding);
    }
    match vm.to_rust_string(v)?.as_str() {
        "nonzero" => Ok(sk::FillRule::Winding),
        "evenodd" => Ok(sk::FillRule::EvenOdd),
        other => Err(vm.type_error(&format!("The provided value '{other}' is not a valid enum value of type CanvasFillRule."))),
    }
}

/// Report the bitmap's size to the collector when it changed
fn account(vm: &mut Vm, this: Value) {
    let Some(o) = this.as_object() else { return };
    let Some(c) = o.get_mut().host_data_mut::<Context2D>() else { return };
    let bytes = c.canvas.pixmap().map_or(0, |p| p.data().len());
    if bytes != c.accounted {
        if bytes > c.accounted {
            vm.heap.note_growth(bytes - c.accounted);
        }
        c.accounted = bytes;
        o.set_size(std::mem::size_of::<JsObject>() + std::mem::size_of::<Context2D>() + bytes);
    }
}

/// Run a drawing operation on `this`'s canvas
fn draw(vm: &mut Vm, this: Value, f: impl FnOnce(&mut Canvas2D)) -> JsResult<Value> {
    let c = this_ctx(vm, this)?;
    f(&mut c.canvas);
    account(vm, this);
    Ok(Value::UNDEFINED)
}

fn new_context(vm: &mut Vm, proto: Gc<JsObject>, owner: Value, width: u32, height: u32, alpha: bool, will_read_frequently: bool) -> Gc<JsObject> {
    let black = atom(vm, "#000000");
    let canvas = if alpha { Canvas2D::new(width, height) } else { Canvas2D::new_opaque(width, height) };
    let ctx = Context2D { canvas, fill: black, stroke: black, saved: Vec::new(), owner, accounted: 0, will_read_frequently };
    new_host_object(vm, proto, Box::new(ctx))
}

/// A canvas's new size: its state and bitmap start over
fn resize_context(vm: &mut Vm, ctx: Value, width: u32, height: u32) {
    let black = atom(vm, "#000000");
    if let Some(c) = data::<Context2D>(ctx) {
        c.canvas.resize(width, height);
        c.fill = black;
        c.stroke = black;
        c.saved.clear();
    }
    account(vm, ctx);
}

// ---- canvas elements ----

/// HTML's rules for parsing non-negative integers (`width="300px"` is 300)
fn parse_dimension(v: Option<&str>, default: u32) -> u32 {
    let Some(v) = v else { return default };
    let v = v.trim_start_matches([' ', '\t', '\n', '\x0c', '\r']);
    let v = v.strip_prefix('+').unwrap_or(v);
    let end = v.find(|c: char| !c.is_ascii_digit()).unwrap_or(v.len());
    v[..end].parse::<u64>().ok().map_or(default, |n| n.min(u32::MAX as u64) as u32)
}

/// A canvas element's size, if `id` is one
fn canvas_size(vm: &mut Vm, id: NodeId) -> Option<(u32, u32)> {
    dom::with_tree(vm, |t| {
        let e = t.get(id)?.as_element()?;
        if dom::element_tag(t, e) != "canvas" {
            return None;
        }
        Some((parse_dimension(t.get_attribute(id, "width"), 300), parse_dimension(t.get_attribute(id, "height"), 150)))
    })
}

fn element_context(vm: &mut Vm, id: NodeId) -> Option<Value> {
    let slot = *host(vm).contexts.get(&id.0)?;
    Some(vm.host_roots[slot])
}

/// `width` or `height` of an element changed: a canvas starts over
pub fn attribute_changed(vm: &mut Vm, id: NodeId, name: &str) {
    if name != "width" && name != "height" {
        return;
    }
    let Some(ctx) = element_context(vm, id) else { return };
    if let Some((w, h)) = canvas_size(vm, id) {
        resize_context(vm, ctx, w, h);
    }
}

/// `__fosCanvasGetContext(canvas, type, options)`: `getContext` of
/// canvas elements and OffscreenCanvas (2D only)
fn get_context(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let target = arg(args, 0);
    let kind = vm.to_rust_string(arg(args, 1))?;
    let options = arg(args, 2);
    let (alpha, will_read) = if options.is_object() {
        let a = vm.get_str(options, "alpha")?;
        let w = vm.get_str(options, "willReadFrequently")?;
        (a.is_undefined() || truthy(a), truthy(w))
    } else {
        (true, false)
    };
    let p = protos(vm);
    if let Some(id) = node_id(target) {
        if let Some(ctx) = element_context(vm, id) {
            return Ok(if kind == "2d" { ctx } else { Value::NULL });
        }
        let Some((w, h)) = canvas_size(vm, id) else { return Err(vm.type_error("Illegal invocation")) };
        if kind != "2d" {
            return Ok(Value::NULL);
        }
        let ctx = Value::object(new_context(vm, p.context, target, w, h, alpha, will_read));
        let slot = vm.host_roots.len();
        vm.host_roots.push(ctx);
        host(vm).contexts.insert(id.0, slot);
        return Ok(ctx);
    }
    let Some(off) = data::<OffscreenObject>(target) else { return Err(vm.type_error("Illegal invocation")) };
    if !off.context.is_undefined() {
        return Ok(if kind == "2d" { off.context } else { Value::NULL });
    }
    if kind != "2d" {
        return Ok(Value::NULL);
    }
    let (w, h) = (off.width, off.height);
    let ctx = Value::object(new_context(vm, p.offscreen_context, target, w, h, alpha, will_read));
    if let Some(off) = data::<OffscreenObject>(target) {
        off.context = ctx;
    }
    Ok(ctx)
}

/// What an image argument (`drawImage`, `createPattern`,
/// `createImageBitmap`) holds
enum Source {
    /// A context or ImageBitmap object, whose bitmap can be borrowed
    Object(Gc<JsObject>),
    /// Transparent pixels of this size (a canvas never drawn on)
    Blank(u32, u32),
    /// Not ready (an image still loading): nothing is drawn
    Unavailable,
}

impl Source {
    fn size(&self) -> (u32, u32) {
        match self {
            Source::Object(o) => {
                if let Some(c) = o.get().host_data::<Context2D>() {
                    (c.canvas.width(), c.canvas.height())
                } else if let Some(b) = o.get().host_data::<BitmapObject>() {
                    b.pixmap.as_ref().map_or((0, 0), |p| (p.width(), p.height()))
                } else {
                    (0, 0)
                }
            }
            Source::Blank(w, h) => (*w, *h),
            Source::Unavailable => (0, 0),
        }
    }

    /// The pixels (`None` for blank or never-drawn canvases)
    fn pixmap(&self) -> Option<&sk::Pixmap> {
        match self {
            Source::Object(o) => {
                let obj = o.get_mut_detached();
                if let Some(c) = obj.host_data::<Context2D>() {
                    c.canvas.pixmap()
                } else {
                    obj.host_data::<BitmapObject>()?.pixmap.as_ref()
                }
            }
            _ => None,
        }
    }

    /// An owned copy (opaque canvases never drawn on are black)
    fn snapshot(&self) -> Option<sk::Pixmap> {
        if let Source::Object(o) = self {
            if let Some(c) = o.get().host_data::<Context2D>() {
                return c.canvas.snapshot();
            }
        }
        if let Some(p) = self.pixmap() {
            return Some(p.clone());
        }
        let (w, h) = self.size();
        sk::Pixmap::new(w.max(1), h.max(1))
    }
}

const IMAGE_TYPES: &str =
    "(CSSImageValue or HTMLCanvasElement or HTMLImageElement or HTMLVideoElement or ImageBitmap or OffscreenCanvas or SVGImageElement or VideoFrame)";

fn image_source(vm: &mut Vm, v: Value, method: &str) -> JsResult<Source> {
    let bad = |vm: &mut Vm| vm.type_error(&format!("Failed to execute '{method}' on 'CanvasRenderingContext2D': The provided value is not of type '{IMAGE_TYPES}'."));
    if let Some(id) = node_id(v) {
        let tag = dom::with_tree(vm, |t| t.get(id).and_then(|n| n.as_element()).map(|e| dom::element_tag(t, e)));
        return match tag.as_deref() {
            Some("canvas") => {
                if let Some(ctx) = element_context(vm, id) {
                    let o = ctx.as_object().expect("context object");
                    let size = (o.get().host_data::<Context2D>().map_or(0, |c| c.canvas.width()), o.get().host_data::<Context2D>().map_or(0, |c| c.canvas.height()));
                    if size.0 == 0 || size.1 == 0 {
                        return Err(dom_exception(vm, "InvalidStateError", &format!("Failed to execute '{method}': The image argument is a canvas element with a width or height of 0.")));
                    }
                    return Ok(Source::Object(o));
                }
                let (w, h) = canvas_size(vm, id).unwrap_or((0, 0));
                if w == 0 || h == 0 {
                    return Err(dom_exception(vm, "InvalidStateError", &format!("Failed to execute '{method}': The image argument is a canvas element with a width or height of 0.")));
                }
                Ok(Source::Blank(w, h))
            }
            // Images and videos are not decoded for scripts yet
            Some("img" | "video" | "svg:image") => Ok(Source::Unavailable),
            _ => Err(bad(vm)),
        };
    }
    let Some(o) = v.as_object() else { return Err(bad(vm)) };
    if o.get().host_data::<Context2D>().is_some() {
        return Ok(Source::Object(o));
    }
    if let Some(off) = o.get().host_data::<OffscreenObject>() {
        let (w, h, ctx) = (off.width, off.height, off.context);
        if w == 0 || h == 0 {
            return Err(dom_exception(vm, "InvalidStateError", &format!("Failed to execute '{method}': The image argument is an OffscreenCanvas element with a width or height of 0.")));
        }
        return Ok(match ctx.as_object() {
            Some(c) => Source::Object(c),
            None => Source::Blank(w, h),
        });
    }
    if let Some(b) = o.get().host_data::<BitmapObject>() {
        if b.pixmap.is_none() {
            return Err(dom_exception(vm, "InvalidStateError", &format!("Failed to execute '{method}': The image source is detached.")));
        }
        return Ok(Source::Object(o));
    }
    Err(bad(vm))
}

// ---- state ----

fn save(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = this_ctx(vm, this)?;
    if c.saved.len() < 1024 {
        c.saved.push((c.fill, c.stroke));
        c.canvas.save();
    }
    Ok(Value::UNDEFINED)
}

fn restore(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = this_ctx(vm, this)?;
    if let Some((f, s)) = c.saved.pop() {
        c.fill = f;
        c.stroke = s;
        c.canvas.restore();
    }
    Ok(Value::UNDEFINED)
}

fn reset(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = this_ctx(vm, this)?;
    let (w, h) = (c.canvas.width(), c.canvas.height());
    resize_context(vm, this, w, h);
    Ok(Value::UNDEFINED)
}

fn is_context_lost(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    this_ctx(vm, this)?;
    Ok(Value::FALSE)
}

fn get_context_attributes(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = this_ctx(vm, this)?;
    let (alpha, will_read) = (!c.canvas.is_opaque(), c.will_read_frequently);
    let o = vm.new_object();
    vm.def_value(o, "alpha", Value::bool(alpha), PropFlags::DEFAULT);
    let srgb = vm.str_value("srgb");
    vm.def_value(o, "colorSpace", srgb, PropFlags::DEFAULT);
    vm.def_value(o, "desynchronized", Value::FALSE, PropFlags::DEFAULT);
    vm.def_value(o, "willReadFrequently", Value::bool(will_read), PropFlags::DEFAULT);
    Ok(Value::object(o))
}

fn get_canvas(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(this_ctx(vm, this)?.owner)
}

fn set_style(vm: &mut Vm, this: Value, args: &[Value], stroke: bool) -> JsResult<Value> {
    let v = arg(args, 0);
    let (style, shown) = if let Some(g) = data::<GradientObject>(v) {
        (Style::Gradient(g.0.clone()), v)
    } else if let Some(p) = data::<PatternObject>(v) {
        (Style::Pattern(p.0.clone()), v)
    } else {
        let s = vm.to_rust_string(v)?;
        let Some(color) = parse_color(&s) else {
            this_ctx(vm, this)?;
            return Ok(Value::UNDEFINED);
        };
        let serialized = serialize_color(color);
        // Usually already serialized (`'#ff0000'`): no new string
        let shown = if v.is_string() && serialized == s { v } else { vm.str_value(&serialized) };
        (Style::Color(color), shown)
    };
    let c = this_ctx(vm, this)?;
    if stroke {
        c.canvas.state.stroke = style;
        c.stroke = shown;
    } else {
        c.canvas.state.fill = style;
        c.fill = shown;
    }
    Ok(Value::UNDEFINED)
}

fn get_fill_style(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(this_ctx(vm, this)?.fill)
}

fn set_fill_style(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    set_style(vm, this, args, false)
}

fn get_stroke_style(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(this_ctx(vm, this)?.stroke)
}

fn set_stroke_style(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    set_style(vm, this, args, true)
}

/// Numeric state: getter, and a setter ignoring invalid values
macro_rules! number_prop {
    ($get:ident, $set:ident, |$s:ident| $field:expr, $valid:expr) => {
        fn $get(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
            let $s = &mut this_ctx(vm, this)?.canvas.state;
            Ok(number($field))
        }
        fn $set(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
            let v = vm.to_number(arg(args, 0))? as f32;
            let $s = &mut this_ctx(vm, this)?.canvas.state;
            let valid: fn(f32) -> bool = $valid;
            if valid(v) {
                $field = v;
            }
            Ok(Value::UNDEFINED)
        }
    };
}

number_prop!(get_line_width, set_line_width, |s| s.line_width, |v| v.is_finite() && v > 0.0);
number_prop!(get_miter_limit, set_miter_limit, |s| s.miter_limit, |v| v.is_finite() && v > 0.0);
number_prop!(get_line_dash_offset, set_line_dash_offset, |s| s.dash_offset, |v| v.is_finite());
number_prop!(get_global_alpha, set_global_alpha, |s| s.global_alpha, |v| (0.0..=1.0).contains(&v));
number_prop!(get_shadow_offset_x, set_shadow_offset_x, |s| s.shadow_offset.0, |v| v.is_finite());
number_prop!(get_shadow_offset_y, set_shadow_offset_y, |s| s.shadow_offset.1, |v| v.is_finite());
number_prop!(get_shadow_blur, set_shadow_blur, |s| s.shadow_blur, |v| v.is_finite() && v >= 0.0);

/// Enumerated state: the keywords and the values they stand for
macro_rules! keyword_prop {
    ($get:ident, $set:ident, |$s:ident| $field:expr, [$($name:literal => $val:expr),* $(,)?]) => {
        fn $get(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
            let $s = &mut this_ctx(vm, this)?.canvas.state;
            let current = $field;
            let name = [$(($name, $val)),*].iter().find(|(_, v)| *v == current).map_or("", |(n, _)| *n);
            Ok(atom(vm, name))
        }
        fn $set(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
            let name = vm.to_rust_string(arg(args, 0))?;
            let $s = &mut this_ctx(vm, this)?.canvas.state;
            if let Some((_, v)) = [$(($name, $val)),*].iter().find(|(n, _)| *n == name) {
                $field = *v;
            }
            Ok(Value::UNDEFINED)
        }
    };
}

keyword_prop!(get_line_cap, set_line_cap, |s| s.line_cap, ["butt" => sk::LineCap::Butt, "round" => sk::LineCap::Round, "square" => sk::LineCap::Square]);
keyword_prop!(get_line_join, set_line_join, |s| s.line_join, ["miter" => sk::LineJoin::Miter, "round" => sk::LineJoin::Round, "bevel" => sk::LineJoin::Bevel]);
keyword_prop!(get_text_align, set_text_align, |s| s.text_align, [
    "start" => TextAlign::Start, "end" => TextAlign::End, "left" => TextAlign::Left, "right" => TextAlign::Right, "center" => TextAlign::Center,
]);
keyword_prop!(get_text_baseline, set_text_baseline, |s| s.text_baseline, [
    "alphabetic" => TextBaseline::Alphabetic, "top" => TextBaseline::Top, "hanging" => TextBaseline::Hanging, "middle" => TextBaseline::Middle,
    "ideographic" => TextBaseline::Ideographic, "bottom" => TextBaseline::Bottom,
]);
keyword_prop!(get_direction, set_direction, |s| s.direction, ["inherit" => Direction::Inherit, "ltr" => Direction::Ltr, "rtl" => Direction::Rtl]);
keyword_prop!(get_smoothing_quality, set_smoothing_quality, |s| s.smoothing_quality, [
    "low" => SmoothingQuality::Low, "medium" => SmoothingQuality::Medium, "high" => SmoothingQuality::High,
]);
keyword_prop!(get_font_kerning, set_font_kerning, |s| s.font_kerning, ["auto" => "auto", "normal" => "normal", "none" => "none"]);
keyword_prop!(get_font_stretch, set_font_stretch, |s| s.font_stretch, [
    "ultra-condensed" => "ultra-condensed", "extra-condensed" => "extra-condensed", "condensed" => "condensed", "semi-condensed" => "semi-condensed",
    "normal" => "normal", "semi-expanded" => "semi-expanded", "expanded" => "expanded", "extra-expanded" => "extra-expanded", "ultra-expanded" => "ultra-expanded",
]);
keyword_prop!(get_font_variant_caps, set_font_variant_caps, |s| s.font_variant_caps, [
    "normal" => "normal", "small-caps" => "small-caps", "all-small-caps" => "all-small-caps", "petite-caps" => "petite-caps",
    "all-petite-caps" => "all-petite-caps", "unicase" => "unicase", "titling-caps" => "titling-caps",
]);
keyword_prop!(get_text_rendering, set_text_rendering, |s| s.text_rendering, [
    "auto" => "auto", "optimizeSpeed" => "optimizeSpeed", "optimizeLegibility" => "optimizeLegibility", "geometricPrecision" => "geometricPrecision",
]);

fn get_composite(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let name = this_ctx(vm, this)?.canvas.state.composite.name();
    Ok(atom(vm, name))
}

fn set_composite(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let name = vm.to_rust_string(arg(args, 0))?;
    let c = this_ctx(vm, this)?;
    if let Some(op) = Composite::parse(&name) {
        c.canvas.state.composite = op;
    }
    Ok(Value::UNDEFINED)
}

fn get_shadow_color(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let color = this_ctx(vm, this)?.canvas.state.shadow_color;
    let s = serialize_color(color);
    Ok(vm.str_value(&s))
}

fn set_shadow_color(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_rust_string(arg(args, 0))?;
    let c = this_ctx(vm, this)?;
    if let Some(color) = parse_color(&s) {
        c.canvas.state.shadow_color = color;
    }
    Ok(Value::UNDEFINED)
}

fn get_font(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let c = this_ctx(vm, this)?;
    let s = c.canvas.state.font.serialized.clone();
    Ok(vm.str_value(&s))
}

fn set_font(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_rust_string(arg(args, 0))?;
    let c = this_ctx(vm, this)?;
    if c.canvas.state.font.serialized != s {
        if let Some(f) = parse_font(&s) {
            c.canvas.state.font = f;
        }
    }
    Ok(Value::UNDEFINED)
}

fn get_letter_spacing(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let v = this_ctx(vm, this)?.canvas.state.letter_spacing;
    Ok(vm.str_value(&format_px(v)))
}

fn set_letter_spacing(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_rust_string(arg(args, 0))?;
    let st = &mut this_ctx(vm, this)?.canvas.state;
    if let Some(px) = parse_spacing(&s, st.font.size) {
        st.letter_spacing = px;
    }
    Ok(Value::UNDEFINED)
}

fn get_word_spacing(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let v = this_ctx(vm, this)?.canvas.state.word_spacing;
    Ok(vm.str_value(&format_px(v)))
}

fn set_word_spacing(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_rust_string(arg(args, 0))?;
    let st = &mut this_ctx(vm, this)?.canvas.state;
    if let Some(px) = parse_spacing(&s, st.font.size) {
        st.word_spacing = px;
    }
    Ok(Value::UNDEFINED)
}

fn get_filter(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let f = this_ctx(vm, this)?.canvas.state.filter.clone();
    Ok(vm.str_value(&f))
}

fn set_filter(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let s = vm.to_rust_string(arg(args, 0))?;
    this_ctx(vm, this)?.canvas.state.filter = Rc::from(s.trim());
    Ok(Value::UNDEFINED)
}

fn get_smoothing(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::bool(this_ctx(vm, this)?.canvas.state.image_smoothing))
}

fn set_smoothing(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    this_ctx(vm, this)?.canvas.state.image_smoothing = truthy(arg(args, 0));
    Ok(Value::UNDEFINED)
}

// ---- transforms ----

fn scale(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 2, "scale", "CanvasRenderingContext2D")?;
    let [x, y] = nums(vm, args)?;
    this_ctx(vm, this)?.canvas.transform(sk::Transform::from_scale(x, y));
    Ok(Value::UNDEFINED)
}

fn rotate(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 1, "rotate", "CanvasRenderingContext2D")?;
    let [a] = nums(vm, args)?;
    let (s, c) = a.sin_cos();
    this_ctx(vm, this)?.canvas.transform(sk::Transform::from_row(c, s, -s, c, 0.0, 0.0));
    Ok(Value::UNDEFINED)
}

fn translate(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 2, "translate", "CanvasRenderingContext2D")?;
    let [x, y] = nums(vm, args)?;
    this_ctx(vm, this)?.canvas.transform(sk::Transform::from_translate(x, y));
    Ok(Value::UNDEFINED)
}

fn transform(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 6, "transform", "CanvasRenderingContext2D")?;
    let [a, b, c, d, e, f] = nums(vm, args)?;
    this_ctx(vm, this)?.canvas.transform(sk::Transform::from_row(a, b, c, d, e, f));
    Ok(Value::UNDEFINED)
}

fn set_transform(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let t = if args.len() >= 6 {
        let m: [f32; 6] = nums(vm, args)?;
        if m.iter().any(|v| !v.is_finite()) {
            this_ctx(vm, this)?;
            return Ok(Value::UNDEFINED);
        }
        sk::Transform::from_row(m[0], m[1], m[2], m[3], m[4], m[5])
    } else {
        matrix_init(vm, arg(args, 0))?
    };
    this_ctx(vm, this)?.canvas.set_transform(t);
    Ok(Value::UNDEFINED)
}

fn reset_transform(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    this_ctx(vm, this)?.canvas.set_transform(sk::Transform::identity());
    Ok(Value::UNDEFINED)
}

fn get_transform(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let t = this_ctx(vm, this)?.canvas.state.transform;
    let parts = [t.sx, t.ky, t.kx, t.sy, t.tx, t.ty].map(number).to_vec();
    let a = Value::object(vm.new_array(parts));
    construct_global(vm, "DOMMatrix", &[a])
}

// ---- styles ----

fn new_gradient(vm: &mut Vm, this: Value, kind: GradientKind) -> JsResult<Value> {
    this_ctx(vm, this)?;
    let proto = protos(vm).gradient;
    let g = GradientObject(Rc::new(RefCell::new(Gradient::new(kind))));
    Ok(Value::object(new_host_object(vm, proto, Box::new(g))))
}

fn create_linear_gradient(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 4, "createLinearGradient", "CanvasRenderingContext2D")?;
    let [x0, y0, x1, y1] = finite_nums(vm, args)?;
    new_gradient(vm, this, GradientKind::Linear { x0, y0, x1, y1 })
}

fn create_radial_gradient(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 6, "createRadialGradient", "CanvasRenderingContext2D")?;
    let [x0, y0, r0, x1, y1, r1] = finite_nums(vm, args)?;
    if r0 < 0.0 || r1 < 0.0 {
        let r = if r0 < 0.0 { r0 } else { r1 };
        return Err(dom_exception(vm, "IndexSizeError", &format!("Failed to execute 'createRadialGradient' on 'CanvasRenderingContext2D': The {} provided is less than 0 ({r}).", if r0 < 0.0 { "r0" } else { "r1" })));
    }
    new_gradient(vm, this, GradientKind::Radial { x0, y0, r0, x1, y1, r1 })
}

fn create_conic_gradient(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 3, "createConicGradient", "CanvasRenderingContext2D")?;
    let [angle, x, y] = finite_nums(vm, args)?;
    new_gradient(vm, this, GradientKind::Conic { angle, x, y })
}

fn add_color_stop(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 2, "addColorStop", "CanvasGradient")?;
    let [offset] = finite_nums(vm, args)?;
    let color = vm.to_rust_string(arg(args, 1))?;
    let Some(g) = data::<GradientObject>(this) else { return Err(vm.type_error("Illegal invocation")) };
    if !(0.0..=1.0).contains(&offset) {
        return Err(dom_exception(vm, "IndexSizeError", &format!("Failed to execute 'addColorStop' on 'CanvasGradient': The provided value ({offset}) is outside the range (0.0, 1.0).")));
    }
    let Some(c) = parse_color(&color) else {
        return Err(dom_exception(vm, "SyntaxError", &format!("Failed to execute 'addColorStop' on 'CanvasGradient': The value provided ('{color}') could not be parsed as a color.")));
    };
    g.0.borrow_mut().add_color_stop(offset, c);
    Ok(Value::UNDEFINED)
}

fn create_pattern(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 2, "createPattern", "CanvasRenderingContext2D")?;
    let source = image_source(vm, arg(args, 0), "createPattern")?;
    let r = arg(args, 1);
    let rep = if r.is_null() { String::new() } else { vm.to_rust_string(r)? };
    let repetition = match rep.as_str() {
        "" | "repeat" => Repetition::Repeat,
        "repeat-x" => Repetition::RepeatX,
        "repeat-y" => Repetition::RepeatY,
        "no-repeat" => Repetition::NoRepeat,
        _ => {
            return Err(dom_exception(vm, "SyntaxError", &format!("Failed to execute 'createPattern' on 'CanvasRenderingContext2D': The provided type ('{rep}') is not one of 'repeat', 'no-repeat', 'repeat-x', or 'repeat-y'.")));
        }
    };
    this_ctx(vm, this)?;
    if matches!(source, Source::Unavailable) {
        return Ok(Value::NULL);
    }
    let Some(pixmap) = source.snapshot() else { return Ok(Value::NULL) };
    let pattern = PatternObject(Rc::new(CanvasPattern { pixmap, repetition, transform: RefCell::new(sk::Transform::identity()) }));
    let proto = protos(vm).pattern;
    Ok(Value::object(new_host_object(vm, proto, Box::new(pattern))))
}

fn pattern_set_transform(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let t = matrix_init(vm, arg(args, 0))?;
    let Some(p) = data::<PatternObject>(this) else { return Err(vm.type_error("Illegal invocation")) };
    *p.0.transform.borrow_mut() = t;
    Ok(Value::UNDEFINED)
}

fn set_line_dash(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 1, "setLineDash", "CanvasRenderingContext2D")?;
    let list = arg(args, 0);
    if !list.is_object() {
        return Err(vm.type_error("Failed to execute 'setLineDash' on 'CanvasRenderingContext2D': The provided value cannot be converted to a sequence."));
    }
    let it = vm.get_iterator(list)?;
    let mut dash = Vec::new();
    while let Some(v) = vm.iter_step(it)? {
        dash.push(vm.to_number(v)? as f32);
        if dash.len() > 1 << 16 {
            vm.iter_close(it)?;
            return Err(vm.range_error("Too many dash segments"));
        }
    }
    let c = this_ctx(vm, this)?;
    if dash.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return Ok(Value::UNDEFINED);
    }
    if dash.len() % 2 == 1 {
        dash.extend_from_within(..);
    }
    c.canvas.state.dash = dash;
    Ok(Value::UNDEFINED)
}

fn get_line_dash(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let dash: Vec<Value> = this_ctx(vm, this)?.canvas.state.dash.iter().map(|&v| number(v)).collect();
    Ok(Value::object(vm.new_array(dash)))
}

// ---- paths (on contexts and Path2D) ----

fn add_cmd(vm: &mut Vm, this: Value, cmd: PathCmd) -> JsResult<Value> {
    if let Some(c) = data::<Context2D>(this) {
        let t = c.canvas.state.transform;
        c.canvas.path.apply(&cmd, &t);
    } else if let Some(p) = data::<PathObject>(this) {
        // Keep a recorded path bounded (each command is ~40 bytes)
        if p.cmds.len() < 1 << 22 {
            p.cmds.push(cmd);
        }
    } else {
        return Err(vm.type_error("Illegal invocation"));
    }
    Ok(Value::UNDEFINED)
}

fn path_interface(this: Value) -> &'static str {
    if data::<PathObject>(this).is_some() {
        "Path2D"
    } else {
        "CanvasRenderingContext2D"
    }
}

fn close_path(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    add_cmd(vm, this, PathCmd::Close)
}

fn move_to(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 2, "moveTo", path_interface(this))?;
    let [x, y] = nums(vm, args)?;
    add_cmd(vm, this, PathCmd::MoveTo(x, y))
}

fn line_to(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 2, "lineTo", path_interface(this))?;
    let [x, y] = nums(vm, args)?;
    add_cmd(vm, this, PathCmd::LineTo(x, y))
}

fn quadratic_curve_to(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 4, "quadraticCurveTo", path_interface(this))?;
    let [cx, cy, x, y] = nums(vm, args)?;
    add_cmd(vm, this, PathCmd::QuadTo(cx, cy, x, y))
}

fn bezier_curve_to(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 6, "bezierCurveTo", path_interface(this))?;
    let [a, b, c, d, x, y] = nums(vm, args)?;
    add_cmd(vm, this, PathCmd::CubicTo(a, b, c, d, x, y))
}

fn negative_radius(vm: &mut Vm, method: &str, this: Value, r: f32) -> Value {
    let iface = path_interface(this);
    dom_exception(vm, "IndexSizeError", &format!("Failed to execute '{method}' on '{iface}': The radius provided ({r}) is negative."))
}

fn arc_to(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 5, "arcTo", path_interface(this))?;
    let [x1, y1, x2, y2, r] = nums(vm, args)?;
    if r < 0.0 {
        return Err(negative_radius(vm, "arcTo", this, r));
    }
    add_cmd(vm, this, PathCmd::ArcTo(x1, y1, x2, y2, r))
}

fn rect(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 4, "rect", path_interface(this))?;
    let [x, y, w, h] = nums(vm, args)?;
    add_cmd(vm, this, PathCmd::Rect(x, y, w, h))
}

fn arc(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 5, "arc", path_interface(this))?;
    let [x, y, r, start, end] = nums(vm, args)?;
    let ccw = truthy(arg(args, 5));
    if r < 0.0 {
        return Err(negative_radius(vm, "arc", this, r));
    }
    add_cmd(vm, this, PathCmd::Ellipse { x, y, rx: r, ry: r, rotation: 0.0, start, end, ccw })
}

fn ellipse(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 7, "ellipse", path_interface(this))?;
    let [x, y, rx, ry, rotation, start, end] = nums(vm, args)?;
    let ccw = truthy(arg(args, 7));
    if rx < 0.0 || ry < 0.0 {
        let iface = path_interface(this);
        let which = if rx < 0.0 { ("major", rx) } else { ("minor", ry) };
        return Err(dom_exception(vm, "IndexSizeError", &format!("Failed to execute 'ellipse' on '{iface}': The {}-axis radius provided ({}) is negative.", which.0, which.1)));
    }
    add_cmd(vm, this, PathCmd::Ellipse { x, y, rx, ry, rotation, start, end, ccw })
}

/// `roundRect` radii: a number, a DOMPointInit, or a list of 1 to 4 of them
/// (`None`: a non-finite radius, ignored)
fn round_rect_radii(vm: &mut Vm, this: Value, v: Value) -> JsResult<Option<[(f32, f32); 4]>> {
    let iface = path_interface(this);
    let one = |vm: &mut Vm, v: Value| -> JsResult<(f32, f32)> {
        if v.is_object() {
            let (x, y) = (vm.get_str(v, "x")?, vm.get_str(v, "y")?);
            let x = if x.is_undefined() { 0.0 } else { vm.to_number(x)? as f32 };
            let y = if y.is_undefined() { 0.0 } else { vm.to_number(y)? as f32 };
            Ok((x, y))
        } else {
            let n = vm.to_number(v)? as f32;
            Ok((n, n))
        }
    };
    let list: Vec<(f32, f32)> = if v.is_undefined() {
        vec![(0.0, 0.0)]
    } else if v.as_object().is_some_and(|o| matches!(o.get().kind, ObjectKind::Array { .. })) {
        let it = vm.get_iterator(v)?;
        let mut out = Vec::new();
        while let Some(x) = vm.iter_step(it)? {
            if out.len() == 4 {
                vm.iter_close(it)?;
                out.push((0.0, 0.0));
                break;
            }
            out.push(one(vm, x)?);
        }
        out
    } else {
        vec![one(vm, v)?]
    };
    if list.is_empty() || list.len() > 4 {
        let msg = format!("Failed to execute 'roundRect' on '{iface}': {} radii provided. Between one and four radii are necessary.", list.len());
        return Err(vm.range_error(&msg));
    }
    if list.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
        return Ok(None);
    }
    if let Some(&(x, y)) = list.iter().find(|(x, y)| *x < 0.0 || *y < 0.0) {
        let r = if x < 0.0 { x } else { y };
        let msg = format!("Failed to execute 'roundRect' on '{iface}': Radius value {r} is negative.");
        return Err(vm.range_error(&msg));
    }
    // [top-left, top-right, bottom-right, bottom-left]
    Ok(Some(match list[..] {
        [a] => [a; 4],
        [a, b] => [a, b, a, b],
        [a, b, c] => [a, b, c, b],
        [a, b, c, d] => [a, b, c, d],
        _ => unreachable!(),
    }))
}

fn round_rect(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 4, "roundRect", path_interface(this))?;
    let [x, y, w, h] = nums(vm, args)?;
    let Some(radii) = round_rect_radii(vm, this, arg(args, 4))? else {
        // A non-finite radius: nothing is added
        return Ok(Value::UNDEFINED);
    };
    add_cmd(vm, this, PathCmd::RoundRect(x, y, w, h, radii))
}

fn begin_path(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    this_ctx(vm, this)?.canvas.path = PathData::new();
    Ok(Value::UNDEFINED)
}

/// `(path?, ...rest)`: a Path2D first argument, built under the current
/// transform, and the arguments after it
fn split_path(args: &[Value]) -> (Option<Vec<PathCmd>>, &[Value]) {
    match data::<PathObject>(arg(args, 0)) {
        Some(p) => (Some(p.cmds.clone()), args.get(1..).unwrap_or(&[])),
        None => (None, args),
    }
}

fn device_path(c: &Context2D, cmds: &Option<Vec<PathCmd>>) -> Option<Option<sk::Path>> {
    cmds.as_ref().map(|cmds| build_path(cmds, &c.canvas.state.transform))
}

fn fill(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (cmds, rest) = split_path(args);
    let rule = fill_rule(vm, arg(rest, 0))?;
    let c = this_ctx(vm, this)?;
    match device_path(c, &cmds) {
        Some(Some(p)) => c.canvas.fill_path(Some(&p), rule),
        Some(None) => {}
        None => c.canvas.fill_path(None, rule),
    }
    account(vm, this);
    Ok(Value::UNDEFINED)
}

fn stroke(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (cmds, _) = split_path(args);
    let c = this_ctx(vm, this)?;
    match device_path(c, &cmds) {
        Some(Some(p)) => c.canvas.stroke_path(Some(&p)),
        Some(None) => {}
        None => c.canvas.stroke_path(None),
    }
    account(vm, this);
    Ok(Value::UNDEFINED)
}

fn clip(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (cmds, rest) = split_path(args);
    let rule = fill_rule(vm, arg(rest, 0))?;
    let c = this_ctx(vm, this)?;
    match device_path(c, &cmds) {
        Some(Some(p)) => c.canvas.clip(Some(&p), rule),
        // An empty Path2D clips everything away
        Some(None) => c.canvas.clip_to_nothing(),
        None => c.canvas.clip(None, rule),
    }
    Ok(Value::UNDEFINED)
}

fn is_point_in_path(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (cmds, rest) = split_path(args);
    require(vm, rest, 2, "isPointInPath", "CanvasRenderingContext2D")?;
    let [x, y] = nums(vm, rest)?;
    let rule = fill_rule(vm, arg(rest, 2))?;
    let c = this_ctx(vm, this)?;
    let inside = match device_path(c, &cmds) {
        Some(Some(p)) => c.canvas.is_point_in_path(Some(&p), x, y, rule),
        Some(None) => false,
        None => c.canvas.is_point_in_path(None, x, y, rule),
    };
    Ok(Value::bool(inside))
}

fn is_point_in_stroke(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (cmds, rest) = split_path(args);
    require(vm, rest, 2, "isPointInStroke", "CanvasRenderingContext2D")?;
    let [x, y] = nums(vm, rest)?;
    let c = this_ctx(vm, this)?;
    let inside = match device_path(c, &cmds) {
        Some(Some(p)) => c.canvas.is_point_in_stroke(Some(&p), x, y),
        Some(None) => false,
        None => c.canvas.is_point_in_stroke(None, x, y),
    };
    Ok(Value::bool(inside))
}

fn noop(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    this_ctx(vm, this)?;
    Ok(Value::UNDEFINED)
}

// ---- rectangles ----

fn fill_rect(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 4, "fillRect", "CanvasRenderingContext2D")?;
    let [x, y, w, h] = nums(vm, args)?;
    draw(vm, this, |c| c.fill_rect(x, y, w, h))
}

fn stroke_rect(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 4, "strokeRect", "CanvasRenderingContext2D")?;
    let [x, y, w, h] = nums(vm, args)?;
    draw(vm, this, |c| c.stroke_rect(x, y, w, h))
}

fn clear_rect(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 4, "clearRect", "CanvasRenderingContext2D")?;
    let [x, y, w, h] = nums(vm, args)?;
    draw(vm, this, |c| c.clear_rect(x, y, w, h))
}

// ---- text ----

fn text_args(vm: &mut Vm, args: &[Value], method: &str) -> JsResult<(String, f32, f32, Option<f32>)> {
    require(vm, args, 3, method, "CanvasRenderingContext2D")?;
    let text = vm.to_rust_string(arg(args, 0))?;
    let x = vm.to_number(arg(args, 1))? as f32;
    let y = vm.to_number(arg(args, 2))? as f32;
    let max = if arg(args, 3).is_undefined() { None } else { Some(vm.to_number(arg(args, 3))? as f32) };
    Ok((text, x, y, max))
}

fn fill_text(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (text, x, y, max) = text_args(vm, args, "fillText")?;
    draw(vm, this, |c| c.fill_text(&text, x, y, max))
}

fn stroke_text(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let (text, x, y, max) = text_args(vm, args, "strokeText")?;
    draw(vm, this, |c| c.stroke_text(&text, x, y, max))
}

fn measure_text(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 1, "measureText", "CanvasRenderingContext2D")?;
    let text = vm.to_rust_string(arg(args, 0))?;
    let m = this_ctx(vm, this)?.canvas.measure_text(&text);
    let proto = protos(vm).metrics;
    let o = vm.new_object_with(Some(proto), ObjectKind::Ordinary);
    for (name, v) in [
        ("width", m.width),
        ("actualBoundingBoxLeft", m.actual_left),
        ("actualBoundingBoxRight", m.actual_right),
        ("fontBoundingBoxAscent", m.font_ascent),
        ("fontBoundingBoxDescent", m.font_descent),
        ("actualBoundingBoxAscent", m.actual_ascent),
        ("actualBoundingBoxDescent", m.actual_descent),
        ("emHeightAscent", m.em_ascent),
        ("emHeightDescent", m.em_descent),
        ("hangingBaseline", m.hanging),
        ("alphabeticBaseline", m.alphabetic),
        ("ideographicBaseline", m.ideographic),
    ] {
        vm.def_value(o, name, number(v), PropFlags::DEFAULT);
    }
    Ok(Value::object(o))
}

// ---- images ----

fn draw_image(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    if !matches!(args.len(), 3 | 5) && args.len() < 9 {
        let msg = format!("Failed to execute 'drawImage' on 'CanvasRenderingContext2D': Valid arities are: [3, 5, 9], but {} arguments provided.", args.len());
        return Err(vm.type_error(&msg));
    }
    let source = image_source(vm, args[0], "drawImage")?;
    let rest = &args[1..];
    let (iw, ih) = {
        let (w, h) = source.size();
        (w as f32, h as f32)
    };
    let [sx, sy, sw, sh, dx, dy, dw, dh] = match rest.len() {
        2 => {
            let [dx, dy] = nums(vm, rest)?;
            [0.0, 0.0, iw, ih, dx, dy, iw, ih]
        }
        4 => {
            let [dx, dy, dw, dh] = nums(vm, rest)?;
            [0.0, 0.0, iw, ih, dx, dy, dw, dh]
        }
        _ => nums::<8>(vm, rest)?,
    };
    this_ctx(vm, this)?;
    let same = matches!(source, Source::Object(o) if Value::object(o) == this);
    if same {
        // Drawing a canvas onto itself: copy it first
        let Some(snapshot) = source.snapshot() else { return Ok(Value::UNDEFINED) };
        return draw(vm, this, |c| c.draw_image(&snapshot, sx, sy, sw, sh, dx, dy, dw, dh));
    }
    match source.pixmap() {
        Some(p) => draw(vm, this, |c| c.draw_image(p, sx, sy, sw, sh, dx, dy, dw, dh)),
        None => {
            // Never drawn on: transparent, unless opaque (black)
            if let Source::Object(o) = &source {
                if o.get().host_data::<Context2D>().is_some_and(|c| c.canvas.is_opaque()) {
                    if let Some(snapshot) = source.snapshot() {
                        return draw(vm, this, |c| c.draw_image(&snapshot, sx, sy, sw, sh, dx, dy, dw, dh));
                    }
                }
            }
            Ok(Value::UNDEFINED)
        }
    }
}

/// Pixel rectangles: negative sizes extend left/up, as `getImageData` does
fn normalize_rect(x: i64, y: i64, w: i64, h: i64) -> (i64, i64, i64, i64) {
    let (x, w) = if w < 0 { (x + w, -w) } else { (x, w) };
    let (y, h) = if h < 0 { (y + h, -h) } else { (y, h) };
    (x, y, w, h)
}

fn to_long(vm: &mut Vm, v: Value) -> JsResult<i64> {
    let n = vm.to_number(v)?;
    if !n.is_finite() {
        return Err(vm.type_error("The provided double value is non-finite."));
    }
    Ok(n.trunc().clamp(-(1i64 << 31) as f64, ((1i64 << 31) - 1) as f64) as i64)
}

fn check_pixels(vm: &mut Vm, w: i64, h: i64, method: &str) -> JsResult<()> {
    if w == 0 || h == 0 {
        let which = if w == 0 { "width" } else { "height" };
        return Err(dom_exception(vm, "IndexSizeError", &format!("Failed to execute '{method}' on 'CanvasRenderingContext2D': The source {which} is 0.")));
    }
    if (w.unsigned_abs()).saturating_mul(h.unsigned_abs()) > MAX_PIXELS {
        return Err(vm.range_error("Out of memory at ImageData creation"));
    }
    Ok(())
}

/// An `ImageData` of `w`×`h` holding `bytes`
fn new_image_data(vm: &mut Vm, bytes: Vec<u8>, w: u32, h: u32) -> JsResult<Value> {
    let buf = dom::new_array_buffer(vm, bytes);
    let pixels = construct_global(vm, "Uint8ClampedArray", &[buf])?;
    construct_global(vm, "ImageData", &[pixels, Value::number(w as f64), Value::number(h as f64)])
}

fn get_image_data(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 4, "getImageData", "CanvasRenderingContext2D")?;
    let mut r = [0i64; 4];
    for (i, v) in r.iter_mut().enumerate() {
        *v = to_long(vm, args[i])?;
    }
    check_pixels(vm, r[2], r[3], "getImageData")?;
    let (x, y, w, h) = normalize_rect(r[0], r[1], r[2], r[3]);
    let bytes = this_ctx(vm, this)?.canvas.get_image_data(x as i32, y as i32, w as u32, h as u32);
    new_image_data(vm, bytes, w as u32, h as u32)
}

fn create_image_data(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 1, "createImageData", "CanvasRenderingContext2D")?;
    this_ctx(vm, this)?;
    let (w, h) = if args[0].is_object() {
        let w = vm.get_str(args[0], "width")?;
        let h = vm.get_str(args[0], "height")?;
        (to_long(vm, w)?, to_long(vm, h)?)
    } else {
        require(vm, args, 2, "createImageData", "CanvasRenderingContext2D")?;
        (to_long(vm, args[0])?, to_long(vm, args[1])?)
    };
    check_pixels(vm, w, h, "createImageData")?;
    let (w, h) = (w.unsigned_abs() as u32, h.unsigned_abs() as u32);
    new_image_data(vm, vec![0; w as usize * h as usize * 4], w, h)
}

/// The bytes of a typed array, borrowed
fn typed_bytes<'a>(v: Value) -> Option<&'a [u8]> {
    let o = v.as_object()?;
    let ObjectKind::TypedArray(view) = &o.get_mut_detached().kind else { return None };
    let ObjectKind::ArrayBuffer(b) = &view.buffer.get_mut_detached().kind else { return None };
    let start = view.offset as usize;
    b.get(start..start + view.length as usize * view.kind.size())
}

fn put_image_data(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 3, "putImageData", "CanvasRenderingContext2D")?;
    let image = args[0];
    if !image.is_object() {
        return Err(vm.type_error("Failed to execute 'putImageData' on 'CanvasRenderingContext2D': parameter 1 is not of type 'ImageData'."));
    }
    let pixels = vm.get_str(image, "data")?;
    let w = vm.get_str(image, "width")?;
    let h = vm.get_str(image, "height")?;
    let (w, h) = (to_long(vm, w)?.max(0) as u32, to_long(vm, h)?.max(0) as u32);
    let dx = to_long(vm, args[1])?;
    let dy = to_long(vm, args[2])?;
    let dirty = if args.len() >= 7 {
        let mut d = [0i64; 4];
        for (i, v) in d.iter_mut().enumerate() {
            *v = to_long(vm, args[3 + i])?;
        }
        (d[0] as i32, d[1] as i32, d[2] as i32, d[3] as i32)
    } else {
        (0, 0, w as i32, h as i32)
    };
    let c = this_ctx(vm, this)?;
    let Some(bytes) = typed_bytes(pixels) else {
        return Err(vm.type_error("Failed to execute 'putImageData' on 'CanvasRenderingContext2D': parameter 1 is not of type 'ImageData'."));
    };
    if (bytes.len() as u64) < w as u64 * h as u64 * 4 {
        return Err(dom_exception(vm, "InvalidStateError", "Failed to execute 'putImageData': The ImageData's data is too small."));
    }
    c.canvas.put_image_data(bytes, w, h, dx as i32, dy as i32, dirty);
    account(vm, this);
    Ok(Value::UNDEFINED)
}

// ---- encoding (`toDataURL`, `toBlob`, `convertToBlob`) ----

/// The PNG of a canvas element, OffscreenCanvas, context or ImageBitmap
/// (`None` when it has no pixels: a zero width or height)
fn encode_png(vm: &mut Vm, v: Value) -> JsResult<Option<Vec<u8>>> {
    if let Some(id) = node_id(v) {
        if let Some(ctx) = element_context(vm, id) {
            return encode_png(vm, ctx);
        }
        let Some((w, h)) = canvas_size(vm, id) else { return Err(vm.type_error("Illegal invocation")) };
        return Ok(if w == 0 || h == 0 { None } else { Canvas2D::new(w, h).to_png() });
    }
    if let Some(c) = data::<Context2D>(v) {
        return Ok(if c.canvas.width() == 0 || c.canvas.height() == 0 { None } else { c.canvas.to_png() });
    }
    if let Some(off) = data::<OffscreenObject>(v) {
        if let Some(ctx) = off.context.as_object() {
            return encode_png(vm, Value::object(ctx));
        }
        return Ok(if off.width == 0 || off.height == 0 { None } else { Canvas2D::new(off.width, off.height).to_png() });
    }
    if let Some(b) = data::<BitmapObject>(v) {
        return Ok(b.pixmap.as_ref().and_then(|p| p.encode_png().ok()));
    }
    Err(vm.type_error("Illegal invocation"))
}

/// `__fosCanvasPNG(canvas)`: its PNG as an ArrayBuffer, or null
fn canvas_png(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(match encode_png(vm, arg(args, 0))? {
        Some(png) => dom::new_array_buffer(vm, png),
        None => Value::NULL,
    })
}

/// `__fosCanvasDataURL(canvas)`: `toDataURL` (PNG, the one format every
/// browser must support; other types fall back to it as the spec allows)
fn canvas_data_url(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let url = match encode_png(vm, arg(args, 0))? {
        Some(png) => format!("data:image/png;base64,{}", base64(&png)),
        None => "data:,".to_string(),
    };
    Ok(vm.str_value(&url))
}

fn base64(bytes: &[u8]) -> String {
    const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16 | (*chunk.get(1).unwrap_or(&0) as u32) << 8 | *chunk.get(2).unwrap_or(&0) as u32;
        for k in 0..4 {
            out.push(if k <= chunk.len() { B64[(n >> (18 - 6 * k) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

// ---- Path2D ----

fn path_call(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("Failed to construct 'Path2D': Please use the 'new' operator, this DOM object constructor cannot be called as a function."))
}

fn prototype_of(vm: &mut Vm, new_target: Value, default: Gc<JsObject>) -> JsResult<Gc<JsObject>> {
    if new_target.is_object() {
        if let Some(p) = vm.get_str(new_target, "prototype")?.as_object() {
            return Ok(p);
        }
    }
    Ok(default)
}

fn path_construct(vm: &mut Vm, new_target: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let source = arg(args, 0);
    let cmds = if source.is_undefined() {
        Vec::new()
    } else if let Some(p) = data::<PathObject>(source) {
        p.cmds.clone()
    } else {
        let d = vm.to_rust_string(source)?;
        parse_svg_path(&d)
    };
    let default = protos(vm).path;
    let proto = prototype_of(vm, new_target, default)?;
    Ok(Value::object(new_host_object(vm, proto, Box::new(PathObject { cmds }))))
}

fn add_path(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 1, "addPath", "Path2D")?;
    let Some(other) = data::<PathObject>(args[0]) else {
        return Err(vm.type_error("Failed to execute 'addPath' on 'Path2D': parameter 1 is not of type 'Path2D'."));
    };
    let cmds = Rc::new(other.cmds.clone());
    let t = matrix_init(vm, arg(args, 1))?;
    add_cmd(vm, this, PathCmd::Path(cmds, t))
}

// ---- OffscreenCanvas ----

fn offscreen_call(vm: &mut Vm, _: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Err(vm.type_error("Failed to construct 'OffscreenCanvas': Please use the 'new' operator, this DOM object constructor cannot be called as a function."))
}

/// `[EnforceRange] unsigned long long`
fn dimension(vm: &mut Vm, v: Value) -> JsResult<u32> {
    let n = vm.to_number(v)?;
    if !n.is_finite() || n < 0.0 || n > 9007199254740991.0 {
        return Err(vm.type_error("Value is outside the 'unsigned long long' value range."));
    }
    Ok(n.trunc().min(u32::MAX as f64) as u32)
}

fn offscreen_construct(vm: &mut Vm, new_target: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    require(vm, args, 2, "OffscreenCanvas", "OffscreenCanvas")?;
    let width = dimension(vm, args[0])?;
    let height = dimension(vm, args[1])?;
    let default = protos(vm).offscreen;
    let proto = prototype_of(vm, new_target, default)?;
    Ok(Value::object(new_host_object(vm, proto, Box::new(OffscreenObject { width, height, context: Value::UNDEFINED }))))
}

fn this_offscreen<'a>(vm: &mut Vm, this: Value) -> JsResult<&'a mut OffscreenObject> {
    data::<OffscreenObject>(this).ok_or_else(|| vm.type_error("Illegal invocation"))
}

fn offscreen_width(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(this_offscreen(vm, this)?.width as f64))
}

fn offscreen_height(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(this_offscreen(vm, this)?.height as f64))
}

fn offscreen_resize(vm: &mut Vm, this: Value, args: &[Value], height: bool) -> JsResult<Value> {
    let n = dimension(vm, arg(args, 0))?;
    let o = this_offscreen(vm, this)?;
    if height {
        o.height = n;
    } else {
        o.width = n;
    }
    let (w, h, ctx) = (o.width, o.height, o.context);
    if ctx.is_object() {
        resize_context(vm, ctx, w, h);
    }
    Ok(Value::UNDEFINED)
}

fn set_offscreen_width(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    offscreen_resize(vm, this, args, false)
}

fn set_offscreen_height(vm: &mut Vm, this: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    offscreen_resize(vm, this, args, true)
}

// ---- ImageBitmap ----

fn new_bitmap(vm: &mut Vm, pixmap: sk::Pixmap) -> Value {
    let size = pixmap.data().len();
    let proto = protos(vm).bitmap;
    let o = new_host_object(vm, proto, Box::new(BitmapObject { pixmap: Some(pixmap) }));
    vm.heap.note_growth(size);
    o.set_size(std::mem::size_of::<JsObject>() + size);
    Value::object(o)
}

fn this_bitmap<'a>(vm: &mut Vm, this: Value) -> JsResult<&'a mut BitmapObject> {
    data::<BitmapObject>(this).ok_or_else(|| vm.type_error("Illegal invocation"))
}

fn bitmap_width(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(this_bitmap(vm, this)?.pixmap.as_ref().map_or(0, |p| p.width()) as f64))
}

fn bitmap_height(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    Ok(Value::number(this_bitmap(vm, this)?.pixmap.as_ref().map_or(0, |p| p.height()) as f64))
}

fn bitmap_close(vm: &mut Vm, this: Value, _: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    this_bitmap(vm, this)?.pixmap = None;
    if let Some(o) = this.as_object() {
        o.set_size(std::mem::size_of::<JsObject>());
    }
    Ok(Value::UNDEFINED)
}

/// Decode a PNG, JPEG, GIF or WebP image (size-limited by the decoder)
pub fn decode_image(bytes: &[u8]) -> Option<sk::Pixmap> {
    let img = fos_render::image::decoders::decode(bytes).ok()?;
    pixmap_from_rgba(&img.pixels, img.width, img.height)
}

/// Premultiplied pixels from straight RGBA
fn pixmap_from_rgba(rgba: &[u8], w: u32, h: u32) -> Option<sk::Pixmap> {
    let mut p = sk::Pixmap::new(w, h)?;
    let n = w as usize * h as usize * 4;
    let src = rgba.get(..n)?;
    for (d, s) in p.data_mut().chunks_exact_mut(4).zip(src.chunks_exact(4)) {
        let a = s[3] as u32;
        for i in 0..3 {
            d[i] = ((s[i] as u32 * a + 127) / 255) as u8;
        }
        d[3] = s[3];
    }
    Some(p)
}

/// `__fosImageBitmap(source, sx, sy, sw, sh)`: an ImageBitmap of a canvas,
/// OffscreenCanvas, ImageBitmap, ImageData or encoded image bytes (a
/// Blob's), optionally cropped
fn image_bitmap(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let source = arg(args, 0);
    let encoded = source.as_object().and_then(|o| match &o.get_mut_detached().kind {
        ObjectKind::ArrayBuffer(b) => Some(&b[..]),
        _ => None,
    });
    let image = if let Some(bytes) = encoded {
        // Encoded bytes (a Blob's)
        let Some(p) = decode_image(bytes) else {
            return Err(dom_exception(vm, "InvalidStateError", "The source image could not be decoded."));
        };
        Some(p)
    } else if data::<Context2D>(source).is_some() || data::<OffscreenObject>(source).is_some() || data::<BitmapObject>(source).is_some() || node_id(source).is_some() {
        let s = image_source(vm, source, "createImageBitmap")?;
        if matches!(s, Source::Unavailable) {
            return Err(dom_exception(vm, "InvalidStateError", "The source image could not be decoded."));
        }
        s.snapshot()
    } else if source.is_object() {
        // ImageData
        let pixels = vm.get_str(source, "data")?;
        let w = vm.get_str(source, "width")?;
        let h = vm.get_str(source, "height")?;
        let (w, h) = (to_long(vm, w)?, to_long(vm, h)?);
        if w <= 0 || h <= 0 || (w as u64 * h as u64) > MAX_PIXELS {
            return Err(dom_exception(vm, "InvalidStateError", "The source image has no pixels."));
        }
        let Some(bytes) = typed_bytes(pixels) else {
            return Err(vm.type_error("Failed to execute 'createImageBitmap': The provided value is not of type 'ImageBitmapSource'."));
        };
        pixmap_from_rgba(bytes, w as u32, h as u32)
    } else {
        return Err(vm.type_error("Failed to execute 'createImageBitmap': The provided value is not of type 'ImageBitmapSource'."));
    };
    let Some(mut image) = image else { return Err(dom_exception(vm, "InvalidStateError", "The source image has no pixels.")) };
    // Crop (areas outside the source are transparent)
    if args.len() >= 5 {
        let mut r = [0i64; 4];
        for (i, v) in r.iter_mut().enumerate() {
            *v = to_long(vm, args[1 + i])?;
        }
        if r[2] == 0 || r[3] == 0 {
            return Err(vm.range_error(&format!("The crop rect {} is 0.", if r[2] == 0 { "width" } else { "height" })));
        }
        let (x, y, w, h) = normalize_rect(r[0], r[1], r[2], r[3]);
        if (w as u64 * h as u64) > MAX_PIXELS {
            return Err(vm.range_error("Out of memory at ImageBitmap creation"));
        }
        let Some(mut cropped) = sk::Pixmap::new(w as u32, h as u32) else { return Err(vm.range_error("Out of memory at ImageBitmap creation")) };
        let paint = sk::PixmapPaint { blend_mode: sk::BlendMode::Source, ..Default::default() };
        cropped.draw_pixmap(-(x as i32), -(y as i32), image.as_ref(), &paint, sk::Transform::identity(), None);
        image = cropped;
    }
    Ok(new_bitmap(vm, image))
}

/// `__fosTransferToImageBitmap(offscreen)`: its bitmap, leaving it blank
fn transfer_to_image_bitmap(vm: &mut Vm, _: Value, args: &[Value], _: Gc<JsObject>) -> JsResult<Value> {
    let target = arg(args, 0);
    let o = this_offscreen(vm, target)?;
    let (w, h, ctx) = (o.width, o.height, o.context);
    if w == 0 || h == 0 {
        return Err(dom_exception(vm, "InvalidStateError", "Cannot transfer an ImageBitmap from an OffscreenCanvas with no width or height."));
    }
    let Some(c) = data::<Context2D>(ctx) else {
        return Err(dom_exception(vm, "InvalidStateError", "Cannot transfer an ImageBitmap from an OffscreenCanvas with no context."));
    };
    let Some(pixmap) = c.canvas.snapshot() else { return Err(vm.range_error("Out of memory at ImageBitmap creation")) };
    let bitmap = new_bitmap(vm, pixmap);
    // The canvas starts over with a blank bitmap (its state stays)
    if let Some(c) = data::<Context2D>(ctx) {
        c.canvas.clear_bitmap();
    }
    account(vm, ctx);
    Ok(bitmap)
}

// ---- install ----

fn proto_with_methods(vm: &mut Vm, name: &str, methods: &[(&str, u32, NativeFn)], accessors: &[(&str, NativeFn, Option<NativeFn>)]) -> Gc<JsObject> {
    let object_proto = vm.realm.object_proto;
    let proto = dom::proto_object(vm, object_proto);
    dom::interface(vm, name, proto);
    for &(n, len, f) in methods {
        vm.def_method(proto, n, len, f);
    }
    for &(n, get, set) in accessors {
        vm.def_accessor(proto, n, get, set);
    }
    proto
}

/// Path methods shared by contexts and Path2D (CanvasPath)
const PATH_METHODS: &[(&str, u32, NativeFn)] = &[
    ("closePath", 0, close_path),
    ("moveTo", 2, move_to),
    ("lineTo", 2, line_to),
    ("quadraticCurveTo", 4, quadratic_curve_to),
    ("bezierCurveTo", 6, bezier_curve_to),
    ("arcTo", 5, arc_to),
    ("rect", 4, rect),
    ("roundRect", 4, round_rect),
    ("arc", 5, arc),
    ("ellipse", 7, ellipse),
];

const CONTEXT_METHODS: &[(&str, u32, NativeFn)] = &[
    ("save", 0, save),
    ("restore", 0, restore),
    ("reset", 0, reset),
    ("isContextLost", 0, is_context_lost),
    ("getContextAttributes", 0, get_context_attributes),
    ("scale", 2, scale),
    ("rotate", 1, rotate),
    ("translate", 2, translate),
    ("transform", 6, transform),
    ("setTransform", 0, set_transform),
    ("getTransform", 0, get_transform),
    ("resetTransform", 0, reset_transform),
    ("createLinearGradient", 4, create_linear_gradient),
    ("createRadialGradient", 6, create_radial_gradient),
    ("createConicGradient", 3, create_conic_gradient),
    ("createPattern", 2, create_pattern),
    ("clearRect", 4, clear_rect),
    ("fillRect", 4, fill_rect),
    ("strokeRect", 4, stroke_rect),
    ("beginPath", 0, begin_path),
    ("fill", 0, fill),
    ("stroke", 0, stroke),
    ("clip", 0, clip),
    ("isPointInPath", 2, is_point_in_path),
    ("isPointInStroke", 2, is_point_in_stroke),
    ("drawFocusIfNeeded", 1, noop),
    ("scrollPathIntoView", 0, noop),
    ("fillText", 3, fill_text),
    ("strokeText", 3, stroke_text),
    ("measureText", 1, measure_text),
    ("drawImage", 3, draw_image),
    ("createImageData", 1, create_image_data),
    ("getImageData", 4, get_image_data),
    ("putImageData", 3, put_image_data),
    ("setLineDash", 1, set_line_dash),
    ("getLineDash", 0, get_line_dash),
];

const CONTEXT_ACCESSORS: &[(&str, NativeFn, Option<NativeFn>)] = &[
    ("globalAlpha", get_global_alpha, Some(set_global_alpha)),
    ("globalCompositeOperation", get_composite, Some(set_composite)),
    ("imageSmoothingEnabled", get_smoothing, Some(set_smoothing)),
    ("imageSmoothingQuality", get_smoothing_quality, Some(set_smoothing_quality)),
    ("strokeStyle", get_stroke_style, Some(set_stroke_style)),
    ("fillStyle", get_fill_style, Some(set_fill_style)),
    ("shadowOffsetX", get_shadow_offset_x, Some(set_shadow_offset_x)),
    ("shadowOffsetY", get_shadow_offset_y, Some(set_shadow_offset_y)),
    ("shadowBlur", get_shadow_blur, Some(set_shadow_blur)),
    ("shadowColor", get_shadow_color, Some(set_shadow_color)),
    ("filter", get_filter, Some(set_filter)),
    ("lineWidth", get_line_width, Some(set_line_width)),
    ("lineCap", get_line_cap, Some(set_line_cap)),
    ("lineJoin", get_line_join, Some(set_line_join)),
    ("miterLimit", get_miter_limit, Some(set_miter_limit)),
    ("lineDashOffset", get_line_dash_offset, Some(set_line_dash_offset)),
    ("font", get_font, Some(set_font)),
    ("textAlign", get_text_align, Some(set_text_align)),
    ("textBaseline", get_text_baseline, Some(set_text_baseline)),
    ("direction", get_direction, Some(set_direction)),
    ("letterSpacing", get_letter_spacing, Some(set_letter_spacing)),
    ("wordSpacing", get_word_spacing, Some(set_word_spacing)),
    ("fontKerning", get_font_kerning, Some(set_font_kerning)),
    ("fontStretch", get_font_stretch, Some(set_font_stretch)),
    ("fontVariantCaps", get_font_variant_caps, Some(set_font_variant_caps)),
    ("textRendering", get_text_rendering, Some(set_text_rendering)),
    ("canvas", get_canvas, None),
];

/// Install the canvas interfaces and natives (before the bootstrap runs)
pub fn install(vm: &mut Vm) {
    let mut contexts = [None; 2];
    for (i, name) in ["CanvasRenderingContext2D", "OffscreenCanvasRenderingContext2D"].into_iter().enumerate() {
        let proto = proto_with_methods(vm, name, CONTEXT_METHODS, CONTEXT_ACCESSORS);
        for &(n, len, f) in PATH_METHODS {
            vm.def_method(proto, n, len, f);
        }
        contexts[i] = Some(proto);
    }
    let gradient = proto_with_methods(vm, "CanvasGradient", &[("addColorStop", 2, add_color_stop)], &[]);
    let pattern = proto_with_methods(vm, "CanvasPattern", &[("setTransform", 0, pattern_set_transform)], &[]);
    let metrics = proto_with_methods(vm, "TextMetrics", &[], &[]);
    let bitmap = proto_with_methods(vm, "ImageBitmap", &[("close", 0, bitmap_close)], &[("width", bitmap_width, None), ("height", bitmap_height, None)]);

    let object_proto = vm.realm.object_proto;
    let path = dom::proto_object(vm, object_proto);
    vm.def_ctor("Path2D", 0, path_call, Some(path_construct), path);
    for &(n, len, f) in PATH_METHODS {
        vm.def_method(path, n, len, f);
    }
    vm.def_method(path, "addPath", 1, add_path);
    let offscreen = dom::proto_object(vm, object_proto);
    vm.def_ctor("OffscreenCanvas", 2, offscreen_call, Some(offscreen_construct), offscreen);
    vm.def_accessor(offscreen, "width", offscreen_width, Some(set_offscreen_width));
    vm.def_accessor(offscreen, "height", offscreen_height, Some(set_offscreen_height));
    for (proto, name) in [(path, "Path2D"), (offscreen, "OffscreenCanvas")] {
        let tag = vm.str_value(name);
        let key = fos_jsvm::object::PropertyKey::Symbol(vm.sym.to_string_tag);
        vm.define_value(proto, key, tag, PropFlags::READONLY_HIDDEN);
    }

    host(vm).protos = Some(Protos {
        context: contexts[0].unwrap(),
        offscreen_context: contexts[1].unwrap(),
        gradient,
        pattern,
        path,
        metrics,
        bitmap,
        offscreen,
    });
    let g = vm.global;
    for &(name, len, f) in &[
        ("__fosCanvasGetContext", 3, get_context as NativeFn),
        ("__fosCanvasPNG", 1, canvas_png),
        ("__fosCanvasDataURL", 1, canvas_data_url),
        ("__fosImageBitmap", 5, image_bitmap),
        ("__fosTransferToImageBitmap", 1, transfer_to_image_bitmap),
    ] {
        vm.def_method(g, name, len, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_and_dimensions() {
        assert_eq!(serialize_color(parse_color("red").unwrap()), "#ff0000");
        assert_eq!(serialize_color(parse_color("rgba(0, 128, 255, 0.5)").unwrap()), "rgba(0, 128, 255, 0.5)");
        assert_eq!(serialize_color(parse_color("rgba(0,0,0,0.3)").unwrap()), "rgba(0, 0, 0, 0.3)");
        assert_eq!(serialize_color(parse_color("transparent").unwrap()), "rgba(0, 0, 0, 0)");
        assert_eq!(serialize_color(parse_color("CurrentColor").unwrap()), "#000000");
        assert!(parse_color("nope").is_none());
        assert_eq!(parse_dimension(Some(" 640px"), 300), 640);
        assert_eq!(parse_dimension(Some("-5"), 300), 300);
        assert_eq!(parse_dimension(Some("abc"), 150), 150);
        assert_eq!(parse_dimension(None, 150), 150);
        assert_eq!(parse_spacing("2px", 10.0), Some(2.0));
        assert_eq!(parse_spacing("0.5em", 20.0), Some(10.0));
        assert_eq!(parse_spacing("2", 10.0), None);
        assert_eq!(format_px(-0.0), "0px");
    }
}
