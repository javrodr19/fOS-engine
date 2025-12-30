//! Visual effects module
//!
//! Provides box-shadow, opacity, and overflow effects.

use crate::{Canvas, Color};

/// Box shadow definition
#[derive(Debug, Clone, Default)]
pub struct BoxShadow {
    /// Horizontal offset
    pub offset_x: f32,
    /// Vertical offset
    pub offset_y: f32,
    /// Blur radius (gaussian blur)
    pub blur_radius: f32,
    /// Spread radius (expand/contract shadow)
    pub spread_radius: f32,
    /// Shadow color
    pub color: Color,
    /// Inset shadow (inside the box)
    pub inset: bool,
}

impl BoxShadow {
    /// Create a simple drop shadow
    pub fn drop(offset_x: f32, offset_y: f32, blur: f32, color: Color) -> Self {
        Self {
            offset_x,
            offset_y,
            blur_radius: blur,
            spread_radius: 0.0,
            color,
            inset: false,
        }
    }
    
    /// Create shadow with spread
    pub fn with_spread(mut self, spread: f32) -> Self {
        self.spread_radius = spread;
        self
    }
    
    /// Make it an inset shadow
    pub fn inset(mut self) -> Self {
        self.inset = true;
        self
    }
}

/// Overflow behavior
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Overflow {
    /// Content is visible outside the box
    #[default]
    Visible,
    /// Content is clipped, no scrollbars
    Hidden,
    /// Content is clipped, scrollbars always shown
    Scroll,
    /// Content is clipped, scrollbars when needed
    Auto,
}

/// CSS outline definition
#[derive(Debug, Clone, Default)]
pub struct Outline {
    /// Outline width
    pub width: f32,
    /// Outline style
    pub style: OutlineStyle,
    /// Outline color
    pub color: Color,
    /// Outline offset (distance from border edge)
    pub offset: f32,
}

impl Outline {
    /// Create a simple solid outline
    pub fn solid(width: f32, color: Color) -> Self {
        Self {
            width,
            style: OutlineStyle::Solid,
            color,
            offset: 0.0,
        }
    }
    
    /// Set outline offset
    pub fn with_offset(mut self, offset: f32) -> Self {
        self.offset = offset;
        self
    }
}

/// Outline style
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutlineStyle {
    #[default]
    None,
    Solid,
    Dashed,
    Dotted,
    Double,
    Groove,
    Ridge,
    Inset,
    Outset,
}

/// CSS clip-path definition
#[derive(Debug, Clone, PartialEq)]
pub enum ClipPath {
    /// No clipping
    None,
    /// Inset rectangle: inset(top right bottom left)
    Inset(f32, f32, f32, f32),
    /// Circle: circle(radius at cx cy)
    Circle { radius: f32, cx: f32, cy: f32 },
    /// Ellipse: ellipse(rx ry at cx cy)
    Ellipse { rx: f32, ry: f32, cx: f32, cy: f32 },
    /// Polygon: polygon(x1 y1, x2 y2, ...)
    Polygon(Vec<(f32, f32)>),
    /// URL reference to SVG clipPath
    Url(String),
}

impl Default for ClipPath {
    fn default() -> Self {
        Self::None
    }
}

impl ClipPath {
    /// Create a circle clip-path
    pub fn circle(radius: f32, cx: f32, cy: f32) -> Self {
        Self::Circle { radius, cx, cy }
    }
    
    /// Create an ellipse clip-path
    pub fn ellipse(rx: f32, ry: f32, cx: f32, cy: f32) -> Self {
        Self::Ellipse { rx, ry, cx, cy }
    }
    
    /// Create an inset rectangle clip-path
    pub fn inset(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self::Inset(top, right, bottom, left)
    }
    
    /// Create a triangle clip-path
    pub fn triangle() -> Self {
        Self::Polygon(vec![
            (0.5, 0.0),
            (1.0, 1.0),
            (0.0, 1.0),
        ])
    }
    
    /// Check if a point is inside the clip-path
    pub fn contains(&self, x: f32, y: f32, box_width: f32, box_height: f32) -> bool {
        match self {
            Self::None => true,
            Self::Inset(top, right, bottom, left) => {
                x >= *left && x <= box_width - *right &&
                y >= *top && y <= box_height - *bottom
            }
            Self::Circle { radius, cx, cy } => {
                let px = x / box_width;
                let py = y / box_height;
                let dx = px - cx;
                let dy = py - cy;
                (dx * dx + dy * dy).sqrt() <= *radius
            }
            Self::Ellipse { rx, ry, cx, cy } => {
                let px = x / box_width;
                let py = y / box_height;
                let dx = (px - cx) / rx;
                let dy = (py - cy) / ry;
                dx * dx + dy * dy <= 1.0
            }
            Self::Polygon(points) => {
                if points.len() < 3 {
                    return true;
                }
                // Point-in-polygon using ray casting
                let px = x / box_width;
                let py = y / box_height;
                let mut inside = false;
                let n = points.len();
                for i in 0..n {
                    let (x1, y1) = points[i];
                    let (x2, y2) = points[(i + 1) % n];
                    
                    if ((y1 > py) != (y2 > py)) &&
                       (px < (x2 - x1) * (py - y1) / (y2 - y1) + x1) {
                        inside = !inside;
                    }
                }
                inside
            }
            Self::Url(_) => true, // Can't check SVG paths here
        }
    }
}

/// Paint an outline around a box
pub fn paint_outline(
    canvas: &mut Canvas,
    x: f32, y: f32, width: f32, height: f32,
    outline: &Outline,
) {
    if outline.width <= 0.0 || outline.style == OutlineStyle::None {
        return;
    }
    
    let ox = x - outline.offset - outline.width;
    let oy = y - outline.offset - outline.width;
    let ow = width + (outline.offset + outline.width) * 2.0;
    let oh = height + (outline.offset + outline.width) * 2.0;
    
    // Draw outline as 4 rectangles (top, right, bottom, left)
    let w = outline.width;
    let color = outline.color;
    
    // Top
    fill_rect_solid(canvas, ox, oy, ow, w, color);
    // Bottom
    fill_rect_solid(canvas, ox, oy + oh - w, ow, w, color);
    // Left
    fill_rect_solid(canvas, ox, oy + w, w, oh - w * 2.0, color);
    // Right
    fill_rect_solid(canvas, ox + ow - w, oy + w, w, oh - w * 2.0, color);
}

fn fill_rect_solid(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, color: Color) {
    let x_start = x.max(0.0) as u32;
    let y_start = y.max(0.0) as u32;
    let x_end = ((x + width) as u32).min(canvas.width());
    let y_end = ((y + height) as u32).min(canvas.height());
    
    for py in y_start..y_end {
        for px in x_start..x_end {
            canvas.set_pixel(px, py, color);
        }
    }
}

/// Paint a box shadow
pub fn paint_box_shadow(
    canvas: &mut Canvas,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    shadow: &BoxShadow,
) {
    if shadow.inset {
        paint_inset_shadow(canvas, x, y, width, height, shadow);
    } else {
        paint_drop_shadow(canvas, x, y, width, height, shadow);
    }
}

/// Paint a drop shadow (outside the box)
fn paint_drop_shadow(
    canvas: &mut Canvas,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    shadow: &BoxShadow,
) {
    // Calculate shadow bounds
    let shadow_x = x + shadow.offset_x - shadow.spread_radius;
    let shadow_y = y + shadow.offset_y - shadow.spread_radius;
    let shadow_w = width + shadow.spread_radius * 2.0;
    let shadow_h = height + shadow.spread_radius * 2.0;
    
    if shadow.blur_radius <= 0.0 {
        // No blur - just a solid rectangle
        fill_rect_alpha(canvas, shadow_x, shadow_y, shadow_w, shadow_h, shadow.color);
    } else {
        // Approximate blur with multiple passes at decreasing opacity
        let passes = (shadow.blur_radius / 2.0).ceil() as i32;
        let base_alpha = shadow.color.a as f32 / passes as f32;
        
        for i in 0..passes {
            let expand = (i as f32 / passes as f32) * shadow.blur_radius;
            let alpha = (base_alpha * (1.0 - i as f32 / passes as f32)) as u8;
            
            if alpha > 0 {
                let color = Color::rgba(shadow.color.r, shadow.color.g, shadow.color.b, alpha);
                fill_rect_alpha(
                    canvas,
                    shadow_x - expand,
                    shadow_y - expand,
                    shadow_w + expand * 2.0,
                    shadow_h + expand * 2.0,
                    color,
                );
            }
        }
    }
}

/// Paint an inset shadow (inside the box)
fn paint_inset_shadow(
    canvas: &mut Canvas,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    shadow: &BoxShadow,
) {
    // Inset shadows: draw gradient from edges inward
    let blur = shadow.blur_radius.max(1.0);
    let passes = (blur / 2.0).ceil() as i32;
    
    for i in 0..passes {
        let inset = (i as f32 / passes as f32) * blur;
        let alpha = ((shadow.color.a as f32) * (1.0 - i as f32 / passes as f32) / passes as f32) as u8;
        
        if alpha > 0 && inset < width.min(height) / 2.0 {
            let color = Color::rgba(shadow.color.r, shadow.color.g, shadow.color.b, alpha);
            
            // Top edge
            fill_rect_alpha(canvas, x + inset, y + inset, width - inset * 2.0, blur - inset, color);
            // Bottom edge
            fill_rect_alpha(canvas, x + inset, y + height - blur, width - inset * 2.0, blur - inset, color);
            // Left edge
            fill_rect_alpha(canvas, x + inset, y + blur, blur - inset, height - blur * 2.0, color);
            // Right edge
            fill_rect_alpha(canvas, x + width - blur, y + blur, blur - inset, height - blur * 2.0, color);
        }
    }
}

/// Fill rectangle with alpha blending
fn fill_rect_alpha(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, color: Color) {
    let x_start = x.max(0.0) as u32;
    let y_start = y.max(0.0) as u32;
    let x_end = ((x + width) as u32).min(canvas.width());
    let y_end = ((y + height) as u32).min(canvas.height());
    
    if color.a == 255 {
        // Opaque - direct fill
        for py in y_start..y_end {
            for px in x_start..x_end {
                canvas.set_pixel(px, py, color);
            }
        }
    } else if color.a > 0 {
        // Semi-transparent - blend
        let alpha = color.a as f32 / 255.0;
        let inv_alpha = 1.0 - alpha;
        
        for py in y_start..y_end {
            for px in x_start..x_end {
                if let Some(bg) = canvas.get_pixel(px, py) {
                    let blended = Color::rgba(
                        (color.r as f32 * alpha + bg.r as f32 * inv_alpha) as u8,
                        (color.g as f32 * alpha + bg.g as f32 * inv_alpha) as u8,
                        (color.b as f32 * alpha + bg.b as f32 * inv_alpha) as u8,
                        255,
                    );
                    canvas.set_pixel(px, py, blended);
                }
            }
        }
    }
}

/// Apply opacity to a rectangular region
pub fn apply_opacity(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, opacity: f32) {
    if opacity >= 1.0 {
        return; // No change needed
    }
    
    let x_start = x.max(0.0) as u32;
    let y_start = y.max(0.0) as u32;
    let x_end = ((x + width) as u32).min(canvas.width());
    let y_end = ((y + height) as u32).min(canvas.height());
    
    let opacity = opacity.clamp(0.0, 1.0);
    
    for py in y_start..y_end {
        for px in x_start..x_end {
            if let Some(pixel) = canvas.get_pixel(px, py) {
                let new_alpha = (pixel.a as f32 * opacity) as u8;
                canvas.set_pixel(px, py, Color::rgba(pixel.r, pixel.g, pixel.b, new_alpha));
            }
        }
    }
}

/// Set up clipping rectangle for overflow
#[derive(Debug, Clone, Copy)]
pub struct ClipRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ClipRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Check if a point is inside the clip rect
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.x + self.width &&
        py >= self.y && py < self.y + self.height
    }
    
    /// Intersect with another clip rect
    pub fn intersect(&self, other: &ClipRect) -> Option<ClipRect> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = (self.x + self.width).min(other.x + other.width);
        let y2 = (self.y + self.height).min(other.y + other.height);
        
        if x2 > x1 && y2 > y1 {
            Some(ClipRect::new(x1, y1, x2 - x1, y2 - y1))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_box_shadow_creation() {
        let shadow = BoxShadow::drop(2.0, 2.0, 4.0, Color::BLACK);
        assert_eq!(shadow.offset_x, 2.0);
        assert_eq!(shadow.blur_radius, 4.0);
        assert!(!shadow.inset);
    }
    
    #[test]
    fn test_clip_rect_contains() {
        let clip = ClipRect::new(10.0, 10.0, 100.0, 100.0);
        assert!(clip.contains(50.0, 50.0));
        assert!(!clip.contains(5.0, 50.0));
        assert!(!clip.contains(150.0, 50.0));
    }
    
    #[test]
    fn test_clip_rect_intersect() {
        let a = ClipRect::new(0.0, 0.0, 100.0, 100.0);
        let b = ClipRect::new(50.0, 50.0, 100.0, 100.0);
        
        let c = a.intersect(&b).unwrap();
        assert_eq!(c.x, 50.0);
        assert_eq!(c.y, 50.0);
        assert_eq!(c.width, 50.0);
        assert_eq!(c.height, 50.0);
    }
    
    #[test]
    fn test_overflow_default() {
        assert_eq!(Overflow::default(), Overflow::Visible);
    }
}
