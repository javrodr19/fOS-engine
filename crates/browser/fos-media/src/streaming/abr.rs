//! Adaptive Bitrate (ABR) Algorithms
//!
//! Quality selection based on bandwidth and buffer.

use super::QualityLevel;
use std::time::Duration;
use std::collections::VecDeque;

/// ABR algorithm type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbrAlgorithm { Throughput, BufferBased, Bola, Hybrid }

/// ABR controller
#[derive(Debug)]
pub struct AbrController {
    algorithm: AbrAlgorithm,
    quality_levels: Vec<QualityLevel>,
    current_quality: usize,
    bandwidth_samples: VecDeque<BandwidthSample>,
    buffer_level: Duration,
    // BOLA parameters
    bola_v: f64,
    bola_gamma: f64,
}

#[derive(Debug, Clone)]
struct BandwidthSample { bandwidth: u64, timestamp: std::time::Instant }

impl AbrController {
    pub fn new(algorithm: AbrAlgorithm, bandwidths: &[u64]) -> Self {
        let quality_levels: Vec<_> = bandwidths.iter().enumerate()
            .map(|(i, &b)| QualityLevel { index: i, bandwidth: b }).collect();
        Self {
            algorithm, quality_levels, current_quality: 0,
            bandwidth_samples: VecDeque::with_capacity(20),
            buffer_level: Duration::from_secs(0),
            bola_v: 0.93, bola_gamma: 5.0,
        }
    }
    
    /// Update with new bandwidth measurement
    pub fn add_bandwidth_sample(&mut self, bandwidth: u64) {
        self.bandwidth_samples.push_back(BandwidthSample { bandwidth, timestamp: std::time::Instant::now() });
        if self.bandwidth_samples.len() > 20 { self.bandwidth_samples.pop_front(); }
    }
    
    /// Update buffer level
    pub fn update_buffer(&mut self, buffer: Duration) { self.buffer_level = buffer; }
    
    /// Select quality level
    pub fn select_quality(&mut self) -> QualityLevel {
        let quality = match self.algorithm {
            AbrAlgorithm::Throughput => self.throughput_based(),
            AbrAlgorithm::BufferBased => self.buffer_based(),
            AbrAlgorithm::Bola => self.bola(),
            AbrAlgorithm::Hybrid => self.hybrid(),
        };
        self.current_quality = quality.index;
        quality
    }
    
    fn estimated_bandwidth(&self) -> u64 {
        if self.bandwidth_samples.is_empty() { return 1_000_000; }
        // Harmonic mean for more conservative estimate
        let sum: f64 = self.bandwidth_samples.iter().map(|s| 1.0 / s.bandwidth as f64).sum();
        (self.bandwidth_samples.len() as f64 / sum) as u64
    }
    
    fn throughput_based(&self) -> QualityLevel {
        let bandwidth = self.estimated_bandwidth();
        let safe_bandwidth = (bandwidth as f64 * 0.8) as u64;
        
        self.quality_levels.iter().rev()
            .find(|q| q.bandwidth <= safe_bandwidth)
            .cloned()
            .unwrap_or_else(|| self.quality_levels.first().cloned().unwrap_or(QualityLevel { index: 0, bandwidth: 0 }))
    }
    
    fn buffer_based(&self) -> QualityLevel {
        let buffer_secs = self.buffer_level.as_secs_f64();
        let ratio = (buffer_secs / 30.0).clamp(0.0, 1.0);
        let target_idx = (ratio * (self.quality_levels.len() - 1) as f64) as usize;
        self.quality_levels.get(target_idx).cloned().unwrap_or_else(|| self.quality_levels.first().cloned().unwrap_or(QualityLevel { index: 0, bandwidth: 0 }))
    }
    
    fn bola(&self) -> QualityLevel {
        // BOLA algorithm: maximize utility function V * ln(quality/gamma) - buffer penalty
        let buffer_secs = self.buffer_level.as_secs_f64();
        let mut best_idx = 0;
        let mut best_score = f64::NEG_INFINITY;
        
        for (i, q) in self.quality_levels.iter().enumerate() {
            let utility = (q.bandwidth as f64).ln();
            let score = (self.bola_v * utility + self.bola_v * self.bola_gamma) / (buffer_secs + self.bola_gamma);
            if score > best_score { best_score = score; best_idx = i; }
        }
        
        self.quality_levels.get(best_idx).cloned().unwrap_or(QualityLevel { index: 0, bandwidth: 0 })
    }
    
    fn hybrid(&self) -> QualityLevel {
        // Combine throughput and buffer-based
        let throughput_choice = self.throughput_based();
        let buffer_choice = self.buffer_based();
        
        let buffer_secs = self.buffer_level.as_secs_f64();
        if buffer_secs < 10.0 { throughput_choice }
        else if buffer_secs > 25.0 { buffer_choice }
        else {
            // Blend: use lower of the two for safety
            if throughput_choice.bandwidth < buffer_choice.bandwidth { throughput_choice } else { buffer_choice }
        }
    }
    
    pub fn current_quality(&self) -> QualityLevel { self.quality_levels.get(self.current_quality).cloned().unwrap_or(QualityLevel { index: 0, bandwidth: 0 }) }
    pub fn quality_count(&self) -> usize { self.quality_levels.len() }
}

impl Default for AbrController { fn default() -> Self { Self::new(AbrAlgorithm::Hybrid, &[500_000, 1_000_000, 2_000_000, 4_000_000, 8_000_000]) } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_abr() { 
        let mut abr = AbrController::default();
        abr.add_bandwidth_sample(5_000_000);
        abr.update_buffer(Duration::from_secs(15));
        let q = abr.select_quality();
        assert!(q.bandwidth > 0);
    }
}
