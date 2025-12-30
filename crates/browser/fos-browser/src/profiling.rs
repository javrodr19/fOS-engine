//! Performance Profiling Integration
//!
//! Integrates fos-devtools performance panel: frame timing, marks, measures, memory.

use fos_devtools::{PerformancePanel, MemoryInfo};
use fos_devtools::performance::EntryType;
use std::time::Instant;

/// Performance profiler for the browser
pub struct PerformanceProfiler {
    /// Performance panel
    panel: PerformancePanel,
    /// Frame counter
    frame_id: u64,
    /// Frame start time
    frame_start: Option<Instant>,
    /// Whether profiling is active
    active: bool,
}

impl PerformanceProfiler {
    /// Create new performance profiler
    pub fn new() -> Self {
        Self {
            panel: PerformancePanel::new(),
            frame_id: 0,
            frame_start: None,
            active: false,
        }
    }
    
    // === Recording Control ===
    
    /// Start profiling
    pub fn start(&mut self) {
        self.active = true;
        self.panel.start_recording();
        log::info!("Performance profiling started");
    }
    
    /// Stop profiling
    pub fn stop(&mut self) {
        self.active = false;
        self.panel.stop_recording();
        log::info!("Performance profiling stopped");
    }
    
    /// Check if profiling is active
    pub fn is_active(&self) -> bool {
        self.active
    }
    
    // === Frame Timing ===
    
    /// Begin a new frame
    pub fn begin_frame(&mut self) {
        if self.active {
            self.frame_id += 1;
            self.frame_start = Some(Instant::now());
            self.panel.begin_frame(self.frame_id);
        }
    }
    
    /// Record script execution time (ms)
    pub fn record_script(&mut self, time_ms: f64) {
        if self.active {
            self.panel.record_script_time(time_ms);
        }
    }
    
    /// Record style calculation time (ms)
    pub fn record_style(&mut self, time_ms: f64) {
        if self.active {
            self.panel.record_style_time(time_ms);
        }
    }
    
    /// Record layout time (ms)
    pub fn record_layout(&mut self, time_ms: f64) {
        if self.active {
            self.panel.record_layout_time(time_ms);
        }
    }
    
    /// Record paint time (ms)
    pub fn record_paint(&mut self, time_ms: f64) {
        if self.active {
            self.panel.record_paint_time(time_ms);
        }
    }
    
    /// End frame
    pub fn end_frame(&mut self) {
        if self.active {
            self.panel.end_frame();
        }
    }
    
    // === Marks and Measures ===
    
    /// Add a performance mark
    pub fn mark(&mut self, name: &str) {
        self.panel.mark(name);
    }
    
    /// Add a performance measure
    pub fn measure(&mut self, name: &str, start_mark: &str, end_mark: Option<&str>) {
        self.panel.measure(name, start_mark, end_mark);
    }
    
    /// Clear marks
    pub fn clear_marks(&mut self, name: Option<&str>) {
        self.panel.clear_marks(name);
    }
    
    /// Clear measures
    pub fn clear_measures(&mut self, name: Option<&str>) {
        self.panel.clear_measures(name);
    }
    
    // === Memory Profiling ===
    
    /// Record memory sample
    pub fn record_memory(&mut self, js_heap: usize, dom_nodes: usize) {
        if self.active {
            self.panel.record_memory(MemoryInfo {
                used_js_heap_size: js_heap,
                total_js_heap_size: js_heap,
                js_heap_size_limit: 256 * 1024 * 1024, // 256 MB
                dom_node_count: dom_nodes,
                dom_element_count: dom_nodes,
                layout_count: self.frame_id as usize,
            });
        }
    }
    
    // === Statistics ===
    
    /// Get average FPS
    pub fn average_fps(&self) -> f64 {
        self.panel.get_average_fps()
    }
    
    /// Get frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_id
    }
    
    /// Get frame timeline
    pub fn frames(&self) -> Vec<FrameSummary> {
        self.panel.get_frame_timeline().iter()
            .map(|f| FrameSummary {
                id: f.frame_id,
                total_ms: f.total_time,
                fps: f.fps(),
            })
            .collect()
    }
    
    /// Get profiling summary
    pub fn summary(&self) -> ProfileSummary {
        let frames = self.panel.get_frame_timeline();
        let memory = self.panel.get_memory_timeline();
        
        ProfileSummary {
            frame_count: frames.len(),
            avg_fps: self.panel.get_average_fps(),
            peak_memory: memory.iter().map(|m| m.used_js_heap_size).max().unwrap_or(0),
            mark_count: self.panel.get_entries_by_type(EntryType::Mark).len(),
            measure_count: self.panel.get_entries_by_type(EntryType::Measure).len(),
        }
    }
}

impl Default for PerformanceProfiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Frame summary
#[derive(Debug, Clone)]
pub struct FrameSummary {
    pub id: u64,
    pub total_ms: f64,
    pub fps: f64,
}

/// Profile summary
#[derive(Debug, Clone)]
pub struct ProfileSummary {
    pub frame_count: usize,
    pub avg_fps: f64,
    pub peak_memory: usize,
    pub mark_count: usize,
    pub measure_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_profiler_creation() {
        let profiler = PerformanceProfiler::new();
        assert!(!profiler.is_active());
    }
    
    #[test]
    fn test_profiler_start_stop() {
        let mut profiler = PerformanceProfiler::new();
        
        profiler.start();
        assert!(profiler.is_active());
        
        profiler.stop();
        assert!(!profiler.is_active());
    }
    
    #[test]
    fn test_frame_recording() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();
        
        profiler.begin_frame();
        profiler.record_script(5.0);
        profiler.record_layout(3.0);
        profiler.end_frame();
        
        assert_eq!(profiler.frame_count(), 1);
    }
    
    #[test]
    fn test_marks_and_measures() {
        let mut profiler = PerformanceProfiler::new();
        profiler.start();
        
        profiler.mark("start");
        profiler.mark("end");
        profiler.measure("total", "start", Some("end"));
        
        let summary = profiler.summary();
        assert_eq!(summary.mark_count, 2);
        assert_eq!(summary.measure_count, 1);
    }
}
