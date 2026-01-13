# CSS Roadmap: Chromium Parity & Beyond

> Goal: Fastest CSS engine with full spec compliance and minimal memory

## Current State ✅
- Full selector parsing, cascade, computed styles
- CSS variables, transitions, web animations
- Style sharing, Bloom filters, rule tree

---

## Phase 1: Selector Performance (Q1)

### 1.1 Selector Matching Optimization
| Technique | Chromium | fOS Status | Action |
|-----------|----------|------------|--------|
| Bloom filter | 8-hash | ✅ `selector_bloom.rs` | Tune parameters |
| Ancestor filter | Yes | Partial | Implement fully |
| Right-to-left matching | Yes | ✅ | Optimize |
| Selector specificity cache | Yes | ❌ | Implement |

```rust
// Optimized selector matching
pub struct SelectorMatcher {
    bloom: AncestorBloom<8>,
    specificity_cache: LruCache<SelectorId, Specificity>,
    // Fast path for common selectors
    id_selectors: HashMap<InternedString, Vec<RuleId>>,
    class_selectors: HashMap<InternedString, Vec<RuleId>>,
    tag_selectors: HashMap<TagName, Vec<RuleId>>,
}
```

### 1.2 Selector Splitting
```rust
// Split complex selectors for parallel matching
pub fn parallelize_selector(sel: &Selector) -> Vec<SelectorFragment> {
    // ".foo .bar .baz" -> [".foo", ".bar", ".baz"]
    // Match fragments independently, combine results
}
```

---

## Phase 2: Style Computation (Q2)

### 2.1 Cascade Optimization
| Feature | Chromium | fOS Target |
|---------|----------|------------|
| Style sharing | Sibling-based | Subtree + content hash |
| Computed cache | Per-element | Structural sharing |
| Inherited props | Copy | Copy-on-write |
| Custom props | HashMap | Flat array |

```rust
pub struct ComputedStyleCache {
    // Share identical computed styles
    styles: HashSet<Arc<ComputedStyle>>,
    
    // Fast lookup by content hash
    by_hash: HashMap<u64, Arc<ComputedStyle>>,
}
```

### 2.2 Incremental Styling
```rust
pub struct StyleInvalidation {
    // Track which selectors might match
    changed_classes: HashSet<InternedString>,
    changed_attrs: HashSet<InternedString>,
    
    // Invalidate minimal subtree
    pub fn invalidate(&self, tree: &DomTree) -> Vec<NodeId> {
        // Use Bloom filters to find affected nodes
    }
}
```

---

## Phase 3: CSS Features Parity (Q3)

### 3.1 Missing CSS Features
| Feature | Priority | Status |
|---------|----------|--------|
| Container queries | ✅ | Done |
| :has() selector | High | Implement |
| Nesting | High | Implement |
| @layer | High | Implement |
| @scope | Medium | Implement |
| Anchor positioning | Medium | Implement |
| View transitions | Low | Implement |

### 3.2 :has() Optimization
```rust
// :has() requires subject-finding, expensive
pub fn match_has_selector(subject: NodeId, rel: &RelativeSelector) -> bool {
    // Cache :has() results aggressively
    // Invalidate on subtree mutations
}
```

---

## Phase 4: Animation Performance (Q4)

### 4.1 Animation Threading
| Animation | Main Thread | Compositor |
|-----------|-------------|------------|
| transform | ❌ | ✅ |
| opacity | ❌ | ✅ |
| filter | ❌ | ✅ |
| clip-path | ❌ | ✅ |
| color | ✅ | ❌ |
| width/height | ✅ | ❌ |

### 4.2 Keyframe Optimization
```rust
pub struct KeyframePrecompute {
    // Pre-interpolate keyframe values
    samples: Vec<ComputedValue>,
    sample_count: usize, // e.g., 60 for 1s @ 60fps
}
```

---

## Phase 5: Surpassing Chromium

### 5.1 Unique Optimizations
| Feature | Description | Chromium? |
|---------|-------------|-----------|
| **Delta styles** | Only store diffs | Partial |
| **Structural sharing** | CoW for computed | No |
| **Predictive styling** | Pre-compute likely states | No |
| **JIT selectors** | Compile hot selectors | No |

### 5.2 JIT Selector Compilation
```rust
// Compile frequently-used selectors to native code
pub fn compile_selector(sel: &Selector) -> CompiledMatcher {
    // Generate x64/ARM code for matching
    // Inline Bloom filter checks
}
```

---

## Benchmarks Target

| Metric | Chromium | fOS Target |
|--------|----------|------------|
| Style 10K elements | 50ms | 25ms |
| Selector match (avg) | 0.5μs | 0.2μs |
| Recalc (class change) | 10ms | 2ms |
| Memory per element | 200 bytes | 100 bytes |

---

## Dependencies Policy

### Keep
- None required

### Custom Implementation Priority
1. Fast CSS parser (SIMD tokenizer)
2. Optimized Bloom filter
3. Structural sharing for computed styles
4. JIT selector matching
