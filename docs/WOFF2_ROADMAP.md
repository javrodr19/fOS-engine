# WOFF2 Implementation Roadmap

WOFF2 support has been implemented with custom Brotli decompression and table transformations.

## Current Status

- **WOFF1**: ✅ Implemented (basic DEFLATE support)
- **WOFF2**: ✅ Implemented (custom Brotli + table transforms)

## WOFF2 Overview

WOFF2 uses:
1. **Brotli compression** - More efficient than DEFLATE
2. **Table transformations** - Pre-processing for better compression
3. **Shared dictionary** - Font-specific Brotli dictionary

## Implementation Phases

### Phase 1: Brotli Decompression
- Implement custom Brotli decoder or integrate `brotli` crate
- Support shared font dictionary
- Estimated effort: 2-3 days

### Phase 2: Table Transformations
- `glyf` table: Triplet encoding, flag transformations
- `loca` table: Reconstructed from glyf
- `hmtx` table: Delta encoding
- Estimated effort: 3-4 days

### Phase 3: WOFF2 Container
- Parse WOFF2 header (different from WOFF1)
- Handle extended metadata
- Reconstruct OpenType tables
- Estimated effort: 1-2 days

### Phase 4: Optimization ✅
- ✅ Streaming decompression (`StreamingBrotliDecoder`)
- ✅ Memory-mapped output (`MmapFontOutput`)
- ✅ Cache transformed tables (`Woff2TableCache`)

## Dependencies

For full WOFF2 support, consider:
```toml
# Option A: Use existing crate (adds ~50KB)
brotli-decompressor = "4.0"

# Option B: Custom from-scratch (preferred for zero-dep goal)
# Implement custom Brotli decoder
```

## References

- [WOFF2 Spec](https://www.w3.org/TR/WOFF2/)
- [Brotli RFC 7932](https://tools.ietf.org/html/rfc7932)
- [Google WOFF2 Reference](https://github.com/nicehash/nicehash-algorithm/blob/master/lib/woff2)

## Timeline

Target: Q2 2025 (after core font parser stabilization)
