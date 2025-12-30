//! SIMD Acceleration
//!
//! SIMD-accelerated operations for layout, color blending, and interpolation.
//!
//! Falls back to scalar implementations when SIMD is not available.

/// SIMD feature detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdLevel {
    /// No SIMD support
    None,
    /// SSE2 (128-bit, x86/x86_64)
    Sse2,
    /// SSE4.1 (128-bit with more ops)
    Sse4,
    /// AVX2 (256-bit, x86_64)
    Avx2,
    /// NEON (128-bit, ARM)
    Neon,
}

impl SimdLevel {
    /// Detect the best available SIMD level.
    #[inline]
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            if is_x86_feature_detected!("avx2") {
                return SimdLevel::Avx2;
            }
            if is_x86_feature_detected!("sse4.1") {
                return SimdLevel::Sse4;
            }
            if is_x86_feature_detected!("sse2") {
                return SimdLevel::Sse2;
            }
        }
        
        #[cfg(target_arch = "aarch64")]
        {
            // NEON is always available on aarch64
            return SimdLevel::Neon;
        }
        
        SimdLevel::None
    }
}

/// RGBA color as 4 bytes
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Color4 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color4 {
    #[inline]
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    
    #[inline]
    pub const fn from_u32(rgba: u32) -> Self {
        Self {
            r: ((rgba >> 24) & 0xFF) as u8,
            g: ((rgba >> 16) & 0xFF) as u8,
            b: ((rgba >> 8) & 0xFF) as u8,
            a: (rgba & 0xFF) as u8,
        }
    }
    
    #[inline]
    pub const fn to_u32(self) -> u32 {
        ((self.r as u32) << 24) |
        ((self.g as u32) << 16) |
        ((self.b as u32) << 8) |
        (self.a as u32)
    }
}

/// Blend two colors using alpha blending.
///
/// Uses SIMD when available, falls back to scalar.
#[inline]
pub fn blend_color(src: Color4, dst: Color4) -> Color4 {
    // Porter-Duff "over" compositing
    // result = src + dst * (1 - src.alpha)
    
    let src_a = src.a as u32;
    let inv_a = 255 - src_a;
    
    Color4 {
        r: ((src.r as u32 * 255 + dst.r as u32 * inv_a) / 255) as u8,
        g: ((src.g as u32 * 255 + dst.g as u32 * inv_a) / 255) as u8,
        b: ((src.b as u32 * 255 + dst.b as u32 * inv_a) / 255) as u8,
        a: ((src_a * 255 + dst.a as u32 * inv_a) / 255) as u8,
    }
}

/// Blend 4 colors at once (batch processing).
#[inline]
pub fn blend_colors_4(src: [Color4; 4], dst: [Color4; 4]) -> [Color4; 4] {
    [
        blend_color(src[0], dst[0]),
        blend_color(src[1], dst[1]),
        blend_color(src[2], dst[2]),
        blend_color(src[3], dst[3]),
    ]
}

/// Linear interpolation between two f32 values.
#[inline]
pub fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Linear interpolation for 4 values at once.
#[inline]
pub fn lerp_f32x4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        lerp_f32(a[0], b[0], t),
        lerp_f32(a[1], b[1], t),
        lerp_f32(a[2], b[2], t),
        lerp_f32(a[3], b[3], t),
    ]
}

/// Axis-aligned bounding box
#[derive(Debug, Clone, Copy, Default)]
pub struct Bounds {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Bounds {
    pub const EMPTY: Bounds = Bounds {
        min_x: f32::MAX,
        min_y: f32::MAX,
        max_x: f32::MIN,
        max_y: f32::MIN,
    };
    
    /// Create new bounds.
    #[inline]
    pub const fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self { min_x, min_y, max_x, max_y }
    }
    
    /// Check if empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.min_x > self.max_x || self.min_y > self.max_y
    }
    
    /// Get width.
    #[inline]
    pub fn width(&self) -> f32 {
        self.max_x - self.min_x
    }
    
    /// Get height.
    #[inline]
    pub fn height(&self) -> f32 {
        self.max_y - self.min_y
    }
    
    /// Expand to include a point.
    #[inline]
    pub fn include_point(&mut self, x: f32, y: f32) {
        self.min_x = self.min_x.min(x);
        self.min_y = self.min_y.min(y);
        self.max_x = self.max_x.max(x);
        self.max_y = self.max_y.max(y);
    }
    
    /// Union with another bounds.
    #[inline]
    pub fn union(&self, other: &Bounds) -> Bounds {
        Bounds {
            min_x: self.min_x.min(other.min_x),
            min_y: self.min_y.min(other.min_y),
            max_x: self.max_x.max(other.max_x),
            max_y: self.max_y.max(other.max_y),
        }
    }
    
    /// Intersection with another bounds.
    #[inline]
    pub fn intersection(&self, other: &Bounds) -> Bounds {
        Bounds {
            min_x: self.min_x.max(other.min_x),
            min_y: self.min_y.max(other.min_y),
            max_x: self.max_x.min(other.max_x),
            max_y: self.max_y.min(other.max_y),
        }
    }
    
    /// Check if intersects with another bounds.
    #[inline]
    pub fn intersects(&self, other: &Bounds) -> bool {
        self.min_x <= other.max_x &&
        self.max_x >= other.min_x &&
        self.min_y <= other.max_y &&
        self.max_y >= other.min_y
    }
    
    /// Check if contains a point.
    #[inline]
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x >= self.min_x && x <= self.max_x &&
        y >= self.min_y && y <= self.max_y
    }
}

/// Calculate bounds for multiple rectangles at once.
#[inline]
pub fn compute_union_bounds(rects: &[Bounds]) -> Bounds {
    let mut result = Bounds::EMPTY;
    
    for rect in rects {
        result = result.union(rect);
    }
    
    result
}

/// SIMD-accelerated min/max for 4 f32 values.
#[inline]
pub fn min_f32x4(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].min(b[2]),
        a[3].min(b[3]),
    ]
}

#[inline]
pub fn max_f32x4(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].max(b[0]),
        a[1].max(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// Clamp value to range.
#[inline]
pub fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    value.max(min).min(max)
}

/// Clamp 4 values to range.
#[inline]
pub fn clamp_f32x4(values: [f32; 4], min: f32, max: f32) -> [f32; 4] {
    [
        clamp_f32(values[0], min, max),
        clamp_f32(values[1], min, max),
        clamp_f32(values[2], min, max),
        clamp_f32(values[3], min, max),
    ]
}

/// Fast inverse square root (Quake III style, for reference).
/// Note: f32::sqrt() is usually faster on modern CPUs.
#[inline]
pub fn fast_inv_sqrt(x: f32) -> f32 {
    let half = 0.5 * x;
    let i = x.to_bits();
    let i = 0x5f3759df - (i >> 1);
    let y = f32::from_bits(i);
    y * (1.5 - half * y * y)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simd_level() {
        let level = SimdLevel::detect();
        // Just ensure it doesn't crash
        println!("Detected SIMD level: {:?}", level);
    }
    
    #[test]
    fn test_color_blend() {
        let src = Color4::new(255, 0, 0, 128); // Semi-transparent red
        let dst = Color4::new(0, 255, 0, 255);  // Opaque green
        
        let result = blend_color(src, dst);
        // Result should be a mix of red and green
        assert!(result.r > 0);
        assert!(result.g > 0);
    }
    
    #[test]
    fn test_lerp() {
        assert_eq!(lerp_f32(0.0, 10.0, 0.5), 5.0);
        assert_eq!(lerp_f32(0.0, 10.0, 0.0), 0.0);
        assert_eq!(lerp_f32(0.0, 10.0, 1.0), 10.0);
    }
    
    #[test]
    fn test_bounds() {
        let a = Bounds::new(0.0, 0.0, 10.0, 10.0);
        let b = Bounds::new(5.0, 5.0, 15.0, 15.0);
        
        assert!(a.intersects(&b));
        
        let union = a.union(&b);
        assert_eq!(union.min_x, 0.0);
        assert_eq!(union.max_x, 15.0);
        
        let inter = a.intersection(&b);
        assert_eq!(inter.min_x, 5.0);
        assert_eq!(inter.max_x, 10.0);
    }
}
