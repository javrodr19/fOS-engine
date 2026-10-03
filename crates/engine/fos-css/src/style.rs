//! Computed styles for layout and painting
//!
//! A `Style` is a handful of `Arc`-shared groups: what children inherit
//! (fonts, text, color), and the box, border and background properties.
//! Elements copy their parent's inherited group and the default groups by
//! reference; a group is cloned only when a declaration changes it, so a
//! page of ten thousand elements needs only as many groups as there are
//! distinct ones. Lengths are computed to pixels at cascade time except for
//! percentages, which layout resolves (`Lp`: pixels plus a percentage, so
//! `calc(100% - 20px)` keeps both parts).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock};

use crate::properties::{Color, Keyword, Length, LengthUnit, PropertyId, PropertyValue};
use crate::Declaration;

// ---- values ----

/// A length-percentage: `px` plus `pct` percent of a basis known at layout
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lp {
    pub px: f32,
    pub pct: f32,
}

impl Lp {
    pub const ZERO: Lp = Lp { px: 0.0, pct: 0.0 };

    pub const fn px(px: f32) -> Lp {
        Lp { px, pct: 0.0 }
    }

    pub const fn pct(pct: f32) -> Lp {
        Lp { px: 0.0, pct }
    }

    /// Pixels, given what a percentage is of
    #[inline]
    pub fn resolve(self, basis: f32) -> f32 {
        if self.pct == 0.0 {
            self.px
        } else {
            self.px + self.pct * basis / 100.0
        }
    }

    pub fn is_zero(self) -> bool {
        self.px == 0.0 && self.pct == 0.0
    }

    pub fn has_percent(self) -> bool {
        self.pct != 0.0
    }
}

/// `auto` or a length-percentage (widths, margins, insets, flex-basis)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LpAuto {
    #[default]
    Auto,
    Lp(Lp),
}

impl LpAuto {
    pub const ZERO: LpAuto = LpAuto::Lp(Lp::ZERO);

    pub fn is_auto(self) -> bool {
        matches!(self, LpAuto::Auto)
    }

    /// Pixels, or `None` for `auto`
    #[inline]
    pub fn resolve(self, basis: f32) -> Option<f32> {
        match self {
            LpAuto::Auto => None,
            LpAuto::Lp(l) => Some(l.resolve(basis)),
        }
    }

    /// Pixels, or `None` for `auto` or a percentage of an unknown basis
    pub fn resolve_definite(self, basis: Option<f32>) -> Option<f32> {
        match self {
            LpAuto::Auto => None,
            LpAuto::Lp(l) if l.has_percent() => basis.map(|b| l.resolve(b)),
            LpAuto::Lp(l) => Some(l.px),
        }
    }
}

/// `none` or a length-percentage (max-width, max-height)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum MaxSize {
    #[default]
    None,
    Lp(Lp),
}

impl MaxSize {
    pub fn resolve(self, basis: f32) -> f32 {
        match self {
            MaxSize::None => f32::INFINITY,
            MaxSize::Lp(l) => l.resolve(basis),
        }
    }
}

/// `line-height`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum LineHeight {
    #[default]
    Normal,
    /// A multiple of the element's own font size (inherited as the number)
    Number(f32),
    /// Pixels (lengths and percentages are computed against the font size)
    Px(f32),
}

impl LineHeight {
    /// Pixels for a font size (`normal` is 1.2, as most fonts' metrics give)
    pub fn resolve(self, font_size: f32) -> f32 {
        match self {
            LineHeight::Normal => font_size * 1.2,
            LineHeight::Number(n) => font_size * n,
            LineHeight::Px(px) => px,
        }
    }
}

/// `vertical-align`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Sub,
    Super,
    TextTop,
    TextBottom,
    Middle,
    Top,
    Bottom,
    /// Raised by this much (percentages of the line height)
    Length(Lp),
}

/// Enumerated properties: CSS keywords and the discriminants that parsed
/// values carry (`PropertyValue::Enum`)
macro_rules! css_enum {
    ($(#[$doc:meta])* $name:ident { $($variant:ident = $css:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn from_css(s: &str) -> Option<$name> {
                match s {
                    $($css => Some($name::$variant),)+
                    _ => None,
                }
            }

            pub fn from_u8(v: u8) -> Option<$name> {
                Self::ALL.get(v as usize).copied()
            }

            pub fn as_css(self) -> &'static str {
                match self {
                    $($name::$variant => $css),+
                }
            }
        }

        impl Default for $name {
            fn default() -> $name {
                Self::ALL[0]
            }
        }
    };
}

css_enum!(
    /// `display` (the initial value is `inline`; the UA stylesheet makes
    /// blocks)
    Display {
        Inline = "inline",
        Block = "block",
        InlineBlock = "inline-block",
        Flex = "flex",
        InlineFlex = "inline-flex",
        Grid = "grid",
        InlineGrid = "inline-grid",
        FlowRoot = "flow-root",
        ListItem = "list-item",
        Table = "table",
        InlineTable = "inline-table",
        TableRowGroup = "table-row-group",
        TableHeaderGroup = "table-header-group",
        TableFooterGroup = "table-footer-group",
        TableRow = "table-row",
        TableCell = "table-cell",
        TableColumnGroup = "table-column-group",
        TableColumn = "table-column",
        TableCaption = "table-caption",
        Contents = "contents",
        None = "none",
    }
);

impl Display {
    /// Participates in an inline formatting context as an atom or text
    pub fn is_inline_level(self) -> bool {
        matches!(self, Display::Inline | Display::InlineBlock | Display::InlineFlex | Display::InlineGrid | Display::InlineTable)
    }

    /// The block-level equivalent (floats, absolute positioning and flex
    /// items "blockify" their display)
    pub fn blockified(self) -> Display {
        match self {
            Display::Inline | Display::InlineBlock => Display::Block,
            Display::InlineFlex => Display::Flex,
            Display::InlineGrid => Display::Grid,
            Display::InlineTable => Display::Table,
            Display::TableRowGroup
            | Display::TableHeaderGroup
            | Display::TableFooterGroup
            | Display::TableRow
            | Display::TableCell
            | Display::TableColumnGroup
            | Display::TableColumn
            | Display::TableCaption => Display::Block,
            other => other,
        }
    }
}

css_enum!(Position { Static = "static", Relative = "relative", Absolute = "absolute", Fixed = "fixed", Sticky = "sticky" });
css_enum!(Float { None = "none", Left = "left", Right = "right" });
css_enum!(Clear { None = "none", Left = "left", Right = "right", Both = "both" });
css_enum!(BoxSizing { ContentBox = "content-box", BorderBox = "border-box" });
css_enum!(Overflow { Visible = "visible", Hidden = "hidden", Clip = "clip", Scroll = "scroll", Auto = "auto" });
css_enum!(Visibility { Visible = "visible", Hidden = "hidden", Collapse = "collapse" });
css_enum!(TextAlign { Start = "start", End = "end", Left = "left", Right = "right", Center = "center", Justify = "justify", MatchParent = "match-parent", WebkitCenter = "-webkit-center" });
css_enum!(WhiteSpace { Normal = "normal", Pre = "pre", Nowrap = "nowrap", PreWrap = "pre-wrap", PreLine = "pre-line", BreakSpaces = "break-spaces" });

impl WhiteSpace {
    /// Runs of spaces and tabs collapse to one
    pub fn collapses_spaces(self) -> bool {
        matches!(self, WhiteSpace::Normal | WhiteSpace::Nowrap | WhiteSpace::PreLine)
    }

    /// Newlines are kept as forced breaks
    pub fn keeps_newlines(self) -> bool {
        !matches!(self, WhiteSpace::Normal | WhiteSpace::Nowrap)
    }

    /// Lines may wrap at soft opportunities
    pub fn wraps(self) -> bool {
        !matches!(self, WhiteSpace::Pre | WhiteSpace::Nowrap)
    }
}

css_enum!(TextTransform { None = "none", Capitalize = "capitalize", Uppercase = "uppercase", Lowercase = "lowercase" });
css_enum!(FontStyle { Normal = "normal", Italic = "italic", Oblique = "oblique" });
css_enum!(BorderStyle {
    None = "none",
    Hidden = "hidden",
    Solid = "solid",
    Dashed = "dashed",
    Dotted = "dotted",
    Double = "double",
    Groove = "groove",
    Ridge = "ridge",
    Inset = "inset",
    Outset = "outset",
});
css_enum!(ListStyleType {
    Disc = "disc",
    Circle = "circle",
    Square = "square",
    Decimal = "decimal",
    DecimalLeadingZero = "decimal-leading-zero",
    LowerAlpha = "lower-alpha",
    UpperAlpha = "upper-alpha",
    LowerRoman = "lower-roman",
    UpperRoman = "upper-roman",
    LowerGreek = "lower-greek",
    None = "none",
});
css_enum!(ListStylePosition { Outside = "outside", Inside = "inside" });
css_enum!(FlexDirection { Row = "row", RowReverse = "row-reverse", Column = "column", ColumnReverse = "column-reverse" });

impl FlexDirection {
    pub fn is_row(self) -> bool {
        matches!(self, FlexDirection::Row | FlexDirection::RowReverse)
    }

    pub fn is_reverse(self) -> bool {
        matches!(self, FlexDirection::RowReverse | FlexDirection::ColumnReverse)
    }
}

css_enum!(FlexWrap { Nowrap = "nowrap", Wrap = "wrap", WrapReverse = "wrap-reverse" });
css_enum!(JustifyContent {
    Normal = "normal",
    FlexStart = "flex-start",
    FlexEnd = "flex-end",
    Center = "center",
    SpaceBetween = "space-between",
    SpaceAround = "space-around",
    SpaceEvenly = "space-evenly",
    Start = "start",
    End = "end",
    Left = "left",
    Right = "right",
    Stretch = "stretch",
});
css_enum!(AlignItems {
    Normal = "normal",
    Stretch = "stretch",
    FlexStart = "flex-start",
    FlexEnd = "flex-end",
    Center = "center",
    Baseline = "baseline",
    Start = "start",
    End = "end",
    SelfStart = "self-start",
    SelfEnd = "self-end",
});
css_enum!(AlignSelf {
    Auto = "auto",
    Normal = "normal",
    Stretch = "stretch",
    FlexStart = "flex-start",
    FlexEnd = "flex-end",
    Center = "center",
    Baseline = "baseline",
    Start = "start",
    End = "end",
    SelfStart = "self-start",
    SelfEnd = "self-end",
});
css_enum!(AlignContent {
    Normal = "normal",
    FlexStart = "flex-start",
    FlexEnd = "flex-end",
    Center = "center",
    SpaceBetween = "space-between",
    SpaceAround = "space-around",
    SpaceEvenly = "space-evenly",
    Stretch = "stretch",
    Start = "start",
    End = "end",
});
css_enum!(Direction { Ltr = "ltr", Rtl = "rtl" });
css_enum!(WordBreak { Normal = "normal", BreakAll = "break-all", KeepAll = "keep-all", BreakWord = "break-word" });
css_enum!(OverflowWrap { Normal = "normal", Anywhere = "anywhere", BreakWord = "break-word" });
css_enum!(TextOverflow { Clip = "clip", Ellipsis = "ellipsis" });
css_enum!(ObjectFit { Fill = "fill", Contain = "contain", Cover = "cover", None = "none", ScaleDown = "scale-down" });
css_enum!(TextDecorationStyle { Solid = "solid", Double = "double", Dotted = "dotted", Dashed = "dashed", Wavy = "wavy" });
css_enum!(BackgroundRepeat { Repeat = "repeat", RepeatX = "repeat-x", RepeatY = "repeat-y", NoRepeat = "no-repeat", Space = "space", Round = "round" });
css_enum!(BorderCollapse { Separate = "separate", Collapse = "collapse" });
css_enum!(TableLayout { Auto = "auto", Fixed = "fixed" });
css_enum!(PointerEvents { Auto = "auto", None = "none" });
css_enum!(VerticalAlignKeyword {
    Baseline = "baseline",
    Sub = "sub",
    Super = "super",
    TextTop = "text-top",
    TextBottom = "text-bottom",
    Middle = "middle",
    Top = "top",
    Bottom = "bottom",
});

/// `text-decoration-line` flags
pub mod decoration {
    pub const UNDERLINE: u8 = 1;
    pub const OVERLINE: u8 = 2;
    pub const LINE_THROUGH: u8 = 4;
}

/// A gradient color stop: color and optional position
#[derive(Clone, Debug, PartialEq)]
pub struct ColorStop {
    pub color: Color,
    /// `None` when the position is implied by its neighbors
    pub position: Option<Lp>,
}

/// A gradient line's direction
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientDirection {
    /// Degrees clockwise from "to top"
    Angle(f32),
    /// `to <corner>`: horizontal and vertical signs (-1 left/top, 1 right/bottom)
    Corner(i8, i8),
}

/// The size of a radial gradient's ending shape
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RadialSize {
    ClosestSide,
    FarthestSide,
    ClosestCorner,
    FarthestCorner,
    /// Explicit radii (one for circles)
    Explicit(Lp, Lp),
}

/// A `background-image` layer
#[derive(Clone, Debug, PartialEq)]
pub enum Image {
    None,
    Url(Arc<str>),
    Linear { direction: GradientDirection, stops: Arc<[ColorStop]>, repeating: bool },
    Radial { circle: bool, size: RadialSize, center: (Lp, Lp), stops: Arc<[ColorStop]>, repeating: bool },
}

/// A `background-size` layer
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BackgroundSize {
    #[default]
    Auto,
    Cover,
    Contain,
    Explicit(LpAuto, LpAuto),
}

// ---- groups ----

/// Inherited properties
#[derive(Clone, Debug, PartialEq)]
pub struct InheritedStyle {
    pub color: Color,
    /// Font families, most preferred first (generic names lowercase)
    pub font_family: Arc<[Arc<str>]>,
    pub font_size: f32,
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub line_height: LineHeight,
    pub text_align: TextAlign,
    pub text_indent: Lp,
    pub text_transform: TextTransform,
    pub white_space: WhiteSpace,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub visibility: Visibility,
    pub list_style_type: ListStyleType,
    pub list_style_position: ListStylePosition,
    pub direction: Direction,
    pub word_break: WordBreak,
    pub overflow_wrap: OverflowWrap,
    pub border_collapse: BorderCollapse,
    pub border_spacing: (f32, f32),
    pub pointer_events: PointerEvents,
}

impl Default for InheritedStyle {
    fn default() -> Self {
        InheritedStyle {
            color: Color::BLACK,
            font_family: Arc::from([Arc::from("serif")]),
            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            line_height: LineHeight::Normal,
            text_align: TextAlign::Start,
            text_indent: Lp::ZERO,
            text_transform: TextTransform::None,
            white_space: WhiteSpace::Normal,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            visibility: Visibility::Visible,
            list_style_type: ListStyleType::Disc,
            list_style_position: ListStylePosition::Outside,
            direction: Direction::Ltr,
            word_break: WordBreak::Normal,
            overflow_wrap: OverflowWrap::Normal,
            border_collapse: BorderCollapse::Separate,
            border_spacing: (0.0, 0.0),
            pointer_events: PointerEvents::Auto,
        }
    }
}

/// A piece of generated content
#[derive(Clone, Debug, PartialEq)]
pub enum ContentItem {
    Text(Arc<str>),
    /// The element's attribute (lowercase name)
    Attr(Arc<str>),
    OpenQuote,
    CloseQuote,
    /// A counter's value, in a list style (the `ListStyleType`
    /// discriminant; decimal when absent)
    Counter(Arc<str>, Option<u8>),
}

/// Box model, positioning, flex and other non-inherited properties
#[derive(Clone, Debug, PartialEq)]
pub struct BoxStyle {
    pub display: Display,
    pub position: Position,
    pub float: Float,
    pub clear: Clear,
    pub box_sizing: BoxSizing,
    pub width: LpAuto,
    pub height: LpAuto,
    pub min_width: LpAuto,
    pub min_height: LpAuto,
    pub max_width: MaxSize,
    pub max_height: MaxSize,
    /// top, right, bottom, left
    pub margin: [LpAuto; 4],
    pub padding: [Lp; 4],
    pub inset: [LpAuto; 4],
    pub overflow_x: Overflow,
    pub overflow_y: Overflow,
    pub z_index: Option<i32>,
    pub vertical_align: VerticalAlign,
    pub opacity: f32,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: LpAuto,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    pub align_self: AlignSelf,
    pub align_content: AlignContent,
    pub order: i32,
    pub row_gap: Option<Lp>,
    pub column_gap: Option<Lp>,
    pub object_fit: ObjectFit,
    pub text_overflow: TextOverflow,
    pub table_layout: TableLayout,
    /// Grid container and item properties
    pub grid: crate::grid::GridStyle,
    /// What a `::before`/`::after` box shows (`None`: none or normal)
    pub content: Option<Arc<[ContentItem]>>,
    pub text_decoration_line: u8,
    /// `None` is currentcolor
    pub text_decoration_color: Option<Color>,
    pub text_decoration_style: TextDecorationStyle,
}

impl Default for BoxStyle {
    fn default() -> Self {
        BoxStyle {
            display: Display::Inline,
            position: Position::Static,
            float: Float::None,
            clear: Clear::None,
            box_sizing: BoxSizing::ContentBox,
            width: LpAuto::Auto,
            height: LpAuto::Auto,
            min_width: LpAuto::Auto,
            min_height: LpAuto::Auto,
            max_width: MaxSize::None,
            max_height: MaxSize::None,
            margin: [LpAuto::ZERO; 4],
            padding: [Lp::ZERO; 4],
            inset: [LpAuto::Auto; 4],
            overflow_x: Overflow::Visible,
            overflow_y: Overflow::Visible,
            z_index: None,
            vertical_align: VerticalAlign::Baseline,
            opacity: 1.0,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::Nowrap,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: LpAuto::Auto,
            justify_content: JustifyContent::Normal,
            align_items: AlignItems::Normal,
            align_self: AlignSelf::Auto,
            align_content: AlignContent::Normal,
            order: 0,
            row_gap: None,
            column_gap: None,
            object_fit: ObjectFit::Fill,
            text_overflow: TextOverflow::Clip,
            table_layout: TableLayout::Auto,
            grid: Default::default(),
            content: None,
            text_decoration_line: 0,
            text_decoration_color: None,
            text_decoration_style: TextDecorationStyle::Solid,
        }
    }
}

/// Borders and outline
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BorderStyles {
    /// Used widths in px (0 when the style is none or hidden): top, right,
    /// bottom, left
    pub width: [f32; 4],
    pub style: [BorderStyle; 4],
    /// `None` is currentcolor
    pub color: [Option<Color>; 4],
    /// Horizontal and vertical radii: top-left, top-right, bottom-right,
    /// bottom-left
    pub radius: [(Lp, Lp); 4],
    pub outline_width: f32,
    pub outline_style: BorderStyle,
    pub outline_color: Option<Color>,
    pub outline_offset: f32,
}

impl BorderStyles {
    pub fn has_border(&self) -> bool {
        self.width.iter().any(|&w| w > 0.0)
    }

    pub fn has_radius(&self) -> bool {
        self.radius.iter().any(|(a, b)| !a.is_zero() && !b.is_zero())
    }
}

/// Backgrounds
#[derive(Clone, Debug, PartialEq)]
pub struct BackgroundStyle {
    pub color: Color,
    /// Layers, top first
    pub images: Arc<[Image]>,
    pub repeat: Arc<[(BackgroundRepeat, BackgroundRepeat)]>,
    pub position: Arc<[(Lp, Lp)]>,
    pub size: Arc<[BackgroundSize]>,
    /// `mask-image` (one layer, a URL) and its size, position and repeat
    pub mask: Option<Arc<str>>,
    pub mask_size: BackgroundSize,
    pub mask_position: (Lp, Lp),
    pub mask_repeat: (BackgroundRepeat, BackgroundRepeat),
}

impl Default for BackgroundStyle {
    fn default() -> Self {
        BackgroundStyle {
            color: Color::TRANSPARENT,
            images: Arc::from([]),
            repeat: Arc::from([(BackgroundRepeat::Repeat, BackgroundRepeat::Repeat)]),
            position: Arc::from([(Lp::ZERO, Lp::ZERO)]),
            size: Arc::from([BackgroundSize::Auto]),
            mask: None,
            mask_size: BackgroundSize::Auto,
            mask_position: (Lp::ZERO, Lp::ZERO),
            mask_repeat: (BackgroundRepeat::Repeat, BackgroundRepeat::Repeat),
        }
    }
}

impl BackgroundStyle {
    /// Whether anything is painted
    pub fn is_visible(&self) -> bool {
        self.color.a > 0 || self.images.iter().any(|i| !matches!(i, Image::None))
    }
}

static DEFAULT_INHERITED: LazyLock<Arc<InheritedStyle>> = LazyLock::new(|| Arc::new(InheritedStyle::default()));
static DEFAULT_BOX: LazyLock<Arc<BoxStyle>> = LazyLock::new(|| Arc::new(BoxStyle::default()));
static DEFAULT_BORDER: LazyLock<Arc<BorderStyles>> = LazyLock::new(|| Arc::new(BorderStyles::default()));
static DEFAULT_BACKGROUND: LazyLock<Arc<BackgroundStyle>> = LazyLock::new(|| Arc::new(BackgroundStyle::default()));
static INITIAL: LazyLock<Style> = LazyLock::new(Style::initial);

/// An element's computed style
#[derive(Clone, Debug)]
pub struct Style {
    pub inherited: Arc<InheritedStyle>,
    pub box_: Arc<BoxStyle>,
    pub border: Arc<BorderStyles>,
    pub background: Arc<BackgroundStyle>,
    /// Custom properties (`--name`), inherited
    pub custom: Option<Arc<crate::values::CustomProperties>>,
}

impl Default for Style {
    fn default() -> Self {
        INITIAL.clone()
    }
}

impl PartialEq for Style {
    fn eq(&self, o: &Style) -> bool {
        let same = |a: bool, b: bool| a || b;
        same(Arc::ptr_eq(&self.inherited, &o.inherited), self.inherited == o.inherited)
            && same(Arc::ptr_eq(&self.box_, &o.box_), self.box_ == o.box_)
            && same(Arc::ptr_eq(&self.border, &o.border), self.border == o.border)
            && same(Arc::ptr_eq(&self.background, &o.background), self.background == o.background)
            && match (&self.custom, &o.custom) {
                (None, None) => true,
                (Some(a), Some(b)) => Arc::ptr_eq(a, b) || a == b,
                _ => false,
            }
    }
}

/// What lengths are computed against
#[derive(Clone, Copy, Debug)]
pub struct StyleContext {
    /// The root element's font size (`rem`)
    pub root_font_size: f32,
    /// The viewport (`vw`, `vh`, `vmin`, `vmax`)
    pub viewport: (f32, f32),
}

impl Default for StyleContext {
    fn default() -> Self {
        StyleContext { root_font_size: 16.0, viewport: (1024.0, 768.0) }
    }
}

impl Style {
    fn initial() -> Style {
        Style {
            inherited: DEFAULT_INHERITED.clone(),
            box_: DEFAULT_BOX.clone(),
            border: DEFAULT_BORDER.clone(),
            background: DEFAULT_BACKGROUND.clone(),
            custom: None,
        }
    }

    /// The initial values of every property
    pub fn initial_ref() -> &'static Style {
        &INITIAL
    }

    /// A child's starting point: inherited properties from `parent`, the
    /// initial values for the rest
    pub fn inherit_from(parent: &Style) -> Style {
        Style {
            inherited: parent.inherited.clone(),
            box_: DEFAULT_BOX.clone(),
            border: DEFAULT_BORDER.clone(),
            background: DEFAULT_BACKGROUND.clone(),
            custom: parent.custom.clone(),
        }
    }

    // Shorthands for the common reads

    pub fn display(&self) -> Display {
        self.box_.display
    }

    pub fn font_size(&self) -> f32 {
        self.inherited.font_size
    }

    pub fn color(&self) -> Color {
        self.inherited.color
    }

    /// The used line height in px
    pub fn line_height_px(&self) -> f32 {
        self.inherited.line_height.resolve(self.inherited.font_size)
    }

    pub fn is_positioned(&self) -> bool {
        self.box_.position != Position::Static
    }

    pub fn is_out_of_flow(&self) -> bool {
        matches!(self.box_.position, Position::Absolute | Position::Fixed)
    }

    /// Clips its content (overflow other than visible)
    pub fn clips(&self) -> bool {
        self.box_.overflow_x != Overflow::Visible || self.box_.overflow_y != Overflow::Visible
    }

    /// Establishes a stacking context
    pub fn is_stacking_context(&self) -> bool {
        (self.is_positioned() && self.box_.z_index.is_some()) || self.box_.opacity < 1.0 || matches!(self.box_.position, Position::Fixed | Position::Sticky)
    }

    /// Apply an element's declarations in cascade order (lowest priority
    /// first): custom properties, then font-size (what `em` depends on),
    /// then the rest. `parent` is what `inherit` refers to.
    pub fn cascade(&mut self, decls: &[&Declaration], parent: &Style, ctx: &StyleContext, cache: &mut crate::values::ResolveCache) {
        let own: Vec<(&str, &str)> = decls
            .iter()
            .filter_map(|d| match &d.value {
                PropertyValue::Custom(b) => Some((b.0.as_str(), b.1.as_str())),
                _ => None,
            })
            .collect();
        if !own.is_empty() {
            let computed = crate::values::compute_custom_properties(self.custom.as_ref(), &own);
            self.custom = Some(Arc::new(computed));
        }
        let parent_font_size = parent.inherited.font_size;
        for font_pass in [true, false] {
            for d in decls {
                let is_font = match &d.value {
                    PropertyValue::Custom(_) => continue,
                    PropertyValue::Unresolved(b) => b.0 == "font-size" || b.0 == "font",
                    _ => d.property == PropertyId::FontSize,
                };
                if is_font != font_pass {
                    continue;
                }
                match &d.value {
                    PropertyValue::Unresolved(b) => {
                        let custom = self.custom.clone();
                        let rctx = crate::values::ResolveContext {
                            custom: custom.as_deref().map(|c| c as &dyn crate::values::VarSource),
                            font_size: self.inherited.font_size,
                            parent_font_size,
                            root_font_size: ctx.root_font_size,
                            viewport: ctx.viewport,
                        };
                        let resolved = cache.resolve(&b.0, &b.1, d.important, custom.as_ref(), &rctx).to_vec();
                        for r in &resolved {
                            self.apply(r.property, &r.value, parent, ctx);
                        }
                    }
                    v => self.apply(d.property, v, parent, ctx),
                }
            }
        }
        self.finish();
    }

    /// Computed-value fixups once every declaration applied
    pub fn finish(&mut self) {
        // A border whose style is none or hidden has no width; used widths
        // snap to whole pixels (at least one), as browsers draw them
        let b = &self.border;
        let fix = |w: f32, s: BorderStyle| if matches!(s, BorderStyle::None | BorderStyle::Hidden) || w <= 0.0 { 0.0 } else if w < 1.0 { 1.0 } else { w.floor() };
        let widths = [0, 1, 2, 3].map(|i| fix(b.width[i], b.style[i]));
        let outline = fix(b.outline_width, b.outline_style);
        if widths != b.width || outline != b.outline_width {
            let b = Arc::make_mut(&mut self.border);
            b.width = widths;
            b.outline_width = outline;
        }
        // Floats and absolutely positioned boxes are blocks
        let bx = &self.box_;
        let out_of_flow = matches!(bx.position, Position::Absolute | Position::Fixed);
        if (out_of_flow || bx.float != Float::None) && bx.display != Display::None && bx.display != Display::Contents {
            let blockified = bx.display.blockified();
            if blockified != bx.display || (out_of_flow && bx.float != Float::None) {
                let bx = Arc::make_mut(&mut self.box_);
                bx.display = blockified;
                if out_of_flow {
                    bx.float = Float::None;
                }
            }
        }
    }

    /// Lengths to pixels (percentages kept)
    fn lp(&self, v: &PropertyValue, ctx: &StyleContext) -> Option<Lp> {
        match *v {
            PropertyValue::Length(l) => Some(self.length(l, ctx)),
            PropertyValue::Mix { px, pct } => Some(Lp { px, pct }),
            PropertyValue::Number(n) if n == 0.0 => Some(Lp::ZERO),
            PropertyValue::Integer(0) => Some(Lp::ZERO),
            _ => None,
        }
    }

    fn length(&self, l: Length, ctx: &StyleContext) -> Lp {
        let fs = self.inherited.font_size;
        let (w, h) = ctx.viewport;
        let px = match l.unit {
            LengthUnit::Px => l.value,
            LengthUnit::Percent => return Lp::pct(l.value),
            LengthUnit::Em => l.value * fs,
            LengthUnit::Rem => l.value * ctx.root_font_size,
            LengthUnit::Vw => l.value * w / 100.0,
            LengthUnit::Vh => l.value * h / 100.0,
            LengthUnit::Vmin => l.value * w.min(h) / 100.0,
            LengthUnit::Vmax => l.value * w.max(h) / 100.0,
            LengthUnit::Ch => l.value * fs * 0.5,
            LengthUnit::Ex => l.value * fs * 0.5,
        };
        Lp::px(px)
    }

    fn repeat_layers(v: &PropertyValue) -> Option<Vec<(BackgroundRepeat, BackgroundRepeat)>> {
        let PropertyValue::List(layers) = v else { return None };
        let r: Option<Vec<_>> = layers
            .iter()
            .map(|l| match l {
                PropertyValue::List(p) if p.len() == 2 => match (&p[0], &p[1]) {
                    (PropertyValue::Enum(x), PropertyValue::Enum(y)) => Some((BackgroundRepeat::from_u8(*x)?, BackgroundRepeat::from_u8(*y)?)),
                    _ => None,
                },
                _ => None,
            })
            .collect();
        r.filter(|r| !r.is_empty())
    }

    fn position_layers(&self, v: &PropertyValue, ctx: &StyleContext) -> Option<Vec<(Lp, Lp)>> {
        let PropertyValue::List(layers) = v else { return None };
        let p: Option<Vec<_>> = layers
            .iter()
            .map(|l| match l {
                PropertyValue::List(p) if p.len() == 2 => Some((self.lp(&p[0], ctx)?, self.lp(&p[1], ctx)?)),
                _ => None,
            })
            .collect();
        p.filter(|p| !p.is_empty())
    }

    fn size_layers(&self, v: &PropertyValue, ctx: &StyleContext) -> Option<Vec<BackgroundSize>> {
        let PropertyValue::List(layers) = v else { return None };
        let s: Option<Vec<_>> = layers
            .iter()
            .map(|l| match l {
                PropertyValue::Integer(1) => Some(BackgroundSize::Cover),
                PropertyValue::Integer(2) => Some(BackgroundSize::Contain),
                PropertyValue::List(p) if p.len() == 2 => {
                    let (w, h) = (self.lp_auto(&p[0], ctx)?, self.lp_auto(&p[1], ctx)?);
                    Some(if w.is_auto() && h.is_auto() { BackgroundSize::Auto } else { BackgroundSize::Explicit(w, h) })
                }
                _ => None,
            })
            .collect();
        s.filter(|s| !s.is_empty())
    }

    /// A length-percentage in text (grid tracks), including resolved
    /// mixes (`-fos-mix(Xpx Y%)`)
    fn lp_text(&self, t: &str, ctx: &StyleContext) -> Option<Lp> {
        if let Some(inner) = t.strip_prefix("-fos-mix(").and_then(|r| r.strip_suffix(')')) {
            let mut lp = Lp::ZERO;
            for part in inner.split_whitespace() {
                let l = crate::parser::parse_length(part)?;
                let v = self.length(l, ctx);
                lp.px += v.px;
                lp.pct += v.pct;
            }
            return Some(lp);
        }
        if t == "0" {
            return Some(Lp::ZERO);
        }
        crate::parser::parse_length(t).map(|l| self.length(l, ctx))
    }

    /// Pixels only (border widths, spacing); percentages have no meaning
    fn px(&self, v: &PropertyValue, ctx: &StyleContext) -> Option<f32> {
        self.lp(v, ctx).filter(|l| !l.has_percent()).map(|l| l.px)
    }

    fn lp_auto(&self, v: &PropertyValue, ctx: &StyleContext) -> Option<LpAuto> {
        match v {
            PropertyValue::Keyword(Keyword::Auto) => Some(LpAuto::Auto),
            _ => self.lp(v, ctx).map(LpAuto::Lp),
        }
    }

    fn color_value(&self, v: &PropertyValue) -> Option<Option<Color>> {
        match v {
            PropertyValue::Color(c) => Some(Some(*c)),
            PropertyValue::CurrentColor => Some(None),
            _ => None,
        }
    }

    /// Apply one longhand
    pub fn apply(&mut self, id: PropertyId, v: &PropertyValue, parent: &Style, ctx: &StyleContext) {
        if let PropertyValue::Keyword(k @ (Keyword::Inherit | Keyword::Initial | Keyword::Unset)) = v {
            let from = match k {
                Keyword::Inherit => parent,
                Keyword::Unset if id.is_inherited() => parent,
                _ => Style::initial_ref(),
            };
            self.copy_property(id, from);
            return;
        }
        macro_rules! set {
            ($group:ident, [$($place:tt)+], $val:expr) => {{
                let val = $val;
                if self.$group.$($place)+ != val {
                    Arc::make_mut(&mut self.$group).$($place)+ = val;
                }
            }};
        }
        macro_rules! enum_of {
            ($ty:ty) => {
                match v {
                    PropertyValue::Enum(e) => match <$ty>::from_u8(*e) {
                        Some(x) => x,
                        None => return,
                    },
                    _ => return,
                }
            };
        }
        match id {
            PropertyId::Display => set!(box_, [display], enum_of!(Display)),
            PropertyId::Position => set!(box_, [position], enum_of!(Position)),
            PropertyId::Float => set!(box_, [float], enum_of!(Float)),
            PropertyId::Clear => set!(box_, [clear], enum_of!(Clear)),
            PropertyId::BoxSizing => set!(box_, [box_sizing], enum_of!(BoxSizing)),
            PropertyId::Width | PropertyId::Height | PropertyId::MinWidth | PropertyId::MinHeight | PropertyId::FlexBasis => {
                let Some(val) = self.lp_auto(v, ctx) else { return };
                match id {
                    PropertyId::Width => set!(box_, [width], val),
                    PropertyId::Height => set!(box_, [height], val),
                    PropertyId::MinWidth => set!(box_, [min_width], val),
                    PropertyId::MinHeight => set!(box_, [min_height], val),
                    _ => set!(box_, [flex_basis], val),
                }
            }
            PropertyId::MaxWidth | PropertyId::MaxHeight => {
                let val = match v {
                    PropertyValue::Keyword(Keyword::None) => MaxSize::None,
                    _ => match self.lp(v, ctx) {
                        Some(l) => MaxSize::Lp(l),
                        None => return,
                    },
                };
                if id == PropertyId::MaxWidth {
                    set!(box_, [max_width], val)
                } else {
                    set!(box_, [max_height], val)
                }
            }
            PropertyId::MarginTop | PropertyId::MarginRight | PropertyId::MarginBottom | PropertyId::MarginLeft => {
                let Some(val) = self.lp_auto(v, ctx) else { return };
                let i = match id {
                    PropertyId::MarginTop => 0,
                    PropertyId::MarginRight => 1,
                    PropertyId::MarginBottom => 2,
                    _ => 3,
                };
                set!(box_, [margin[i]], val);
            }
            PropertyId::PaddingTop | PropertyId::PaddingRight | PropertyId::PaddingBottom | PropertyId::PaddingLeft => {
                let Some(val) = self.lp(v, ctx) else { return };
                let val = Lp { px: val.px.max(0.0), pct: val.pct.max(0.0) };
                let i = match id {
                    PropertyId::PaddingTop => 0,
                    PropertyId::PaddingRight => 1,
                    PropertyId::PaddingBottom => 2,
                    _ => 3,
                };
                set!(box_, [padding[i]], val);
            }
            PropertyId::Top | PropertyId::Right | PropertyId::Bottom | PropertyId::Left => {
                let Some(val) = self.lp_auto(v, ctx) else { return };
                let i = match id {
                    PropertyId::Top => 0,
                    PropertyId::Right => 1,
                    PropertyId::Bottom => 2,
                    _ => 3,
                };
                set!(box_, [inset[i]], val);
            }
            PropertyId::OverflowX => set!(box_, [overflow_x], enum_of!(Overflow)),
            PropertyId::OverflowY => set!(box_, [overflow_y], enum_of!(Overflow)),
            PropertyId::ZIndex => match v {
                PropertyValue::Keyword(Keyword::Auto) => set!(box_, [z_index], None),
                PropertyValue::Integer(n) => set!(box_, [z_index], Some(*n)),
                PropertyValue::Number(n) if n.fract() == 0.0 => set!(box_, [z_index], Some(*n as i32)),
                _ => {}
            },
            PropertyId::VerticalAlign => {
                let val = match v {
                    PropertyValue::Enum(e) => match VerticalAlignKeyword::from_u8(*e) {
                        Some(VerticalAlignKeyword::Baseline) => VerticalAlign::Baseline,
                        Some(VerticalAlignKeyword::Sub) => VerticalAlign::Sub,
                        Some(VerticalAlignKeyword::Super) => VerticalAlign::Super,
                        Some(VerticalAlignKeyword::TextTop) => VerticalAlign::TextTop,
                        Some(VerticalAlignKeyword::TextBottom) => VerticalAlign::TextBottom,
                        Some(VerticalAlignKeyword::Middle) => VerticalAlign::Middle,
                        Some(VerticalAlignKeyword::Top) => VerticalAlign::Top,
                        Some(VerticalAlignKeyword::Bottom) => VerticalAlign::Bottom,
                        None => return,
                    },
                    _ => match self.lp(v, ctx) {
                        Some(l) => VerticalAlign::Length(l),
                        None => return,
                    },
                };
                set!(box_, [vertical_align], val);
            }
            PropertyId::Opacity => {
                if let PropertyValue::Number(n) = v {
                    set!(box_, [opacity], n.clamp(0.0, 1.0));
                }
            }
            PropertyId::FlexDirection => set!(box_, [flex_direction], enum_of!(FlexDirection)),
            PropertyId::FlexWrap => set!(box_, [flex_wrap], enum_of!(FlexWrap)),
            PropertyId::FlexGrow | PropertyId::FlexShrink => {
                let n = match v {
                    PropertyValue::Number(n) if *n >= 0.0 => *n,
                    PropertyValue::Integer(n) if *n >= 0 => *n as f32,
                    _ => return,
                };
                if id == PropertyId::FlexGrow {
                    set!(box_, [flex_grow], n)
                } else {
                    set!(box_, [flex_shrink], n)
                }
            }
            PropertyId::JustifyContent => set!(box_, [justify_content], enum_of!(JustifyContent)),
            PropertyId::AlignItems => set!(box_, [align_items], enum_of!(AlignItems)),
            PropertyId::AlignSelf => set!(box_, [align_self], enum_of!(AlignSelf)),
            PropertyId::AlignContent => set!(box_, [align_content], enum_of!(AlignContent)),
            PropertyId::Order => match v {
                PropertyValue::Integer(n) => set!(box_, [order], *n),
                PropertyValue::Number(n) => set!(box_, [order], *n as i32),
                _ => {}
            },
            PropertyId::RowGap | PropertyId::ColumnGap => {
                let val = match v {
                    PropertyValue::Keyword(Keyword::Normal) => None,
                    _ => match self.lp(v, ctx) {
                        Some(l) => Some(l),
                        None => return,
                    },
                };
                if id == PropertyId::RowGap {
                    set!(box_, [row_gap], val)
                } else {
                    set!(box_, [column_gap], val)
                }
            }
            PropertyId::ObjectFit => set!(box_, [object_fit], enum_of!(ObjectFit)),
            PropertyId::TextOverflow => set!(box_, [text_overflow], enum_of!(TextOverflow)),
            PropertyId::TableLayout => set!(box_, [table_layout], enum_of!(TableLayout)),
            PropertyId::GridTemplateColumns | PropertyId::GridTemplateRows | PropertyId::GridAutoColumns | PropertyId::GridAutoRows => {
                let PropertyValue::Grid(text) = v else { return };
                let len = |t: &str| self.lp_text(t, ctx);
                match id {
                    PropertyId::GridTemplateColumns | PropertyId::GridTemplateRows => {
                        let Some(list) = crate::grid::parse_track_list(text, &len) else { return };
                        if id == PropertyId::GridTemplateColumns {
                            set!(box_, [grid.template_columns], list);
                        } else {
                            set!(box_, [grid.template_rows], list);
                        }
                    }
                    _ => {
                        let Some(list) = crate::grid::parse_auto_tracks(text, &len) else { return };
                        if id == PropertyId::GridAutoColumns {
                            set!(box_, [grid.auto_columns], list);
                        } else {
                            set!(box_, [grid.auto_rows], list);
                        }
                    }
                }
            }
            PropertyId::Content => {
                if let PropertyValue::Content(items) = v {
                    set!(box_, [content], (!items.is_empty()).then(|| items.clone()));
                }
            }
            PropertyId::GridTemplateAreas => {
                if let PropertyValue::Grid(text) = v {
                    if let Some(areas) = crate::grid::parse_areas(text) {
                        set!(box_, [grid.areas], areas);
                    }
                }
            }
            PropertyId::GridAutoFlow => {
                if let Some((column, dense)) = match v { PropertyValue::Grid(t) => crate::grid::parse_flow(t), _ => None } {
                    set!(box_, [grid.flow_column], column);
                    set!(box_, [grid.flow_dense], dense);
                }
            }
            PropertyId::GridColumnStart | PropertyId::GridColumnEnd | PropertyId::GridRowStart | PropertyId::GridRowEnd => {
                let Some(line) = (match v { PropertyValue::Grid(t) => crate::grid::parse_line(t), _ => None }) else { return };
                match id {
                    PropertyId::GridColumnStart => set!(box_, [grid.column_start], line),
                    PropertyId::GridColumnEnd => set!(box_, [grid.column_end], line),
                    PropertyId::GridRowStart => set!(box_, [grid.row_start], line),
                    _ => set!(box_, [grid.row_end], line),
                }
            }
            PropertyId::JustifyItems => set!(box_, [grid.justify_items], enum_of!(AlignItems) as u8),
            PropertyId::JustifySelf => set!(box_, [grid.justify_self], enum_of!(AlignSelf) as u8),
            PropertyId::TextDecorationLine => {
                if let PropertyValue::Integer(n) = v {
                    set!(box_, [text_decoration_line], *n as u8);
                }
            }
            PropertyId::TextDecorationColor => {
                if let Some(c) = self.color_value(v) {
                    set!(box_, [text_decoration_color], c);
                }
            }
            PropertyId::TextDecorationStyle => set!(box_, [text_decoration_style], enum_of!(TextDecorationStyle)),

            // Borders
            PropertyId::BorderTopWidth | PropertyId::BorderRightWidth | PropertyId::BorderBottomWidth | PropertyId::BorderLeftWidth => {
                let Some(px) = self.px(v, ctx) else { return };
                let i = match id {
                    PropertyId::BorderTopWidth => 0,
                    PropertyId::BorderRightWidth => 1,
                    PropertyId::BorderBottomWidth => 2,
                    _ => 3,
                };
                set!(border, [width[i]], px.max(0.0));
            }
            PropertyId::BorderTopStyle | PropertyId::BorderRightStyle | PropertyId::BorderBottomStyle | PropertyId::BorderLeftStyle => {
                let s = enum_of!(BorderStyle);
                let i = match id {
                    PropertyId::BorderTopStyle => 0,
                    PropertyId::BorderRightStyle => 1,
                    PropertyId::BorderBottomStyle => 2,
                    _ => 3,
                };
                set!(border, [style[i]], s);
            }
            PropertyId::BorderTopColor | PropertyId::BorderRightColor | PropertyId::BorderBottomColor | PropertyId::BorderLeftColor => {
                let Some(c) = self.color_value(v) else { return };
                let i = match id {
                    PropertyId::BorderTopColor => 0,
                    PropertyId::BorderRightColor => 1,
                    PropertyId::BorderBottomColor => 2,
                    _ => 3,
                };
                set!(border, [color[i]], c);
            }
            PropertyId::BorderTopLeftRadius | PropertyId::BorderTopRightRadius | PropertyId::BorderBottomRightRadius | PropertyId::BorderBottomLeftRadius => {
                let (a, b) = match v {
                    PropertyValue::List(l) if l.len() == 2 => match (self.lp(&l[0], ctx), self.lp(&l[1], ctx)) {
                        (Some(a), Some(b)) => (a, b),
                        _ => return,
                    },
                    _ => match self.lp(v, ctx) {
                        Some(a) => (a, a),
                        None => return,
                    },
                };
                let i = match id {
                    PropertyId::BorderTopLeftRadius => 0,
                    PropertyId::BorderTopRightRadius => 1,
                    PropertyId::BorderBottomRightRadius => 2,
                    _ => 3,
                };
                set!(border, [radius[i]], (a, b));
            }
            PropertyId::OutlineWidth => {
                if let Some(px) = self.px(v, ctx) {
                    set!(border, [outline_width], px.max(0.0));
                }
            }
            PropertyId::OutlineStyle => set!(border, [outline_style], enum_of!(BorderStyle)),
            PropertyId::OutlineColor => {
                if let Some(c) = self.color_value(v) {
                    set!(border, [outline_color], c);
                }
            }
            PropertyId::OutlineOffset => {
                if let Some(px) = self.px(v, ctx) {
                    set!(border, [outline_offset], px);
                }
            }

            // Backgrounds
            PropertyId::BackgroundColor => {
                if let Some(c) = self.color_value(v) {
                    set!(background, [color], c.unwrap_or(self.inherited.color));
                }
            }
            PropertyId::BackgroundImage => {
                if let PropertyValue::Images(images) = v {
                    set!(background, [images], images.clone());
                }
            }
            PropertyId::BackgroundRepeat => {
                if let Some(r) = Self::repeat_layers(v) {
                    set!(background, [repeat], Arc::from(r));
                }
            }
            PropertyId::BackgroundPosition => {
                if let Some(p) = self.position_layers(v, ctx) {
                    set!(background, [position], Arc::from(p));
                }
            }
            PropertyId::BackgroundSize => {
                if let Some(s) = self.size_layers(v, ctx) {
                    set!(background, [size], Arc::from(s));
                }
            }
            PropertyId::MaskImage => {
                if let PropertyValue::Images(images) = v {
                    let url = images.iter().find_map(|i| if let Image::Url(u) = i { Some(u.clone()) } else { None });
                    set!(background, [mask], url);
                }
            }
            PropertyId::MaskRepeat => {
                if let Some(r) = Self::repeat_layers(v).and_then(|r| r.into_iter().next()) {
                    set!(background, [mask_repeat], r);
                }
            }
            PropertyId::MaskPosition => {
                if let Some(p) = self.position_layers(v, ctx).and_then(|p| p.into_iter().next()) {
                    set!(background, [mask_position], p);
                }
            }
            PropertyId::MaskSize => {
                if let Some(sz) = self.size_layers(v, ctx).and_then(|sz| sz.into_iter().next()) {
                    set!(background, [mask_size], sz);
                }
            }

            // Inherited
            PropertyId::Color => {
                if let Some(c) = self.color_value(v) {
                    set!(inherited, [color], c.unwrap_or(parent.inherited.color));
                }
            }
            PropertyId::FontFamily => {
                if let PropertyValue::String(list) = v {
                    let families: Vec<Arc<str>> = list.split(',').map(|f| Arc::from(f.trim())).filter(|f: &Arc<str>| !f.is_empty()).collect();
                    if !families.is_empty() && *self.inherited.font_family != *families {
                        Arc::make_mut(&mut self.inherited).font_family = Arc::from(families);
                    }
                }
            }
            PropertyId::FontSize => {
                let parent_fs = parent.inherited.font_size;
                let px = match v {
                    PropertyValue::Length(l) => match l.unit {
                        LengthUnit::Em => l.value * parent_fs,
                        LengthUnit::Percent => l.value * parent_fs / 100.0,
                        LengthUnit::Ch | LengthUnit::Ex => l.value * parent_fs * 0.5,
                        _ => self.length(*l, ctx).px,
                    },
                    PropertyValue::Mix { px, pct } => px + pct * parent_fs / 100.0,
                    _ => return,
                };
                if px.is_finite() && px >= 0.0 {
                    set!(inherited, [font_size], px);
                }
            }
            PropertyId::FontWeight => {
                let w = match v {
                    PropertyValue::Integer(w) => *w,
                    PropertyValue::Number(w) => *w as i32,
                    // bolder / lighter, relative to the parent
                    PropertyValue::Keyword(Keyword::Bolder) => match parent.inherited.font_weight {
                        0..=349 => 400,
                        350..=549 => 700,
                        _ => 900,
                    },
                    PropertyValue::Keyword(Keyword::Lighter) => match parent.inherited.font_weight {
                        0..=549 => 100,
                        550..=749 => 400,
                        _ => 700,
                    },
                    _ => return,
                };
                set!(inherited, [font_weight], w.clamp(1, 1000) as u16);
            }
            PropertyId::FontStyle => set!(inherited, [font_style], enum_of!(FontStyle)),
            PropertyId::LineHeight => {
                let fs = self.inherited.font_size;
                let lh = match v {
                    PropertyValue::Keyword(Keyword::Normal) => LineHeight::Normal,
                    PropertyValue::Number(n) if *n >= 0.0 => LineHeight::Number(*n),
                    PropertyValue::Integer(n) if *n >= 0 => LineHeight::Number(*n as f32),
                    _ => match self.lp(v, ctx) {
                        Some(l) => LineHeight::Px(l.resolve(fs).max(0.0)),
                        None => return,
                    },
                };
                set!(inherited, [line_height], lh);
            }
            PropertyId::TextAlign => set!(inherited, [text_align], enum_of!(TextAlign)),
            PropertyId::TextIndent => {
                if let Some(l) = self.lp(v, ctx) {
                    set!(inherited, [text_indent], l);
                }
            }
            PropertyId::TextTransform => set!(inherited, [text_transform], enum_of!(TextTransform)),
            PropertyId::WhiteSpace => set!(inherited, [white_space], enum_of!(WhiteSpace)),
            PropertyId::LetterSpacing | PropertyId::WordSpacing => {
                let px = match v {
                    PropertyValue::Keyword(Keyword::Normal) => 0.0,
                    _ => match self.lp(v, ctx) {
                        // Percentages of word-spacing are of the space width: approximate
                        Some(l) => l.px + l.pct * self.inherited.font_size * 0.25 / 100.0,
                        None => return,
                    },
                };
                if id == PropertyId::LetterSpacing {
                    set!(inherited, [letter_spacing], px)
                } else {
                    set!(inherited, [word_spacing], px)
                }
            }
            PropertyId::Visibility => set!(inherited, [visibility], enum_of!(Visibility)),
            PropertyId::ListStyleType => set!(inherited, [list_style_type], enum_of!(ListStyleType)),
            PropertyId::ListStylePosition => set!(inherited, [list_style_position], enum_of!(ListStylePosition)),
            PropertyId::Direction => set!(inherited, [direction], enum_of!(Direction)),
            PropertyId::WordBreak => set!(inherited, [word_break], enum_of!(WordBreak)),
            PropertyId::OverflowWrap => set!(inherited, [overflow_wrap], enum_of!(OverflowWrap)),
            PropertyId::BorderCollapse => set!(inherited, [border_collapse], enum_of!(BorderCollapse)),
            PropertyId::BorderSpacing => {
                let (h, w) = match v {
                    PropertyValue::List(l) if l.len() == 2 => match (self.px(&l[0], ctx), self.px(&l[1], ctx)) {
                        (Some(a), Some(b)) => (a, b),
                        _ => return,
                    },
                    _ => match self.px(v, ctx) {
                        Some(a) => (a, a),
                        None => return,
                    },
                };
                set!(inherited, [border_spacing], (h.max(0.0), w.max(0.0)));
            }
            PropertyId::PointerEvents => set!(inherited, [pointer_events], enum_of!(PointerEvents)),
            _ => {}
        }
    }

    /// `inherit` / `initial`: take one property's value from `from`
    fn copy_property(&mut self, id: PropertyId, from: &Style) {
        macro_rules! copy {
            ($group:ident, [$($place:tt)+]) => {{
                if self.$group.$($place)+ != from.$group.$($place)+ {
                    Arc::make_mut(&mut self.$group).$($place)+ = from.$group.$($place)+.clone();
                }
            }};
        }
        match id {
            PropertyId::Display => copy!(box_, [display]),
            PropertyId::Position => copy!(box_, [position]),
            PropertyId::Float => copy!(box_, [float]),
            PropertyId::Clear => copy!(box_, [clear]),
            PropertyId::BoxSizing => copy!(box_, [box_sizing]),
            PropertyId::Width => copy!(box_, [width]),
            PropertyId::Height => copy!(box_, [height]),
            PropertyId::MinWidth => copy!(box_, [min_width]),
            PropertyId::MinHeight => copy!(box_, [min_height]),
            PropertyId::MaxWidth => copy!(box_, [max_width]),
            PropertyId::MaxHeight => copy!(box_, [max_height]),
            PropertyId::FlexBasis => copy!(box_, [flex_basis]),
            PropertyId::MarginTop => copy!(box_, [margin[0]]),
            PropertyId::MarginRight => copy!(box_, [margin[1]]),
            PropertyId::MarginBottom => copy!(box_, [margin[2]]),
            PropertyId::MarginLeft => copy!(box_, [margin[3]]),
            PropertyId::PaddingTop => copy!(box_, [padding[0]]),
            PropertyId::PaddingRight => copy!(box_, [padding[1]]),
            PropertyId::PaddingBottom => copy!(box_, [padding[2]]),
            PropertyId::PaddingLeft => copy!(box_, [padding[3]]),
            PropertyId::Top => copy!(box_, [inset[0]]),
            PropertyId::Right => copy!(box_, [inset[1]]),
            PropertyId::Bottom => copy!(box_, [inset[2]]),
            PropertyId::Left => copy!(box_, [inset[3]]),
            PropertyId::OverflowX => copy!(box_, [overflow_x]),
            PropertyId::OverflowY => copy!(box_, [overflow_y]),
            PropertyId::ZIndex => copy!(box_, [z_index]),
            PropertyId::VerticalAlign => copy!(box_, [vertical_align]),
            PropertyId::Opacity => copy!(box_, [opacity]),
            PropertyId::FlexDirection => copy!(box_, [flex_direction]),
            PropertyId::FlexWrap => copy!(box_, [flex_wrap]),
            PropertyId::FlexGrow => copy!(box_, [flex_grow]),
            PropertyId::FlexShrink => copy!(box_, [flex_shrink]),
            PropertyId::JustifyContent => copy!(box_, [justify_content]),
            PropertyId::AlignItems => copy!(box_, [align_items]),
            PropertyId::AlignSelf => copy!(box_, [align_self]),
            PropertyId::AlignContent => copy!(box_, [align_content]),
            PropertyId::Order => copy!(box_, [order]),
            PropertyId::RowGap => copy!(box_, [row_gap]),
            PropertyId::ColumnGap => copy!(box_, [column_gap]),
            PropertyId::ObjectFit => copy!(box_, [object_fit]),
            PropertyId::TextOverflow => copy!(box_, [text_overflow]),
            PropertyId::TableLayout => copy!(box_, [table_layout]),
            PropertyId::GridTemplateColumns => copy!(box_, [grid.template_columns]),
            PropertyId::GridTemplateRows => copy!(box_, [grid.template_rows]),
            PropertyId::GridTemplateAreas => copy!(box_, [grid.areas]),
            PropertyId::Content => copy!(box_, [content]),
            PropertyId::GridAutoColumns => copy!(box_, [grid.auto_columns]),
            PropertyId::GridAutoRows => copy!(box_, [grid.auto_rows]),
            PropertyId::GridAutoFlow => {
                copy!(box_, [grid.flow_column]);
                copy!(box_, [grid.flow_dense]);
            }
            PropertyId::GridColumnStart => copy!(box_, [grid.column_start]),
            PropertyId::GridColumnEnd => copy!(box_, [grid.column_end]),
            PropertyId::GridRowStart => copy!(box_, [grid.row_start]),
            PropertyId::GridRowEnd => copy!(box_, [grid.row_end]),
            PropertyId::JustifyItems => copy!(box_, [grid.justify_items]),
            PropertyId::JustifySelf => copy!(box_, [grid.justify_self]),
            PropertyId::TextDecorationLine => copy!(box_, [text_decoration_line]),
            PropertyId::TextDecorationColor => copy!(box_, [text_decoration_color]),
            PropertyId::TextDecorationStyle => copy!(box_, [text_decoration_style]),
            PropertyId::BorderTopWidth => copy!(border, [width[0]]),
            PropertyId::BorderRightWidth => copy!(border, [width[1]]),
            PropertyId::BorderBottomWidth => copy!(border, [width[2]]),
            PropertyId::BorderLeftWidth => copy!(border, [width[3]]),
            PropertyId::BorderTopStyle => copy!(border, [style[0]]),
            PropertyId::BorderRightStyle => copy!(border, [style[1]]),
            PropertyId::BorderBottomStyle => copy!(border, [style[2]]),
            PropertyId::BorderLeftStyle => copy!(border, [style[3]]),
            PropertyId::BorderTopColor => copy!(border, [color[0]]),
            PropertyId::BorderRightColor => copy!(border, [color[1]]),
            PropertyId::BorderBottomColor => copy!(border, [color[2]]),
            PropertyId::BorderLeftColor => copy!(border, [color[3]]),
            PropertyId::BorderTopLeftRadius => copy!(border, [radius[0]]),
            PropertyId::BorderTopRightRadius => copy!(border, [radius[1]]),
            PropertyId::BorderBottomRightRadius => copy!(border, [radius[2]]),
            PropertyId::BorderBottomLeftRadius => copy!(border, [radius[3]]),
            PropertyId::OutlineWidth => copy!(border, [outline_width]),
            PropertyId::OutlineStyle => copy!(border, [outline_style]),
            PropertyId::OutlineColor => copy!(border, [outline_color]),
            PropertyId::OutlineOffset => copy!(border, [outline_offset]),
            PropertyId::BackgroundColor => copy!(background, [color]),
            PropertyId::BackgroundImage => copy!(background, [images]),
            PropertyId::BackgroundRepeat => copy!(background, [repeat]),
            PropertyId::BackgroundPosition => copy!(background, [position]),
            PropertyId::BackgroundSize => copy!(background, [size]),
            PropertyId::MaskImage => copy!(background, [mask]),
            PropertyId::MaskSize => copy!(background, [mask_size]),
            PropertyId::MaskPosition => copy!(background, [mask_position]),
            PropertyId::MaskRepeat => copy!(background, [mask_repeat]),
            PropertyId::Color => copy!(inherited, [color]),
            PropertyId::FontFamily => copy!(inherited, [font_family]),
            PropertyId::FontSize => copy!(inherited, [font_size]),
            PropertyId::FontWeight => copy!(inherited, [font_weight]),
            PropertyId::FontStyle => copy!(inherited, [font_style]),
            PropertyId::LineHeight => copy!(inherited, [line_height]),
            PropertyId::TextAlign => copy!(inherited, [text_align]),
            PropertyId::TextIndent => copy!(inherited, [text_indent]),
            PropertyId::TextTransform => copy!(inherited, [text_transform]),
            PropertyId::WhiteSpace => copy!(inherited, [white_space]),
            PropertyId::LetterSpacing => copy!(inherited, [letter_spacing]),
            PropertyId::WordSpacing => copy!(inherited, [word_spacing]),
            PropertyId::Visibility => copy!(inherited, [visibility]),
            PropertyId::ListStyleType => copy!(inherited, [list_style_type]),
            PropertyId::ListStylePosition => copy!(inherited, [list_style_position]),
            PropertyId::Direction => copy!(inherited, [direction]),
            PropertyId::WordBreak => copy!(inherited, [word_break]),
            PropertyId::OverflowWrap => copy!(inherited, [overflow_wrap]),
            PropertyId::BorderCollapse => copy!(inherited, [border_collapse]),
            PropertyId::BorderSpacing => copy!(inherited, [border_spacing]),
            PropertyId::PointerEvents => copy!(inherited, [pointer_events]),
            _ => {}
        }
    }
}
