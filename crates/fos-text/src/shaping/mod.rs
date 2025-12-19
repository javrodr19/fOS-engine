//! Text shaping module

mod shaper;
mod run;
pub mod cache;

pub use shaper::TextShaper;
pub use run::{ShapedGlyph, ShapedRun};
pub use cache::{TextRunCache, TextRunKey, CachedTextRun, TextRunCacheStats};
