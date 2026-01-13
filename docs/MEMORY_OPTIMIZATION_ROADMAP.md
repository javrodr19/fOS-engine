# Memory Optimization Roadmap: Surpassing Chromium

> Goal: 50-70% less memory than Chromium with zero performance penalty

## Current State ✅
- Tiered memory (hot/warm/cold) in `fos-engine`
- String interning
- Arena allocators
- Compressed pointers

---

## Phase 1: Allocation Strategy (Q1)

### 1.1 Custom Allocators
| Allocator | Use Case | vs System Alloc |
|-----------|----------|-----------------|
| Bump/Arena | Temp parsing data | 10x faster |
| Pool | Fixed-size objects (nodes) | 5x faster |
| Slab | Variable but bounded | 3x faster |
| Tiered | Long-lived varying access | 50% less memory |

```rust
// Per-thread bump allocator for parsing
pub struct BumpArena {
    chunks: Vec<Box<[u8; 64 * 1024]>>,
    current: *mut u8,
    end: *mut u8,
}

impl BumpArena {
    pub fn alloc<T>(&mut self) -> &mut MaybeUninit<T> {
        // Fast path: just bump the pointer
        let ptr = self.current;
        self.current = unsafe { ptr.add(size_of::<T>()) };
        unsafe { &mut *(ptr as *mut MaybeUninit<T>) }
    }
    
    pub fn reset(&mut self) {
        // Free everything instantly
        self.current = self.chunks[0].as_mut_ptr();
    }
}
```

### 1.2 Object Pooling
```rust
pub struct NodePool {
    free_list: Vec<NodeId>,
    nodes: Vec<Option<Node>>,
    
    pub fn alloc(&mut self) -> NodeId {
        self.free_list.pop().unwrap_or_else(|| {
            let id = NodeId(self.nodes.len());
            self.nodes.push(None);
            id
        })
    }
    
    pub fn free(&mut self, id: NodeId) {
        self.nodes[id.0] = None;
        self.free_list.push(id);
    }
}
```

---

## Phase 2: Data Compression (Q2)

### 2.1 In-Memory Compression
| Data Type | Uncompressed | Compressed | Ratio |
|-----------|--------------|------------|-------|
| Inactive DOM | 20 MB | 3 MB | 7:1 |
| Style sheets | 5 MB | 0.5 MB | 10:1 |
| JS bytecode | 10 MB | 2 MB | 5:1 |
| History | 100 MB | 10 MB | 10:1 |

```rust
// LZ4 for warm tier (fast decompress)
pub struct CompressedStorage<T> {
    compressed: Vec<u8>,
    original_len: usize,
    
    pub fn decompress(&self) -> Vec<T> {
        lz4_flex::decompress(&self.compressed, self.original_len)
    }
}

// Zstd for cold tier (best ratio)
pub struct ColdStorage<T> {
    file: File,
    index: BTreeMap<Key, (u64, u32)>, // offset, len
}
```

### 2.2 Semantic Compression
```rust
// DOM pattern compression (identify repeated structures)
pub struct DomCompressor {
    patterns: HashMap<PatternHash, PatternId>,
    
    pub fn compress(&mut self, subtree: &Node) -> CompressedDom {
        // Find repeated subtrees (nav, footer, list items)
        // Store once, reference multiple times
    }
}
```

---

## Phase 3: Compact Representations (Q3)

### 3.1 Compressed Pointers
```rust
// 32-bit indices instead of 64-bit pointers
pub struct CompactPtr<T> {
    index: u32,
    _marker: PhantomData<T>,
}

// Saves 4 bytes per pointer (half the size)
// ~40% of DOM node size is pointers
```

### 3.2 Small String Optimization
```rust
pub enum CompactString {
    // Inline for ≤23 bytes (no allocation)
    Inline { len: u8, data: [u8; 23] },
    // Interned for common strings (shared)
    Interned(InternedId),
    // Heap for rare long strings
    Heap(Box<str>),
}
```

### 3.3 Compact Numbers
```rust
// Variable-length integers for DOM indices
pub struct VarInt(u32);  // 1-5 bytes depending on value

// Fixed-point for layout (4 bytes vs 8 for f64)
pub struct LayoutUnit(i32);  // 1/64 px precision
```

---

## Phase 4: Memory Tiering (Q4)

### 4.1 Hot/Warm/Cold Architecture
```rust
pub struct TieredCache<K, V> {
    hot: LruCache<K, V>,           // In memory, uncompressed
    warm: LruCache<K, Compressed<V>>, // In memory, compressed
    cold: DiskCache<K, V>,         // On disk
    
    hot_budget: usize,
    warm_budget: usize,
    
    pub fn get(&mut self, key: &K) -> Option<&V> {
        // Promote through tiers on access
        if let Some(v) = self.hot.get(key) {
            return Some(v);
        }
        if let Some(compressed) = self.warm.remove(key) {
            let v = compressed.decompress();
            self.hot.put(key.clone(), v);
            return self.hot.get(key);
        }
        if let Some(v) = self.cold.get(key) {
            self.hot.put(key.clone(), v);
            return self.hot.get(key);
        }
        None
    }
}
```

### 4.2 Automatic Tier Migration
```rust
pub struct TierPolicy {
    hot_to_warm_age: Duration,    // 30 seconds
    warm_to_cold_age: Duration,   // 5 minutes
    cold_eviction_age: Duration,  // 1 hour
    
    memory_pressure_threshold: f32, // 0.8
}
```

---

## Phase 5: Tab Memory Management

### 5.1 Tab Hibernation
| Tab State | Memory Usage | Chromium | fOS Target |
|-----------|--------------|----------|------------|
| Active | 100 MB | 100 MB | 60 MB |
| Background (5min) | 100 MB | 100 MB | 20 MB |
| Background (1hr) | 100 MB | 100 MB | 5 MB |
| Hibernated | Discarded | Discarded | 1 MB |

```rust
pub struct TabHibernation {
    pub fn hibernate(&mut self, tab: &mut Tab) {
        // 1. Serialize DOM to compressed format
        let dom_snapshot = self.compress_dom(&tab.dom);
        
        // 2. Serialize JS heap references
        let js_snapshot = self.snapshot_js_heap(&tab.js_runtime);
        
        // 3. Drop live objects
        tab.dom = None;
        tab.js_runtime = None;
        tab.layout_tree = None;
        
        // 4. Store snapshot
        self.snapshots.insert(tab.id, (dom_snapshot, js_snapshot));
    }
    
    pub fn wake(&mut self, tab: &mut Tab) {
        // Restore from snapshot (fast)
        let (dom, js) = self.snapshots.remove(&tab.id).unwrap();
        tab.dom = Some(self.decompress_dom(dom));
        tab.js_runtime = Some(self.restore_js_heap(js));
    }
}
```

---

## Phase 6: Memory-Mapped Resources

### 6.1 Font Sharing
```rust
// Share font files across all tabs via mmap
pub struct SharedFontCache {
    fonts: HashMap<FontKey, Mmap>,
    
    pub fn load_font(&mut self, path: &Path) -> &[u8] {
        self.fonts.entry(FontKey::from_path(path))
            .or_insert_with(|| Mmap::open(path).unwrap())
    }
}
```

### 6.2 Image Caching
```rust
pub struct ImageCache {
    // Memory-mapped decoded images
    decoded: HashMap<Url, MappedImage>,
    
    // Disk cache for encoded images
    encoded: DiskCache<Url, Vec<u8>>,
}
```

---

## Chromium Comparison

| Aspect | Chromium | fOS Target |
|--------|----------|------------|
| Per-tab overhead | 50 MB | 20 MB |
| Background tabs | Full memory | Compressed |
| String storage | Basic interning | Multi-level |
| DOM nodes | 200+ bytes | <80 bytes |
| Style objects | Per-element | Shared |
| Hibernation | Tab discard | Snapshot/restore |

---

## Benchmarks Target

| Metric | Chromium | fOS Target | Improvement |
|--------|----------|------------|-------------|
| Empty tab | 50 MB | 15 MB | 70% less |
| Gmail | 300 MB | 100 MB | 67% less |
| 10 tabs | 1 GB | 300 MB | 70% less |
| 50 tabs | 5 GB | 800 MB | 84% less |
| Cold start | 100 MB | 30 MB | 70% less |

---

## Implementation Priority

1. **String interning** - Low effort, high impact
2. **Compact node representation** - DOM is largest
3. **Tab hibernation** - Many users have 50+ tabs
4. **Tiered caching** - Automatic optimization
5. **Memory-mapped resources** - Share across processes
