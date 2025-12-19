//! Image handling module
//!
//! Provides image decoding, caching, and rendering for the browser engine.

mod decoder;
mod cache;
mod renderer;

pub use decoder::{ImageDecoder, DecodedImage, ImageFormat};
pub use cache::{ImageCache, ImageKey};
pub use renderer::{ImageRenderer, ScaleMode, ImagePosition};
