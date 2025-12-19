//! CSS Property Definitions
//!
//! All supported CSS properties and their value types.
//! Uses enums for fixed values to save memory vs strings.

/// Property identifier - uses enum for type safety and memory efficiency
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum PropertyId {
    // Display & Layout
    Display,
    Position,
    Float,
    Clear,
    
    // Flexbox
    FlexDirection,
    FlexWrap,
    JustifyContent,
    AlignItems,
    AlignContent,
    FlexGrow,
    FlexShrink,
    FlexBasis,
    
    // Box Model
    Width,
    Height,
    MinWidth,
    MinHeight,
    MaxWidth,
    MaxHeight,
    Margin,
    MarginTop,
    MarginRight,
    MarginBottom,
    MarginLeft,
    Padding,
    PaddingTop,
    PaddingRight,
    PaddingBottom,
    PaddingLeft,
    
    // Border
    Border,
    BorderWidth,
    BorderStyle,
    BorderColor,
    BorderRadius,
    
    // Colors & Background
    Color,
    BackgroundColor,
    Background,
    Opacity,
    
    // Text
    FontFamily,
    FontSize,
    FontWeight,
    FontStyle,
    TextAlign,
    TextDecoration,
    LineHeight,
    LetterSpacing,
    WhiteSpace,
    
    // Visual
    Overflow,
    OverflowX,
    OverflowY,
    Visibility,
    ZIndex,
    
    // Positioning
    Top,
    Right,
    Bottom,
    Left,
    
    // Transform
    Transform,
    TransformOrigin,
    
    // Transition & Animation
    Transition,
    Animation,
    
    // Custom property (--var-name)
    Custom(u32), // Index into custom property table
}

impl PropertyId {
    /// Parse a property name into a PropertyId
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "display" => Self::Display,
            "position" => Self::Position,
            "float" => Self::Float,
            "clear" => Self::Clear,
            
            "flex-direction" => Self::FlexDirection,
            "flex-wrap" => Self::FlexWrap,
            "justify-content" => Self::JustifyContent,
            "align-items" => Self::AlignItems,
            "align-content" => Self::AlignContent,
            "flex-grow" => Self::FlexGrow,
            "flex-shrink" => Self::FlexShrink,
            "flex-basis" => Self::FlexBasis,
            
            "width" => Self::Width,
            "height" => Self::Height,
            "min-width" => Self::MinWidth,
            "min-height" => Self::MinHeight,
            "max-width" => Self::MaxWidth,
            "max-height" => Self::MaxHeight,
            
            "margin" => Self::Margin,
            "margin-top" => Self::MarginTop,
            "margin-right" => Self::MarginRight,
            "margin-bottom" => Self::MarginBottom,
            "margin-left" => Self::MarginLeft,
            
            "padding" => Self::Padding,
            "padding-top" => Self::PaddingTop,
            "padding-right" => Self::PaddingRight,
            "padding-bottom" => Self::PaddingBottom,
            "padding-left" => Self::PaddingLeft,
            
            "border" => Self::Border,
            "border-width" => Self::BorderWidth,
            "border-style" => Self::BorderStyle,
            "border-color" => Self::BorderColor,
            "border-radius" => Self::BorderRadius,
            
            "color" => Self::Color,
            "background-color" => Self::BackgroundColor,
            "background" => Self::Background,
            "opacity" => Self::Opacity,
            
            "font-family" => Self::FontFamily,
            "font-size" => Self::FontSize,
            "font-weight" => Self::FontWeight,
            "font-style" => Self::FontStyle,
            "text-align" => Self::TextAlign,
            "text-decoration" => Self::TextDecoration,
            "line-height" => Self::LineHeight,
            "letter-spacing" => Self::LetterSpacing,
            "white-space" => Self::WhiteSpace,
            
            "overflow" => Self::Overflow,
            "overflow-x" => Self::OverflowX,
            "overflow-y" => Self::OverflowY,
            "visibility" => Self::Visibility,
            "z-index" => Self::ZIndex,
            
            "top" => Self::Top,
            "right" => Self::Right,
            "bottom" => Self::Bottom,
            "left" => Self::Left,
            
            "transform" => Self::Transform,
            "transform-origin" => Self::TransformOrigin,
            "transition" => Self::Transition,
            "animation" => Self::Animation,
            
            _ => return None,
        })
    }
}

/// Property value - parsed and typed
#[derive(Debug, Clone)]
pub enum PropertyValue {
    /// Keyword value (inherit, initial, unset, none, auto, etc.)
    Keyword(Keyword),
    /// Length value (px, em, rem, %, etc.)
    Length(Length),
    /// Color value
    Color(Color),
    /// Number (for opacity, z-index, flex-grow, etc.)
    Number(f32),
    /// Integer
    Integer(i32),
    /// String (for font-family, content, etc.)
    String(String),
    /// Multiple values (for shorthand properties)
    List(Vec<PropertyValue>),
    /// Raw CSS (for complex values we don't fully parse)
    Raw(String),
}

/// CSS keyword values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    // Global keywords
    Inherit,
    Initial,
    Unset,
    
    // Common values
    None,
    Auto,
    Normal,
    Hidden,
    Visible,
    
    // Display
    Block,
    Inline,
    InlineBlock,
    Flex,
    Grid,
    Contents,
    
    // Position
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
    
    // Flexbox
    Row,
    RowReverse,
    Column,
    ColumnReverse,
    Wrap,
    Nowrap,
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
    Stretch,
    Baseline,
    
    // Text
    Left,
    Right,
    Justify,
    Underline,
    Overline,
    LineThrough,
    
    // Border style
    Solid,
    Dashed,
    Dotted,
    Double,
    
    // Overflow
    Scroll,
    Clip,
    
    // Font
    Bold,
    Bolder,
    Lighter,
    Italic,
    Oblique,
}

impl Keyword {
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "inherit" => Self::Inherit,
            "initial" => Self::Initial,
            "unset" => Self::Unset,
            "none" => Self::None,
            "auto" => Self::Auto,
            "normal" => Self::Normal,
            "hidden" => Self::Hidden,
            "visible" => Self::Visible,
            "block" => Self::Block,
            "inline" => Self::Inline,
            "inline-block" => Self::InlineBlock,
            "flex" => Self::Flex,
            "grid" => Self::Grid,
            "contents" => Self::Contents,
            "static" => Self::Static,
            "relative" => Self::Relative,
            "absolute" => Self::Absolute,
            "fixed" => Self::Fixed,
            "sticky" => Self::Sticky,
            "row" => Self::Row,
            "row-reverse" => Self::RowReverse,
            "column" => Self::Column,
            "column-reverse" => Self::ColumnReverse,
            "wrap" => Self::Wrap,
            "nowrap" => Self::Nowrap,
            "flex-start" => Self::FlexStart,
            "flex-end" => Self::FlexEnd,
            "center" => Self::Center,
            "space-between" => Self::SpaceBetween,
            "space-around" => Self::SpaceAround,
            "space-evenly" => Self::SpaceEvenly,
            "stretch" => Self::Stretch,
            "baseline" => Self::Baseline,
            "left" => Self::Left,
            "right" => Self::Right,
            "justify" => Self::Justify,
            "underline" => Self::Underline,
            "overline" => Self::Overline,
            "line-through" => Self::LineThrough,
            "solid" => Self::Solid,
            "dashed" => Self::Dashed,
            "dotted" => Self::Dotted,
            "double" => Self::Double,
            "scroll" => Self::Scroll,
            "clip" => Self::Clip,
            "bold" => Self::Bold,
            "bolder" => Self::Bolder,
            "lighter" => Self::Lighter,
            "italic" => Self::Italic,
            "oblique" => Self::Oblique,
            _ => return None,
        })
    }
}

/// CSS length value
#[derive(Debug, Clone, Copy)]
pub struct Length {
    pub value: f32,
    pub unit: LengthUnit,
}

impl Length {
    pub fn px(value: f32) -> Self {
        Self { value, unit: LengthUnit::Px }
    }
    
    pub fn em(value: f32) -> Self {
        Self { value, unit: LengthUnit::Em }
    }
    
    pub fn percent(value: f32) -> Self {
        Self { value, unit: LengthUnit::Percent }
    }
    
    pub fn zero() -> Self {
        Self { value: 0.0, unit: LengthUnit::Px }
    }
}

/// Length units
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit {
    Px,
    Em,
    Rem,
    Percent,
    Vw,
    Vh,
    Vmin,
    Vmax,
    Ch,
    Ex,
}

/// CSS color
#[derive(Debug, Clone, Copy)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Default for Color {
    fn default() -> Self {
        Self::BLACK
    }
}

impl Color {
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    
    /// Parse a hex color (#RGB, #RRGGBB, #RRGGBBAA)
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                Some(Self::rgb(r, g, b))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Self::rgb(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Self::rgba(r, g, b, a))
            }
            _ => None,
        }
    }
    
    /// Parse a named color
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "transparent" => Self::TRANSPARENT,
            "black" => Self::BLACK,
            "white" => Self::WHITE,
            "red" => Self::rgb(255, 0, 0),
            "green" => Self::rgb(0, 128, 0),
            "blue" => Self::rgb(0, 0, 255),
            "yellow" => Self::rgb(255, 255, 0),
            "cyan" | "aqua" => Self::rgb(0, 255, 255),
            "magenta" | "fuchsia" => Self::rgb(255, 0, 255),
            "gray" | "grey" => Self::rgb(128, 128, 128),
            "silver" => Self::rgb(192, 192, 192),
            "maroon" => Self::rgb(128, 0, 0),
            "olive" => Self::rgb(128, 128, 0),
            "lime" => Self::rgb(0, 255, 0),
            "navy" => Self::rgb(0, 0, 128),
            "purple" => Self::rgb(128, 0, 128),
            "teal" => Self::rgb(0, 128, 128),
            "orange" => Self::rgb(255, 165, 0),
            _ => return None,
        })
    }
}
