//! fOS Render - Painting Engine
//!
//! CPU rendering of layout trees using tiny-skia.
//! Optional GPU rendering with wgpu.
//!
//! This crate provides:
//! - Canvas backed by tiny-skia Pixmap
//! - Background painting (solid colors, border-radius)
//! - Border painting (solid, dashed, dotted)
//! - Layout tree painter
//! - Text rendering
//! - Image rendering
//! - Visual effects (box-shadow, opacity, overflow)
//! - CSS transforms (rotate, scale, skew, translate)
//! - CSS animations (transitions, keyframes)
//! - CSS filters (blur, brightness, contrast, etc.)
//! - GPU compositing and layer management
//! - GPU rendering with wgpu (optional)

mod canvas;
mod paint;
mod background;
mod border;
mod painter;
pub mod media;
pub mod text;
pub mod image;
pub mod effects;
pub mod transform;
pub mod animation;
pub mod filters;
pub mod compositor;
pub mod layers;
pub mod gpu;
pub mod webgl;
pub mod webgpu;
pub mod render_opt;
pub mod gradient;

pub use canvas::Canvas;
pub use paint::{FillStyle, StrokeStyle, Border, BorderSide, BorderStyle, BorderRadius, DashPattern};
pub use background::Background;
pub use painter::{Painter, BoxStyle, BoxStyles, css_color_to_render};
pub use text::TextRenderer;
pub use image::{ImageRenderer, ImageDecoder, DecodedImage, ImageCache, ImageFormat};
pub use effects::{
    BoxShadow, Overflow, ClipRect, paint_box_shadow, apply_opacity,
    Outline, OutlineStyle, ClipPath, paint_outline,
};
pub use transform::{
    Transform2D, TransformOrigin, transform_around_origin,
    Transform3D, BackfaceVisibility, PerspectiveOrigin,
};
pub use animation::{
    TimingFunction, Transition, Keyframe, KeyframeAnimation, 
    AnimatedValue, AnimationInstance, AnimationDirection, FillMode
};
pub use filters::{
    FilterFunction, FilterList, BlendMode, apply_filters,
    brightness_4, grayscale_4, blend_4, invert_4, alpha_blend_4,
};
pub use gpu::{GpuRenderer, GpuConfig, GpuState, GpuBackend, RenderFrame, TextureId, GpuError};
pub use webgl::{WebGLRenderingContext, WebGLVersion, ShaderType, TextureFormat};
pub use webgpu::{GPUDevice, GPURenderPipeline, GPUComputePipeline, GPUBuffer, GPUTexture};
pub use render_opt::{DisplayList, TextureAtlas, DirtyRectTracker, OcclusionCuller, RenderTreeDiffer, OffscreenCanvas};
pub use gradient::{
    Gradient, ColorStop, GradientDirection, RadialShape, RadialExtent,
    fill_gradient, fill_linear_gradient, fill_radial_gradient, fill_conic_gradient,
    parse_gradient,
};

/// Color (RGBA)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const WHITE: Color = Color { r: 255, g: 255, b: 255, a: 255 };
    pub const BLACK: Color = Color { r: 0, g: 0, b: 0, a: 255 };
    pub const TRANSPARENT: Color = Color { r: 0, g: 0, b: 0, a: 0 };
    pub const RED: Color = Color { r: 255, g: 0, b: 0, a: 255 };
    pub const GREEN: Color = Color { r: 0, g: 255, b: 0, a: 255 };
    pub const BLUE: Color = Color { r: 0, g: 0, b: 255, a: 255 };
    
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    
    /// Create from hex string (e.g., "#ff0000")
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.strip_prefix('#').unwrap_or(hex);
        
        match hex.len() {
            3 => {
                let r = u8::from_str_radix(&hex[0..1], 16).ok()?;
                let g = u8::from_str_radix(&hex[1..2], 16).ok()?;
                let b = u8::from_str_radix(&hex[2..3], 16).ok()?;
                Some(Color::rgb(r * 17, g * 17, b * 17))
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                Some(Color::rgb(r, g, b))
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                Some(Color::rgba(r, g, b, a))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_color_constants() {
        assert_eq!(Color::WHITE.r, 255);
        assert_eq!(Color::BLACK.r, 0);
        assert_eq!(Color::TRANSPARENT.a, 0);
    }
    
    #[test]
    fn test_color_from_hex() {
        assert_eq!(Color::from_hex("#ff0000"), Some(Color::RED));
        assert_eq!(Color::from_hex("00ff00"), Some(Color::GREEN));
        assert_eq!(Color::from_hex("#f00"), Some(Color::RED));
    }
}
