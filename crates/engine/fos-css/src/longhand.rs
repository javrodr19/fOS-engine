//! Declarations to typed longhands
//!
//! Every property a declaration names is parsed once, when the stylesheet
//! is: shorthands (`border`, `background`, `flex`, `font`, ...) expand into
//! their longhands and values become typed (`PropertyValue::Enum` for
//! keywords, lengths, colors, images), so computing styles never reparses
//! text. Invalid values are dropped, as browsers drop them.

use std::sync::Arc;

use crate::parser::{components, font_size, font_weight, parse_color, parse_length, split_top};
use crate::properties::{Keyword, Length, LengthUnit, PropertyId, PropertyValue};
use crate::style::*;
use crate::Declaration;

type Out<'a> = &'a mut Vec<Declaration>;

fn push(out: Out, property: PropertyId, value: PropertyValue, important: bool) {
    out.push(Declaration { property, value, important });
}

fn global_keyword(v: &str) -> Option<Keyword> {
    match v {
        "inherit" => Some(Keyword::Inherit),
        "initial" | "revert" | "revert-layer" => Some(Keyword::Initial),
        "unset" => Some(Keyword::Unset),
        _ => None,
    }
}

/// The longhands a shorthand sets (for `inherit` and friends)
fn longhands_of(name: &str) -> Option<&'static [PropertyId]> {
    use PropertyId::*;
    Some(match name {
        "margin" => &[MarginTop, MarginRight, MarginBottom, MarginLeft],
        "padding" => &[PaddingTop, PaddingRight, PaddingBottom, PaddingLeft],
        "inset" => &[Top, Right, Bottom, Left],
        "margin-block" => &[MarginTop, MarginBottom],
        "margin-inline" => &[MarginLeft, MarginRight],
        "padding-block" => &[PaddingTop, PaddingBottom],
        "padding-inline" => &[PaddingLeft, PaddingRight],
        "border" => &[
            BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth, BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle,
            BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor,
        ],
        "border-width" => &[BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth],
        "border-style" => &[BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle],
        "border-color" => &[BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor],
        "border-radius" => &[BorderTopLeftRadius, BorderTopRightRadius, BorderBottomRightRadius, BorderBottomLeftRadius],
        "border-top" => &[BorderTopWidth, BorderTopStyle, BorderTopColor],
        "border-right" => &[BorderRightWidth, BorderRightStyle, BorderRightColor],
        "border-bottom" => &[BorderBottomWidth, BorderBottomStyle, BorderBottomColor],
        "border-left" => &[BorderLeftWidth, BorderLeftStyle, BorderLeftColor],
        "outline" => &[OutlineWidth, OutlineStyle, OutlineColor],
        "background" => &[BackgroundColor, BackgroundImage, BackgroundRepeat, BackgroundPosition, BackgroundSize],
        "mask" => &[MaskImage, MaskRepeat, MaskPosition, MaskSize],
        "font" => &[FontStyle, FontWeight, FontSize, LineHeight, FontFamily],
        "flex" => &[FlexGrow, FlexShrink, FlexBasis],
        "flex-flow" => &[FlexDirection, FlexWrap],
        "gap" | "grid-gap" => &[RowGap, ColumnGap],
        "grid-row" => &[GridRowStart, GridRowEnd],
        "grid-column" => &[GridColumnStart, GridColumnEnd],
        "grid-area" => &[GridRowStart, GridColumnStart, GridRowEnd, GridColumnEnd],
        "grid-template" => &[GridTemplateRows, GridTemplateColumns, GridTemplateAreas],
        "grid" => &[GridTemplateRows, GridTemplateColumns, GridTemplateAreas, GridAutoFlow, GridAutoRows, GridAutoColumns],
        "overflow" => &[OverflowX, OverflowY],
        "text-decoration" => &[TextDecorationLine, TextDecorationStyle, TextDecorationColor],
        "list-style" => &[ListStyleType, ListStylePosition],
        "place-items" => &[AlignItems],
        "place-content" => &[AlignContent, JustifyContent],
        "place-self" => &[AlignSelf],
        _ => return None,
    })
}

/// Logical names, mapped to the physical ones for horizontal left-to-right text
fn physical(name: &str) -> &str {
    match name {
        "margin-block-start" => "margin-top",
        "margin-block-end" => "margin-bottom",
        "margin-inline-start" => "margin-left",
        "margin-inline-end" => "margin-right",
        "padding-block-start" => "padding-top",
        "padding-block-end" => "padding-bottom",
        "padding-inline-start" => "padding-left",
        "padding-inline-end" => "padding-right",
        "-webkit-mask" => "mask",
        "-webkit-mask-image" => "mask-image",
        "-webkit-mask-size" => "mask-size",
        "-webkit-mask-position" => "mask-position",
        "-webkit-mask-repeat" => "mask-repeat",
        "inset-block-start" => "top",
        "inset-block-end" => "bottom",
        "inset-inline-start" => "left",
        "inset-inline-end" => "right",
        "border-block-start" => "border-top",
        "border-block-end" => "border-bottom",
        "border-inline-start" => "border-left",
        "border-inline-end" => "border-right",
        "border-block-start-width" => "border-top-width",
        "border-block-end-width" => "border-bottom-width",
        "border-inline-start-width" => "border-left-width",
        "border-inline-end-width" => "border-right-width",
        "border-block-start-color" => "border-top-color",
        "border-block-end-color" => "border-bottom-color",
        "border-inline-start-color" => "border-left-color",
        "border-inline-end-color" => "border-right-color",
        "border-block-start-style" => "border-top-style",
        "border-block-end-style" => "border-bottom-style",
        "border-inline-start-style" => "border-left-style",
        "border-inline-end-style" => "border-right-style",
        "border-start-start-radius" => "border-top-left-radius",
        "border-start-end-radius" => "border-top-right-radius",
        "border-end-end-radius" => "border-bottom-right-radius",
        "border-end-start-radius" => "border-bottom-left-radius",
        "inline-size" => "width",
        "block-size" => "height",
        "min-inline-size" => "min-width",
        "min-block-size" => "min-height",
        "max-inline-size" => "max-width",
        "max-block-size" => "max-height",
        "grid-row-gap" => "row-gap",
        "grid-column-gap" => "column-gap",
        "-webkit-box-flex" => "flex-grow",
        "-webkit-flex" => "flex",
        "-webkit-box-sizing" => "box-sizing",
        "word-wrap" => "overflow-wrap",
        "-webkit-text-decoration" => "text-decoration",
        other => other,
    }
}

fn longhand_id(name: &str) -> Option<PropertyId> {
    use PropertyId::*;
    Some(match name {
        "display" => Display,
        "position" => Position,
        "float" => Float,
        "clear" => Clear,
        "box-sizing" => BoxSizing,
        "width" => Width,
        "height" => Height,
        "min-width" => MinWidth,
        "min-height" => MinHeight,
        "max-width" => MaxWidth,
        "max-height" => MaxHeight,
        "margin-top" => MarginTop,
        "margin-right" => MarginRight,
        "margin-bottom" => MarginBottom,
        "margin-left" => MarginLeft,
        "padding-top" => PaddingTop,
        "padding-right" => PaddingRight,
        "padding-bottom" => PaddingBottom,
        "padding-left" => PaddingLeft,
        "top" => Top,
        "right" => Right,
        "bottom" => Bottom,
        "left" => Left,
        "overflow-x" => OverflowX,
        "overflow-y" => OverflowY,
        "z-index" => ZIndex,
        "vertical-align" => VerticalAlign,
        "opacity" => Opacity,
        "flex-direction" => FlexDirection,
        "flex-wrap" => FlexWrap,
        "flex-grow" => FlexGrow,
        "flex-shrink" => FlexShrink,
        "flex-basis" => FlexBasis,
        "justify-content" => JustifyContent,
        "align-items" => AlignItems,
        "align-self" => AlignSelf,
        "align-content" => AlignContent,
        "order" => Order,
        "row-gap" => RowGap,
        "column-gap" => ColumnGap,
        "object-fit" => ObjectFit,
        "text-overflow" => TextOverflow,
        "table-layout" => TableLayout,
        "text-decoration-line" => TextDecorationLine,
        "text-decoration-color" => TextDecorationColor,
        "text-decoration-style" => TextDecorationStyle,
        "border-top-width" => BorderTopWidth,
        "border-right-width" => BorderRightWidth,
        "border-bottom-width" => BorderBottomWidth,
        "border-left-width" => BorderLeftWidth,
        "border-top-style" => BorderTopStyle,
        "border-right-style" => BorderRightStyle,
        "border-bottom-style" => BorderBottomStyle,
        "border-left-style" => BorderLeftStyle,
        "border-top-color" => BorderTopColor,
        "border-right-color" => BorderRightColor,
        "border-bottom-color" => BorderBottomColor,
        "border-left-color" => BorderLeftColor,
        "border-top-left-radius" => BorderTopLeftRadius,
        "border-top-right-radius" => BorderTopRightRadius,
        "border-bottom-right-radius" => BorderBottomRightRadius,
        "border-bottom-left-radius" => BorderBottomLeftRadius,
        "outline-width" => OutlineWidth,
        "outline-style" => OutlineStyle,
        "outline-color" => OutlineColor,
        "outline-offset" => OutlineOffset,
        "background-color" => BackgroundColor,
        "background-image" => BackgroundImage,
        "background-repeat" => BackgroundRepeat,
        "background-position" => BackgroundPosition,
        "background-size" => BackgroundSize,
        "color" => Color,
        "font-family" => FontFamily,
        "font-size" => FontSize,
        "font-weight" => FontWeight,
        "font-style" => FontStyle,
        "line-height" => LineHeight,
        "text-align" => TextAlign,
        "text-indent" => TextIndent,
        "text-transform" => TextTransform,
        "white-space" => WhiteSpace,
        "letter-spacing" => LetterSpacing,
        "word-spacing" => WordSpacing,
        "visibility" => Visibility,
        "list-style-type" => ListStyleType,
        "list-style-position" => ListStylePosition,
        "direction" => Direction,
        "word-break" => WordBreak,
        "overflow-wrap" => OverflowWrap,
        "border-collapse" => BorderCollapse,
        "border-spacing" => BorderSpacing,
        "pointer-events" => PointerEvents,
        "grid-template-columns" => GridTemplateColumns,
        "grid-template-rows" => GridTemplateRows,
        "grid-template-areas" => GridTemplateAreas,
        "content" => Content,
        "counter-reset" => CounterReset,
        "counter-increment" => CounterIncrement,
        "counter-set" => CounterSet,
        "clip" => Clip,
        "clip-path" | "-webkit-clip-path" => ClipPath,
        "transform" | "-webkit-transform" | "-ms-transform" => Transform,
        "transform-origin" | "-webkit-transform-origin" => TransformOrigin,
        "translate" => Translate,
        "rotate" => Rotate,
        "scale" => Scale,
        "box-shadow" | "-webkit-box-shadow" => BoxShadow,
        "grid-auto-columns" => GridAutoColumns,
        "grid-auto-rows" => GridAutoRows,
        "grid-auto-flow" => GridAutoFlow,
        "grid-column-start" => GridColumnStart,
        "grid-column-end" => GridColumnEnd,
        "grid-row-start" => GridRowStart,
        "grid-row-end" => GridRowEnd,
        "justify-items" => JustifyItems,
        "justify-self" => JustifySelf,
        "mask-image" => MaskImage,
        "mask-size" => MaskSize,
        "mask-position" => MaskPosition,
        "mask-repeat" => MaskRepeat,
        _ => return None,
    })
}

/// Whether `name` is a property parsed here
pub(crate) fn is_known(name: &str) -> bool {
    let name = physical(name);
    longhands_of(name).is_some()
        || longhand_id(name).is_some()
        || matches!(name, "margin-block" | "margin-inline" | "padding-block" | "padding-inline")
}

/// Parse `name: value` into longhands; false if the property is unknown
pub(crate) fn expand(name: &str, value: &str, important: bool, out: Out) -> bool {
    let name = physical(name);
    let lower = value.trim().to_ascii_lowercase();
    let start = out.len();
    if let Some(k) = global_keyword(&lower) {
        if let Some(ids) = longhands_of(name) {
            for &id in ids {
                push(out, id, PropertyValue::Keyword(k), important);
            }
            return true;
        }
        if let Some(id) = longhand_id(name) {
            push(out, id, PropertyValue::Keyword(k), important);
            return true;
        }
        return false;
    }
    let handled = match name {
        "grid-row" | "grid-column" | "grid-area" | "grid-template" | "grid" => {
            if let Some(parts) = crate::grid::expand_shorthand(name, value.trim()) {
                if parts.iter().all(|(n, v)| crate::grid::valid(n, v)) {
                    for (n, v) in parts {
                        if let Some(id) = longhand_id(n) {
                            push(out, id, PropertyValue::Grid(Arc::from(v.as_str())), important);
                        }
                    }
                }
            }
            true
        }
        "margin" | "padding" | "inset" => {
            let ids = longhands_of(name).unwrap();
            let negative = name != "padding";
            let parts: Option<Vec<PropertyValue>> = components(&lower).into_iter().map(|c| length(c, name != "padding", false, negative)).collect();
            if let Some(parts) = parts.filter(|p| (1..=4).contains(&p.len())) {
                for (id, v) in ids.iter().zip(four(&parts)) {
                    push(out, *id, v, important);
                }
            }
            true
        }
        "margin-block" | "margin-inline" | "padding-block" | "padding-inline" => {
            let ids = longhands_of(name).unwrap();
            let auto = name.starts_with("margin");
            let parts: Option<Vec<PropertyValue>> = components(&lower).into_iter().map(|c| length(c, auto, false, auto)).collect();
            if let Some(parts) = parts.filter(|p| (1..=2).contains(&p.len())) {
                push(out, ids[0], parts[0].clone(), important);
                push(out, ids[1], parts.get(1).unwrap_or(&parts[0]).clone(), important);
            }
            true
        }
        "border" | "border-top" | "border-right" | "border-bottom" | "border-left" | "outline" => {
            border_shorthand(name, &lower, important, out);
            true
        }
        "border-width" | "border-style" | "border-color" => {
            let ids = longhands_of(name).unwrap();
            let parts: Option<Vec<PropertyValue>> = components(&lower)
                .into_iter()
                .map(|c| match name {
                    "border-width" => border_width(c),
                    "border-style" => enum_value::<BorderStyle>(c),
                    _ => color(c),
                })
                .collect();
            if let Some(parts) = parts.filter(|p| (1..=4).contains(&p.len())) {
                for (id, v) in ids.iter().zip(four(&parts)) {
                    push(out, *id, v, important);
                }
            }
            true
        }
        "border-radius" => {
            border_radius(&lower, important, out);
            true
        }
        "background" => {
            background_shorthand(value.trim(), important, out);
            true
        }
        "mask" => {
            // A background layer without color; mask modes, origins and
            // compositing keywords do not change what is drawn here
            let layer: Vec<&str> = components(value.trim())
                .into_iter()
                .filter(|c| !matches!(c.to_ascii_lowercase().as_str(), "alpha" | "luminance" | "match-source" | "add" | "subtract" | "intersect" | "exclude" | "no-clip" | "border-box" | "padding-box" | "content-box" | "fill-box" | "stroke-box" | "view-box"))
                .collect();
            let mut tmp = Vec::new();
            background_shorthand(&layer.join(" "), important, &mut tmp);
            for d in tmp {
                let id = match d.property {
                    PropertyId::BackgroundImage => PropertyId::MaskImage,
                    PropertyId::BackgroundRepeat => PropertyId::MaskRepeat,
                    PropertyId::BackgroundPosition => PropertyId::MaskPosition,
                    PropertyId::BackgroundSize => PropertyId::MaskSize,
                    _ => continue,
                };
                push(out, id, d.value, important);
            }
            true
        }
        "font" => {
            font_shorthand(value.trim(), important, out);
            true
        }
        "flex" => {
            flex_shorthand(&lower, important, out);
            true
        }
        "flex-flow" => {
            for c in components(&lower) {
                if let Some(v) = enum_value::<FlexDirection>(c) {
                    push(out, PropertyId::FlexDirection, v, important);
                } else if let Some(v) = enum_value::<FlexWrap>(c) {
                    push(out, PropertyId::FlexWrap, v, important);
                }
            }
            true
        }
        "gap" | "grid-gap" => {
            let parts: Option<Vec<PropertyValue>> = components(&lower).into_iter().map(gap).collect();
            if let Some(parts) = parts.filter(|p| (1..=2).contains(&p.len())) {
                push(out, PropertyId::RowGap, parts[0].clone(), important);
                push(out, PropertyId::ColumnGap, parts.get(1).unwrap_or(&parts[0]).clone(), important);
            }
            true
        }
        "overflow" => {
            let parts: Option<Vec<PropertyValue>> = components(&lower).into_iter().map(overflow).collect();
            if let Some(parts) = parts.filter(|p| (1..=2).contains(&p.len())) {
                push(out, PropertyId::OverflowX, parts[0].clone(), important);
                push(out, PropertyId::OverflowY, parts.get(1).unwrap_or(&parts[0]).clone(), important);
            }
            true
        }
        "text-decoration" => {
            let mut line = 0u8;
            let mut ok = true;
            for c in components(&lower) {
                if let Some(bits) = decoration_line(c) {
                    line |= bits;
                } else if let Some(v) = enum_value::<TextDecorationStyle>(c) {
                    push(out, PropertyId::TextDecorationStyle, v, important);
                } else if let Some(v) = color(c) {
                    push(out, PropertyId::TextDecorationColor, v, important);
                } else {
                    ok = false;
                }
            }
            if ok {
                push(out, PropertyId::TextDecorationLine, PropertyValue::Integer(line as i32), important);
            } else {
                out.truncate(start);
            }
            true
        }
        "list-style" => {
            for c in components(&lower) {
                if let Some(v) = list_style_type(c) {
                    push(out, PropertyId::ListStyleType, v, important);
                } else if let Some(v) = enum_value::<ListStylePosition>(c) {
                    push(out, PropertyId::ListStylePosition, v, important);
                }
            }
            true
        }
        "place-items" | "place-self" => {
            let first = components(&lower).first().copied().unwrap_or("");
            let (id, v) = if name == "place-items" {
                (PropertyId::AlignItems, align_items(first))
            } else {
                (PropertyId::AlignSelf, enum_value::<AlignSelf>(first))
            };
            if let Some(v) = v {
                push(out, id, v, important);
            }
            true
        }
        "place-content" => {
            let parts = components(&lower);
            if let Some(v) = parts.first().and_then(|c| enum_value::<AlignContent>(c)) {
                push(out, PropertyId::AlignContent, v, important);
            }
            if let Some(v) = parts.get(1).or(parts.first()).and_then(|c| justify_content(c)) {
                push(out, PropertyId::JustifyContent, v, important);
            }
            true
        }
        _ => {
            let Some(id) = longhand_id(name) else { return false };
            if let Some(v) = longhand(id, &lower, value.trim()) {
                push(out, id, v, important);
            }
            true
        }
    };
    handled
}

/// One longhand's value (`raw` keeps case, for urls and family names)
fn longhand(id: PropertyId, v: &str, raw: &str) -> Option<PropertyValue> {
    use PropertyId as P;
    match id {
        P::Display => display(v),
        P::Position => enum_value::<Position>(v.trim_start_matches("-webkit-")),
        P::Float => enum_value::<Float>(match v {
            "inline-start" => "left",
            "inline-end" => "right",
            other => other,
        }),
        P::Clear => enum_value::<Clear>(match v {
            "inline-start" => "left",
            "inline-end" => "right",
            other => other,
        }),
        P::BoxSizing => enum_value::<BoxSizing>(v),
        P::Width | P::Height | P::FlexBasis => size(v, true),
        P::MinWidth | P::MinHeight => size(v, true),
        P::MaxWidth | P::MaxHeight => {
            if v == "none" {
                Some(PropertyValue::Keyword(Keyword::None))
            } else {
                size(v, false)
            }
        }
        P::MarginTop | P::MarginRight | P::MarginBottom | P::MarginLeft | P::Top | P::Right | P::Bottom | P::Left => length(v, true, false, true),
        P::PaddingTop | P::PaddingRight | P::PaddingBottom | P::PaddingLeft => length(v, false, false, false),
        P::OverflowX | P::OverflowY => overflow(v),
        P::ZIndex => {
            if v == "auto" {
                Some(PropertyValue::Keyword(Keyword::Auto))
            } else {
                v.parse::<i32>().ok().map(PropertyValue::Integer)
            }
        }
        P::VerticalAlign => enum_value::<VerticalAlignKeyword>(v).or_else(|| length(v, false, false, true)),
        P::Opacity => {
            let n = match v.strip_suffix('%') {
                Some(p) => p.trim().parse::<f32>().ok().map(|p| p / 100.0),
                None => v.parse::<f32>().ok(),
            };
            n.filter(|n| n.is_finite()).map(PropertyValue::Number)
        }
        P::FlexDirection => enum_value::<FlexDirection>(v),
        P::FlexWrap => enum_value::<FlexWrap>(v),
        P::FlexGrow | P::FlexShrink => v.parse::<f32>().ok().filter(|n| *n >= 0.0 && n.is_finite()).map(PropertyValue::Number),
        P::JustifyContent => justify_content(v),
        P::AlignItems => align_items(v),
        P::AlignSelf => enum_value::<AlignSelf>(strip_safety(v)),
        P::AlignContent => enum_value::<AlignContent>(strip_safety(v)),
        P::Order => v.parse::<i32>().ok().map(PropertyValue::Integer),
        P::RowGap | P::ColumnGap => gap(v),
        P::ObjectFit => enum_value::<ObjectFit>(v),
        P::TextOverflow => enum_value::<TextOverflow>(v),
        P::TableLayout => enum_value::<TableLayout>(v),
        // `none` (no box, no marker) differs from `normal` for markers
        P::Content if v == "none" => Some(PropertyValue::Keyword(Keyword::None)),
        P::Content => content(raw).map(|c| PropertyValue::Content(Arc::from(c))),
        P::CounterReset | P::CounterSet => counters(raw, 0).map(|c| PropertyValue::Counters(Arc::from(c))),
        P::CounterIncrement => counters(raw, 1).map(|c| PropertyValue::Counters(Arc::from(c))),
        // Kept as text and resolved per element (lengths need its font)
        P::Clip | P::ClipPath => Some(PropertyValue::Transform(Arc::from(raw.trim()))),
        P::Transform => crate::transform::valid(raw).then(|| PropertyValue::Transform(Arc::from(raw.trim()))),
        P::TransformOrigin | P::Translate => Some(PropertyValue::Transform(Arc::from(raw.trim()))),
        P::Rotate => crate::transform::parse_rotate(raw).map(|_| PropertyValue::Transform(Arc::from(raw.trim()))),
        P::BoxShadow => crate::style::parse_shadows(raw, &|t| crate::parser::parse_length(t).map(|_| 0.0).or((t == "0").then_some(0.0))).map(|_| PropertyValue::Transform(Arc::from(raw.trim()))),
        P::Scale => crate::transform::parse_scale(raw).map(|_| PropertyValue::Transform(Arc::from(raw.trim()))),
        P::GridTemplateColumns | P::GridTemplateRows | P::GridTemplateAreas | P::GridAutoColumns | P::GridAutoRows | P::GridAutoFlow | P::GridColumnStart | P::GridColumnEnd | P::GridRowStart | P::GridRowEnd => {
            let name = grid_name(id);
            crate::grid::valid(name, raw.trim()).then(|| PropertyValue::Grid(Arc::from(raw.trim())))
        }
        P::JustifyItems => align_items(match v {
            "left" | "flex-start" | "self-start" => "start",
            "right" | "flex-end" | "self-end" => "end",
            "legacy" | "legacy center" | "center legacy" => if v.contains("center") { "center" } else { "normal" },
            other => other,
        }),
        P::JustifySelf => enum_value::<AlignSelf>(match strip_safety(v) {
            "left" | "flex-start" => "start",
            "right" | "flex-end" => "end",
            other => other,
        }),
        P::TextDecorationLine => {
            let mut bits = 0u8;
            for c in components(v) {
                bits |= decoration_line(c)?;
            }
            Some(PropertyValue::Integer(bits as i32))
        }
        P::TextDecorationColor | P::BackgroundColor | P::Color | P::OutlineColor => color(v),
        P::BorderTopColor | P::BorderRightColor | P::BorderBottomColor | P::BorderLeftColor => color(v),
        P::TextDecorationStyle => enum_value::<TextDecorationStyle>(v),
        P::BorderTopWidth | P::BorderRightWidth | P::BorderBottomWidth | P::BorderLeftWidth | P::OutlineWidth => border_width(v),
        P::BorderTopStyle | P::BorderRightStyle | P::BorderBottomStyle | P::BorderLeftStyle => enum_value::<BorderStyle>(v),
        P::OutlineStyle => enum_value::<BorderStyle>(if v == "auto" { "solid" } else { v }),
        P::OutlineOffset => length(v, false, false, true),
        P::BorderTopLeftRadius | P::BorderTopRightRadius | P::BorderBottomRightRadius | P::BorderBottomLeftRadius => {
            let parts: Option<Vec<PropertyValue>> = components(v).into_iter().map(|c| length(c, false, false, false)).collect();
            match parts?.as_slice() {
                [a] => Some(a.clone()),
                [a, b] => Some(PropertyValue::List(vec![a.clone(), b.clone()])),
                _ => None,
            }
        }
        P::BackgroundImage | P::MaskImage => background_images(raw).map(|i| PropertyValue::Images(Arc::from(i))),
        P::BackgroundRepeat | P::MaskRepeat => layers(v, background_repeat),
        P::BackgroundPosition | P::MaskPosition => layers(v, |l| background_position(&components(l))),
        P::BackgroundSize | P::MaskSize => layers(v, background_size),
        P::FontFamily => font_family(raw),
        P::FontSize => font_size(v),
        P::FontWeight => match v {
            "bolder" => Some(PropertyValue::Keyword(Keyword::Bolder)),
            "lighter" => Some(PropertyValue::Keyword(Keyword::Lighter)),
            _ => font_weight(v).map(PropertyValue::Integer),
        },
        P::FontStyle => {
            let first = v.split_whitespace().next().unwrap_or("");
            enum_value::<FontStyle>(first)
        }
        P::LineHeight => line_height(v),
        P::TextAlign => text_align(v),
        P::TextIndent => {
            let first = components(v).into_iter().next()?;
            length(first, false, false, true)
        }
        P::TextTransform => enum_value::<TextTransform>(v.split_whitespace().next().unwrap_or("")),
        P::WhiteSpace => white_space(v),
        P::LetterSpacing | P::WordSpacing => {
            if v == "normal" {
                Some(PropertyValue::Keyword(Keyword::Normal))
            } else {
                length(v, false, false, true)
            }
        }
        P::Visibility => enum_value::<Visibility>(v),
        P::ListStyleType => list_style_type(v),
        P::ListStylePosition => enum_value::<ListStylePosition>(v),
        P::Direction => enum_value::<Direction>(v),
        P::WordBreak => enum_value::<WordBreak>(v),
        P::OverflowWrap => enum_value::<OverflowWrap>(v),
        P::BorderCollapse => enum_value::<BorderCollapse>(v),
        P::BorderSpacing => {
            let parts: Option<Vec<PropertyValue>> = components(v).into_iter().map(|c| length(c, false, false, false)).collect();
            match parts?.as_slice() {
                [a] => Some(a.clone()),
                [a, b] => Some(PropertyValue::List(vec![a.clone(), b.clone()])),
                _ => None,
            }
        }
        P::PointerEvents => enum_value::<PointerEvents>(if v == "auto" || v == "none" { v } else { "auto" }),
        _ => None,
    }
}

// ---- value parsers ----

trait CssEnum: Sized + Copy {
    fn parse(s: &str) -> Option<Self>;
    fn index(self) -> u8;
}

macro_rules! css_enum_impl {
    ($($t:ty),+) => {
        $(impl CssEnum for $t {
            fn parse(s: &str) -> Option<Self> { <$t>::from_css(s) }
            fn index(self) -> u8 { self as u8 }
        })+
    };
}
css_enum_impl!(
    Display, Position, Float, Clear, BoxSizing, Overflow, Visibility, TextAlign, WhiteSpace, TextTransform, FontStyle, BorderStyle, ListStyleType,
    ListStylePosition, FlexDirection, FlexWrap, JustifyContent, AlignItems, AlignSelf, AlignContent, Direction, WordBreak, OverflowWrap, TextOverflow,
    ObjectFit, TextDecorationStyle, BackgroundRepeat, BorderCollapse, TableLayout, PointerEvents, VerticalAlignKeyword
);

fn enum_value<T: CssEnum>(v: &str) -> Option<PropertyValue> {
    T::parse(v).map(|e| PropertyValue::Enum(e.index()))
}

fn display(v: &str) -> Option<PropertyValue> {
    let parts: Vec<&str> = v.split_whitespace().collect();
    let d = match parts.as_slice() {
        [one] => match *one {
            "-webkit-box" | "-webkit-flex" | "-ms-flexbox" => Display::Flex,
            "-webkit-inline-box" | "-webkit-inline-flex" | "-ms-inline-flexbox" => Display::InlineFlex,
            "run-in" => Display::Block,
            "ruby" | "ruby-base" | "ruby-text" => Display::Inline,
            other => Display::from_css(other)?,
        },
        // Two-value syntax
        ["block", "flow"] | ["flow", "block"] => Display::Block,
        ["inline", "flow"] | ["flow", "inline"] => Display::Inline,
        ["inline", "flow-root"] | ["flow-root", "inline"] => Display::InlineBlock,
        ["block", "flow-root"] | ["flow-root", "block"] => Display::FlowRoot,
        ["block", "flex"] | ["flex", "block"] => Display::Flex,
        ["inline", "flex"] | ["flex", "inline"] => Display::InlineFlex,
        ["block", "grid"] | ["grid", "block"] => Display::Grid,
        ["inline", "grid"] | ["grid", "inline"] => Display::InlineGrid,
        ["block", "table"] | ["table", "block"] => Display::Table,
        ["inline", "table"] | ["table", "inline"] => Display::InlineTable,
        [_, "list-item"] | ["list-item", _] | [_, _, "list-item"] => Display::ListItem,
        _ => return None,
    };
    Some(PropertyValue::Enum(d as u8))
}

fn text_align(v: &str) -> Option<PropertyValue> {
    enum_value::<TextAlign>(match v {
        "-moz-center" => "-webkit-center",
        "-webkit-left" | "-moz-left" => "left",
        "-webkit-right" | "-moz-right" => "right",
        "justify-all" => "justify",
        other => other,
    })
}

fn white_space(v: &str) -> Option<PropertyValue> {
    enum_value::<WhiteSpace>(match v {
        "-webkit-nowrap" => "nowrap",
        "-moz-pre-wrap" | "-pre-wrap" | "-o-pre-wrap" => "pre-wrap",
        // white-space-collapse / text-wrap-mode combinations
        "collapse wrap" | "wrap" => "normal",
        "collapse nowrap" => "nowrap",
        "preserve nowrap" => "pre",
        "preserve wrap" => "pre-wrap",
        other => other,
    })
}

fn overflow(v: &str) -> Option<PropertyValue> {
    enum_value::<Overflow>(match v {
        "overlay" => "auto",
        "-moz-hidden-unscrollable" => "hidden",
        other => other,
    })
}

/// `safe` / `unsafe` alignment prefixes
fn strip_safety(v: &str) -> &str {
    v.strip_prefix("safe ").or_else(|| v.strip_prefix("unsafe ")).unwrap_or(v).trim()
}

/// A `content` value: `none`/`normal` (empty), or strings, `attr()`,
/// quotes and counters (alternative text after `/` is dropped)
fn content(raw: &str) -> Option<Vec<ContentItem>> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case("none") || raw.eq_ignore_ascii_case("normal") {
        return Some(Vec::new());
    }
    let mut out = Vec::new();
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b' ' | b'\t' | b'\n' => i += 1,
            b'/' => break,
            q @ (b'"' | b'\'') => {
                let (text, end) = css_string(raw, i + 1, q)?;
                out.push(ContentItem::Text(Arc::from(text)));
                i = end;
            }
            _ => {
                let start = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b'_') {
                    i += 1;
                }
                let name = raw[start..i].to_ascii_lowercase();
                if name.is_empty() {
                    return None;
                }
                if b.get(i) == Some(&b'(') {
                    let (args, end) = function_args(raw, i + 1)?;
                    i = end;
                    let arg = |n: usize| args.get(n).map(|a| a.trim().to_string());
                    let style = |a: Option<String>| a.and_then(|s| enum_value::<ListStyleType>(&s.to_ascii_lowercase())).map(|v| match v {
                        PropertyValue::Enum(e) => e,
                        _ => 0,
                    });
                    let ident = |a: Option<String>| a.filter(|s| !s.is_empty() && !s.starts_with(['"', '\''])).map(|s| Arc::<str>::from(s.as_str()));
                    let string = |a: Option<String>| -> Option<String> {
                        let a = a?;
                        let q = *a.as_bytes().first()?;
                        matches!(q, b'"' | b'\'').then(|| css_string(&a, 1, q).map(|(t, _)| t)).flatten()
                    };
                    match name.as_str() {
                        "attr" => out.push(ContentItem::Attr(Arc::from(arg(0)?.split_whitespace().next()?.to_ascii_lowercase()))),
                        "counter" => out.push(ContentItem::Counter(ident(arg(0))?, style(arg(1)))),
                        "counters" => out.push(ContentItem::Counters(ident(arg(0))?, Arc::from(string(arg(1))?), style(arg(2)))),
                        // Commas belong to the URL (data: URLs)
                        "url" => {
                            let raw = args.join(",");
                            let raw = raw.trim();
                            let url = match raw.as_bytes().first() {
                                Some(&q @ (b'"' | b'\'')) => css_string(raw, 1, q)?.0,
                                _ => raw.to_string(),
                            };
                            out.push(ContentItem::Image(Arc::from(url)));
                        }
                        // Gradients and other functions show nothing
                        _ => {}
                    }
                } else {
                    match name.as_str() {
                        "open-quote" => out.push(ContentItem::OpenQuote),
                        "close-quote" => out.push(ContentItem::CloseQuote),
                        "no-open-quote" | "no-close-quote" => {}
                        _ => return None,
                    }
                }
            }
        }
    }
    Some(out)
}

/// The comma-separated arguments of a function whose `(` ends before
/// byte `i` (commas and parentheses inside strings do not count), and
/// the index after its `)`
fn function_args(raw: &str, mut i: usize) -> Option<(Vec<&str>, usize)> {
    let b = raw.as_bytes();
    let (mut args, mut start, mut depth) = (Vec::new(), i, 0);
    while i < b.len() {
        match b[i] {
            q @ (b'"' | b'\'') => i = css_string(raw, i + 1, q)?.1,
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' if depth > 0 => {
                depth -= 1;
                i += 1;
            }
            b')' => {
                args.push(&raw[start..i]);
                return Some((args, i + 1));
            }
            b',' if depth == 0 => {
                args.push(&raw[start..i]);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    None
}

/// A `counter-reset`/`-increment`/`-set` value: `none` (empty), or names
/// each followed by an optional integer (`default` when absent)
fn counters(raw: &str, default: i32) -> Option<Vec<(Arc<str>, i32)>> {
    let words: Vec<&str> = raw.split_whitespace().collect();
    if words.len() == 1 && words[0].eq_ignore_ascii_case("none") {
        return Some(Vec::new());
    }
    let mut out: Vec<(Arc<str>, i32)> = Vec::new();
    for w in words {
        if let Ok(n) = w.parse::<i32>() {
            out.last_mut()?.1 = n;
            continue;
        }
        let valid = w.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_' || c == '-' || !c.is_ascii())
            && w.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || !c.is_ascii());
        if !valid || ["none", "inherit", "initial", "unset", "default"].iter().any(|k| w.eq_ignore_ascii_case(k)) {
            return None;
        }
        out.push((Arc::from(w), default));
    }
    (!out.is_empty()).then_some(out)
}

/// A CSS string from byte `i` (after its opening quote `q`): its text
/// with escapes resolved, and the index after its closing quote
pub fn css_string(raw: &str, mut i: usize, q: u8) -> Option<(String, usize)> {
    let b = raw.as_bytes();
    let mut out = String::new();
    while i < b.len() {
        let c = b[i];
        if c == q {
            return Some((out, i + 1));
        }
        if c == b'\\' {
            i += 1;
            let hex_end = (i..b.len().min(i + 6)).find(|&j| !b[j].is_ascii_hexdigit()).unwrap_or(b.len().min(i + 6));
            if hex_end > i {
                let n = u32::from_str_radix(&raw[i..hex_end], 16).ok()?;
                out.push(char::from_u32(n).filter(|&c| c != '\0').unwrap_or('\u{FFFD}'));
                i = hex_end;
                // One white space after a hex escape belongs to it
                if matches!(b.get(i), Some(b' ' | b'\t' | b'\n')) {
                    i += 1;
                }
            } else if let Some(ch) = raw[i..].chars().next() {
                if ch != '\n' {
                    out.push(ch);
                }
                i += ch.len_utf8();
            }
            continue;
        }
        let ch = raw[i..].chars().next()?;
        out.push(ch);
        i += ch.len_utf8();
    }
    // An unclosed string ends at the value's end
    Some((out, i))
}

fn grid_name(id: PropertyId) -> &'static str {
    use PropertyId as P;
    match id {
        P::GridTemplateColumns => "grid-template-columns",
        P::GridTemplateRows => "grid-template-rows",
        P::GridTemplateAreas => "grid-template-areas",
        P::GridAutoColumns => "grid-auto-columns",
        P::GridAutoRows => "grid-auto-rows",
        P::GridAutoFlow => "grid-auto-flow",
        P::GridColumnStart => "grid-column-start",
        P::GridColumnEnd => "grid-column-end",
        P::GridRowStart => "grid-row-start",
        _ => "grid-row-end",
    }
}

fn justify_content(v: &str) -> Option<PropertyValue> {
    enum_value::<JustifyContent>(strip_safety(v))
}

fn align_items(v: &str) -> Option<PropertyValue> {
    let v = strip_safety(v);
    enum_value::<AlignItems>(match v {
        "first baseline" | "last baseline" => "baseline",
        other => other,
    })
}

fn list_style_type(v: &str) -> Option<PropertyValue> {
    enum_value::<ListStyleType>(match v {
        "lower-latin" => "lower-alpha",
        "upper-latin" => "upper-alpha",
        "disclosure-open" | "disclosure-closed" => "disc",
        other => other,
    })
}

fn decoration_line(v: &str) -> Option<u8> {
    Some(match v {
        "none" => 0,
        "underline" => decoration::UNDERLINE,
        "overline" => decoration::OVERLINE,
        "line-through" => decoration::LINE_THROUGH,
        "blink" => 0,
        _ => return None,
    })
}

fn color(v: &str) -> Option<PropertyValue> {
    if v == "currentcolor" {
        return Some(PropertyValue::CurrentColor);
    }
    parse_color(v).map(PropertyValue::Color)
}

/// `-fos-mix(<px>px <pct>%)`: what math functions mixing a percentage and
/// pixels compute to
fn mix(v: &str) -> Option<PropertyValue> {
    let inner = v.strip_prefix("-fos-mix(")?.strip_suffix(')')?;
    let mut parts = inner.split_whitespace();
    let px: f32 = parts.next()?.strip_suffix("px")?.parse().ok()?;
    let pct: f32 = parts.next()?.strip_suffix('%')?.parse().ok()?;
    Some(PropertyValue::Mix { px, pct })
}

/// A length-percentage, optionally `auto` or `none`
fn length(v: &str, allow_auto: bool, allow_none: bool, allow_negative: bool) -> Option<PropertyValue> {
    let v = v.trim();
    if allow_auto && v == "auto" {
        return Some(PropertyValue::Keyword(Keyword::Auto));
    }
    if allow_none && v == "none" {
        return Some(PropertyValue::Keyword(Keyword::None));
    }
    if v.starts_with("-fos-mix(") {
        return mix(v);
    }
    if matches!(v, "0" | "-0" | "+0") {
        return Some(PropertyValue::Length(Length::px(0.0)));
    }
    let l = parse_length(v)?;
    if !l.value.is_finite() || (!allow_negative && l.value < 0.0) {
        return None;
    }
    Some(PropertyValue::Length(l))
}

/// width / height / min-* / flex-basis (intrinsic keywords become auto)
fn size(v: &str, allow_auto: bool) -> Option<PropertyValue> {
    match v {
        "min-content" | "max-content" | "fit-content" | "-webkit-fit-content" | "-moz-fit-content" | "-webkit-fill-available" | "stretch"
        | "content" => Some(if allow_auto { PropertyValue::Keyword(Keyword::Auto) } else { PropertyValue::Keyword(Keyword::None) }),
        _ => length(v, allow_auto, false, false),
    }
}

fn border_width(v: &str) -> Option<PropertyValue> {
    let px = match v {
        "thin" => 1.0,
        "medium" => 3.0,
        "thick" => 5.0,
        _ => return length(v, false, false, false).filter(|l| !matches!(l, PropertyValue::Length(Length { unit: LengthUnit::Percent, .. }))),
    };
    Some(PropertyValue::Length(Length::px(px)))
}

fn gap(v: &str) -> Option<PropertyValue> {
    if v == "normal" {
        return Some(PropertyValue::Keyword(Keyword::Normal));
    }
    length(v, false, false, false)
}

fn line_height(v: &str) -> Option<PropertyValue> {
    if v == "normal" {
        return Some(PropertyValue::Keyword(Keyword::Normal));
    }
    if let Ok(n) = v.parse::<f32>() {
        return (n >= 0.0 && n.is_finite()).then_some(PropertyValue::Number(n));
    }
    length(v, false, false, false)
}

/// 1 to 4 values to top, right, bottom, left
fn four(parts: &[PropertyValue]) -> [PropertyValue; 4] {
    match parts {
        [a] => [a.clone(), a.clone(), a.clone(), a.clone()],
        [a, b] => [a.clone(), b.clone(), a.clone(), b.clone()],
        [a, b, c] => [a.clone(), b.clone(), c.clone(), b.clone()],
        [a, b, c, d] => [a.clone(), b.clone(), c.clone(), d.clone()],
        _ => unreachable!(),
    }
}

fn border_shorthand(name: &str, v: &str, important: bool, out: Out) {
    use PropertyId::*;
    let (widths, styles, colors): (&[PropertyId], &[PropertyId], &[PropertyId]) = match name {
        "border" => (
            &[BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth],
            &[BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle],
            &[BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor],
        ),
        "border-top" => (&[BorderTopWidth], &[BorderTopStyle], &[BorderTopColor]),
        "border-right" => (&[BorderRightWidth], &[BorderRightStyle], &[BorderRightColor]),
        "border-bottom" => (&[BorderBottomWidth], &[BorderBottomStyle], &[BorderBottomColor]),
        "border-left" => (&[BorderLeftWidth], &[BorderLeftStyle], &[BorderLeftColor]),
        _ => (&[OutlineWidth], &[OutlineStyle], &[OutlineColor]),
    };
    // Omitted parts reset to their initial values
    let (mut width, mut style, mut col) = (PropertyValue::Length(Length::px(3.0)), PropertyValue::Enum(crate::style::BorderStyle::None as u8), PropertyValue::CurrentColor);
    for c in components(v) {
        if let Some(w) = border_width(c) {
            width = w;
        } else if let Some(s) = enum_value::<crate::style::BorderStyle>(if c == "auto" { "solid" } else { c }) {
            style = s;
        } else if let Some(k) = color(c) {
            col = k;
        } else {
            return;
        }
    }
    for &id in widths {
        push(out, id, width.clone(), important);
    }
    for &id in styles {
        push(out, id, style.clone(), important);
    }
    for &id in colors {
        push(out, id, col.clone(), important);
    }
}

fn border_radius(v: &str, important: bool, out: Out) {
    let (h, vert) = match v.split_once('/') {
        Some((a, b)) => (a, Some(b)),
        None => (v, None),
    };
    let parse = |s: &str| -> Option<Vec<PropertyValue>> {
        let p: Option<Vec<PropertyValue>> = components(s).into_iter().map(|c| length(c, false, false, false)).collect();
        p.filter(|p| (1..=4).contains(&p.len()))
    };
    let Some(hs) = parse(h) else { return };
    let vs = match vert {
        Some(s) => match parse(s) {
            Some(v) => v,
            None => return,
        },
        None => hs.clone(),
    };
    // top-left, top-right, bottom-right, bottom-left
    let order = |p: &[PropertyValue]| -> [PropertyValue; 4] {
        match p {
            [a] => [a.clone(), a.clone(), a.clone(), a.clone()],
            [a, b] => [a.clone(), b.clone(), a.clone(), b.clone()],
            [a, b, c] => [a.clone(), b.clone(), c.clone(), b.clone()],
            [a, b, c, d] => [a.clone(), b.clone(), c.clone(), d.clone()],
            _ => unreachable!(),
        }
    };
    let (h4, v4) = (order(&hs), order(&vs));
    use PropertyId::*;
    for (i, id) in [BorderTopLeftRadius, BorderTopRightRadius, BorderBottomRightRadius, BorderBottomLeftRadius].into_iter().enumerate() {
        push(out, id, PropertyValue::List(vec![h4[i].clone(), v4[i].clone()]), important);
    }
}

fn flex_shorthand(v: &str, important: bool, out: Out) {
    let num = |n: f32| PropertyValue::Number(n);
    let (grow, shrink, basis) = match v {
        "none" => (num(0.0), num(0.0), PropertyValue::Keyword(Keyword::Auto)),
        "auto" => (num(1.0), num(1.0), PropertyValue::Keyword(Keyword::Auto)),
        _ => {
            let mut numbers = Vec::new();
            let mut basis = None;
            for c in components(v) {
                match c.parse::<f32>() {
                    Ok(n) if n >= 0.0 && numbers.len() < 2 && basis.is_none() || (n >= 0.0 && numbers.len() == 1) => numbers.push(n),
                    _ => match size(c, true) {
                        Some(b) if basis.is_none() => basis = Some(b),
                        _ => return,
                    },
                }
            }
            if numbers.is_empty() && basis.is_none() {
                return;
            }
            let grow = numbers.first().copied().unwrap_or(1.0);
            let shrink = numbers.get(1).copied().unwrap_or(1.0);
            // A unitless grow alone means a basis of 0
            let basis = basis.unwrap_or(PropertyValue::Length(Length::px(0.0)));
            (num(grow), num(shrink), basis)
        }
    };
    push(out, PropertyId::FlexGrow, grow, important);
    push(out, PropertyId::FlexShrink, shrink, important);
    push(out, PropertyId::FlexBasis, basis, important);
}

/// `font: [style] [variant] [weight] [stretch] size[/line-height] family`
fn font_shorthand(v: &str, important: bool, out: Out) {
    let lower = v.to_ascii_lowercase();
    let parts = components(&lower);
    let raw_parts = components(v);
    // System fonts (caption, menu, ...) keep the inherited font
    let Some(size_at) = parts.iter().position(|p| font_size(p.split('/').next().unwrap_or("")).is_some()) else { return };
    let mut style = PropertyValue::Enum(FontStyle::Normal as u8);
    let mut weight = PropertyValue::Integer(400);
    for p in &parts[..size_at] {
        if let Some(s) = enum_value::<FontStyle>(p) {
            style = s;
        } else if let Some(w) = font_weight(p) {
            weight = PropertyValue::Integer(w);
        }
    }
    let (size, lh) = match parts[size_at].split_once('/') {
        Some((s, l)) => (s, Some(l)),
        None => (parts[size_at], None),
    };
    // `size / line-height` may be spaced out
    let mut family_at = size_at + 1;
    let mut lh = lh.map(str::to_string);
    if lh.as_deref() == Some("") {
        lh = parts.get(family_at).map(|s| s.to_string());
        family_at += 1;
    } else if parts.get(family_at) == Some(&"/") {
        lh = parts.get(family_at + 1).map(|s| s.to_string());
        family_at += 2;
    }
    let Some(size) = font_size(size) else { return };
    let line = match lh {
        Some(l) => match line_height(&l) {
            Some(v) => v,
            None => return,
        },
        None => PropertyValue::Keyword(Keyword::Normal),
    };
    let family = raw_parts.get(family_at..).map(|f| f.join(" ")).unwrap_or_default();
    push(out, PropertyId::FontStyle, style, important);
    push(out, PropertyId::FontWeight, weight, important);
    push(out, PropertyId::FontSize, size, important);
    push(out, PropertyId::LineHeight, line, important);
    if let Some(f) = font_family(&family) {
        push(out, PropertyId::FontFamily, f, important);
    }
}

/// Families without quotes, generic names lowercase
fn font_family(raw: &str) -> Option<PropertyValue> {
    let mut list = Vec::new();
    for f in split_top(raw, b',') {
        let f = f.trim();
        let name = if (f.starts_with('"') && f.ends_with('"') || f.starts_with('\'') && f.ends_with('\'')) && f.len() >= 2 {
            f[1..f.len() - 1].to_string()
        } else {
            let lower = f.to_ascii_lowercase();
            match lower.as_str() {
                "serif" | "sans-serif" | "monospace" | "cursive" | "fantasy" | "system-ui" | "ui-sans-serif" | "ui-serif" | "ui-monospace" | "math"
                | "emoji" | "-apple-system" | "blinkmacsystemfont" => lower,
                _ => f.split_whitespace().collect::<Vec<_>>().join(" "),
            }
        };
        if !name.is_empty() {
            list.push(name);
        }
    }
    (!list.is_empty()).then(|| PropertyValue::String(list.join(",")))
}

/// A comma-separated list of layers
fn layers(v: &str, f: impl Fn(&str) -> Option<PropertyValue>) -> Option<PropertyValue> {
    let l: Option<Vec<PropertyValue>> = split_top(v, b',').into_iter().map(|l| f(l.trim())).collect();
    l.filter(|l| !l.is_empty()).map(PropertyValue::List)
}

fn background_repeat(v: &str) -> Option<PropertyValue> {
    let e = |r: BackgroundRepeat| PropertyValue::Enum(r as u8);
    let parts: Vec<&str> = v.split_whitespace().collect();
    let (x, y) = match parts.as_slice() {
        ["repeat-x"] => (BackgroundRepeat::Repeat, BackgroundRepeat::NoRepeat),
        ["repeat-y"] => (BackgroundRepeat::NoRepeat, BackgroundRepeat::Repeat),
        [one] => {
            let r = BackgroundRepeat::from_css(one)?;
            (r, r)
        }
        [a, b] => (BackgroundRepeat::from_css(a)?, BackgroundRepeat::from_css(b)?),
        _ => return None,
    };
    Some(PropertyValue::List(vec![e(x), e(y)]))
}

/// `background-position` for one layer: keywords and offsets
fn background_position(parts: &[&str]) -> Option<PropertyValue> {
    let pct = |p: f32| PropertyValue::Length(Length::percent(p));
    let keyword = |k: &str| -> Option<(char, f32)> {
        Some(match k {
            "left" => ('x', 0.0),
            "right" => ('x', 100.0),
            "top" => ('y', 0.0),
            "bottom" => ('y', 100.0),
            "center" => ('c', 50.0),
            _ => return None,
        })
    };
    let (x, y) = match parts {
        [a] => match keyword(a) {
            Some(('y', p)) => (pct(50.0), pct(p)),
            Some((_, p)) => (pct(p), pct(50.0)),
            None => (length(a, false, false, true)?, pct(50.0)),
        },
        [a, b] => match (keyword(a), keyword(b)) {
            (Some(('y', pa)), Some((_, pb))) => (pct(pb), pct(pa)),
            (Some((_, pa)), Some((_, pb))) => (pct(pa), pct(pb)),
            (Some(('y', _)), None) => return None,
            (Some((_, pa)), None) => (pct(pa), length(b, false, false, true)?),
            (None, Some(('x', _))) => return None,
            (None, Some((_, pb))) => (length(a, false, false, true)?, pct(pb)),
            (None, None) => (length(a, false, false, true)?, length(b, false, false, true)?),
        },
        // Edge offsets (`right 10px bottom 5px`, `left 4px top`): the
        // offset counts from the named edge
        [_, _, _] | [_, _, _, _] => {
            let pairs: Vec<(&str, &str)> = match parts {
                [k1, o1, k2] if length(o1, false, false, true).is_some() => vec![(k1, o1), (k2, "0px")],
                [k1, k2, o2] => vec![(k1, "0px"), (k2, o2)],
                [k1, o1, k2, o2] => vec![(k1, o1), (k2, o2)],
                _ => return None,
            };
            let off = |k: &str, o: &str| -> Option<(char, PropertyValue)> {
                let (axis, base) = keyword(k)?;
                let sign = if base == 100.0 { -1.0 } else { 1.0 };
                let v = match length(o, false, false, true)? {
                    PropertyValue::Length(l) if l.unit == LengthUnit::Percent => pct(base + sign * l.value),
                    PropertyValue::Length(l) if l.unit == LengthUnit::Px => {
                        if base == 0.0 {
                            PropertyValue::Length(Length::px(l.value))
                        } else {
                            PropertyValue::Mix { px: sign * l.value, pct: base }
                        }
                    }
                    _ => return None,
                };
                Some((axis, v))
            };
            let (a, b) = (off(pairs[0].0, pairs[0].1)?, off(pairs[1].0, pairs[1].1)?);
            if a.0 == 'y' || b.0 == 'x' {
                (b.1, a.1)
            } else {
                (a.1, b.1)
            }
        }
        _ => return None,
    };
    Some(PropertyValue::List(vec![x, y]))
}

fn background_size(v: &str) -> Option<PropertyValue> {
    match v {
        "cover" => return Some(PropertyValue::Integer(1)),
        "contain" => return Some(PropertyValue::Integer(2)),
        _ => {}
    }
    let parts: Vec<&str> = v.split_whitespace().collect();
    let auto = PropertyValue::Keyword(Keyword::Auto);
    let (w, h) = match parts.as_slice() {
        [w] => (length(w, true, false, false)?, auto),
        [w, h] => (length(w, true, false, false)?, length(h, true, false, false)?),
        _ => return None,
    };
    Some(PropertyValue::List(vec![w, h]))
}

/// `background`: layers of `image repeat position [/ size]` (plus
/// attachment, origin and clip, ignored), with the color in the last one
fn background_shorthand(v: &str, important: bool, out: Out) {
    let mut images = Vec::new();
    let mut repeats = Vec::new();
    let mut positions = Vec::new();
    let mut sizes = Vec::new();
    let mut bg_color = PropertyValue::Color(crate::properties::Color::TRANSPARENT);
    let layer_list = split_top(v, b',');
    let count = layer_list.len();
    for (i, layer) in layer_list.into_iter().enumerate() {
        // Space out `/` (outside parentheses) so `center/cover` splits
        let mut spaced = String::with_capacity(layer.len() + 4);
        let mut depth = 0;
        for ch in layer.chars() {
            match ch {
                '(' => depth += 1,
                ')' => depth -= 1,
                '/' if depth == 0 => {
                    spaced.push_str(" / ");
                    continue;
                }
                _ => {}
            }
            spaced.push(ch);
        }
        let raw_tokens = components(&spaced);
        let mut image_v = Image::None;
        let mut repeat: Option<PropertyValue> = None;
        let mut pos_tokens: Vec<String> = Vec::new();
        let mut size_tokens: Vec<String> = Vec::new();
        let mut after_slash = false;
        for raw in raw_tokens {
            let t = raw.to_ascii_lowercase();
            if t == "/" {
                after_slash = true;
                continue;
            }
            if after_slash && size_tokens.len() < 2 && (t == "cover" || t == "contain" || length(&t, true, false, false).is_some()) {
                size_tokens.push(t);
                continue;
            }
            after_slash = false;
            if let Some(img) = image(raw).filter(|_| t != "none" || matches!(image_v, Image::None)) {
                image_v = img;
            } else if matches!(t.as_str(), "repeat" | "repeat-x" | "repeat-y" | "no-repeat" | "space" | "round") {
                let joined = match &repeat {
                    Some(PropertyValue::List(p)) if p.len() == 2 && !matches!(t.as_str(), "repeat-x" | "repeat-y") => {
                        // A second keyword: the vertical repeat
                        let first = match &p[0] {
                            PropertyValue::Enum(e) => BackgroundRepeat::from_u8(*e).map(|r| r.as_css()).unwrap_or("repeat"),
                            _ => "repeat",
                        };
                        format!("{first} {t}")
                    }
                    _ => t.clone(),
                };
                repeat = background_repeat(&joined);
            } else if matches!(t.as_str(), "scroll" | "fixed" | "local" | "border-box" | "padding-box" | "content-box" | "text") {
            } else if matches!(t.as_str(), "left" | "right" | "top" | "bottom" | "center") || length(&t, false, false, true).is_some() {
                pos_tokens.push(t);
            } else if let (true, Some(c)) = (i + 1 == count, color(&t)) {
                bg_color = match c {
                    PropertyValue::CurrentColor => PropertyValue::CurrentColor,
                    c => c,
                };
            } else {
                // Invalid: the whole declaration is dropped
                return;
            }
        }
        images.push(image_v);
        repeats.push(repeat.unwrap_or_else(|| background_repeat("repeat").unwrap()));
        let pos_refs: Vec<&str> = pos_tokens.iter().map(String::as_str).collect();
        positions.push(if pos_refs.is_empty() {
            PropertyValue::List(vec![PropertyValue::Length(Length::percent(0.0)), PropertyValue::Length(Length::percent(0.0))])
        } else {
            match background_position(&pos_refs) {
                Some(p) => p,
                None => return,
            }
        });
        sizes.push(if size_tokens.is_empty() {
            PropertyValue::List(vec![PropertyValue::Keyword(Keyword::Auto), PropertyValue::Keyword(Keyword::Auto)])
        } else {
            match background_size(&size_tokens.join(" ")) {
                Some(s) => s,
                None => return,
            }
        });
    }
    push(out, PropertyId::BackgroundColor, bg_color, important);
    push(out, PropertyId::BackgroundImage, PropertyValue::Images(Arc::from(images)), important);
    push(out, PropertyId::BackgroundRepeat, PropertyValue::List(repeats), important);
    push(out, PropertyId::BackgroundPosition, PropertyValue::List(positions), important);
    push(out, PropertyId::BackgroundSize, PropertyValue::List(sizes), important);
}

/// `url(...)`'s address
fn url(v: &str) -> Option<Arc<str>> {
    let lower = v.to_ascii_lowercase();
    let inner = if lower.starts_with("url(") && v.ends_with(')') {
        &v[4..v.len() - 1]
    } else {
        return None;
    };
    let inner = inner.trim().trim_matches(|c| c == '"' || c == '\'');
    Some(Arc::from(inner))
}

fn background_images(raw: &str) -> Option<Vec<Image>> {
    split_top(raw, b',').into_iter().map(|l| image(l.trim())).collect()
}

/// One image: none, url(), a gradient, or image-set()'s first image
fn image(v: &str) -> Option<Image> {
    let lower = v.to_ascii_lowercase();
    if lower == "none" {
        return Some(Image::None);
    }
    if let Some(u) = url(v) {
        return Some(Image::Url(u));
    }
    let (func, args) = {
        let open = v.find('(')?;
        if !v.ends_with(')') {
            return None;
        }
        (lower[..open].trim().to_string(), &v[open + 1..v.len() - 1])
    };
    match func.trim_start_matches("-webkit-").trim_start_matches("-moz-") {
        "image-set" => {
            let first = split_top(args, b',').into_iter().next()?.trim();
            let first = components(first).into_iter().next()?;
            if first.starts_with('"') || first.starts_with('\'') {
                Some(Image::Url(Arc::from(first.trim_matches(|c| c == '"' || c == '\''))))
            } else {
                image(first)
            }
        }
        "linear-gradient" | "repeating-linear-gradient" => linear_gradient(args, func.contains("repeating"), func.starts_with('-')),
        "radial-gradient" | "repeating-radial-gradient" => radial_gradient(args, func.contains("repeating")),
        _ => None,
    }
}

fn angle(v: &str) -> Option<f32> {
    let v = v.trim();
    let split = v.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(v.len());
    let n: f32 = v[..split].parse().ok()?;
    Some(match &v[split..] {
        "deg" => n,
        "rad" => n.to_degrees(),
        "grad" => n * 0.9,
        "turn" => n * 360.0,
        "" if n == 0.0 => 0.0,
        _ => return None,
    })
}

/// Color stops (the arguments after any direction)
fn color_stops(args: &[&str]) -> Option<Arc<[ColorStop]>> {
    let mut stops = Vec::new();
    for a in args {
        let parts = components(a.trim());
        let Some(c) = parts.first().and_then(|c| parse_color(c)) else {
            // A transition hint (a bare length) shapes interpolation: skip
            if parts.len() == 1 && length(parts[0], false, false, true).is_some() {
                continue;
            }
            return None;
        };
        let pos = |s: &str| -> Option<Lp> {
            match length(s, false, false, true)? {
                PropertyValue::Length(l) => Some(match l.unit {
                    LengthUnit::Percent => Lp::pct(l.value),
                    LengthUnit::Px => Lp::px(l.value),
                    LengthUnit::Em | LengthUnit::Rem => Lp::px(l.value * 16.0),
                    _ => Lp::px(l.value),
                }),
                PropertyValue::Mix { px, pct } => Some(Lp { px, pct }),
                _ => None,
            }
        };
        match parts.len() {
            1 => stops.push(ColorStop { color: c, position: None }),
            2 => stops.push(ColorStop { color: c, position: Some(pos(parts[1])?) }),
            3 => {
                stops.push(ColorStop { color: c, position: Some(pos(parts[1])?) });
                stops.push(ColorStop { color: c, position: Some(pos(parts[2])?) });
            }
            _ => return None,
        }
    }
    (stops.len() >= 2).then(|| Arc::from(stops))
}

fn linear_gradient(args: &str, repeating: bool, legacy: bool) -> Option<Image> {
    let parts = split_top(args, b',');
    let first = parts.first()?.trim().to_ascii_lowercase();
    let side = |s: &str| -> Option<(i8, i8)> {
        let mut xy = (0i8, 0i8);
        for w in s.split_whitespace() {
            match w {
                "left" => xy.0 = -1,
                "right" => xy.0 = 1,
                "top" => xy.1 = -1,
                "bottom" => xy.1 = 1,
                _ => return None,
            }
        }
        Some(xy)
    };
    let (direction, stops_from) = if let Some(rest) = first.strip_prefix("to ") {
        let (x, y) = side(rest)?;
        let dir = match (x, y) {
            (0, -1) => GradientDirection::Angle(0.0),
            (1, 0) => GradientDirection::Angle(90.0),
            (0, 1) => GradientDirection::Angle(180.0),
            (-1, 0) => GradientDirection::Angle(270.0),
            (x, y) if x != 0 && y != 0 => GradientDirection::Corner(x, y),
            _ => return None,
        };
        (dir, 1)
    } else if let Some(a) = angle(&first) {
        // Legacy -webkit- angles run counterclockwise from "to right"
        (GradientDirection::Angle(if legacy { 90.0 - a } else { a }), 1)
    } else if legacy && side(&first).is_some() {
        // Legacy syntax names the starting side
        let (x, y) = side(&first)?;
        let dir = match (x, y) {
            (0, -1) => GradientDirection::Angle(180.0),
            (1, 0) => GradientDirection::Angle(270.0),
            (0, 1) => GradientDirection::Angle(0.0),
            (-1, 0) => GradientDirection::Angle(90.0),
            (x, y) => GradientDirection::Corner(-x, -y),
        };
        (dir, 1)
    } else {
        (GradientDirection::Angle(180.0), 0)
    };
    let stops = color_stops(&parts[stops_from..])?;
    Some(Image::Linear { direction, stops, repeating })
}

fn radial_gradient(args: &str, repeating: bool) -> Option<Image> {
    let parts = split_top(args, b',');
    let first = parts.first()?.trim().to_ascii_lowercase();
    let mut circle = false;
    let mut size = RadialSize::FarthestCorner;
    let mut center = (Lp::pct(50.0), Lp::pct(50.0));
    let words = components(&first);
    let is_config = words.iter().any(|w| {
        matches!(*w, "circle" | "ellipse" | "closest-side" | "farthest-side" | "closest-corner" | "farthest-corner" | "at")
            || (parse_color(w).is_none() && length(w, false, false, false).is_some())
    });
    let stops_from = if is_config {
        let (shape_part, at_part) = match words.iter().position(|w| *w == "at") {
            Some(i) => (&words[..i], Some(&words[i + 1..])),
            None => (&words[..], None),
        };
        let mut explicit = Vec::new();
        for w in shape_part {
            match *w {
                "circle" => circle = true,
                "ellipse" => {}
                "closest-side" => size = RadialSize::ClosestSide,
                "farthest-side" => size = RadialSize::FarthestSide,
                "closest-corner" => size = RadialSize::ClosestCorner,
                "farthest-corner" => size = RadialSize::FarthestCorner,
                w => match length(w, false, false, false)? {
                    PropertyValue::Length(l) => explicit.push(if l.unit == LengthUnit::Percent { Lp::pct(l.value) } else { Lp::px(l.value) }),
                    _ => return None,
                },
            }
        }
        match explicit.as_slice() {
            [] => {}
            [r] => {
                circle = true;
                size = RadialSize::Explicit(*r, *r);
            }
            [a, b] => size = RadialSize::Explicit(*a, *b),
            _ => return None,
        }
        if let Some(at) = at_part {
            if let Some(PropertyValue::List(xy)) = background_position(at) {
                let lp = |v: &PropertyValue| match v {
                    PropertyValue::Length(l) if l.unit == LengthUnit::Percent => Some(Lp::pct(l.value)),
                    PropertyValue::Length(l) => Some(Lp::px(l.value)),
                    PropertyValue::Mix { px, pct } => Some(Lp { px: *px, pct: *pct }),
                    _ => None,
                };
                center = (lp(&xy[0])?, lp(&xy[1])?);
            } else {
                return None;
            }
        }
        1
    } else {
        0
    };
    let stops = color_stops(&parts[stops_from..])?;
    Some(Image::Radial { circle, size, center, stops, repeating })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(css: &str) -> Vec<(PropertyId, PropertyValue)> {
        crate::parser::parse_declarations(css).into_iter().map(|d| (d.property, d.value)).collect()
    }

    fn computed(css: &str) -> Style {
        let decls = crate::parser::parse_declarations(css);
        let refs: Vec<&Declaration> = decls.iter().collect();
        let parent = Style::default();
        let mut s = Style::inherit_from(&parent);
        s.cascade(&refs, &parent, &StyleContext::default(), &mut crate::values::ResolveCache::default());
        s
    }

    #[test]
    fn counter_values() {
        let value = |d: &str| match crate::parse_declarations(d).first().map(|d| d.value.clone()) {
            Some(PropertyValue::Counters(c)) => Some(c.iter().map(|(n, v)| (n.to_string(), *v)).collect::<Vec<_>>()),
            _ => None,
        };
        assert_eq!(value("counter-reset: none"), Some(vec![]));
        assert_eq!(value("counter-reset: a b 3"), Some(vec![("a".into(), 0), ("b".into(), 3)]));
        assert_eq!(value("counter-increment: item"), Some(vec![("item".into(), 1)]));
        assert_eq!(value("counter-increment: item -2"), Some(vec![("item".into(), -2)]));
        assert_eq!(value("counter-set: 3"), None);
    }

    #[test]
    fn content_values() {
        let content = |v: &str| match crate::parse_declarations(&format!("content: {v}")).first().map(|d| d.value.clone()) {
            Some(PropertyValue::Content(items)) => Some(items.to_vec()),
            _ => None,
        };
        // none is a keyword, kept apart from normal
        assert_eq!(content("none"), None);
        assert_eq!(content("normal"), Some(vec![]));
        assert_eq!(content(r#""\201C  x" 'y'"#), Some(vec![ContentItem::Text(Arc::from("\u{201C} x")), ContentItem::Text(Arc::from("y"))]));
        assert_eq!(content("attr(data-Label) open-quote"), Some(vec![ContentItem::Attr(Arc::from("data-label")), ContentItem::OpenQuote]));
        assert_eq!(content(r#"counter(item) ". ""#), Some(vec![ContentItem::Counter(Arc::from("item"), None), ContentItem::Text(Arc::from(". "))]));
        assert_eq!(content(r#"counters(sec, ", ", upper-roman) ")""#), Some(vec![ContentItem::Counters(Arc::from("sec"), Arc::from(", "), Some(ListStyleType::UpperRoman as u8)), ContentItem::Text(Arc::from(")"))]));
        assert_eq!(content("counter(x, lower-alpha)"), Some(vec![ContentItem::Counter(Arc::from("x"), Some(ListStyleType::LowerAlpha as u8))]));
        // Alternative text is dropped; images show nothing
        assert_eq!(content(r#""\2192" / "next""#), Some(vec![ContentItem::Text(Arc::from("\u{2192}"))]));
        assert_eq!(content("url(a.svg)"), Some(vec![ContentItem::Image(Arc::from("a.svg"))]));
        assert_eq!(content(r#"url('data:image/svg+xml,<svg a="1,2"/>') "x""#), Some(vec![ContentItem::Image(Arc::from(r#"data:image/svg+xml,<svg a="1,2"/>"#)), ContentItem::Text(Arc::from("x"))]));
        // Other image functions show nothing
        assert_eq!(content("linear-gradient(red, blue)"), Some(vec![]));
        assert_eq!(content("bogus"), None);
    }

    #[test]
    fn shadows_and_transforms_compute() {
        let s = computed("box-shadow: 0 1px 2px rgba(0, 0, 0, 0.5), inset 0 0 0 1em red; font-size: 10px");
        let sh = s.box_.box_shadow.as_deref().unwrap();
        assert_eq!(sh.len(), 2);
        assert_eq!((sh[0].x, sh[0].y, sh[0].blur, sh[0].spread, sh[0].inset), (0.0, 1.0, 2.0, 0.0, false));
        assert_eq!(sh[0].color.map(|c| c.a), Some(128));
        assert_eq!((sh[1].spread, sh[1].inset, sh[1].color.map(|c| c.r)), (10.0, true, Some(255)));
        assert!(computed("box-shadow: none").box_.box_shadow.is_none());
        assert!(computed("box-shadow: 1px").box_.box_shadow.is_none());
        let s = computed("transform: translate(calc(50% - 10px), 2em) rotate(90deg); font-size: 10px");
        let m = s.transform_matrix(0.0, 0.0, 100.0, 20.0).unwrap();
        // About the center (50, 10): (50, 10) moves by (40, 20)
        let (x, y) = crate::transform::apply(m, (50.0, 10.0));
        assert!((x - 90.0).abs() < 1e-3 && (y - 30.0).abs() < 1e-3, "{x} {y}");
        assert!(computed("-webkit-transform: none").transform_matrix(0.0, 0.0, 1.0, 1.0).is_none());
    }

    #[test]
    fn masks_compute() {
        let s = computed("-webkit-mask: url(icon.svg) no-repeat center / 20px 20px; background-color: red");
        let b = &s.background;
        assert_eq!(b.mask.as_deref(), Some("icon.svg"));
        assert_eq!(b.mask_repeat, (BackgroundRepeat::NoRepeat, BackgroundRepeat::NoRepeat));
        assert_eq!(b.mask_position, (Lp::pct(50.0), Lp::pct(50.0)));
        assert_eq!(b.mask_size, BackgroundSize::Explicit(LpAuto::Lp(Lp::px(20.0)), LpAuto::Lp(Lp::px(20.0))));
        let s = computed("mask-image: url(a.svg); mask-size: contain");
        assert_eq!((s.background.mask.as_deref(), s.background.mask_size), (Some("a.svg"), BackgroundSize::Contain));
    }

    #[test]
    fn grid_properties_compute() {
        use crate::grid::{Breadth, GridLine, TemplateItem};
        let s = computed("display: grid; grid-template-columns: 10em 1fr; grid-area: Main; justify-items: center; grid-template-areas: 'Main side'");
        let g = &s.box_.grid;
        assert!(matches!(g.template_columns[0], TemplateItem::Track(t) if t.min == Breadth::Length(Lp::px(160.0))));
        assert_eq!(g.row_start, GridLine::Ident(Arc::from("Main")));
        assert_eq!(g.column_end, GridLine::Ident(Arc::from("Main")));
        assert_eq!(g.justify_items, AlignItems::Center as u8);
        assert_eq!(g.areas.as_ref().unwrap().columns, 2);
        let s = computed("grid-template-columns: repeat(2, calc(50% - 10px))");
        assert!(matches!(&s.box_.grid.template_columns[0], TemplateItem::Repeat(_, inner) if matches!(inner[0], TemplateItem::Track(t) if t.min == Breadth::Length(Lp { px: -10.0, pct: 50.0 }))));
    }

    #[test]
    fn shorthands_expand() {
        let s = computed("margin: 1px 2px 3px; padding: 4px; border: 2px solid red; border-left: none; border-radius: 4px / 8px 2px");
        assert_eq!(s.box_.margin, [LpAuto::Lp(Lp::px(1.0)), LpAuto::Lp(Lp::px(2.0)), LpAuto::Lp(Lp::px(3.0)), LpAuto::Lp(Lp::px(2.0))]);
        assert_eq!(s.box_.padding, [Lp::px(4.0); 4]);
        assert_eq!(s.border.width, [2.0, 2.0, 2.0, 0.0]);
        assert_eq!(s.border.style[0], BorderStyle::Solid);
        assert_eq!(s.border.color[0], Some(crate::properties::Color::rgb(255, 0, 0)));
        assert_eq!(s.border.radius[0], (Lp::px(4.0), Lp::px(8.0)));
        assert_eq!(s.border.radius[1], (Lp::px(4.0), Lp::px(2.0)));
        let s = computed("flex: 1; gap: 10px 5%; overflow: hidden auto; inset: 0 auto");
        assert_eq!((s.box_.flex_grow, s.box_.flex_shrink, s.box_.flex_basis), (1.0, 1.0, LpAuto::ZERO));
        assert_eq!((s.box_.row_gap, s.box_.column_gap), (Some(Lp::px(10.0)), Some(Lp::pct(5.0))));
        assert_eq!((s.box_.overflow_x, s.box_.overflow_y), (Overflow::Hidden, Overflow::Auto));
        assert_eq!(s.box_.inset, [LpAuto::ZERO, LpAuto::Auto, LpAuto::ZERO, LpAuto::Auto]);
        let s = computed("flex: none");
        assert_eq!((s.box_.flex_grow, s.box_.flex_shrink, s.box_.flex_basis), (0.0, 0.0, LpAuto::Auto));
        let s = computed("flex: 2 0 100px");
        assert_eq!((s.box_.flex_grow, s.box_.flex_shrink, s.box_.flex_basis), (2.0, 0.0, LpAuto::Lp(Lp::px(100.0))));
    }

    #[test]
    fn fonts_and_text() {
        let s = computed("font: italic bold 2em/1.5 'Helvetica Neue', Arial, sans-serif; text-align: -webkit-center; white-space: nowrap");
        let i = &s.inherited;
        assert_eq!((i.font_style, i.font_weight, i.font_size), (FontStyle::Italic, 700, 32.0));
        assert_eq!(i.line_height, LineHeight::Number(1.5));
        assert_eq!(&*i.font_family.iter().map(|f| f.to_string()).collect::<Vec<_>>(), ["Helvetica Neue", "Arial", "sans-serif"]);
        assert_eq!((i.text_align, i.white_space), (TextAlign::WebkitCenter, WhiteSpace::Nowrap));
        let s = computed("font-size: 20px; line-height: 150%; text-decoration: underline dotted red; letter-spacing: .1em");
        assert_eq!(s.inherited.line_height, LineHeight::Px(30.0));
        assert_eq!(s.inherited.letter_spacing, 2.0);
        assert_eq!(s.box_.text_decoration_line, decoration::UNDERLINE);
        assert_eq!(s.box_.text_decoration_style, TextDecorationStyle::Dotted);
    }

    #[test]
    fn keywords_and_blockification() {
        let s = computed("display: inline flow-root");
        assert_eq!(s.box_.display, Display::InlineBlock);
        let s = computed("display: inline; float: left");
        assert_eq!(s.box_.display, Display::Block);
        let s = computed("display: inline-flex; position: absolute; float: right");
        assert_eq!((s.box_.display, s.box_.float), (Display::Flex, Float::None));
        let s = computed("display: bogus; position: -webkit-sticky; z-index: 3; opacity: 50%");
        assert_eq!((s.box_.display, s.box_.position, s.box_.z_index, s.box_.opacity), (Display::Inline, Position::Sticky, Some(3), 0.5));
        let s = computed("border-style: dashed; border-width: thin medium");
        assert_eq!(s.border.width, [1.0, 3.0, 1.0, 3.0]);
    }

    #[test]
    fn inheritance_keywords() {
        let decls = crate::parser::parse_declarations("color: red; border-top: 1px solid; margin-top: 5px; font-size: 30px");
        let refs: Vec<&Declaration> = decls.iter().collect();
        let mut parent = Style::default();
        parent.cascade(&refs, &Style::default(), &StyleContext::default(), &mut Default::default());
        let child_decls = crate::parser::parse_declarations("margin-top: inherit; color: initial; font-size: unset; border-top-color: currentColor");
        let refs: Vec<&Declaration> = child_decls.iter().collect();
        let mut child = Style::inherit_from(&parent);
        child.cascade(&refs, &parent, &StyleContext::default(), &mut Default::default());
        assert_eq!(child.box_.margin[0], LpAuto::Lp(Lp::px(5.0)));
        assert_eq!(child.inherited.color, crate::properties::Color::BLACK);
        assert_eq!(child.inherited.font_size, 30.0);
        // Children share their parent's inherited group until they change it
        let plain = Style::inherit_from(&parent);
        assert!(Arc::ptr_eq(&plain.inherited, &parent.inherited));
        assert!(Arc::ptr_eq(&plain.box_, &Style::default().box_));
    }

    #[test]
    fn backgrounds_and_gradients() {
        let s = computed("background: url(\"a.png\") no-repeat right 10px bottom / 20px auto, linear-gradient(to right, red, blue 80%) #fff");
        let b = &s.background;
        assert_eq!(b.color, crate::properties::Color::WHITE);
        assert!(matches!(&b.images[0], Image::Url(u) if &**u == "a.png"));
        assert!(matches!(&b.images[1], Image::Linear { direction: GradientDirection::Angle(a), stops, .. } if *a == 90.0 && stops.len() == 2 && stops[1].position == Some(Lp::pct(80.0))));
        assert_eq!(b.repeat[0], (BackgroundRepeat::NoRepeat, BackgroundRepeat::NoRepeat));
        assert_eq!(b.position[0], (Lp { px: -10.0, pct: 100.0 }, Lp::pct(100.0)));
        assert_eq!(b.size[0], BackgroundSize::Explicit(LpAuto::Lp(Lp::px(20.0)), LpAuto::Auto));
        let s = computed("background-image: radial-gradient(circle at 10px 20%, rgba(0,0,0,.5) 0, transparent 50px), -webkit-linear-gradient(top, #000, #fff)");
        assert!(matches!(&s.background.images[0], Image::Radial { circle: true, center, .. } if *center == (Lp::px(10.0), Lp::pct(20.0))));
        assert!(matches!(&s.background.images[1], Image::Linear { direction: GradientDirection::Angle(a), .. } if *a == 180.0));
        let s = computed("background: none");
        assert!(!s.background.is_visible());
    }

    #[test]
    fn invalid_values_are_dropped() {
        assert!(parse("width: -5px; padding: -1px; display: flexy; margin: 1px 2px 3px 4px 5px").is_empty());
        assert!(parse("border: 2px solid red bogus").is_empty());
        assert!(parse("color: notacolor; flex: a b").is_empty());
        // Unknown properties are not longhands
        assert!(parse("animation-name: x").iter().all(|(id, _)| *id != PropertyId::Width));
    }
}
