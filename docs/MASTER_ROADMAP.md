# fOS-engine Master Roadmap

> Path to matching and surpassing Chromium with a lean, optimized, dependency-free browser engine

---

## 🎯 Core Principles

| Principle | Description |
|-----------|-------------|
| **Zero Dependencies** | Custom implementations over external crates |
| **Maximum Performance** | SIMD, cache-friendly, lock-free where possible |
| **Memory Efficiency** | Tiered memory, compression, compact representations |
| **Rust Safety** | Eliminate UAF, buffer overflows, data races |

---

## 📚 Component Roadmaps

| Component | Roadmap | Priority | Est. Effort |
|-----------|---------|----------|-------------|
| **Networking** | [NETWORKING_ROADMAP.md](./NETWORKING_ROADMAP.md) | P0 | 3 months |
| **Rendering** | [RENDERING_ROADMAP.md](./RENDERING_ROADMAP.md) | P0 | 4 months |
| **DOM/HTML** | [DOM_HTML_ROADMAP.md](./DOM_HTML_ROADMAP.md) | P0 | 2 months |
| **CSS** | [CSS_ROADMAP.md](./CSS_ROADMAP.md) | P0 | 3 months |
| **Layout** | [LAYOUT_ROADMAP.md](./LAYOUT_ROADMAP.md) | P0 | 3 months |
| **JavaScript** | [JAVASCRIPT_ROADMAP.md](./JAVASCRIPT_ROADMAP.md) | P0 | 6 months |
| **Security** | [SECURITY_ROADMAP.md](./SECURITY_ROADMAP.md) | P1 | 4 months |
| **Media** | [MEDIA_ROADMAP.md](./MEDIA_ROADMAP.md) | P1 | 6 months |
| **Architecture** | [ARCHITECTURE_ROADMAP.md](./ARCHITECTURE_ROADMAP.md) | P1 | 4 months |
| **Text** | [TEXT_RENDERING_ROADMAP.md](./TEXT_RENDERING_ROADMAP.md) | P1 | 3 months |
| **Accessibility** | [ACCESSIBILITY_ROADMAP.md](./ACCESSIBILITY_ROADMAP.md) | P2 | 2 months |
| **DevTools** | [DEVTOOLS_ROADMAP.md](./DEVTOOLS_ROADMAP.md) | P2 | 3 months |

---

## 🚀 Optimization Roadmaps

> **Goal: Surpass Chromium by 2-4x in key metrics**

| Optimization | Roadmap | Target Improvement |
|--------------|---------|-------------------|
| **SIMD** | [SIMD_OPTIMIZATION_ROADMAP.md](./SIMD_OPTIMIZATION_ROADMAP.md) | 2-4x parse/render speed |
| **Memory** | [MEMORY_OPTIMIZATION_ROADMAP.md](./MEMORY_OPTIMIZATION_ROADMAP.md) | 50-70% less memory |
| **Startup** | [STARTUP_OPTIMIZATION_ROADMAP.md](./STARTUP_OPTIMIZATION_ROADMAP.md) | Sub-100ms cold start |
| **Caching** | [CACHING_OPTIMIZATION_ROADMAP.md](./CACHING_OPTIMIZATION_ROADMAP.md) | 95% cache hit rates |
| **Concurrency** | [CONCURRENCY_OPTIMIZATION_ROADMAP.md](./CONCURRENCY_OPTIMIZATION_ROADMAP.md) | Full core utilization |
| **Energy** | [ENERGY_OPTIMIZATION_ROADMAP.md](./ENERGY_OPTIMIZATION_ROADMAP.md) | 40% less power |
| **Profiling** | [PROFILING_ROADMAP.md](./PROFILING_ROADMAP.md) | Performance tracking |

---

## 🗓️ Quarterly Plan

### Q1: Foundation
```
├── Networking: HTTP/2 & QUIC hardening
├── DOM/HTML: Parser optimization, CoW nodes
├── CSS: Selector bloom, :has() support
├── Layout: Flexbox/Grid fast paths
└── JS: Inline caches, baseline JIT start
```

### Q2: Performance
```
├── Rendering: GPU pipeline, tile optimization
├── CSS: Style sharing, incremental styling
├── Layout: Parallel layout, caching
├── JS: Baseline JIT complete, GC improvements
└── Text: Custom shaper (Latin, Arabic, CJK)
```

### Q3: Features
```
├── Security: Process separation started
├── Media: Core codec implementations
├── CSS: All modern features
├── Layout: Full spec compliance
└── Accessibility: Platform integration
```

### Q4: Polish
```
├── Architecture: Multi-process complete
├── Security: Full sandboxing
├── Media: Hardware acceleration
├── JS: Optimizing JIT
└── DevTools: CDP compatibility
```

---

## 📊 Chromium Comparison Summary

| Area | Gap | Priority |
|------|-----|----------|
| JS Performance | V8 ~2x faster | 🔴 High |
| Multi-process | Missing | 🔴 High |
| Video Codecs | Missing | 🟡 Medium |
| WebAssembly | Missing | 🟡 Medium |
| Web Compat | Unknown | 🟡 Test |
| Text Shaping | HarfBuzz-level | 🟢 Good progress |
| All else | Comparable | 🟢 Good |

---

## 🏆 Unique Advantages (vs Chromium)

| Feature | Impact |
|---------|--------|
| **Rust memory safety** | Zero UAF/buffer overflow CVEs |
| **Tiered memory** | 50% less memory usage |
| **Persistent DOM** | 10x faster cloneNode |
| **Single binary** | 75% smaller distribution |
| **Zero C/C++** | Simpler build, security |

---

## 🎯 Milestone Targets

### M1: Basic Browsing (Current + 3 months)
- [x] Render HTML/CSS correctly
- [x] Execute JavaScript
- [ ] Pass Acid3 test
- [ ] 80% CSS2.1 tests

### M2: Modern Web (Current + 6 months)
- [ ] Pass 90% WPT for core features
- [ ] YouTube playback
- [ ] Gmail/Google Docs usable
- [ ] 60fps scrolling

### M3: Production Ready (Current + 12 months)
- [ ] Multi-process architecture
- [ ] Full V8 parity (80% benchmark)
- [ ] All major video codecs
- [ ] Security audit passed

---

## 📁 File Structure

```
docs/
├── MASTER_ROADMAP.md               # This file (index)
│
├── Component Roadmaps
│   ├── NETWORKING_ROADMAP.md       # HTTP, QUIC, WebSocket
│   ├── RENDERING_ROADMAP.md        # GPU, compositing, paint
│   ├── DOM_HTML_ROADMAP.md         # Parser, DOM ops, memory
│   ├── CSS_ROADMAP.md              # Selectors, cascade, animations
│   ├── LAYOUT_ROADMAP.md           # Flexbox, Grid, tables
│   ├── JAVASCRIPT_ROADMAP.md       # JIT, GC, optimization
│   ├── SECURITY_ROADMAP.md         # Sandbox, isolation
│   ├── MEDIA_ROADMAP.md            # Codecs, WebRTC
│   ├── ARCHITECTURE_ROADMAP.md     # Process model, IPC
│   ├── TEXT_RENDERING_ROADMAP.md   # Fonts, shaping
│   ├── ACCESSIBILITY_ROADMAP.md    # ARIA, screen readers
│   └── DEVTOOLS_ROADMAP.md         # CDP, debugging
│
└── Optimization Roadmaps
    ├── SIMD_OPTIMIZATION_ROADMAP.md      # AVX-512, NEON vectorization
    ├── MEMORY_OPTIMIZATION_ROADMAP.md    # Tiered memory, compression
    ├── STARTUP_OPTIMIZATION_ROADMAP.md   # Cold/warm start
    ├── CACHING_OPTIMIZATION_ROADMAP.md   # Multi-layer caching
    ├── CONCURRENCY_OPTIMIZATION_ROADMAP.md # Lock-free, parallel
    ├── ENERGY_OPTIMIZATION_ROADMAP.md    # Power efficiency
    └── PROFILING_ROADMAP.md              # Benchmarks, metrics
```

---

## 🔧 Development Guidelines

### Dependency Policy
```
✅ Allowed: std, core, alloc
✅ Allowed: Platform APIs (Linux/macOS/Windows)
⚠️  Review: Crypto primitives, compression
❌ Banned: Large frameworks (tokio for non-async I/O)
❌ Banned: C/C++ bindings (FFmpeg, Skia, HarfBuzz)
```

### Performance Rules
1. Profile before optimizing
2. Prefer SIMD for hot paths
3. Cache-oblivious algorithms where possible
4. Zero-copy by default
5. Measure memory, not just speed
