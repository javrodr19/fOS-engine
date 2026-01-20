//! Workload Classification
//!
//! Classifies browser workload for CPU frequency hints.

use std::time::{Duration, Instant};

/// Workload type for CPU frequency hints
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WorkloadType {
    /// No work, lowest frequency
    #[default]
    Idle,
    /// Reading/scrolling, low frequency
    LightBrowsing,
    /// Active interaction, medium frequency
    Interactive,
    /// Layout/paint, high frequency
    HeavyProcessing,
    /// Video/audio playback, fixed frequency
    MediaPlayback,
}

impl WorkloadType {
    /// Get suggested CPU frequency multiplier (0.0 - 1.0)
    pub fn frequency_hint(&self) -> f32 {
        match self {
            Self::Idle => 0.2,
            Self::LightBrowsing => 0.4,
            Self::Interactive => 0.6,
            Self::HeavyProcessing => 1.0,
            Self::MediaPlayback => 0.5,
        }
    }
    
    /// Should use efficiency cores (big.LITTLE)
    pub fn prefer_efficiency_cores(&self) -> bool {
        matches!(self, Self::Idle | Self::LightBrowsing)
    }
    
    /// Get expected power usage (relative)
    pub fn power_factor(&self) -> f32 {
        match self {
            Self::Idle => 0.1,
            Self::LightBrowsing => 0.3,
            Self::Interactive => 0.5,
            Self::HeavyProcessing => 1.0,
            Self::MediaPlayback => 0.4,
        }
    }
}

/// Workload metrics for classification
#[derive(Debug, Clone, Default)]
pub struct WorkloadMetrics {
    /// Input events in last second
    pub input_events: u32,
    /// Paint operations in last second
    pub paints: u32,
    /// JavaScript execution time (0.0 - 1.0 of total time)
    pub js_time_ratio: f32,
    /// Layout operations in last second
    pub layouts: u32,
    /// Active media streams
    pub media_streams: u32,
    /// Scroll events in last second
    pub scroll_events: u32,
}

/// Workload classifier
#[derive(Debug)]
pub struct WorkloadClassifier {
    /// Current metrics
    metrics: WorkloadMetrics,
    /// Current workload classification
    current: WorkloadType,
    /// Last classification time
    last_classification: Instant,
    /// Smoothed input rate
    input_rate: f32,
    /// Smoothed paint rate
    paint_rate: f32,
    /// Classification history for hysteresis
    history: [WorkloadType; 5],
    history_idx: usize,
}

impl Default for WorkloadClassifier {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkloadClassifier {
    /// Create a new workload classifier
    pub fn new() -> Self {
        Self {
            metrics: WorkloadMetrics::default(),
            current: WorkloadType::Idle,
            last_classification: Instant::now(),
            input_rate: 0.0,
            paint_rate: 0.0,
            history: [WorkloadType::Idle; 5],
            history_idx: 0,
        }
    }
    
    /// Update metrics
    pub fn update_metrics(&mut self, metrics: WorkloadMetrics) {
        // Exponential moving average for smoothing
        let alpha = 0.3;
        self.input_rate = self.input_rate * (1.0 - alpha) + metrics.input_events as f32 * alpha;
        self.paint_rate = self.paint_rate * (1.0 - alpha) + metrics.paints as f32 * alpha;
        self.metrics = metrics;
    }
    
    /// Record input event
    pub fn record_input(&mut self) {
        self.metrics.input_events += 1;
    }
    
    /// Record paint
    pub fn record_paint(&mut self) {
        self.metrics.paints += 1;
    }
    
    /// Record layout
    pub fn record_layout(&mut self) {
        self.metrics.layouts += 1;
    }
    
    /// Set JS time ratio
    pub fn set_js_time(&mut self, ratio: f32) {
        self.metrics.js_time_ratio = ratio;
    }
    
    /// Set media streams count
    pub fn set_media_streams(&mut self, count: u32) {
        self.metrics.media_streams = count;
    }
    
    /// Classify current workload
    pub fn classify(&mut self) -> WorkloadType {
        let input_rate = self.input_rate;
        let paint_rate = self.paint_rate;
        let js_time = self.metrics.js_time_ratio;
        
        let classification = if self.metrics.media_streams > 0 {
            WorkloadType::MediaPlayback
        } else if input_rate > 10.0 || self.metrics.scroll_events > 5 {
            WorkloadType::Interactive
        } else if paint_rate > 30.0 || js_time > 0.5 || self.metrics.layouts > 10 {
            WorkloadType::HeavyProcessing
        } else if paint_rate > 0.0 || input_rate > 0.0 {
            WorkloadType::LightBrowsing
        } else {
            WorkloadType::Idle
        };
        
        // Add to history for hysteresis
        self.history[self.history_idx] = classification;
        self.history_idx = (self.history_idx + 1) % 5;
        
        // Use most common classification from history for stability
        self.current = self.most_common_workload();
        self.last_classification = Instant::now();
        
        self.current
    }
    
    /// Get most common workload from history
    fn most_common_workload(&self) -> WorkloadType {
        let mut counts = [0u8; 5];
        for wl in &self.history {
            counts[*wl as usize] += 1;
        }
        
        let max_idx = counts.iter()
            .enumerate()
            .max_by_key(|(_, &c)| c)
            .map(|(i, _)| i)
            .unwrap_or(0);
        
        match max_idx {
            0 => WorkloadType::Idle,
            1 => WorkloadType::LightBrowsing,
            2 => WorkloadType::Interactive,
            3 => WorkloadType::HeavyProcessing,
            4 => WorkloadType::MediaPlayback,
            _ => WorkloadType::Idle,
        }
    }
    
    /// Get current workload without reclassifying
    pub fn current_workload(&self) -> WorkloadType {
        self.current
    }
    
    /// Get input events per second
    pub fn input_events_per_second(&self) -> f32 {
        self.input_rate
    }
    
    /// Get paints per second
    pub fn paints_per_second(&self) -> f32 {
        self.paint_rate
    }
    
    /// Get JS time percentage
    pub fn js_time_percentage(&self) -> f32 {
        self.metrics.js_time_ratio
    }
    
    /// Reset metrics for new measurement period
    pub fn reset_period(&mut self) {
        self.metrics = WorkloadMetrics::default();
    }
    
    /// Get frequency hint for current workload
    pub fn frequency_hint(&self) -> f32 {
        self.current.frequency_hint()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_workload_frequency_hint() {
        assert!(WorkloadType::Idle.frequency_hint() < WorkloadType::HeavyProcessing.frequency_hint());
    }
    
    #[test]
    fn test_classifier_idle() {
        let mut classifier = WorkloadClassifier::new();
        assert_eq!(classifier.classify(), WorkloadType::Idle);
    }
    
    #[test]
    fn test_classifier_interactive() {
        let mut classifier = WorkloadClassifier::new();
        // Simulate lots of input
        classifier.input_rate = 15.0;
        assert_eq!(classifier.classify(), WorkloadType::Interactive);
    }
    
    #[test]
    fn test_classifier_media() {
        let mut classifier = WorkloadClassifier::new();
        classifier.set_media_streams(1);
        assert_eq!(classifier.classify(), WorkloadType::MediaPlayback);
    }
    
    #[test]
    fn test_efficiency_cores() {
        assert!(WorkloadType::Idle.prefer_efficiency_cores());
        assert!(!WorkloadType::HeavyProcessing.prefer_efficiency_cores());
    }
}
