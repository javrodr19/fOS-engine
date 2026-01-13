# Startup Optimization Roadmap: Surpassing Chromium

> Goal: Sub-100ms cold start, instant warm start

## Current State
- Single binary architecture
- Some lazy initialization

---

## Phase 1: Binary Optimization (Q1)

### 1.1 Binary Size Reduction
| Build | Chromium | fOS Current | fOS Target |
|-------|----------|-------------|------------|
| Release | 120 MB | ~30 MB | 15 MB |
| Minimal | N/A | N/A | 5 MB |
| Core only | N/A | N/A | 2 MB |

```rust
// Cargo.toml optimizations
[profile.release]
opt-level = "z"          # Optimize for size
lto = "fat"              # Full LTO for dead code elimination
codegen-units = 1        # Better optimization
panic = "abort"          # Smaller panic handling
strip = true             # Strip symbols
```

### 1.2 Feature Flags
```rust
// Compile-time feature gates
#[cfg(feature = "webrtc")]
mod webrtc;

#[cfg(feature = "devtools")]
mod devtools;

#[cfg(feature = "media")]
mod media;

// Minimal build excludes heavy features
// cargo build --release --no-default-features --features "core,networking"
```

---

## Phase 2: Lazy Initialization (Q2)

### 2.1 Deferred Subsystem Loading
| Subsystem | Chromium Load | fOS Load |
|-----------|---------------|----------|
| UI | Immediate | Immediate |
| Networking | Immediate | Immediate |
| JS Engine | Immediate | On first script |
| DevTools | Immediate | On F12 |
| Extensions | Immediate | On use |
| Media | Immediate | On first media |

```rust
pub struct LazySubsystem<T> {
    init: fn() -> T,
    instance: OnceCell<T>,
}

impl<T> LazySubsystem<T> {
    pub fn get(&self) -> &T {
        self.instance.get_or_init(self.init)
    }
}

// Example: JS engine not loaded until first <script>
static JS_ENGINE: LazySubsystem<JsEngine> = LazySubsystem::new(JsEngine::new);
```

### 2.2 Lazy Font Loading
```rust
pub struct FontDatabase {
    // Only load font metadata on startup
    metadata: Vec<FontMetadata>,
    
    // Load actual font data on demand
    loaded_fonts: HashMap<FontId, LazyFont>,
}

impl FontDatabase {
    pub fn get_font(&mut self, id: FontId) -> &Font {
        self.loaded_fonts.entry(id)
            .or_insert_with(|| LazyFont::load(id))
            .get()
    }
}
```

---

## Phase 3: Parallel Startup (Q3)

### 3.1 Concurrent Initialization
```rust
pub fn startup() {
    // Phase 1: Critical path (serial, <10ms)
    init_memory_allocator();
    init_main_window();
    
    // Phase 2: Parallel init
    rayon::scope(|s| {
        s.spawn(|_| init_network_stack());
        s.spawn(|_| init_font_database());
        s.spawn(|_| init_cookie_store());
        s.spawn(|_| init_cache());
    });
    
    // Phase 3: Deferred (after first paint)
    schedule_idle(|| {
        init_service_worker_manager();
        init_extension_system();
        init_sync_engine();
    });
}
```

### 3.2 Startup Timeline Target
```
0ms     - Process start
5ms     - Memory allocator ready
10ms    - Main window created
20ms    - Event loop running
50ms    - Network ready
80ms    - First navigation possible
100ms   - First paint complete
```

---

## Phase 4: Warm Start (Q4)

### 4.1 Process Prefork Pool
```rust
// Pre-spawn renderer processes during idle time
pub struct ProcessPool {
    ready_renderers: Vec<PreforkedProcess>,
    
    pub fn get_renderer(&mut self) -> PreforkedProcess {
        if let Some(proc) = self.ready_renderers.pop() {
            // Instant: return pre-spawned process
            self.replenish_async();
            proc
        } else {
            // Fallback: spawn fresh
            PreforkedProcess::spawn()
        }
    }
}
```

### 4.2 Session Restore Optimization
```rust
pub struct SessionRestore {
    // Prioritize visible tab
    pub fn restore(&self, session: &Session) {
        // 1. Restore active tab immediately
        let active = &session.tabs[session.active_index];
        self.restore_tab_full(active);
        
        // 2. Restore visible metadata for other tabs
        for tab in &session.tabs {
            self.restore_tab_minimal(tab);
        }
        
        // 3. Background restore remaining tabs
        schedule_idle(|| {
            for tab in session.tabs.iter().skip(1) {
                self.restore_tab_full(tab);
            }
        });
    }
}
```

---

## Phase 5: Precomputation

### 5.1 Build-Time Computation
```rust
// Perfect hash for HTML tag names (computed at build time)
const HTML_TAGS: PerfectHashMap<&str, TagId> = phf_map! {
    "div" => TagId::Div,
    "span" => TagId::Span,
    // ... all 110+ HTML tags
};

// Pre-generated shader bytecode
const PRECOMPILED_SHADERS: &[u8] = include_bytes!("shaders.spv");
```

### 5.2 First-Run Warmup
```rust
// On first run or update, pre-warm caches
pub fn first_run_warmup() {
    // Pre-compile regex patterns
    compile_common_regexes();
    
    // Pre-JIT common JS patterns
    warmup_js_jit();
    
    // Pre-load common fonts
    preload_system_fonts();
    
    // Build shader cache
    compile_gpu_shaders();
}
```

---

## Phase 6: Speculative Startup

### 6.1 Predictive Preloading
```rust
pub struct StartupPredictor {
    frequent_sites: Vec<(Url, f32)>,  // URL, probability
    
    pub fn predict_navigation(&self) -> Vec<Url> {
        // Based on time of day, history
        self.frequent_sites.iter()
            .filter(|(_, prob)| *prob > 0.3)
            .map(|(url, _)| url.clone())
            .collect()
    }
}

// Speculatively preconnect/prefetch on browser start
pub fn speculative_warmup(predictor: &StartupPredictor) {
    for url in predictor.predict_navigation().take(3) {
        dns_prefetch(&url);
        preconnect(&url);
    }
}
```

---

## Chromium Comparison

| Metric | Chromium | fOS Target | Improvement |
|--------|----------|------------|-------------|
| Cold start | 500ms | 100ms | 5x faster |
| Warm start | 200ms | 20ms | 10x faster |
| First paint | 150ms | 50ms | 3x faster |
| Memory at start | 100 MB | 30 MB | 70% less |
| Binary size | 120 MB | 15 MB | 88% less |

---

## Implementation Priority

1. **Lazy subsystems** - Biggest immediate impact
2. **Parallel init** - Uses available cores
3. **Binary optimization** - Reduces I/O time
4. **Process prefork** - Instant tab creation
5. **Speculative warmup** - Perceived instant
