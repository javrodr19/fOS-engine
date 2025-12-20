# fOS Engine Development Phases

## Overview

| Phase | Duration | Focus | Status |
|-------|----------|-------|--------|
| 1 | 2-3 months | HTML/CSS Parsing & DOM | ✅ Complete |
| 2 | 3-4 months | Layout Engine | ✅ Complete |
| 3 | 2-3 months | Rendering | ✅ Complete |
| 4 | 1-2 months | JavaScript Integration | ✅ Complete |
| 5 | 6+ months | Web APIs | ✅ Complete |
| 6 | 3+ months | Media & Advanced Features | ✅ Complete |
| 7 | Ongoing | Optimization & Polish | ✅ Complete |

---

## Phase 1: HTML/CSS Parsing & DOM (2-3 months)

### Goals
- Parse HTML5 documents into DOM tree
- Parse CSS stylesheets
- Build style cascade system
- Memory-efficient DOM representation

### Deliverables
- [x] `fos-html`: HTML5 parser wrapper around html5ever
- [x] `fos-css`: CSS parser using lightningcss
- [x] `fos-dom`: DOM tree with efficient memory layout
- [x] Basic style computation (cascade, specificity)
- [x] Unit tests for parsing edge cases

### Success Criteria
- Parse any valid HTML5 document
- Compute styles for all elements
- RAM usage < 10MB for parser alone

---

## Phase 2: Layout Engine (3-4 months)

### Goals
- Implement CSS box model
- Basic block/inline layout
- Text measurement and wrapping
- Flexbox support
- Grid support (basic)

### Deliverables
- [x] `fos-layout`: Layout tree generation
- [x] Box model (margin, border, padding, content)
- [x] Block formatting context
- [x] Inline formatting context
- [x] Flexbox layout algorithm
- [x] Text shaping (use cosmic-text or rustybuzz)

### Success Criteria
- Correctly layout 80% of popular websites
- Layout computation < 50ms for typical pages

---

## Phase 3: Rendering (2-3 months)

### Goals
- Paint layout tree to pixels
- GPU acceleration (optional)
- Efficient invalidation and repainting

### Deliverables
- [x] `fos-render`: Painting engine
- [x] CPU rendering with tiny-skia
- [x] GPU rendering with wgpu (optional)
- [x] Text rendering
- [x] Image decoding and display
- [x] Borders, backgrounds, shadows

### Success Criteria
- Render pages at 60fps on mid-range hardware
- Support for common image formats (PNG, JPEG, WebP, GIF)

---

## Phase 4: JavaScript Integration (1-2 months)

### Goals
- Embed QuickJS JavaScript engine
- Bridge DOM to JavaScript
- Event handling

### Deliverables
- [x] `fos-js`: QuickJS wrapper
- [x] DOM bindings (document, element, etc.)
- [x] Event system (click, input, etc.)
- [x] Console API
- [x] Timers (setTimeout, setInterval)

### Success Criteria
- Run basic JavaScript on pages
- DOM manipulation works
- Event listeners fire correctly

---

## Phase 5: Web APIs (6+ months)

### Goals
- Implement essential Web APIs
- Network requests from JavaScript
- Storage APIs

### Deliverables
- [x] `fetch()` API
- [x] XMLHttpRequest (legacy support)
- [x] `localStorage` / `sessionStorage`
- [x] `history` API
- [x] `location` API
- [x] Canvas 2D API
- [x] WebSocket

### Success Criteria
- YouTube homepage renders (may not play video yet)
- GitHub pages work
- Most static sites functional

---

## Phase 6: Media & Advanced Features (3+ months)

### Goals
- Video and audio playback
- WebGL (optional)
- Web Workers

### Deliverables
- [x] `<video>` / `<audio>` elements (GStreamer backend)
- [x] Media Source Extensions (MSE)
- [x] WebGL 1.0 (optional)
- [x] Web Workers

### Success Criteria
- YouTube video playback
- Most media-heavy sites work

---

## Phase 7: Optimization & Polish (Ongoing)

### Goals
- Performance optimization
- Memory reduction
- Compatibility improvements

### Focus Areas
- [x] Memory profiling and optimization
- [x] Lazy loading and virtualization
- [x] Caching strategies
- [x] Compatibility testing with top 1000 sites
- [x] Accessibility (a11y) support

---

## RAM Targets

| Scenario | Target | Stretch Goal |
|----------|--------|--------------|
| Engine idle | 15 MB | 10 MB |
| Simple page (1 tab) | 30 MB | 20 MB |
| Complex page (1 tab) | 80 MB | 60 MB |
| 5 tabs average | 200 MB | 150 MB |
| 10 tabs average | 350 MB | 250 MB |

---

## Technology Stack

| Component | Library | Reason |
|-----------|---------|--------|
| HTML Parser | html5ever | Battle-tested, used by Servo |
| CSS Parser | lightningcss | Fast, modern, used by Parcel |
| JavaScript | rquickjs (QuickJS) | Tiny (~2MB), fast startup |
| CPU Rendering | tiny-skia | Pure Rust, no dependencies |
| GPU Rendering | wgpu | Cross-platform, modern |
| Networking | reqwest | Async, TLS support |
| Text Shaping | cosmic-text | Rust-native, good perf |
| Async Runtime | smol | Lightweight alternative to tokio |

