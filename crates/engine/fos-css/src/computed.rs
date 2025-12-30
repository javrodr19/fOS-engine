//! Computed Styles
//!
//! The final computed style values for an element after cascade.
//! Uses compact representation for memory efficiency.

use crate::properties::{PropertyId, PropertyValue, Keyword, Length, LengthUnit, Color};
use crate::Declaration;

/// Computed style for an element
/// 
/// Contains the final resolved values after cascade.
/// Uses Option<T> for properties - None means 'inherit' or 'initial' depending on property.
#[derive(Debug, Default, Clone)]
pub struct ComputedStyle {
    // Display & Layout
    pub display: Display,
    pub position: Position,
    
    // Box Model
    pub width: SizeValue,
    pub height: SizeValue,
    pub min_width: SizeValue,
    pub min_height: SizeValue,
    pub max_width: SizeValue,
    pub max_height: SizeValue,
    
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,
    pub border_width: EdgeSizes,
    
    // Colors
    pub color: Color,
    pub background_color: Color,
    
    // Text
    pub font_size: f32,        // in pixels
    pub font_weight: u16,      // 100-900
    pub line_height: f32,      // multiplier
    
    // Flex
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    
    // Visibility
    pub visibility: Visibility,
    pub opacity: f32,
    pub overflow: Overflow,
    pub z_index: Option<i32>,
    
    // Positioning
    pub top: SizeValue,
    pub right: SizeValue,
    pub bottom: SizeValue,
    pub left: SizeValue,
    
    // Property presence bitmask (tracks which properties were explicitly set)
    pub property_mask: PropertyMask,
}

/// Bitmask for tracking which properties are explicitly set
/// 
/// This allows skipping inherit/default logic for unset properties.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PropertyMask(pub u64);

impl PropertyMask {
    // Property bit positions
    pub const DISPLAY: u64 = 1 << 0;
    pub const POSITION: u64 = 1 << 1;
    pub const WIDTH: u64 = 1 << 2;
    pub const HEIGHT: u64 = 1 << 3;
    pub const MARGIN: u64 = 1 << 4;
    pub const PADDING: u64 = 1 << 5;
    pub const COLOR: u64 = 1 << 6;
    pub const BACKGROUND: u64 = 1 << 7;
    pub const FONT_SIZE: u64 = 1 << 8;
    pub const FONT_WEIGHT: u64 = 1 << 9;
    pub const LINE_HEIGHT: u64 = 1 << 10;
    pub const FLEX_DIRECTION: u64 = 1 << 11;
    pub const JUSTIFY_CONTENT: u64 = 1 << 12;
    pub const ALIGN_ITEMS: u64 = 1 << 13;
    pub const OPACITY: u64 = 1 << 14;
    pub const OVERFLOW: u64 = 1 << 15;
    pub const Z_INDEX: u64 = 1 << 16;
    pub const TOP: u64 = 1 << 17;
    pub const RIGHT: u64 = 1 << 18;
    pub const BOTTOM: u64 = 1 << 19;
    pub const LEFT: u64 = 1 << 20;
    pub const VISIBILITY: u64 = 1 << 21;
    pub const MIN_WIDTH: u64 = 1 << 22;
    pub const MIN_HEIGHT: u64 = 1 << 23;
    pub const MAX_WIDTH: u64 = 1 << 24;
    pub const MAX_HEIGHT: u64 = 1 << 25;
    pub const BORDER_WIDTH: u64 = 1 << 26;
    pub const FLEX_WRAP: u64 = 1 << 27;
    
    /// Create empty mask
    pub fn new() -> Self {
        Self(0)
    }
    
    /// Set a property bit
    pub fn set(&mut self, bit: u64) {
        self.0 |= bit;
    }
    
    /// Check if a property is set
    pub fn is_set(&self, bit: u64) -> bool {
        (self.0 & bit) != 0
    }
    
    /// Clear a property bit
    pub fn clear(&mut self, bit: u64) {
        self.0 &= !bit;
    }
    
    /// Count set properties
    pub fn count(&self) -> u32 {
        self.0.count_ones()
    }
    
    /// Check if any property is set
    pub fn any(&self) -> bool {
        self.0 != 0
    }
    
    /// Merge with another mask
    pub fn merge(&mut self, other: PropertyMask) {
        self.0 |= other.0;
    }
}

impl ComputedStyle {
    /// Apply a declaration to this computed style
    pub fn apply_declaration(&mut self, decl: &Declaration) {
        match decl.property {
            PropertyId::Display => {
                if let PropertyValue::Keyword(kw) = &decl.value {
                    self.display = match kw {
                        Keyword::None => Display::None,
                        Keyword::Block => Display::Block,
                        Keyword::Inline => Display::Inline,
                        Keyword::InlineBlock => Display::InlineBlock,
                        Keyword::Flex => Display::Flex,
                        Keyword::Grid => Display::Grid,
                        Keyword::Contents => Display::Contents,
                        _ => return,
                    };
                }
            }
            PropertyId::Position => {
                if let PropertyValue::Keyword(kw) = &decl.value {
                    self.position = match kw {
                        Keyword::Static => Position::Static,
                        Keyword::Relative => Position::Relative,
                        Keyword::Absolute => Position::Absolute,
                        Keyword::Fixed => Position::Fixed,
                        Keyword::Sticky => Position::Sticky,
                        _ => return,
                    };
                }
            }
            PropertyId::Width => {
                self.width = Self::value_to_size(&decl.value);
            }
            PropertyId::Height => {
                self.height = Self::value_to_size(&decl.value);
            }
            PropertyId::Color => {
                if let PropertyValue::Color(c) = &decl.value {
                    self.color = *c;
                }
            }
            PropertyId::BackgroundColor => {
                if let PropertyValue::Color(c) = &decl.value {
                    self.background_color = *c;
                }
            }
            PropertyId::FontSize => {
                if let PropertyValue::Length(len) = &decl.value {
                    self.font_size = Self::length_to_px(len, self.font_size);
                }
            }
            PropertyId::Opacity => {
                if let PropertyValue::Number(n) = &decl.value {
                    self.opacity = n.clamp(0.0, 1.0);
                }
            }
            PropertyId::FlexDirection => {
                if let PropertyValue::Keyword(kw) = &decl.value {
                    self.flex_direction = match kw {
                        Keyword::Row => FlexDirection::Row,
                        Keyword::RowReverse => FlexDirection::RowReverse,
                        Keyword::Column => FlexDirection::Column,
                        Keyword::ColumnReverse => FlexDirection::ColumnReverse,
                        _ => return,
                    };
                }
            }
            PropertyId::JustifyContent => {
                if let PropertyValue::Keyword(kw) = &decl.value {
                    self.justify_content = match kw {
                        Keyword::FlexStart => JustifyContent::FlexStart,
                        Keyword::FlexEnd => JustifyContent::FlexEnd,
                        Keyword::Center => JustifyContent::Center,
                        Keyword::SpaceBetween => JustifyContent::SpaceBetween,
                        Keyword::SpaceAround => JustifyContent::SpaceAround,
                        Keyword::SpaceEvenly => JustifyContent::SpaceEvenly,
                        _ => return,
                    };
                }
            }
            PropertyId::AlignItems => {
                if let PropertyValue::Keyword(kw) = &decl.value {
                    self.align_items = match kw {
                        Keyword::FlexStart => AlignItems::FlexStart,
                        Keyword::FlexEnd => AlignItems::FlexEnd,
                        Keyword::Center => AlignItems::Center,
                        Keyword::Stretch => AlignItems::Stretch,
                        Keyword::Baseline => AlignItems::Baseline,
                        _ => return,
                    };
                }
            }
            // Handle shorthand properties
            PropertyId::Margin => {
                self.margin = Self::value_to_edges(&decl.value);
            }
            PropertyId::Padding => {
                self.padding = Self::value_to_edges(&decl.value);
            }
            // Individual margin/padding sides
            PropertyId::MarginTop => {
                self.margin.top = Self::value_to_size(&decl.value);
            }
            PropertyId::MarginRight => {
                self.margin.right = Self::value_to_size(&decl.value);
            }
            PropertyId::MarginBottom => {
                self.margin.bottom = Self::value_to_size(&decl.value);
            }
            PropertyId::MarginLeft => {
                self.margin.left = Self::value_to_size(&decl.value);
            }
            PropertyId::PaddingTop => {
                self.padding.top = Self::value_to_size(&decl.value);
            }
            PropertyId::PaddingRight => {
                self.padding.right = Self::value_to_size(&decl.value);
            }
            PropertyId::PaddingBottom => {
                self.padding.bottom = Self::value_to_size(&decl.value);
            }
            PropertyId::PaddingLeft => {
                self.padding.left = Self::value_to_size(&decl.value);
            }
            _ => {
                // Other properties not yet handled
            }
        }
    }
    
    fn value_to_size(value: &PropertyValue) -> SizeValue {
        match value {
            PropertyValue::Keyword(Keyword::Auto) => SizeValue::Auto,
            PropertyValue::Length(len) => SizeValue::Length(len.value, len.unit),
            PropertyValue::Number(n) if *n == 0.0 => SizeValue::Length(0.0, LengthUnit::Px),
            _ => SizeValue::Auto,
        }
    }
    
    fn value_to_edges(value: &PropertyValue) -> EdgeSizes {
        match value {
            PropertyValue::Length(len) => {
                let size = SizeValue::Length(len.value, len.unit);
                EdgeSizes {
                    top: size.clone(),
                    right: size.clone(),
                    bottom: size.clone(),
                    left: size,
                }
            }
            PropertyValue::List(values) => {
                let sizes: Vec<_> = values.iter().map(Self::value_to_size).collect();
                match sizes.len() {
                    1 => EdgeSizes {
                        top: sizes[0].clone(),
                        right: sizes[0].clone(),
                        bottom: sizes[0].clone(),
                        left: sizes[0].clone(),
                    },
                    2 => EdgeSizes {
                        top: sizes[0].clone(),
                        right: sizes[1].clone(),
                        bottom: sizes[0].clone(),
                        left: sizes[1].clone(),
                    },
                    3 => EdgeSizes {
                        top: sizes[0].clone(),
                        right: sizes[1].clone(),
                        bottom: sizes[2].clone(),
                        left: sizes[1].clone(),
                    },
                    4 => EdgeSizes {
                        top: sizes[0].clone(),
                        right: sizes[1].clone(),
                        bottom: sizes[2].clone(),
                        left: sizes[3].clone(),
                    },
                    _ => EdgeSizes::default(),
                }
            }
            _ => EdgeSizes::default(),
        }
    }
    
    fn length_to_px(len: &Length, parent_font_size: f32) -> f32 {
        match len.unit {
            LengthUnit::Px => len.value,
            LengthUnit::Em => len.value * parent_font_size,
            LengthUnit::Rem => len.value * 16.0, // Assuming 16px root font size
            LengthUnit::Percent => len.value * parent_font_size / 100.0,
            _ => len.value, // Approximate for other units
        }
    }
}

/// Display property values
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Display {
    #[default]
    Block,
    Inline,
    InlineBlock,
    Flex,
    Grid,
    None,
    Contents,
}

/// Position property values
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Position {
    #[default]
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// Size value (can be length, percentage, or auto)
#[derive(Debug, Clone)]
pub enum SizeValue {
    Auto,
    Length(f32, LengthUnit),
}

impl Default for SizeValue {
    fn default() -> Self {
        Self::Auto
    }
}

/// Edge sizes for margin, padding, border
#[derive(Debug, Clone, Default)]
pub struct EdgeSizes {
    pub top: SizeValue,
    pub right: SizeValue,
    pub bottom: SizeValue,
    pub left: SizeValue,
}

/// Flex direction
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlexDirection {
    #[default]
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

/// Flex wrap
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FlexWrap {
    #[default]
    Nowrap,
    Wrap,
    WrapReverse,
}

/// Justify content
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Align items
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AlignItems {
    #[default]
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

/// Visibility
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

/// Overflow
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
    Auto,
    Clip,
}
