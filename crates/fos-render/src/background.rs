//! Background painting

use crate::{Canvas, Color};
use crate::paint::BorderRadius;

/// Background specification
#[derive(Debug, Clone, Default)]
pub struct Background {
    /// Background color
    pub color: Option<Color>,
    // Future: images, gradients
}

impl Background {
    /// Create solid color background
    pub fn color(color: Color) -> Self {
        Self { color: Some(color) }
    }
    
    /// Check if background has content to paint
    pub fn is_visible(&self) -> bool {
        self.color.map(|c| c.a > 0).unwrap_or(false)
    }
}

/// Paint a background
pub fn paint_background(
    canvas: &mut Canvas,
    x: f32, y: f32,
    width: f32, height: f32,
    background: &Background,
    radius: &BorderRadius,
) {
    if let Some(color) = background.color {
        if color.a == 0 {
            return;
        }
        
        if radius.has_radius() {
            // Use rounded rect with max radius for simplicity
            canvas.fill_rounded_rect(x, y, width, height, radius.max(), color);
        } else {
            canvas.fill_rect(x, y, width, height, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_background_paint() {
        let mut canvas = Canvas::new(100, 100).unwrap();
        canvas.clear(Color::WHITE);
        
        let bg = Background::color(Color::rgb(100, 150, 200));
        paint_background(&mut canvas, 10.0, 10.0, 50.0, 50.0, &bg, &BorderRadius::default());
        
        let pixel = canvas.get_pixel(35, 35).unwrap();
        assert_eq!(pixel.r, 100);
        assert_eq!(pixel.g, 150);
        assert_eq!(pixel.b, 200);
    }
}
