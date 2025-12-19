//! Painter - paints layout boxes to canvas

use crate::Canvas;

/// Painter for rendering layout trees
pub struct Painter {
    canvas: Canvas,
}

impl Painter {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            canvas: Canvas::new(width, height),
        }
    }
    
    /// Get the canvas
    pub fn canvas(&self) -> &Canvas {
        &self.canvas
    }
    
    /// Get mutable canvas
    pub fn canvas_mut(&mut self) -> &mut Canvas {
        &mut self.canvas
    }
}
