//! Transform Matrix
//!
//! 2D transformation matrix for Canvas.

/// 2D Transform Matrix (3x3 homogeneous)
/// | a c e |
/// | b d f |
/// | 0 0 1 |
#[derive(Debug, Clone, Copy)]
pub struct TransformMatrix {
    pub a: f64, // scale x
    pub b: f64, // skew y
    pub c: f64, // skew x
    pub d: f64, // scale y
    pub e: f64, // translate x
    pub f: f64, // translate y
}

impl TransformMatrix {
    /// Identity matrix
    pub fn identity() -> Self {
        Self {
            a: 1.0, b: 0.0,
            c: 0.0, d: 1.0,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Create from values
    pub fn new(a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) -> Self {
        Self { a, b, c, d, e, f }
    }
    
    /// Translation matrix
    pub fn translate(tx: f64, ty: f64) -> Self {
        Self {
            a: 1.0, b: 0.0,
            c: 0.0, d: 1.0,
            e: tx, f: ty,
        }
    }
    
    /// Scale matrix
    pub fn scale(sx: f64, sy: f64) -> Self {
        Self {
            a: sx, b: 0.0,
            c: 0.0, d: sy,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Rotation matrix (angle in radians)
    pub fn rotate(angle: f64) -> Self {
        let cos = angle.cos();
        let sin = angle.sin();
        Self {
            a: cos, b: sin,
            c: -sin, d: cos,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Skew X matrix
    pub fn skew_x(angle: f64) -> Self {
        Self {
            a: 1.0, b: 0.0,
            c: angle.tan(), d: 1.0,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Skew Y matrix
    pub fn skew_y(angle: f64) -> Self {
        Self {
            a: 1.0, b: angle.tan(),
            c: 0.0, d: 1.0,
            e: 0.0, f: 0.0,
        }
    }
    
    /// Multiply matrices
    pub fn multiply(&self, other: &Self) -> Self {
        Self {
            a: self.a * other.a + self.c * other.b,
            b: self.b * other.a + self.d * other.b,
            c: self.a * other.c + self.c * other.d,
            d: self.b * other.c + self.d * other.d,
            e: self.a * other.e + self.c * other.f + self.e,
            f: self.b * other.e + self.d * other.f + self.f,
        }
    }
    
    /// Transform a point  
    pub fn transform_point(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
    
    /// Invert matrix
    pub fn invert(&self) -> Option<Self> {
        let det = self.a * self.d - self.b * self.c;
        if det.abs() < 1e-10 {
            return None;
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
    
    /// Check if identity
    pub fn is_identity(&self) -> bool {
        (self.a - 1.0).abs() < 1e-10 &&
        self.b.abs() < 1e-10 &&
        self.c.abs() < 1e-10 &&
        (self.d - 1.0).abs() < 1e-10 &&
        self.e.abs() < 1e-10 &&
        self.f.abs() < 1e-10
    }
    
    /// Reset to identity
    pub fn reset(&mut self) {
        *self = Self::identity();
    }
}

impl Default for TransformMatrix {
    fn default() -> Self {
        Self::identity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_identity() {
        let m = TransformMatrix::identity();
        assert!(m.is_identity());
        
        let (x, y) = m.transform_point(10.0, 20.0);
        assert_eq!(x, 10.0);
        assert_eq!(y, 20.0);
    }
    
    #[test]
    fn test_translate() {
        let m = TransformMatrix::translate(100.0, 50.0);
        let (x, y) = m.transform_point(10.0, 20.0);
        
        assert_eq!(x, 110.0);
        assert_eq!(y, 70.0);
    }
    
    #[test]
    fn test_scale() {
        let m = TransformMatrix::scale(2.0, 3.0);
        let (x, y) = m.transform_point(10.0, 20.0);
        
        assert_eq!(x, 20.0);
        assert_eq!(y, 60.0);
    }
    
    #[test]
    fn test_invert() {
        let m = TransformMatrix::translate(100.0, 50.0);
        let inv = m.invert().unwrap();
        let result = m.multiply(&inv);
        
        assert!(result.is_identity());
    }
}
