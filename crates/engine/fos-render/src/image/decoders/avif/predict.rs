//! AV1 Intra Prediction
//!
//! Implements all intra prediction modes for AV1 decoding.

use super::AvifError;

/// Intra prediction modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntraMode {
    #[default]
    DcPred,
    VPred,
    HPred,
    D45Pred,
    D135Pred,
    D113Pred,
    D157Pred,
    D203Pred,
    D67Pred,
    SmoothPred,
    SmoothVPred,
    SmoothHPred,
    PaethPred,
    CflPred,       // Chroma from luma
}

impl IntraMode {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::DcPred,
            1 => Self::VPred,
            2 => Self::HPred,
            3 => Self::D45Pred,
            4 => Self::D135Pred,
            5 => Self::D113Pred,
            6 => Self::D157Pred,
            7 => Self::D203Pred,
            8 => Self::D67Pred,
            9 => Self::SmoothPred,
            10 => Self::SmoothVPred,
            11 => Self::SmoothHPred,
            12 => Self::PaethPred,
            _ => Self::DcPred,
        }
    }
    
    /// Get angular direction (in units of 1/8 degree) or None for non-directional modes
    pub fn angle(&self) -> Option<i32> {
        match self {
            Self::VPred => Some(90 * 8),
            Self::HPred => Some(180 * 8),
            Self::D45Pred => Some(45 * 8),
            Self::D135Pred => Some(135 * 8),
            Self::D113Pred => Some(113 * 8),
            Self::D157Pred => Some(157 * 8),
            Self::D203Pred => Some(203 * 8),
            Self::D67Pred => Some(67 * 8),
            _ => None,
        }
    }
}

/// Apply intra prediction to a block
pub fn intra_predict(
    mode: IntraMode,
    output: &mut [i16],
    width: u32,
    height: u32,
    top: &[i16],
    left: &[i16],
    top_left: i16,
    bit_depth: u8,
) -> Result<(), AvifError> {
    let w = width as usize;
    let h = height as usize;
    
    if output.len() < w * h {
        return Err(AvifError::PredictionError);
    }
    
    match mode {
        IntraMode::DcPred => predict_dc(output, w, h, top, left)?,
        IntraMode::VPred => predict_vertical(output, w, h, top)?,
        IntraMode::HPred => predict_horizontal(output, w, h, left)?,
        IntraMode::D45Pred => predict_directional(output, w, h, top, left, top_left, 45)?,
        IntraMode::D135Pred => predict_directional(output, w, h, top, left, top_left, 135)?,
        IntraMode::D113Pred => predict_directional(output, w, h, top, left, top_left, 113)?,
        IntraMode::D157Pred => predict_directional(output, w, h, top, left, top_left, 157)?,
        IntraMode::D203Pred => predict_directional(output, w, h, top, left, top_left, 203)?,
        IntraMode::D67Pred => predict_directional(output, w, h, top, left, top_left, 67)?,
        IntraMode::SmoothPred => predict_smooth(output, w, h, top, left, bit_depth)?,
        IntraMode::SmoothVPred => predict_smooth_v(output, w, h, top, left, bit_depth)?,
        IntraMode::SmoothHPred => predict_smooth_h(output, w, h, top, left, bit_depth)?,
        IntraMode::PaethPred => predict_paeth(output, w, h, top, left, top_left)?,
        IntraMode::CflPred => predict_dc(output, w, h, top, left)?, // Base DC for CFL
    }
    
    Ok(())
}

/// Chroma-from-luma prediction
pub fn cfl_predict(
    output: &mut [i16],
    width: u32,
    height: u32,
    top: &[i16],
    left: &[i16],
    top_left: i16,
    alpha: i16,
    luma_avg: i32,
    _bit_depth: u8,
) -> Result<(), AvifError> {
    let w = width as usize;
    let h = height as usize;
    
    // First apply DC prediction as base
    predict_dc(output, w, h, top, left)?;
    
    // Then apply CFL adjustment
    let chroma_dc: i32 = output.iter().map(|&v| v as i32).sum::<i32>() / (w * h) as i32;
    
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            // CFL: chroma = DC + alpha * (luma - luma_avg) / 8
            let adjustment = (alpha as i32 * (luma_avg - chroma_dc)) >> 6;
            output[idx] = (output[idx] as i32 + adjustment).clamp(0, (1 << 16) - 1) as i16;
        }
    }
    
    Ok(())
}

fn predict_dc(
    output: &mut [i16],
    width: usize,
    height: usize,
    top: &[i16],
    left: &[i16],
) -> Result<(), AvifError> {
    let mut sum: i32 = 0;
    let mut count = 0;
    
    // Sum top neighbors
    for i in 0..width.min(top.len()) {
        sum += top[i] as i32;
        count += 1;
    }
    
    // Sum left neighbors  
    for i in 0..height.min(left.len()) {
        sum += left[i] as i32;
        count += 1;
    }
    
    let dc = if count > 0 {
        ((sum + count / 2) / count) as i16
    } else {
        128 // Default for 8-bit
    };
    
    // Fill entire block with DC value
    for pixel in output.iter_mut().take(width * height) {
        *pixel = dc;
    }
    
    Ok(())
}

fn predict_vertical(
    output: &mut [i16],
    width: usize,
    height: usize,
    top: &[i16],
) -> Result<(), AvifError> {
    for y in 0..height {
        for x in 0..width {
            output[y * width + x] = top.get(x).copied().unwrap_or(128);
        }
    }
    Ok(())
}

fn predict_horizontal(
    output: &mut [i16],
    width: usize,
    height: usize,
    left: &[i16],
) -> Result<(), AvifError> {
    for y in 0..height {
        let val = left.get(y).copied().unwrap_or(128);
        for x in 0..width {
            output[y * width + x] = val;
        }
    }
    Ok(())
}

fn predict_directional(
    output: &mut [i16],
    width: usize,
    height: usize,
    top: &[i16],
    left: &[i16],
    top_left: i16,
    angle: i32,
) -> Result<(), AvifError> {
    // Convert angle to dx, dy for interpolation
    let angle_rad = (angle as f32) * std::f32::consts::PI / 180.0;
    let dx = angle_rad.cos();
    let dy = angle_rad.sin();
    
    for y in 0..height {
        for x in 0..width {
            // Project back along the direction to find source sample
            let src_x = x as f32 - (y as f32 * dx / dy.abs().max(0.001));
            let src_y = y as f32 - (x as f32 * dy / dx.abs().max(0.001));
            
            let val = if angle < 90 {
                // Top reference
                let idx = src_x.round() as i32;
                if idx >= 0 && (idx as usize) < top.len() {
                    top[idx as usize]
                } else if idx == -1 {
                    top_left
                } else {
                    top.get(0).copied().unwrap_or(128)
                }
            } else if angle < 180 {
                // Mix of top and left
                if src_y < 0.0 {
                    let idx = src_x.round().max(0.0) as usize;
                    top.get(idx).copied().unwrap_or(top_left)
                } else {
                    let idx = src_y.round().max(0.0) as usize;
                    left.get(idx).copied().unwrap_or(top_left)
                }
            } else {
                // Left reference
                let idx = src_y.round() as i32;
                if idx >= 0 && (idx as usize) < left.len() {
                    left[idx as usize]
                } else if idx == -1 {
                    top_left
                } else {
                    left.get(0).copied().unwrap_or(128)
                }
            };
            
            output[y * width + x] = val;
        }
    }
    
    Ok(())
}

fn predict_smooth(
    output: &mut [i16],
    width: usize,
    height: usize,
    top: &[i16],
    left: &[i16],
    _bit_depth: u8,
) -> Result<(), AvifError> {
    // Get corner values
    let top_right = top.get(width - 1).copied().unwrap_or(128);
    let bottom_left = left.get(height - 1).copied().unwrap_or(128);
    
    for y in 0..height {
        for x in 0..width {
            // Smooth weights based on distance to edges
            let weight_y = height - 1 - y;
            let weight_x = width - 1 - x;
            
            let v_top = top.get(x).copied().unwrap_or(128) as i32;
            let v_left = left.get(y).copied().unwrap_or(128) as i32;
            
            // Bilinear smooth interpolation
            let smooth_v = (v_top * weight_y as i32 + bottom_left as i32 * y as i32) / height as i32;
            let smooth_h = (v_left * weight_x as i32 + top_right as i32 * x as i32) / width as i32;
            
            output[y * width + x] = ((smooth_v + smooth_h + 1) / 2) as i16;
        }
    }
    
    Ok(())
}

fn predict_smooth_v(
    output: &mut [i16],
    width: usize,
    height: usize,
    top: &[i16],
    left: &[i16],
    _bit_depth: u8,
) -> Result<(), AvifError> {
    let bottom_left = left.get(height - 1).copied().unwrap_or(128);
    
    for y in 0..height {
        let weight = height - 1 - y;
        for x in 0..width {
            let v_top = top.get(x).copied().unwrap_or(128) as i32;
            let val = (v_top * weight as i32 + bottom_left as i32 * y as i32) / height as i32;
            output[y * width + x] = val as i16;
        }
    }
    
    Ok(())
}

fn predict_smooth_h(
    output: &mut [i16],
    width: usize,
    height: usize,
    top: &[i16],
    left: &[i16],
    _bit_depth: u8,
) -> Result<(), AvifError> {
    let top_right = top.get(width - 1).copied().unwrap_or(128);
    
    for y in 0..height {
        let v_left = left.get(y).copied().unwrap_or(128) as i32;
        for x in 0..width {
            let weight = width - 1 - x;
            let val = (v_left * weight as i32 + top_right as i32 * x as i32) / width as i32;
            output[y * width + x] = val as i16;
        }
    }
    
    Ok(())
}

fn predict_paeth(
    output: &mut [i16],
    width: usize,
    height: usize,
    top: &[i16],
    left: &[i16],
    top_left: i16,
) -> Result<(), AvifError> {
    for y in 0..height {
        for x in 0..width {
            let t = top.get(x).copied().unwrap_or(128) as i32;
            let l = left.get(y).copied().unwrap_or(128) as i32;
            let tl = top_left as i32;
            
            // Paeth predictor
            let base = t + l - tl;
            let pt = (base - t).abs();
            let pl = (base - l).abs();
            let ptl = (base - tl).abs();
            
            let val = if pt <= pl && pt <= ptl {
                t
            } else if pl <= ptl {
                l
            } else {
                tl
            };
            
            output[y * width + x] = val as i16;
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_intra_mode_from_u8() {
        assert_eq!(IntraMode::from_u8(0), IntraMode::DcPred);
        assert_eq!(IntraMode::from_u8(1), IntraMode::VPred);
        assert_eq!(IntraMode::from_u8(12), IntraMode::PaethPred);
    }
    
    #[test]
    fn test_dc_prediction() {
        let mut output = vec![0i16; 16];
        let top = vec![100i16; 4];
        let left = vec![100i16; 4];
        
        predict_dc(&mut output, 4, 4, &top, &left).unwrap();
        
        // All values should be 100 (average of constants)
        assert!(output.iter().all(|&v| v == 100));
    }
    
    #[test]
    fn test_vertical_prediction() {
        let mut output = vec![0i16; 16];
        let top = vec![10i16, 20, 30, 40];
        let left = vec![0i16; 4];
        
        predict_vertical(&mut output, 4, 4, &top).unwrap();
        
        // Each column should match top
        for y in 0..4 {
            assert_eq!(output[y * 4 + 0], 10);
            assert_eq!(output[y * 4 + 1], 20);
            assert_eq!(output[y * 4 + 2], 30);
            assert_eq!(output[y * 4 + 3], 40);
        }
    }
    
    #[test]
    fn test_horizontal_prediction() {
        let mut output = vec![0i16; 16];
        let top = vec![0i16; 4];
        let left = vec![10i16, 20, 30, 40];
        
        predict_horizontal(&mut output, 4, 4, &left).unwrap();
        
        // Each row should match left
        for x in 0..4 {
            assert_eq!(output[0 * 4 + x], 10);
            assert_eq!(output[1 * 4 + x], 20);
            assert_eq!(output[2 * 4 + x], 30);
            assert_eq!(output[3 * 4 + x], 40);
        }
    }
    
    #[test]
    fn test_paeth_prediction() {
        let mut output = vec![0i16; 4];
        let top = vec![100i16, 100];
        let left = vec![100i16, 100];
        let top_left = 100i16;
        
        predict_paeth(&mut output, 2, 2, &top, &left, top_left).unwrap();
        
        // With all same values, Paeth should predict same value
        assert!(output.iter().all(|&v| v == 100));
    }
}
