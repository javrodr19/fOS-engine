//! SIMD Optimized Array Operations
//!
//! Vectorized operations for numeric arrays.
//! Falls back to scalar when SIMD unavailable.

use super::value::JsVal;

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// SIMD-optimized array fill with f64
#[inline]
pub fn simd_fill_f64(data: &mut [f64], value: f64) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { simd_fill_f64_avx2(data, value); }
            return;
        }
    }
    // Scalar fallback
    data.fill(value);
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn simd_fill_f64_avx2(data: &mut [f64], value: f64) {
    let vec = _mm256_set1_pd(value);
    let chunks = data.len() / 4;
    let ptr = data.as_mut_ptr();
    
    for i in 0..chunks {
        _mm256_storeu_pd(ptr.add(i * 4), vec);
    }
    
    // Handle remainder
    for i in (chunks * 4)..data.len() {
        *ptr.add(i) = value;
    }
}

/// SIMD-optimized array sum
#[inline]
pub fn simd_sum_f64(data: &[f64]) -> f64 {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            return unsafe { simd_sum_f64_avx2(data) };
        }
    }
    // Scalar fallback
    data.iter().sum()
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn simd_sum_f64_avx2(data: &[f64]) -> f64 {
    let chunks = data.len() / 4;
    let ptr = data.as_ptr();
    let mut sum_vec = _mm256_setzero_pd();
    
    for i in 0..chunks {
        let chunk = _mm256_loadu_pd(ptr.add(i * 4));
        sum_vec = _mm256_add_pd(sum_vec, chunk);
    }
    
    // Horizontal sum
    let low = _mm256_castpd256_pd128(sum_vec);
    let high = _mm256_extractf128_pd(sum_vec, 1);
    let sum128 = _mm_add_pd(low, high);
    let sum64 = _mm_hadd_pd(sum128, sum128);
    
    let mut result = 0.0f64;
    _mm_store_sd(&mut result, sum64);
    
    // Add remainder
    for i in (chunks * 4)..data.len() {
        result += *ptr.add(i);
    }
    result
}

/// SIMD-optimized array multiply (a[i] * scalar)
#[inline]
pub fn simd_mul_scalar_f64(data: &mut [f64], scalar: f64) {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { simd_mul_scalar_f64_avx2(data, scalar); }
            return;
        }
    }
    // Scalar fallback
    for x in data.iter_mut() { *x *= scalar; }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn simd_mul_scalar_f64_avx2(data: &mut [f64], scalar: f64) {
    let mul_vec = _mm256_set1_pd(scalar);
    let chunks = data.len() / 4;
    let ptr = data.as_mut_ptr();
    
    for i in 0..chunks {
        let chunk = _mm256_loadu_pd(ptr.add(i * 4));
        let result = _mm256_mul_pd(chunk, mul_vec);
        _mm256_storeu_pd(ptr.add(i * 4), result);
    }
    
    for i in (chunks * 4)..data.len() {
        *ptr.add(i) *= scalar;
    }
}

/// SIMD-optimized array add (a + b -> result)
#[inline]
pub fn simd_add_arrays_f64(a: &[f64], b: &[f64], result: &mut [f64]) {
    assert_eq!(a.len(), b.len());
    assert_eq!(a.len(), result.len());
    
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") {
            unsafe { simd_add_arrays_f64_avx2(a, b, result); }
            return;
        }
    }
    // Scalar fallback
    for i in 0..a.len() {
        result[i] = a[i] + b[i];
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn simd_add_arrays_f64_avx2(a: &[f64], b: &[f64], result: &mut [f64]) {
    let chunks = a.len() / 4;
    let a_ptr = a.as_ptr();
    let b_ptr = b.as_ptr();
    let r_ptr = result.as_mut_ptr();
    
    for i in 0..chunks {
        let av = _mm256_loadu_pd(a_ptr.add(i * 4));
        let bv = _mm256_loadu_pd(b_ptr.add(i * 4));
        let rv = _mm256_add_pd(av, bv);
        _mm256_storeu_pd(r_ptr.add(i * 4), rv);
    }
    
    for i in (chunks * 4)..a.len() {
        *r_ptr.add(i) = *a_ptr.add(i) + *b_ptr.add(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simd_fill() {
        let mut data = vec![0.0; 100];
        simd_fill_f64(&mut data, 42.0);
        assert!(data.iter().all(|&x| x == 42.0));
    }
    
    #[test]
    fn test_simd_sum() {
        let data: Vec<f64> = (1..=100).map(|x| x as f64).collect();
        let sum = simd_sum_f64(&data);
        assert_eq!(sum, 5050.0);
    }
    
    #[test]
    fn test_simd_mul_scalar() {
        let mut data = vec![1.0, 2.0, 3.0, 4.0];
        simd_mul_scalar_f64(&mut data, 2.0);
        assert_eq!(data, vec![2.0, 4.0, 6.0, 8.0]);
    }
}
