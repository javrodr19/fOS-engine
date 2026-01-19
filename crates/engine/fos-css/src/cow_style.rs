//! Copy-on-Write Styles
//!
//! Efficient inherited property handling using copy-on-write semantics.
//! Properties are only cloned when modified, saving memory for inherited styles.

use std::sync::Arc;
use std::collections::HashMap;

// ============================================================================
// Inherited Properties with CoW
// ============================================================================

/// Copy-on-write inherited properties container
#[derive(Debug, Clone)]
pub struct CowInheritedProps {
    /// Shared backing store
    inner: Arc<InheritedPropsInner>,
    /// Local overrides (lazy - only allocated when needed)
    overrides: Option<Box<PropertyOverrides>>,
}

/// Shared inherited properties data
#[derive(Debug, Clone)]
struct InheritedPropsInner {
    // Text properties
    pub font_family: Box<str>,
    pub font_size: f32,
    pub font_weight: u16,
    pub font_style: FontStyle,
    pub line_height: LineHeight,
    pub letter_spacing: f32,
    pub word_spacing: f32,
    pub text_align: TextAlign,
    pub text_indent: f32,
    pub text_transform: TextTransform,
    pub white_space: WhiteSpace,
    pub direction: Direction,
    pub writing_mode: WritingMode,
    
    // Color properties
    pub color: Color,
    pub text_decoration_color: Option<Color>,
    
    // List properties
    pub list_style_type: ListStyleType,
    pub list_style_position: ListStylePosition,
    
    // Visibility
    pub visibility: Visibility,
    
    // Cursor
    pub cursor: Cursor,
    
    // Quotes
    pub quotes: Quotes,
    
    // Tab size
    pub tab_size: u32,
    
    // Word break
    pub word_break: WordBreak,
    pub overflow_wrap: OverflowWrap,
    
    // Hyphens
    pub hyphens: Hyphens,
}

/// Local property overrides
#[derive(Debug, Clone, Default)]
struct PropertyOverrides {
    font_family: Option<Box<str>>,
    font_size: Option<f32>,
    font_weight: Option<u16>,
    font_style: Option<FontStyle>,
    line_height: Option<LineHeight>,
    letter_spacing: Option<f32>,
    word_spacing: Option<f32>,
    text_align: Option<TextAlign>,
    text_indent: Option<f32>,
    text_transform: Option<TextTransform>,
    white_space: Option<WhiteSpace>,
    direction: Option<Direction>,
    writing_mode: Option<WritingMode>,
    color: Option<Color>,
    text_decoration_color: Option<Option<Color>>,
    list_style_type: Option<ListStyleType>,
    list_style_position: Option<ListStylePosition>,
    visibility: Option<Visibility>,
    cursor: Option<Cursor>,
    quotes: Option<Quotes>,
    tab_size: Option<u32>,
    word_break: Option<WordBreak>,
    overflow_wrap: Option<OverflowWrap>,
    hyphens: Option<Hyphens>,
}

impl Default for InheritedPropsInner {
    fn default() -> Self {
        Self {
            font_family: "sans-serif".into(),
            font_size: 16.0,
            font_weight: 400,
            font_style: FontStyle::Normal,
            line_height: LineHeight::Normal,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            text_align: TextAlign::Start,
            text_indent: 0.0,
            text_transform: TextTransform::None,
            white_space: WhiteSpace::Normal,
            direction: Direction::Ltr,
            writing_mode: WritingMode::HorizontalTb,
            color: Color::black(),
            text_decoration_color: None,
            list_style_type: ListStyleType::Disc,
            list_style_position: ListStylePosition::Outside,
            visibility: Visibility::Visible,
            cursor: Cursor::Auto,
            quotes: Quotes::Auto,
            tab_size: 8,
            word_break: WordBreak::Normal,
            overflow_wrap: OverflowWrap::Normal,
            hyphens: Hyphens::Manual,
        }
    }
}

impl Default for CowInheritedProps {
    fn default() -> Self {
        Self::new()
    }
}

impl CowInheritedProps {
    /// Create with default inherited styles
    pub fn new() -> Self {
        Self {
            inner: Arc::new(InheritedPropsInner::default()),
            overrides: None,
        }
    }
    
    /// Create a child that inherits from this
    pub fn inherit(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            overrides: None,
        }
    }
    
    /// Flatten overrides into a new shared backing store
    /// Call this periodically to reduce memory overhead
    pub fn flatten(&mut self) {
        if self.overrides.is_none() {
            return;
        }
        
        let overrides = self.overrides.take().unwrap();
        let mut new_inner = (*self.inner).clone();
        
        if let Some(v) = overrides.font_family { new_inner.font_family = v; }
        if let Some(v) = overrides.font_size { new_inner.font_size = v; }
        if let Some(v) = overrides.font_weight { new_inner.font_weight = v; }
        if let Some(v) = overrides.font_style { new_inner.font_style = v; }
        if let Some(v) = overrides.line_height { new_inner.line_height = v; }
        if let Some(v) = overrides.letter_spacing { new_inner.letter_spacing = v; }
        if let Some(v) = overrides.word_spacing { new_inner.word_spacing = v; }
        if let Some(v) = overrides.text_align { new_inner.text_align = v; }
        if let Some(v) = overrides.text_indent { new_inner.text_indent = v; }
        if let Some(v) = overrides.text_transform { new_inner.text_transform = v; }
        if let Some(v) = overrides.white_space { new_inner.white_space = v; }
        if let Some(v) = overrides.direction { new_inner.direction = v; }
        if let Some(v) = overrides.writing_mode { new_inner.writing_mode = v; }
        if let Some(v) = overrides.color { new_inner.color = v; }
        if let Some(v) = overrides.text_decoration_color { new_inner.text_decoration_color = v; }
        if let Some(v) = overrides.list_style_type { new_inner.list_style_type = v; }
        if let Some(v) = overrides.list_style_position { new_inner.list_style_position = v; }
        if let Some(v) = overrides.visibility { new_inner.visibility = v; }
        if let Some(v) = overrides.cursor { new_inner.cursor = v; }
        if let Some(v) = overrides.quotes { new_inner.quotes = v; }
        if let Some(v) = overrides.tab_size { new_inner.tab_size = v; }
        if let Some(v) = overrides.word_break { new_inner.word_break = v; }
        if let Some(v) = overrides.overflow_wrap { new_inner.overflow_wrap = v; }
        if let Some(v) = overrides.hyphens { new_inner.hyphens = v; }
        
        self.inner = Arc::new(new_inner);
    }
    
    /// Check if this shares backing store with another
    pub fn shares_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
    
    /// Count of values in override layer
    pub fn override_count(&self) -> usize {
        self.overrides.as_ref().map_or(0, |o| {
            let mut count = 0;
            if o.font_family.is_some() { count += 1; }
            if o.font_size.is_some() { count += 1; }
            if o.font_weight.is_some() { count += 1; }
            if o.font_style.is_some() { count += 1; }
            if o.line_height.is_some() { count += 1; }
            if o.letter_spacing.is_some() { count += 1; }
            if o.word_spacing.is_some() { count += 1; }
            if o.text_align.is_some() { count += 1; }
            if o.text_indent.is_some() { count += 1; }
            if o.text_transform.is_some() { count += 1; }
            if o.white_space.is_some() { count += 1; }
            if o.direction.is_some() { count += 1; }
            if o.writing_mode.is_some() { count += 1; }
            if o.color.is_some() { count += 1; }
            if o.text_decoration_color.is_some() { count += 1; }
            if o.list_style_type.is_some() { count += 1; }
            if o.list_style_position.is_some() { count += 1; }
            if o.visibility.is_some() { count += 1; }
            if o.cursor.is_some() { count += 1; }
            if o.quotes.is_some() { count += 1; }
            if o.tab_size.is_some() { count += 1; }
            if o.word_break.is_some() { count += 1; }
            if o.overflow_wrap.is_some() { count += 1; }
            if o.hyphens.is_some() { count += 1; }
            count
        })
    }
    
    /// Ensure overrides exist
    fn ensure_overrides(&mut self) -> &mut PropertyOverrides {
        if self.overrides.is_none() {
            self.overrides = Some(Box::new(PropertyOverrides::default()));
        }
        self.overrides.as_mut().unwrap()
    }
    
    // ========================================================================
    // Getters
    // ========================================================================
    
    pub fn font_family(&self) -> &str {
        self.overrides.as_ref()
            .and_then(|o| o.font_family.as_ref())
            .map(|s| s.as_ref())
            .unwrap_or(&self.inner.font_family)
    }
    
    pub fn font_size(&self) -> f32 {
        self.overrides.as_ref()
            .and_then(|o| o.font_size)
            .unwrap_or(self.inner.font_size)
    }
    
    pub fn font_weight(&self) -> u16 {
        self.overrides.as_ref()
            .and_then(|o| o.font_weight)
            .unwrap_or(self.inner.font_weight)
    }
    
    pub fn font_style(&self) -> FontStyle {
        self.overrides.as_ref()
            .and_then(|o| o.font_style)
            .unwrap_or(self.inner.font_style)
    }
    
    pub fn line_height(&self) -> LineHeight {
        self.overrides.as_ref()
            .and_then(|o| o.line_height)
            .unwrap_or(self.inner.line_height)
    }
    
    pub fn letter_spacing(&self) -> f32 {
        self.overrides.as_ref()
            .and_then(|o| o.letter_spacing)
            .unwrap_or(self.inner.letter_spacing)
    }
    
    pub fn word_spacing(&self) -> f32 {
        self.overrides.as_ref()
            .and_then(|o| o.word_spacing)
            .unwrap_or(self.inner.word_spacing)
    }
    
    pub fn text_align(&self) -> TextAlign {
        self.overrides.as_ref()
            .and_then(|o| o.text_align)
            .unwrap_or(self.inner.text_align)
    }
    
    pub fn text_indent(&self) -> f32 {
        self.overrides.as_ref()
            .and_then(|o| o.text_indent)
            .unwrap_or(self.inner.text_indent)
    }
    
    pub fn text_transform(&self) -> TextTransform {
        self.overrides.as_ref()
            .and_then(|o| o.text_transform)
            .unwrap_or(self.inner.text_transform)
    }
    
    pub fn white_space(&self) -> WhiteSpace {
        self.overrides.as_ref()
            .and_then(|o| o.white_space)
            .unwrap_or(self.inner.white_space)
    }
    
    pub fn direction(&self) -> Direction {
        self.overrides.as_ref()
            .and_then(|o| o.direction)
            .unwrap_or(self.inner.direction)
    }
    
    pub fn writing_mode(&self) -> WritingMode {
        self.overrides.as_ref()
            .and_then(|o| o.writing_mode)
            .unwrap_or(self.inner.writing_mode)
    }
    
    pub fn color(&self) -> Color {
        self.overrides.as_ref()
            .and_then(|o| o.color)
            .unwrap_or(self.inner.color)
    }
    
    pub fn text_decoration_color(&self) -> Option<Color> {
        self.overrides.as_ref()
            .and_then(|o| o.text_decoration_color)
            .unwrap_or(self.inner.text_decoration_color)
    }
    
    pub fn list_style_type(&self) -> ListStyleType {
        self.overrides.as_ref()
            .and_then(|o| o.list_style_type)
            .unwrap_or(self.inner.list_style_type)
    }
    
    pub fn list_style_position(&self) -> ListStylePosition {
        self.overrides.as_ref()
            .and_then(|o| o.list_style_position)
            .unwrap_or(self.inner.list_style_position)
    }
    
    pub fn visibility(&self) -> Visibility {
        self.overrides.as_ref()
            .and_then(|o| o.visibility)
            .unwrap_or(self.inner.visibility)
    }
    
    pub fn cursor(&self) -> Cursor {
        self.overrides.as_ref()
            .and_then(|o| o.cursor)
            .unwrap_or(self.inner.cursor)
    }
    
    pub fn quotes(&self) -> Quotes {
        self.overrides.as_ref()
            .and_then(|o| o.quotes)
            .unwrap_or(self.inner.quotes)
    }
    
    pub fn tab_size(&self) -> u32 {
        self.overrides.as_ref()
            .and_then(|o| o.tab_size)
            .unwrap_or(self.inner.tab_size)
    }
    
    pub fn word_break(&self) -> WordBreak {
        self.overrides.as_ref()
            .and_then(|o| o.word_break)
            .unwrap_or(self.inner.word_break)
    }
    
    pub fn overflow_wrap(&self) -> OverflowWrap {
        self.overrides.as_ref()
            .and_then(|o| o.overflow_wrap)
            .unwrap_or(self.inner.overflow_wrap)
    }
    
    pub fn hyphens(&self) -> Hyphens {
        self.overrides.as_ref()
            .and_then(|o| o.hyphens)
            .unwrap_or(self.inner.hyphens)
    }
    
    // ========================================================================
    // Setters (copy-on-write)
    // ========================================================================
    
    pub fn set_font_family(&mut self, value: &str) {
        self.ensure_overrides().font_family = Some(value.into());
    }
    
    pub fn set_font_size(&mut self, value: f32) {
        self.ensure_overrides().font_size = Some(value);
    }
    
    pub fn set_font_weight(&mut self, value: u16) {
        self.ensure_overrides().font_weight = Some(value);
    }
    
    pub fn set_font_style(&mut self, value: FontStyle) {
        self.ensure_overrides().font_style = Some(value);
    }
    
    pub fn set_line_height(&mut self, value: LineHeight) {
        self.ensure_overrides().line_height = Some(value);
    }
    
    pub fn set_letter_spacing(&mut self, value: f32) {
        self.ensure_overrides().letter_spacing = Some(value);
    }
    
    pub fn set_word_spacing(&mut self, value: f32) {
        self.ensure_overrides().word_spacing = Some(value);
    }
    
    pub fn set_text_align(&mut self, value: TextAlign) {
        self.ensure_overrides().text_align = Some(value);
    }
    
    pub fn set_text_indent(&mut self, value: f32) {
        self.ensure_overrides().text_indent = Some(value);
    }
    
    pub fn set_text_transform(&mut self, value: TextTransform) {
        self.ensure_overrides().text_transform = Some(value);
    }
    
    pub fn set_white_space(&mut self, value: WhiteSpace) {
        self.ensure_overrides().white_space = Some(value);
    }
    
    pub fn set_direction(&mut self, value: Direction) {
        self.ensure_overrides().direction = Some(value);
    }
    
    pub fn set_writing_mode(&mut self, value: WritingMode) {
        self.ensure_overrides().writing_mode = Some(value);
    }
    
    pub fn set_color(&mut self, value: Color) {
        self.ensure_overrides().color = Some(value);
    }
    
    pub fn set_text_decoration_color(&mut self, value: Option<Color>) {
        self.ensure_overrides().text_decoration_color = Some(value);
    }
    
    pub fn set_list_style_type(&mut self, value: ListStyleType) {
        self.ensure_overrides().list_style_type = Some(value);
    }
    
    pub fn set_list_style_position(&mut self, value: ListStylePosition) {
        self.ensure_overrides().list_style_position = Some(value);
    }
    
    pub fn set_visibility(&mut self, value: Visibility) {
        self.ensure_overrides().visibility = Some(value);
    }
    
    pub fn set_cursor(&mut self, value: Cursor) {
        self.ensure_overrides().cursor = Some(value);
    }
    
    pub fn set_quotes(&mut self, value: Quotes) {
        self.ensure_overrides().quotes = Some(value);
    }
    
    pub fn set_tab_size(&mut self, value: u32) {
        self.ensure_overrides().tab_size = Some(value);
    }
    
    pub fn set_word_break(&mut self, value: WordBreak) {
        self.ensure_overrides().word_break = Some(value);
    }
    
    pub fn set_overflow_wrap(&mut self, value: OverflowWrap) {
        self.ensure_overrides().overflow_wrap = Some(value);
    }
    
    pub fn set_hyphens(&mut self, value: Hyphens) {
        self.ensure_overrides().hyphens = Some(value);
    }
}

// ============================================================================
// Supporting Types
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontStyle {
    #[default]
    Normal,
    Italic,
    Oblique,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum LineHeight {
    #[default]
    Normal,
    Number(f32),
    Length(f32),
    Percentage(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
    Justify,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextTransform {
    #[default]
    None,
    Capitalize,
    Uppercase,
    Lowercase,
    FullWidth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    Nowrap,
    Pre,
    PreWrap,
    PreLine,
    BreakSpaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WritingMode {
    #[default]
    HorizontalTb,
    VerticalRl,
    VerticalLr,
    SidewaysRl,
    SidewaysLr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    
    pub fn black() -> Self {
        Self::new(0, 0, 0, 255)
    }
    
    pub fn white() -> Self {
        Self::new(255, 255, 255, 255)
    }
    
    pub fn transparent() -> Self {
        Self::new(0, 0, 0, 0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListStyleType {
    #[default]
    Disc,
    Circle,
    Square,
    Decimal,
    DecimalLeadingZero,
    LowerRoman,
    UpperRoman,
    LowerGreek,
    LowerLatin,
    UpperLatin,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListStylePosition {
    #[default]
    Outside,
    Inside,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
    Collapse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cursor {
    #[default]
    Auto,
    Default,
    None,
    ContextMenu,
    Help,
    Pointer,
    Progress,
    Wait,
    Cell,
    Crosshair,
    Text,
    VerticalText,
    Alias,
    Copy,
    Move,
    NoDrop,
    NotAllowed,
    Grab,
    Grabbing,
    ColResize,
    RowResize,
    NResize,
    EResize,
    SResize,
    WResize,
    NeResize,
    NwResize,
    SeResize,
    SwResize,
    EwResize,
    NsResize,
    NeswResize,
    NwseResize,
    ZoomIn,
    ZoomOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quotes {
    #[default]
    Auto,
    None,
    // For custom quotes, use a separate storage
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordBreak {
    #[default]
    Normal,
    BreakAll,
    KeepAll,
    BreakWord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OverflowWrap {
    #[default]
    Normal,
    BreakWord,
    Anywhere,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Hyphens {
    None,
    #[default]
    Manual,
    Auto,
}

// ============================================================================
// Flat Custom Properties
// ============================================================================

/// Flat array storage for custom properties
#[derive(Debug, Clone)]
pub struct FlatCustomProperties {
    /// Interned names
    names: HashMap<Box<str>, usize>,
    /// Values indexed by name position
    values: Vec<Option<Box<str>>>,
    /// Reverse lookup
    reverse: Vec<Box<str>>,
}

impl Default for FlatCustomProperties {
    fn default() -> Self {
        Self::new()
    }
}

impl FlatCustomProperties {
    pub fn new() -> Self {
        Self {
            names: HashMap::new(),
            values: Vec::new(),
            reverse: Vec::new(),
        }
    }
    
    /// Get or create an index for a property name
    pub fn intern(&mut self, name: &str) -> usize {
        if let Some(&idx) = self.names.get(name) {
            return idx;
        }
        
        let idx = self.names.len();
        self.names.insert(name.into(), idx);
        self.values.push(None);
        self.reverse.push(name.into());
        idx
    }
    
    /// Get a property value by name
    pub fn get(&self, name: &str) -> Option<&str> {
        self.names.get(name)
            .and_then(|&idx| self.values.get(idx))
            .and_then(|v| v.as_ref())
            .map(|s| s.as_ref())
    }
    
    /// Get a property value by index
    pub fn get_by_index(&self, idx: usize) -> Option<&str> {
        self.values.get(idx)
            .and_then(|v| v.as_ref())
            .map(|s| s.as_ref())
    }
    
    /// Set a property value
    pub fn set(&mut self, name: &str, value: &str) {
        let idx = self.intern(name);
        if idx < self.values.len() {
            self.values[idx] = Some(value.into());
        }
    }
    
    /// Set a property value by index
    pub fn set_by_index(&mut self, idx: usize, value: &str) {
        if idx < self.values.len() {
            self.values[idx] = Some(value.into());
        }
    }
    
    /// Remove a property
    pub fn remove(&mut self, name: &str) {
        if let Some(&idx) = self.names.get(name) {
            if idx < self.values.len() {
                self.values[idx] = None;
            }
        }
    }
    
    /// Number of defined properties
    pub fn len(&self) -> usize {
        self.values.iter().filter(|v| v.is_some()).count()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.values.iter().all(|v| v.is_none())
    }
    
    /// Iterate over defined properties
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.iter()
            .enumerate()
            .filter_map(move |(idx, v)| {
                v.as_ref().map(|value| {
                    (self.reverse[idx].as_ref(), value.as_ref())
                })
            })
    }
    
    /// Clone non-empty properties into a new map
    pub fn to_hashmap(&self) -> HashMap<Box<str>, Box<str>> {
        self.iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cow_inherit() {
        let parent = CowInheritedProps::new();
        let child = parent.inherit();
        
        assert!(child.shares_with(&parent));
        assert_eq!(child.font_size(), 16.0);
    }
    
    #[test]
    fn test_cow_override() {
        let parent = CowInheritedProps::new();
        let mut child = parent.inherit();
        
        child.set_font_size(24.0);
        
        assert!(child.shares_with(&parent)); // Still shares backing
        assert_eq!(child.font_size(), 24.0); // But returns override
        assert_eq!(parent.font_size(), 16.0); // Parent unchanged
    }
    
    #[test]
    fn test_cow_flatten() {
        let parent = CowInheritedProps::new();
        let mut child = parent.inherit();
        
        child.set_font_size(24.0);
        child.set_color(Color::white());
        
        assert_eq!(child.override_count(), 2);
        
        child.flatten();
        
        assert!(!child.shares_with(&parent));
        assert_eq!(child.override_count(), 0);
        assert_eq!(child.font_size(), 24.0);
    }
    
    #[test]
    fn test_flat_custom_properties() {
        let mut props = FlatCustomProperties::new();
        
        props.set("--primary-color", "#ff0000");
        props.set("--font-size", "16px");
        
        assert_eq!(props.get("--primary-color"), Some("#ff0000"));
        assert_eq!(props.get("--font-size"), Some("16px"));
        assert_eq!(props.get("--unknown"), None);
        assert_eq!(props.len(), 2);
    }
    
    #[test]
    fn test_flat_custom_properties_index() {
        let mut props = FlatCustomProperties::new();
        
        let idx = props.intern("--color");
        props.set_by_index(idx, "red");
        
        assert_eq!(props.get_by_index(idx), Some("red"));
        assert_eq!(props.get("--color"), Some("red"));
    }
}
