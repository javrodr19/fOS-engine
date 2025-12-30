//! Garbage Collector
//!
//! Mark-and-sweep garbage collector for JavaScript objects.

use super::value::JsVal;
use super::object::{JsObject, JsArray};
use std::collections::HashSet;

/// GC statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct GcStats {
    pub collections: u64,
    pub objects_freed: u64,
    pub bytes_freed: u64,
}

/// Garbage Collector
pub struct GarbageCollector {
    heap_size: usize,
    threshold: usize,
    stats: GcStats,
}

impl Default for GarbageCollector {
    fn default() -> Self { Self::new() }
}

impl GarbageCollector {
    pub fn new() -> Self {
        Self { heap_size: 0, threshold: 1024 * 1024, stats: GcStats::default() }
    }
    
    pub fn with_threshold(threshold: usize) -> Self {
        Self { heap_size: 0, threshold, stats: GcStats::default() }
    }
    
    /// Check if collection is needed
    pub fn should_collect(&self) -> bool { self.heap_size >= self.threshold }
    
    /// Mark phase - trace from roots
    pub fn mark(&self, roots: &[JsVal], objects: &[JsObject], arrays: &[JsArray]) -> HashSet<u32> {
        let mut marked = HashSet::new();
        let mut worklist: Vec<u32> = Vec::new();
        
        for root in roots {
            let id = root.as_object_id()
                .or_else(|| root.as_array_id())
                .or_else(|| root.as_function_id());
            if let Some(id) = id {
                if marked.insert(id) { worklist.push(id); }
            }
        }
        
        while let Some(id) = worklist.pop() {
            if let Some(obj) = objects.get(id as usize) {
                for val in obj.keys().filter_map(|k| obj.get(k)) {
                    let id = val.as_object_id()
                        .or_else(|| val.as_array_id())
                        .or_else(|| val.as_function_id());
                    if let Some(id) = id {
                        if marked.insert(id) { worklist.push(id); }
                    }
                }
            }
        }
        
        marked
    }
    
    /// Record allocation
    pub fn record_alloc(&mut self, bytes: usize) { self.heap_size += bytes; }
    
    /// Record freed memory
    pub fn record_free(&mut self, bytes: usize, count: u64) {
        self.heap_size = self.heap_size.saturating_sub(bytes);
        self.stats.objects_freed += count;
        self.stats.bytes_freed += bytes as u64;
        self.stats.collections += 1;
    }
    
    pub fn stats(&self) -> &GcStats { &self.stats }
    pub fn heap_size(&self) -> usize { self.heap_size }
}
