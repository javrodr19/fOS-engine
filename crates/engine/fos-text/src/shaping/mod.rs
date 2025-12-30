//! Text shaping module
//!
//! Full HarfBuzz-compatible text shaping with:
//! - GSUB/GPOS OpenType table processing
//! - UAX #9 Bidi algorithm
//! - Script itemization
//! - Arabic and Indic complex script shaping

mod shaper;
mod run;
pub mod cache;
pub mod gsub;
pub mod gpos;
pub mod bidi;
pub mod script;
pub mod arabic;
pub mod indic;
pub mod memory;

pub use shaper::{TextShaper, TextDirection, ShaperConfig, Feature};
pub use run::{ShapedGlyph, ShapedRun, PositionedGlyph};
pub use cache::{TextRunCache, TextRunKey, CachedTextRun, TextRunCacheStats};
pub use gsub::{GsubTable, Coverage, ClassDef, Substitution};
pub use gpos::{GposTable, ValueRecord, Anchor};
pub use bidi::{BidiParagraph, BidiClass, BidiRun, Level};
pub use script::{Script, ScriptItemizer, ScriptRun, Direction, Language};
pub use arabic::{ArabicShaper, JoiningType, PositionalForm};
pub use indic::{IndicShaper, IndicCategory, Syllable};
pub use memory::{BumpAllocator, StringInterner, InternedString};
