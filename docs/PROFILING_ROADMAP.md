# Profiling & Benchmarking Roadmap

> Goal: Comprehensive performance tracking to guide optimizations

## Phase 1: Built-in Profiler (Q1)

### 1.1 Tracing Infrastructure
```rust
// Zero-cost when disabled
#[cfg(feature = "profiling")]
macro_rules! trace_scope {
    ($name:expr) => {
        let _guard = profiler::ScopeGuard::new($name);
    };
}

#[cfg(not(feature = "profiling"))]
macro_rules! trace_scope {
    ($name:expr) => {};
}

// Usage
pub fn layout(&mut self) {
    trace_scope!("layout");
    // ... layout code
}
```

### 1.2 Metrics Collection
```rust
pub struct Metrics {
    // Timing metrics
    pub frame_times: Histogram,
    pub parse_times: Histogram,
    pub layout_times: Histogram,
    pub paint_times: Histogram,
    
    // Memory metrics
    pub heap_size: Gauge,
    pub dom_node_count: Counter,
    pub style_count: Counter,
    
    // Cache metrics
    pub cache_hits: Counter,
    pub cache_misses: Counter,
}
```

---

## Phase 2: Benchmark Suite (Q2)

### 2.1 Micro-Benchmarks
```rust
// Per-component benchmarks
#[bench]
fn bench_html_parse_10kb(b: &mut Bencher) {
    let html = include_str!("fixtures/10kb.html");
    b.iter(|| parse_html(html));
}

#[bench]
fn bench_selector_match(b: &mut Bencher) {
    let selector = parse_selector(".foo .bar > .baz:hover");
    let element = create_test_element();
    b.iter(|| selector.matches(&element));
}
```

### 2.2 Macro-Benchmarks
| Benchmark | Measures | Target |
|-----------|----------|--------|
| cold_start | Startup to first paint | <100ms |
| page_load | Navigation to complete | <500ms |
| scroll_fps | Scrolling smoothness | >58fps |
| input_latency | Keystroke to display | <16ms |
| memory_10_tabs | Memory with 10 tabs | <300MB |

---

## Phase 3: Comparison Framework (Q3)

### 3.1 Chromium Comparison Tests
```rust
pub struct ComparisonBenchmark {
    pub fn run(&self) -> ComparisonResult {
        let fos_time = self.run_fos();
        let chromium_time = self.run_chromium_headless();
        
        ComparisonResult {
            fos_time,
            chromium_time,
            speedup: chromium_time / fos_time,
        }
    }
}
```

### 3.2 Performance Regression Tests
```rust
// CI: fail if regression > 5%
pub fn check_regression(current: Duration, baseline: Duration) -> Result<()> {
    let regression = current.as_secs_f64() / baseline.as_secs_f64();
    if regression > 1.05 {
        Err(format!("Regression: {:.1}% slower", (regression - 1.0) * 100.0))
    } else {
        Ok(())
    }
}
```

---

## Phase 4: Real-World Metrics (Q4)

### 4.1 Web Platform Tests
```
wpt/                           # Web Platform Tests
├── css/                       # CSS conformance
├── html/                      # HTML parsing
├── dom/                       # DOM API correctness
└── performance/               # Timing APIs
```

### 4.2 Popular Site Tests
| Site | Metrics |
|------|---------|
| Google | FCP, LCP, TBT |
| YouTube | Video start, scroll |
| Twitter | Infinite scroll |
| Reddit | Comment threads |
| Amazon | Product pages |

---

## Metrics Dashboard

```
fOS Engine Performance Dashboard
================================

Startup
-------
Cold start:     85ms  [████████░░] 85% of target
First paint:    42ms  [████████░░] 84% of target

Rendering
---------
60fps rate:     98.5% [██████████] Excellent
Paint time:     3.2ms [████░░░░░░] Good
Layout time:    1.8ms [███░░░░░░░] Good

Memory
------
Heap size:      45MB  [████░░░░░░] 45% of budget
DOM nodes:      1,234 
Cache hit rate: 94%   [█████████░] Excellent

vs Chromium
-----------
Parse:    2.1x faster  [>>>>>>>>>>]
Layout:   1.5x faster  [>>>>>>>   ]
Paint:    1.8x faster  [>>>>>>>>>]
Memory:   0.6x usage   [<<<       ]
```
