#!/bin/bash
# Optimize native binaries for size
#
# This script strips symbols and optionally compresses native binaries
# using UPX. Can achieve 50-80% size reduction.
#
# Usage: ./scripts/optimize-native.sh [input-binary] [output-binary]

set -e

BINARY="${1:-target/release/fos-engine}"
OUTPUT="${2:-${BINARY}.optimized}"

echo "=== Native Binary Size Optimization ==="
echo "Input:  $BINARY"
echo "Output: $OUTPUT"
echo

# Check if input file exists
if [ ! -f "$BINARY" ]; then
    echo "Error: Input file not found: $BINARY"
    echo
    echo "Build the release binary first:"
    echo "  cargo build --release -p fos-engine"
    exit 1
fi

# Copy binary
cp "$BINARY" "$OUTPUT"

# Original size
ORIGINAL_SIZE=$(stat -c%s "$BINARY" 2>/dev/null || stat -f%z "$BINARY")
echo "Original size: $(numfmt --to=iec $ORIGINAL_SIZE 2>/dev/null || echo "$ORIGINAL_SIZE bytes")"

# Strip all symbols
echo
echo "Stripping symbols..."
if command -v strip &> /dev/null; then
    # Linux strip
    strip --strip-all "$OUTPUT" 2>/dev/null || \
    # macOS strip
    strip -x "$OUTPUT" 2>/dev/null || \
    echo "Warning: strip failed"
else
    echo "Warning: strip not found"
fi

STRIPPED_SIZE=$(stat -c%s "$OUTPUT" 2>/dev/null || stat -f%z "$OUTPUT")
echo "Stripped size: $(numfmt --to=iec $STRIPPED_SIZE 2>/dev/null || echo "$STRIPPED_SIZE bytes")"

# Try sstrip for even more stripping (if available)
if command -v sstrip &> /dev/null; then
    echo "Running sstrip..."
    sstrip "$OUTPUT" 2>/dev/null || true
fi

# Try objcopy for additional stripping
if command -v objcopy &> /dev/null; then
    echo "Running objcopy --strip-unneeded..."
    objcopy --strip-unneeded "$OUTPUT" 2>/dev/null || true
fi

AFTER_STRIP_SIZE=$(stat -c%s "$OUTPUT" 2>/dev/null || stat -f%z "$OUTPUT")
echo "After all stripping: $(numfmt --to=iec $AFTER_STRIP_SIZE 2>/dev/null || echo "$AFTER_STRIP_SIZE bytes")"

# Check for UPX
if command -v upx &> /dev/null; then
    echo
    echo "Compressing with UPX..."
    
    # Use best compression with LZMA
    upx --best --lzma -q "$OUTPUT" 2>/dev/null || \
    # Fall back to regular compression
    upx --best -q "$OUTPUT" 2>/dev/null || \
    echo "Warning: UPX compression failed (binary may be incompatible)"
    
    COMPRESSED_SIZE=$(stat -c%s "$OUTPUT" 2>/dev/null || stat -f%z "$OUTPUT")
    TOTAL_SAVINGS=$((ORIGINAL_SIZE - COMPRESSED_SIZE))
    
    if [ "$ORIGINAL_SIZE" -gt 0 ]; then
        PERCENT=$((TOTAL_SAVINGS * 100 / ORIGINAL_SIZE))
    else
        PERCENT=0
    fi
    
    echo
    echo "=== Results ==="
    echo "Final size: $(numfmt --to=iec $COMPRESSED_SIZE 2>/dev/null || echo "$COMPRESSED_SIZE bytes")"
    echo "Total savings: $(numfmt --to=iec $TOTAL_SAVINGS 2>/dev/null || echo "$TOTAL_SAVINGS bytes") ($PERCENT%)"
else
    echo
    echo "Note: UPX not found. Install for additional compression:"
    echo "  pacman -S upx      # Arch/Manjaro"
    echo "  apt install upx    # Debian/Ubuntu"
    echo "  brew install upx   # macOS"
    
    SAVINGS=$((ORIGINAL_SIZE - STRIPPED_SIZE))
    if [ "$ORIGINAL_SIZE" -gt 0 ]; then
        PERCENT=$((SAVINGS * 100 / ORIGINAL_SIZE))
    else
        PERCENT=0
    fi
    
    echo
    echo "=== Results ==="
    echo "Final size: $(numfmt --to=iec $STRIPPED_SIZE 2>/dev/null || echo "$STRIPPED_SIZE bytes")"
    echo "Savings: $(numfmt --to=iec $SAVINGS 2>/dev/null || echo "$SAVINGS bytes") ($PERCENT%)"
fi

echo
echo "Output written to: $OUTPUT"
