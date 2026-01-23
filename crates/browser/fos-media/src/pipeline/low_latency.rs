//! Low-Latency Pipeline
//!
//! Optimized pipeline for live streaming with minimal buffering.

use std::time::Duration;

/// Low-latency pipeline configuration
#[derive(Debug, Clone)]
pub struct LowLatencyConfig {
    pub buffer_target: Duration,
    pub max_latency: Duration,
    pub frame_drop_strategy: FrameDropStrategy,
    pub skip_b_frames: bool,
}

impl Default for LowLatencyConfig {
    fn default() -> Self {
        Self {
            buffer_target: Duration::from_millis(100),
            max_latency: Duration::from_millis(500),
            frame_drop_strategy: FrameDropStrategy::NonReference,
            skip_b_frames: true,
        }
    }
}

/// Frame drop strategy when behind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameDropStrategy {
    None,
    NonReference,
    ToNextKeyframe,
    Aggressive,
}

/// Low-latency pipeline state
#[derive(Debug)]
pub struct LowLatencyPipeline {
    config: LowLatencyConfig,
    current_latency: Duration,
    frames_dropped: u64,
    catchup_mode: bool,
}

impl LowLatencyPipeline {
    pub fn new(config: LowLatencyConfig) -> Self {
        Self { config, current_latency: Duration::ZERO, frames_dropped: 0, catchup_mode: false }
    }
    
    /// Check if we should drop this frame
    pub fn should_drop_frame(&mut self, is_key: bool, is_reference: bool) -> bool {
        if self.current_latency <= self.config.buffer_target { 
            self.catchup_mode = false;
            return false; 
        }
        
        if self.current_latency > self.config.max_latency { self.catchup_mode = true; }
        
        if !self.catchup_mode { return false; }
        
        let drop = match self.config.frame_drop_strategy {
            FrameDropStrategy::None => false,
            FrameDropStrategy::NonReference => !is_reference && !is_key,
            FrameDropStrategy::ToNextKeyframe => !is_key,
            FrameDropStrategy::Aggressive => !is_key,
        };
        
        if drop { self.frames_dropped += 1; }
        drop
    }
    
    /// Update current latency measurement
    pub fn update_latency(&mut self, latency: Duration) { self.current_latency = latency; }
    
    pub fn current_latency(&self) -> Duration { self.current_latency }
    pub fn frames_dropped(&self) -> u64 { self.frames_dropped }
    pub fn is_catching_up(&self) -> bool { self.catchup_mode }
}

impl Default for LowLatencyPipeline { fn default() -> Self { Self::new(LowLatencyConfig::default()) } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_low_latency() { let ll = LowLatencyPipeline::default(); assert_eq!(ll.frames_dropped(), 0); }
}
