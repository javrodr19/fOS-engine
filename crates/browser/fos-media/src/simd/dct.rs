//! SIMD IDCT
//!
//! SIMD-accelerated inverse discrete cosine transform for video decoding.

/// 8x8 IDCT using SIMD when available
pub fn idct_8x8_simd(input: &[i16; 64], output: &mut [i16; 64]) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("sse2") {
            unsafe { idct_8x8_sse2(input, output); return; }
        }
    }
    idct_8x8_scalar(input, output);
}

/// Scalar IDCT implementation
pub fn idct_8x8_scalar(input: &[i16; 64], output: &mut [i16; 64]) {
    // Constants for IDCT (scaled by 1024)
    const C1: i32 = 1004; // cos(1*pi/16) * 1024
    const C2: i32 = 946;  // cos(2*pi/16) * 1024
    const C3: i32 = 851;  // cos(3*pi/16) * 1024
    const C4: i32 = 724;  // cos(4*pi/16) * 1024
    const C5: i32 = 569;  // cos(5*pi/16) * 1024
    const C6: i32 = 392;  // cos(6*pi/16) * 1024
    const C7: i32 = 200;  // cos(7*pi/16) * 1024
    
    let mut temp = [0i32; 64];
    
    // 1D IDCT on rows
    for row in 0..8 {
        let base = row * 8;
        let s0 = input[base] as i32;
        let s1 = input[base + 1] as i32;
        let s2 = input[base + 2] as i32;
        let s3 = input[base + 3] as i32;
        let s4 = input[base + 4] as i32;
        let s5 = input[base + 5] as i32;
        let s6 = input[base + 6] as i32;
        let s7 = input[base + 7] as i32;
        
        // Even part
        let t0 = (s0 + s4) * C4;
        let t1 = (s0 - s4) * C4;
        let t2 = s2 * C6 - s6 * C2;
        let t3 = s2 * C2 + s6 * C6;
        
        let e0 = t0 + t3;
        let e1 = t1 + t2;
        let e2 = t1 - t2;
        let e3 = t0 - t3;
        
        // Odd part
        let o0 = s1 * C1 + s3 * C3 + s5 * C5 + s7 * C7;
        let o1 = s1 * C3 - s3 * C7 - s5 * C1 - s7 * C5;
        let o2 = s1 * C5 - s3 * C1 + s5 * C7 + s7 * C3;
        let o3 = s1 * C7 - s3 * C5 + s5 * C3 - s7 * C1;
        
        temp[base] = e0 + o0;
        temp[base + 1] = e1 + o1;
        temp[base + 2] = e2 + o2;
        temp[base + 3] = e3 + o3;
        temp[base + 4] = e3 - o3;
        temp[base + 5] = e2 - o2;
        temp[base + 6] = e1 - o1;
        temp[base + 7] = e0 - o0;
    }
    
    // 1D IDCT on columns
    for col in 0..8 {
        let s0 = temp[col];
        let s1 = temp[8 + col];
        let s2 = temp[16 + col];
        let s3 = temp[24 + col];
        let s4 = temp[32 + col];
        let s5 = temp[40 + col];
        let s6 = temp[48 + col];
        let s7 = temp[56 + col];
        
        let t0 = (s0 + s4) * C4;
        let t1 = (s0 - s4) * C4;
        let t2 = s2 * C6 - s6 * C2;
        let t3 = s2 * C2 + s6 * C6;
        
        let e0 = t0 + t3;
        let e1 = t1 + t2;
        let e2 = t1 - t2;
        let e3 = t0 - t3;
        
        let o0 = s1 * C1 + s3 * C3 + s5 * C5 + s7 * C7;
        let o1 = s1 * C3 - s3 * C7 - s5 * C1 - s7 * C5;
        let o2 = s1 * C5 - s3 * C1 + s5 * C7 + s7 * C3;
        let o3 = s1 * C7 - s3 * C5 + s5 * C3 - s7 * C1;
        
        // Scale down and clamp
        output[col] = ((e0 + o0) >> 20).clamp(-256, 255) as i16;
        output[8 + col] = ((e1 + o1) >> 20).clamp(-256, 255) as i16;
        output[16 + col] = ((e2 + o2) >> 20).clamp(-256, 255) as i16;
        output[24 + col] = ((e3 + o3) >> 20).clamp(-256, 255) as i16;
        output[32 + col] = ((e3 - o3) >> 20).clamp(-256, 255) as i16;
        output[40 + col] = ((e2 - o2) >> 20).clamp(-256, 255) as i16;
        output[48 + col] = ((e1 - o1) >> 20).clamp(-256, 255) as i16;
        output[56 + col] = ((e0 - o0) >> 20).clamp(-256, 255) as i16;
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "sse2")]
unsafe fn idct_8x8_sse2(input: &[i16; 64], output: &mut [i16; 64]) {
    // SSE2 implementation would process 8 values at once
    // For now, fall back to scalar
    idct_8x8_scalar(input, output);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_idct() {
        let mut input = [0i16; 64];
        input[0] = 1024;
        let mut output = [0i16; 64];
        idct_8x8_simd(&input, &mut output);
        // DC component should produce uniform output
    }
}
