# Concurrency Optimization Roadmap: Surpassing Chromium

> Goal: Maximum parallelism with lock-free data structures and work-stealing

## Current State
- Single-threaded rendering
- Some parallel parsing
- Basic task scheduling

---

## Phase 1: Lock-Free Foundations (Q1)

### 1.1 Lock-Free Data Structures
```rust
// Lock-free concurrent hash map
pub struct ConcurrentMap<K, V> {
    buckets: Box<[AtomicPtr<Node<K, V>>]>,
}

impl<K: Hash + Eq, V> ConcurrentMap<K, V> {
    pub fn get(&self, key: &K) -> Option<&V> {
        let bucket = self.bucket_for(key);
        let mut node = bucket.load(Ordering::Acquire);
        
        while !node.is_null() {
            let n = unsafe { &*node };
            if n.key == *key {
                return Some(&n.value);
            }
            node = n.next.load(Ordering::Acquire);
        }
        None
    }
    
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        // CAS-based insertion
    }
}
```

### 1.2 Wait-Free Queues
```rust
// Multi-producer, single-consumer queue (for main thread)
pub struct MpscQueue<T> {
    head: AtomicPtr<Node<T>>,
    tail: AtomicPtr<Node<T>>,
}

// Single-producer, single-consumer queue (for thread pairs)
pub struct SpscQueue<T> {
    buffer: Box<[UnsafeCell<MaybeUninit<T>>]>,
    head: AtomicUsize,
    tail: AtomicUsize,
}
```

---

## Phase 2: Parallel Parsing (Q2)

### 2.1 Speculative HTML Parsing
```rust
pub struct ParallelHtmlParser {
    // Main parser (authoritative)
    main_parser: HtmlParser,
    
    // Speculative parsers for likely branches
    speculative: Vec<SpeculativeParser>,
}

impl ParallelHtmlParser {
    pub fn parse(&mut self, html: &[u8]) -> Dom {
        // 1. Main thread: parse sequentially
        // 2. Worker threads: speculatively parse ahead
        // 3. Merge results when speculation correct
        
        rayon::scope(|s| {
            // Preload scanner on separate thread
            s.spawn(|_| self.preload_scan(html));
            
            // Speculative parsing for large documents
            if html.len() > 100_000 {
                s.spawn(|_| self.parse_speculative(html));
            }
        });
        
        self.main_parser.parse(html)
    }
}
```

### 2.2 Parallel CSS Parsing
```rust
pub fn parse_stylesheets_parallel(sheets: Vec<&str>) -> Vec<Stylesheet> {
    sheets.par_iter()
        .map(|css| parse_stylesheet(css))
        .collect()
}
```

---

## Phase 3: Parallel Styling (Q3)

### 3.1 Concurrent Style Resolution
```rust
pub fn resolve_styles_parallel(tree: &DomTree, styles: &Stylesheets) -> StyleTree {
    // Phase 1: Match selectors in parallel (read-only)
    let matches: Vec<_> = tree.elements()
        .par_iter()
        .map(|elem| match_selectors(elem, styles))
        .collect();
    
    // Phase 2: Cascade (parallel per subtree for inherited props)
    let computed: StyleTree = tree.par_traverse(|node, parent_style| {
        compute_style(node, &matches[node.id], parent_style)
    });
    
    computed
}
```

### 3.2 Independent Subtree Styling
```rust
// Style independent subtrees in parallel
pub fn identify_independent_subtrees(tree: &DomTree) -> Vec<SubtreeId> {
    // Subtrees with no inherited custom properties
    // Subtrees with no :has() affecting them
    // Shadow roots (style isolated)
}
```

---

## Phase 4: Parallel Layout (Q4)

### 4.1 Layout Phases
```rust
pub fn layout_parallel(tree: &StyleTree) -> LayoutTree {
    // Phase 1: Intrinsic sizes (parallel, bottom-up)
    let intrinsic = tree.par_traverse_postorder(|node, children| {
        compute_intrinsic_sizes(node, children)
    });
    
    // Phase 2: Constraint propagation (mostly sequential)
    let constraints = propagate_constraints(tree, &intrinsic);
    
    // Phase 3: Final sizing (parallel per independent subtree)
    let sizes = tree.independent_subtrees()
        .par_iter()
        .map(|subtree| layout_subtree(subtree, &constraints))
        .collect();
    
    // Phase 4: Positioning (parallel)
    let positions = compute_positions_parallel(&sizes);
    
    LayoutTree::from_parts(sizes, positions)
}
```

### 4.2 Flex/Grid Parallelism
```rust
// Layout independent flex/grid children in parallel
pub fn layout_flex_parallel(container: &FlexContainer) -> FlexLayout {
    // Parallel: compute child intrinsic sizes
    let child_sizes: Vec<_> = container.children
        .par_iter()
        .map(|child| compute_intrinsic_sizes(child))
        .collect();
    
    // Sequential: flex sizing algorithm
    let flex_sizes = resolve_flex_sizes(container, &child_sizes);
    
    // Parallel: layout cross-axis
    container.children.par_iter()
        .zip(flex_sizes.par_iter())
        .map(|(child, size)| layout_child(child, size))
        .collect()
}
```

---

## Phase 5: Parallel Rendering (Q4+)

### 5.1 Tile-Based Parallelism
```rust
pub struct TileRenderer {
    thread_pool: rayon::ThreadPool,
    tiles: Vec<Tile>,
}

impl TileRenderer {
    pub fn raster_parallel(&self, display_list: &DisplayList) {
        self.tiles.par_iter_mut()
            .filter(|tile| tile.is_dirty())
            .for_each(|tile| {
                let clipped_list = display_list.clip_to(tile.bounds);
                tile.raster(&clipped_list);
            });
    }
}
```

### 5.2 Layer Compositing
```rust
pub fn composite_layers_parallel(layers: &[Layer]) -> CompositeResult {
    // Sort layers by z-order (sequential)
    let sorted = sort_layers(layers);
    
    // Blend independent layer groups in parallel
    sorted.groups()
        .par_iter()
        .map(|group| blend_group(group))
        .reduce(|| CompositeResult::empty(), |a, b| a.merge(b))
}
```

---

## Phase 6: Work Stealing Scheduler

### 6.1 Task Scheduler
```rust
pub struct WorkStealingScheduler {
    workers: Vec<Worker>,
    global_queue: ConcurrentQueue<Task>,
}

pub struct Worker {
    local_queue: SpscQueue<Task>,
    rng: XorShift,
}

impl Worker {
    pub fn run(&self, scheduler: &WorkStealingScheduler) {
        loop {
            // 1. Try local queue first
            if let Some(task) = self.local_queue.pop() {
                task.run();
                continue;
            }
            
            // 2. Try global queue
            if let Some(task) = scheduler.global_queue.pop() {
                task.run();
                continue;
            }
            
            // 3. Steal from random other worker
            let victim = self.rng.next() % scheduler.workers.len();
            if let Some(task) = scheduler.workers[victim].local_queue.steal() {
                task.run();
                continue;
            }
            
            // 4. Sleep if nothing to do
            std::thread::park();
        }
    }
}
```

### 6.2 Priority-Based Scheduling
```rust
pub enum TaskPriority {
    UserBlocking,      // Input, animation frames
    UserVisible,       // Visible content rendering
    Background,        // Prefetch, speculative work
    Idle,              // GC, cache cleanup
}

impl WorkStealingScheduler {
    pub fn schedule(&self, task: Task, priority: TaskPriority) {
        match priority {
            TaskPriority::UserBlocking => {
                // Run on current thread immediately
                task.run();
            }
            TaskPriority::UserVisible => {
                self.global_queue.push_front(task);
                self.wake_workers();
            }
            TaskPriority::Background => {
                self.global_queue.push_back(task);
            }
            TaskPriority::Idle => {
                self.idle_queue.push(task);
            }
        }
    }
}
```

---

## Chromium Comparison

| Area | Chromium | fOS Target |
|------|----------|------------|
| HTML parsing | Sequential + preload | Speculative parallel |
| CSS parsing | Parallel | Parallel |
| Style resolution | Parallel | Parallel + incremental |
| Layout | Mostly sequential | Phase-parallel |
| Paint | Tile-parallel | Tile-parallel |
| JS execution | Single thread | Single thread* |
| GC | Concurrent | Concurrent |

*JS is inherently single-threaded per realm

---

## Thread Utilization Target

| Cores | Chromium Utilization | fOS Target |
|-------|---------------------|------------|
| 1 | 100% | 100% |
| 4 | ~200% | 350% |
| 8 | ~300% | 650% |
| 16 | ~400% | 1200% |

---

## Implementation Priority

1. **Parallel style resolution** - Easy win, high impact
2. **Lock-free caches** - Reduce contention
3. **Tile-parallel paint** - Scales with cores
4. **Parallel layout** - Complex but high value
5. **Work-stealing scheduler** - Foundation for all
