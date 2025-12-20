#!/bin/bash
# Optimize WASM binaries for size
#
# This script uses wasm-opt and wasm-snip to reduce WASM binary size.
# Can achieve 15-30% size reduction on typical Rust WASM binaries.
#
# Usage: ./scripts/optimize-wasm.sh [input.wasm] [output.wasm]

set -e

WASM_FILE="${1:-target/wasm32-unknown-unknown/release/fos_engine.wasm}"
OUTPUT="${2:-${WASM_FILE%.wasm}.optimized.wasm}"

echo "=== WASM Size Optimization ==="
echo "Input:  $WASM_FILE"
echo "Output: $OUTPUT"
echo

# Check if input file exists
if [ ! -f "$WASM_FILE" ]; then
    echo "Error: Input file not found: $WASM_FILE"
    echo
    echo "Build the WASM target first:"
    echo "  cargo build --release --target wasm32-unknown-unknown -p fos-engine"
    exit 1
fi

# Check for wasm-opt
if ! command -v wasm-opt &> /dev/null; then
    echo "Error: wasm-opt not found."
    echo
    echo "Install binaryen:"
    echo "  pacman -S binaryen      # Arch/Manjaro"
    echo "  apt install binaryen    # Debian/Ubuntu"
    echo "  brew install binaryen   # macOS"
    exit 1
fi

# Original size
ORIGINAL_SIZE=$(stat -c%s "$WASM_FILE" 2>/dev/null || stat -f%z "$WASM_FILE")
echo "Original size: $(numfmt --to=iec $ORIGINAL_SIZE 2>/dev/null || echo "$ORIGINAL_SIZE bytes")"

# Optimize with wasm-opt
echo
echo "Running wasm-opt -Oz..."
wasm-opt -Oz \
    --strip-debug \
    --strip-producers \
    --strip-target-features \
    --remove-unused-names \
    --remove-unused-module-elements \
    -o "$OUTPUT" \
    "$WASM_FILE"

# Check for wasm-snip (optional extra optimization)
if command -v wasm-snip &> /dev/null; then
    echo "Running wasm-snip..."
    wasm-snip "$OUTPUT" -o "$OUTPUT" \
        --snip-rust-fmt-code \
        --snip-rust-panicking-code 2>/dev/null || true
fi

# Optimized size
OPTIMIZED_SIZE=$(stat -c%s "$OUTPUT" 2>/dev/null || stat -f%z "$OUTPUT")
SAVINGS=$((ORIGINAL_SIZE - OPTIMIZED_SIZE))

if [ "$ORIGINAL_SIZE" -gt 0 ]; then
    PERCENT=$((SAVINGS * 100 / ORIGINAL_SIZE))
else
    PERCENT=0
fi

echo
echo "=== Results ==="
echo "Optimized size: $(numfmt --to=iec $OPTIMIZED_SIZE 2>/dev/null || echo "$OPTIMIZED_SIZE bytes")"
echo "Savings: $(numfmt --to=iec $SAVINGS 2>/dev/null || echo "$SAVINGS bytes") ($PERCENT%)"
echo
echo "Output written to: $OUTPUT"
