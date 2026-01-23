//! SIMD Optimizations
//!
//! SIMD-accelerated operations for video/audio processing.

pub mod yuv;
pub mod dct;

/// Check for SIMD support at runtime
#[derive(Debug, Clone, Copy)]
pub struct SimdSupport { pub sse2: bool, pub avx: bool, pub avx2: bool, pub avx512: bool, pub neon: bool }

impl SimdSupport {
    pub fn detect() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Self {
                sse2: is_x86_feature_detected!("sse2"),
                avx: is_x86_feature_detected!("avx"),
                avx2: is_x86_feature_detected!("avx2"),
                avx512: is_x86_feature_detected!("avx512f"),
                neon: false,
            }
        }
        #[cfg(target_arch = "aarch64")]
        { Self { sse2: false, avx: false, avx2: false, avx512: false, neon: true } }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        { Self { sse2: false, avx: false, avx2: false, avx512: false, neon: false } }
    }
    
    pub fn best_vector_width(&self) -> usize {
        if self.avx512 { 64 } else if self.avx2 { 32 } else if self.avx || self.sse2 { 16 } else if self.neon { 16 } else { 8 }
    }
}

impl Default for SimdSupport { fn default() -> Self { Self::detect() } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_detect() { let s = SimdSupport::detect(); assert!(s.best_vector_width() >= 8); }
}
