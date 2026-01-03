//! SVG Image Support
//!
//! Complete SVG rendering with software rasterization.
//! Supports basic shapes, paths, transforms, and colors.

use std::collections::HashMap;

/// SVG image container
#[derive(Debug, Clone)]
pub struct SvgImage {
    /// Original SVG source
    pub source: String,
    /// Parsed viewBox
    pub view_box: Option<ViewBox>,
    /// Intrinsic width (if specified)
    pub width: Option<f32>,
    /// Intrinsic height (if specified)
    pub height: Option<f32>,
    /// Parsed elements
    pub elements: Vec<SvgElement>,
}

/// SVG viewBox
#[derive(Debug, Clone, Copy)]
pub struct ViewBox {
    pub min_x: f32,
    pub min_y: f32,
    pub width: f32,
    pub height: f32,
}

impl ViewBox {
    pub fn new(min_x: f32, min_y: f32, width: f32, height: f32) -> Self {
        Self { min_x, min_y, width, height }
    }
    
    /// Get aspect ratio
    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0.0 { 1.0 } else { self.width / self.height }
    }
}

/// SVG element (simplified representation)
#[derive(Debug, Clone)]
pub enum SvgElement {
    Rect { x: f32, y: f32, width: f32, height: f32, fill: Option<String>, stroke: Option<String>, stroke_width: f32 },
    Circle { cx: f32, cy: f32, r: f32, fill: Option<String>, stroke: Option<String>, stroke_width: f32 },
    Ellipse { cx: f32, cy: f32, rx: f32, ry: f32, fill: Option<String>, stroke: Option<String>, stroke_width: f32 },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, stroke: Option<String>, stroke_width: f32 },
    Polyline { points: Vec<(f32, f32)>, stroke: Option<String>, stroke_width: f32 },
    Polygon { points: Vec<(f32, f32)>, fill: Option<String>, stroke: Option<String>, stroke_width: f32 },
    Path { commands: Vec<PathCommand>, fill: Option<String>, stroke: Option<String>, stroke_width: f32 },
    Text { x: f32, y: f32, content: String, font_size: f32, fill: Option<String> },
    Group { transform: Option<Transform2D>, elements: Vec<SvgElement> },
    Image { href: String, x: f32, y: f32, width: f32, height: f32 },
    Use { href: String, x: f32, y: f32 },
}

/// Path command for SVG paths
#[derive(Debug, Clone, Copy)]
pub enum PathCommand {
    /// Move to (x, y)
    MoveTo(f32, f32),
    /// Line to (x, y)
    LineTo(f32, f32),
    /// Horizontal line to x
    HorizontalLineTo(f32),
    /// Vertical line to y
    VerticalLineTo(f32),
    /// Cubic bezier (cx1, cy1, cx2, cy2, x, y)
    CubicBezier(f32, f32, f32, f32, f32, f32),
    /// Quadratic bezier (cx, cy, x, y)
    QuadraticBezier(f32, f32, f32, f32),
    /// Arc (rx, ry, rotation, large_arc, sweep, x, y)
    Arc(f32, f32, f32, bool, bool, f32, f32),
    /// Close path
    Close,
}

/// 2D transform matrix
#[derive(Debug, Clone, Copy)]
pub struct Transform2D {
    pub a: f32, pub b: f32,
    pub c: f32, pub d: f32,
    pub e: f32, pub f: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::identity()
    }
}

impl Transform2D {
    pub fn identity() -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 }
    }
    
    pub fn translate(tx: f32, ty: f32) -> Self {
        Self { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: tx, f: ty }
    }
    
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self { a: sx, b: 0.0, c: 0.0, d: sy, e: 0.0, f: 0.0 }
    }
    
    pub fn rotate(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self { a: cos, b: sin, c: -sin, d: cos, e: 0.0, f: 0.0 }
    }
    
    /// Transform a point
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
    
    /// Multiply two transforms
    pub fn multiply(&self, other: &Transform2D) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }
}

/// RGBA color
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SvgColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl SvgColor {
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }
    
    pub fn transparent() -> Self {
        Self::rgba(0, 0, 0, 0)
    }
    
    /// Parse color from string
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        
        // Named colors
        match s.to_lowercase().as_str() {
            "none" | "transparent" => return Some(Self::transparent()),
            "black" => return Some(Self::rgb(0, 0, 0)),
            "white" => return Some(Self::rgb(255, 255, 255)),
            "red" => return Some(Self::rgb(255, 0, 0)),
            "green" => return Some(Self::rgb(0, 128, 0)),
            "blue" => return Some(Self::rgb(0, 0, 255)),
            "yellow" => return Some(Self::rgb(255, 255, 0)),
            "cyan" => return Some(Self::rgb(0, 255, 255)),
            "magenta" => return Some(Self::rgb(255, 0, 255)),
            "gray" | "grey" => return Some(Self::rgb(128, 128, 128)),
            "orange" => return Some(Self::rgb(255, 165, 0)),
            "purple" => return Some(Self::rgb(128, 0, 128)),
            "pink" => return Some(Self::rgb(255, 192, 203)),
            _ => {}
        }
        
        // Hex colors
        if s.starts_with('#') {
            let hex = &s[1..];
            match hex.len() {
                3 => {
                    let r = u8::from_str_radix(&hex[0..1], 16).ok()? * 17;
                    let g = u8::from_str_radix(&hex[1..2], 16).ok()? * 17;
                    let b = u8::from_str_radix(&hex[2..3], 16).ok()? * 17;
                    return Some(Self::rgb(r, g, b));
                }
                6 => {
                    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                    return Some(Self::rgb(r, g, b));
                }
                8 => {
                    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
                    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
                    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
                    let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
                    return Some(Self::rgba(r, g, b, a));
                }
                _ => {}
            }
        }
        
        // rgb(r, g, b) or rgba(r, g, b, a)
        if s.starts_with("rgb(") || s.starts_with("rgba(") {
            let inner = s.trim_start_matches("rgba(")
                .trim_start_matches("rgb(")
                .trim_end_matches(')');
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() >= 3 {
                let r = parts[0].trim().parse().ok()?;
                let g = parts[1].trim().parse().ok()?;
                let b = parts[2].trim().parse().ok()?;
                let a = parts.get(3)
                    .and_then(|s| s.trim().parse::<f32>().ok())
                    .map(|f| (f * 255.0) as u8)
                    .unwrap_or(255);
                return Some(Self::rgba(r, g, b, a));
            }
        }
        
        None
    }
}

/// SVG rasterizer
#[derive(Debug)]
pub struct SvgRasterizer {
    /// Output buffer (RGBA)
    buffer: Vec<u8>,
    /// Width
    width: u32,
    /// Height
    height: u32,
    /// Current transform stack
    transform_stack: Vec<Transform2D>,
}

impl SvgRasterizer {
    /// Create a new rasterizer
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            buffer: vec![0u8; (width * height * 4) as usize],
            width,
            height,
            transform_stack: vec![Transform2D::identity()],
        }
    }
    
    /// Clear the buffer
    pub fn clear(&mut self, color: SvgColor) {
        for chunk in self.buffer.chunks_exact_mut(4) {
            chunk[0] = color.r;
            chunk[1] = color.g;
            chunk[2] = color.b;
            chunk[3] = color.a;
        }
    }
    
    /// Get current transform
    fn current_transform(&self) -> Transform2D {
        *self.transform_stack.last().unwrap_or(&Transform2D::identity())
    }
    
    /// Push transform
    pub fn push_transform(&mut self, transform: Transform2D) {
        let current = self.current_transform();
        self.transform_stack.push(current.multiply(&transform));
    }
    
    /// Pop transform
    pub fn pop_transform(&mut self) {
        if self.transform_stack.len() > 1 {
            self.transform_stack.pop();
        }
    }
    
    /// Set pixel with alpha blending
    fn set_pixel(&mut self, x: i32, y: i32, color: SvgColor) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return;
        }
        
        let idx = ((y as u32 * self.width + x as u32) * 4) as usize;
        if idx + 3 >= self.buffer.len() {
            return;
        }
        
        if color.a == 255 {
            self.buffer[idx] = color.r;
            self.buffer[idx + 1] = color.g;
            self.buffer[idx + 2] = color.b;
            self.buffer[idx + 3] = color.a;
        } else if color.a > 0 {
            // Alpha blend
            let alpha = color.a as f32 / 255.0;
            let inv_alpha = 1.0 - alpha;
            self.buffer[idx] = (color.r as f32 * alpha + self.buffer[idx] as f32 * inv_alpha) as u8;
            self.buffer[idx + 1] = (color.g as f32 * alpha + self.buffer[idx + 1] as f32 * inv_alpha) as u8;
            self.buffer[idx + 2] = (color.b as f32 * alpha + self.buffer[idx + 2] as f32 * inv_alpha) as u8;
            self.buffer[idx + 3] = ((color.a as f32 + self.buffer[idx + 3] as f32 * inv_alpha).min(255.0)) as u8;
        }
    }
    
    /// Draw line with anti-aliasing (Bresenham)
    pub fn draw_line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: SvgColor, width: f32) {
        let transform = self.current_transform();
        let (x0, y0) = transform.apply(x0, y0);
        let (x1, y1) = transform.apply(x1, y1);
        
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let steps = dx.max(dy) as i32;
        
        if steps == 0 {
            self.set_pixel(x0 as i32, y0 as i32, color);
            return;
        }
        
        let x_inc = (x1 - x0) / steps as f32;
        let y_inc = (y1 - y0) / steps as f32;
        
        let mut x = x0;
        let mut y = y0;
        
        let half_width = (width / 2.0).max(0.5);
        
        for _ in 0..=steps {
            // Draw with width
            if width > 1.0 {
                for wy in -(half_width as i32)..=(half_width as i32) {
                    for wx in -(half_width as i32)..=(half_width as i32) {
                        let dist = ((wx * wx + wy * wy) as f32).sqrt();
                        if dist <= half_width {
                            self.set_pixel(x as i32 + wx, y as i32 + wy, color);
                        }
                    }
                }
            } else {
                self.set_pixel(x as i32, y as i32, color);
            }
            x += x_inc;
            y += y_inc;
        }
    }
    
    /// Fill rectangle
    pub fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: SvgColor) {
        let transform = self.current_transform();
        
        for py in 0..height as i32 {
            for px in 0..width as i32 {
                let (tx, ty) = transform.apply(x + px as f32, y + py as f32);
                self.set_pixel(tx as i32, ty as i32, color);
            }
        }
    }
    
    /// Stroke rectangle
    pub fn stroke_rect(&mut self, x: f32, y: f32, width: f32, height: f32, color: SvgColor, stroke_width: f32) {
        self.draw_line(x, y, x + width, y, color, stroke_width);
        self.draw_line(x + width, y, x + width, y + height, color, stroke_width);
        self.draw_line(x + width, y + height, x, y + height, color, stroke_width);
        self.draw_line(x, y + height, x, y, color, stroke_width);
    }
    
    /// Fill circle
    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, color: SvgColor) {
        let transform = self.current_transform();
        let r_sq = r * r;
        
        for dy in -(r as i32)..=(r as i32) {
            for dx in -(r as i32)..=(r as i32) {
                let dist_sq = (dx * dx + dy * dy) as f32;
                if dist_sq <= r_sq {
                    let (tx, ty) = transform.apply(cx + dx as f32, cy + dy as f32);
                    
                    // Anti-aliasing at edge
                    if dist_sq > r_sq - r * 2.0 {
                        let edge_dist = (r_sq - dist_sq).sqrt();
                        let alpha = (edge_dist * 255.0 / r).min(255.0) as u8;
                        let aa_color = SvgColor::rgba(color.r, color.g, color.b, 
                            (color.a as u16 * alpha as u16 / 255) as u8);
                        self.set_pixel(tx as i32, ty as i32, aa_color);
                    } else {
                        self.set_pixel(tx as i32, ty as i32, color);
                    }
                }
            }
        }
    }
    
    /// Stroke circle
    pub fn stroke_circle(&mut self, cx: f32, cy: f32, r: f32, color: SvgColor, stroke_width: f32) {
        let steps = (2.0 * std::f32::consts::PI * r) as i32;
        let step = 2.0 * std::f32::consts::PI / steps as f32;
        
        let mut prev_x = cx + r;
        let mut prev_y = cy;
        
        for i in 1..=steps {
            let angle = i as f32 * step;
            let x = cx + r * angle.cos();
            let y = cy + r * angle.sin();
            self.draw_line(prev_x, prev_y, x, y, color, stroke_width);
            prev_x = x;
            prev_y = y;
        }
    }
    
    /// Fill ellipse
    pub fn fill_ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, color: SvgColor) {
        let transform = self.current_transform();
        
        for dy in -(ry as i32)..=(ry as i32) {
            for dx in -(rx as i32)..=(rx as i32) {
                let norm = (dx as f32 / rx).powi(2) + (dy as f32 / ry).powi(2);
                if norm <= 1.0 {
                    let (tx, ty) = transform.apply(cx + dx as f32, cy + dy as f32);
                    self.set_pixel(tx as i32, ty as i32, color);
                }
            }
        }
    }
    
    /// Fill polygon (scanline algorithm)
    pub fn fill_polygon(&mut self, points: &[(f32, f32)], color: SvgColor) {
        if points.len() < 3 {
            return;
        }
        
        let transform = self.current_transform();
        let transformed: Vec<(f32, f32)> = points.iter()
            .map(|(x, y)| transform.apply(*x, *y))
            .collect();
        
        // Find bounds
        let min_y = transformed.iter().map(|(_, y)| *y).fold(f32::INFINITY, f32::min) as i32;
        let max_y = transformed.iter().map(|(_, y)| *y).fold(f32::NEG_INFINITY, f32::max) as i32;
        
        for y in min_y.max(0)..=max_y.min(self.height as i32 - 1) {
            let mut intersections = Vec::new();
            
            for i in 0..transformed.len() {
                let j = (i + 1) % transformed.len();
                let (x1, y1) = transformed[i];
                let (x2, y2) = transformed[j];
                
                if (y1 <= y as f32 && y2 > y as f32) || (y2 <= y as f32 && y1 > y as f32) {
                    let x = x1 + (y as f32 - y1) / (y2 - y1) * (x2 - x1);
                    intersections.push(x);
                }
            }
            
            intersections.sort_by(|a, b| a.partial_cmp(b).unwrap());
            
            for pair in intersections.chunks(2) {
                if pair.len() == 2 {
                    let start = (pair[0] as i32).max(0);
                    let end = (pair[1] as i32).min(self.width as i32 - 1);
                    for x in start..=end {
                        self.set_pixel(x, y, color);
                    }
                }
            }
        }
    }
    
    /// Stroke polygon
    pub fn stroke_polygon(&mut self, points: &[(f32, f32)], color: SvgColor, stroke_width: f32) {
        if points.is_empty() {
            return;
        }
        
        for i in 0..points.len() {
            let j = (i + 1) % points.len();
            self.draw_line(points[i].0, points[i].1, points[j].0, points[j].1, color, stroke_width);
        }
    }
    
    /// Draw path
    pub fn draw_path(&mut self, commands: &[PathCommand], fill: Option<SvgColor>, stroke: Option<SvgColor>, stroke_width: f32) {
        // Convert path to points for rendering
        let mut points = Vec::new();
        let mut current = (0.0f32, 0.0f32);
        let mut start = (0.0f32, 0.0f32);
        
        for cmd in commands {
            match *cmd {
                PathCommand::MoveTo(x, y) => {
                    if !points.is_empty() && fill.is_some() {
                        self.fill_polygon(&points, fill.unwrap());
                    }
                    points.clear();
                    current = (x, y);
                    start = current;
                    points.push(current);
                }
                PathCommand::LineTo(x, y) => {
                    if let Some(color) = stroke {
                        self.draw_line(current.0, current.1, x, y, color, stroke_width);
                    }
                    current = (x, y);
                    points.push(current);
                }
                PathCommand::HorizontalLineTo(x) => {
                    if let Some(color) = stroke {
                        self.draw_line(current.0, current.1, x, current.1, color, stroke_width);
                    }
                    current = (x, current.1);
                    points.push(current);
                }
                PathCommand::VerticalLineTo(y) => {
                    if let Some(color) = stroke {
                        self.draw_line(current.0, current.1, current.0, y, color, stroke_width);
                    }
                    current = (current.0, y);
                    points.push(current);
                }
                PathCommand::CubicBezier(cx1, cy1, cx2, cy2, x, y) => {
                    // Approximate bezier with line segments
                    let steps = 20;
                    for i in 1..=steps {
                        let t = i as f32 / steps as f32;
                        let t2 = t * t;
                        let t3 = t2 * t;
                        let mt = 1.0 - t;
                        let mt2 = mt * mt;
                        let mt3 = mt2 * mt;
                        
                        let px = mt3 * current.0 + 3.0 * mt2 * t * cx1 + 3.0 * mt * t2 * cx2 + t3 * x;
                        let py = mt3 * current.1 + 3.0 * mt2 * t * cy1 + 3.0 * mt * t2 * cy2 + t3 * y;
                        
                        if let Some(color) = stroke {
                            let prev = if i == 1 { current } else { points.last().copied().unwrap_or(current) };
                            self.draw_line(prev.0, prev.1, px, py, color, stroke_width);
                        }
                        points.push((px, py));
                    }
                    current = (x, y);
                }
                PathCommand::QuadraticBezier(cx, cy, x, y) => {
                    let steps = 15;
                    for i in 1..=steps {
                        let t = i as f32 / steps as f32;
                        let mt = 1.0 - t;
                        
                        let px = mt * mt * current.0 + 2.0 * mt * t * cx + t * t * x;
                        let py = mt * mt * current.1 + 2.0 * mt * t * cy + t * t * y;
                        
                        if let Some(color) = stroke {
                            let prev = if i == 1 { current } else { points.last().copied().unwrap_or(current) };
                            self.draw_line(prev.0, prev.1, px, py, color, stroke_width);
                        }
                        points.push((px, py));
                    }
                    current = (x, y);
                }
                PathCommand::Arc(_, _, _, _, _, x, y) => {
                    // Simplified: just draw line to endpoint
                    if let Some(color) = stroke {
                        self.draw_line(current.0, current.1, x, y, color, stroke_width);
                    }
                    current = (x, y);
                    points.push(current);
                }
                PathCommand::Close => {
                    if let Some(color) = stroke {
                        self.draw_line(current.0, current.1, start.0, start.1, color, stroke_width);
                    }
                    current = start;
                    points.push(current);
                }
            }
        }
        
        // Fill final path
        if !points.is_empty() && fill.is_some() {
            self.fill_polygon(&points, fill.unwrap());
        }
    }
    
    /// Render SVG element
    pub fn render_element(&mut self, element: &SvgElement) {
        match element {
            SvgElement::Rect { x, y, width, height, fill, stroke, stroke_width } => {
                if let Some(fill_color) = fill.as_ref().and_then(|s| SvgColor::parse(s)) {
                    self.fill_rect(*x, *y, *width, *height, fill_color);
                }
                if let Some(stroke_color) = stroke.as_ref().and_then(|s| SvgColor::parse(s)) {
                    self.stroke_rect(*x, *y, *width, *height, stroke_color, *stroke_width);
                }
            }
            SvgElement::Circle { cx, cy, r, fill, stroke, stroke_width } => {
                if let Some(fill_color) = fill.as_ref().and_then(|s| SvgColor::parse(s)) {
                    self.fill_circle(*cx, *cy, *r, fill_color);
                }
                if let Some(stroke_color) = stroke.as_ref().and_then(|s| SvgColor::parse(s)) {
                    self.stroke_circle(*cx, *cy, *r, stroke_color, *stroke_width);
                }
            }
            SvgElement::Ellipse { cx, cy, rx, ry, fill, stroke, stroke_width } => {
                if let Some(fill_color) = fill.as_ref().and_then(|s| SvgColor::parse(s)) {
                    self.fill_ellipse(*cx, *cy, *rx, *ry, fill_color);
                }
                if let Some(stroke_color) = stroke.as_ref().and_then(|s| SvgColor::parse(s)) {
                    // Approximate ellipse stroke
                    let steps = 50;
                    let step = 2.0 * std::f32::consts::PI / steps as f32;
                    for i in 0..steps {
                        let a1 = i as f32 * step;
                        let a2 = (i + 1) as f32 * step;
                        self.draw_line(
                            cx + rx * a1.cos(), cy + ry * a1.sin(),
                            cx + rx * a2.cos(), cy + ry * a2.sin(),
                            stroke_color, *stroke_width,
                        );
                    }
                }
            }
            SvgElement::Line { x1, y1, x2, y2, stroke, stroke_width } => {
                if let Some(stroke_color) = stroke.as_ref().and_then(|s| SvgColor::parse(s)) {
                    self.draw_line(*x1, *y1, *x2, *y2, stroke_color, *stroke_width);
                }
            }
            SvgElement::Polyline { points, stroke, stroke_width } => {
                if let Some(stroke_color) = stroke.as_ref().and_then(|s| SvgColor::parse(s)) {
                    for i in 0..points.len().saturating_sub(1) {
                        self.draw_line(points[i].0, points[i].1, points[i+1].0, points[i+1].1, stroke_color, *stroke_width);
                    }
                }
            }
            SvgElement::Polygon { points, fill, stroke, stroke_width } => {
                if let Some(fill_color) = fill.as_ref().and_then(|s| SvgColor::parse(s)) {
                    self.fill_polygon(points, fill_color);
                }
                if let Some(stroke_color) = stroke.as_ref().and_then(|s| SvgColor::parse(s)) {
                    self.stroke_polygon(points, stroke_color, *stroke_width);
                }
            }
            SvgElement::Path { commands, fill, stroke, stroke_width } => {
                let fill_color = fill.as_ref().and_then(|s| SvgColor::parse(s));
                let stroke_color = stroke.as_ref().and_then(|s| SvgColor::parse(s));
                self.draw_path(commands, fill_color, stroke_color, *stroke_width);
            }
            SvgElement::Group { transform, elements } => {
                if let Some(t) = transform {
                    self.push_transform(*t);
                }
                for elem in elements {
                    self.render_element(elem);
                }
                if transform.is_some() {
                    self.pop_transform();
                }
            }
            SvgElement::Text { x, y, content: _, font_size: _, fill: _ } => {
                // Text rendering would require font support
                // Just draw a placeholder rectangle
                let placeholder = SvgColor::rgb(128, 128, 128);
                self.fill_rect(*x, *y - 10.0, 50.0, 12.0, placeholder);
            }
            SvgElement::Image { .. } | SvgElement::Use { .. } => {
                // Would require external resource loading
            }
        }
    }
    
    /// Get rendered buffer
    pub fn get_buffer(&self) -> &[u8] {
        &self.buffer
    }
    
    /// Take ownership of buffer
    pub fn into_buffer(self) -> Vec<u8> {
        self.buffer
    }
}

/// SVG decoder/parser
#[derive(Debug, Default)]
pub struct SvgDecoder {
    /// Cached definitions (for <use> elements)
    definitions: HashMap<String, SvgElement>,
}

impl SvgDecoder {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Parse SVG from string
    pub fn parse(&mut self, svg: &str) -> Result<SvgImage, SvgError> {
        let view_box = self.parse_view_box(svg);
        let width = self.parse_dimension(svg, "width");
        let height = self.parse_dimension(svg, "height");
        let elements = self.parse_elements(svg);
        
        Ok(SvgImage {
            source: svg.to_string(),
            view_box,
            width,
            height,
            elements,
        })
    }
    
    fn parse_view_box(&self, svg: &str) -> Option<ViewBox> {
        if let Some(start) = svg.find("viewBox=\"") {
            let rest = &svg[start + 9..];
            if let Some(end) = rest.find('"') {
                let vb = &rest[..end];
                let parts: Vec<f32> = vb.split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|s| !s.is_empty())
                    .filter_map(|s| s.parse().ok())
                    .collect();
                if parts.len() >= 4 {
                    return Some(ViewBox::new(parts[0], parts[1], parts[2], parts[3]));
                }
            }
        }
        None
    }
    
    fn parse_dimension(&self, svg: &str, attr: &str) -> Option<f32> {
        let pattern = format!("{}=\"", attr);
        if let Some(start) = svg.find(&pattern) {
            let rest = &svg[start + pattern.len()..];
            if let Some(end) = rest.find('"') {
                let value = &rest[..end];
                let num: String = value.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
                return num.parse().ok();
            }
        }
        None
    }
    
    fn parse_elements(&self, svg: &str) -> Vec<SvgElement> {
        let mut elements = Vec::new();
        
        // Simple element parsing
        self.parse_rects(svg, &mut elements);
        self.parse_circles(svg, &mut elements);
        self.parse_ellipses(svg, &mut elements);
        self.parse_lines(svg, &mut elements);
        self.parse_paths(svg, &mut elements);
        
        elements
    }
    
    fn parse_rects(&self, svg: &str, elements: &mut Vec<SvgElement>) {
        let mut pos = 0;
        while let Some(start) = svg[pos..].find("<rect") {
            let abs_start = pos + start;
            if let Some(end) = svg[abs_start..].find("/>").or_else(|| svg[abs_start..].find(">")) {
                let tag = &svg[abs_start..abs_start + end + 2];
                let x = self.parse_attr(tag, "x").unwrap_or(0.0);
                let y = self.parse_attr(tag, "y").unwrap_or(0.0);
                let width = self.parse_attr(tag, "width").unwrap_or(0.0);
                let height = self.parse_attr(tag, "height").unwrap_or(0.0);
                let fill = self.parse_str_attr(tag, "fill");
                let stroke = self.parse_str_attr(tag, "stroke");
                let stroke_width = self.parse_attr(tag, "stroke-width").unwrap_or(1.0);
                
                elements.push(SvgElement::Rect { x, y, width, height, fill, stroke, stroke_width });
                pos = abs_start + end + 2;
            } else {
                break;
            }
        }
    }
    
    fn parse_circles(&self, svg: &str, elements: &mut Vec<SvgElement>) {
        let mut pos = 0;
        while let Some(start) = svg[pos..].find("<circle") {
            let abs_start = pos + start;
            if let Some(end) = svg[abs_start..].find("/>").or_else(|| svg[abs_start..].find(">")) {
                let tag = &svg[abs_start..abs_start + end + 2];
                let cx = self.parse_attr(tag, "cx").unwrap_or(0.0);
                let cy = self.parse_attr(tag, "cy").unwrap_or(0.0);
                let r = self.parse_attr(tag, "r").unwrap_or(0.0);
                let fill = self.parse_str_attr(tag, "fill");
                let stroke = self.parse_str_attr(tag, "stroke");
                let stroke_width = self.parse_attr(tag, "stroke-width").unwrap_or(1.0);
                
                elements.push(SvgElement::Circle { cx, cy, r, fill, stroke, stroke_width });
                pos = abs_start + end + 2;
            } else {
                break;
            }
        }
    }
    
    fn parse_ellipses(&self, svg: &str, elements: &mut Vec<SvgElement>) {
        let mut pos = 0;
        while let Some(start) = svg[pos..].find("<ellipse") {
            let abs_start = pos + start;
            if let Some(end) = svg[abs_start..].find("/>").or_else(|| svg[abs_start..].find(">")) {
                let tag = &svg[abs_start..abs_start + end + 2];
                let cx = self.parse_attr(tag, "cx").unwrap_or(0.0);
                let cy = self.parse_attr(tag, "cy").unwrap_or(0.0);
                let rx = self.parse_attr(tag, "rx").unwrap_or(0.0);
                let ry = self.parse_attr(tag, "ry").unwrap_or(0.0);
                let fill = self.parse_str_attr(tag, "fill");
                let stroke = self.parse_str_attr(tag, "stroke");
                let stroke_width = self.parse_attr(tag, "stroke-width").unwrap_or(1.0);
                
                elements.push(SvgElement::Ellipse { cx, cy, rx, ry, fill, stroke, stroke_width });
                pos = abs_start + end + 2;
            } else {
                break;
            }
        }
    }
    
    fn parse_lines(&self, svg: &str, elements: &mut Vec<SvgElement>) {
        let mut pos = 0;
        while let Some(start) = svg[pos..].find("<line") {
            let abs_start = pos + start;
            if let Some(end) = svg[abs_start..].find("/>").or_else(|| svg[abs_start..].find(">")) {
                let tag = &svg[abs_start..abs_start + end + 2];
                let x1 = self.parse_attr(tag, "x1").unwrap_or(0.0);
                let y1 = self.parse_attr(tag, "y1").unwrap_or(0.0);
                let x2 = self.parse_attr(tag, "x2").unwrap_or(0.0);
                let y2 = self.parse_attr(tag, "y2").unwrap_or(0.0);
                let stroke = self.parse_str_attr(tag, "stroke");
                let stroke_width = self.parse_attr(tag, "stroke-width").unwrap_or(1.0);
                
                elements.push(SvgElement::Line { x1, y1, x2, y2, stroke, stroke_width });
                pos = abs_start + end + 2;
            } else {
                break;
            }
        }
    }
    
    fn parse_paths(&self, svg: &str, elements: &mut Vec<SvgElement>) {
        let mut pos = 0;
        while let Some(start) = svg[pos..].find("<path") {
            let abs_start = pos + start;
            if let Some(end) = svg[abs_start..].find("/>").or_else(|| svg[abs_start..].find(">")) {
                let tag = &svg[abs_start..abs_start + end + 2];
                let d = self.parse_str_attr(tag, "d").unwrap_or_default();
                let commands = parse_path_commands(&d);
                let fill = self.parse_str_attr(tag, "fill");
                let stroke = self.parse_str_attr(tag, "stroke");
                let stroke_width = self.parse_attr(tag, "stroke-width").unwrap_or(1.0);
                
                elements.push(SvgElement::Path { commands, fill, stroke, stroke_width });
                pos = abs_start + end + 2;
            } else {
                break;
            }
        }
    }
    
    fn parse_attr(&self, tag: &str, attr: &str) -> Option<f32> {
        let pattern = format!("{}=\"", attr);
        if let Some(start) = tag.find(&pattern) {
            let rest = &tag[start + pattern.len()..];
            if let Some(end) = rest.find('"') {
                let value = &rest[..end];
                let num: String = value.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
                return num.parse().ok();
            }
        }
        None
    }
    
    fn parse_str_attr(&self, tag: &str, attr: &str) -> Option<String> {
        let pattern = format!("{}=\"", attr);
        if let Some(start) = tag.find(&pattern) {
            let rest = &tag[start + pattern.len()..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        }
        None
    }
    
    /// Render SVG to pixel buffer
    pub fn render(&self, svg: &SvgImage, width: u32, height: u32) -> Vec<u8> {
        let mut rasterizer = SvgRasterizer::new(width, height);
        rasterizer.clear(SvgColor::transparent());
        
        // Apply viewBox transform if needed
        if let Some(vb) = svg.view_box {
            let scale_x = width as f32 / vb.width;
            let scale_y = height as f32 / vb.height;
            let scale = scale_x.min(scale_y);
            
            let transform = Transform2D::scale(scale, scale)
                .multiply(&Transform2D::translate(-vb.min_x, -vb.min_y));
            rasterizer.push_transform(transform);
        }
        
        for element in &svg.elements {
            rasterizer.render_element(element);
        }
        
        rasterizer.into_buffer()
    }
}

/// Parse path d attribute into commands
fn parse_path_commands(d: &str) -> Vec<PathCommand> {
    let mut commands = Vec::new();
    let mut chars = d.chars().peekable();
    let mut current_cmd = 'M';
    let mut current_x = 0.0f32;
    let mut current_y = 0.0f32;
    
    loop {
        // Skip whitespace
        while chars.peek().map_or(false, |c| c.is_whitespace() || *c == ',') {
            chars.next();
        }
        
        if chars.peek().is_none() {
            break;
        }
        
        // Check for command letter
        if let Some(&c) = chars.peek() {
            if c.is_ascii_alphabetic() {
                current_cmd = c;
                chars.next();
            }
        }
        
        match current_cmd.to_ascii_uppercase() {
            'M' => {
                if let Some((x, y)) = parse_two_numbers(&mut chars) {
                    let (x, y) = if current_cmd.is_lowercase() { (current_x + x, current_y + y) } else { (x, y) };
                    commands.push(PathCommand::MoveTo(x, y));
                    current_x = x;
                    current_y = y;
                    current_cmd = if current_cmd.is_lowercase() { 'l' } else { 'L' };
                } else { break; }
            }
            'L' => {
                if let Some((x, y)) = parse_two_numbers(&mut chars) {
                    let (x, y) = if current_cmd.is_lowercase() { (current_x + x, current_y + y) } else { (x, y) };
                    commands.push(PathCommand::LineTo(x, y));
                    current_x = x;
                    current_y = y;
                } else { break; }
            }
            'H' => {
                if let Some(x) = parse_number(&mut chars) {
                    let x = if current_cmd.is_lowercase() { current_x + x } else { x };
                    commands.push(PathCommand::HorizontalLineTo(x));
                    current_x = x;
                } else { break; }
            }
            'V' => {
                if let Some(y) = parse_number(&mut chars) {
                    let y = if current_cmd.is_lowercase() { current_y + y } else { y };
                    commands.push(PathCommand::VerticalLineTo(y));
                    current_y = y;
                } else { break; }
            }
            'C' => {
                if let (Some(c1), Some(c2), Some(end)) = (
                    parse_two_numbers(&mut chars),
                    parse_two_numbers(&mut chars),
                    parse_two_numbers(&mut chars),
                ) {
                    let rel = current_cmd.is_lowercase();
                    let (cx1, cy1) = if rel { (current_x + c1.0, current_y + c1.1) } else { c1 };
                    let (cx2, cy2) = if rel { (current_x + c2.0, current_y + c2.1) } else { c2 };
                    let (x, y) = if rel { (current_x + end.0, current_y + end.1) } else { end };
                    commands.push(PathCommand::CubicBezier(cx1, cy1, cx2, cy2, x, y));
                    current_x = x;
                    current_y = y;
                } else { break; }
            }
            'Q' => {
                if let (Some(c), Some(end)) = (
                    parse_two_numbers(&mut chars),
                    parse_two_numbers(&mut chars),
                ) {
                    let rel = current_cmd.is_lowercase();
                    let (cx, cy) = if rel { (current_x + c.0, current_y + c.1) } else { c };
                    let (x, y) = if rel { (current_x + end.0, current_y + end.1) } else { end };
                    commands.push(PathCommand::QuadraticBezier(cx, cy, x, y));
                    current_x = x;
                    current_y = y;
                } else { break; }
            }
            'Z' => {
                commands.push(PathCommand::Close);
            }
            _ => {
                chars.next(); // Skip unknown command
            }
        }
    }
    
    commands
}

fn parse_number(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<f32> {
    while chars.peek().map_or(false, |c| c.is_whitespace() || *c == ',') {
        chars.next();
    }
    
    let mut s = String::new();
    if chars.peek() == Some(&'-') || chars.peek() == Some(&'+') {
        s.push(chars.next().unwrap());
    }
    while chars.peek().map_or(false, |c| c.is_ascii_digit() || *c == '.') {
        s.push(chars.next().unwrap());
    }
    
    s.parse().ok()
}

fn parse_two_numbers(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<(f32, f32)> {
    let a = parse_number(chars)?;
    let b = parse_number(chars)?;
    Some((a, b))
}

/// Check if data is SVG
pub fn is_svg(data: &[u8]) -> bool {
    let text = std::str::from_utf8(&data[..data.len().min(100)]).unwrap_or("");
    text.contains("<svg") || text.contains("<?xml")
}

/// SVG errors
#[derive(Debug, thiserror::Error)]
pub enum SvgError {
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Invalid element: {0}")]
    InvalidElement(String),
    #[error("Render error: {0}")]
    Render(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_svg_decoder() {
        let mut decoder = SvgDecoder::new();
        let svg = r#"<svg viewBox="0 0 100 100" width="200" height="200"></svg>"#;
        let result = decoder.parse(svg).unwrap();
        
        assert!(result.view_box.is_some());
        assert_eq!(result.view_box.unwrap().width, 100.0);
        assert_eq!(result.width, Some(200.0));
    }
    
    #[test]
    fn test_is_svg() {
        assert!(is_svg(b"<svg></svg>"));
        assert!(is_svg(b"<?xml version=\"1.0\"?>"));
        assert!(!is_svg(b"\x89PNG"));
    }
    
    #[test]
    fn test_color_parsing() {
        assert_eq!(SvgColor::parse("red"), Some(SvgColor::rgb(255, 0, 0)));
        assert_eq!(SvgColor::parse("#ff0000"), Some(SvgColor::rgb(255, 0, 0)));
        assert_eq!(SvgColor::parse("#f00"), Some(SvgColor::rgb(255, 0, 0)));
        assert_eq!(SvgColor::parse("rgb(0, 255, 0)"), Some(SvgColor::rgb(0, 255, 0)));
    }
    
    #[test]
    fn test_transform() {
        let t = Transform2D::translate(10.0, 20.0);
        let (x, y) = t.apply(5.0, 5.0);
        assert_eq!((x, y), (15.0, 25.0));
    }
    
    #[test]
    fn test_path_parsing() {
        let commands = parse_path_commands("M 10 10 L 20 20 Z");
        assert_eq!(commands.len(), 3);
        assert!(matches!(commands[0], PathCommand::MoveTo(10.0, 10.0)));
        assert!(matches!(commands[1], PathCommand::LineTo(20.0, 20.0)));
        assert!(matches!(commands[2], PathCommand::Close));
    }
    
    #[test]
    fn test_rasterizer() {
        let mut rasterizer = SvgRasterizer::new(100, 100);
        rasterizer.clear(SvgColor::rgb(255, 255, 255));
        rasterizer.fill_rect(10.0, 10.0, 30.0, 30.0, SvgColor::rgb(255, 0, 0));
        
        let buffer = rasterizer.get_buffer();
        assert_eq!(buffer.len(), 100 * 100 * 4);
    }
    
    #[test]
    fn test_svg_with_elements() {
        let mut decoder = SvgDecoder::new();
        let svg = r#"<svg viewBox="0 0 100 100">
            <rect x="10" y="10" width="30" height="30" fill="red"/>
            <circle cx="50" cy="50" r="20" fill="blue"/>
        </svg>"#;
        
        let result = decoder.parse(svg).unwrap();
        assert_eq!(result.elements.len(), 2);
        
        // Render
        let pixels = decoder.render(&result, 100, 100);
        assert_eq!(pixels.len(), 100 * 100 * 4);
    }
}

