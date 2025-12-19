//! Text layout module

mod line;
mod paragraph;

pub use line::LineBreaker;
pub use paragraph::ParagraphLayout;

/// Text alignment
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Left,
    Right,
    Center,
    Justify,
}

/// A laid out line of text
#[derive(Debug, Clone)]
pub struct TextLine {
    /// Start index in original text
    pub start: usize,
    /// End index in original text  
    pub end: usize,
    /// Width of the line in pixels
    pub width: f32,
    /// X offset for alignment
    pub x_offset: f32,
}

/// Complete text layout result
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// Lines of text
    pub lines: Vec<TextLine>,
    /// Total width
    pub width: f32,
    /// Total height
    pub height: f32,
    /// Line height used
    pub line_height: f32,
}

impl TextLayout {
    /// Create empty layout
    pub fn empty() -> Self {
        Self {
            lines: Vec::new(),
            width: 0.0,
            height: 0.0,
            line_height: 0.0,
        }
    }
    
    /// Number of lines
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
}
