# fOS Engine Development Phases

## Overview

| Phase | Duration | Focus | Status |
|-------|----------|-------|--------|
| 1 | 2-3 months | HTML/CSS Parsing & DOM | 🔄 In Progress |
| 2 | 3-4 months | Layout Engine | ⏳ Planned |
| 3 | 2-3 months | Rendering | ⏳ Planned |
| 4 | 1-2 months | JavaScript Integration | ⏳ Planned |
| 5 | 6+ months | Web APIs | ⏳ Planned |
| 6 | 3+ months | Media & Advanced Features | ⏳ Planned |
| 7 | Ongoing | Optimization & Polish | ⏳ Planned |

---

## Phase 1: HTML/CSS Parsing & DOM (2-3 months)

### Goals
- Parse HTML5 documents into DOM tree
- Parse CSS stylesheets
- Build style cascade system
- Memory-efficient DOM representation

### Deliverables
- [ ] `fos-html`: HTML5 parser wrapper around html5ever
- [ ] `fos-css`: CSS parser using lightningcss
- [ ] `fos-dom`: DOM tree with efficient memory layout
- [ ] Basic style computation (cascade, specificity)
- [ ] Unit tests for parsing edge cases

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
- [ ] `fos-layout`: Layout tree generation
- [ ] Box model (margin, border, padding, content)
- [ ] Block formatting context
- [ ] Inline formatting context
- [ ] Flexbox layout algorithm
- [ ] Text shaping (use cosmic-text or rustybuzz)

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
- [ ] `fos-render`: Painting engine
- [ ] CPU rendering with tiny-skia
- [ ] GPU rendering with wgpu (optional)
- [ ] Text rendering
- [ ] Image decoding and display
- [ ] Borders, backgrounds, shadows

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
- [ ] `fos-js`: QuickJS wrapper
- [ ] DOM bindings (document, element, etc.)
- [ ] Event system (click, input, etc.)
- [ ] Console API
- [ ] Timers (setTimeout, setInterval)

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
- [ ] `fetch()` API
- [ ] XMLHttpRequest (legacy support)
- [ ] `localStorage` / `sessionStorage`
- [ ] `history` API
- [ ] `location` API
- [ ] Canvas 2D API
- [ ] WebSocket

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
- [ ] `<video>` / `<audio>` elements (GStreamer backend)
- [ ] Media Source Extensions (MSE)
- [ ] WebGL 1.0 (optional)
- [ ] Web Workers

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
- [ ] Memory profiling and optimization
- [ ] Lazy loading and virtualization
- [ ] Caching strategies
- [ ] Compatibility testing with top 1000 sites
- [ ] Accessibility (a11y) support

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
