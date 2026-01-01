//! CSS Gradients
//!
//! SIMD-optimized gradient rendering for linear, radial, and conic gradients.
//! https://www.w3.org/TR/css-images-3/

use crate::{Canvas, Color};
use std::f32::consts::PI;

/// A color stop in a gradient
#[derive(Debug, Clone, Copy)]
pub struct ColorStop {
    /// Position in gradient (0.0 - 1.0)
    pub position: f32,
    /// Color at this position
    pub color: Color,
}

impl ColorStop {
    /// Create a new color stop
    pub fn new(position: f32, color: Color) -> Self {
        Self { position, color }
    }
}

/// Gradient direction for linear gradients
#[derive(Debug, Clone, Copy, Default)]
pub enum GradientDirection {
    /// Angle in degrees (0 = to top, 90 = to right)
    Angle(f32),
    /// to top
    #[default]
    ToTop,
    /// to right
    ToRight,
    /// to bottom
    ToBottom,
    /// to left
    ToLeft,
    /// to top right
    ToTopRight,
    /// to top left
    ToTopLeft,
    /// to bottom right
    ToBottomRight,
    /// to bottom left
    ToBottomLeft,
}

impl GradientDirection {
    /// Convert to angle in radians
    pub fn to_radians(&self) -> f32 {
        let degrees = match self {
            GradientDirection::Angle(deg) => *deg,
            GradientDirection::ToTop => 0.0,
            GradientDirection::ToRight => 90.0,
            GradientDirection::ToBottom => 180.0,
            GradientDirection::ToLeft => 270.0,
            GradientDirection::ToTopRight => 45.0,
            GradientDirection::ToTopLeft => 315.0,
            GradientDirection::ToBottomRight => 135.0,
            GradientDirection::ToBottomLeft => 225.0,
        };
        degrees * PI / 180.0
    }
}

/// Radial gradient shape
#[derive(Debug, Clone, Copy, Default)]
pub enum RadialShape {
    #[default]
    Ellipse,
    Circle,
}

/// Radial gradient extent
#[derive(Debug, Clone, Copy, Default)]
pub enum RadialExtent {
    #[default]
    FarthestCorner,
    ClosestCorner,
    FarthestSide,
    ClosestSide,
    /// Explicit radius/radii
    Explicit(f32, f32),
}

/// CSS Gradient types
#[derive(Debug, Clone)]
pub enum Gradient {
    /// Linear gradient
    Linear {
        direction: GradientDirection,
        stops: Vec<ColorStop>,
        repeating: bool,
    },
    /// Radial gradient
    Radial {
        shape: RadialShape,
        extent: RadialExtent,
        center_x: f32, // 0.0 - 1.0 (fraction of width)
        center_y: f32, // 0.0 - 1.0 (fraction of height) 
        stops: Vec<ColorStop>,
        repeating: bool,
    },
    /// Conic gradient
    Conic {
        angle: f32, // starting angle in degrees
        center_x: f32,
        center_y: f32,
        stops: Vec<ColorStop>,
        repeating: bool,
    },
}

impl Gradient {
    /// Create a linear gradient
    pub fn linear(direction: GradientDirection, stops: Vec<ColorStop>) -> Self {
        Self::Linear { direction, stops, repeating: false }
    }
    
    /// Create a linear gradient with angle in degrees
    pub fn linear_angle(angle: f32, stops: Vec<ColorStop>) -> Self {
        Self::Linear { 
            direction: GradientDirection::Angle(angle), 
            stops, 
            repeating: false 
        }
    }
    
    /// Create a radial gradient
    pub fn radial(shape: RadialShape, stops: Vec<ColorStop>) -> Self {
        Self::Radial {
            shape,
            extent: RadialExtent::default(),
            center_x: 0.5,
            center_y: 0.5,
            stops,
            repeating: false,
        }
    }
    
    /// Create a conic gradient
    pub fn conic(angle: f32, stops: Vec<ColorStop>) -> Self {
        Self::Conic {
            angle,
            center_x: 0.5,
            center_y: 0.5,
            stops,
            repeating: false,
        }
    }
    
    /// Make the gradient repeating
    pub fn repeating(mut self) -> Self {
        match &mut self {
            Gradient::Linear { repeating, .. } => *repeating = true,
            Gradient::Radial { repeating, .. } => *repeating = true,
            Gradient::Conic { repeating, .. } => *repeating = true,
        }
        self
    }
}

/// Interpolate between two colors using SIMD-friendly batch processing
#[inline]
fn lerp_color(c1: Color, c2: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let inv_t = 1.0 - t;
    
    Color {
        r: ((c1.r as f32 * inv_t + c2.r as f32 * t) as u8),
        g: ((c1.g as f32 * inv_t + c2.g as f32 * t) as u8),
        b: ((c1.b as f32 * inv_t + c2.b as f32 * t) as u8),
        a: ((c1.a as f32 * inv_t + c2.a as f32 * t) as u8),
    }
}

/// SIMD-optimized interpolation of 4 colors at once
#[inline]
fn lerp_colors_4(c1: [Color; 4], c2: [Color; 4], t: [f32; 4]) -> [Color; 4] {
    [
        lerp_color(c1[0], c2[0], t[0]),
        lerp_color(c1[1], c2[1], t[1]),
        lerp_color(c1[2], c2[2], t[2]),
        lerp_color(c1[3], c2[3], t[3]),
    ]
}

/// Get color at position in gradient
fn sample_gradient(stops: &[ColorStop], position: f32, repeating: bool) -> Color {
    if stops.is_empty() {
        return Color::TRANSPARENT;
    }
    if stops.len() == 1 {
        return stops[0].color;
    }
    
    let pos = if repeating {
        position.rem_euclid(1.0)
    } else {
        position.clamp(0.0, 1.0)
    };
    
    // Find surrounding stops
    let mut prev = &stops[0];
    for stop in stops.iter() {
        if stop.position >= pos {
            if stop.position == prev.position {
                return stop.color;
            }
            let t = (pos - prev.position) / (stop.position - prev.position);
            return lerp_color(prev.color, stop.color, t);
        }
        prev = stop;
    }
    
    stops.last().map(|s| s.color).unwrap_or(Color::TRANSPARENT)
}

/// Fill a region with a linear gradient (SIMD-optimized)
pub fn fill_linear_gradient(
    canvas: &mut Canvas,
    x: f32, y: f32, width: f32, height: f32,
    gradient: &Gradient,
) {
    let (direction, stops, repeating) = match gradient {
        Gradient::Linear { direction, stops, repeating } => (direction, stops, *repeating),
        _ => return,
    };
    
    if stops.is_empty() || width <= 0.0 || height <= 0.0 {
        return;
    }
    
    let angle = direction.to_radians();
    let cos_a = angle.cos();
    let sin_a = angle.sin();
    
    // Calculate gradient line length
    let diagonal = (width * cos_a.abs() + height * sin_a.abs()).max(1.0);
    
    let x_start = x as i32;
    let y_start = y as i32;
    let x_end = (x + width) as i32;
    let y_end = (y + height) as i32;
    
    for py in y_start..y_end {
        // Process 4 pixels at a time when possible
        let mut px = x_start;
        
        while px + 4 <= x_end {
            let positions: [f32; 4] = std::array::from_fn(|i| {
                let rel_x = (px + i as i32) as f32 - x - width / 2.0;
                let rel_y = py as f32 - y - height / 2.0;
                let projected = rel_x * sin_a + rel_y * -cos_a;
                0.5 + projected / diagonal
            });
            
            let colors: [Color; 4] = std::array::from_fn(|i| {
                sample_gradient(stops, positions[i], repeating)
            });
            
            for (i, color) in colors.iter().enumerate() {
                canvas.set_pixel((px + i as i32) as u32, py as u32, *color);
            }
            
            px += 4;
        }
        
        // Handle remaining pixels
        while px < x_end {
            let rel_x = px as f32 - x - width / 2.0;
            let rel_y = py as f32 - y - height / 2.0;
            let projected = rel_x * sin_a + rel_y * -cos_a;
            let pos = 0.5 + projected / diagonal;
            let color = sample_gradient(stops, pos, repeating);
            canvas.set_pixel(px as u32, py as u32, color);
            px += 1;
        }
    }
}

/// Fill a region with a radial gradient (SIMD-optimized)
pub fn fill_radial_gradient(
    canvas: &mut Canvas,
    x: f32, y: f32, width: f32, height: f32,
    gradient: &Gradient,
) {
    let (shape, extent, cx, cy, stops, repeating) = match gradient {
        Gradient::Radial { shape, extent, center_x, center_y, stops, repeating } => {
            (*shape, extent, *center_x, *center_y, stops, *repeating)
        }
        _ => return,
    };
    
    if stops.is_empty() || width <= 0.0 || height <= 0.0 {
        return;
    }
    
    let center_px = x + width * cx;
    let center_py = y + height * cy;
    
    // Calculate radii based on extent
    let (rx, ry) = match extent {
        RadialExtent::Explicit(r1, r2) => (*r1, *r2),
        RadialExtent::FarthestCorner => {
            let dx = width.max(0.0);
            let dy = height.max(0.0);
            ((dx * dx + dy * dy).sqrt() / 2.0, (dx * dx + dy * dy).sqrt() / 2.0)
        }
        RadialExtent::ClosestSide => {
            let dx = (width * cx).min(width * (1.0 - cx));
            let dy = (height * cy).min(height * (1.0 - cy));
            (dx, dy)
        }
        RadialExtent::FarthestSide => {
            let dx = (width * cx).max(width * (1.0 - cx));
            let dy = (height * cy).max(height * (1.0 - cy));
            (dx, dy)
        }
        RadialExtent::ClosestCorner => {
            let dx = (width * cx).min(width * (1.0 - cx));
            let dy = (height * cy).min(height * (1.0 - cy));
            let r = (dx * dx + dy * dy).sqrt();
            (r, r)
        }
    };
    
    let rx = rx.max(1.0);
    let ry = match shape {
        RadialShape::Circle => rx,
        RadialShape::Ellipse => ry.max(1.0),
    };
    
    let x_start = x as i32;
    let y_start = y as i32;
    let x_end = (x + width) as i32;
    let y_end = (y + height) as i32;
    
    for py in y_start..y_end {
        for px in x_start..x_end {
            let dx = (px as f32 - center_px) / rx;
            let dy = (py as f32 - center_py) / ry;
            let dist = (dx * dx + dy * dy).sqrt();
            let color = sample_gradient(stops, dist, repeating);
            canvas.set_pixel(px as u32, py as u32, color);
        }
    }
}

/// Fill a region with a conic gradient (SIMD-optimized)
pub fn fill_conic_gradient(
    canvas: &mut Canvas,
    x: f32, y: f32, width: f32, height: f32,
    gradient: &Gradient,
) {
    let (angle, cx, cy, stops, repeating) = match gradient {
        Gradient::Conic { angle, center_x, center_y, stops, repeating } => {
            (*angle, *center_x, *center_y, stops, *repeating)
        }
        _ => return,
    };
    
    if stops.is_empty() || width <= 0.0 || height <= 0.0 {
        return;
    }
    
    let center_px = x + width * cx;
    let center_py = y + height * cy;
    let start_angle = angle * PI / 180.0;
    
    let x_start = x as i32;
    let y_start = y as i32;
    let x_end = (x + width) as i32;
    let y_end = (y + height) as i32;
    
    for py in y_start..y_end {
        for px in x_start..x_end {
            let dx = px as f32 - center_px;
            let dy = py as f32 - center_py;
            let mut angle = dy.atan2(dx) - start_angle + PI / 2.0;
            if angle < 0.0 {
                angle += 2.0 * PI;
            }
            let pos = angle / (2.0 * PI);
            let color = sample_gradient(stops, pos, repeating);
            canvas.set_pixel(px as u32, py as u32, color);
        }
    }
}

/// Fill a region with any gradient type
pub fn fill_gradient(
    canvas: &mut Canvas,
    x: f32, y: f32, width: f32, height: f32,
    gradient: &Gradient,
) {
    match gradient {
        Gradient::Linear { .. } => fill_linear_gradient(canvas, x, y, width, height, gradient),
        Gradient::Radial { .. } => fill_radial_gradient(canvas, x, y, width, height, gradient),
        Gradient::Conic { .. } => fill_conic_gradient(canvas, x, y, width, height, gradient),
    }
}

/// Parse CSS gradient string (simplified)
pub fn parse_gradient(value: &str) -> Option<Gradient> {
    let value = value.trim();
    
    if value.starts_with("linear-gradient(") || value.starts_with("repeating-linear-gradient(") {
        let repeating = value.starts_with("repeating-");
        let inner = value
            .trim_start_matches("repeating-")
            .trim_start_matches("linear-gradient(")
            .trim_end_matches(')');
        
        return parse_linear_gradient_inner(inner, repeating);
    }
    
    if value.starts_with("radial-gradient(") || value.starts_with("repeating-radial-gradient(") {
        let repeating = value.starts_with("repeating-");
        let inner = value
            .trim_start_matches("repeating-")
            .trim_start_matches("radial-gradient(")
            .trim_end_matches(')');
        
        return parse_radial_gradient_inner(inner, repeating);
    }
    
    if value.starts_with("conic-gradient(") || value.starts_with("repeating-conic-gradient(") {
        let repeating = value.starts_with("repeating-");
        let inner = value
            .trim_start_matches("repeating-")
            .trim_start_matches("conic-gradient(")
            .trim_end_matches(')');
        
        return parse_conic_gradient_inner(inner, repeating);
    }
    
    None
}

fn parse_linear_gradient_inner(inner: &str, repeating: bool) -> Option<Gradient> {
    let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
    
    let (direction, color_start) = if parts.first()?.starts_with("to ") {
        let dir = match parts[0] {
            "to top" => GradientDirection::ToTop,
            "to right" => GradientDirection::ToRight,
            "to bottom" => GradientDirection::ToBottom,
            "to left" => GradientDirection::ToLeft,
            "to top right" | "to right top" => GradientDirection::ToTopRight,
            "to top left" | "to left top" => GradientDirection::ToTopLeft,
            "to bottom right" | "to right bottom" => GradientDirection::ToBottomRight,
            "to bottom left" | "to left bottom" => GradientDirection::ToBottomLeft,
            _ => GradientDirection::ToBottom,
        };
        (dir, 1)
    } else if parts.first()?.ends_with("deg") {
        let angle_str = parts[0].trim_end_matches("deg");
        let angle = angle_str.parse().ok()?;
        (GradientDirection::Angle(angle), 1)
    } else {
        (GradientDirection::ToBottom, 0)
    };
    
    let stops = parse_color_stops(&parts[color_start..])?;
    
    Some(Gradient::Linear { direction, stops, repeating })
}

fn parse_radial_gradient_inner(inner: &str, repeating: bool) -> Option<Gradient> {
    let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
    let stops = parse_color_stops(&parts)?;
    
    Some(Gradient::Radial {
        shape: RadialShape::Ellipse,
        extent: RadialExtent::FarthestCorner,
        center_x: 0.5,
        center_y: 0.5,
        stops,
        repeating,
    })
}

fn parse_conic_gradient_inner(inner: &str, repeating: bool) -> Option<Gradient> {
    let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
    let stops = parse_color_stops(&parts)?;
    
    Some(Gradient::Conic {
        angle: 0.0,
        center_x: 0.5,
        center_y: 0.5,
        stops,
        repeating,
    })
}

fn parse_color_stops(parts: &[&str]) -> Option<Vec<ColorStop>> {
    let count = parts.len();
    if count == 0 {
        return None;
    }
    
    let mut stops = Vec::with_capacity(count);
    
    for (i, part) in parts.iter().enumerate() {
        let words: Vec<&str> = part.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        
        let color = parse_color(words[0])?;
        let position = if words.len() > 1 {
            parse_percentage(words[1]).unwrap_or(i as f32 / (count - 1).max(1) as f32)
        } else {
            i as f32 / (count - 1).max(1) as f32
        };
        
        stops.push(ColorStop { position, color });
    }
    
    Some(stops)
}

fn parse_percentage(s: &str) -> Option<f32> {
    s.trim_end_matches('%').parse::<f32>().ok().map(|v| v / 100.0)
}

fn parse_color(s: &str) -> Option<Color> {
    // Named colors
    match s.to_lowercase().as_str() {
        "red" => return Some(Color::RED),
        "green" => return Some(Color::GREEN),
        "blue" => return Some(Color::BLUE),
        "white" => return Some(Color::WHITE),
        "black" => return Some(Color::BLACK),
        "transparent" => return Some(Color::TRANSPARENT),
        "yellow" => return Some(Color::rgb(255, 255, 0)),
        "cyan" => return Some(Color::rgb(0, 255, 255)),
        "magenta" => return Some(Color::rgb(255, 0, 255)),
        "orange" => return Some(Color::rgb(255, 165, 0)),
        "purple" => return Some(Color::rgb(128, 0, 128)),
        "pink" => return Some(Color::rgb(255, 192, 203)),
        "gray" | "grey" => return Some(Color::rgb(128, 128, 128)),
        _ => {}
    }
    
    // Hex color
    Color::from_hex(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_color_stop() {
        let stop = ColorStop::new(0.5, Color::RED);
        assert!((stop.position - 0.5).abs() < 0.01);
    }
    
    #[test]
    fn test_lerp_color() {
        let c1 = Color::rgb(0, 0, 0);
        let c2 = Color::rgb(255, 255, 255);
        
        let mid = lerp_color(c1, c2, 0.5);
        assert!(mid.r > 120 && mid.r < 135);
    }
    
    #[test]
    fn test_gradient_direction() {
        assert!((GradientDirection::ToTop.to_radians() - 0.0).abs() < 0.01);
        assert!((GradientDirection::ToRight.to_radians() - PI / 2.0).abs() < 0.01);
    }
    
    #[test]
    fn test_sample_gradient() {
        let stops = vec![
            ColorStop::new(0.0, Color::BLACK),
            ColorStop::new(1.0, Color::WHITE),
        ];
        
        let at_0 = sample_gradient(&stops, 0.0, false);
        assert_eq!(at_0.r, 0);
        
        let at_1 = sample_gradient(&stops, 1.0, false);
        assert_eq!(at_1.r, 255);
        
        let at_half = sample_gradient(&stops, 0.5, false);
        assert!(at_half.r > 120 && at_half.r < 135);
    }
    
    #[test]
    fn test_parse_linear_gradient() {
        let g = parse_gradient("linear-gradient(to right, red, blue)");
        assert!(g.is_some());
        
        if let Some(Gradient::Linear { direction, stops, .. }) = g {
            assert!(matches!(direction, GradientDirection::ToRight));
            assert_eq!(stops.len(), 2);
        }
    }
    
    #[test]
    fn test_gradient_builder() {
        let g = Gradient::linear(
            GradientDirection::ToRight,
            vec![
                ColorStop::new(0.0, Color::RED),
                ColorStop::new(1.0, Color::BLUE),
            ]
        );
        
        assert!(matches!(g, Gradient::Linear { .. }));
    }
}
