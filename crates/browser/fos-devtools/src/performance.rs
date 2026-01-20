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

/// Paint event for DevTools timeline
#[derive(Debug, Clone)]
pub struct PaintEvent {
    pub timestamp: f64,
    pub duration: f64,
    pub layer_id: u32,
    pub clip: PaintClip,
    pub paint_type: PaintType,
}

/// Paint type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintType {
    Full,
    Incremental,
    Composite,
}

/// Paint clip region
#[derive(Debug, Clone, Default)]
pub struct PaintClip {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Script execution event for profiling
#[derive(Debug, Clone)]
pub struct ScriptExecutionEvent {
    pub timestamp: f64,
    pub duration: f64,
    pub function_name: String,
    pub url: String,
    pub line: u32,
    pub column: u32,
    pub event_type: ScriptEventType,
    pub call_uid: u64,
    pub parent_uid: Option<u64>,
}

/// Script event type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptEventType {
    FunctionCall,
    Compile,
    Execute,
    GC,
    ParseHTML,
    EvaluateScript,
    EventHandler,
    TimerFire,
    RequestAnimationFrame,
}

/// Flame chart for visualization
#[derive(Debug, Clone)]
pub struct FlameChart {
    pub start_time: f64,
    pub end_time: f64,
    pub nodes: Vec<FlameChartNode>,
    pub max_depth: u32,
}

impl FlameChart {
    pub fn new() -> Self {
        Self {
            start_time: 0.0,
            end_time: 0.0,
            nodes: Vec::new(),
            max_depth: 0,
        }
    }
    
    /// Add a node to the flame chart
    pub fn add_node(&mut self, node: FlameChartNode) {
        if node.depth > self.max_depth {
            self.max_depth = node.depth;
        }
        if node.start_time < self.start_time || self.nodes.is_empty() {
            self.start_time = node.start_time;
        }
        if node.end_time > self.end_time {
            self.end_time = node.end_time;
        }
        self.nodes.push(node);
    }
    
    /// Get total duration
    pub fn duration(&self) -> f64 {
        self.end_time - self.start_time
    }
}

impl Default for FlameChart {
    fn default() -> Self {
        Self::new()
    }
}

/// Flame chart node
#[derive(Debug, Clone)]
pub struct FlameChartNode {
    pub id: u32,
    pub name: String,
    pub category: FlameChartCategory,
    pub start_time: f64,
    pub end_time: f64,
    pub depth: u32,
    pub self_time: f64,
    pub total_time: f64,
    pub url: Option<String>,
    pub line: Option<u32>,
}

/// Flame chart category for color coding
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlameChartCategory {
    Scripting,
    Rendering,
    Painting,
    Loading,
    System,
    Idle,
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
    /// Paint events timeline
    paint_events: Vec<PaintEvent>,
    /// Script execution events
    script_events: Vec<ScriptExecutionEvent>,
    /// Generated flame chart
    flame_chart: FlameChart,
    /// Next call UID for script events
    next_call_uid: u64,
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
            paint_events: Vec::new(),
            script_events: Vec::new(),
            flame_chart: FlameChart::new(),
            next_call_uid: 0,
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
    
    // === Paint Events ===
    
    /// Record a paint event
    pub fn record_paint(&mut self, layer_id: u32, clip: PaintClip, paint_type: PaintType, duration: f64) {
        if self.recording {
            self.paint_events.push(PaintEvent {
                timestamp: current_time(),
                duration,
                layer_id,
                clip,
                paint_type,
            });
        }
    }
    
    /// Get paint events
    pub fn get_paint_events(&self) -> &[PaintEvent] {
        &self.paint_events
    }
    
    // === Script Execution ===
    
    /// Begin script execution tracking
    pub fn begin_script_execution(
        &mut self, 
        function_name: &str, 
        url: &str, 
        line: u32, 
        column: u32,
        event_type: ScriptEventType,
        parent_uid: Option<u64>,
    ) -> u64 {
        let uid = self.next_call_uid;
        self.next_call_uid += 1;
        
        if self.recording {
            self.script_events.push(ScriptExecutionEvent {
                timestamp: current_time(),
                duration: 0.0, // Will be updated on end
                function_name: function_name.to_string(),
                url: url.to_string(),
                line,
                column,
                event_type,
                call_uid: uid,
                parent_uid,
            });
        }
        
        uid
    }
    
    /// End script execution tracking
    pub fn end_script_execution(&mut self, call_uid: u64) {
        if let Some(event) = self.script_events.iter_mut()
            .find(|e| e.call_uid == call_uid)
        {
            event.duration = current_time() - event.timestamp;
        }
    }
    
    /// Get script events
    pub fn get_script_events(&self) -> &[ScriptExecutionEvent] {
        &self.script_events
    }
    
    // === Flame Chart ===
    
    /// Generate flame chart from recorded script events
    pub fn generate_flame_chart(&mut self) -> &FlameChart {
        self.flame_chart = FlameChart::new();
        let mut next_id = 0u32;
        
        // Build flame chart nodes from script events
        for event in &self.script_events {
            let depth = self.calculate_depth(event.call_uid);
            
            let category = match event.event_type {
                ScriptEventType::FunctionCall | ScriptEventType::Execute | 
                ScriptEventType::EvaluateScript => FlameChartCategory::Scripting,
                ScriptEventType::ParseHTML => FlameChartCategory::Loading,
                ScriptEventType::GC => FlameChartCategory::System,
                _ => FlameChartCategory::Scripting,
            };
            
            let node = FlameChartNode {
                id: next_id,
                name: event.function_name.clone(),
                category,
                start_time: event.timestamp,
                end_time: event.timestamp + event.duration,
                depth,
                self_time: event.duration, // Simplified - would subtract child time
                total_time: event.duration,
                url: Some(event.url.clone()),
                line: Some(event.line),
            };
            
            self.flame_chart.add_node(node);
            next_id += 1;
        }
        
        // Add paint events as rendering nodes
        for event in &self.paint_events {
            let node = FlameChartNode {
                id: next_id,
                name: format!("Paint ({:?})", event.paint_type),
                category: FlameChartCategory::Painting,
                start_time: event.timestamp,
                end_time: event.timestamp + event.duration,
                depth: 0,
                self_time: event.duration,
                total_time: event.duration,
                url: None,
                line: None,
            };
            
            self.flame_chart.add_node(node);
            next_id += 1;
        }
        
        &self.flame_chart
    }
    
    /// Calculate depth for a call in the call tree
    fn calculate_depth(&self, call_uid: u64) -> u32 {
        let mut depth = 0u32;
        let mut current = self.script_events.iter()
            .find(|e| e.call_uid == call_uid);
        
        while let Some(event) = current {
            if let Some(parent_uid) = event.parent_uid {
                depth += 1;
                current = self.script_events.iter()
                    .find(|e| e.call_uid == parent_uid);
            } else {
                break;
            }
        }
        
        depth
    }
    
    /// Get the current flame chart
    pub fn get_flame_chart(&self) -> &FlameChart {
        &self.flame_chart
    }
    
    /// Clear all performance data
    pub fn clear_all(&mut self) {
        self.entries.clear();
        self.frames.clear();
        self.memory_samples.clear();
        self.marks.clear();
        self.measures.clear();
        self.paint_events.clear();
        self.script_events.clear();
        self.flame_chart = FlameChart::new();
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
