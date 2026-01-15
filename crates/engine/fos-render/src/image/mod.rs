//! Image handling module
//!
//! Provides image decoding, caching, pooling, and rendering for the browser engine.

mod decoder;
mod cache;
mod renderer;
pub mod decoders;
pub mod pool;
pub mod queue;
pub mod avif;
pub mod svg;
pub mod ico;
pub mod sprites;
pub mod srcset;
pub mod progressive;
pub mod texture_atlas_cache;

pub use decoder::{ImageDecoder, DecodedImage, ImageFormat};
pub use cache::{ImageCache, ImageKey};
pub use renderer::{ImageRenderer, ScaleMode, ImagePosition};
pub use pool::{BitmapPool, BitmapBuffer, PoolStats};
pub use queue::{DecodeQueue, DecodeRequest, DecodePriority, DecodeQueueStats};
pub use avif::{AvifDecoder, AvifImage, is_avif};
pub use svg::{SvgDecoder, SvgImage, SvgElement, is_svg};
pub use ico::{IcoDecoder, IcoImage, IcoEntry, is_ico};
pub use sprites::{SpriteSheet, SpriteRegion, SpritePacker, CssSpriteResolver};
pub use srcset::{ResponsiveImageResolver, SrcsetEntry, SizesEntry};
pub use progressive::{ProgressiveDecoder, ProgressiveFormat, DecodeProgress, MmapImage};
pub use texture_atlas_cache::{TextureAtlasCache, TextureAtlas, AtlasEntry, ImageId, AtlasCacheStats};

