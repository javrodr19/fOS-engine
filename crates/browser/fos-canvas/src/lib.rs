//! fOS Canvas
//!
//! Canvas APIs for the fOS browser engine:
//! - The 2D context (`Canvas2D`): paths, fills and strokes, gradients and
//!   patterns, compositing, shadows, clipping, text, images and pixel
//!   access, drawn with tiny-skia
//! - WebGL 1.0 with resource pooling

pub mod pool;
pub mod canvas2d;
pub mod text2d;
pub mod svg_path;
pub mod webgl;

pub use canvas2d::{
    build_path, Canvas2D, CanvasPattern, Composite, Direction, Gradient, GradientKind, PathCmd, PathData, Repetition, SmoothingQuality, State,
    Style, TextAlign, TextBaseline,
};
pub use svg_path::parse_svg_path;
pub use text2d::{parse_font, FontSpec, TextMetrics2D};
pub use webgl::{WebGLRenderingContext, WebGLProgram, WebGLShader, WebGLBuffer, WebGLTexture};
/// tiny-skia, for callers handing bitmaps to `Canvas2D`
pub use tiny_skia;

/// Canvas error
#[derive(Debug, thiserror::Error)]
pub enum CanvasError {
    #[error("Invalid state: {0}")]
    InvalidState(String),

    #[error("Not supported: {0}")]
    NotSupported(String),
}
