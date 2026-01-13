# Rendering Roadmap: Chromium Parity & Beyond

> Goal: Match RenderingNG performance with zero GPU library dependencies

## Current State ✅
- GPU compositing, tile rendering, display lists
- WebGL, WebGPU, SVG, image decoding
- Occlusion culling, partial invalidation

---

## Phase 1: GPU Pipeline Optimization (Q1)

### 1.1 Rendering Architecture
| Component | Chromium | fOS Status | Action |
|-----------|----------|------------|--------|
| Display list | DisplayItemList | ✅ `display_list.rs` | Optimize serialization |
| Compositor | Viz process | ✅ `compositor.rs` | Add threaded compositing |
| Rasterizer | Skia | Custom | Implement software fallback |
| Tile manager | cc component | ✅ `tile_renderer.rs` | Priority-based scheduling |

### 1.2 GPU Abstraction Layer
```rust
pub trait GpuBackend {
    fn create_texture(&self, desc: TextureDesc) -> Texture;
    fn submit_commands(&self, cmds: &[Command]);
    fn present(&self, surface: Surface);
}

// Zero-dependency backends
pub struct VulkanBackend { /* Direct Vulkan */ }
pub struct MetalBackend { /* Direct Metal */ }
pub struct SoftwareBackend { /* CPU rasterizer */ }
```

**Dependency Policy**: Direct GPU API calls, no wgpu/gfx-hal

---

## Phase 2: Compositing Excellence (Q2)

### 2.1 Layer Tree Optimization
| Feature | Chromium | fOS Target |
|---------|----------|------------|
| Layer promotion | Heuristic | ML-based prediction |
| Layer squashing | Yes | Aggressive squashing |
| Composited layers | Per-element | Per-subtree |
| Memory limit | 512MB | Adaptive (25% GPU RAM) |

```rust
pub struct LayerDecision {
    will_change: bool,
    animation_active: bool,
    opacity_animated: bool,
    transform_animated: bool,
    // Decision: composite separately or paint inline
}
```

### 2.2 Damage Tracking
```rust
pub struct DamageTracker {
    dirty_rects: RoaringBitmap,  // Tile-level tracking
    layer_damage: HashMap<LayerId, Rect>,
    frame_budget_ms: f32,
}
```

| Optimization | Description |
|--------------|-------------|
| Dirty rect fusion | Merge nearby dirty rects |
| Overdraw elimination | Skip fully occluded paints |
| Retained mode | Reuse unchanged tiles |
| Speculative raster | Pre-render likely tiles |

---

## Phase 3: Paint Performance (Q3)

### 3.1 Display List Optimization
| Technique | Impact | Implementation |
|-----------|--------|----------------|
| Op clustering | -20% GPU calls | Group similar ops |
| Instruction batching | -15% overhead | Batch draw calls |
| Shader caching | -30% compile time | Persistent cache |
| Atlas packing | -40% texture switches | Dynamic atlasing |

### 3.2 Text Rendering Pipeline
```rust
pub struct TextPipeline {
    glyph_atlas: GlyphAtlas,        // SDF or bitmap glyphs
    shaped_cache: LruCache<ShapeKey, ShapedText>,
    subpixel_aa: bool,
    lcd_filter: LcdFilter,
}
```

| Feature | Chromium | fOS Target |
|---------|----------|------------|
| Glyph caching | Per-font atlas | Unified SDF atlas |
| Subpixel AA | Platform-specific | Cross-platform LCD |
| Font fallback | System | Custom chain |
| Emoji rendering | COLRv1 | COLRv1 + custom SVG |

---

## Phase 4: Image Pipeline (Q3-Q4)

### 4.1 Decoder Implementation
| Format | Current | Dependency | Action |
|--------|---------|------------|--------|
| PNG | ✅ | None | Optimize SIMD |
| JPEG | ✅ | None | Add progressive |
| WebP | ✅ | None | Add animation |
| AVIF | ✅ | None | Optimize decoding |
| JPEG-XL | ❌ | None | Implement |
| HEIC | ❌ | None | Implement |

### 4.2 Progressive Decoding
```rust
pub struct ProgressiveDecoder {
    format: ImageFormat,
    decoded_scanlines: usize,
    interlace_pass: u8,
    // Render partial image during download
}
```

---

## Phase 5: Animation Performance (Q4)

### 5.1 Animation Threading
| Animation Type | Thread | Notes |
|----------------|--------|-------|
| CSS Transitions | Compositor | No main thread |
| CSS Animations | Compositor | Keyframe pre-compute |
| Web Animations | Main → Compositor | Handoff supported |
| JS-driven | Main thread | requestAnimationFrame |

### 5.2 Jank Prevention
```rust
pub struct FrameScheduler {
    target_fps: u32,
    frame_budget: Duration,
    vsync_aligned: bool,
    work_estimation: WorkEstimator,
}

impl FrameScheduler {
    pub fn should_yield(&self) -> bool {
        self.remaining_budget() < Duration::from_micros(500)
    }
}
```

---

## Phase 6: Surpassing Chromium

### 6.1 Unique Rendering Features
| Feature | Description | Chromium? |
|---------|-------------|-----------|
| **Predictive raster** | Pre-render scroll targets | Partial |
| **Semantic caching** | Cache by DOM meaning | No |
| **Adaptive quality** | Reduce quality under load | No |
| **Energy-aware** | Throttle on battery | Partial |

### 6.2 Memory Optimization
```rust
pub struct TieredTextureManager {
    hot: GpuTexturePool,      // Active tiles
    warm: CompressedPool,     // GPU-compressed
    cold: DiskCache,          // Serialized to disk
    budget: MemoryBudget,
}
```

---

## Benchmarks Target

| Metric | Chromium | fOS Target |
|--------|----------|------------|
| 60fps scroll | 98% | 99.5% |
| Paint time (avg) | 8ms | 5ms |
| First paint | 150ms | 100ms |
| GPU memory | 200MB | 120MB |
| Tile raster time | 4ms | 2ms |

---

## Dependencies Policy

### Keep
- Vulkan/Metal headers only
- No runtime libraries

### Remove/Replace
- wgpu → direct Vulkan/Metal
- image crate → custom decoders
- Skia bindings → custom rasterizer

### Custom Implementation Priority
1. Software rasterizer (accessibility)
2. Vulkan backend
3. Metal backend
4. GPU shader compiler cache
