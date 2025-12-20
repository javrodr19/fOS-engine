#!/bin/bash
# Analyze WASM binary bloat using twiggy
#
# Twiggy is a code size profiler for WASM that helps identify
# which functions and data contribute most to binary size.
#
# Usage: ./scripts/analyze-wasm.sh [input.wasm]

set -e

WASM_FILE="${1:-target/wasm32-unknown-unknown/release/fos_engine.wasm}"

echo "=== WASM Bloat Analysis with Twiggy ==="
echo "Input: $WASM_FILE"
echo

# Check if input file exists
if [ ! -f "$WASM_FILE" ]; then
    echo "Error: Input file not found: $WASM_FILE"
    echo
    echo "Build the WASM target first:"
    echo "  cargo build --release --target wasm32-unknown-unknown -p fos-engine"
    exit 1
fi

# Check for twiggy
if ! command -v twiggy &> /dev/null; then
    echo "Error: twiggy not found."
    echo
    echo "Install twiggy:"
    echo "  cargo install twiggy"
    exit 1
fi

# File size
FILE_SIZE=$(stat -c%s "$WASM_FILE" 2>/dev/null || stat -f%z "$WASM_FILE")
echo "File size: $(numfmt --to=iec $FILE_SIZE 2>/dev/null || echo "$FILE_SIZE bytes")"
echo

# Top items by size
echo "=== Top 30 Items by Size ==="
twiggy top -n 30 "$WASM_FILE"
echo

# Dominators (what transitively requires what)
echo "=== Top 20 Dominators ==="
twiggy dominators -n 20 "$WASM_FILE"
echo

# Summary by monomorphization
echo "=== Monomorphization Bloat (Generic Instantiations) ==="
twiggy monos -n 20 "$WASM_FILE" 2>/dev/null || echo "No monomorphization data available"
echo

# Code paths
echo "=== Top Code Paths ==="
twiggy paths -n 10 "$WASM_FILE" 2>/dev/null || echo "No path data available"
echo

# Garbage collection roots
echo "=== GC Roots ==="
twiggy garbage "$WASM_FILE" 2>/dev/null | head -30 || echo "No garbage data available"
echo

# Diff mode hint
echo "=== Comparing Builds ==="
echo "To compare two WASM builds:"
echo "  twiggy diff old.wasm new.wasm"
echo
echo "To save a JSON report:"
echo "  twiggy top -f json $WASM_FILE > bloat-report.json"
