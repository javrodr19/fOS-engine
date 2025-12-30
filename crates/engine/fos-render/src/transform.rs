//! CSS Transforms module
//!
//! Provides 2D and 3D transforms for elements.

use std::f32::consts::PI;

/// 2D transformation matrix (3x3 homogeneous)
/// 
/// | a c e |
/// | b d f |
/// | 0 0 1 |
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform2D {
    pub a: f32,  // scale-x
    pub b: f32,  // skew-y
    pub c: f32,  // skew-x
    pub d: f32,  // scale-y
    pub e: f32,  // translate-x
    pub f: f32,  // translate-y
}

impl Default for Transform2D {
    fn default() -> Self {
        Self::identity()
    }
}

impl Transform2D {
    /// Identity transform (no transformation)
    pub const fn identity() -> Self {
        Self {
            a: 1.0, b: 0.0,
            c: 0.0, d: 1.0,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Translation transform
    pub fn translate(tx: f32, ty: f32) -> Self {
        Self {
            a: 1.0, b: 0.0,
            c: 0.0, d: 1.0,
            e: tx, f: ty,
        }
    }
    
    /// Scale transform
    pub fn scale(sx: f32, sy: f32) -> Self {
        Self {
            a: sx, b: 0.0,
            c: 0.0, d: sy,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Uniform scale
    pub fn scale_uniform(s: f32) -> Self {
        Self::scale(s, s)
    }
    
    /// Rotation transform (angle in radians)
    pub fn rotate(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            a: cos, b: sin,
            c: -sin, d: cos,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Rotation in degrees
    pub fn rotate_deg(degrees: f32) -> Self {
        Self::rotate(degrees * PI / 180.0)
    }
    
    /// Skew X (angle in radians)
    pub fn skew_x(angle: f32) -> Self {
        Self {
            a: 1.0, b: 0.0,
            c: angle.tan(), d: 1.0,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Skew Y (angle in radians)
    pub fn skew_y(angle: f32) -> Self {
        Self {
            a: 1.0, b: angle.tan(),
            c: 0.0, d: 1.0,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Skew both axes
    pub fn skew(ax: f32, ay: f32) -> Self {
        Self {
            a: 1.0, b: ay.tan(),
            c: ax.tan(), d: 1.0,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Matrix multiplication (combine transforms)
    /// Returns self * other (self applied after other)
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
    
    /// Chain another transform (fluent API)
    pub fn then(self, other: Transform2D) -> Self {
        other.multiply(&self)
    }
    
    /// Apply transform to a point
    pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
    
    /// Get the inverse transform (if possible)
    pub fn inverse(&self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-10 {
            return None; // Singular matrix
        }
        let inv_det = 1.0 / det;
        Some(Self {
            a: self.d * inv_det,
            b: -self.b * inv_det,
            c: -self.c * inv_det,
            d: self.a * inv_det,
            e: (self.c * self.f - self.d * self.e) * inv_det,
            f: (self.b * self.e - self.a * self.f) * inv_det,
        })
    }
    
    /// Check if this is the identity transform
    pub fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() < 1e-6 &&
        self.b.abs() < 1e-6 &&
        self.c.abs() < 1e-6 &&
        (self.d - 1.0).abs() < 1e-6 &&
        self.e.abs() < 1e-6 &&
        self.f.abs() < 1e-6
    }
    
    /// Check if transform is only translation
    pub fn is_translation_only(&self) -> bool {
        (self.a - 1.0).abs() < 1e-6 &&
        self.b.abs() < 1e-6 &&
        self.c.abs() < 1e-6 &&
        (self.d - 1.0).abs() < 1e-6
    }
    
    /// Convert to tiny-skia Transform
    pub fn to_tiny_skia(&self) -> tiny_skia::Transform {
        tiny_skia::Transform::from_row(self.a, self.b, self.c, self.d, self.e, self.f)
    }
}

/// Transform origin point
#[derive(Debug, Clone, Copy, Default)]
pub struct TransformOrigin {
    /// X coordinate (pixels or percentage)
    pub x: f32,
    /// Y coordinate (pixels or percentage)
    pub y: f32,
    /// Whether x is a percentage (0.0-1.0)
    pub x_percent: bool,
    /// Whether y is a percentage (0.0-1.0)
    pub y_percent: bool,
}

impl TransformOrigin {
    /// Center origin (default)
    pub const CENTER: Self = Self {
        x: 0.5, y: 0.5,
        x_percent: true, y_percent: true,
    };
    
    /// Top-left origin
    pub const TOP_LEFT: Self = Self {
        x: 0.0, y: 0.0,
        x_percent: true, y_percent: true,
    };
    
    /// Compute actual pixel coordinates
    pub fn compute(&self, width: f32, height: f32) -> (f32, f32) {
        let x = if self.x_percent { self.x * width } else { self.x };
        let y = if self.y_percent { self.y * height } else { self.y };
        (x, y)
    }
}

/// Apply transform around an origin point
pub fn transform_around_origin(
    transform: &Transform2D,
    origin_x: f32,
    origin_y: f32,
) -> Transform2D {
    // Translate to origin -> apply transform -> translate back
    Transform2D::translate(-origin_x, -origin_y)
        .then(*transform)
        .then(Transform2D::translate(origin_x, origin_y))
}

/// 3D transformation matrix (4x4 homogeneous)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform3D {
    /// Matrix elements (row-major)
    pub m: [[f32; 4]; 4],
}

impl Default for Transform3D {
    fn default() -> Self {
        Self::identity()
    }
}

impl Transform3D {
    /// Identity transform
    pub fn identity() -> Self {
        Self {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
    
    /// Translation in 3D
    pub fn translate(tx: f32, ty: f32, tz: f32) -> Self {
        Self {
            m: [
                [1.0, 0.0, 0.0, tx],
                [0.0, 1.0, 0.0, ty],
                [0.0, 0.0, 1.0, tz],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
    
    /// Scale in 3D
    pub fn scale(sx: f32, sy: f32, sz: f32) -> Self {
        Self {
            m: [
                [sx, 0.0, 0.0, 0.0],
                [0.0, sy, 0.0, 0.0],
                [0.0, 0.0, sz, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
    
    /// Rotation around X axis (radians)
    pub fn rotate_x(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, cos, -sin, 0.0],
                [0.0, sin, cos, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
    
    /// Rotation around Y axis (radians)
    pub fn rotate_y(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            m: [
                [cos, 0.0, sin, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [-sin, 0.0, cos, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
    
    /// Rotation around Z axis (radians) - same as 2D rotate
    pub fn rotate_z(angle: f32) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            m: [
                [cos, -sin, 0.0, 0.0],
                [sin, cos, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
    
    /// Rotation in degrees
    pub fn rotate_x_deg(degrees: f32) -> Self {
        Self::rotate_x(degrees * PI / 180.0)
    }
    
    pub fn rotate_y_deg(degrees: f32) -> Self {
        Self::rotate_y(degrees * PI / 180.0)
    }
    
    pub fn rotate_z_deg(degrees: f32) -> Self {
        Self::rotate_z(degrees * PI / 180.0)
    }
    
    /// Perspective transform
    /// d is the distance from the viewer to the z=0 plane
    pub fn perspective(d: f32) -> Self {
        if d <= 0.0 {
            return Self::identity();
        }
        Self {
            m: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, -1.0 / d, 1.0],
            ],
        }
    }
    
    /// Matrix multiplication
    pub fn multiply(&self, other: &Transform3D) -> Self {
        let mut result = [[0.0f32; 4]; 4];
        for i in 0..4 {
            for j in 0..4 {
                for k in 0..4 {
                    result[i][j] += self.m[i][k] * other.m[k][j];
                }
            }
        }
        Self { m: result }
    }
    
    /// Chain transforms
    pub fn then(self, other: Transform3D) -> Self {
        other.multiply(&self)
    }
    
    /// Transform a 3D point
    pub fn transform_point(&self, x: f32, y: f32, z: f32) -> (f32, f32, f32) {
        let w = self.m[3][0] * x + self.m[3][1] * y + self.m[3][2] * z + self.m[3][3];
        let w = if w.abs() < 1e-10 { 1.0 } else { w };
        
        (
            (self.m[0][0] * x + self.m[0][1] * y + self.m[0][2] * z + self.m[0][3]) / w,
            (self.m[1][0] * x + self.m[1][1] * y + self.m[1][2] * z + self.m[1][3]) / w,
            (self.m[2][0] * x + self.m[2][1] * y + self.m[2][2] * z + self.m[2][3]) / w,
        )
    }
    
    /// Project 3D point to 2D (for rendering)
    pub fn project_to_2d(&self, x: f32, y: f32, z: f32) -> (f32, f32) {
        let (px, py, _) = self.transform_point(x, y, z);
        (px, py)
    }
    
    /// Check if backface is visible (z < 0 after transform means facing away)
    pub fn is_backface_visible(&self, x: f32, y: f32, z: f32) -> bool {
        let (_, _, pz) = self.transform_point(x, y, z);
        pz < 0.0
    }
    
    /// Convert to 2D transform (ignoring Z)
    pub fn to_2d(&self) -> Transform2D {
        Transform2D {
            a: self.m[0][0],
            b: self.m[1][0],
            c: self.m[0][1],
            d: self.m[1][1],
            e: self.m[0][3],
            f: self.m[1][3],
        }
    }
    
    /// Create from 2D transform
    pub fn from_2d(t: &Transform2D) -> Self {
        Self {
            m: [
                [t.a, t.c, 0.0, t.e],
                [t.b, t.d, 0.0, t.f],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }
}

/// Backface visibility setting
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BackfaceVisibility {
    /// Backface is visible (default)
    #[default]
    Visible,
    /// Backface is hidden
    Hidden,
}

/// Perspective origin for 3D transforms
#[derive(Debug, Clone, Copy, Default)]
pub struct PerspectiveOrigin {
    pub x: f32,
    pub y: f32,
    pub x_percent: bool,
    pub y_percent: bool,
}

impl PerspectiveOrigin {
    pub const CENTER: Self = Self {
        x: 0.5, y: 0.5,
        x_percent: true, y_percent: true,
    };
    
    pub fn compute(&self, width: f32, height: f32) -> (f32, f32) {
        let x = if self.x_percent { self.x * width } else { self.x };
        let y = if self.y_percent { self.y * height } else { self.y };
        (x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_identity() {
        let t = Transform2D::identity();
        assert!(t.is_identity());
        
        let (x, y) = t.transform_point(10.0, 20.0);
        assert_eq!((x, y), (10.0, 20.0));
    }
    
    #[test]
    fn test_translate() {
        let t = Transform2D::translate(5.0, 10.0);
        let (x, y) = t.transform_point(0.0, 0.0);
        assert_eq!((x, y), (5.0, 10.0));
    }
    
    #[test]
    fn test_scale() {
        let t = Transform2D::scale(2.0, 3.0);
        let (x, y) = t.transform_point(10.0, 10.0);
        assert_eq!((x, y), (20.0, 30.0));
    }
    
    #[test]
    fn test_rotate_90() {
        let t = Transform2D::rotate_deg(90.0);
        let (x, y) = t.transform_point(1.0, 0.0);
        assert!((x - 0.0).abs() < 0.001);
        assert!((y - 1.0).abs() < 0.001);
    }
    
    #[test]
    fn test_chain_transforms() {
        let t = Transform2D::translate(10.0, 0.0)
            .then(Transform2D::scale(2.0, 2.0));
        
        let (x, y) = t.transform_point(0.0, 0.0);
        assert_eq!((x, y), (20.0, 0.0)); // Translated then scaled
    }
    
    #[test]
    fn test_inverse() {
        let t = Transform2D::translate(10.0, 20.0)
            .then(Transform2D::scale(2.0, 2.0));
        let inv = t.inverse().unwrap();
        
        // Apply both should give identity
        let combined = t.multiply(&inv);
        assert!(combined.is_identity());
    }
    
    #[test]
    fn test_transform_origin() {
        let origin = TransformOrigin::CENTER;
        let (x, y) = origin.compute(100.0, 200.0);
        assert_eq!((x, y), (50.0, 100.0));
    }
    
    #[test]
    fn test_transform3d_identity() {
        let t = Transform3D::identity();
        let (x, y, z) = t.transform_point(10.0, 20.0, 30.0);
        assert_eq!((x, y, z), (10.0, 20.0, 30.0));
    }
    
    #[test]
    fn test_transform3d_translate() {
        let t = Transform3D::translate(5.0, 10.0, 15.0);
        let (x, y, z) = t.transform_point(0.0, 0.0, 0.0);
        assert_eq!((x, y, z), (5.0, 10.0, 15.0));
    }
    
    #[test]
    fn test_transform3d_rotate_x() {
        let t = Transform3D::rotate_x_deg(90.0);
        let (x, y, z) = t.transform_point(0.0, 1.0, 0.0);
        // Rotating (0,1,0) 90 degrees around X gives (0,0,1)
        assert!((x - 0.0).abs() < 0.001);
        assert!((y - 0.0).abs() < 0.001);
        assert!((z - 1.0).abs() < 0.001);
    }
    
    #[test]
    fn test_transform3d_perspective() {
        let t = Transform3D::perspective(500.0);
        // Object at z=0 should be unchanged
        let (x, y, _) = t.transform_point(100.0, 100.0, 0.0);
        assert!((x - 100.0).abs() < 0.01);
        assert!((y - 100.0).abs() < 0.01);
        
        // Object closer (negative z) should appear larger
        let t2 = Transform3D::perspective(1000.0);
        let (x2, y2, _) = t2.transform_point(100.0, 0.0, 500.0);
        // w = 1 + z * (-1/d) = 1 - 500/1000 = 0.5, so x = 100 / 0.5 = 200
        assert!((x2 - 200.0).abs() < 1.0, "x2 = {}", x2);
    }
}

