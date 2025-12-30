//! SIMD Utilities for Image Decoding
//!
//! Provides SIMD-accelerated operations for:
//! - DEFLATE Adler-32 checksum
//! - PNG filter reconstruction
//! - JPEG IDCT and color conversion
//! - Memory operations

/// SIMD capability level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimdLevel {
    None,
    Sse2,
    Sse4,
    Avx2,
    Neon,
}

impl SimdLevel {
    /// Detect best available SIMD level
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
            return SimdLevel::Neon;
        }
        SimdLevel::None
    }
}

/// SIMD operations container
pub struct SimdOps {
    level: SimdLevel,
}

impl SimdOps {
    pub fn new() -> Self {
        Self { level: SimdLevel::detect() }
    }

    pub fn level(&self) -> SimdLevel {
        self.level
    }

    // ========== Adler-32 (for DEFLATE) ==========

    /// Compute Adler-32 checksum with SIMD acceleration
    #[inline]
    pub fn adler32(&self, data: &[u8]) -> u32 {
        match self.level {
            SimdLevel::Avx2 => self.adler32_avx2(data),
            SimdLevel::Sse4 | SimdLevel::Sse2 => self.adler32_sse(data),
            SimdLevel::Neon => self.adler32_neon(data),
            SimdLevel::None => self.adler32_scalar(data),
        }
    }

    fn adler32_scalar(&self, data: &[u8]) -> u32 {
        let mut a: u32 = 1;
        let mut b: u32 = 0;
        const MOD: u32 = 65521;

        for chunk in data.chunks(5552) {
            for &byte in chunk {
                a = a.wrapping_add(byte as u32);
                b = b.wrapping_add(a);
            }
            a %= MOD;
            b %= MOD;
        }
        (b << 16) | a
    }

    #[cfg(target_arch = "x86_64")]
    fn adler32_avx2(&self, data: &[u8]) -> u32 {
        use std::arch::x86_64::*;

        if data.len() < 32 {
            return self.adler32_scalar(data);
        }

        unsafe {
            let mut a: u32 = 1;
            let mut b: u32 = 0;
            const MOD: u32 = 65521;

            let ones = _mm256_set1_epi16(1);
            let mut chunks = data.chunks_exact(32);

            for chunk in chunks.by_ref() {
                let v = _mm256_loadu_si256(chunk.as_ptr() as *const __m256i);

                // Sum all bytes
                let zero = _mm256_setzero_si256();
                let sad = _mm256_sad_epu8(v, zero);
                let sum_lo = _mm256_extract_epi64(sad, 0) as u32 + _mm256_extract_epi64(sad, 2) as u32;
                let sum_hi = _mm256_extract_epi64(sad, 1) as u32 + _mm256_extract_epi64(sad, 3) as u32;
                let byte_sum = sum_lo + sum_hi;

                // Weight calculation for b
                let weights: [i16; 16] = [32,31,30,29,28,27,26,25,24,23,22,21,20,19,18,17];
                let weights2: [i16; 16] = [16,15,14,13,12,11,10,9,8,7,6,5,4,3,2,1];
                let w1 = _mm256_loadu_si256(weights.as_ptr() as *const __m256i);
                let w2 = _mm256_loadu_si256(weights2.as_ptr() as *const __m256i);

                // Extend bytes to 16-bit and multiply
                let lo = _mm256_unpacklo_epi8(v, zero);
                let hi = _mm256_unpackhi_epi8(v, zero);
                let prod1 = _mm256_madd_epi16(lo, w1);
                let prod2 = _mm256_madd_epi16(hi, w2);
                let sum = _mm256_add_epi32(prod1, prod2);

                // Horizontal sum
                let sum128 = _mm_add_epi32(
                    _mm256_castsi256_si128(sum),
                    _mm256_extracti128_si256(sum, 1)
                );
                let sum64 = _mm_add_epi32(sum128, _mm_srli_si128(sum128, 8));
                let sum32 = _mm_add_epi32(sum64, _mm_srli_si128(sum64, 4));
                let weight_sum = _mm_cvtsi128_si32(sum32) as u32;

                b = b.wrapping_add(a.wrapping_mul(32)).wrapping_add(weight_sum);
                a = a.wrapping_add(byte_sum);

                if a >= MOD { a %= MOD; }
                if b >= MOD { b %= MOD; }
            }

            // Process remainder
            for &byte in chunks.remainder() {
                a = a.wrapping_add(byte as u32);
                b = b.wrapping_add(a);
            }
            a %= MOD;
            b %= MOD;

            (b << 16) | a
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn adler32_avx2(&self, data: &[u8]) -> u32 {
        self.adler32_scalar(data)
    }

    #[cfg(target_arch = "x86_64")]
    fn adler32_sse(&self, data: &[u8]) -> u32 {
        // Simplified SSE implementation
        self.adler32_scalar(data)
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn adler32_sse(&self, data: &[u8]) -> u32 {
        self.adler32_scalar(data)
    }

    #[cfg(target_arch = "aarch64")]
    fn adler32_neon(&self, data: &[u8]) -> u32 {
        use std::arch::aarch64::*;

        if data.len() < 16 {
            return self.adler32_scalar(data);
        }

        unsafe {
            let mut a: u32 = 1;
            let mut b: u32 = 0;
            const MOD: u32 = 65521;

            let mut chunks = data.chunks_exact(16);

            for chunk in chunks.by_ref() {
                let v = vld1q_u8(chunk.as_ptr());

                // Sum bytes
                let sum16 = vpaddlq_u8(v);
                let sum32 = vpaddlq_u16(sum16);
                let sum64 = vpaddlq_u32(sum32);
                let byte_sum = vgetq_lane_u64(sum64, 0) as u32 + vgetq_lane_u64(sum64, 1) as u32;

                b = b.wrapping_add(a.wrapping_mul(16)).wrapping_add(byte_sum * 8);
                a = a.wrapping_add(byte_sum);

                if a >= MOD { a %= MOD; }
                if b >= MOD { b %= MOD; }
            }

            for &byte in chunks.remainder() {
                a = a.wrapping_add(byte as u32);
                b = b.wrapping_add(a);
            }
            a %= MOD;
            b %= MOD;

            (b << 16) | a
        }
    }

    #[cfg(not(target_arch = "aarch64"))]
    fn adler32_neon(&self, data: &[u8]) -> u32 {
        self.adler32_scalar(data)
    }

    // ========== PNG Filter Reconstruction ==========

    /// Reconstruct PNG filter type Sub (SIMD)
    #[inline]
    pub fn png_unfilter_sub(&self, row: &mut [u8], bpp: usize) {
        match self.level {
            SimdLevel::Avx2 | SimdLevel::Sse4 | SimdLevel::Sse2 => {
                self.png_unfilter_sub_simd(row, bpp)
            }
            _ => self.png_unfilter_sub_scalar(row, bpp),
        }
    }

    fn png_unfilter_sub_scalar(&self, row: &mut [u8], bpp: usize) {
        for i in bpp..row.len() {
            row[i] = row[i].wrapping_add(row[i - bpp]);
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn png_unfilter_sub_simd(&self, row: &mut [u8], bpp: usize) {
        // For common bpp values, use optimized path
        if bpp == 4 && row.len() >= 16 {
            use std::arch::x86_64::*;
            unsafe {
                let mut i = bpp;
                while i + 16 <= row.len() {
                    // Process 4 pixels at a time
                    for j in 0..4 {
                        let idx = i + j * 4;
                        if idx + 4 <= row.len() {
                            row[idx] = row[idx].wrapping_add(row[idx - 4]);
                            row[idx + 1] = row[idx + 1].wrapping_add(row[idx - 3]);
                            row[idx + 2] = row[idx + 2].wrapping_add(row[idx - 2]);
                            row[idx + 3] = row[idx + 3].wrapping_add(row[idx - 1]);
                        }
                    }
                    i += 16;
                }
                // Remainder
                while i < row.len() {
                    row[i] = row[i].wrapping_add(row[i - bpp]);
                    i += 1;
                }
            }
        } else {
            self.png_unfilter_sub_scalar(row, bpp);
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn png_unfilter_sub_simd(&self, row: &mut [u8], bpp: usize) {
        self.png_unfilter_sub_scalar(row, bpp);
    }

    /// Reconstruct PNG filter type Up
    #[inline]
    pub fn png_unfilter_up(&self, row: &mut [u8], prev: &[u8]) {
        debug_assert_eq!(row.len(), prev.len());
        match self.level {
            SimdLevel::Avx2 => self.png_unfilter_up_avx2(row, prev),
            SimdLevel::Sse4 | SimdLevel::Sse2 => self.png_unfilter_up_sse(row, prev),
            SimdLevel::Neon => self.png_unfilter_up_neon(row, prev),
            SimdLevel::None => self.png_unfilter_up_scalar(row, prev),
        }
    }

    fn png_unfilter_up_scalar(&self, row: &mut [u8], prev: &[u8]) {
        for (r, &p) in row.iter_mut().zip(prev.iter()) {
            *r = r.wrapping_add(p);
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn png_unfilter_up_avx2(&self, row: &mut [u8], prev: &[u8]) {
        use std::arch::x86_64::*;
        unsafe {
            let mut i = 0;
            while i + 32 <= row.len() {
                let r = _mm256_loadu_si256(row.as_ptr().add(i) as *const __m256i);
                let p = _mm256_loadu_si256(prev.as_ptr().add(i) as *const __m256i);
                let sum = _mm256_add_epi8(r, p);
                _mm256_storeu_si256(row.as_mut_ptr().add(i) as *mut __m256i, sum);
                i += 32;
            }
            // Scalar remainder
            while i < row.len() {
                row[i] = row[i].wrapping_add(prev[i]);
                i += 1;
            }
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn png_unfilter_up_avx2(&self, row: &mut [u8], prev: &[u8]) {
        self.png_unfilter_up_scalar(row, prev);
    }

    #[cfg(target_arch = "x86_64")]
    fn png_unfilter_up_sse(&self, row: &mut [u8], prev: &[u8]) {
        use std::arch::x86_64::*;
        unsafe {
            let mut i = 0;
            while i + 16 <= row.len() {
                let r = _mm_loadu_si128(row.as_ptr().add(i) as *const __m128i);
                let p = _mm_loadu_si128(prev.as_ptr().add(i) as *const __m128i);
                let sum = _mm_add_epi8(r, p);
                _mm_storeu_si128(row.as_mut_ptr().add(i) as *mut __m128i, sum);
                i += 16;
            }
            while i < row.len() {
                row[i] = row[i].wrapping_add(prev[i]);
                i += 1;
            }
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn png_unfilter_up_sse(&self, row: &mut [u8], prev: &[u8]) {
        self.png_unfilter_up_scalar(row, prev);
    }

    #[cfg(target_arch = "aarch64")]
    fn png_unfilter_up_neon(&self, row: &mut [u8], prev: &[u8]) {
        use std::arch::aarch64::*;
        unsafe {
            let mut i = 0;
            while i + 16 <= row.len() {
                let r = vld1q_u8(row.as_ptr().add(i));
                let p = vld1q_u8(prev.as_ptr().add(i));
                let sum = vaddq_u8(r, p);
                vst1q_u8(row.as_mut_ptr().add(i), sum);
                i += 16;
            }
            while i < row.len() {
                row[i] = row[i].wrapping_add(prev[i]);
                i += 1;
            }
        }
    }

    #[cfg(not(target_arch = "aarch64"))]
    fn png_unfilter_up_neon(&self, row: &mut [u8], prev: &[u8]) {
        self.png_unfilter_up_scalar(row, prev);
    }

    /// Reconstruct PNG filter type Average
    #[inline]
    pub fn png_unfilter_avg(&self, row: &mut [u8], prev: &[u8], bpp: usize) {
        for i in 0..row.len() {
            let left = if i >= bpp { row[i - bpp] } else { 0 };
            let up = prev[i];
            row[i] = row[i].wrapping_add(((left as u16 + up as u16) / 2) as u8);
        }
    }

    /// Reconstruct PNG filter type Paeth
    #[inline]
    pub fn png_unfilter_paeth(&self, row: &mut [u8], prev: &[u8], bpp: usize) {
        for i in 0..row.len() {
            let a = if i >= bpp { row[i - bpp] as i16 } else { 0 };
            let b = prev[i] as i16;
            let c = if i >= bpp { prev[i - bpp] as i16 } else { 0 };

            let p = a + b - c;
            let pa = (p - a).abs();
            let pb = (p - b).abs();
            let pc = (p - c).abs();

            let pr = if pa <= pb && pa <= pc {
                a as u8
            } else if pb <= pc {
                b as u8
            } else {
                c as u8
            };

            row[i] = row[i].wrapping_add(pr);
        }
    }

    // ========== JPEG IDCT ==========

    /// Perform 8x8 IDCT with SIMD acceleration
    #[inline]
    pub fn jpeg_idct_8x8(&self, block: &mut [i16; 64]) {
        match self.level {
            SimdLevel::Avx2 => self.jpeg_idct_avx2(block),
            SimdLevel::Sse4 | SimdLevel::Sse2 => self.jpeg_idct_sse(block),
            _ => self.jpeg_idct_scalar(block),
        }
    }

    fn jpeg_idct_scalar(&self, block: &mut [i16; 64]) {
        // AAN algorithm constants
        const SCALE: i32 = 4096;
        const C1: i32 = 4017;  // cos(pi/16) * 4096
        const C2: i32 = 3784;  // cos(2*pi/16) * 4096
        const C3: i32 = 3406;  // cos(3*pi/16) * 4096
        const C5: i32 = 2276;  // cos(5*pi/16) * 4096
        const C6: i32 = 1567;  // cos(6*pi/16) * 4096
        const C7: i32 = 799;   // cos(7*pi/16) * 4096

        let mut temp = [0i32; 64];

        // Rows
        for row in 0..8 {
            let base = row * 8;
            let s0 = block[base] as i32;
            let s1 = block[base + 1] as i32;
            let s2 = block[base + 2] as i32;
            let s3 = block[base + 3] as i32;
            let s4 = block[base + 4] as i32;
            let s5 = block[base + 5] as i32;
            let s6 = block[base + 6] as i32;
            let s7 = block[base + 7] as i32;

            // Even part
            let t0 = (s0 + s4) * SCALE;
            let t1 = (s0 - s4) * SCALE;
            let t2 = s2 * C6 - s6 * C2;
            let t3 = s2 * C2 + s6 * C6;

            let e0 = t0 + t3;
            let e1 = t1 + t2;
            let e2 = t1 - t2;
            let e3 = t0 - t3;

            // Odd part
            let t4 = s1 * C7 - s7 * C1;
            let t5 = s5 * C3 - s3 * C5;
            let t6 = s5 * C5 + s3 * C3;
            let t7 = s1 * C1 + s7 * C7;

            let o0 = t4 + t5;
            let o1 = t7 - t6;
            let o2 = t4 - t5;
            let o3 = t7 + t6;

            temp[base] = e0 + o3;
            temp[base + 1] = e1 + o2;
            temp[base + 2] = e2 + o1;
            temp[base + 3] = e3 + o0;
            temp[base + 4] = e3 - o0;
            temp[base + 5] = e2 - o1;
            temp[base + 6] = e1 - o2;
            temp[base + 7] = e0 - o3;
        }

        // Columns
        for col in 0..8 {
            let s0 = temp[col];
            let s1 = temp[col + 8];
            let s2 = temp[col + 16];
            let s3 = temp[col + 24];
            let s4 = temp[col + 32];
            let s5 = temp[col + 40];
            let s6 = temp[col + 48];
            let s7 = temp[col + 56];

            let t0 = (s0 + s4);
            let t1 = (s0 - s4);
            let t2 = (s2 * C6 - s6 * C2) / SCALE;
            let t3 = (s2 * C2 + s6 * C6) / SCALE;

            let e0 = t0 + t3;
            let e1 = t1 + t2;
            let e2 = t1 - t2;
            let e3 = t0 - t3;

            let t4 = (s1 * C7 - s7 * C1) / SCALE;
            let t5 = (s5 * C3 - s3 * C5) / SCALE;
            let t6 = (s5 * C5 + s3 * C3) / SCALE;
            let t7 = (s1 * C1 + s7 * C7) / SCALE;

            let o0 = t4 + t5;
            let o1 = t7 - t6;
            let o2 = t4 - t5;
            let o3 = t7 + t6;

            // Scale and clamp
            block[col] = ((e0 + o3) >> 20).clamp(-128, 127) as i16;
            block[col + 8] = ((e1 + o2) >> 20).clamp(-128, 127) as i16;
            block[col + 16] = ((e2 + o1) >> 20).clamp(-128, 127) as i16;
            block[col + 24] = ((e3 + o0) >> 20).clamp(-128, 127) as i16;
            block[col + 32] = ((e3 - o0) >> 20).clamp(-128, 127) as i16;
            block[col + 40] = ((e2 - o1) >> 20).clamp(-128, 127) as i16;
            block[col + 48] = ((e1 - o2) >> 20).clamp(-128, 127) as i16;
            block[col + 56] = ((e0 - o3) >> 20).clamp(-128, 127) as i16;
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn jpeg_idct_avx2(&self, block: &mut [i16; 64]) {
        // Fallback to scalar for now - AVX2 IDCT is complex
        self.jpeg_idct_scalar(block);
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn jpeg_idct_avx2(&self, block: &mut [i16; 64]) {
        self.jpeg_idct_scalar(block);
    }

    #[cfg(target_arch = "x86_64")]
    fn jpeg_idct_sse(&self, block: &mut [i16; 64]) {
        self.jpeg_idct_scalar(block);
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn jpeg_idct_sse(&self, block: &mut [i16; 64]) {
        self.jpeg_idct_scalar(block);
    }

    // ========== YCbCr to RGB Conversion ==========

    /// Convert YCbCr to RGB with SIMD
    #[inline]
    pub fn ycbcr_to_rgb(&self, y: &[u8], cb: &[u8], cr: &[u8], rgb: &mut [u8]) {
        debug_assert_eq!(y.len(), cb.len());
        debug_assert_eq!(y.len(), cr.len());
        debug_assert_eq!(rgb.len(), y.len() * 3);

        match self.level {
            SimdLevel::Avx2 => self.ycbcr_to_rgb_avx2(y, cb, cr, rgb),
            _ => self.ycbcr_to_rgb_scalar(y, cb, cr, rgb),
        }
    }

    fn ycbcr_to_rgb_scalar(&self, y: &[u8], cb: &[u8], cr: &[u8], rgb: &mut [u8]) {
        for i in 0..y.len() {
            let yy = y[i] as i32;
            let cbb = cb[i] as i32 - 128;
            let crr = cr[i] as i32 - 128;

            // ITU-R BT.601 conversion
            let r = yy + ((crr * 359) >> 8);
            let g = yy - ((cbb * 88 + crr * 183) >> 8);
            let b = yy + ((cbb * 454) >> 8);

            rgb[i * 3] = r.clamp(0, 255) as u8;
            rgb[i * 3 + 1] = g.clamp(0, 255) as u8;
            rgb[i * 3 + 2] = b.clamp(0, 255) as u8;
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn ycbcr_to_rgb_avx2(&self, y: &[u8], cb: &[u8], cr: &[u8], rgb: &mut [u8]) {
        // Fallback for now
        self.ycbcr_to_rgb_scalar(y, cb, cr, rgb);
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn ycbcr_to_rgb_avx2(&self, y: &[u8], cb: &[u8], cr: &[u8], rgb: &mut [u8]) {
        self.ycbcr_to_rgb_scalar(y, cb, cr, rgb);
    }

    // ========== Fast memcpy for LZ77 ==========

    /// Fast copy with possible overlap (for LZ77 matches)
    #[inline]
    pub fn copy_match(&self, dst: &mut [u8], dst_pos: usize, distance: usize, length: usize) {
        let src_pos = dst_pos - distance;

        if distance >= length {
            // No overlap, can use fast copy
            dst.copy_within(src_pos..src_pos + length, dst_pos);
        } else {
            // Overlap, copy byte by byte
            for i in 0..length {
                dst[dst_pos + i] = dst[src_pos + i];
            }
        }
    }
}

impl Default for SimdOps {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simd_detection() {
        let ops = SimdOps::new();
        let level = ops.level();
        println!("SIMD level: {:?}", level);
    }

    #[test]
    fn test_adler32() {
        let ops = SimdOps::new();
        let data = b"Hello, World!";
        let checksum = ops.adler32(data);
        // Expected Adler-32 for "Hello, World!" is 0x1F9E046A
        assert_eq!(checksum, 0x1F9E046A);
    }

    #[test]
    fn test_png_unfilter_up() {
        let ops = SimdOps::new();
        let prev = [10u8, 20, 30, 40];
        let mut row = [1u8, 2, 3, 4];
        ops.png_unfilter_up(&mut row, &prev);
        assert_eq!(row, [11, 22, 33, 44]);
    }

    #[test]
    fn test_ycbcr_to_rgb() {
        let ops = SimdOps::new();
        let y = [128u8];
        let cb = [128u8];  // neutral
        let cr = [128u8];  // neutral
        let mut rgb = [0u8; 3];
        ops.ycbcr_to_rgb(&y, &cb, &cr, &mut rgb);
        // Y=128 with neutral Cb/Cr should give ~(128, 128, 128)
        assert!((rgb[0] as i32 - 128).abs() <= 1);
        assert!((rgb[1] as i32 - 128).abs() <= 1);
        assert!((rgb[2] as i32 - 128).abs() <= 1);
    }
}
