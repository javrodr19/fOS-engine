# Layout Roadmap: Chromium Parity & Beyond

> Goal: LayoutNG-equivalent performance with zero layout thrashing

## Current State ✅
- Block, inline, Flexbox, Grid, table, multicolumn
- Subgrid, layout cache, streaming layout
- Constraint caching, invalidation tracking

---

## Phase 1: Layout Algorithm Optimization (Q1)

### 1.1 Flexbox Optimization
| Feature | Chromium | fOS Status | Action |
|---------|----------|------------|--------|
| Single-pass layout | Partial | ❌ | Implement |
| Cached flex factors | Yes | Partial | Complete cache |
| Intrinsic sizing | Spec-accurate | ✅ | Optimize |
| Nested flex | Multi-pass | Single-pass | Flatten |

```rust
// Single-pass Flexbox when possible
pub fn layout_flex_fast_path(container: &FlexContainer) -> Option<FlexLayout> {
    // Fast path conditions:
    // - No flex-wrap
    // - No min/max constraints
    // - No align-content variations
    if container.can_use_fast_path() {
        Some(compute_single_pass(container))
    } else {
        None // Fall back to full algorithm
    }
}
```

### 1.2 Grid Optimization
```rust
pub struct GridLayoutCache {
    // Cache track sizing for similar grids
    track_cache: HashMap<TrackSizingKey, Vec<TrackSize>>,
    
    // Reuse placement for static grids
    placement_cache: HashMap<PlacementKey, GridPlacement>,
}
```

---

## Phase 2: Layout Caching (Q2)

### 2.1 Constraint-Based Caching
| Scenario | Chromium | fOS Target |
|----------|----------|------------|
| Same constraints | Skip layout | Skip layout |
| Width-only change | Partial relayout | Skip if height-independent |
| Child change | Subtree relayout | Minimal invalidation |

```rust
pub struct LayoutConstraint {
    available_width: AvailableSize,
    available_height: AvailableSize,
    percentage_base: Size,
}

pub struct LayoutCache {
    constraints: LayoutConstraint,
    result: LayoutResult,
    // Cache hit if new constraints compatible
}
```

### 2.2 Delta Layout
```rust
pub enum LayoutDelta {
    Position(Point),      // Just moved, same size
    Size(Size),           // Size changed
    Subtree,              // Children changed
    Full,                 // Full relayout needed
}

pub fn compute_delta(old: &Layout, change: &StyleChange) -> LayoutDelta {
    // Return minimal delta for incremental update
}
```

---

## Phase 3: Layout Features (Q3)

### 3.1 Missing Layout Features
| Feature | Priority | Status |
|---------|----------|--------|
| Subgrid | ✅ | Done |
| Masonry | Medium | Implement |
| Container queries layout | High | Implement |
| Anchor positioning | Medium | Implement |
| Math layout | Low | Implement |

### 3.2 Intrinsic Sizing
```rust
pub fn compute_intrinsic_sizes(node: &LayoutNode) -> IntrinsicSizes {
    IntrinsicSizes {
        min_content_width: compute_min_content(node),
        max_content_width: compute_max_content(node),
        // Cache these - expensive to recompute
    }
}
```

---

## Phase 4: Performance Optimization (Q4)

### 4.1 Parallel Layout
```rust
pub fn parallel_layout(root: LayoutNode) -> LayoutTree {
    // Phase 1: Compute intrinsic sizes (parallel)
    rayon::scope(|s| {
        for subtree in root.independent_subtrees() {
            s.spawn(|_| compute_intrinsic_sizes(subtree));
        }
    });
    
    // Phase 2: Resolve sizes (sequential, uses constraints)
    resolve_sizes(root);
    
    // Phase 3: Position children (parallel)
    rayon::scope(|s| {
        for subtree in root.independent_subtrees() {
            s.spawn(|_| position_children(subtree));
        }
    });
}
```

### 4.2 Layout Prediction
```rust
pub struct LayoutPredictor {
    // Predict layout during parse
    pub fn predict_during_parse(&self, token: &HtmlToken) -> Option<LayoutHint> {
        // "This looks like a header" -> reserve top space
    }
    
    // Pre-layout likely scroll targets
    pub fn speculative_layout(&self, viewport: Rect, scroll_vel: Vec2) {
        // Layout content we're scrolling toward
    }
}
```

---

## Phase 5: Surpassing Chromium

### 5.1 Unique Optimizations
| Feature | Description | Chromium? |
|---------|-------------|-----------|
| **Sparse matrix layout** | Skip empty grid cells | No |
| **Layout streaming** | Layout during download | Partial |
| **Constraint dedup** | Share identical constraints | No |
| **Predictive layout** | Pre-layout scroll targets | No |

### 5.2 Memory Optimization
```rust
pub struct CompactLayoutResult {
    // Pack common layout data tightly
    x: i16,           // Relative to parent (covers ±32K)
    y: i16,
    width: u16,
    height: u16,
    // 8 bytes total vs 32+ bytes typical
}
```

---

## Benchmarks Target

| Metric | Chromium | fOS Target |
|--------|----------|------------|
| Initial layout (1000 nodes) | 20ms | 10ms |
| Relayout (class change) | 5ms | 1ms |
| Flex layout (100 children) | 2ms | 0.5ms |
| Grid layout (10x10) | 1ms | 0.3ms |
| Memory per box | 200 bytes | 50 bytes |

---

## Dependencies Policy

### Keep
- None required

### Custom Implementation Priority
1. Fast constraint solver
2. Parallel layout coordinator
3. Layout cache with delta updates
4. Compact layout result storage
