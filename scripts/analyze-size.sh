#!/bin/bash
# Analyze fOS engine binary size contribution by crate
#
# This script uses cargo-bloat to analyze which crates and functions
# contribute most to the binary size. Useful for identifying optimization
# opportunities.
#
# Usage: ./scripts/analyze-size.sh

set -e

echo "=== fOS Engine Binary Size Analysis ==="
echo

# Check for cargo-bloat
if ! command -v cargo-bloat &> /dev/null; then
    echo "Installing cargo-bloat..."
    cargo install cargo-bloat
fi

# Build release binary
echo "Building release binary..."
cargo build --release -p fos-engine 2>/dev/null || cargo build --release -p fos-engine

echo
echo "=== Binary Size ==="
if [ -f "target/release/libfos_engine.rlib" ]; then
    ls -lh target/release/libfos_engine.rlib
elif [ -f "target/release/libfos_engine.a" ]; then
    ls -lh target/release/libfos_engine.a
fi

# Analyze by crate
echo
echo "=== Size by Crate ==="
cargo bloat --release -p fos-engine --crates 2>/dev/null || echo "cargo bloat failed - try running manually"

# Analyze largest functions
echo
echo "=== Top 20 Largest Functions ==="
cargo bloat --release -p fos-engine -n 20 2>/dev/null || echo "cargo bloat failed - try running manually"

# Compare full vs minimal builds
echo
echo "=== Full vs Minimal Build Comparison ==="

echo "Building with full features..."
cargo build --release -p fos-engine --features full 2>/dev/null || true
FULL_SIZE=$(find target/release -name "libfos_engine*" -type f 2>/dev/null | head -1 | xargs ls -l 2>/dev/null | awk '{print $5}')

echo "Building with minimal features..."
cargo build --release -p fos-engine --no-default-features --features minimal 2>/dev/null || true
MINIMAL_SIZE=$(find target/release -name "libfos_engine*" -type f 2>/dev/null | head -1 | xargs ls -l 2>/dev/null | awk '{print $5}')

if [ -n "$FULL_SIZE" ] && [ -n "$MINIMAL_SIZE" ]; then
    echo "Full build size: $FULL_SIZE bytes"
    echo "Minimal build size: $MINIMAL_SIZE bytes"
    SAVINGS=$((FULL_SIZE - MINIMAL_SIZE))
    if [ "$FULL_SIZE" -gt 0 ]; then
        PERCENT=$((SAVINGS * 100 / FULL_SIZE))
        echo "Savings: $SAVINGS bytes ($PERCENT%)"
    fi
else
    echo "Could not determine build sizes"
fi

echo
echo "=== Analysis Complete ==="
