//! fOS Text - Text Rendering Engine
//!
//! This crate provides text rendering for the fOS browser engine:
//! - Font loading and matching (fontdb)
//! - Text shaping (rustybuzz - HarfBuzz port)
//! - Text layout (line breaking, word wrap)
//! - Glyph rasterization and caching
//! - Pre-rendered glyph atlas for ASCII
//! - Ruby annotations for CJK text

pub mod font;
pub mod shaping;
pub mod layout;
pub mod render;
pub mod glyph_atlas;
pub mod ruby;

pub use font::{FontDatabase, FontFace, FontId, FontStyle, FontWeight, FontQuery};
pub use shaping::{TextShaper, ShapedGlyph, ShapedRun};
pub use layout::{TextLayout, LineBreaker, ParagraphLayout};
pub use render::{GlyphRasterizer, GlyphAtlas, GlyphKey, RasterizedGlyph};
pub use glyph_atlas::{GlyphAtlasCache, GlyphInfo};
pub use ruby::{RubyAnnotation, RubyContainer, RubyStyle};

/// Text rendering error types
#[derive(Debug, thiserror::Error)]
pub enum TextError {
    #[error("Font not found: {0}")]
    FontNotFound(String),
    
    #[error("Failed to parse font: {0}")]
    FontParsing(String),
    
    #[error("Shaping failed: {0}")]
    ShapingFailed(String),
    
    #[error("Rasterization failed: {0}")]
    RasterizationFailed(String),
}

pub type Result<T> = std::result::Result<T, TextError>;
