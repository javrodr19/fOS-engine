//! Performance Panel
//!
//! Frame timing, CPU, and memory profiling.

use std::collections::VecDeque;

/// Performance entry
#[derive(Debug, Clone)]
pub struct PerformanceEntry {
    pub name: String,
    pub entry_type: EntryType,
    pub start_time: f64,
    pub duration: f64,
}

/// Entry type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryType {
    Mark,
    Measure,
    Navigation,
    Resource,
    Paint,
    LongestContentfulPaint,
    FirstInput,
    LayoutShift,
}

/// Frame timing
#[derive(Debug, Clone)]
pub struct FrameTimingInfo {
    pub frame_id: u64,
    pub start_time: f64,
    pub script_time: f64,
    pub style_time: f64,
    pub layout_time: f64,
    pub paint_time: f64,
    pub composite_time: f64,
    pub total_time: f64,
}

impl FrameTimingInfo {
    pub fn new(frame_id: u64, start_time: f64) -> Self {
        Self {
            frame_id,
            start_time,
            script_time: 0.0,
            style_time: 0.0,
            layout_time: 0.0,
            paint_time: 0.0,
            composite_time: 0.0,
            total_time: 0.0,
        }
    }
    
    /// Calculate FPS
    pub fn fps(&self) -> f64 {
        if self.total_time > 0.0 {
            1000.0 / self.total_time
        } else {
            0.0
        }
    }
}

/// Memory info
#[derive(Debug, Clone, Default)]
pub struct MemoryInfo {
    pub used_js_heap_size: usize,
    pub total_js_heap_size: usize,
    pub js_heap_size_limit: usize,
    pub dom_node_count: usize,
    pub dom_element_count: usize,
    pub layout_count: usize,
}

/// CPU profile
#[derive(Debug, Clone)]
pub struct CpuProfile {
    pub start_time: f64,
    pub end_time: f64,
    pub samples: Vec<CpuSample>,
    pub nodes: Vec<ProfileNode>,
}

/// CPU sample
#[derive(Debug, Clone)]
pub struct CpuSample {
    pub timestamp: f64,
    pub node_id: u32,
}

/// Profile node
#[derive(Debug, Clone)]
pub struct ProfileNode {
    pub id: u32,
    pub function_name: String,
    pub url: String,
    pub line_number: u32,
    pub column_number: u32,
    pub hit_count: u32,
    pub children: Vec<u32>,
}

/// Performance panel
#[derive(Debug)]
pub struct PerformancePanel {
    entries: VecDeque<PerformanceEntry>,
    frames: VecDeque<FrameTimingInfo>,
    memory_samples: VecDeque<MemoryInfo>,
    marks: Vec<PerformanceEntry>,
    measures: Vec<PerformanceEntry>,
    max_entries: usize,
    recording: bool,
    current_frame: Option<FrameTimingInfo>,
}

impl Default for PerformancePanel {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            frames: VecDeque::new(),
            memory_samples: VecDeque::new(),
            marks: Vec::new(),
            measures: Vec::new(),
            max_entries: 1000,
            recording: false,
            current_frame: None,
        }
    }
}

impl PerformancePanel {
    pub fn new() -> Self { Self::default() }
    
    /// Start recording
    pub fn start_recording(&mut self) {
        self.recording = true;
        self.entries.clear();
        self.frames.clear();
        self.memory_samples.clear();
    }
    
    /// Stop recording
    pub fn stop_recording(&mut self) {
        self.recording = false;
    }
    
    /// Add performance mark
    pub fn mark(&mut self, name: &str) {
        let entry = PerformanceEntry {
            name: name.to_string(),
            entry_type: EntryType::Mark,
            start_time: current_time(),
            duration: 0.0,
        };
        self.marks.push(entry.clone());
        self.add_entry(entry);
    }
    
    /// Add performance measure
    pub fn measure(&mut self, name: &str, start_mark: &str, end_mark: Option<&str>) {
        let start = self.marks.iter()
            .find(|m| m.name == start_mark)
            .map(|m| m.start_time)
            .unwrap_or(0.0);
            
        let end = end_mark
            .and_then(|em| self.marks.iter().find(|m| m.name == em))
            .map(|m| m.start_time)
            .unwrap_or_else(current_time);
        
        let entry = PerformanceEntry {
            name: name.to_string(),
            entry_type: EntryType::Measure,
            start_time: start,
            duration: end - start,
        };
        self.measures.push(entry.clone());
        self.add_entry(entry);
    }
    
    /// Begin frame
    pub fn begin_frame(&mut self, frame_id: u64) {
        if self.recording {
            self.current_frame = Some(FrameTimingInfo::new(frame_id, current_time()));
        }
    }
    
    /// Record script time
    pub fn record_script_time(&mut self, time: f64) {
        if let Some(ref mut frame) = self.current_frame {
            frame.script_time = time;
        }
    }
    
    /// Record style time
    pub fn record_style_time(&mut self, time: f64) {
        if let Some(ref mut frame) = self.current_frame {
            frame.style_time = time;
        }
    }
    
    /// Record layout time
    pub fn record_layout_time(&mut self, time: f64) {
        if let Some(ref mut frame) = self.current_frame {
            frame.layout_time = time;
        }
    }
    
    /// Record paint time
    pub fn record_paint_time(&mut self, time: f64) {
        if let Some(ref mut frame) = self.current_frame {
            frame.paint_time = time;
        }
    }
    
    /// End frame
    pub fn end_frame(&mut self) {
        if let Some(mut frame) = self.current_frame.take() {
            frame.total_time = current_time() - frame.start_time;
            
            self.frames.push_back(frame);
            while self.frames.len() > self.max_entries {
                self.frames.pop_front();
            }
        }
    }
    
    /// Record memory sample
    pub fn record_memory(&mut self, info: MemoryInfo) {
        if self.recording {
            self.memory_samples.push_back(info);
            while self.memory_samples.len() > self.max_entries {
                self.memory_samples.pop_front();
            }
        }
    }
    
    fn add_entry(&mut self, entry: PerformanceEntry) {
        if self.recording {
            self.entries.push_back(entry);
            while self.entries.len() > self.max_entries {
                self.entries.pop_front();
            }
        }
    }
    
    /// Get average FPS
    pub fn get_average_fps(&self) -> f64 {
        if self.frames.is_empty() {
            return 0.0;
        }
        
        let total: f64 = self.frames.iter().map(|f| f.fps()).sum();
        total / self.frames.len() as f64
    }
    
    /// Get frame timeline
    pub fn get_frame_timeline(&self) -> &VecDeque<FrameTimingInfo> {
        &self.frames
    }
    
    /// Get memory timeline
    pub fn get_memory_timeline(&self) -> &VecDeque<MemoryInfo> {
        &self.memory_samples
    }
    
    /// Get entries by type
    pub fn get_entries_by_type(&self, entry_type: EntryType) -> Vec<&PerformanceEntry> {
        self.entries.iter()
            .filter(|e| e.entry_type == entry_type)
            .collect()
    }
    
    /// Clear marks
    pub fn clear_marks(&mut self, name: Option<&str>) {
        match name {
            Some(n) => self.marks.retain(|m| m.name != n),
            None => self.marks.clear(),
        }
    }
    
    /// Clear measures
    pub fn clear_measures(&mut self, name: Option<&str>) {
        match name {
            Some(n) => self.measures.retain(|m| m.name != n),
            None => self.measures.clear(),
        }
    }
}

fn current_time() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_performance_mark() {
        let mut panel = PerformancePanel::new();
        panel.start_recording();
        
        panel.mark("start");
        panel.mark("end");
        panel.measure("total", "start", Some("end"));
        
        assert_eq!(panel.marks.len(), 2);
        assert_eq!(panel.measures.len(), 1);
    }
    
    #[test]
    fn test_frame_timing() {
        let mut panel = PerformancePanel::new();
        panel.start_recording();
        
        panel.begin_frame(1);
        panel.record_script_time(5.0);
        panel.record_layout_time(3.0);
        panel.end_frame();
        
        assert_eq!(panel.frames.len(), 1);
    }
}
