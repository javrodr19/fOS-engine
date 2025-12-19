//! Image handling module
//!
//! Provides image decoding, caching, pooling, and rendering for the browser engine.

mod decoder;
mod cache;
mod renderer;
pub mod pool;
pub mod queue;

pub use decoder::{ImageDecoder, DecodedImage, ImageFormat};
pub use cache::{ImageCache, ImageKey};
pub use renderer::{ImageRenderer, ScaleMode, ImagePosition};
pub use pool::{BitmapPool, BitmapBuffer, PoolStats};
pub use queue::{DecodeQueue, DecodeRequest, DecodePriority, DecodeQueueStats};
