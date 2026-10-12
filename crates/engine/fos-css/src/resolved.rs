//! Resolved values: computed styles serialized as `getComputedStyle`
//! reports them (colors as `rgb()`, lengths in pixels, and for rendered
//! boxes the used sizes of the box model's properties)

use crate::properties::Color;
use crate::style::*;

/// The properties a computed style declaration lists (alphabetical, as in
/// browsers)
pub const PROPERTIES: &[&str] = &[
    "align-content", "align-items", "align-self", "aspect-ratio", "background-clip", "background-color",
    "background-image", "background-position", "background-repeat", "background-size", "border-bottom-color",
    "border-bottom-left-radius", "border-bottom-right-radius", "border-bottom-style", "border-bottom-width",
    "border-collapse", "border-left-color", "border-left-style", "border-left-width", "border-right-color",
    "border-right-style", "border-right-width", "border-spacing", "border-top-color", "border-top-left-radius",
    "border-top-right-radius", "border-top-style", "border-top-width", "bottom", "box-shadow", "box-sizing",
    "caption-side", "clear", "color", "column-gap", "content", "direction", "display", "flex-basis",
    "flex-direction", "flex-grow", "flex-shrink", "flex-wrap", "float", "font-family", "font-size", "font-style",
    "font-weight", "height", "justify-content", "left", "letter-spacing", "line-height", "list-style-position",
    "list-style-type", "margin-bottom", "margin-left", "margin-right", "margin-top", "max-height", "max-width",
    "min-height", "min-width", "object-fit", "opacity", "order", "outline-color", "outline-offset", "outline-style",
    "outline-width", "overflow-wrap", "overflow-x", "overflow-y", "padding-bottom", "padding-left", "padding-right",
    "padding-top", "pointer-events", "position", "right", "row-gap", "table-layout", "text-align",
    "text-decoration-color", "text-decoration-line", "text-decoration-style", "text-indent", "text-overflow",
    "text-transform", "text-wrap-style", "top", "transform", "transform-origin", "vertical-align", "visibility",
    "white-space", "width", "word-break", "word-spacing", "z-index",
];

/// What layout used for a rendered box, which some properties report in
/// place of their computed values
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UsedBox {
    /// The border box's size
    pub border_box: (f32, f32),
    /// Border widths and padding: top, right, bottom, left
    pub border: [f32; 4],
    pub padding: [f32; 4],
    pub margin: [f32; 4],
    /// The offsets of a positioned box: top, right, bottom, left
    pub inset: Option<[f32; 4]>,
    /// `width` and `height` apply (not an inline box's)
    pub sized: bool,
}

/// A number as browsers print it: at most six significant digits, no
/// trailing zeros
pub fn number(v: f32) -> String {
    if !v.is_finite() {
        return "0".into();
    }
    let v = v as f64;
    if (v - v.round()).abs() < 5e-7 {
        return format!("{}", v.round() as i64);
    }
    let before = if v.abs() < 1.0 { 0 } else { v.abs().log10().floor() as i32 + 1 };
    let decimals = (6 - before).clamp(0, 6) as usize;
    let s = format!("{v:.decimals$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.') } else { &s };
    if s == "-0" { "0".into() } else { s.to_string() }
}

pub fn px(v: f32) -> String {
    format!("{}px", number(v))
}

pub fn color(c: Color) -> String {
    if c.a == 255 {
        return format!("rgb({}, {}, {})", c.r, c.g, c.b);
    }
    // The shortest alpha that maps back to the same byte
    let a = c.a as f32 / 255.0;
    let two = (a * 100.0).round() / 100.0;
    let alpha = if (two * 255.0).round() as u8 == c.a { two } else { (a * 1000.0).round() / 1000.0 };
    format!("rgba({}, {}, {}, {})", c.r, c.g, c.b, number(alpha))
}

fn lp(v: Lp) -> String {
    if v.pct == 0.0 {
        px(v.px)
    } else if v.px == 0.0 {
        format!("{}%", number(v.pct))
    } else if v.px < 0.0 {
        format!("calc({}% - {})", number(v.pct), px(-v.px))
    } else {
        format!("calc({}% + {})", number(v.pct), px(v.px))
    }
}

fn lp_auto(v: LpAuto) -> String {
    match v {
        LpAuto::Auto => "auto".into(),
        LpAuto::Lp(l) => lp(l),
    }
}

fn family(name: &str) -> String {
    let generic = matches!(
        name,
        "serif" | "sans-serif" | "monospace" | "cursive" | "fantasy" | "system-ui" | "ui-serif" | "ui-sans-serif" | "ui-monospace" | "ui-rounded" | "math" | "emoji"
    );
    let plain = name.split(' ').all(|w| !w.is_empty() && !w.starts_with(|c: char| c.is_ascii_digit()) && w.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_'));
    if generic || (plain && !name.contains(' ')) {
        name.to_string()
    } else {
        format!("\"{}\"", name.replace('"', "\\\""))
    }
}

fn image(i: &Image) -> String {
    let stops = |stops: &[ColorStop]| stops.iter().map(|s| match s.position {
        Some(p) => format!("{} {}", color(s.color), lp(p)),
        None => color(s.color),
    }).collect::<Vec<_>>().join(", ");
    match i {
        Image::None => "none".into(),
        Image::Url(u) => format!("url(\"{}\")", u.replace('"', "\\\"")),
        Image::Linear { direction, stops: s, repeating } => {
            let dir = match *direction {
                GradientDirection::Angle(a) if (a - 180.0).abs() < 1e-3 => None,
                GradientDirection::Angle(a) => Some(format!("{}deg", number(a))),
                GradientDirection::Corner(x, y) => {
                    let v = match y { -1 => "top", 1 => "bottom", _ => "" };
                    let h = match x { -1 => "left", 1 => "right", _ => "" };
                    Some(format!("to {}", [v, h].iter().filter(|w| !w.is_empty()).copied().collect::<Vec<_>>().join(" ")))
                }
            };
            let name = if *repeating { "repeating-linear-gradient" } else { "linear-gradient" };
            match dir {
                Some(d) => format!("{name}({d}, {})", stops(s)),
                None => format!("{name}({})", stops(s)),
            }
        }
        Image::Radial { circle, center, stops: s, repeating, .. } => {
            let name = if *repeating { "repeating-radial-gradient" } else { "radial-gradient" };
            let shape = if *circle { "circle" } else { "ellipse" };
            format!("{name}({shape} at {} {}, {})", lp(center.0), lp(center.1), stops(s))
        }
    }
}

fn shadow(s: &Shadow, current: Color) -> String {
    let mut out = format!("{} {} {} {} {}", color(s.color.unwrap_or(current)), px(s.x), px(s.y), px(s.blur), px(s.spread));
    if s.inset {
        out.push_str(" inset");
    }
    out
}

fn content(items: Option<&[ContentItem]>) -> String {
    let Some(items) = items else { return "normal".into() };
    if items.is_empty() {
        return "none".into();
    }
    items.iter().map(|i| match i {
        ContentItem::Text(t) => format!("\"{}\"", t.replace('\\', "\\\\").replace('"', "\\\"")),
        ContentItem::Attr(a) => format!("attr({a})"),
        ContentItem::OpenQuote => "open-quote".into(),
        ContentItem::CloseQuote => "close-quote".into(),
        ContentItem::Counter(n, _) => format!("counter({n})"),
        ContentItem::Counters(n, s, _) => format!("counters({n}, \"{s}\")"),
        ContentItem::Image(u) => format!("url(\"{u}\")"),
    }).collect::<Vec<_>>().join(" ")
}

const SIDES: [&str; 4] = ["top", "right", "bottom", "left"];

impl Style {
    /// The resolved value of property `name` (a longhand, one of a few
    /// shorthands, or a custom property), with `used` the box layout gave
    /// the element when it is rendered
    pub fn resolved_value(&self, name: &str, used: Option<&UsedBox>) -> Option<String> {
        if name.starts_with("--") {
            return self.custom.as_ref().and_then(|c| c.get(name)).map(|v| v.trim().to_string());
        }
        let (i, b, bd, bg) = (&*self.inherited, &*self.box_, &*self.border, &*self.background);
        let side = |prefix: &str, suffix: &str| -> Option<usize> {
            let rest = name.strip_prefix(prefix)?;
            let rest = rest.strip_suffix(suffix)?;
            SIDES.iter().position(|s| *s == rest)
        };
        let border_width = |k: usize| if matches!(bd.style[k], BorderStyle::None | BorderStyle::Hidden) { 0.0 } else { bd.width[k] };
        let current = i.color;
        let four = |v: [String; 4]| -> String {
            if v[1] == v[3] {
                if v[0] == v[2] {
                    if v[0] == v[1] { v[0].clone() } else { format!("{} {}", v[0], v[1]) }
                } else {
                    format!("{} {} {}", v[0], v[1], v[2])
                }
            } else {
                v.join(" ")
            }
        };
        let margin = |k: usize| match used {
            Some(u) => px(u.margin[k]),
            None => lp_auto(b.margin[k]),
        };
        let padding = |k: usize| match used {
            Some(u) => px(u.padding[k]),
            None => lp(b.padding[k]),
        };
        let radius = |k: usize| {
            let (h, v) = bd.radius[k];
            if h == v { lp(h) } else { format!("{} {}", lp(h), lp(v)) }
        };
        if let Some(k) = side("margin-", "") {
            return Some(margin(k));
        }
        if let Some(k) = side("padding-", "") {
            return Some(padding(k));
        }
        if let Some(k) = side("border-", "-width") {
            return Some(px(border_width(k)));
        }
        if let Some(k) = side("border-", "-style") {
            return Some(bd.style[k].as_css().into());
        }
        if let Some(k) = side("border-", "-color") {
            return Some(color(bd.color[k].unwrap_or(current)));
        }
        if let Some(k) = SIDES.iter().position(|s| *s == name) {
            return Some(match used.and_then(|u| u.inset) {
                Some(inset) if b.position != Position::Static => px(inset[k]),
                _ => lp_auto(b.inset[k]),
            });
        }
        let corner = ["border-top-left-radius", "border-top-right-radius", "border-bottom-right-radius", "border-bottom-left-radius"];
        if let Some(k) = corner.iter().position(|c| *c == name) {
            return Some(radius(k));
        }
        let size = |v: LpAuto, axis: usize| match used {
            Some(u) if u.sized => {
                let (w, h) = u.border_box;
                let (outer, edges) = if axis == 0 { (w, u.border[1] + u.border[3] + u.padding[1] + u.padding[3]) } else { (h, u.border[0] + u.border[2] + u.padding[0] + u.padding[2]) };
                px(if b.box_sizing == BoxSizing::BorderBox { outer } else { (outer - edges).max(0.0) })
            }
            _ => lp_auto(v),
        };
        let max = |m: MaxSize| match m {
            MaxSize::None => "none".into(),
            MaxSize::Lp(l) => lp(l),
        };
        let gap = |g: Option<Lp>| g.map_or("normal".into(), lp);
        Some(match name {
            "display" => b.display.as_css().into(),
            "position" => b.position.as_css().into(),
            "float" => b.float.as_css().into(),
            "clear" => b.clear.as_css().into(),
            "box-sizing" => b.box_sizing.as_css().into(),
            "overflow-x" => b.overflow_x.as_css().into(),
            "overflow-y" => b.overflow_y.as_css().into(),
            "overflow" => {
                if b.overflow_x == b.overflow_y { b.overflow_x.as_css().into() } else { format!("{} {}", b.overflow_x.as_css(), b.overflow_y.as_css()) }
            }
            "visibility" => i.visibility.as_css().into(),
            "width" => size(b.width, 0),
            "height" => size(b.height, 1),
            "min-width" => lp_auto(b.min_width),
            "min-height" => lp_auto(b.min_height),
            "max-width" => max(b.max_width),
            "max-height" => max(b.max_height),
            "margin" => four([margin(0), margin(1), margin(2), margin(3)]),
            "padding" => four([padding(0), padding(1), padding(2), padding(3)]),
            "border-width" => four([0, 1, 2, 3].map(|k| px(border_width(k)))),
            "border-style" => four([0, 1, 2, 3].map(|k| bd.style[k].as_css().to_string())),
            "border-color" => four([0, 1, 2, 3].map(|k| color(bd.color[k].unwrap_or(current)))),
            "border-radius" => {
                let h = four([0, 1, 2, 3].map(|k| lp(bd.radius[k].0)));
                let v = four([0, 1, 2, 3].map(|k| lp(bd.radius[k].1)));
                if h == v { h } else { format!("{h} / {v}") }
            }
            "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
                let ks: &[usize] = match name { "border-top" => &[0], "border-right" => &[1], "border-bottom" => &[2], "border-left" => &[3], _ => &[0, 1, 2, 3] };
                let one = |k: usize| format!("{} {} {}", px(border_width(k)), bd.style[k].as_css(), color(bd.color[k].unwrap_or(current)));
                let first = one(ks[0]);
                if ks.iter().all(|&k| one(k) == first) { first } else { String::new() }
            }
            "z-index" => b.z_index.map_or("auto".into(), |z| z.to_string()),
            "opacity" => number(b.opacity),
            "order" => b.order.to_string(),
            "flex-direction" => b.flex_direction.as_css().into(),
            "flex-wrap" => b.flex_wrap.as_css().into(),
            "flex-grow" => number(b.flex_grow),
            "flex-shrink" => number(b.flex_shrink),
            "flex-basis" => lp_auto(b.flex_basis),
            "flex" => format!("{} {} {}", number(b.flex_grow), number(b.flex_shrink), lp_auto(b.flex_basis)),
            "justify-content" => b.justify_content.as_css().into(),
            "align-items" => b.align_items.as_css().into(),
            "align-self" => b.align_self.as_css().into(),
            "align-content" => b.align_content.as_css().into(),
            "row-gap" => gap(b.row_gap),
            "column-gap" => gap(b.column_gap),
            "gap" => {
                let (r, c) = (gap(b.row_gap), gap(b.column_gap));
                if r == c { r } else { format!("{r} {c}") }
            }
            "object-fit" => b.object_fit.as_css().into(),
            "text-overflow" => b.text_overflow.as_css().into(),
            "table-layout" => b.table_layout.as_css().into(),
            "vertical-align" => match b.vertical_align {
                VerticalAlign::Baseline => "baseline".into(),
                VerticalAlign::Sub => "sub".into(),
                VerticalAlign::Super => "super".into(),
                VerticalAlign::TextTop => "text-top".into(),
                VerticalAlign::TextBottom => "text-bottom".into(),
                VerticalAlign::Middle => "middle".into(),
                VerticalAlign::Top => "top".into(),
                VerticalAlign::Bottom => "bottom".into(),
                VerticalAlign::Length(l) => lp(l),
            },
            "aspect-ratio" => b.aspect_ratio.map_or("auto".into(), |r| format!("{} / 1", number(r))),
            "content" => content(b.content.as_deref()),
            "transform" => match &b.transform {
                None => "none".into(),
                Some(fns) => {
                    let (w, h) = used.map_or((0.0, 0.0), |u| u.border_box);
                    let m = fns.iter().fold(crate::transform::IDENTITY, |m, f| crate::transform::multiply(m, f.matrix(w, h)));
                    format!("matrix({})", m.iter().map(|v| number(*v)).collect::<Vec<_>>().join(", "))
                }
            },
            "transform-origin" => {
                let (w, h) = used.map_or((0.0, 0.0), |u| u.border_box);
                let (x, y) = b.transform_origin;
                if used.is_some() { format!("{} {}", px(x.resolve(w)), px(y.resolve(h))) } else { format!("{} {}", lp(x), lp(y)) }
            }
            "box-shadow" => match &b.box_shadow {
                None => "none".into(),
                Some(s) => s.iter().map(|s| shadow(s, current)).collect::<Vec<_>>().join(", "),
            },
            "text-decoration-line" => {
                let names: Vec<&str> = [(decoration::UNDERLINE, "underline"), (decoration::OVERLINE, "overline"), (decoration::LINE_THROUGH, "line-through")]
                    .iter()
                    .filter(|(bit, _)| b.text_decoration_line & bit != 0)
                    .map(|(_, n)| *n)
                    .collect();
                if names.is_empty() { "none".into() } else { names.join(" ") }
            }
            "text-decoration-style" => b.text_decoration_style.as_css().into(),
            "text-decoration-color" => color(b.text_decoration_color.unwrap_or(current)),
            "text-decoration" => format!(
                "{} {} {}",
                self.resolved_value("text-decoration-line", used)?,
                b.text_decoration_style.as_css(),
                color(b.text_decoration_color.unwrap_or(current))
            ),
            "outline-style" => bd.outline_style.as_css().into(),
            "outline-width" => px(if bd.outline_style == BorderStyle::None { 0.0 } else { bd.outline_width }),
            "outline-color" => color(bd.outline_color.unwrap_or(current)),
            "outline-offset" => px(bd.outline_offset),
            "background-color" => color(bg.color),
            "background-image" => {
                if bg.images.is_empty() { "none".into() } else { bg.images.iter().map(image).collect::<Vec<_>>().join(", ") }
            }
            "background-repeat" => {
                let one = |(x, y): (BackgroundRepeat, BackgroundRepeat)| match (x, y) {
                    (BackgroundRepeat::Repeat, BackgroundRepeat::NoRepeat) => "repeat-x".to_string(),
                    (BackgroundRepeat::NoRepeat, BackgroundRepeat::Repeat) => "repeat-y".to_string(),
                    (x, y) if x == y => x.as_css().to_string(),
                    (x, y) => format!("{} {}", x.as_css(), y.as_css()),
                };
                if bg.repeat.is_empty() { "repeat".into() } else { bg.repeat.iter().map(|r| one(*r)).collect::<Vec<_>>().join(", ") }
            }
            "background-position" => {
                if bg.position.is_empty() { "0% 0%".into() } else { bg.position.iter().map(|(x, y)| format!("{} {}", lp(*x), lp(*y))).collect::<Vec<_>>().join(", ") }
            }
            "background-size" => {
                let one = |s: &BackgroundSize| match s {
                    BackgroundSize::Auto => "auto".to_string(),
                    BackgroundSize::Cover => "cover".to_string(),
                    BackgroundSize::Contain => "contain".to_string(),
                    BackgroundSize::Explicit(x, LpAuto::Auto) => lp_auto(*x),
                    BackgroundSize::Explicit(x, y) => format!("{} {}", lp_auto(*x), lp_auto(*y)),
                };
                if bg.size.is_empty() { "auto".into() } else { bg.size.iter().map(one).collect::<Vec<_>>().join(", ") }
            }
            "background-clip" => bg.clip.as_css().into(),
            "color" => color(current),
            "font-family" => i.font_family.iter().map(|f| family(f)).collect::<Vec<_>>().join(", "),
            "font-size" => px(i.font_size),
            "font-weight" => i.font_weight.to_string(),
            "font-style" => i.font_style.as_css().into(),
            "line-height" => match i.line_height {
                LineHeight::Normal => "normal".into(),
                LineHeight::Number(n) => px(n * i.font_size),
                LineHeight::Px(p) => px(p),
            },
            "text-align" => match i.text_align {
                TextAlign::WebkitCenter => "-webkit-center".into(),
                t => t.as_css().into(),
            },
            "text-indent" => lp(i.text_indent),
            "text-transform" => i.text_transform.as_css().into(),
            "white-space" => i.white_space.as_css().into(),
            "text-wrap-style" => i.text_wrap.as_css().into(),
            "caption-side" => i.caption_side.as_css().into(),
            "letter-spacing" => if i.letter_spacing == 0.0 { "normal".into() } else { px(i.letter_spacing) },
            "word-spacing" => px(i.word_spacing),
            "list-style-type" => i.list_style_type.as_css().into(),
            "list-style-position" => i.list_style_position.as_css().into(),
            "direction" => i.direction.as_css().into(),
            "word-break" => i.word_break.as_css().into(),
            "overflow-wrap" | "word-wrap" => i.overflow_wrap.as_css().into(),
            "border-collapse" => i.border_collapse.as_css().into(),
            "border-spacing" => {
                let (h, v) = i.border_spacing;
                if h == v { px(h) } else { format!("{} {}", px(h), px(v)) }
            }
            "pointer-events" => i.pointer_events.as_css().into(),
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(css: &str) -> Style {
        let decls = crate::parse_declarations(css);
        let refs: Vec<&crate::Declaration> = decls.iter().collect();
        let parent = Style::default();
        let mut s = Style::inherit_from(&parent);
        s.cascade(&refs, &parent, &StyleContext::default(), &mut crate::values::ResolveCache::default());
        s
    }

    #[test]
    fn numbers_and_colors() {
        assert_eq!(number(16.0), "16");
        assert_eq!(number(13.333333), "13.3333");
        assert_eq!(number(0.5), "0.5");
        assert_eq!(number(-0.0), "0");
        assert_eq!(color(Color { r: 255, g: 0, b: 0, a: 255 }), "rgb(255, 0, 0)");
        assert_eq!(color(Color { r: 0, g: 0, b: 0, a: 0 }), "rgba(0, 0, 0, 0)");
        assert_eq!(color(Color { r: 0, g: 0, b: 0, a: 128 }), "rgba(0, 0, 0, 0.5)");
    }

    #[test]
    fn values_as_browsers_report_them() {
        let s = style("display: flex; color: #f00; margin: 0 auto; padding: 5%; border: 2px solid; width: 50%; line-height: 1.5; font-size: 20px; font-family: 'Helvetica Neue', Arial, sans-serif; z-index: 3; opacity: .5; --brand: #0a0 ");
        let v = |n: &str| s.resolved_value(n, None).unwrap();
        assert_eq!(v("display"), "flex");
        assert_eq!(v("color"), "rgb(255, 0, 0)");
        assert_eq!(v("margin-left"), "auto");
        assert_eq!(v("margin"), "0px auto");
        assert_eq!(v("padding-top"), "5%");
        // Borders without their own color take the text's
        assert_eq!(v("border-top-color"), "rgb(255, 0, 0)");
        assert_eq!(v("border"), "2px solid rgb(255, 0, 0)");
        assert_eq!(v("width"), "50%");
        assert_eq!(v("line-height"), "30px");
        assert_eq!(v("font-family"), "\"Helvetica Neue\", Arial, sans-serif");
        assert_eq!((v("z-index"), v("opacity")), ("3".into(), "0.5".into()));
        assert_eq!(v("--brand"), "#0a0");
        assert_eq!(s.resolved_value("--missing", None), None);
        assert_eq!(v("transform"), "none");
        // No border style: no width
        assert_eq!(style("border-width: 4px").resolved_value("border-top-width", None).unwrap(), "0px");
    }

    #[test]
    fn rendered_boxes_report_used_sizes() {
        let s = style("width: 50%; padding: 10px; border: 1px solid; margin: auto; position: relative; top: 10%");
        let used = UsedBox { border_box: (222.0, 42.0), border: [1.0; 4], padding: [10.0; 4], margin: [0.0, 89.0, 0.0, 89.0], inset: Some([6.0, 0.0, -6.0, 0.0]), sized: true };
        let v = |n: &str| s.resolved_value(n, Some(&used)).unwrap();
        assert_eq!(v("width"), "200px");
        assert_eq!(v("height"), "20px");
        assert_eq!(v("margin-left"), "89px");
        assert_eq!(v("top"), "6px");
        let border_box = style("box-sizing: border-box; width: 50%");
        assert_eq!(border_box.resolved_value("width", Some(&UsedBox { border_box: (300.0, 10.0), sized: true, ..Default::default() })).unwrap(), "300px");
        let t = style("transform: translate(50%, 10px) scale(2)");
        assert_eq!(t.resolved_value("transform", Some(&UsedBox { border_box: (100.0, 40.0), ..Default::default() })).unwrap(), "matrix(2, 0, 0, 2, 50, 10)");
    }
}
