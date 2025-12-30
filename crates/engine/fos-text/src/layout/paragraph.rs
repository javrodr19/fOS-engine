//! Paragraph layout

use super::{TextAlign, TextLine, TextLayout, LineBreaker};
use crate::shaping::TextShaper;
use crate::font::{FontDatabase, FontId};
use crate::Result;

/// Paragraph layout configuration
#[derive(Debug, Clone)]
pub struct ParagraphStyle {
    /// Maximum width for line wrapping
    pub max_width: f32,
    /// Line height multiplier
    pub line_height: f32,
    /// Text alignment
    pub align: TextAlign,
    /// Font size
    pub font_size: f32,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        Self {
            max_width: f32::INFINITY,
            line_height: 1.2,
            align: TextAlign::Left,
            font_size: 16.0,
        }
    }
}

/// Paragraph layout engine
pub struct ParagraphLayout {
    style: ParagraphStyle,
}

impl ParagraphLayout {
    /// Create a new paragraph layout with default style
    pub fn new() -> Self {
        Self {
            style: ParagraphStyle::default(),
        }
    }
    
    /// Create with specific style
    pub fn with_style(style: ParagraphStyle) -> Self {
        Self { style }
    }
    
    /// Set max width
    pub fn max_width(mut self, width: f32) -> Self {
        self.style.max_width = width;
        self
    }
    
    /// Set text alignment
    pub fn align(mut self, align: TextAlign) -> Self {
        self.style.align = align;
        self
    }
    
    /// Set font size
    pub fn font_size(mut self, size: f32) -> Self {
        self.style.font_size = size;
        self
    }
    
    /// Layout text
    pub fn layout(
        &self,
        text: &str,
        db: &FontDatabase,
        font_id: FontId,
        shaper: &mut TextShaper,
    ) -> Result<TextLayout> {
        if text.is_empty() {
            return Ok(TextLayout::empty());
        }
        
        // Get line height from font metrics
        let line_height = db.with_face_data(font_id, |data, index| {
            crate::font::FontParser::parse_index(data, index)
                .map(|parser| {
                    let upem = parser.units_per_em() as f32;
                    let ascender = parser.ascender() as f32;
                    let descender = parser.descender() as f32;
                    let gap = parser.line_gap() as f32;
                    (ascender - descender + gap) * self.style.font_size / upem * self.style.line_height
                })
                .unwrap_or(self.style.font_size * self.style.line_height)
        }).unwrap_or(self.style.font_size * self.style.line_height);
        
        // Measure function for line breaking
        let mut measure = |s: &str| -> f32 {
            shaper.shape(db, font_id, s, self.style.font_size)
                .map(|run| run.width())
                .unwrap_or(0.0)
        };
        
        // Break into lines
        let line_ranges = LineBreaker::break_lines(text, self.style.max_width, &mut measure);
        
        // Build layout
        let mut lines = Vec::new();
        let mut max_width = 0.0f32;
        
        for (start, end) in line_ranges {
            let line_text = &text[start..end].trim_end();
            let width = shaper.shape(db, font_id, line_text, self.style.font_size)
                .map(|run| run.width())
                .unwrap_or(0.0);
            max_width = max_width.max(width);
            
            let x_offset = match self.style.align {
                TextAlign::Left => 0.0,
                TextAlign::Right => self.style.max_width - width,
                TextAlign::Center => (self.style.max_width - width) / 2.0,
                TextAlign::Justify => 0.0, // TODO: Justify needs special handling
            };
            
            lines.push(TextLine {
                start,
                end,
                width,
                x_offset: if self.style.max_width.is_finite() { x_offset } else { 0.0 },
            });
        }
        
        let height = lines.len() as f32 * line_height;
        
        Ok(TextLayout {
            lines,
            width: if self.style.max_width.is_finite() { self.style.max_width } else { max_width },
            height,
            line_height,
        })
    }
}

impl Default for ParagraphLayout {
    fn default() -> Self {
        Self::new()
    }
}
