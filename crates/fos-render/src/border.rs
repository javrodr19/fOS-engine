//! Border painting

use crate::{Canvas, Color};
use crate::paint::{Border, BorderStyle, BorderRadius};

/// Paint borders around a box
pub fn paint_border(
    canvas: &mut Canvas,
    x: f32, y: f32,
    width: f32, height: f32,
    border: &Border,
    _radius: &BorderRadius, // TODO: Implement rounded borders
) {
    // Top border
    if border.top.is_visible() {
        paint_border_side(
            canvas,
            x, y,
            x + width, y,
            border.top.width,
            border.top.style,
            border.top.color,
        );
    }
    
    // Right border
    if border.right.is_visible() {
        paint_border_side(
            canvas,
            x + width, y,
            x + width, y + height,
            border.right.width,
            border.right.style,
            border.right.color,
        );
    }
    
    // Bottom border
    if border.bottom.is_visible() {
        paint_border_side(
            canvas,
            x + width, y + height,
            x, y + height,
            border.bottom.width,
            border.bottom.style,
            border.bottom.color,
        );
    }
    
    // Left border
    if border.left.is_visible() {
        paint_border_side(
            canvas,
            x, y + height,
            x, y,
            border.left.width,
            border.left.style,
            border.left.color,
        );
    }
}

/// Paint a single border side as a filled trapezoid area
fn paint_border_side(
    canvas: &mut Canvas,
    x1: f32, y1: f32,
    x2: f32, y2: f32,
    width: f32,
    style: BorderStyle,
    color: Color,
) {
    if width <= 0.0 || color.a == 0 {
        return;
    }
    
    match style {
        BorderStyle::None | BorderStyle::Hidden => return,
        BorderStyle::Solid => {
            canvas.draw_line(x1, y1, x2, y2, width, color);
        }
        BorderStyle::Dashed => {
            // Simple dashed implementation
            paint_dashed_line(canvas, x1, y1, x2, y2, width, color, width * 3.0, width * 2.0);
        }
        BorderStyle::Dotted => {
            // Dotted as small dashes
            paint_dashed_line(canvas, x1, y1, x2, y2, width, color, width, width);
        }
        BorderStyle::Double => {
            // Two lines with gap
            let third = width / 3.0;
            canvas.draw_line(x1, y1, x2, y2, third, color);
            // Offset for second line - simplified
        }
    }
}

/// Paint a dashed line
fn paint_dashed_line(
    canvas: &mut Canvas,
    x1: f32, y1: f32,
    x2: f32, y2: f32,
    stroke_width: f32,
    color: Color,
    dash_length: f32,
    gap_length: f32,
) {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let length = (dx * dx + dy * dy).sqrt();
    
    if length == 0.0 {
        return;
    }
    
    let ux = dx / length;
    let uy = dy / length;
    
    let mut pos = 0.0;
    let mut drawing = true;
    
    while pos < length {
        let segment_length = if drawing { dash_length } else { gap_length };
        let end_pos = (pos + segment_length).min(length);
        
        if drawing {
            let sx = x1 + ux * pos;
            let sy = y1 + uy * pos;
            let ex = x1 + ux * end_pos;
            let ey = y1 + uy * end_pos;
            
            canvas.draw_line(sx, sy, ex, ey, stroke_width, color);
        }
        
        pos = end_pos;
        drawing = !drawing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::BorderSide;
    
    #[test]
    fn test_paint_border() {
        let mut canvas = Canvas::new(100, 100).unwrap();
        canvas.clear(Color::WHITE);
        
        let border = Border::all(2.0, BorderStyle::Solid, Color::BLACK);
        paint_border(&mut canvas, 10.0, 10.0, 50.0, 50.0, &border, &BorderRadius::default());
        
        // Border should be painted (check a point on top edge)
        let pixel = canvas.get_pixel(35, 10).unwrap();
        // Due to anti-aliasing, just ensure it's darker than white
        assert!(pixel.r < 255 || pixel.g < 255 || pixel.b < 255);
    }
    
    #[test]
    fn test_dashed_border() {
        let mut canvas = Canvas::new(100, 100).unwrap();
        canvas.clear(Color::WHITE);
        
        let border = Border::all(2.0, BorderStyle::Dashed, Color::rgb(255, 0, 0));
        paint_border(&mut canvas, 10.0, 10.0, 50.0, 50.0, &border, &BorderRadius::default());
        
        // Should not crash
    }
}
