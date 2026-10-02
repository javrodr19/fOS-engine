//! Glyph rendering module

mod rasterizer;
mod atlas;
pub mod decorations;
pub mod hinting;

pub use rasterizer::{GlyphRasterizer, RasterizedGlyph};
pub use atlas::{GlyphAtlas, GlyphKey, SYNTH_BOLD, SYNTH_OBLIQUE};
pub use decorations::{TextDecoration, TextDecorationLine, TextDecorationStyle, DecorationGeometry, calculate_decorations};
pub use hinting::{SubpixelMode, HintingMode, AntialiasMode, TextRenderSettings, SubpixelFilter, SubpixelBitmap};
