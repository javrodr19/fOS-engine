//! Memory Panel
//!
//! Heap snapshots, allocation timeline, and memory leak detection.

use std::collections::HashMap;

/// Heap snapshot
#[derive(Debug, Clone)]
pub struct HeapSnapshot {
    pub id: u64,
    pub timestamp: u64,
    pub total_size: usize,
    pub node_count: usize,
    pub nodes: Vec<HeapNode>,
    pub edges: Vec<HeapEdge>,
}

/// Heap node
#[derive(Debug, Clone)]
pub struct HeapNode {
    pub id: u64,
    pub node_type: HeapNodeType,
    pub name: String,
    pub size: usize,
    pub retained_size: usize,
    pub edge_count: usize,
}

/// Heap node type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapNodeType { Hidden, Array, String, Object, Code, Closure, Regexp, Number, Native, Synthetic, Concatenated, Sliced, Symbol, BigInt }

impl HeapNodeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hidden => "hidden", Self::Array => "array", Self::String => "string", Self::Object => "object",
            Self::Code => "code", Self::Closure => "closure", Self::Regexp => "regexp", Self::Number => "number",
            Self::Native => "native", Self::Synthetic => "synthetic", Self::Concatenated => "concatenated",
            Self::Sliced => "sliced", Self::Symbol => "symbol", Self::BigInt => "bigint",
        }
    }
}

/// Heap edge
#[derive(Debug, Clone)]
pub struct HeapEdge {
    pub edge_type: HeapEdgeType,
    pub name_or_index: String,
    pub from_node: u64,
    pub to_node: u64,
}

/// Heap edge type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapEdgeType { Context, Element, Property, Internal, Hidden, Shortcut, Weak }

/// Allocation sample
#[derive(Debug, Clone)]
pub struct AllocationSample {
    pub timestamp: u64,
    pub size: usize,
    pub node_id: u64,
    pub stack_trace: Vec<StackFrame>,
}

/// Stack frame
#[derive(Debug, Clone)]
pub struct StackFrame {
    pub function_name: String,
    pub script_id: u32,
    pub url: String,
    pub line: u32,
    pub column: u32,
}

/// Retention path
#[derive(Debug, Clone)]
pub struct RetentionPath {
    pub nodes: Vec<RetentionNode>,
    pub total_retained: usize,
}

/// Retention node
#[derive(Debug, Clone)]
pub struct RetentionNode {
    pub node_id: u64,
    pub name: String,
    pub edge_name: String,
}

/// Memory leak candidate
#[derive(Debug, Clone)]
pub struct LeakCandidate {
    pub node_type: String,
    pub constructor: String,
    pub count_delta: i64,
    pub size_delta: i64,
    pub allocation_site: Option<StackFrame>,
}

/// Memory panel
#[derive(Debug, Default)]
pub struct MemoryPanel {
    snapshots: Vec<HeapSnapshot>,
    allocations: Vec<AllocationSample>,
    recording: bool,
    next_snapshot_id: u64,
}

impl MemoryPanel {
    pub fn new() -> Self { Self::default() }
    
    /// Take heap snapshot
    pub fn take_snapshot(&mut self) -> u64 {
        let id = self.next_snapshot_id;
        self.next_snapshot_id += 1;
        
        let snapshot = HeapSnapshot { id, timestamp: current_time_ms(), total_size: 0,
            node_count: 0, nodes: Vec::new(), edges: Vec::new() };
        self.snapshots.push(snapshot);
        id
    }
    
    /// Get snapshot
    pub fn get_snapshot(&self, id: u64) -> Option<&HeapSnapshot> {
        self.snapshots.iter().find(|s| s.id == id)
    }
    
    /// Start allocation recording
    pub fn start_recording(&mut self) {
        self.allocations.clear();
        self.recording = true;
    }
    
    /// Stop recording
    pub fn stop_recording(&mut self) { self.recording = false; }
    
    /// Record allocation
    pub fn record_allocation(&mut self, sample: AllocationSample) {
        if self.recording { self.allocations.push(sample); }
    }
    
    /// Compare snapshots
    pub fn compare_snapshots(&self, before_id: u64, after_id: u64) -> SnapshotComparison {
        let before = self.get_snapshot(before_id);
        let after = self.get_snapshot(after_id);
        
        let (size_delta, node_delta) = match (before, after) {
            (Some(b), Some(a)) => (a.total_size as i64 - b.total_size as i64, a.node_count as i64 - b.node_count as i64),
            _ => (0, 0),
        };
        
        SnapshotComparison { before_id, after_id, size_delta, node_delta, added_nodes: Vec::new(),
            deleted_nodes: Vec::new(), leak_candidates: Vec::new() }
    }
    
    /// Get allocation timeline
    pub fn get_allocation_timeline(&self) -> &[AllocationSample] { &self.allocations }
    
    /// Get snapshots
    pub fn get_snapshots(&self) -> &[HeapSnapshot] { &self.snapshots }
    
    /// Clear all data
    pub fn clear(&mut self) { self.snapshots.clear(); self.allocations.clear(); }
}

/// Snapshot comparison
#[derive(Debug, Clone)]
pub struct SnapshotComparison {
    pub before_id: u64,
    pub after_id: u64,
    pub size_delta: i64,
    pub node_delta: i64,
    pub added_nodes: Vec<HeapNode>,
    pub deleted_nodes: Vec<HeapNode>,
    pub leak_candidates: Vec<LeakCandidate>,
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_take_snapshot() {
        let mut panel = MemoryPanel::new();
        let id1 = panel.take_snapshot();
        let id2 = panel.take_snapshot();
        assert_eq!(id1, 0);
        assert_eq!(id2, 1);
        assert_eq!(panel.get_snapshots().len(), 2);
    }
    
    #[test]
    fn test_allocation_recording() {
        let mut panel = MemoryPanel::new();
        panel.start_recording();
        panel.record_allocation(AllocationSample { timestamp: 0, size: 100, node_id: 1, stack_trace: vec![] });
        panel.stop_recording();
        assert_eq!(panel.get_allocation_timeline().len(), 1);
    }
}
