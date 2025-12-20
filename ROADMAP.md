# fOS Engine - Complete Roadmap to Chromium Compatibility

## Current Status

| Component | Coverage | Lines |
|-----------|----------|-------|
| HTML Parser | Basic | ~500 |
| CSS Parser | ~30 props | ~1000 |
| DOM | Basic tree | ~800 |
| Layout | Block/Inline/Flex | ~1500 |
| Rendering | Shapes only | ~1000 |
| JavaScript | QuickJS core | ~800 |
| Networking | HTTP/1.1 | ~500 |
| **Total** | ~2% | ~6,100 |

---

# Phase 1-7: Foundation (COMPLETE)

> Core engine implementation complete. See PHASES.md for details.

---

# Phase 8: Text Rendering (3-6 months)

## 8.1 Core Features

### Font Loading
- [x] Font file parsing (TTF, OTF, WOFF, WOFF2)
- [x] System font enumeration
- [x] @font-face CSS rule
- [x] Font matching algorithm
- [x] Font fallback chains

### Text Shaping
- [x] Integrate rustybuzz (HarfBuzz port)
- [x] Unicode BiDi algorithm
- [x] Script detection
- [x] Ligatures and kerning
- [x] Variable fonts

### Text Layout
- [x] Line breaking (UAX #14)
- [x] Word wrapping
- [x] Text-align (left, right, center, justify)
- [x] Vertical text (writing-mode)
- [x] Ruby annotations

### Text Rendering
- [x] Glyph rasterization
- [x] Subpixel antialiasing
- [x] Font hinting
- [x] Emoji support (color fonts)
- [x] Text decorations (underline, strikethrough)

## 8.2 Phase 8 Optimizations

### Font Memory Optimization
- [x] Font subsetting (only used glyphs)
- [x] Font glyph streaming (load on demand)
- [x] Shared font cache across tabs
- [x] mmap font files (OS paging)

### Text Rendering Optimization
- [x] Pre-rendered glyph atlas (common ASCII)
- [x] Text run caching (same font+text → same glyphs)
- [x] Flyweight for glyph metrics
- [x] 80% shaping time savings

---

# Phase 9: Image Support (2-4 months)

## 9.1 Core Features

### Image Decoding
- [x] PNG (via image crate)
- [x] JPEG
- [x] GIF (animated)
- [x] WebP
- [x] AVIF
- [x] SVG (via resvg)
- [x] ICO/favicon

### Image Rendering
- [x] Image scaling (bilinear, bicubic)
- [x] Aspect ratio handling
- [x] object-fit, object-position
- [x] Background images
- [x] Image sprites
- [x] Lazy loading (loading="lazy")

### Image Optimization
- [x] Progressive decoding
- [x] Memory-mapped loading
- [x] Thumbnail caching
- [x] Responsive images (srcset)

## 9.2 Phase 9 Optimizations

### Bitmap Memory Optimization
- [x] RGB565 for opaque images (2 bytes/pixel vs 4)
- [x] Decode images to exact display size
- [x] Tile large images, load visible tiles only
- [x] Pool bitmap memory across images
- [x] Release offscreen bitmaps immediately

### Image Loading Optimization
- [x] Image decode on scroll (viewport-only)
- [x] Image decode priority queue (visible first)
- [x] Cancel offscreen decodes
- [x] Memory-bounded decode queue
- [x] Content-addressable image cache

---

# Phase 10: Complete CSS (6-12 months)

## 10.1 Core Features

### Box Model Extensions
- [x] box-shadow
- [x] outline
- [x] overflow (scroll, hidden, auto)
- [x] clip-path
- [x] mask

### Visual Effects
- [x] opacity
- [x] filter (blur, brightness, etc.)
- [x] backdrop-filter
- [x] mix-blend-mode
- [x] isolation

### Transforms
- [x] transform (rotate, scale, skew, translate)
- [x] transform-origin
- [x] perspective
- [x] 3D transforms
- [x] backface-visibility

### Animations
- [x] transition
- [x] @keyframes
- [x] animation properties
- [x] Animation timing functions
- [x] Web Animations API

### Grid Layout
- [x] grid-template-columns/rows
- [x] grid-gap
- [x] grid-area
- [x] Implicit grid
- [x] Auto-placement
- [x] Subgrid

### Table Layout
- [x] table-layout
- [x] border-collapse
- [x] caption-side
- [x] Table cell spanning

### Multi-column
- [x] column-count
- [x] column-width
- [x] column-gap
- [x] column-rule

### Advanced Selectors
- [x] Attribute selectors ([attr^=], [attr$=])
- [x] Pseudo-elements (::before, ::after)
- [x] Pseudo-classes (:nth-child, :not, :is, :where)
- [x] :has() selector
- [x] Container queries

### CSS Variables
- [x] Custom properties (--var)
- [x] var() function
- [x] calc()
- [x] min(), max(), clamp()

## 10.2 Phase 10 Optimizations

### CSS Style Sharing (Servo-inspired)
- [x] Computed style cache (hash-based lookup)
- [x] Share style structs across identical elements
- [x] Inherit-only properties stored once per cascade level
- [x] Rule tree (share common selectors)
- [x] Bloom filter for selector matching

### Selector Optimization
- [x] Bloom filter for fast rejection
- [x] Hash selectors for O(1) lookup
- [x] Right-to-left matching optimization
- [x] Hybrid Interpreted/Compiled selectors

### CSS Value Optimization
- [x] Bit-Packed CSS Values (4 bytes each)
- [x] CSS Property Presence Bitmask (64 bytes)
- [x] Interned color values (u8 index)
- [x] Sentinel values for optional numerics

### Style Calculation Optimization
- [x] Deterministic style cache
- [x] On-demand style calculation (hidden elements)
- [x] Style inheritance snapshots
- [x] CSS Custom Property hoisting
- [x] Shared computed style objects

---

# Phase 11: Forms & Input (3-6 months)

## 11.1 Core Features

### Form Elements
- [x] `<input>` (all types: text, password, email, number, date, etc.)
- [x] `<textarea>`
- [x] `<select>`, `<option>`
- [x] `<button>`
- [x] `<form>`
- [x] `<label>`
- [x] `<fieldset>`, `<legend>`

### Input Handling
- [x] Keyboard events
- [x] Mouse events
- [x] Touch events
- [x] Focus management
- [x] Selection API
- [x] Clipboard API
- [x] Drag and drop

### Form Validation
- [x] HTML5 validation attributes
- [x] Constraint Validation API
- [x] Custom validity
- [x] :valid, :invalid pseudo-classes

## 11.2 Phase 11 Optimizations

### Event Optimization
- [x] Event listener coalescing (1 handler per type)
- [x] Event handler deduplication
- [x] Lazy event binding

---

# Phase 12: Complete DOM API (6-12 months)

## 12.1 Core Features

### Node Operations
- [x] appendChild, removeChild, insertBefore
- [x] cloneNode
- [x] replaceChild
- [x] normalize
- [x] DocumentFragment

### Element API
- [x] querySelector, querySelectorAll
- [x] getElementsByClassName
- [x] getElementsByTagName
- [x] closest, matches
- [x] classList
- [x] dataset
- [x] attributes (get/set/remove)

### Geometry APIs
- [x] getBoundingClientRect
- [x] getClientRects
- [x] offsetTop/Left/Width/Height
- [x] scrollTop/Left/Width/Height
- [x] IntersectionObserver
- [x] ResizeObserver

### Mutation APIs
- [x] MutationObserver
- [x] DOM change events

### Shadow DOM
- [x] attachShadow
- [x] Shadow root
- [x] Slots
- [x] CSS scoping

### Custom Elements
- [x] customElements.define
- [x] Lifecycle callbacks
- [x] Autonomous elements
- [x] Customized built-in elements

## 12.2 Phase 12 Optimizations

### Compact DOM Representation
- [x] Node struct: 32 bytes max (vs typical 100+)
- [x] Inline small text (<24 bytes in node)
- [x] Attribute storage: 2 inline, overflow to arena
- [x] Child pointers: single linked list (save 8 bytes)
- [x] Element names: u16 ID (not String)

### DOM Data Structure Optimizations
- [x] Arena allocation (all nodes contiguous)
- [x] ECS-Style data layout
- [x] SmallVec for children (<8 inline)
- [x] Packed enums (#[repr(u8)])
- [x] Bitfield flags (8 bools in 1 byte)

### DOM Query Optimization
- [x] Selector-result memoization
- [x] DOM generation IDs (O(1) validation)
- [x] Deduplicated attribute storage

### String Optimization
- [x] String interning (tag names, attributes)
- [x] Borrowed DOM strings (zero-alloc parsing)
- [x] Zero-copy parsing into arena

---

# Phase 13: Complete JavaScript (6-12 months)

## 13.1 Core Features

### ES2020+ Features
- [x] async/await
- [x] Optional chaining (?.)
- [x] Nullish coalescing (??)
- [x] Private class fields
- [x] Top-level await
- [x] BigInt
- [x] WeakRef

### Built-in Objects
- [x] Promise (full spec)
- [x] Map, Set, WeakMap, WeakSet
- [x] Symbol
- [x] Proxy, Reflect
- [x] SharedArrayBuffer
- [x] Atomics

### Web APIs in JS
- [x] URL, URLSearchParams
- [x] FormData
- [x] AbortController
- [x] TextEncoder/TextDecoder
- [x] Blob, File
- [x] FileReader

## 13.2 Phase 13 Optimizations

### JavaScript Heap Optimization
- [x] Limit heap per context (configurable, default 64MB)
- [x] Compress idle context heap
- [x] Share builtins across contexts
- [x] Immediate GC on tab hide

### JS Execution Optimization
- [x] Lazy function compilation
- [x] Dead code elimination
- [x] Constant folding
- [x] Escape analysis (stack-allocate non-escaping)
- [x] JIT-less mode (smaller binary)
- [x] Bytecode caching

### JS Binding Optimization
- [x] Lazy JavaScript binding (bind on access)
- [x] 80% binding memory savings

---

# Phase 14: Advanced Web APIs (12-24 months)

## 14.1 Core Features

### Storage
- [x] IndexedDB
- [x] Cache API
- [x] Cookies API

### Networking
- [x] WebSocket (full spec)
- [x] HTTP/2
- [x] HTTP/3 / QUIC
- [x] Server-Sent Events
- [x] Beacon API

### Workers
- [x] Full Web Workers
- [x] Shared Workers
- [x] Service Workers
- [x] Worklets

### Geolocation & Sensors
- [x] Geolocation API
- [x] DeviceOrientation
- [x] Sensor APIs

### Notifications
- [x] Notifications API
- [x] Push API
- [x] Vibration API

### Permissions
- [x] Permissions API
- [x] Permission prompts

## 14.2 Phase 14 Optimizations

### Network Optimization
- [x] HTTP/3 with QUIC (multiplexed streams)
- [x] Request coalescing (batch small requests)
- [x] Predictive DNS resolution
- [x] Global connection pool
- [x] Delta sync protocol

### Resource Optimization
- [x] Cross-tab immutable resource sharing
- [x] Content-addressable caching
- [x] Resource deduplication

---

# Phase 15: Canvas & Graphics (6-12 months)

## 15.1 Core Features

### Canvas 2D
- [x] CanvasRenderingContext2D
- [x] Path drawing
- [x] Text rendering
- [x] Image drawing
- [x] Compositing
- [x] Transformations
- [x] OffscreenCanvas

### WebGL
- [x] WebGL 1.0
- [x] Shader compilation
- [x] Texture handling
- [x] Framebuffers
- [x] Extensions

### WebGL 2.0
- [x] WebGL 2 context
- [x] Transform feedback
- [x] Uniform buffer objects

### WebGPU (Future)
- [x] GPUDevice
- [x] Render pipelines
- [x] Compute shaders

## 15.2 Phase 15 Optimizations

### Rendering Optimization
- [x] Display list compilation (GPU command buffer)
- [x] Texture atlas packing
- [x] Dirty rectangle fusion
- [x] Occluded element culling
- [x] Render tree diffing

### GPU Optimization
- [x] GPU-accelerated layout
- [x] Tile-based rendering
- [x] Layer management

---

# Phase 16: Media (6-12 months)

## 16.1 Core Features

### Audio/Video
- [x] `<video>` element (full)
- [x] `<audio>` element (full)
- [x] Playback controls
- [x] Fullscreen API
- [x] Picture-in-Picture

### Media Decoding
- [x] H.264
- [x] H.265 / HEVC
- [x] VP8 / VP9
- [x] AV1
- [x] AAC, MP3, Opus, Vorbis

### Media Source Extensions
- [x] MSE API
- [x] SourceBuffer
- [x] Adaptive streaming (DASH, HLS)

### Encrypted Media
- [x] EME API
- [x] Widevine CDM
- [x] Clear Key

### WebRTC
- [x] RTCPeerConnection
- [x] MediaStream
- [x] Data channels
- [x] Screen sharing

### Web Audio
- [x] AudioContext
- [x] Audio nodes
- [x] Spatial audio
- [x] Audio worklets

## 16.2 Phase 16 Optimizations

### Media Loading Optimization
- [x] Lazy feature loading (load on first use)
- [x] Media codecs via system libraries
- [x] Plugin model (.so files)

---

# Phase 17: Security (3-6 months)

## 17.1 Core Features

### Same-Origin Policy
- [x] Cross-origin restrictions
- [x] CORS handling
- [x] CSP enforcement

### Secure Contexts
- [x] HTTPS enforcement
- [x] Mixed content blocking
- [x] Certificate validation

### Sandboxing
- [x] Process isolation
- [x] JS sandbox
- [x] iframe sandbox

### Privacy
- [x] Tracking protection
- [x] Cookie policies
- [x] Referrer policy

---

# Phase 18: Accessibility (3-6 months)

## 18.1 Core Features

### ARIA Support
- [x] ARIA roles
- [x] ARIA states/properties
- [x] Live regions

### Screen Reader Support
- [x] Accessibility tree
- [x] Text alternatives
- [x] Focus indicators

### Keyboard Navigation
- [x] Tab order
- [x] Focus trapping
- [x] Skip links

---

# Phase 19: DevTools (6-12 months)

## 19.1 Core Features

### Element Inspector
- [ ] DOM tree view
- [ ] Style inspection
- [ ] Computed styles

### Console
- [ ] console.* methods
- [ ] Error display
- [ ] Object inspection

### Network Panel
- [ ] Request logging
- [ ] Timing information
- [ ] Headers/body inspection

### Performance Panel
- [ ] Frame timing
- [ ] Memory profiling
- [ ] CPU profiling

### Debugger
- [ ] Breakpoints
- [ ] Step execution
- [ ] Variable inspection

---

# Phase 20: Performance & Polish (Ongoing)

## 20.1 Core Features

### Rendering Pipeline
- [ ] GPU compositing
- [ ] Layer management
- [ ] Tile-based rendering
- [ ] Occlusion culling

### Parsing Optimizations
- [ ] Speculative parsing
- [ ] Preload scanner
- [ ] Resource hints (preload, prefetch)

### JavaScript Optimization
- [ ] JIT compilation (optional)
- [ ] Inline caching
- [ ] Hidden classes

### Memory Optimization
- [ ] Incremental GC
- [ ] Memory compression
- [ ] DOM compression

---

# Phase 21: Advanced Memory Optimization (Ongoing)

## 21.1 Core Memory Techniques

### Arena Allocation
- [x] DOM node arena (all nodes in contiguous memory)
- [x] Layout tree arena (per-frame allocation)
- [ ] CSS style arena (shared across elements)
- [x] Bump allocator for temporary objects
- [ ] Arena recycling between page loads

### Zero-Copy Parsing
- [ ] HTML: Parse directly into arena, no intermediate copies
- [ ] CSS: Slice references to source text
- [ ] JSON: String views instead of allocations
- [ ] URLs: Lazy parsing, keep as bytes

### String Interning
- [x] Tag names (only ~100 unique HTML tags)
- [x] Attribute names (class, id, style, etc.)
- [x] CSS property names (~500 properties)
- [ ] Common CSS values (auto, none, inherit)

### Tab Hibernation
- [x] Serialize inactive tab DOM to disk
- [ ] Free JS heap for background tabs
- [ ] Compress hibernated state (zstd)
- [ ] Wake on demand (<100ms)
- [ ] DOM Lazy Serialization (background tabs only)

### Memory-Mapped I/O
- [ ] mmap large resources (fonts, images)
- [ ] Let OS page in/out as needed
- [ ] Share mmapped resources across tabs
- [ ] Reduce peak memory for large files
- [ ] mmap font files
- [ ] mmap large images
- [ ] Let OS handle paging

### Memory Pressure Response
- [x] Monitor system memory pressure
- [x] Proactively hibernate tabs at 70% memory
- [x] Reduce cache limits under pressure
- [ ] Release all non-essential buffers

### Lazy Loading
- [ ] Viewport-only layout computation
- [ ] Image decode on scroll
- [ ] Font subsetting (only used glyphs)
- [ ] Deferred script parsing

## 21.2 Tab & Resource Sharing

### Cooperative Tab Model
- [ ] Single JS context shared, sandboxed per tab
- [ ] One layout engine, time-sliced
- [ ] Shared font/image caches (no per-tab duplication)
- [ ] Thread pool instead of process-per-tab

### Resource Deduplication
- [x] Content-addressable image cache
- [ ] Dedupe identical stylesheets
- [ ] Share decoded fonts across tabs
- [ ] Single copy of common scripts (jQuery, React)

### Cross-Tab Immutable Resource Sharing
- [x] Content-addressed global store (SHA256 → Arc<[u8]>)
- [ ] Fonts, images, scripts shared across all tabs
- [ ] Reference counting, not copying
- [ ] 10x savings for N tabs with same resources

## 21.3 Data Structure Optimizations

### Flyweight Pattern
- [ ] Font flyweights (share glyph metrics)
- [ ] Style flyweights (intrinsic style properties)
- [ ] Factory for identical CSS declarations
- [ ] Separate extrinsic state (position, size)

### Rope Data Structure (for Text)
- [ ] Tree-based string storage (O(log n) insert)
- [ ] Avoid full buffer copies on edit
- [ ] Efficient for large document editing
- [ ] Memory-efficient line handling

### Copy-on-Write (COW)
- [x] COW for cloned DOM trees
- [x] COW for style inheritance
- [x] COW for image buffers
- [x] Transparent duplication on modification

### SmallVec Pattern
- [ ] Inline storage for <8 children (no heap)
- [ ] Inline attributes (<4 per element)
- [ ] Inline style properties
- [ ] Spill to heap only when exceeded

### Packed Enums
- [ ] Use `#[repr(u8)]` for node types
- [ ] Compact display enum (1 byte)
- [ ] CSS unit type (1 byte)
- [ ] Combine tag + flags in single u32

### Bitfield Flags
- [x] Node flags: 8 bools in 1 byte
- [ ] Style presence bits
- [ ] Layout dirty bits
- [ ] Event listener flags

### Generational References
- [x] Slot maps (prevent dangling)
- [x] Generation + index (8 bytes total)
- [x] Safe without GC overhead

### Generational/Region GC Patterns
- [ ] Young generation in arena (fast alloc/free)
- [ ] Old generation for long-lived objects
- [ ] Scope-based deallocation for parsing

## 21.4 Compression Strategies

### In-Memory Compression
- [x] LZ4 for hibernated tabs (fast)
- [ ] Zstd for disk cache (high ratio)
- [ ] Compress strings >1KB
- [ ] Decompress on demand

### Delta Encoding
- [x] Store style differences from parent
- [ ] Layout deltas between frames
- [ ] Incremental DOM updates

### Varint Encoding
- [x] Variable-length integers for IDs
- [x] Compact serialization
- [ ] Smaller hibernation format

## 21.5 Streaming/Incremental Processing

### Streaming Processing
- [ ] Parse-as-you-receive (no buffering full document)
- [ ] Incremental layout (dirty regions only)
- [ ] Incremental style calculation
- [ ] Progressive image rendering

---

# Phase 22: Binary Size Optimization (COMPLETE)

## 22.1 Compile-Time Optimization

### Compile-Time Stripping
- [x] Remove panic messages (`panic = abort`)
- [x] Strip debug info (`strip = "symbols"`)
- [x] Disable unwinding
- [x] Remove unused format strings

### Conditional Compilation
- [x] Feature flags for optional components
- [x] `#[cfg]` to exclude WebGL, Media, etc.
- [x] Minimal default, additive features

### Code Generation Optimization
- [x] Aggressive inlining for hot paths
- [x] Avoid monomorphization bloat (dyn Trait)
- [x] Use `#[inline(never)]` for cold paths

## 22.2 Dependency Optimization

### Dependency Minimization
- [x] Audit each crate's size contribution
- [x] Replace heavy crates with minimal alternatives
- [x] Use `cargo bloat` and `cargo tree`

### External Dependencies (Plugin Model)
- [x] WebGL as optional .so plugin
- [x] Media codecs via system libraries
- [x] Font rendering via system (FreeType)

## 22.3 Post-Processing

### WASM-Specific
- [x] wasm-opt (Binaryen, 15-30% smaller)
- [x] wasm-snip (remove unused functions)
- [x] twiggy (analyze bloat)

### Native-Specific
- [x] strip --strip-all
- [x] objcopy --strip-unneeded
- [x] upx compression (80% smaller)

---

# Phase 23: Low-Level Optimizations (Ongoing)

## 23.1 Rust Optimizations

### Struct Packing & Layout
- [x] Order fields largest to smallest
- [x] `#[repr(C)]` for predictable layout
- [x] Align to cache lines (64 bytes)

### Custom Allocators
- [x] Use mimalloc (30% faster)
- [x] Slab allocator for DOM nodes
- [x] Arena for parsing phase
- [x] Pool for layout phase

## 23.2 SIMD Optimization

### SIMD Acceleration
- [x] SIMD for layout calculations
- [x] SIMD for color blending
- [ ] SIMD HTML tag detection
- [ ] SIMD CSS tokenization
- [x] Platform-specific intrinsics (AVX, NEON)

## 23.3 Layout Optimizations

### Fixed-Point Arithmetic
- [x] Use i32 fixed-point (16.16) vs f32
- [x] Deterministic cross-platform
- [x] 50% memory vs f64

### Relative Coordinates
- [ ] Store relative to parent
- [ ] Only absolute at paint time

### Skip Invisible Content
- [x] visibility: hidden → skip paint
- [x] display: none → skip layout entirely
- [x] offscreen → skip until scroll

---

# Phase 24: Experimental Optimizations (Future)

## 24.1 Novel Ideas (fOS Original)

### Semantic DOM Compression
- [ ] Recognize repeating DOM patterns (cards, lists, rows)
- [ ] Store as: Template ID + slot values
- [ ] 80%+ savings for repetitive content (feeds, products)
- [ ] Pattern learning from first render

### Predictive Layout Cache
- [ ] Hash(DOM structure + viewport) → cached layout
- [ ] On revisit, skip entire layout phase if match
- [ ] Persist cache to disk between sessions
- [ ] 100% layout skip on repeat visits

### Borrowed DOM Strings (Zero-Alloc Parsing)
- [ ] Never own text strings—slice original HTML
- [ ] Keep source buffer alive during page lifetime
- [ ] All TextNodes are `&'src str` references
- [ ] 50% text memory, zero copy parsing

### Speculative Offscreen Eviction
- [ ] Track subtree visibility over time
- [ ] After 5s invisible, serialize to temp file
- [ ] Keep only bounding box + file offset
- [ ] Reconstruct on scroll near
- [ ] Long pages use constant memory

### Hybrid Interpreted/Compiled CSS Selectors
- [ ] Top 100 selectors → compile to Rust functions
- [ ] Rare selectors → interpret at runtime
- [ ] 10x faster matching for hot paths
- [ ] Runtime JIT for selector functions

### Sentinel Values (Avoid Option Padding)
- [ ] Use NaN/MAX as "none" for numerics
- [ ] 4 bytes instead of 8 (Option<f32>)
- [ ] Macro to wrap/unwrap sentinels
- [ ] 50% savings on optional numerics

### DOM Generation IDs
- [ ] Each node has generation counter
- [ ] Increment on any mutation
- [ ] If unchanged, all cached values valid
- [ ] O(1) subtree validation

### Progressive Fidelity Rendering
- [ ] Pass 1: Solid boxes (1ms, interactive)
- [ ] Pass 2: Borders, images (5ms)
- [ ] Pass 3: Subpixel text, shadows (20ms)
- [ ] Interrupt on scroll, restart from pass 1

## 24.2 More Experimental Ideas

### Structural Sharing (Persistent Data Structures)
- [ ] Immutable DOM with path copying
- [ ] Undo/redo for free
- [ ] Share unchanged subtrees between versions
- [ ] Like Clojure's persistent vectors

### Lazy Attribute Parsing
- [ ] Store attributes as raw bytes initially
- [ ] Parse only when accessed
- [ ] Many attributes never read (data-*, aria-*)
- [ ] 30% parsing time savings

### Tiered Memory (Hot/Warm/Cold)
- [ ] Hot: Current viewport (fastest access)
- [ ] Warm: ±2 screens (in RAM, maybe compressed)
- [ ] Cold: Rest of document (on disk)
- [ ] Automatic migration based on scroll

### Inline Style Deduplication
- [ ] Hash all inline style strings
- [ ] Store once, reference by ID
- [ ] Many elements have identical inline styles
- [ ] 80% inline style memory savings

### Render Tree Pruning
- [ ] Remove from tree if invisible
- [ ] Reconstruct when visible
- [ ] Separate visible/hidden trees
- [ ] Smaller working set

### Network Response Streaming to DOM
- [ ] Pipe HTTP response directly to parser
- [ ] No intermediate buffer
- [ ] Zero-copy from socket to DOM
- [ ] Eliminate buffering memory

### Streaming DOM Construction
- [ ] Don't wait for </html>
- [ ] Render as chunks arrive
- [ ] Layout visible portion first
- [ ] Background parse rest

### Attribute Access Tracking
- [ ] Track which attributes ever accessed
- [ ] On re-parse, skip never-accessed
- [ ] Learn per-site patterns
- [ ] Adaptive optimization

### Layout Constraint Solving Cache
- [ ] Cache flex/grid solutions
- [ ] Same inputs → same outputs
- [ ] Skip solver on relayout
- [ ] 95% layout skip for animations

### Compact Empty Nodes
- [ ] Use minimal struct for empty text nodes
- [ ] Compact comment node storage
- [ ] Share whitespace-only text content
- [ ] 20% less memory (full DOM preserved)

### DOM Diff Compression
- [ ] For undo: store diffs not snapshots
- [ ] Reverse diff to undo
- [ ] 95% smaller undo stack
- [ ] Efficient history

## 24.3 Bleeding-Edge Techniques

### Succinct Data Structures
- [ ] Near information-theoretic minimum space
- [ ] Query without decompression
- [ ] Succinct tries for URL/selector lookup
- [ ] Rank/select operations on bit arrays
- [ ] 90%+ space savings vs naive structures

### Roaring Bitmaps
- [ ] Compressed bitmap sets for node IDs
- [ ] Fast intersection (visible ∩ dirty)
- [ ] Chunk-based encoding (dense/sparse)
- [ ] Used by Lucene, Netflix, Google
- [ ] O(1) set operations

### Sparse Matrices for Layout
- [ ] Only store non-zero flex basis values
- [ ] Sparse grid track definitions
- [ ] Skip empty table cells
- [ ] CSR format for constraint matrices

### Predictive Prefetch (ML-Based)
- [ ] Lightweight model predicts next click
- [ ] Pre-render likely link targets
- [ ] Pre-fetch nearby resources
- [ ] Instant perceived navigation

### DOM Compilation to Bytecode
- [ ] Serialize DOM ops as bytecode
- [ ] Replay for SSR hydration
- [ ] Smaller than JSON serialization
- [ ] Fast interpretation

### Fingerprint-Based Layout Cache
- [ ] Visual fingerprint, not just URL
- [ ] Same layout for similar pages
- [ ] Cross-site optimization
- [ ] Perceptual hashing

### Viewport Prediction
- [ ] Predict scroll direction
- [ ] Pre-layout 2 screens ahead
- [ ] Evict opposite direction
- [ ] Smooth scrolling guaranteed

### Style Inheritance Snapshots
- [ ] Freeze inherited style at element
- [ ] Compare to parent on mutation
- [ ] Skip cascade if parent unchanged
- [ ] Incremental recalculation

### Content-Aware Compression
- [ ] Text: Brotli/zstd
- [ ] Images: Already compressed, skip
- [ ] Scripts: Minify then compress
- [ ] Styles: Property-specific encoding

## 24.4 Extreme Optimizations

### Memory Architecture Revolution
- [ ] Compressed pointers (32-bit relative offsets)
- [ ] Tagged pointers (type in unused low 3 bits)
- [ ] Memory-mapped DOM for hibernated tabs
- [ ] Stack-based layout (zero heap during layout)
- [ ] 32-bit node IDs (4B nodes max, compatibility safe)

### SIMD-Accelerated Parsing
- [ ] SIMD HTML tag detection (scan 16 bytes at once)
- [ ] Vectorized whitespace skipping
- [ ] Parallel UTF-8 validation
- [ ] SIMD CSS tokenization
- [ ] Batch character classification

### Parallel Tokenization
- [ ] Split HTML at safe boundaries
- [ ] Tokenize chunks in parallel threads
- [ ] Merge token streams
- [ ] 4x speedup on multicore

### Grammar-Based DOM Compression
- [ ] Represent repeating patterns as grammar rules
- [ ] S → <div>AB</div>, A → <span class="x">
- [ ] 95% compression for repetitive pages
- [ ] Decompress on access

### Script Content as Blob
- [ ] Don't parse inside <script>
- [ ] Keep as raw byte slice
- [ ] Pass to JS engine as-is
- [ ] 30% less parsing work

### Incremental Re-Parsing
- [ ] Track character ranges
- [ ] Only re-parse edited ranges
- [ ] Reuse unaffected nodes
- [ ] O(edit size) not O(document size)

## 24.5 Rendering Extremes

### Display List Compilation
- [ ] Convert paint ops to GPU command buffer once
- [ ] Replay without CPU involvement
- [ ] Cache compiled lists
- [ ] 10x repaint speed

### Texture Atlas Packing
- [ ] All small images in one GPU texture
- [ ] Single draw call for many images
- [ ] Bin packing algorithm
- [ ] 90% fewer texture binds

### Pre-Rendered Glyph Atlas
- [ ] Render common ASCII to texture at startup
- [ ] Sample from atlas during rendering
- [ ] No per-glyph rasterization
- [ ] 100x text rendering speed

### Dirty Rectangle Fusion
- [ ] Merge nearby dirty rectangles
- [ ] Reduce overdraw
- [ ] Adaptive fusion threshold
- [ ] 50% fewer repaints

### Occluded Element Culling
- [ ] Track which elements are fully covered
- [ ] Skip rendering occluded elements
- [ ] Depth-based visibility
- [ ] 30% render skip on complex pages

## 24.6 JavaScript Extremes

### Heap Snapshot on Tab Switch
- [ ] Serialize entire JS heap to disk
- [ ] Free memory completely
- [ ] Restore on tab activate
- [ ] 0 MB per background tab

### Lazy Function Compilation
- [ ] Parse but don't compile until called
- [ ] Many functions never called
- [ ] Compile on first invocation
- [ ] 50% faster page load

### Dead Code Elimination
- [ ] Static analysis of script
- [ ] Remove unreachable code
- [ ] Tree shaking at runtime
- [ ] Smaller active heap

### Constant Folding
- [ ] Pre-compute constant expressions
- [ ] `1 + 2` → `3` at parse time
- [ ] Reduce runtime computation

### Escape Analysis
- [ ] Detect non-escaping objects
- [ ] Stack-allocate instead of heap
- [ ] No GC for short-lived objects
- [ ] 80% fewer allocations

## 24.7 Network Extremes

### HTTP/3 with QUIC
- [ ] Single connection, multiplexed streams
- [ ] 0-RTT connection resumption
- [ ] Per-stream flow control
- [ ] Faster than HTTP/2

### Request Coalescing
- [ ] Batch multiple small requests
- [ ] Single network round trip
- [ ] Combine CSS/JS fetches
- [ ] 50% fewer requests

### Predictive DNS Resolution
- [ ] Pre-resolve domains in links
- [ ] Background DNS queries
- [ ] Zero DNS latency on click
- [ ] Parse href attributes early

### Global Connection Pool
- [ ] Share TCP connections across tabs
- [ ] Reuse keep-alive connections
- [ ] Connection limit management
- [ ] Fewer handshakes

### Delta Sync Protocol
- [ ] Request only changed bytes
- [ ] Use ETags + Range requests
- [ ] 90% bandwidth savings on reload
- [ ] Diff-based updates

## 24.8 Exotic/Experimental

### WebAssembly DOM Engine
- [ ] Compile hot DOM operations to WASM
- [ ] Near-native speed
- [ ] Portable across platforms
- [ ] Share WASM modules

### GPU-Accelerated Layout
- [ ] Constraint solving on GPU (CUDA/Metal)
- [ ] Parallel flexbox/grid computation
- [ ] 1000x nodes = same time
- [ ] Massively parallel

### Accelerated Selector Matching
- [ ] Bloom filter pre-filtering (exact, not probabilistic)
- [ ] Hash-based selector lookup
- [ ] Compiled hot selectors to Rust functions
- [ ] Always exact matching (no ML prediction)

### Speculative JS Execution
- [ ] Predict likely branches
- [ ] Pre-execute probable paths
- [ ] Rollback on misprediction
- [ ] Faster interactive response

### Persistent Engine Process
- [ ] Keep engine process alive between sessions
- [ ] Pre-warmed memory pools
- [ ] Instant cold start
- [ ] Shared across browser instances

### AOT-Compiled CSS Selectors
- [ ] Compile selectors to machine code at build
- [ ] No interpretation at runtime
- [ ] Load as dynamic library
- [ ] Selector matching in nanoseconds

### Lock-Free Data Structures
- [ ] Lock-free DOM tree updates
- [ ] Concurrent read/write
- [ ] Wait-free style resolution
- [ ] No mutex contention

### Memory Compaction
- [ ] Periodically compact heap
- [ ] Move objects to reduce fragmentation
- [ ] Update all references
- [ ] 20% memory reclaim

### Copy-on-Write Page Tables
- [ ] Share page tables across tabs
- [ ] COW at OS level
- [ ] Only copy modified pages
- [ ] 80% savings for similar tabs

### Branch Prediction Hints
- [ ] `[[likely]]` / `[[unlikely]]` annotations
- [ ] Guide CPU branch predictor
- [ ] Fewer pipeline stalls
- [ ] 5-10% speedup on hot paths

---

# Timeline Estimate

| Phase | Duration | Cumulative |
|-------|----------|------------|
| 8: Text | 6 months | 6 months |
| 9: Images | 4 months | 10 months |
| 10: CSS | 12 months | 22 months |
| 11: Forms | 6 months | 28 months |
| 12: DOM API | 12 months | 40 months |
| 13: JavaScript | 12 months | 52 months |
| 14: Web APIs | 24 months | 76 months |
| 15: Canvas/WebGL | 12 months | 88 months |
| 16: Media | 12 months | 100 months |
| 17: Security | 6 months | 106 months |
| 18: Accessibility | 6 months | 112 months |
| 19: DevTools | 12 months | 124 months |
| 20-24: Optimization | Ongoing | -- |

**Total: ~10 years** (with small team, can be parallelized)

---

# RAM Budget (Aggressive Targets)

| Component | Target | Stretch |
|-----------|--------|---------|
| Core engine | 2 MB | 1 MB |
| DOM (1000 nodes) | 0.5 MB | 0.3 MB |
| CSS styles | 1 MB | 0.5 MB |
| Layout tree | 0.5 MB | 0.3 MB |
| Font cache (shared) | 5 MB | 3 MB |
| Image cache (shared) | 10 MB | 5 MB |
| JS heap | 8 MB | 5 MB |
| Rendering buffers | 3 MB | 2 MB |
| **Total per simple tab** | **~20 MB** | **~12 MB** |
| **Total per complex tab** | **~60 MB** | **~40 MB** |

### Per-Scenario Targets

| Scenario | Target | Stretch |
|----------|--------|---------|
| Engine idle | 15 MB | 10 MB |
| Simple page (1 tab) | 30 MB | 20 MB |
| Complex page (1 tab) | 80 MB | 60 MB |
| 5 tabs average | 150 MB | 100 MB |
| 10 tabs (with hibernation) | 200 MB | 150 MB |

---

# Binary Size Budget (Aggressive Targets)

| Component | Target | Stretch | Notes |
|-----------|--------|---------|-------|
| Core engine | 1 MB | 0.5 MB | LTO + strip |
| HTML parser | 0.3 MB | 0.2 MB | html5ever minimal |
| CSS parser | 0.5 MB | 0.3 MB | lightningcss |
| Layout engine | 0.3 MB | 0.2 MB | |
| JavaScript (QuickJS) | 1 MB | 0.8 MB | |
| Rendering (tiny-skia) | 0.5 MB | 0.3 MB | |
| Text (rustybuzz) | 1.5 MB | 1 MB | |
| Image decoders | 2 MB | 1 MB | Minimal formats |
| Networking (ureq) | 0.3 MB | 0.2 MB | Use ureq over reqwest |
| TLS (rustls) | 1 MB | 0.8 MB | |
| Compression (zstd) | 0.3 MB | 0.2 MB | |
| **Total (no media)** | **~8 MB** | **~5 MB** | |
| Media (optional) | 5 MB | 3 MB | System codecs |
| WebGL (optional) | 3 MB | 2 MB | |
| **Total (full)** | **~16 MB** | **~10 MB** | |

### Build Optimization Flags
```toml
[profile.release]
lto = "fat"
codegen-units = 1
panic = "abort"
strip = "symbols"
opt-level = "z"
```

### Lightweight Alternatives
| Current | Alternative | Savings |
|---------|-------------|---------|
| reqwest | ureq | ~2 MB |
| tokio | smol | ~1 MB |
| image (all) | image (minimal) | ~3 MB |
| wgpu | Optional at runtime | ~10 MB |
