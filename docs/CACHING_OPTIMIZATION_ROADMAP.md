# Caching Optimization Roadmap: Surpassing Chromium

> Goal: Cache hit rates >95% with intelligent multi-layer caching

## Current State ✅
- HTTP cache
- Layout cache
- Style cache
- Query cache

---

## Phase 1: Multi-Level Cache Architecture (Q1)

### 1.1 Cache Hierarchy
```
L1: CPU Cache (ns)
    └── Hot data structures, inline caches
L2: Process Memory (μs)
    └── Parsed resources, computed styles, layout boxes
L3: Shared Memory (μs)
    └── Shared across tabs: fonts, images, DNS
L4: Disk Cache (ms)
    └── HTTP responses, compiled bytecode
L5: Network (100ms+)
    └── CDN, origin server
```

### 1.2 Cache Manager
```rust
pub struct CacheManager {
    memory_budget: usize,
    disk_budget: usize,
    
    // Per-resource-type caches
    http_cache: HttpCache,
    dom_cache: DomCache,
    style_cache: StyleCache,
    layout_cache: LayoutCache,
    bytecode_cache: BytecodeCache,
    image_cache: ImageCache,
    
    pub fn memory_pressure(&self) -> f32 {
        self.current_usage() as f32 / self.memory_budget as f32
    }
    
    pub fn evict_to_target(&mut self, target: f32) {
        // LRU eviction across all caches
        while self.memory_pressure() > target {
            self.evict_coldest();
        }
    }
}
```

---

## Phase 2: HTTP Caching (Q2)

### 2.1 Advanced Cache Control
| Feature | Chromium | fOS Target |
|---------|----------|------------|
| RFC 7234 compliance | Full | Full |
| stale-while-revalidate | Yes | Yes + predictive |
| Cache partitioning | Yes | Yes |
| Shared dict cache | Partial | Full |

```rust
pub struct HttpCache {
    entries: HashMap<CacheKey, CacheEntry>,
    
    pub fn get(&mut self, request: &Request) -> CacheResult {
        let key = self.cache_key(request);
        
        if let Some(entry) = self.entries.get(&key) {
            if entry.is_fresh() {
                return CacheResult::Fresh(entry.response.clone());
            }
            if entry.can_stale_while_revalidate() {
                // Return stale, revalidate in background
                self.revalidate_async(key.clone(), entry);
                return CacheResult::Stale(entry.response.clone());
            }
            return CacheResult::MustRevalidate(entry.validators());
        }
        CacheResult::Miss
    }
}
```

### 2.2 Predictive Caching
```rust
pub struct PredictiveCache {
    navigation_model: MarkovChain<Url>,
    
    pub fn predict_next_resources(&self, current: &Url) -> Vec<Url> {
        // Based on navigation patterns
        self.navigation_model.likely_next(current, 0.5)
    }
    
    pub fn prefetch_predicted(&self) {
        for url in self.predict_next_resources(&self.current_url) {
            if !self.http_cache.contains(&url) {
                self.fetch_low_priority(url);
            }
        }
    }
}
```

---

## Phase 3: Style Caching (Q3)

### 3.1 Computed Style Cache
```rust
pub struct StyleCache {
    // Share identical computed styles
    by_hash: HashMap<u64, Arc<ComputedStyle>>,
    
    // Cache by element signature
    by_signature: HashMap<ElementSignature, Arc<ComputedStyle>>,
}

pub struct ElementSignature {
    tag: TagId,
    classes: SmallVec<[ClassId; 4]>,
    pseudo_state: PseudoState,
    parent_style_hash: u64,
}

impl StyleCache {
    pub fn get_or_compute(
        &mut self,
        element: &Element,
        compute: impl FnOnce() -> ComputedStyle
    ) -> Arc<ComputedStyle> {
        let sig = ElementSignature::from(element);
        
        self.by_signature.entry(sig)
            .or_insert_with(|| Arc::new(compute()))
            .clone()
    }
}
```

### 3.2 Selector Match Cache
```rust
// Cache selector match results per element
pub struct SelectorMatchCache {
    // Bloom filter for "definitely doesn't match"
    negative_bloom: BloomFilter<16>,
    
    // LRU cache for "definitely matches"
    positive_cache: LruCache<(ElementId, SelectorId), bool>,
}
```

---

## Phase 4: Layout Caching (Q4)

### 4.1 Constraint-Based Cache
```rust
pub struct LayoutCache {
    entries: HashMap<LayoutCacheKey, LayoutResult>,
}

pub struct LayoutCacheKey {
    node_id: NodeId,
    available_width: AvailableSize,
    available_height: AvailableSize,
    writing_mode: WritingMode,
}

impl LayoutCache {
    pub fn get_or_compute(
        &mut self,
        node: &LayoutNode,
        constraints: &LayoutConstraints,
        compute: impl FnOnce() -> LayoutResult
    ) -> &LayoutResult {
        let key = LayoutCacheKey::new(node, constraints);
        
        self.entries.entry(key)
            .or_insert_with(compute)
    }
}
```

### 4.2 Intrinsic Size Cache
```rust
// Min/max content sizes rarely change
pub struct IntrinsicSizeCache {
    sizes: HashMap<NodeId, IntrinsicSizes>,
    generation: u64,  // Invalidate on DOM change
}
```

---

## Phase 5: Bytecode Caching

### 5.1 JavaScript Bytecode Cache
```rust
pub struct BytecodeCache {
    // Disk-backed cache for compiled JS
    disk_cache: DiskCache<ScriptHash, CompiledScript>,
    
    pub fn get_compiled(&mut self, source: &str, url: &Url) -> Option<CompiledScript> {
        let hash = self.hash_source(source, url);
        self.disk_cache.get(&hash)
    }
    
    pub fn store_compiled(&mut self, source: &str, url: &Url, compiled: &CompiledScript) {
        let hash = self.hash_source(source, url);
        self.disk_cache.put(hash, compiled.clone());
    }
}
```

### 5.2 WASM Module Cache
```rust
pub struct WasmModuleCache {
    // Compiled WASM modules (expensive to compile)
    modules: HashMap<ModuleHash, CompiledWasmModule>,
    
    // Streaming compilation results
    streaming: HashMap<Url, StreamingCompileState>,
}
```

---

## Phase 6: Image Caching

### 6.1 Decoded Image Cache
```rust
pub struct ImageCache {
    // Decoded images by URL
    decoded: LruCache<Url, DecodedImage>,
    
    // Resized versions
    resized: LruCache<(Url, Size), DecodedImage>,
    
    pub fn get_resized(&mut self, url: &Url, size: Size) -> Option<&DecodedImage> {
        let key = (url.clone(), size);
        
        if let Some(img) = self.resized.get(&key) {
            return Some(img);
        }
        
        // Generate resized version from original
        if let Some(original) = self.decoded.get(url) {
            let resized = original.resize(size);
            self.resized.put(key.clone(), resized);
            return self.resized.get(&key);
        }
        
        None
    }
}
```

### 6.2 Texture Atlas Cache
```rust
pub struct TextureAtlasCache {
    atlases: Vec<TextureAtlas>,
    entries: HashMap<ImageId, AtlasEntry>,
    
    pub fn get_or_upload(&mut self, image: &DecodedImage) -> AtlasEntry {
        // Reuse existing atlas entry or upload new
    }
}
```

---

## Chromium Comparison

| Cache Type | Chromium | fOS Target |
|------------|----------|------------|
| HTTP cache | SQLite | Custom (faster) |
| Style sharing | Basic | Advanced signature |
| Layout cache | Per-element | Constraint-based |
| Bytecode cache | V8 cache | Custom format |
| Predictive prefetch | Limited | ML-based |

---

## Cache Hit Rate Targets

| Cache | Chromium | fOS Target |
|-------|----------|------------|
| HTTP | 50% | 70% |
| DNS | 80% | 95% |
| Style | 60% | 90% |
| Layout | 40% | 80% |
| Bytecode | 70% | 95% |
| Images | 80% | 95% |
| **Overall** | 60% | 90% |

---

## Implementation Priority

1. **Style cache** - Most frequent lookups
2. **Layout cache** - Expensive to recompute
3. **Bytecode cache** - Parse/compile is slow
4. **Predictive HTTP** - Reduce latency
5. **Image resize cache** - Common operation
