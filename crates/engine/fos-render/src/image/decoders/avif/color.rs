//! Color Space Conversion
//!
//! YUV to RGB conversion with support for various color spaces and HDR.

use super::frame::Frame;
use super::super::simd::SimdOps;
use super::AvifError;

/// Color primaries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorPrimaries {
    Bt709,              // sRGB, Rec.709
    Unspecified,
    Bt470M,
    Bt470Bg,
    Bt601,              // NTSC/PAL
    Smpte240,
    Film,
    Bt2020,             // HDR, Wide gamut
    Xyz,
    Smpte431,           // DCI-P3
    Smpte432,           // Display P3
    Ebu3213,
}

impl ColorPrimaries {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Bt709,
            4 => Self::Bt470M,
            5 => Self::Bt470Bg,
            6 => Self::Bt601,
            7 => Self::Smpte240,
            8 => Self::Film,
            9 => Self::Bt2020,
            10 => Self::Xyz,
            11 => Self::Smpte431,
            12 => Self::Smpte432,
            22 => Self::Ebu3213,
            _ => Self::Unspecified,
        }
    }
}

/// Transfer characteristics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferCharacteristics {
    Bt709,
    Unspecified,
    Bt470M,
    Bt470Bg,
    Bt601,
    Smpte240,
    Linear,
    Log100,
    Log316,
    Iec61966,
    Bt1361,
    Srgb,               // Standard RGB
    Bt2020_10,
    Bt2020_12,
    Pq,                 // Perceptual Quantizer (HDR10)
    Smpte428,
    Hlg,                // Hybrid Log-Gamma
}

impl TransferCharacteristics {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Bt709,
            4 => Self::Bt470M,
            5 => Self::Bt470Bg,
            6 => Self::Bt601,
            7 => Self::Smpte240,
            8 => Self::Linear,
            9 => Self::Log100,
            10 => Self::Log316,
            11 => Self::Iec61966,
            12 => Self::Bt1361,
            13 => Self::Srgb,
            14 => Self::Bt2020_10,
            15 => Self::Bt2020_12,
            16 => Self::Pq,
            17 => Self::Smpte428,
            18 => Self::Hlg,
            _ => Self::Unspecified,
        }
    }
    
    pub fn is_hdr(&self) -> bool {
        matches!(self, Self::Pq | Self::Hlg)
    }
}

/// Matrix coefficients for YUV to RGB conversion
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatrixCoefficients {
    Identity,           // RGB
    Bt709,
    Unspecified,
    Fcc,
    Bt470Bg,
    Bt601,              // NTSC/PAL
    Smpte240,
    YCgCo,
    Bt2020Ncl,
    Bt2020Cl,
    Smpte2085,
    ChromaticityNcl,
    ChromaticityCl,
    ICtCp,
}

impl MatrixCoefficients {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Identity,
            1 => Self::Bt709,
            4 => Self::Fcc,
            5 => Self::Bt470Bg,
            6 => Self::Bt601,
            7 => Self::Smpte240,
            8 => Self::YCgCo,
            9 => Self::Bt2020Ncl,
            10 => Self::Bt2020Cl,
            11 => Self::Smpte2085,
            12 => Self::ChromaticityNcl,
            13 => Self::ChromaticityCl,
            14 => Self::ICtCp,
            _ => Self::Unspecified,
        }
    }
    
    /// Get YUV to RGB conversion coefficients
    /// Returns (Kr, Kg, Kb) where Y = Kr*R + Kg*G + Kb*B
    pub fn coefficients(&self) -> (f32, f32, f32) {
        match self {
            Self::Identity => (0.0, 1.0, 0.0), // RGB passthrough
            Self::Bt709 => (0.2126, 0.7152, 0.0722),
            Self::Bt601 | Self::Bt470Bg => (0.299, 0.587, 0.114),
            Self::Bt2020Ncl | Self::Bt2020Cl => (0.2627, 0.6780, 0.0593),
            Self::Smpte240 => (0.212, 0.701, 0.087),
            _ => (0.2126, 0.7152, 0.0722), // Default to BT.709
        }
    }
}

/// Combined color information
#[derive(Debug, Clone)]
pub struct ColorInfo {
    pub primaries: u8,
    pub transfer: u8,
    pub matrix: u8,
}

/// Convert YUV frame to RGBA pixels
pub fn yuv_to_rgba(
    frame: &Frame,
    color_info: &ColorInfo,
    simd: &SimdOps,
) -> Result<Vec<u8>, AvifError> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    
    let transfer = TransferCharacteristics::from_u8(color_info.transfer);
    let matrix = MatrixCoefficients::from_u8(color_info.matrix);
    
    // Get conversion coefficients
    let (kr, kg, kb) = matrix.coefficients();
    
    // Allocate output buffer
    let mut rgba = vec![0u8; width * height * 4];
    
    let bit_depth = frame.bit_depth;
    let max_value = ((1 << bit_depth) - 1) as f32;
    
    if frame.monochrome {
        // Monochrome: just Y channel
        for y in 0..height {
            for x in 0..width {
                let luma = frame.get_pixel(0, x, y) as f32 / max_value;
                let rgb = transfer_to_srgb(luma, transfer);
                let val = (rgb * 255.0).clamp(0.0, 255.0) as u8;
                
                let idx = (y * width + x) * 4;
                rgba[idx] = val;
                rgba[idx + 1] = val;
                rgba[idx + 2] = val;
                rgba[idx + 3] = 255;
            }
        }
    } else if matrix == MatrixCoefficients::Identity {
        // RGB mode (no conversion needed)
        for y in 0..height {
            for x in 0..width {
                let r = frame.get_pixel(0, x, y) as f32 / max_value;
                let g = frame.get_pixel(1, x, y) as f32 / max_value;
                let b = frame.get_pixel(2, x, y) as f32 / max_value;
                
                let idx = (y * width + x) * 4;
                rgba[idx] = (transfer_to_srgb(r, transfer) * 255.0).clamp(0.0, 255.0) as u8;
                rgba[idx + 1] = (transfer_to_srgb(g, transfer) * 255.0).clamp(0.0, 255.0) as u8;
                rgba[idx + 2] = (transfer_to_srgb(b, transfer) * 255.0).clamp(0.0, 255.0) as u8;
                rgba[idx + 3] = 255;
            }
        }
    } else {
        // Standard YUV to RGB conversion
        convert_yuv_to_rgb(frame, &mut rgba, kr, kg, kb, transfer, simd)?;
    }
    
    Ok(rgba)
}

fn convert_yuv_to_rgb(
    frame: &Frame,
    rgba: &mut [u8],
    kr: f32,
    kg: f32,
    kb: f32,
    transfer: TransferCharacteristics,
    _simd: &SimdOps,
) -> Result<(), AvifError> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let bit_depth = frame.bit_depth;
    
    let max_value = ((1 << bit_depth) - 1) as f32;
    let half_value = max_value / 2.0;
    
    // Conversion matrix coefficients
    // R = Y + (1-Kr)/0.5 * Cr
    // B = Y + (1-Kb)/0.5 * Cb  
    // G = Y - Kr*(1-Kr)/(0.5*Kg) * Cr - Kb*(1-Kb)/(0.5*Kg) * Cb
    
    let cr_to_r = (1.0 - kr) / 0.5;
    let cb_to_b = (1.0 - kb) / 0.5;
    let cr_to_g = -kr * (1.0 - kr) / (0.5 * kg);
    let cb_to_g = -kb * (1.0 - kb) / (0.5 * kg);
    
    let sub_x = frame.subsampling_x as usize;
    let sub_y = frame.subsampling_y as usize;
    
    for y in 0..height {
        for x in 0..width {
            // Get Y value
            let y_val = frame.get_pixel(0, x, y) as f32 / max_value;
            
            // Get chroma values (with subsampling)
            let chroma_x = x >> sub_x;
            let chroma_y = y >> sub_y;
            let cb = (frame.get_pixel(1, chroma_x, chroma_y) as f32 - half_value) / max_value;
            let cr = (frame.get_pixel(2, chroma_x, chroma_y) as f32 - half_value) / max_value;
            
            // Convert to RGB
            let r = y_val + cr_to_r * cr;
            let g = y_val + cr_to_g * cr + cb_to_g * cb;
            let b = y_val + cb_to_b * cb;
            
            // Apply transfer function and clamp
            let r_out = transfer_to_srgb(r.clamp(0.0, 1.0), transfer);
            let g_out = transfer_to_srgb(g.clamp(0.0, 1.0), transfer);
            let b_out = transfer_to_srgb(b.clamp(0.0, 1.0), transfer);
            
            let idx = (y * width + x) * 4;
            rgba[idx] = (r_out * 255.0).clamp(0.0, 255.0) as u8;
            rgba[idx + 1] = (g_out * 255.0).clamp(0.0, 255.0) as u8;
            rgba[idx + 2] = (b_out * 255.0).clamp(0.0, 255.0) as u8;
            rgba[idx + 3] = 255;
        }
    }
    
    Ok(())
}

/// Apply transfer function to convert to sRGB display space
fn transfer_to_srgb(linear: f32, transfer: TransferCharacteristics) -> f32 {
    match transfer {
        TransferCharacteristics::Srgb => {
            // Already sRGB
            linear
        }
        TransferCharacteristics::Linear => {
            // Linear to sRGB gamma
            if linear <= 0.0031308 {
                linear * 12.92
            } else {
                1.055 * linear.powf(1.0 / 2.4) - 0.055
            }
        }
        TransferCharacteristics::Pq => {
            // PQ (ST2084) to SDR with tone mapping
            pq_to_sdr(linear)
        }
        TransferCharacteristics::Hlg => {
            // HLG to SDR with tone mapping
            hlg_to_sdr(linear)
        }
        TransferCharacteristics::Bt709 | 
        TransferCharacteristics::Bt601 |
        TransferCharacteristics::Bt2020_10 |
        TransferCharacteristics::Bt2020_12 => {
            // BT.1886 transfer (gamma 2.4)
            if linear < 0.018 {
                linear * 4.5
            } else {
                1.099 * linear.powf(0.45) - 0.099
            }
        }
        _ => {
            // Default gamma 2.2
            linear.powf(1.0 / 2.2)
        }
    }
}

/// PQ (Perceptual Quantizer) EOTF inverse and tone mapping to SDR
fn pq_to_sdr(pq_value: f32) -> f32 {
    // PQ constants
    const M1: f32 = 0.1593017578125;
    const M2: f32 = 78.84375;
    const C1: f32 = 0.8359375;
    const C2: f32 = 18.8515625;
    const C3: f32 = 18.6875;
    
    // PQ EOTF (inverse of OETF)
    let pq_pow = pq_value.powf(1.0 / M2);
    let numerator = (pq_pow - C1).max(0.0);
    let denominator = C2 - C3 * pq_pow;
    let linear = (numerator / denominator).powf(1.0 / M1);
    
    // Linear light in nits (0-10000)
    let nits = linear * 10000.0;
    
    // Simple tone mapping to SDR (0-100 nits)
    let sdr = reinhard_tone_map(nits, 100.0);
    
    // Apply sRGB gamma
    if sdr <= 0.0031308 {
        sdr * 12.92
    } else {
        1.055 * sdr.powf(1.0 / 2.4) - 0.055
    }
}

/// HLG (Hybrid Log-Gamma) EOTF and tone mapping to SDR
fn hlg_to_sdr(hlg_value: f32) -> f32 {
    // HLG constants
    const A: f32 = 0.17883277;
    const B: f32 = 0.28466892;
    const C: f32 = 0.55991073;
    
    // HLG OETF inverse
    let linear = if hlg_value <= 0.5 {
        (hlg_value * hlg_value) / 3.0
    } else {
        ((hlg_value - C).exp() + B) / (12.0 * A)
    };
    
    // HLG OOTF (system gamma = 1.2 for typical viewing)
    let display = linear.powf(1.2);
    
    // Tone map to SDR range
    let sdr = reinhard_tone_map(display, 1.0);
    
    // Apply sRGB gamma
    if sdr <= 0.0031308 {
        sdr * 12.92
    } else {
        1.055 * sdr.powf(1.0 / 2.4) - 0.055
    }
}

/// Simple Reinhard tone mapping
fn reinhard_tone_map(value: f32, max_luminance: f32) -> f32 {
    let normalized = value / max_luminance;
    normalized / (1.0 + normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_color_primaries() {
        assert_eq!(ColorPrimaries::from_u8(1), ColorPrimaries::Bt709);
        assert_eq!(ColorPrimaries::from_u8(9), ColorPrimaries::Bt2020);
    }
    
    #[test]
    fn test_transfer_characteristics() {
        let pq = TransferCharacteristics::from_u8(16);
        assert_eq!(pq, TransferCharacteristics::Pq);
        assert!(pq.is_hdr());
        
        let srgb = TransferCharacteristics::from_u8(13);
        assert!(!srgb.is_hdr());
    }
    
    #[test]
    fn test_matrix_coefficients() {
        let bt709 = MatrixCoefficients::from_u8(1);
        let (kr, kg, kb) = bt709.coefficients();
        assert!((kr - 0.2126).abs() < 0.001);
        assert!((kg - 0.7152).abs() < 0.001);
        assert!((kb - 0.0722).abs() < 0.001);
    }
    
    #[test]
    fn test_transfer_to_srgb() {
        // Linear to sRGB
        let result = transfer_to_srgb(0.5, TransferCharacteristics::Linear);
        assert!(result > 0.5); // sRGB is brighter in midtones
        
        // sRGB passthrough
        let result = transfer_to_srgb(0.5, TransferCharacteristics::Srgb);
        assert!((result - 0.5).abs() < 0.001);
    }
    
    #[test]
    fn test_tone_mapping() {
        // High luminance should be compressed
        let result = reinhard_tone_map(1000.0, 100.0);
        assert!(result < 1.0);
        assert!(result > 0.9);
        
        // Low luminance should pass through mostly unchanged
        let result = reinhard_tone_map(10.0, 100.0);
        assert!((result - 0.0909).abs() < 0.01);
    }
    
    #[test]
    fn test_yuv_to_rgba_monochrome() {
        let frame = Frame::new(2, 2, 8, 0, 0, true);
        let color_info = ColorInfo {
            primaries: 1,
            transfer: 13,
            matrix: 1,
        };
        let simd = SimdOps::new();
        
        let result = yuv_to_rgba(&frame, &color_info, &simd);
        assert!(result.is_ok());
        
        let rgba = result.unwrap();
        assert_eq!(rgba.len(), 2 * 2 * 4);
    }
}
