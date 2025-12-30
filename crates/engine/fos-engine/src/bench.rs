//! Phase 24 Benchmark Suite
//!
//! Benchmarks for experimental optimizations in Phase 24.
//! Run with: cargo bench --package fos-benchmarks

use std::hint::black_box;
use std::time::{Duration, Instant};

/// Simple benchmark result
#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub name: String,
    pub iterations: u64,
    pub total_time: Duration,
    pub mean_time: Duration,
    pub min_time: Duration,
    pub max_time: Duration,
    pub throughput: Option<f64>, // ops/sec or bytes/sec
}

impl std::fmt::Display for BenchmarkResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: mean={:?}, min={:?}, max={:?}, iters={}",
            self.name, self.mean_time, self.min_time, self.max_time, self.iterations
        )?;
        if let Some(tp) = self.throughput {
            write!(f, ", throughput={:.2}/sec", tp)?;
        }
        Ok(())
    }
}

/// Benchmark runner
pub struct Bencher {
    warmup_iters: u64,
    bench_iters: u64,
    min_time: Duration,
}

impl Default for Bencher {
    fn default() -> Self {
        Self {
            warmup_iters: 10,
            bench_iters: 100,
            min_time: Duration::from_millis(100),
        }
    }
}

impl Bencher {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn warmup_iters(mut self, n: u64) -> Self {
        self.warmup_iters = n;
        self
    }
    
    pub fn bench_iters(mut self, n: u64) -> Self {
        self.bench_iters = n;
        self
    }
    
    pub fn min_time(mut self, d: Duration) -> Self {
        self.min_time = d;
        self
    }
    
    /// Run a benchmark
    pub fn bench<F, R>(&self, name: &str, mut f: F) -> BenchmarkResult
    where
        F: FnMut() -> R,
    {
        // Warmup
        for _ in 0..self.warmup_iters {
            black_box(f());
        }
        
        // Collect samples
        let mut times = Vec::with_capacity(self.bench_iters as usize);
        let start = Instant::now();
        let mut iters = 0u64;
        
        while iters < self.bench_iters || start.elapsed() < self.min_time {
            let iter_start = Instant::now();
            black_box(f());
            times.push(iter_start.elapsed());
            iters += 1;
        }
        
        let total_time: Duration = times.iter().sum();
        let mean_time = total_time / iters as u32;
        let min_time = *times.iter().min().unwrap_or(&Duration::ZERO);
        let max_time = *times.iter().max().unwrap_or(&Duration::ZERO);
        
        BenchmarkResult {
            name: name.to_string(),
            iterations: iters,
            total_time,
            mean_time,
            min_time,
            max_time,
            throughput: Some(iters as f64 / total_time.as_secs_f64()),
        }
    }
    
    /// Run a benchmark with setup
    pub fn bench_with_setup<S, F, R, T>(&self, name: &str, mut setup: S, mut f: F) -> BenchmarkResult
    where
        S: FnMut() -> T,
        F: FnMut(T) -> R,
    {
        // Warmup
        for _ in 0..self.warmup_iters {
            let input = setup();
            black_box(f(input));
        }
        
        // Collect samples
        let mut times = Vec::with_capacity(self.bench_iters as usize);
        let start = Instant::now();
        let mut iters = 0u64;
        
        while iters < self.bench_iters || start.elapsed() < self.min_time {
            let input = setup();
            let iter_start = Instant::now();
            black_box(f(input));
            times.push(iter_start.elapsed());
            iters += 1;
        }
        
        let total_time: Duration = times.iter().sum();
        let mean_time = total_time / iters as u32;
        let min_time = *times.iter().min().unwrap_or(&Duration::ZERO);
        let max_time = *times.iter().max().unwrap_or(&Duration::ZERO);
        
        BenchmarkResult {
            name: name.to_string(),
            iterations: iters,
            total_time,
            mean_time,
            min_time,
            max_time,
            throughput: Some(iters as f64 / total_time.as_secs_f64()),
        }
    }
}

/// Benchmark group for organizing related benchmarks
pub struct BenchmarkGroup {
    pub name: String,
    pub results: Vec<BenchmarkResult>,
}

impl BenchmarkGroup {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            results: Vec::new(),
        }
    }
    
    pub fn add(&mut self, result: BenchmarkResult) {
        self.results.push(result);
    }
    
    pub fn print_report(&self) {
        println!("\n=== {} ===", self.name);
        for result in &self.results {
            println!("  {}", result);
        }
    }
}

/// Memory benchmark utilities
pub mod memory {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    static ALLOCATED: AtomicUsize = AtomicUsize::new(0);
    static DEALLOCATED: AtomicUsize = AtomicUsize::new(0);
    
    /// Tracking allocator for memory benchmarks
    pub struct TrackingAllocator;
    
    unsafe impl GlobalAlloc for TrackingAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
            unsafe { System.alloc(layout) }
        }
        
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            DEALLOCATED.fetch_add(layout.size(), Ordering::SeqCst);
            unsafe { System.dealloc(ptr, layout) }
        }
    }
    
    /// Get current allocation stats
    pub fn allocation_stats() -> (usize, usize) {
        (
            ALLOCATED.load(Ordering::SeqCst),
            DEALLOCATED.load(Ordering::SeqCst),
        )
    }
    
    /// Reset allocation counters
    pub fn reset_stats() {
        ALLOCATED.store(0, Ordering::SeqCst);
        DEALLOCATED.store(0, Ordering::SeqCst);
    }
    
    /// Get net allocated bytes
    pub fn net_allocated() -> usize {
        let (alloc, dealloc) = allocation_stats();
        alloc.saturating_sub(dealloc)
    }
    
    /// Measure memory usage of a closure
    pub fn measure_memory<F, R>(f: F) -> (R, usize)
    where
        F: FnOnce() -> R,
    {
        reset_stats();
        let result = f();
        let mem = net_allocated();
        (result, mem)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bencher() {
        let bencher = Bencher::new().bench_iters(10).warmup_iters(2);
        let result = bencher.bench("test_add", || {
            black_box(1 + 1)
        });
        assert!(result.iterations >= 10);
        assert!(result.mean_time > Duration::ZERO);
    }
    
    #[test]
    fn test_bench_with_setup() {
        let bencher = Bencher::new().bench_iters(10);
        let result = bencher.bench_with_setup(
            "test_vec_push",
            || Vec::with_capacity(100),
            |mut v| {
                v.push(42);
                v
            },
        );
        assert!(result.iterations >= 10);
    }
}
