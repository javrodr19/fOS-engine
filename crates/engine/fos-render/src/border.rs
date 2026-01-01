//! Border painting
//! 
//! Implements CSS border-style and border-image support.

use crate::{Canvas, Color};
use crate::paint::{Border, BorderStyle, BorderRadius};

// ============================================================================
// Border Image Support
// ============================================================================

/// CSS border-image definition
#[derive(Debug, Clone)]
pub struct BorderImage {
    /// Source image data (RGBA, width, height)
    pub source: Option<BorderImageSource>,
    /// Slice values (top, right, bottom, left) in pixels or percentages
    pub slice: BorderImageSlice,
    /// Width of border-image (overrides border-width)
    pub width: BorderImageWidth,
    /// Outset (how far the image extends beyond border box)
    pub outset: EdgeValues,
    /// How to fill the middle area
    pub repeat: BorderImageRepeat,
    /// Fill the middle slice
    pub fill: bool,
}

impl Default for BorderImage {
    fn default() -> Self {
        Self {
            source: None,
            slice: BorderImageSlice::default(),
            width: BorderImageWidth::default(),
            outset: EdgeValues::default(),
            repeat: BorderImageRepeat::default(),
            fill: false,
        }
    }
}

/// Border image source
#[derive(Debug, Clone)]
pub struct BorderImageSource {
    /// RGBA pixel data
    pub data: Vec<u8>,
    /// Image width
    pub width: u32,
    /// Image height
    pub height: u32,
}

/// Border image slice values
#[derive(Debug, Clone, Default)]
pub struct BorderImageSlice {
    pub top: SliceValue,
    pub right: SliceValue,
    pub bottom: SliceValue,
    pub left: SliceValue,
}

impl BorderImageSlice {
    pub fn all(value: f32) -> Self {
        Self {
            top: SliceValue::Pixels(value),
            right: SliceValue::Pixels(value),
            bottom: SliceValue::Pixels(value),
            left: SliceValue::Pixels(value),
        }
    }
    
    pub fn percent(value: f32) -> Self {
        Self {
            top: SliceValue::Percent(value),
            right: SliceValue::Percent(value),
            bottom: SliceValue::Percent(value),
            left: SliceValue::Percent(value),
        }
    }
}

/// Slice value (pixels or percentage)
#[derive(Debug, Clone, Copy, Default)]
pub enum SliceValue {
    #[default]
    Auto,
    Pixels(f32),
    Percent(f32),
}

impl SliceValue {
    pub fn resolve(&self, dimension: f32) -> f32 {
        match self {
            SliceValue::Auto => 0.0,
            SliceValue::Pixels(px) => *px,
            SliceValue::Percent(pct) => dimension * pct / 100.0,
        }
    }
}

/// Border image width
#[derive(Debug, Clone, Default)]
pub struct BorderImageWidth {
    pub top: WidthValue,
    pub right: WidthValue,
    pub bottom: WidthValue,
    pub left: WidthValue,
}

/// Width value for border-image-width
#[derive(Debug, Clone, Copy, Default)]
pub enum WidthValue {
    #[default]
    Auto,
    Pixels(f32),
    Percent(f32),
    Number(f32), // Multiplier of border-width
}

/// Edge values for outset
#[derive(Debug, Clone, Default)]
pub struct EdgeValues {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeValues {
    pub fn all(value: f32) -> Self {
        Self { top: value, right: value, bottom: value, left: value }
    }
}

/// Border image repeat mode
#[derive(Debug, Clone, Copy, Default)]
pub struct BorderImageRepeat {
    pub horizontal: RepeatMode,
    pub vertical: RepeatMode,
}

/// How to repeat/scale border image slices
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum RepeatMode {
    #[default]
    Stretch,
    Repeat,
    Round,
    Space,
}

/// Paint a border image
pub fn paint_border_image(
    canvas: &mut Canvas,
    x: f32, y: f32,
    width: f32, height: f32,
    image: &BorderImage,
) {
    let source = match &image.source {
        Some(s) => s,
        None => return,
    };
    
    if source.width == 0 || source.height == 0 {
        return;
    }
    
    // Calculate slice regions
    let slice_top = image.slice.top.resolve(source.height as f32);
    let slice_right = image.slice.right.resolve(source.width as f32);
    let slice_bottom = image.slice.bottom.resolve(source.height as f32);
    let slice_left = image.slice.left.resolve(source.width as f32);
    
    // Apply outset
    let draw_x = x - image.outset.left;
    let draw_y = y - image.outset.top;
    let draw_w = width + image.outset.left + image.outset.right;
    let draw_h = height + image.outset.top + image.outset.bottom;
    
    // Draw corner slices (always stretched to fit)
    // Top-left corner
    draw_image_slice(
        canvas, source,
        0.0, 0.0, slice_left, slice_top, // src
        draw_x, draw_y, slice_left, slice_top, // dst
    );
    
    // Top-right corner
    draw_image_slice(
        canvas, source,
        source.width as f32 - slice_right, 0.0, slice_right, slice_top,
        draw_x + draw_w - slice_right, draw_y, slice_right, slice_top,
    );
    
    // Bottom-left corner
    draw_image_slice(
        canvas, source,
        0.0, source.height as f32 - slice_bottom, slice_left, slice_bottom,
        draw_x, draw_y + draw_h - slice_bottom, slice_left, slice_bottom,
    );
    
    // Bottom-right corner
    draw_image_slice(
        canvas, source,
        source.width as f32 - slice_right, source.height as f32 - slice_bottom, slice_right, slice_bottom,
        draw_x + draw_w - slice_right, draw_y + draw_h - slice_bottom, slice_right, slice_bottom,
    );
    
    // Edge slices (stretched for now - full implementation would handle repeat modes)
    let edge_w = source.width as f32 - slice_left - slice_right;
    let edge_h = source.height as f32 - slice_top - slice_bottom;
    let dst_edge_w = draw_w - slice_left - slice_right;
    let dst_edge_h = draw_h - slice_top - slice_bottom;
    
    // Top edge
    draw_image_slice(
        canvas, source,
        slice_left, 0.0, edge_w, slice_top,
        draw_x + slice_left, draw_y, dst_edge_w, slice_top,
    );
    
    // Bottom edge
    draw_image_slice(
        canvas, source,
        slice_left, source.height as f32 - slice_bottom, edge_w, slice_bottom,
        draw_x + slice_left, draw_y + draw_h - slice_bottom, dst_edge_w, slice_bottom,
    );
    
    // Left edge
    draw_image_slice(
        canvas, source,
        0.0, slice_top, slice_left, edge_h,
        draw_x, draw_y + slice_top, slice_left, dst_edge_h,
    );
    
    // Right edge
    draw_image_slice(
        canvas, source,
        source.width as f32 - slice_right, slice_top, slice_right, edge_h,
        draw_x + draw_w - slice_right, draw_y + slice_top, slice_right, dst_edge_h,
    );
    
    // Center fill (if enabled)
    if image.fill && edge_w > 0.0 && edge_h > 0.0 {
        draw_image_slice(
            canvas, source,
            slice_left, slice_top, edge_w, edge_h,
            draw_x + slice_left, draw_y + slice_top, dst_edge_w, dst_edge_h,
        );
    }
}

/// Draw a slice of the source image to the canvas (with scaling)
fn draw_image_slice(
    canvas: &mut Canvas,
    source: &BorderImageSource,
    src_x: f32, src_y: f32, src_w: f32, src_h: f32,
    dst_x: f32, dst_y: f32, dst_w: f32, dst_h: f32,
) {
    if src_w <= 0.0 || src_h <= 0.0 || dst_w <= 0.0 || dst_h <= 0.0 {
        return;
    }
    
    let dst_x_start = dst_x.max(0.0) as u32;
    let dst_y_start = dst_y.max(0.0) as u32;
    let dst_x_end = ((dst_x + dst_w) as u32).min(canvas.width());
    let dst_y_end = ((dst_y + dst_h) as u32).min(canvas.height());
    
    for py in dst_y_start..dst_y_end {
        for px in dst_x_start..dst_x_end {
            // Map destination pixel to source
            let rel_x = (px as f32 - dst_x) / dst_w;
            let rel_y = (py as f32 - dst_y) / dst_h;
            
            let sx = (src_x + rel_x * src_w) as u32;
            let sy = (src_y + rel_y * src_h) as u32;
            
            if sx < source.width && sy < source.height {
                let idx = ((sy * source.width + sx) * 4) as usize;
                if idx + 3 < source.data.len() {
                    let color = Color::rgba(
                        source.data[idx],
                        source.data[idx + 1],
                        source.data[idx + 2],
                        source.data[idx + 3],
                    );
                    
                    if color.a > 0 {
                        canvas.set_pixel(px, py, color);
                    }
                }
            }
        }
    }
}

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
