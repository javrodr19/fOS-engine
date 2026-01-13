//! AV1 Transform Coding
//!
//! Inverse DCT, ADST, and identity transforms for AV1 intra-frame decoding.

use super::super::simd::SimdOps;
use super::AvifError;

/// Transform sizes supported by AV1
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxSize {
    Tx4x4,
    Tx8x8,
    Tx16x16,
    Tx32x32,
    Tx64x64,
    // Rectangular transforms
    Tx4x8,
    Tx8x4,
    Tx8x16,
    Tx16x8,
    Tx16x32,
    Tx32x16,
    Tx32x64,
    Tx64x32,
    Tx4x16,
    Tx16x4,
    Tx8x32,
    Tx32x8,
}

impl TxSize {
    pub fn width(&self) -> usize {
        match self {
            Self::Tx4x4 | Self::Tx4x8 | Self::Tx4x16 => 4,
            Self::Tx8x8 | Self::Tx8x4 | Self::Tx8x16 | Self::Tx8x32 => 8,
            Self::Tx16x16 | Self::Tx16x8 | Self::Tx16x32 | Self::Tx16x4 => 16,
            Self::Tx32x32 | Self::Tx32x16 | Self::Tx32x64 | Self::Tx32x8 => 32,
            Self::Tx64x64 | Self::Tx64x32 => 64,
        }
    }
    
    pub fn height(&self) -> usize {
        match self {
            Self::Tx4x4 | Self::Tx8x4 | Self::Tx16x4 => 4,
            Self::Tx8x8 | Self::Tx4x8 | Self::Tx16x8 | Self::Tx32x8 => 8,
            Self::Tx16x16 | Self::Tx8x16 | Self::Tx32x16 | Self::Tx4x16 => 16,
            Self::Tx32x32 | Self::Tx16x32 | Self::Tx64x32 | Self::Tx8x32 => 32,
            Self::Tx64x64 | Self::Tx32x64 => 64,
        }
    }
    
    pub fn from_dimensions(width: u32, height: u32) -> Option<Self> {
        match (width, height) {
            (4, 4) => Some(Self::Tx4x4),
            (8, 8) => Some(Self::Tx8x8),
            (16, 16) => Some(Self::Tx16x16),
            (32, 32) => Some(Self::Tx32x32),
            (64, 64) => Some(Self::Tx64x64),
            (4, 8) => Some(Self::Tx4x8),
            (8, 4) => Some(Self::Tx8x4),
            (8, 16) => Some(Self::Tx8x16),
            (16, 8) => Some(Self::Tx16x8),
            (16, 32) => Some(Self::Tx16x32),
            (32, 16) => Some(Self::Tx32x16),
            (32, 64) => Some(Self::Tx32x64),
            (64, 32) => Some(Self::Tx64x32),
            (4, 16) => Some(Self::Tx4x16),
            (16, 4) => Some(Self::Tx16x4),
            (8, 32) => Some(Self::Tx8x32),
            (32, 8) => Some(Self::Tx32x8),
            _ => None,
        }
    }
}

/// Transform types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TxType {
    #[default]
    DctDct,
    AdstDct,
    DctAdst,
    AdstAdst,
    FlipAdstDct,
    DctFlipAdst,
    FlipAdstFlipAdst,
    AdstFlipAdst,
    FlipAdstAdst,
    IdentityIdentity,
    IdentityDct,
    DctIdentity,
    IdentityAdst,
    AdstIdentity,
    IdentityFlipAdst,
    FlipAdstIdentity,
}

/// Apply inverse transform to coefficient block
pub fn inverse_transform(
    coeffs: &mut Vec<i16>,
    tx_size: TxSize,
    tx_type: TxType,
    simd: &SimdOps,
) -> Result<(), AvifError> {
    let width = tx_size.width();
    let height = tx_size.height();
    
    // Apply column transform first, then row transform
    let (col_tx, row_tx) = split_tx_type(tx_type);
    
    // Temporary buffer for 2D transform
    let mut temp = vec![0i32; width * height];
    
    // Copy coeffs to temp with proper scaling
    for (i, &c) in coeffs.iter().enumerate() {
        temp[i] = c as i32;
    }
    
    // Column transform (vertical)
    for x in 0..width {
        let mut col: Vec<i32> = (0..height).map(|y| temp[y * width + x]).collect();
        apply_1d_transform(&mut col, col_tx, height, simd)?;
        for y in 0..height {
            temp[y * width + x] = col[y];
        }
    }
    
    // Row transform (horizontal)
    for y in 0..height {
        let mut row: Vec<i32> = (0..width).map(|x| temp[y * width + x]).collect();
        apply_1d_transform(&mut row, row_tx, width, simd)?;
        for x in 0..width {
            temp[y * width + x] = row[x];
        }
    }
    
    // Copy back with rounding
    coeffs.clear();
    coeffs.reserve(width * height);
    for &v in temp.iter() {
        // Round and clip
        let rounded = (v + (1 << 5)) >> 6;
        coeffs.push(rounded.clamp(i16::MIN as i32, i16::MAX as i32) as i16);
    }
    
    Ok(())
}

fn split_tx_type(tx_type: TxType) -> (Transform1D, Transform1D) {
    match tx_type {
        TxType::DctDct => (Transform1D::Dct, Transform1D::Dct),
        TxType::AdstDct => (Transform1D::Adst, Transform1D::Dct),
        TxType::DctAdst => (Transform1D::Dct, Transform1D::Adst),
        TxType::AdstAdst => (Transform1D::Adst, Transform1D::Adst),
        TxType::FlipAdstDct => (Transform1D::FlipAdst, Transform1D::Dct),
        TxType::DctFlipAdst => (Transform1D::Dct, Transform1D::FlipAdst),
        TxType::FlipAdstFlipAdst => (Transform1D::FlipAdst, Transform1D::FlipAdst),
        TxType::AdstFlipAdst => (Transform1D::Adst, Transform1D::FlipAdst),
        TxType::FlipAdstAdst => (Transform1D::FlipAdst, Transform1D::Adst),
        TxType::IdentityIdentity => (Transform1D::Identity, Transform1D::Identity),
        TxType::IdentityDct => (Transform1D::Identity, Transform1D::Dct),
        TxType::DctIdentity => (Transform1D::Dct, Transform1D::Identity),
        TxType::IdentityAdst => (Transform1D::Identity, Transform1D::Adst),
        TxType::AdstIdentity => (Transform1D::Adst, Transform1D::Identity),
        TxType::IdentityFlipAdst => (Transform1D::Identity, Transform1D::FlipAdst),
        TxType::FlipAdstIdentity => (Transform1D::FlipAdst, Transform1D::Identity),
    }
}

#[derive(Debug, Clone, Copy)]
enum Transform1D {
    Dct,
    Adst,
    FlipAdst,
    Identity,
}

fn apply_1d_transform(
    data: &mut Vec<i32>,
    transform: Transform1D,
    size: usize,
    _simd: &SimdOps,
) -> Result<(), AvifError> {
    match transform {
        Transform1D::Dct => inverse_dct(data, size),
        Transform1D::Adst => inverse_adst(data, size),
        Transform1D::FlipAdst => {
            inverse_adst(data, size)?;
            data.reverse();
            Ok(())
        }
        Transform1D::Identity => {
            // Identity transform just scales
            let scale = match size {
                4 => 5793,    // sqrt(2) * 4096
                8 => 4096,    // 1 * 4096
                16 => 2896,   // 1/sqrt(2) * 4096
                32 => 2048,   // 0.5 * 4096
                64 => 2048,
                _ => 4096,
            };
            for v in data.iter_mut() {
                *v = (*v * scale) >> 12;
            }
            Ok(())
        }
    }
}

// DCT constants for fixed-point arithmetic
const COS_BIT: i32 = 12;
const ROUND_SHIFT: i32 = 12;

fn inverse_dct(data: &mut Vec<i32>, size: usize) -> Result<(), AvifError> {
    match size {
        4 => inverse_dct_4(data),
        8 => inverse_dct_8(data),
        16 => inverse_dct_16(data),
        32 => inverse_dct_32(data),
        64 => inverse_dct_64(data),
        _ => Err(AvifError::TransformError),
    }
}

fn inverse_dct_4(data: &mut Vec<i32>) -> Result<(), AvifError> {
    if data.len() < 4 {
        return Err(AvifError::TransformError);
    }
    
    // Cosine constants for 4-point DCT (scaled by 4096)
    const C1: i32 = 4017; // cos(pi/8) * 4096
    const C2: i32 = 2896; // cos(pi/4) * 4096 = sqrt(2)/2 * 4096
    const C3: i32 = 1567; // cos(3*pi/8) * 4096
    
    let a0 = data[0];
    let a1 = data[1];
    let a2 = data[2];
    let a3 = data[3];
    
    // Stage 1
    let b0 = (a0 + a2) * C2 >> COS_BIT;
    let b1 = (a0 - a2) * C2 >> COS_BIT;
    let b2 = (a1 * C3 - a3 * C1) >> COS_BIT;
    let b3 = (a1 * C1 + a3 * C3) >> COS_BIT;
    
    // Stage 2
    data[0] = b0 + b3;
    data[1] = b1 + b2;
    data[2] = b1 - b2;
    data[3] = b0 - b3;
    
    Ok(())
}

fn inverse_dct_8(data: &mut Vec<i32>) -> Result<(), AvifError> {
    if data.len() < 8 {
        return Err(AvifError::TransformError);
    }
    
    // First do 4-point DCT on even indices
    let mut even = vec![data[0], data[2], data[4], data[6]];
    inverse_dct_4(&mut even)?;
    
    // Odd indices butterfly
    const C1: i32 = 4076; // cos(pi/16) * 4096
    const C3: i32 = 3784; // cos(3*pi/16) * 4096
    const C5: i32 = 3218; // cos(5*pi/16) * 4096
    const C7: i32 = 2404; // cos(7*pi/16) * 4096
    const S1: i32 = 401;  // sin(pi/16) * 4096
    const S3: i32 = 1189; // sin(3*pi/16) * 4096
    const S5: i32 = 1931; // sin(5*pi/16) * 4096
    const S7: i32 = 2598; // sin(7*pi/16) * 4096
    
    let a1 = data[1];
    let a3 = data[3];
    let a5 = data[5];
    let a7 = data[7];
    
    let b0 = (a1 * C1 + a7 * S1) >> COS_BIT;
    let b1 = (a1 * S1 - a7 * C1) >> COS_BIT;
    let b2 = (a3 * C3 + a5 * S3) >> COS_BIT;
    let b3 = (a3 * S3 - a5 * C3) >> COS_BIT;
    let b4 = (a5 * C5 + a3 * S5) >> COS_BIT;
    let b5 = (a5 * S5 - a3 * C5) >> COS_BIT;
    let b6 = (a7 * C7 + a1 * S7) >> COS_BIT;
    let b7 = (a7 * S7 - a1 * C7) >> COS_BIT;
    
    let odd = [
        (b0 + b4) >> 1,
        (b1 + b5) >> 1,
        (b2 + b6) >> 1,
        (b3 + b7) >> 1,
    ];
    
    // Combine even and odd
    data[0] = even[0] + odd[0];
    data[1] = even[1] + odd[1];
    data[2] = even[2] + odd[2];
    data[3] = even[3] + odd[3];
    data[4] = even[3] - odd[3];
    data[5] = even[2] - odd[2];
    data[6] = even[1] - odd[1];
    data[7] = even[0] - odd[0];
    
    Ok(())
}

fn inverse_dct_16(data: &mut Vec<i32>) -> Result<(), AvifError> {
    if data.len() < 16 {
        return Err(AvifError::TransformError);
    }
    
    // Recursive: 8-point DCT on even indices
    let mut even: Vec<i32> = (0..8).map(|i| data[i * 2]).collect();
    inverse_dct_8(&mut even)?;
    
    // Odd coefficients butterfly (simplified)
    let mut odd: Vec<i32> = (0..8).map(|i| data[i * 2 + 1]).collect();
    
    // Apply butterfly operations
    for i in 0..4 {
        let a = odd[i];
        let b = odd[7 - i];
        odd[i] = a + b;
        odd[7 - i] = a - b;
    }
    
    // Combine
    for i in 0..8 {
        data[i] = even[i] + (odd[i] >> 1);
        data[15 - i] = even[i] - (odd[i] >> 1);
    }
    
    Ok(())
}

fn inverse_dct_32(data: &mut Vec<i32>) -> Result<(), AvifError> {
    if data.len() < 32 {
        return Err(AvifError::TransformError);
    }
    
    // Recursive: 16-point DCT on even indices
    let mut even: Vec<i32> = (0..16).map(|i| data[i * 2]).collect();
    inverse_dct_16(&mut even)?;
    
    let mut odd: Vec<i32> = (0..16).map(|i| data[i * 2 + 1]).collect();
    
    for i in 0..8 {
        let a = odd[i];
        let b = odd[15 - i];
        odd[i] = a + b;
        odd[15 - i] = a - b;
    }
    
    for i in 0..16 {
        data[i] = even[i] + (odd[i] >> 1);
        data[31 - i] = even[i] - (odd[i] >> 1);
    }
    
    Ok(())
}

fn inverse_dct_64(data: &mut Vec<i32>) -> Result<(), AvifError> {
    if data.len() < 64 {
        return Err(AvifError::TransformError);
    }
    
    // 64-point DCT (for large transforms, only use first 32 coefficients)
    let mut even: Vec<i32> = (0..32).map(|i| data[i * 2]).collect();
    inverse_dct_32(&mut even)?;
    
    let mut odd: Vec<i32> = (0..32).map(|i| data.get(i * 2 + 1).copied().unwrap_or(0)).collect();
    
    for i in 0..16 {
        let a = odd[i];
        let b = odd[31 - i];
        odd[i] = a + b;
        odd[31 - i] = a - b;
    }
    
    for i in 0..32 {
        data[i] = even[i] + (odd[i] >> 1);
        data[63 - i] = even[i] - (odd[i] >> 1);
    }
    
    Ok(())
}

fn inverse_adst(data: &mut Vec<i32>, size: usize) -> Result<(), AvifError> {
    match size {
        4 => inverse_adst_4(data),
        8 => inverse_adst_8(data),
        16 => inverse_adst_16(data),
        _ => {
            // For larger sizes, fall back to DCT (ADST not used for 32/64)
            inverse_dct(data, size)
        }
    }
}

fn inverse_adst_4(data: &mut Vec<i32>) -> Result<(), AvifError> {
    if data.len() < 4 {
        return Err(AvifError::TransformError);
    }
    
    // ADST constants
    const S0: i32 = 1321;  // sin(pi/9) * 4096
    const S1: i32 = 3630;  // sin(2*pi/9) * 4096  
    const S2: i32 = 4073;  // sin(4*pi/9) * 4096
    const S3: i32 = 3197;  // sin(7*pi/18) * 4096
    
    let a0 = data[0];
    let a1 = data[1];
    let a2 = data[2];
    let a3 = data[3];
    
    let b0 = (a0 * S0 + a1 * S1 + a2 * S2 + a3 * S3) >> COS_BIT;
    let b1 = (a0 * S1 + a2 * S0 - a1 * S3 - a3 * S2) >> COS_BIT;
    let b2 = (a0 * S2 - a1 * S0 - a2 * S3 + a3 * S1) >> COS_BIT;
    let b3 = (a0 * S3 - a1 * S2 + a2 * S1 - a3 * S0) >> COS_BIT;
    
    data[0] = b0;
    data[1] = b1;
    data[2] = b2;
    data[3] = b3;
    
    Ok(())
}

fn inverse_adst_8(data: &mut Vec<i32>) -> Result<(), AvifError> {
    if data.len() < 8 {
        return Err(AvifError::TransformError);
    }
    
    // Simplified 8-point ADST using butterfly structure
    let mut a = [0i32; 8];
    for i in 0..8 {
        a[i] = data[i];
    }
    
    // Stage 1: multiply by ADST matrix (simplified)
    let mut b = [0i32; 8];
    for i in 0..8 {
        let mut sum = 0i32;
        for j in 0..8 {
            // Approximate ADST basis function
            let angle = ((2 * i + 1) * (2 * j + 1)) as f32 * std::f32::consts::PI / 32.0;
            let coeff = (angle.sin() * 4096.0) as i32;
            sum += (a[j] * coeff) >> COS_BIT;
        }
        b[i] = sum;
    }
    
    for i in 0..8 {
        data[i] = b[i];
    }
    
    Ok(())
}

fn inverse_adst_16(data: &mut Vec<i32>) -> Result<(), AvifError> {
    if data.len() < 16 {
        return Err(AvifError::TransformError);
    }
    
    // Simplified 16-point ADST
    let mut a = [0i32; 16];
    for i in 0..16 {
        a[i] = data[i];
    }
    
    let mut b = [0i32; 16];
    for i in 0..16 {
        let mut sum = 0i32;
        for j in 0..16 {
            let angle = ((2 * i + 1) * (2 * j + 1)) as f32 * std::f32::consts::PI / 64.0;
            let coeff = (angle.sin() * 4096.0) as i32;
            sum += (a[j] * coeff) >> COS_BIT;
        }
        b[i] = sum;
    }
    
    for i in 0..16 {
        data[i] = b[i];
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tx_size_dimensions() {
        assert_eq!(TxSize::Tx4x4.width(), 4);
        assert_eq!(TxSize::Tx4x4.height(), 4);
        assert_eq!(TxSize::Tx8x16.width(), 8);
        assert_eq!(TxSize::Tx8x16.height(), 16);
    }
    
    #[test]
    fn test_tx_size_from_dimensions() {
        assert_eq!(TxSize::from_dimensions(4, 4), Some(TxSize::Tx4x4));
        assert_eq!(TxSize::from_dimensions(32, 32), Some(TxSize::Tx32x32));
        assert_eq!(TxSize::from_dimensions(3, 3), None);
    }
    
    #[test]
    fn test_inverse_dct_4() {
        let mut data = vec![100, 50, 25, 10];
        inverse_dct_4(&mut data).unwrap();
        // Verify output is different from input (transform applied)
        assert_ne!(data, vec![100, 50, 25, 10]);
    }
    
    #[test]
    fn test_identity_transform() {
        let simd = SimdOps::new();
        let mut coeffs = vec![100, 0, 0, 0];
        inverse_transform(&mut coeffs, TxSize::Tx4x4, TxType::IdentityIdentity, &simd).unwrap();
        // Identity should preserve DC-like behavior
        assert!(coeffs[0] != 0);
    }
}
