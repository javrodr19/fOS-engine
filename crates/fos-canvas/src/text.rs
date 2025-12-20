//! Text Drawing
//!
//! Canvas 2D text methods.

/// Text metrics
#[derive(Debug, Clone, Default)]
pub struct TextMetrics {
    pub width: f64,
    pub actual_bounding_box_left: f64,
    pub actual_bounding_box_right: f64,
    pub font_bounding_box_ascent: f64,
    pub font_bounding_box_descent: f64,
    pub actual_bounding_box_ascent: f64,
    pub actual_bounding_box_descent: f64,
    pub em_height_ascent: f64,
    pub em_height_descent: f64,
    pub hanging_baseline: f64,
    pub alphabetic_baseline: f64,
    pub ideographic_baseline: f64,
}

/// Text drawing functions for CanvasRenderingContext2D
pub trait TextDrawing {
    /// Fill text
    fn fill_text(&mut self, text: &str, x: f64, y: f64);
    
    /// Fill text with max width
    fn fill_text_max(&mut self, text: &str, x: f64, y: f64, max_width: f64);
    
    /// Stroke text
    fn stroke_text(&mut self, text: &str, x: f64, y: f64);
    
    /// Stroke text with max width 
    fn stroke_text_max(&mut self, text: &str, x: f64, y: f64, max_width: f64);
    
    /// Measure text
    fn measure_text(&self, text: &str) -> TextMetrics;
}

impl TextDrawing for super::context2d::CanvasRenderingContext2D {
    fn fill_text(&mut self, text: &str, x: f64, y: f64) {
        self.fill_text_max(text, x, y, f64::INFINITY);
    }
    
    fn fill_text_max(&mut self, _text: &str, _x: f64, _y: f64, _max_width: f64) {
        // Would rasterize text using the font in state
        // Uses self.state().font, text_align, text_baseline
    }
    
    fn stroke_text(&mut self, text: &str, x: f64, y: f64) {
        self.stroke_text_max(text, x, y, f64::INFINITY);
    }
    
    fn stroke_text_max(&mut self, _text: &str, _x: f64, _y: f64, _max_width: f64) {
        // Would stroke text outline
    }
    
    fn measure_text(&self, text: &str) -> TextMetrics {
        // Simple estimation: ~8 pixels per character
        let width = text.len() as f64 * 8.0;
        TextMetrics {
            width,
            actual_bounding_box_left: 0.0,
            actual_bounding_box_right: width,
            font_bounding_box_ascent: 10.0,
            font_bounding_box_descent: 3.0,
            actual_bounding_box_ascent: 10.0,
            actual_bounding_box_descent: 3.0,
            em_height_ascent: 10.0,
            em_height_descent: 3.0,
            hanging_baseline: 8.0,
            alphabetic_baseline: 0.0,
            ideographic_baseline: -3.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CanvasRenderingContext2D;
    
    #[test]
    fn test_measure_text() {
        let ctx = CanvasRenderingContext2D::new(100, 100);
        let metrics = ctx.measure_text("Hello");
        assert!(metrics.width > 0.0);
    }
}
