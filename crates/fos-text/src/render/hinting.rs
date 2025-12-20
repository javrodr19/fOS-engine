//! Subpixel Antialiasing and Font Hinting
//!
//! Advanced text rendering techniques for crisp text on LCD screens.

/// Subpixel rendering mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubpixelMode {
    /// No subpixel rendering (grayscale antialiasing)
    #[default]
    None,
    /// RGB horizontal subpixel order (most common)
    Rgb,
    /// BGR horizontal subpixel order
    Bgr,
    /// RGB vertical subpixel order
    VRgb,
    /// BGR vertical subpixel order
    VBgr,
}

impl SubpixelMode {
    /// Get number of horizontal subpixels
    pub fn h_subpixels(&self) -> u8 {
        match self {
            Self::None => 1,
            Self::Rgb | Self::Bgr => 3,
            Self::VRgb | Self::VBgr => 1,
        }
    }
    
    /// Get number of vertical subpixels
    pub fn v_subpixels(&self) -> u8 {
        match self {
            Self::None => 1,
            Self::Rgb | Self::Bgr => 1,
            Self::VRgb | Self::VBgr => 3,
        }
    }
    
    /// Check if horizontal subpixels
    pub fn is_horizontal(&self) -> bool {
        matches!(self, Self::Rgb | Self::Bgr)
    }
    
    /// Get subpixel order
    pub fn subpixel_order(&self) -> [usize; 3] {
        match self {
            Self::None => [0, 0, 0],
            Self::Rgb | Self::VRgb => [0, 1, 2], // R, G, B
            Self::Bgr | Self::VBgr => [2, 1, 0], // B, G, R
        }
    }
}

/// Font hinting mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HintingMode {
    /// No hinting (rasterize outlines as-is)
    None,
    /// Light hinting (preserve original shapes more)
    Light,
    /// Normal hinting (balance between sharpness and fidelity)
    #[default]
    Normal,
    /// Full hinting (maximum sharpness, may distort)
    Full,
}

impl HintingMode {
    /// Get hinting strength (0.0 - 1.0)
    pub fn strength(&self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::Light => 0.3,
            Self::Normal => 0.6,
            Self::Full => 1.0,
        }
    }
}

/// Antialiasing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AntialiasMode {
    /// No antialiasing (binary)
    None,
    /// Grayscale antialiasing
    #[default]
    Grayscale,
    /// Subpixel antialiasing (LCD)
    Subpixel,
}

/// Text rendering settings
#[derive(Debug, Clone, Copy)]
pub struct TextRenderSettings {
    pub subpixel_mode: SubpixelMode,
    pub hinting_mode: HintingMode,
    pub antialias_mode: AntialiasMode,
    /// Gamma correction value (typically 1.0 - 2.2)
    pub gamma: f32,
    /// Contrast adjustment (0.0 - 1.0)
    pub contrast: f32,
}

impl Default for TextRenderSettings {
    fn default() -> Self {
        Self {
            subpixel_mode: SubpixelMode::Rgb,
            hinting_mode: HintingMode::Normal,
            antialias_mode: AntialiasMode::Grayscale,
            gamma: 1.8,
            contrast: 0.5,
        }
    }
}

impl TextRenderSettings {
    /// Create settings for LCD display
    pub fn lcd() -> Self {
        Self {
            subpixel_mode: SubpixelMode::Rgb,
            hinting_mode: HintingMode::Normal,
            antialias_mode: AntialiasMode::Subpixel,
            gamma: 1.8,
            contrast: 0.5,
        }
    }
    
    /// Create settings for grayscale (non-LCD)
    pub fn grayscale() -> Self {
        Self {
            subpixel_mode: SubpixelMode::None,
            hinting_mode: HintingMode::Light,
            antialias_mode: AntialiasMode::Grayscale,
            gamma: 1.0,
            contrast: 0.5,
        }
    }
    
    /// Create settings for high DPI displays
    pub fn hidpi() -> Self {
        Self {
            subpixel_mode: SubpixelMode::None,
            hinting_mode: HintingMode::None,
            antialias_mode: AntialiasMode::Grayscale,
            gamma: 1.0,
            contrast: 0.5,
        }
    }
}

/// Subpixel filter for LCD rendering
#[derive(Debug, Clone)]
pub struct SubpixelFilter {
    /// Filter weights (typically 5 taps)
    pub weights: [f32; 5],
}

impl Default for SubpixelFilter {
    fn default() -> Self {
        // FreeType default filter
        Self {
            weights: [0.08, 0.24, 0.36, 0.24, 0.08],
        }
    }
}

impl SubpixelFilter {
    /// Create a light filter (less color fringing)
    pub fn light() -> Self {
        Self {
            weights: [0.0, 0.25, 0.5, 0.25, 0.0],
        }
    }
    
    /// Create a legacy filter (sharper, more color fringing)
    pub fn legacy() -> Self {
        Self {
            weights: [0.1, 0.2, 0.4, 0.2, 0.1],
        }
    }
    
    /// Apply filter to grayscale coverage
    pub fn apply(&self, coverage: &[u8], width: usize) -> Vec<u8> {
        let mut result = Vec::with_capacity(coverage.len() * 3);
        
        for i in 0..coverage.len() {
            // For each pixel, compute R, G, B subpixels
            for channel in 0..3 {
                let offset = channel as isize - 1;
                let mut value = 0.0f32;
                
                for (j, &weight) in self.weights.iter().enumerate() {
                    let tap_offset = j as isize - 2 + offset;
                    let idx = (i as isize + tap_offset).clamp(0, coverage.len() as isize - 1) as usize;
                    value += coverage[idx] as f32 * weight;
                }
                
                result.push(value.clamp(0.0, 255.0) as u8);
            }
        }
        
        result
    }
}

/// Glyph bitmap with subpixel data
#[derive(Debug, Clone)]
pub struct SubpixelBitmap {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Pitch (bytes per row)
    pub pitch: u32,
    /// Pixel data (3 bytes per pixel for subpixel, 1 for grayscale)
    pub data: Vec<u8>,
    /// Subpixel or grayscale
    pub mode: AntialiasMode,
}

impl SubpixelBitmap {
    /// Create a new grayscale bitmap
    pub fn grayscale(width: u32, height: u32) -> Self {
        let pitch = width;
        Self {
            width,
            height,
            pitch,
            data: vec![0; (pitch * height) as usize],
            mode: AntialiasMode::Grayscale,
        }
    }
    
    /// Create a new subpixel bitmap
    pub fn subpixel(width: u32, height: u32) -> Self {
        let pitch = width * 3;
        Self {
            width,
            height,
            pitch,
            data: vec![0; (pitch * height) as usize],
            mode: AntialiasMode::Subpixel,
        }
    }
    
    /// Get pixel at position
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<&[u8]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        
        let bytes_per_pixel = match self.mode {
            AntialiasMode::Subpixel => 3,
            _ => 1,
        };
        
        let offset = (y * self.pitch + x * bytes_per_pixel) as usize;
        Some(&self.data[offset..offset + bytes_per_pixel as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_subpixel_mode() {
        assert_eq!(SubpixelMode::Rgb.h_subpixels(), 3);
        assert_eq!(SubpixelMode::None.h_subpixels(), 1);
        assert!(SubpixelMode::Rgb.is_horizontal());
    }
    
    #[test]
    fn test_hinting_mode() {
        assert_eq!(HintingMode::None.strength(), 0.0);
        assert_eq!(HintingMode::Full.strength(), 1.0);
    }
    
    #[test]
    fn test_render_settings() {
        let lcd = TextRenderSettings::lcd();
        assert_eq!(lcd.antialias_mode, AntialiasMode::Subpixel);
        
        let hidpi = TextRenderSettings::hidpi();
        assert_eq!(hidpi.hinting_mode, HintingMode::None);
    }
    
    #[test]
    fn test_subpixel_filter() {
        let filter = SubpixelFilter::default();
        let coverage = vec![0, 128, 255, 128, 0];
        let result = filter.apply(&coverage, 5);
        
        assert_eq!(result.len(), 15); // 5 pixels * 3 channels
    }
    
    #[test]
    fn test_subpixel_bitmap() {
        let bitmap = SubpixelBitmap::grayscale(100, 50);
        assert_eq!(bitmap.data.len(), 5000);
        
        let bitmap = SubpixelBitmap::subpixel(100, 50);
        assert_eq!(bitmap.data.len(), 15000);
    }
}
