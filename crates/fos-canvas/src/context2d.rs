//! Canvas 2D Rendering Context
//!
//! CanvasRenderingContext2D implementation.

use crate::path::Path2D;
use crate::transforms::TransformMatrix;
use crate::compositing::{CompositeOperation, BlendMode};

/// Canvas 2D rendering context
#[derive(Debug)]
pub struct CanvasRenderingContext2D {
    /// Canvas width
    width: u32,
    /// Canvas height
    height: u32,
    /// Pixel data (RGBA)
    data: Vec<u8>,
    /// State stack
    states: Vec<CanvasState>,
    /// Current path
    current_path: Path2D,
}

/// Canvas state (for save/restore)
#[derive(Debug, Clone)]
pub struct CanvasState {
    pub transform: TransformMatrix,
    pub fill_style: FillStyle,
    pub stroke_style: StrokeStyle,
    pub line_width: f64,
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub miter_limit: f64,
    pub line_dash: Vec<f64>,
    pub line_dash_offset: f64,
    pub font: String,
    pub text_align: TextAlign,
    pub text_baseline: TextBaseline,
    pub global_alpha: f64,
    pub global_composite_operation: CompositeOperation,
    pub shadow_offset_x: f64,
    pub shadow_offset_y: f64,
    pub shadow_blur: f64,
    pub shadow_color: Color,
    pub clip_path: Option<Path2D>,
}

/// Fill style
#[derive(Debug, Clone)]
pub enum FillStyle {
    Color(Color),
    Gradient(Gradient),
    Pattern(Pattern),
}

/// Stroke style
#[derive(Debug, Clone)]
pub enum StrokeStyle {
    Color(Color),
    Gradient(Gradient),
    Pattern(Pattern),
}

/// Color (RGBA)
#[derive(Debug, Clone, Copy, Default)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Gradient
#[derive(Debug, Clone)]
pub struct Gradient {
    pub gradient_type: GradientType,
    pub stops: Vec<ColorStop>,
}

#[derive(Debug, Clone, Copy)]
pub enum GradientType {
    Linear { x0: f64, y0: f64, x1: f64, y1: f64 },
    Radial { x0: f64, y0: f64, r0: f64, x1: f64, y1: f64, r1: f64 },
    Conic { x: f64, y: f64, angle: f64 },
}

#[derive(Debug, Clone)]
pub struct ColorStop {
    pub offset: f64,
    pub color: Color,
}

/// Pattern
#[derive(Debug, Clone)]
pub struct Pattern {
    pub image_data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub repetition: PatternRepetition,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum PatternRepetition {
    #[default]
    Repeat,
    RepeatX,
    RepeatY,
    NoRepeat,
}

/// Line cap
#[derive(Debug, Clone, Copy, Default)]
pub enum LineCap {
    #[default]
    Butt,
    Round,
    Square,
}

/// Line join
#[derive(Debug, Clone, Copy, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    Round,
    Bevel,
}

/// Text alignment
#[derive(Debug, Clone, Copy, Default)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
}

/// Text baseline
#[derive(Debug, Clone, Copy, Default)]
pub enum TextBaseline {
    Top,
    Hanging,
    Middle,
    #[default]
    Alphabetic,
    Ideographic,
    Bottom,
}

impl CanvasRenderingContext2D {
    /// Create a new 2D context
    pub fn new(width: u32, height: u32) -> Self {
        let data = vec![0u8; (width * height * 4) as usize];
        Self {
            width,
            height,
            data,
            states: vec![CanvasState::default()],
            current_path: Path2D::new(),
        }
    }
    
    /// Get canvas width
    pub fn width(&self) -> u32 {
        self.width
    }
    
    /// Get canvas height
    pub fn height(&self) -> u32 {
        self.height
    }
    
    /// Get pixel data
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    
    /// Get mutable pixel data
    pub fn data_mut(&mut self) -> &mut [u8] {
        &mut self.data
    }
    
    // State management
    
    /// Save current state
    pub fn save(&mut self) {
        if let Some(state) = self.states.last() {
            self.states.push(state.clone());
        }
    }
    
    /// Restore previous state
    pub fn restore(&mut self) {
        if self.states.len() > 1 {
            self.states.pop();
        }
    }
    
    /// Get current state
    pub fn state(&self) -> &CanvasState {
        self.states.last().unwrap()
    }
    
    /// Get mutable current state
    pub fn state_mut(&mut self) -> &mut CanvasState {
        self.states.last_mut().unwrap()
    }
    
    // Drawing rectangles
    
    /// Fill a rectangle
    pub fn fill_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        let color = match &self.state().fill_style {
            FillStyle::Color(c) => *c,
            _ => Color::default(),
        };
        
        let alpha = self.state().global_alpha;
        self.draw_rect(x, y, width, height, color, alpha);
    }
    
    /// Stroke a rectangle
    pub fn stroke_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        let color = match &self.state().stroke_style {
            StrokeStyle::Color(c) => *c,
            _ => Color::default(),
        };
        
        let line_width = self.state().line_width;
        let alpha = self.state().global_alpha;
        
        // Top
        self.draw_rect(x, y, width, line_width, color, alpha);
        // Bottom
        self.draw_rect(x, y + height - line_width, width, line_width, color, alpha);
        // Left
        self.draw_rect(x, y, line_width, height, color, alpha);
        // Right
        self.draw_rect(x + width - line_width, y, line_width, height, color, alpha);
    }
    
    /// Clear a rectangle
    pub fn clear_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
        self.draw_rect(x, y, width, height, Color { r: 0, g: 0, b: 0, a: 0 }, 1.0);
    }
    
    fn draw_rect(&mut self, x: f64, y: f64, width: f64, height: f64, color: Color, alpha: f64) {
        let x0 = x.max(0.0) as u32;
        let y0 = y.max(0.0) as u32;
        let x1 = ((x + width) as u32).min(self.width);
        let y1 = ((y + height) as u32).min(self.height);
        
        let a = (color.a as f64 * alpha) as u8;
        
        for py in y0..y1 {
            for px in x0..x1 {
                let idx = ((py * self.width + px) * 4) as usize;
                if idx + 3 < self.data.len() {
                    self.data[idx] = color.r;
                    self.data[idx + 1] = color.g;
                    self.data[idx + 2] = color.b;
                    self.data[idx + 3] = a;
                }
            }
        }
    }
    
    // Path methods
    
    /// Begin a new path
    pub fn begin_path(&mut self) {
        self.current_path = Path2D::new();
    }
    
    /// Close the current path
    pub fn close_path(&mut self) {
        self.current_path.close_path();
    }
    
    /// Move to point
    pub fn move_to(&mut self, x: f64, y: f64) {
        self.current_path.move_to(x, y);
    }
    
    /// Line to point
    pub fn line_to(&mut self, x: f64, y: f64) {
        self.current_path.line_to(x, y);
    }
    
    /// Fill the current path
    pub fn fill(&mut self) {
        // Would rasterize path and fill
    }
    
    /// Stroke the current path
    pub fn stroke(&mut self) {
        // Would rasterize path and stroke
    }
    
    /// Clip to current path
    pub fn clip(&mut self) {
        self.state_mut().clip_path = Some(self.current_path.clone());
    }
}

impl Default for CanvasState {
    fn default() -> Self {
        Self {
            transform: TransformMatrix::identity(),
            fill_style: FillStyle::Color(Color { r: 0, g: 0, b: 0, a: 255 }),
            stroke_style: StrokeStyle::Color(Color { r: 0, g: 0, b: 0, a: 255 }),
            line_width: 1.0,
            line_cap: LineCap::default(),
            line_join: LineJoin::default(),
            miter_limit: 10.0,
            line_dash: Vec::new(),
            line_dash_offset: 0.0,
            font: "10px sans-serif".to_string(),
            text_align: TextAlign::default(),
            text_baseline: TextBaseline::default(),
            global_alpha: 1.0,
            global_composite_operation: CompositeOperation::default(),
            shadow_offset_x: 0.0,
            shadow_offset_y: 0.0,
            shadow_blur: 0.0,
            shadow_color: Color::default(),
            clip_path: None,
        }
    }
}

impl Color {
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_context() {
        let ctx = CanvasRenderingContext2D::new(100, 100);
        assert_eq!(ctx.width(), 100);
        assert_eq!(ctx.height(), 100);
    }
    
    #[test]
    fn test_fill_rect() {
        let mut ctx = CanvasRenderingContext2D::new(100, 100);
        ctx.state_mut().fill_style = FillStyle::Color(Color::rgb(255, 0, 0));
        ctx.fill_rect(10.0, 10.0, 20.0, 20.0);
        
        // Check pixel at (15, 15)
        let idx = (15 * 100 + 15) * 4;
        assert_eq!(ctx.data()[idx as usize], 255); // Red
    }
    
    #[test]
    fn test_save_restore() {
        let mut ctx = CanvasRenderingContext2D::new(100, 100);
        ctx.state_mut().global_alpha = 0.5;
        ctx.save();
        ctx.state_mut().global_alpha = 0.3;
        assert_eq!(ctx.state().global_alpha, 0.3);
        ctx.restore();
        assert_eq!(ctx.state().global_alpha, 0.5);
    }
}
