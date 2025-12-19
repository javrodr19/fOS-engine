//! Text shaper using rustybuzz

use std::str::FromStr;
use rustybuzz::{Face, UnicodeBuffer, shape};
use crate::font::{FontDatabase, FontId};
use crate::{Result, TextError};
use super::{ShapedGlyph, ShapedRun};

/// Text shaper using HarfBuzz (via rustybuzz)
pub struct TextShaper {
    /// Direction for shaping  
    direction: Direction,
    /// Script for shaping
    script: Option<rustybuzz::Script>,
    /// Language for shaping
    language: Option<rustybuzz::Language>,
}

/// Text direction
#[derive(Debug, Clone, Copy, Default)]
pub enum Direction {
    #[default]
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

impl From<Direction> for rustybuzz::Direction {
    fn from(d: Direction) -> Self {
        match d {
            Direction::LeftToRight => rustybuzz::Direction::LeftToRight,
            Direction::RightToLeft => rustybuzz::Direction::RightToLeft,
            Direction::TopToBottom => rustybuzz::Direction::TopToBottom,
            Direction::BottomToTop => rustybuzz::Direction::BottomToTop,
        }
    }
}

impl TextShaper {
    /// Create a new text shaper
    pub fn new() -> Self {
        Self {
            direction: Direction::LeftToRight,
            script: None,
            language: None,
        }
    }
    
    /// Set text direction
    pub fn direction(mut self, direction: Direction) -> Self {
        self.direction = direction;
        self
    }
    
    /// Set script (for automatic feature selection)
    pub fn script(mut self, script: rustybuzz::Script) -> Self {
        self.script = Some(script);
        self
    }
    
    /// Set language (for automatic feature selection)
    pub fn language(mut self, language: &str) -> Self {
        self.language = rustybuzz::Language::from_str(language).ok();
        self
    }
    
    /// Shape text using a font from the database
    pub fn shape(
        &self,
        db: &FontDatabase,
        font_id: FontId,
        text: &str,
        font_size: f32,
    ) -> Result<ShapedRun> {
        db.with_face_data(font_id, |data, index| {
            self.shape_with_data(data, index, text, font_size)
        }).ok_or_else(|| TextError::FontNotFound("Font not found in database".into()))?
    }
    
    /// Shape text with raw font data
    pub fn shape_with_data(
        &self,
        font_data: &[u8],
        face_index: u32,
        text: &str,
        font_size: f32,
    ) -> Result<ShapedRun> {
        // Parse font using rustybuzz
        let face = Face::from_slice(font_data, face_index)
            .ok_or_else(|| TextError::FontParsing("Failed to parse font".into()))?;
        
        // Create unicode buffer with text
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.set_direction(self.direction.into());
        
        if let Some(script) = self.script {
            buffer.set_script(script);
        }
        
        if let Some(ref lang) = self.language {
            buffer.set_language(lang.clone());
        }
        
        // Shape!
        let output = shape(&face, &[], buffer);
        
        // Extract glyphs
        let positions = output.glyph_positions();
        let infos = output.glyph_infos();
        
        let glyphs: Vec<ShapedGlyph> = infos.iter().zip(positions.iter())
            .map(|(info, pos)| ShapedGlyph {
                glyph_id: info.glyph_id as u16,
                x_offset: pos.x_offset,
                y_offset: pos.y_offset,
                x_advance: pos.x_advance,
                y_advance: pos.y_advance,
                cluster: info.cluster,
            })
            .collect();
        
        Ok(ShapedRun::new(glyphs, font_size, face.units_per_em() as u16))
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_shaper_creation() {
        let shaper = TextShaper::new()
            .direction(Direction::LeftToRight);
        assert!(matches!(shaper.direction, Direction::LeftToRight));
    }
}
