# AVIF Decoder Implementation Roadmap

This document outlines the roadmap for implementing a custom AVIF decoder from scratch.

## Overview

AVIF (AV1 Image File Format) is based on the AV1 video codec. A full implementation requires:

1. **ISOBMFF Container Parser** (~500 lines)
2. **AV1 OBU Parser** (~800 lines)
3. **AV1 Intra-Frame Decoder** (~8000+ lines)
4. **Color Space Conversion** (~500 lines)

**Estimated total: 10,000+ lines of code**

## Phase 1: Container Parsing

### ISOBMFF (ISO Base Media File Format)
- Parse `ftyp` box to identify AVIF brand
- Parse `meta` box for image metadata
- Parse `mdat` box for image data
- Handle `iloc` (item location) box

### Key Structures
```rust
struct AvifContainer {
    brand: [u8; 4],
    width: u32,
    height: u32,
    bit_depth: u8,
    color_primaries: u8,
    transfer_characteristics: u8,
    matrix_coefficients: u8,
    data_offset: usize,
    data_size: usize,
}
```

## Phase 2: AV1 OBU Parsing

### Open Bitstream Units (OBUs)
- Sequence Header OBU
- Frame Header OBU
- Tile Group OBU
- Metadata OBU

### Key Parameters
- Profile (Main, High, Professional)
- Still picture flag
- Reduced still picture header
- Quantization parameters
- Loop filter parameters

## Phase 3: Intra-Frame Decoding

### Transform Coding
- DCT sizes: 4×4, 8×8, 16×16, 32×32, 64×64
- ADST (Asymmetric DST)
- Identity transform
- SIMD acceleration for transforms

### Prediction Modes
- DC prediction
- Directional prediction (58 angles)
- Paeth prediction
- Smooth prediction variants
- Chroma-from-luma (CfL)

### Entropy Coding
- Arithmetic coding (CDFs)
- Symbol coding
- Coefficient coding

### Loop Filters
- Deblocking filter
- CDEF (Constrained Directional Enhancement Filter)
- Loop restoration (Wiener, Self-guided)

## Phase 4: Color Space

### Supported Formats
- YUV 4:2:0, 4:2:2, 4:4:4
- 8-bit, 10-bit, 12-bit
- BT.601, BT.709, BT.2020

### HDR Support
- PQ (Perceptual Quantizer)
- HLG (Hybrid Log-Gamma)
- Tone mapping to SDR

## Milestones

| Phase | Description | Est. Lines | Est. Time |
|-------|-------------|------------|-----------|
| 1 | Container | 500 | 1 week |
| 2 | OBU Parser | 800 | 1 week |
| 3a | Transforms | 2000 | 2 weeks |
| 3b | Prediction | 3000 | 2 weeks |
| 3c | Entropy | 2000 | 2 weeks |
| 3d | Filters | 1500 | 1 week |
| 4 | Color | 500 | 1 week |
| - | Testing | - | 1 week |

**Total: ~10,300 lines, ~11 weeks**

## References

- [AV1 Bitstream Specification](https://aomediacodec.github.io/av1-spec/)
- [AVIF Specification](https://aomediacodec.github.io/av1-avif/)
- [ISOBMFF (ISO 14496-12)](https://www.iso.org/standard/74428.html)
- [libaom Reference Implementation](https://aomedia.googlesource.com/aom/)

## Current Status

The existing `avif.rs` in `fos-render/src/image/` provides:
- AVIF format detection via magic bytes
- Container validation (ftyp box check)
- Placeholder structure ready for full implementation

## Recommended Approach

1. Start with container parsing to extract raw AV1 data
2. Implement sequence/frame header parsing
3. Add basic 8-bit 4:2:0 intra-frame decoding
4. Extend to 10-bit and other subsampling formats
5. Add SIMD optimization for transforms
6. Add loop filtering for quality improvement
