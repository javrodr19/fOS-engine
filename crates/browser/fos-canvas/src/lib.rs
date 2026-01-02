//! fOS Canvas
//!
//! Canvas 2D and WebGL APIs for the fOS browser engine.
//!
//! Features:
//! - CanvasRenderingContext2D with SIMD acceleration
//! - Path2D
//! - ImageData
//! - Transforms and compositing
//! - OffscreenCanvas
//! - WebGL 1.0 with resource pooling

pub mod simd;
pub mod pool;
pub mod context2d;
pub mod path;
pub mod transforms;
pub mod compositing;
pub mod image_data;
pub mod offscreen;
pub mod text;
pub mod drawing;
pub mod webgl;

pub use context2d::{
    CanvasRenderingContext2D, CanvasState, Color,
    FillStyle, StrokeStyle, Gradient, Pattern,
    LineCap, LineJoin, TextAlign, TextBaseline,
};
pub use path::{Path2D, PathCommand};
pub use transforms::TransformMatrix;
pub use compositing::{CompositeOperation, BlendMode, blend_colors};
pub use image_data::{ImageData, ColorSpace};
pub use offscreen::{OffscreenCanvas, ImageBitmap};
pub use text::{TextMetrics, TextDrawing};
pub use drawing::{CanvasImageSource, ImageDrawing};
pub use webgl::{WebGLRenderingContext, WebGLProgram, WebGLShader, WebGLBuffer, WebGLTexture};

/// Canvas error
#[derive(Debug, thiserror::Error)]
pub enum CanvasError {
    #[error("Invalid state: {0}")]
    InvalidState(String),
    
    #[error("Not supported: {0}")]
    NotSupported(String),
}
