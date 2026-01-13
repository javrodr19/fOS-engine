# DOM & HTML Roadmap: Chromium Parity & Beyond

> Goal: Fastest DOM implementation with minimal memory footprint

## Current State ✅
- Full DOM tree, Shadow DOM, Custom Elements
- Streaming/incremental HTML parser
- MutationObserver, Range/Selection, TreeWalker
- Persistent DOM, string interning

---

## Phase 1: Parser Performance (Q1)

### 1.1 HTML Parser Optimization
| Feature | Chromium | fOS Status | Action |
|---------|----------|------------|--------|
| Tokenizer | Fast | ✅ SIMD | Further vectorize |
| Tree builder | Spec-compliant | ✅ | Add fast paths |
| Preload scanner | Separate pass | ✅ | Integrate with main |
| Streaming | Yes | ✅ | Optimize buffering |

```rust
// SIMD tag detection
pub fn find_tag_start(bytes: &[u8]) -> Option<usize> {
    #[cfg(target_arch = "x86_64")]
    {
        use std::arch::x86_64::*;
        let needle = _mm256_set1_epi8(b'<' as i8);
        // Vectorized search...
    }
}
```

### 1.2 Spec Compliance Delta
| Feature | Status | Priority |
|---------|--------|----------|
| Adoption agency | ✅ | - |
| Foster parenting | ✅ | - |
| Template contents | ✅ | - |
| Declarative Shadow DOM | Partial | High |
| Sanitizer API | ❌ | Medium |

---

## Phase 2: DOM Operations (Q2)

### 2.1 Tree Mutation Performance
| Operation | Chromium | fOS Target |
|-----------|----------|------------|
| appendChild | O(1) | O(1) |
| insertBefore | O(n) childNodes | O(1) with skip list |
| removeChild | O(1) | O(1) |
| cloneNode | O(n) deep | O(1) CoW |
| querySelector | O(n) | O(log n) with index |

```rust
// Copy-on-Write node for efficient cloning
pub struct CowNode<T> {
    inner: Arc<T>,
    local_changes: Option<Box<LocalDelta<T>>>,
}

impl<T: Clone> CowNode<T> {
    pub fn mutate(&mut self) -> &mut T {
        if Arc::strong_count(&self.inner) > 1 {
            self.inner = Arc::new((*self.inner).clone());
        }
        Arc::make_mut(&mut self.inner)
    }
}
```

### 2.2 Query Optimization
```rust
pub struct QueryIndex {
    by_id: HashMap<InternedString, NodeId>,
    by_class: HashMap<InternedString, RoaringBitmap>,
    by_tag: HashMap<TagName, RoaringBitmap>,
    dirty: bool,
}

impl QueryIndex {
    pub fn query_selector(&self, sel: &Selector) -> Option<NodeId> {
        // Use indices instead of tree traversal
    }
}
```

---

## Phase 3: Memory Optimization (Q3)

### 3.1 Node Representation
| Component | Current Size | Target Size |
|-----------|--------------|-------------|
| Node struct | 128 bytes | 64 bytes |
| Attributes | HashMap | Inline small |
| Children | Vec | SlotMap |
| Text content | String | Rope |

```rust
// Compact node representation
#[repr(C)]
pub struct CompactNode {
    tag_and_flags: u32,     // Tag ID + node type + flags
    parent: NodeId,          // 4 bytes
    first_child: NodeId,     // 4 bytes
    next_sibling: NodeId,    // 4 bytes
    attrs: AttrStorage,      // Inline or pointer (8 bytes)
    data: NodeData,          // Union: text rope, element data (32 bytes)
}
// Total: 56 bytes + padding = 64 bytes
```

### 3.2 Attribute Storage
```rust
pub enum AttrStorage {
    // Inline for ≤3 common attributes
    Inline([InlineAttr; 3]),
    // Heap for more attributes
    Heap(Box<AttrMap>),
}

pub struct InlineAttr {
    name: InternedString,  // 4 bytes
    value: InternedString, // 4 bytes
}
```

### 3.3 Text Node Optimization
```rust
pub enum TextContent {
    // Short strings inline (≤23 bytes on 64-bit)
    Short(SmallString<23>),
    // Longer strings use rope for efficient editing
    Rope(Rope),
    // Very long, read-only content uses Arc
    Shared(Arc<str>),
}
```

---

## Phase 4: Spec Parity (Q3-Q4)

### 4.1 Missing DOM APIs
| API | Priority | Status |
|-----|----------|--------|
| `adoptNode` | High | Implement |
| `importNode` | High | Implement |
| `createTreeWalker` | ✅ | Done |
| `createNodeIterator` | ✅ | Done |
| `compareDocumentPosition` | Medium | Implement |
| `normalize` | Low | Implement |

### 4.2 Event System
| Feature | Chromium | fOS Status |
|---------|----------|------------|
| Capture/bubble | ✅ | ✅ |
| stopPropagation | ✅ | ✅ |
| Event delegation | ✅ | Optimize |
| Passive listeners | ✅ | ✅ |
| Trusted events | ✅ | Implement |

---

## Phase 5: Surpassing Chromium

### 5.1 Unique Optimizations
| Feature | Description | Chromium? |
|---------|-------------|-----------|
| **Persistent DOM** | CoW tree structure | No |
| **Generational IDs** | Fast node validity | No |
| **Semantic compression** | Compress DOM patterns | No |
| **Lazy attributes** | On-demand parsing | Partial |

### 5.2 Concurrent DOM Operations
```rust
pub struct ConcurrentDom {
    // Reader-writer lock at subtree level
    subtree_locks: DashMap<NodeId, RwLock<()>>,
    
    // Main thread owns mutations
    mutation_queue: Mutex<Vec<DomMutation>>,
    
    // Layout/paint can read concurrently
    snapshot: Arc<DomSnapshot>,
}
```

### 5.3 DOM Diffing for Frameworks
```rust
pub struct DomDiff {
    pub fn diff(old: &Node, new: &Node) -> Vec<DomPatch> {
        // Efficient tree diff for React-like frameworks
        // Return minimal mutation set
    }
}
```

---

## Benchmarks Target

| Metric | Chromium | fOS Target |
|--------|----------|------------|
| Parse 1MB HTML | 50ms | 30ms |
| querySelector (1000 nodes) | 0.5ms | 0.1ms |
| appendChild (deep tree) | 0.01ms | 0.005ms |
| cloneNode (deep) | 5ms | 0.1ms (CoW) |
| Memory per node | 200 bytes | 80 bytes |

---

## Dependencies Policy

### Keep
- None required

### Custom Implementation Priority
1. SIMD HTML tokenizer
2. Rope for text content
3. Compact node representation
4. Query indices (id, class, tag)
