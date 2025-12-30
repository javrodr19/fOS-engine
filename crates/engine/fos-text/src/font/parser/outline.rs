//! Glyph outline building

/// Glyph outline builder trait (compatible with tiny-skia)
pub trait OutlineBuilder {
    /// Move to point
    fn move_to(&mut self, x: f32, y: f32);
    /// Line to point
    fn line_to(&mut self, x: f32, y: f32);
    /// Quadratic bezier curve
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32);
    /// Cubic bezier curve
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32);
    /// Close path
    fn close(&mut self);
}

/// A complete glyph outline
#[derive(Debug, Clone, Default)]
pub struct GlyphOutline {
    pub commands: Vec<OutlineCommand>,
}

/// Outline command
#[derive(Debug, Clone)]
pub enum OutlineCommand {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo(f32, f32, f32, f32),
    CurveTo(f32, f32, f32, f32, f32, f32),
    Close,
}

impl OutlineBuilder for GlyphOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(OutlineCommand::MoveTo(x, y));
    }
    
    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(OutlineCommand::LineTo(x, y));
    }
    
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.commands.push(OutlineCommand::QuadTo(x1, y1, x, y));
    }
    
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.commands.push(OutlineCommand::CurveTo(x1, y1, x2, y2, x, y));
    }
    
    fn close(&mut self) {
        self.commands.push(OutlineCommand::Close);
    }
}
