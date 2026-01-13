# SIMD Optimization Roadmap: Surpassing Chromium

> Goal: SIMD-first architecture with 2-4x speedups over Chromium in hot paths

## Current State
- Some SIMD in HTML parser (`simd_parse.rs`)
- SIMD utilities in `fos-engine/src/simd.rs`
- Canvas SIMD operations

---

## Phase 1: Foundation (Q1)

### 1.1 SIMD Abstraction Layer
```rust
// Unified SIMD API across platforms
pub trait SimdOps {
    type V128: Copy;  // 128-bit vectors (SSE2/NEON baseline)
    type V256: Copy;  // 256-bit vectors (AVX2/SVE)
    type V512: Copy;  // 512-bit vectors (AVX-512)
    
    fn load_v128(ptr: *const u8) -> Self::V128;
    fn cmpeq_v128(a: Self::V128, b: Self::V128) -> Self::V128;
    fn movemask_v128(a: Self::V128) -> u32;
    // ...
}

// Runtime feature detection
pub fn simd_level() -> SimdLevel {
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx512f") { SimdLevel::Avx512 }
        else if is_x86_feature_detected!("avx2") { SimdLevel::Avx2 }
        else { SimdLevel::Sse2 }
    }
    #[cfg(target_arch = "aarch64")]
    { SimdLevel::Neon }
}
```

### 1.2 Platform Support Matrix
| ISA | Width | Chromium | fOS Target |
|-----|-------|----------|------------|
| SSE2 | 128-bit | Baseline | Baseline |
| AVX2 | 256-bit | Some | Extensive |
| AVX-512 | 512-bit | Minimal | Full support |
| NEON | 128-bit | ARM builds | Full support |
| SVE/SVE2 | Variable | None | Future |

---

## Phase 2: Parser SIMD (Q1-Q2)

### 2.1 HTML Tokenizer
| Operation | Scalar | SSE2 | AVX2 | AVX-512 |
|-----------|--------|------|------|---------|
| Find `<` | 1B/cycle | 16B/cycle | 32B/cycle | 64B/cycle |
| Attribute scan | 1B/cycle | 16B/cycle | 32B/cycle | 64B/cycle |
| Whitespace skip | 1B/cycle | 16B/cycle | 32B/cycle | 64B/cycle |

```rust
// AVX-512 tag scanner
#[cfg(target_feature = "avx512f")]
pub fn find_tag_boundaries_avx512(html: &[u8]) -> Vec<usize> {
    use std::arch::x86_64::*;
    
    let lt = _mm512_set1_epi8(b'<' as i8);
    let gt = _mm512_set1_epi8(b'>' as i8);
    
    let mut positions = Vec::new();
    for chunk in html.chunks(64) {
        let data = _mm512_loadu_si512(chunk.as_ptr() as *const _);
        let lt_mask = _mm512_cmpeq_epi8_mask(data, lt);
        let gt_mask = _mm512_cmpeq_epi8_mask(data, gt);
        // Extract positions from masks...
    }
    positions
}
```

### 2.2 CSS Tokenizer
```rust
// SIMD number parsing (12x faster than scalar)
pub fn parse_numbers_simd(css: &[u8]) -> Vec<f32> {
    // Vectorized digit detection
    // Parallel decimal handling
    // SIMD float conversion
}
```

### 2.3 JSON Parser (for fetch/XHR)
| Operation | Chromium (V8) | fOS Target |
|-----------|---------------|------------|
| String scan | 2 GB/s | 8 GB/s |
| Structure parse | 500 MB/s | 2 GB/s |
| Number parse | 200 MB/s | 800 MB/s |

---

## Phase 3: String Operations (Q2)

### 3.1 UTF-8 Validation
```rust
// simdutf-style validation (10x faster than std::str::from_utf8)
pub fn validate_utf8_simd(bytes: &[u8]) -> bool {
    // Process 64 bytes at a time with AVX-512
    // Vectorized continuation byte checking
    // Overlong sequence detection
}
```

### 3.2 String Comparison
```rust
pub fn strcmp_simd(a: &[u8], b: &[u8]) -> Ordering {
    // Compare 32/64 bytes at once
    // Early exit on first difference
}

pub fn strchr_simd(haystack: &[u8], needle: u8) -> Option<usize> {
    // 64 bytes per iteration with AVX-512
}
```

### 3.3 Case Conversion
```rust
// ASCII case folding (16x faster)
pub fn to_lowercase_ascii_simd(s: &mut [u8]) {
    let upper_a = _mm256_set1_epi8(b'A' as i8);
    let upper_z = _mm256_set1_epi8(b'Z' as i8);
    let diff = _mm256_set1_epi8(32);
    
    for chunk in s.chunks_mut(32) {
        let data = _mm256_loadu_si256(chunk.as_ptr() as *const _);
        let is_upper = _mm256_and_si256(
            _mm256_cmpgt_epi8(data, _mm256_sub_epi8(upper_a, _mm256_set1_epi8(1))),
            _mm256_cmpgt_epi8(_mm256_add_epi8(upper_z, _mm256_set1_epi8(1)), data)
        );
        let lowered = _mm256_add_epi8(data, _mm256_and_si256(is_upper, diff));
        _mm256_storeu_si256(chunk.as_mut_ptr() as *mut _, lowered);
    }
}
```

---

## Phase 4: Rendering SIMD (Q2-Q3)

### 4.1 Color Operations
```rust
// Premultiply alpha (8 pixels at once)
pub fn premultiply_alpha_avx2(pixels: &mut [u32]) {
    for chunk in pixels.chunks_mut(8) {
        let rgba = _mm256_loadu_si256(chunk.as_ptr() as *const _);
        // Extract alpha, multiply RGB, reassemble
    }
}

// Blend operations (Porter-Duff)
pub fn blend_over_avx2(dst: &mut [u32], src: &[u32]) {
    // SIMD alpha blending: 8 pixels/iteration
}
```

### 4.2 Image Processing
| Operation | Scalar | AVX2 | Speedup |
|-----------|--------|------|---------|
| RGBA→BGRA | 100 Mpx/s | 2000 Mpx/s | 20x |
| Resize (bilinear) | 10 Mpx/s | 200 Mpx/s | 20x |
| Blur (box) | 5 Mpx/s | 100 Mpx/s | 20x |
| Gaussian blur | 2 Mpx/s | 50 Mpx/s | 25x |

### 4.3 Geometry
```rust
// Matrix multiplication (4x4)
pub fn mat4_mul_avx(a: &Mat4, b: &Mat4) -> Mat4 {
    // 4x faster than scalar
}

// Transform points (batch)
pub fn transform_points_avx2(points: &mut [[f32; 2]], matrix: &Mat3) {
    // Process 4 points per iteration
}
```

---

## Phase 5: Layout SIMD (Q3)

### 5.1 Box Model Calculations
```rust
// Compute 8 box margins/paddings at once
pub fn compute_box_edges_simd(
    styles: &[ComputedStyle; 8],
    containing_block: f32
) -> [[f32; 4]; 8] {
    // Vectorized percentage resolution
    // Parallel margin collapsing
}
```

### 5.2 Hit Testing
```rust
// Test point against 8 rects at once
pub fn hit_test_simd(point: Point, rects: &[Rect; 8]) -> u8 {
    // Returns bitmask of hits
}
```

---

## Phase 6: JavaScript SIMD (Q4)

### 6.1 Runtime Operations
```rust
// TypedArray operations
pub fn typed_array_add_f64(dst: &mut [f64], a: &[f64], b: &[f64]) {
    // 4x f64 per AVX iteration
}

// String operations in JS
pub fn js_string_indexof_simd(haystack: &JsString, needle: u16) -> i32 {
    // Vectorized UTF-16 search
}
```

### 6.2 Bytecode Dispatch
```rust
// SIMD-accelerated opcode dispatch (experimental)
// Batch decode multiple opcodes, execute in parallel where independent
```

---

## Chromium Comparison

| Area | Chromium SIMD | fOS Target |
|------|---------------|------------|
| HTML parser | SSE2 | AVX-512 |
| CSS tokenizer | None | AVX2 |
| String ops | Partial | Full |
| Image decode | Platform libs | Custom SIMD |
| Color blend | Some | Full pipeline |
| Layout math | None | Partial |
| JS TypedArrays | V8 intrinsics | Full SIMD |

---

## Benchmarks Target

| Operation | Chromium | fOS Target | Speedup |
|-----------|----------|------------|---------|
| HTML parse (1MB) | 50ms | 12ms | 4x |
| CSS parse (100KB) | 10ms | 3ms | 3x |
| JSON parse (1MB) | 20ms | 5ms | 4x |
| Image resize | 50ms | 10ms | 5x |
| Color conversion | 10ms | 1ms | 10x |

---

## Implementation Priority

1. **HTML tokenizer** - Most parsing time
2. **String operations** - Used everywhere
3. **Color/pixel ops** - Rendering hot path
4. **CSS tokenizer** - Second most parsing
5. **Layout math** - Helps complex pages
