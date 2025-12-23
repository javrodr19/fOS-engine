//! Benchmarking Integration
//!
//! Browser benchmarking for performance testing.

use std::time::{Duration, Instant};
use std::hint::black_box;

/// Benchmark result
#[derive(Debug, Clone)]
pub struct BenchResult {
    pub name: String,
    pub iterations: u64,
    pub total_time: Duration,
    pub mean_time: Duration,
    pub min_time: Duration,
    pub max_time: Duration,
    pub ops_per_sec: f64,
}

/// Browser benchmarker
pub struct BrowserBenchmark {
    /// Warmup iterations
    warmup: u64,
    /// Benchmark iterations
    iterations: u64,
    /// Results
    results: Vec<BenchResult>,
}

impl BrowserBenchmark {
    /// Create new benchmarker
    pub fn new() -> Self {
        Self {
            warmup: 5,
            iterations: 50,
            results: Vec::new(),
        }
    }
    
    /// Set warmup iterations
    pub fn warmup(mut self, n: u64) -> Self {
        self.warmup = n;
        self
    }
    
    /// Set benchmark iterations
    pub fn iterations(mut self, n: u64) -> Self {
        self.iterations = n;
        self
    }
    
    /// Run a benchmark
    pub fn bench<F, R>(&mut self, name: &str, mut f: F) -> &BenchResult
    where
        F: FnMut() -> R,
    {
        // Warmup
        for _ in 0..self.warmup {
            black_box(f());
        }
        
        // Collect times
        let mut times = Vec::with_capacity(self.iterations as usize);
        
        for _ in 0..self.iterations {
            let start = Instant::now();
            black_box(f());
            times.push(start.elapsed());
        }
        
        let total: Duration = times.iter().sum();
        let mean = total / self.iterations as u32;
        let min = *times.iter().min().unwrap_or(&Duration::ZERO);
        let max = *times.iter().max().unwrap_or(&Duration::ZERO);
        
        let result = BenchResult {
            name: name.to_string(),
            iterations: self.iterations,
            total_time: total,
            mean_time: mean,
            min_time: min,
            max_time: max,
            ops_per_sec: self.iterations as f64 / total.as_secs_f64(),
        };
        
        self.results.push(result);
        self.results.last().unwrap()
    }
    
    /// Get all results
    pub fn results(&self) -> &[BenchResult] {
        &self.results
    }
    
    /// Get summary
    pub fn summary(&self) -> BenchSummary {
        let total_time: Duration = self.results.iter().map(|r| r.total_time).sum();
        
        BenchSummary {
            benchmarks_run: self.results.len(),
            total_iterations: self.results.iter().map(|r| r.iterations).sum(),
            total_time,
        }
    }
    
    /// Clear results
    pub fn clear(&mut self) {
        self.results.clear();
    }
}

impl Default for BrowserBenchmark {
    fn default() -> Self {
        Self::new()
    }
}

/// Benchmark summary
#[derive(Debug, Clone)]
pub struct BenchSummary {
    pub benchmarks_run: usize,
    pub total_iterations: u64,
    pub total_time: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_benchmark_creation() {
        let bench = BrowserBenchmark::new();
        assert!(bench.results().is_empty());
    }
    
    #[test]
    fn test_simple_benchmark() {
        let mut bench = BrowserBenchmark::new().iterations(10).warmup(2);
        
        let result = bench.bench("add", || 1 + 1);
        
        assert_eq!(result.iterations, 10);
        assert!(result.mean_time > Duration::ZERO);
    }
}
