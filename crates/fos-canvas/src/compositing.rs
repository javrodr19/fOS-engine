//! Compositing Operations
//!
//! Global composite operations for Canvas 2D.

/// Composite operation
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompositeOperation {
    #[default]
    SourceOver,
    SourceIn,
    SourceOut,
    SourceAtop,
    DestinationOver,
    DestinationIn,
    DestinationOut,
    DestinationAtop,
    Lighter,
    Copy,
    Xor,
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
    Hue,
    Saturation,
    Color,
    Luminosity,
}

/// Blend mode (same as composite for most operations)
pub type BlendMode = CompositeOperation;

impl CompositeOperation {
    /// Parse from string
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "source-over" => Self::SourceOver,
            "source-in" => Self::SourceIn,
            "source-out" => Self::SourceOut,
            "source-atop" => Self::SourceAtop,
            "destination-over" => Self::DestinationOver,
            "destination-in" => Self::DestinationIn,
            "destination-out" => Self::DestinationOut,
            "destination-atop" => Self::DestinationAtop,
            "lighter" => Self::Lighter,
            "copy" => Self::Copy,
            "xor" => Self::Xor,
            "multiply" => Self::Multiply,
            "screen" => Self::Screen,
            "overlay" => Self::Overlay,
            "darken" => Self::Darken,
            "lighten" => Self::Lighten,
            "color-dodge" => Self::ColorDodge,
            "color-burn" => Self::ColorBurn,
            "hard-light" => Self::HardLight,
            "soft-light" => Self::SoftLight,
            "difference" => Self::Difference,
            "exclusion" => Self::Exclusion,
            "hue" => Self::Hue,
            "saturation" => Self::Saturation,
            "color" => Self::Color,
            "luminosity" => Self::Luminosity,
            _ => return None,
        })
    }
    
    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SourceOver => "source-over",
            Self::SourceIn => "source-in",
            Self::SourceOut => "source-out",
            Self::SourceAtop => "source-atop",
            Self::DestinationOver => "destination-over",
            Self::DestinationIn => "destination-in",
            Self::DestinationOut => "destination-out",
            Self::DestinationAtop => "destination-atop",
            Self::Lighter => "lighter",
            Self::Copy => "copy",
            Self::Xor => "xor",
            Self::Multiply => "multiply",
            Self::Screen => "screen",
            Self::Overlay => "overlay",
            Self::Darken => "darken",
            Self::Lighten => "lighten",
            Self::ColorDodge => "color-dodge",
            Self::ColorBurn => "color-burn",
            Self::HardLight => "hard-light",
            Self::SoftLight => "soft-light",
            Self::Difference => "difference",
            Self::Exclusion => "exclusion",
            Self::Hue => "hue",
            Self::Saturation => "saturation",
            Self::Color => "color",
            Self::Luminosity => "luminosity",
        }
    }
}

/// Blend two colors
pub fn blend_colors(src: (u8, u8, u8, u8), dst: (u8, u8, u8, u8), op: CompositeOperation) -> (u8, u8, u8, u8) {
    let (sr, sg, sb, sa) = (src.0 as f64 / 255.0, src.1 as f64 / 255.0, src.2 as f64 / 255.0, src.3 as f64 / 255.0);
    let (dr, dg, db, da) = (dst.0 as f64 / 255.0, dst.1 as f64 / 255.0, dst.2 as f64 / 255.0, dst.3 as f64 / 255.0);
    
    let (r, g, b, a) = match op {
        CompositeOperation::SourceOver => {
            let a = sa + da * (1.0 - sa);
            if a < 1e-10 {
                (0.0, 0.0, 0.0, 0.0)
            } else {
                let r = (sr * sa + dr * da * (1.0 - sa)) / a;
                let g = (sg * sa + dg * da * (1.0 - sa)) / a;
                let b = (sb * sa + db * da * (1.0 - sa)) / a;
                (r, g, b, a)
            }
        }
        CompositeOperation::Multiply => {
            (sr * dr, sg * dg, sb * db, sa * da)
        }
        CompositeOperation::Screen => {
            (1.0 - (1.0 - sr) * (1.0 - dr), 1.0 - (1.0 - sg) * (1.0 - dg), 1.0 - (1.0 - sb) * (1.0 - db), 1.0 - (1.0 - sa) * (1.0 - da))
        }
        _ => (sr, sg, sb, sa), // Default to source
    };
    
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, (a * 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse() {
        assert_eq!(CompositeOperation::from_str("source-over"), Some(CompositeOperation::SourceOver));
        assert_eq!(CompositeOperation::from_str("multiply"), Some(CompositeOperation::Multiply));
    }
}
