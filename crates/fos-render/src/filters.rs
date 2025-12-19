//! CSS Filter Effects
//!
//! Implements CSS filter and backdrop-filter properties.

use crate::{Canvas, Color};

/// CSS filter function
#[derive(Debug, Clone, PartialEq)]
pub enum FilterFunction {
    /// blur(radius) - Gaussian blur
    Blur(f32),
    /// brightness(amount) - 0 = black, 1 = normal, >1 = brighter
    Brightness(f32),
    /// contrast(amount) - 0 = gray, 1 = normal
    Contrast(f32),
    /// grayscale(amount) - 0 = normal, 1 = fully gray
    Grayscale(f32),
    /// hue-rotate(angle) - degrees
    HueRotate(f32),
    /// invert(amount) - 0 = normal, 1 = fully inverted
    Invert(f32),
    /// opacity(amount) - 0 = transparent, 1 = opaque
    Opacity(f32),
    /// saturate(amount) - 0 = desaturated, 1 = normal, >1 = over-saturated
    Saturate(f32),
    /// sepia(amount) - 0 = normal, 1 = fully sepia
    Sepia(f32),
    /// drop-shadow(x, y, blur, color)
    DropShadow(f32, f32, f32, Color),
}

/// A list of filter functions to apply
#[derive(Debug, Clone, Default)]
pub struct FilterList {
    pub filters: Vec<FilterFunction>,
}

impl FilterList {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn push(mut self, filter: FilterFunction) -> Self {
        self.filters.push(filter);
        self
    }
    
    pub fn blur(self, radius: f32) -> Self {
        self.push(FilterFunction::Blur(radius))
    }
    
    pub fn brightness(self, amount: f32) -> Self {
        self.push(FilterFunction::Brightness(amount))
    }
    
    pub fn grayscale(self, amount: f32) -> Self {
        self.push(FilterFunction::Grayscale(amount.clamp(0.0, 1.0)))
    }
    
    pub fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }
}

/// Apply filters to a region of the canvas
pub fn apply_filters(
    canvas: &mut Canvas,
    x: f32, y: f32, width: f32, height: f32,
    filters: &FilterList,
) {
    for filter in &filters.filters {
        match filter {
            FilterFunction::Blur(radius) => apply_blur(canvas, x, y, width, height, *radius),
            FilterFunction::Brightness(amount) => apply_brightness(canvas, x, y, width, height, *amount),
            FilterFunction::Contrast(amount) => apply_contrast(canvas, x, y, width, height, *amount),
            FilterFunction::Grayscale(amount) => apply_grayscale(canvas, x, y, width, height, *amount),
            FilterFunction::HueRotate(angle) => apply_hue_rotate(canvas, x, y, width, height, *angle),
            FilterFunction::Invert(amount) => apply_invert(canvas, x, y, width, height, *amount),
            FilterFunction::Opacity(amount) => apply_filter_opacity(canvas, x, y, width, height, *amount),
            FilterFunction::Saturate(amount) => apply_saturate(canvas, x, y, width, height, *amount),
            FilterFunction::Sepia(amount) => apply_sepia(canvas, x, y, width, height, *amount),
            FilterFunction::DropShadow(dx, dy, blur, color) => {
                apply_drop_shadow(canvas, x, y, width, height, *dx, *dy, *blur, *color);
            }
        }
    }
}

/// Box blur approximation (fast)
fn apply_blur(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, radius: f32) {
    if radius <= 0.0 {
        return;
    }
    
    let x_start = x.max(0.0) as u32;
    let y_start = y.max(0.0) as u32;
    let x_end = ((x + width) as u32).min(canvas.width());
    let y_end = ((y + height) as u32).min(canvas.height());
    
    let kernel_size = (radius as i32 * 2 + 1).min(15) as usize;
    let half = kernel_size as i32 / 2;
    
    // Horizontal pass (store in temporary buffer)
    let mut temp: Vec<Color> = Vec::with_capacity((x_end - x_start) as usize * (y_end - y_start) as usize);
    
    for py in y_start..y_end {
        for px in x_start..x_end {
            let mut r_sum = 0u32;
            let mut g_sum = 0u32;
            let mut b_sum = 0u32;
            let mut a_sum = 0u32;
            let mut count = 0u32;
            
            for kx in -half..=half {
                let sx = (px as i32 + kx).max(x_start as i32).min(x_end as i32 - 1) as u32;
                if let Some(pixel) = canvas.get_pixel(sx, py) {
                    r_sum += pixel.r as u32;
                    g_sum += pixel.g as u32;
                    b_sum += pixel.b as u32;
                    a_sum += pixel.a as u32;
                    count += 1;
                }
            }
            
            if count > 0 {
                temp.push(Color::rgba(
                    (r_sum / count) as u8,
                    (g_sum / count) as u8,
                    (b_sum / count) as u8,
                    (a_sum / count) as u8,
                ));
            } else {
                temp.push(Color::TRANSPARENT);
            }
        }
    }
    
    // Write back with vertical blur
    let row_width = (x_end - x_start) as usize;
    for py in y_start..y_end {
        for px in x_start..x_end {
            let mut r_sum = 0u32;
            let mut g_sum = 0u32;
            let mut b_sum = 0u32;
            let mut a_sum = 0u32;
            let mut count = 0u32;
            
            for ky in -half..=half {
                let sy = (py as i32 + ky).max(y_start as i32).min(y_end as i32 - 1) as u32;
                let idx = (sy - y_start) as usize * row_width + (px - x_start) as usize;
                if idx < temp.len() {
                    let pixel = temp[idx];
                    r_sum += pixel.r as u32;
                    g_sum += pixel.g as u32;
                    b_sum += pixel.b as u32;
                    a_sum += pixel.a as u32;
                    count += 1;
                }
            }
            
            if count > 0 {
                canvas.set_pixel(px, py, Color::rgba(
                    (r_sum / count) as u8,
                    (g_sum / count) as u8,
                    (b_sum / count) as u8,
                    (a_sum / count) as u8,
                ));
            }
        }
    }
}

fn apply_brightness(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, amount: f32) {
    apply_per_pixel(canvas, x, y, width, height, |c| {
        Color::rgba(
            (c.r as f32 * amount).min(255.0) as u8,
            (c.g as f32 * amount).min(255.0) as u8,
            (c.b as f32 * amount).min(255.0) as u8,
            c.a,
        )
    });
}

fn apply_contrast(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, amount: f32) {
    apply_per_pixel(canvas, x, y, width, height, |c| {
        let adjust = |v: u8| -> u8 {
            let f = (v as f32 / 255.0 - 0.5) * amount + 0.5;
            (f * 255.0).clamp(0.0, 255.0) as u8
        };
        Color::rgba(adjust(c.r), adjust(c.g), adjust(c.b), c.a)
    });
}

fn apply_grayscale(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, amount: f32) {
    apply_per_pixel(canvas, x, y, width, height, |c| {
        let gray = (c.r as f32 * 0.299 + c.g as f32 * 0.587 + c.b as f32 * 0.114) as u8;
        let blend = |orig: u8| -> u8 {
            (orig as f32 * (1.0 - amount) + gray as f32 * amount) as u8
        };
        Color::rgba(blend(c.r), blend(c.g), blend(c.b), c.a)
    });
}

fn apply_hue_rotate(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, angle: f32) {
    let rad = angle * std::f32::consts::PI / 180.0;
    let cos = rad.cos();
    let sin = rad.sin();
    
    apply_per_pixel(canvas, x, y, width, height, |c| {
        // Simplified hue rotation matrix
        let r = c.r as f32 / 255.0;
        let g = c.g as f32 / 255.0;
        let b = c.b as f32 / 255.0;
        
        let nr = (0.213 + 0.787 * cos - 0.213 * sin) * r
               + (0.715 - 0.715 * cos - 0.715 * sin) * g
               + (0.072 - 0.072 * cos + 0.928 * sin) * b;
        let ng = (0.213 - 0.213 * cos + 0.143 * sin) * r
               + (0.715 + 0.285 * cos + 0.140 * sin) * g
               + (0.072 - 0.072 * cos - 0.283 * sin) * b;
        let nb = (0.213 - 0.213 * cos - 0.787 * sin) * r
               + (0.715 - 0.715 * cos + 0.715 * sin) * g
               + (0.072 + 0.928 * cos + 0.072 * sin) * b;
        
        Color::rgba(
            (nr * 255.0).clamp(0.0, 255.0) as u8,
            (ng * 255.0).clamp(0.0, 255.0) as u8,
            (nb * 255.0).clamp(0.0, 255.0) as u8,
            c.a,
        )
    });
}

fn apply_invert(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, amount: f32) {
    apply_per_pixel(canvas, x, y, width, height, |c| {
        let invert = |v: u8| -> u8 {
            let inv = 255 - v;
            (v as f32 * (1.0 - amount) + inv as f32 * amount) as u8
        };
        Color::rgba(invert(c.r), invert(c.g), invert(c.b), c.a)
    });
}

fn apply_filter_opacity(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, amount: f32) {
    apply_per_pixel(canvas, x, y, width, height, |c| {
        Color::rgba(c.r, c.g, c.b, (c.a as f32 * amount) as u8)
    });
}

fn apply_saturate(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, amount: f32) {
    apply_per_pixel(canvas, x, y, width, height, |c| {
        let gray = (c.r as f32 * 0.299 + c.g as f32 * 0.587 + c.b as f32 * 0.114);
        let saturate = |v: u8| -> u8 {
            (gray + (v as f32 - gray) * amount).clamp(0.0, 255.0) as u8
        };
        Color::rgba(saturate(c.r), saturate(c.g), saturate(c.b), c.a)
    });
}

fn apply_sepia(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, amount: f32) {
    apply_per_pixel(canvas, x, y, width, height, |c| {
        let r = c.r as f32 / 255.0;
        let g = c.g as f32 / 255.0;
        let b = c.b as f32 / 255.0;
        
        let sr = 0.393 * r + 0.769 * g + 0.189 * b;
        let sg = 0.349 * r + 0.686 * g + 0.168 * b;
        let sb = 0.272 * r + 0.534 * g + 0.131 * b;
        
        Color::rgba(
            ((r * (1.0 - amount) + sr * amount) * 255.0).clamp(0.0, 255.0) as u8,
            ((g * (1.0 - amount) + sg * amount) * 255.0).clamp(0.0, 255.0) as u8,
            ((b * (1.0 - amount) + sb * amount) * 255.0).clamp(0.0, 255.0) as u8,
            c.a,
        )
    });
}

fn apply_drop_shadow(
    canvas: &mut Canvas,
    x: f32, y: f32, width: f32, height: f32,
    dx: f32, dy: f32, blur: f32, color: Color,
) {
    // Drop shadow is handled separately (before content, like box-shadow)
    // This is a simplified implementation
    let shadow_x = x + dx;
    let shadow_y = y + dy;
    
    // Fill shadow area
    let x_start = shadow_x.max(0.0) as u32;
    let y_start = shadow_y.max(0.0) as u32;
    let x_end = ((shadow_x + width) as u32).min(canvas.width());
    let y_end = ((shadow_y + height) as u32).min(canvas.height());
    
    for py in y_start..y_end {
        for px in x_start..x_end {
            if let Some(bg) = canvas.get_pixel(px, py) {
                let alpha = color.a as f32 / 255.0;
                let blended = Color::rgba(
                    (bg.r as f32 * (1.0 - alpha) + color.r as f32 * alpha) as u8,
                    (bg.g as f32 * (1.0 - alpha) + color.g as f32 * alpha) as u8,
                    (bg.b as f32 * (1.0 - alpha) + color.b as f32 * alpha) as u8,
                    255,
                );
                canvas.set_pixel(px, py, blended);
            }
        }
    }
    
    if blur > 0.0 {
        apply_blur(canvas, shadow_x, shadow_y, width, height, blur);
    }
}

/// Helper to apply a per-pixel transformation
fn apply_per_pixel<F>(canvas: &mut Canvas, x: f32, y: f32, width: f32, height: f32, transform: F)
where
    F: Fn(Color) -> Color,
{
    let x_start = x.max(0.0) as u32;
    let y_start = y.max(0.0) as u32;
    let x_end = ((x + width) as u32).min(canvas.width());
    let y_end = ((y + height) as u32).min(canvas.height());
    
    for py in y_start..y_end {
        for px in x_start..x_end {
            if let Some(pixel) = canvas.get_pixel(px, py) {
                canvas.set_pixel(px, py, transform(pixel));
            }
        }
    }
}

/// Blend mode for mix-blend-mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
}

impl BlendMode {
    /// Blend two colors
    pub fn blend(&self, base: Color, top: Color) -> Color {
        let ba = base.a as f32 / 255.0;
        let ta = top.a as f32 / 255.0;
        
        if ta == 0.0 {
            return base;
        }
        
        let blend_channel = |b: u8, t: u8| -> u8 {
            let bf = b as f32 / 255.0;
            let tf = t as f32 / 255.0;
            
            let result = match self {
                BlendMode::Normal => tf,
                BlendMode::Multiply => bf * tf,
                BlendMode::Screen => 1.0 - (1.0 - bf) * (1.0 - tf),
                BlendMode::Overlay => {
                    if bf < 0.5 { 2.0 * bf * tf } else { 1.0 - 2.0 * (1.0 - bf) * (1.0 - tf) }
                }
                BlendMode::Darken => bf.min(tf),
                BlendMode::Lighten => bf.max(tf),
                BlendMode::ColorDodge => {
                    if tf >= 1.0 { 1.0 } else { (bf / (1.0 - tf)).min(1.0) }
                }
                BlendMode::ColorBurn => {
                    if tf <= 0.0 { 0.0 } else { 1.0 - ((1.0 - bf) / tf).min(1.0) }
                }
                BlendMode::HardLight => {
                    if tf < 0.5 { 2.0 * bf * tf } else { 1.0 - 2.0 * (1.0 - bf) * (1.0 - tf) }
                }
                BlendMode::SoftLight => {
                    if tf < 0.5 {
                        bf - (1.0 - 2.0 * tf) * bf * (1.0 - bf)
                    } else {
                        bf + (2.0 * tf - 1.0) * (if bf < 0.25 {
                            ((16.0 * bf - 12.0) * bf + 4.0) * bf
                        } else {
                            bf.sqrt()
                        } - bf)
                    }
                }
                BlendMode::Difference => (bf - tf).abs(),
                BlendMode::Exclusion => bf + tf - 2.0 * bf * tf,
            };
            
            // Alpha composite
            let composited = result * ta + bf * ba * (1.0 - ta);
            (composited * 255.0).clamp(0.0, 255.0) as u8
        };
        
        Color::rgba(
            blend_channel(base.r, top.r),
            blend_channel(base.g, top.g),
            blend_channel(base.b, top.b),
            ((ba + ta * (1.0 - ba)) * 255.0) as u8,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_filter_list() {
        let filters = FilterList::new()
            .blur(5.0)
            .grayscale(1.0);
        assert_eq!(filters.filters.len(), 2);
    }
    
    #[test]
    fn test_blend_multiply() {
        let mode = BlendMode::Multiply;
        let white = Color::WHITE;
        let gray = Color::rgb(128, 128, 128);
        
        let result = mode.blend(white, gray);
        // Multiply: white * gray = gray
        assert!(result.r > 120 && result.r < 136);
    }
    
    #[test]
    fn test_blend_screen() {
        let mode = BlendMode::Screen;
        let black = Color::BLACK;
        let gray = Color::rgb(128, 128, 128);
        
        let result = mode.blend(black, gray);
        // Screen: black screened with gray = gray
        assert!(result.r > 120 && result.r < 136);
    }
}
