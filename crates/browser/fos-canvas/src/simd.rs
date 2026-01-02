//! SIMD Utilities for Canvas 2D
//!
//! Local SIMD acceleration for pixel operations.
//! This is a minimal copy to avoid cyclic dependencies with fos-engine.

/// RGBA color for SIMD operations
#[derive(Debug, Clone, Copy, Default)]
pub struct Color4 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color4 {
    /// Create a new color
    pub fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Create from array
    pub fn from_array(arr: [u8; 4]) -> Self {
        Self {
            r: arr[0],
            g: arr[1],
            b: arr[2],
            a: arr[3],
        }
    }

    /// Convert to array
    pub fn to_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }
}

/// Blend source color over destination (Porter-Duff over)
pub fn blend_color(src: Color4, dst: Color4) -> Color4 {
    if src.a == 255 {
        return src;
    }
    if src.a == 0 {
        return dst;
    }

    let src_a = src.a as u32;
    let dst_a = dst.a as u32;
    let inv_src_a = 255 - src_a;

    // out_a = src_a + dst_a * (1 - src_a)
    let out_a = src_a + (dst_a * inv_src_a) / 255;
    if out_a == 0 {
        return Color4::default();
    }

    // out_c = (src_c * src_a + dst_c * dst_a * (1 - src_a)) / out_a
    let blend = |s: u8, d: u8| -> u8 {
        let s = s as u32;
        let d = d as u32;
        ((s * src_a + d * dst_a * inv_src_a / 255) / out_a) as u8
    };

    Color4 {
        r: blend(src.r, dst.r),
        g: blend(src.g, dst.g),
        b: blend(src.b, dst.b),
        a: out_a as u8,
    }
}

/// Blend 4 colors at once (SIMD-friendly structure)
pub fn blend_colors_4(src: [Color4; 4], dst: [Color4; 4]) -> [Color4; 4] {
    [
        blend_color(src[0], dst[0]),
        blend_color(src[1], dst[1]),
        blend_color(src[2], dst[2]),
        blend_color(src[3], dst[3]),
    ]
}

/// Linear interpolation for f32x4
pub fn lerp_f32x4(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blend_opaque() {
        let src = Color4::new(255, 0, 0, 255);
        let dst = Color4::new(0, 255, 0, 255);
        let result = blend_color(src, dst);
        assert_eq!(result.r, 255);
        assert_eq!(result.g, 0);
    }

    #[test]
    fn test_blend_transparent() {
        let src = Color4::new(255, 0, 0, 0);
        let dst = Color4::new(0, 255, 0, 255);
        let result = blend_color(src, dst);
        assert_eq!(result.g, 255);
    }

    #[test]
    fn test_blend_semi_transparent() {
        let src = Color4::new(255, 0, 0, 128);
        let dst = Color4::new(0, 0, 255, 255);
        let result = blend_color(src, dst);
        // Should be around 128 red, 0 green, 127 blue
        assert!(result.r > 100);
        assert!(result.b > 100);
    }
}
