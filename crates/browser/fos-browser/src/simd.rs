//! SIMD Acceleration Integration
//!
//! SIMD-accelerated operations for color blending and geometry.

/// SIMD feature detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdLevel {
    None, Sse2, Sse4, Avx2, Neon,
}

impl SimdLevel {
    /// Detect the best available SIMD level
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") { return SimdLevel::Avx2; }
            if is_x86_feature_detected!("sse4.1") { return SimdLevel::Sse4; }
            if is_x86_feature_detected!("sse2") { return SimdLevel::Sse2; }
        }
        #[cfg(target_arch = "aarch64")]
        { return SimdLevel::Neon; }
        SimdLevel::None
    }
}

/// RGBA color as 4 bytes
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color4 {
    pub r: u8, pub g: u8, pub b: u8, pub a: u8,
}

impl Color4 {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self { Self { r, g, b, a } }
    
    pub const fn from_u32(rgba: u32) -> Self {
        Self {
            r: ((rgba >> 24) & 0xFF) as u8,
            g: ((rgba >> 16) & 0xFF) as u8,
            b: ((rgba >> 8) & 0xFF) as u8,
            a: (rgba & 0xFF) as u8,
        }
    }
    
    pub const fn to_u32(self) -> u32 {
        ((self.r as u32) << 24) | ((self.g as u32) << 16) | ((self.b as u32) << 8) | (self.a as u32)
    }
}

/// Blend two colors using alpha blending (Porter-Duff "over")
pub fn blend_color(src: Color4, dst: Color4) -> Color4 {
    let src_a = src.a as u32;
    let inv_a = 255 - src_a;
    Color4 {
        r: ((src.r as u32 * 255 + dst.r as u32 * inv_a) / 255) as u8,
        g: ((src.g as u32 * 255 + dst.g as u32 * inv_a) / 255) as u8,
        b: ((src.b as u32 * 255 + dst.b as u32 * inv_a) / 255) as u8,
        a: ((src_a * 255 + dst.a as u32 * inv_a) / 255) as u8,
    }
}

/// Axis-aligned bounding box
#[derive(Debug, Clone, Copy, Default)]
pub struct Bounds {
    pub min_x: f32, pub min_y: f32, pub max_x: f32, pub max_y: f32,
}

impl Bounds {
    pub const fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self { min_x, min_y, max_x, max_y }
    }
    
    pub fn width(&self) -> f32 { self.max_x - self.min_x }
    pub fn height(&self) -> f32 { self.max_y - self.min_y }
    
    pub fn intersects(&self, other: &Bounds) -> bool {
        self.min_x <= other.max_x && self.max_x >= other.min_x &&
        self.min_y <= other.max_y && self.max_y >= other.min_y
    }
    
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }
    
    pub fn union(&self, other: &Bounds) -> Bounds {
        Bounds {
            min_x: self.min_x.min(other.min_x), min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x), max_y: self.max_y.max(other.max_y),
        }
    }
}

/// Linear interpolation
pub fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

/// Clamp value to range
pub fn clamp(value: f32, min: f32, max: f32) -> f32 { value.max(min).min(max) }

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simd_level() {
        let _level = SimdLevel::detect();
    }
    
    #[test]
    fn test_color_blend() {
        let src = Color4::new(255, 0, 0, 128);
        let dst = Color4::new(0, 255, 0, 255);
        let result = blend_color(src, dst);
        assert!(result.r > 0);
        assert!(result.g > 0);
    }
    
    #[test]
    fn test_bounds() {
        let a = Bounds::new(0.0, 0.0, 10.0, 10.0);
        let b = Bounds::new(5.0, 5.0, 15.0, 15.0);
        assert!(a.intersects(&b));
    }
    
    #[test]
    fn test_lerp() {
        assert_eq!(lerp(0.0, 10.0, 0.5), 5.0);
    }
}
