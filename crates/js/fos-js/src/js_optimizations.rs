//! JavaScript Execution Optimizations
//!
//! Lazy compilation, dead code elimination, constant folding, escape analysis,
//! bytecode caching, and heap compression.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Lazy function compilation
#[derive(Debug, Default)]
pub struct LazyCompiler {
    /// Compiled functions
    compiled: HashSet<u64>,
    /// Functions awaiting compilation
    pending: Vec<PendingFunction>,
    /// Compilation threshold (call count before compiling)
    threshold: u32,
    /// Call counts
    call_counts: HashMap<u64, u32>,
}

/// Pending function for lazy compilation
#[derive(Debug, Clone)]
pub struct PendingFunction {
    pub id: u64,
    pub source: String,
    pub is_async: bool,
    pub is_generator: bool,
}

impl LazyCompiler {
    pub fn new(threshold: u32) -> Self {
        Self {
            threshold,
            ..Default::default()
        }
    }
    
    /// Register a function for lazy compilation
    pub fn register(&mut self, func: PendingFunction) {
        if !self.compiled.contains(&func.id) {
            self.pending.push(func);
        }
    }
    
    /// Record a call to a function
    pub fn record_call(&mut self, func_id: u64) -> bool {
        let count = self.call_counts.entry(func_id).or_insert(0);
        *count += 1;
        
        // Check if should compile
        if *count >= self.threshold && !self.compiled.contains(&func_id) {
            self.compiled.insert(func_id);
            return true; // Trigger compilation
        }
        false
    }
    
    /// Check if function is compiled
    pub fn is_compiled(&self, func_id: u64) -> bool {
        self.compiled.contains(&func_id)
    }
    
    /// Get pending functions that should be compiled
    pub fn get_hot_functions(&self) -> Vec<u64> {
        self.call_counts.iter()
            .filter(|(id, count)| **count >= self.threshold && !self.compiled.contains(*id))
            .map(|(id, _)| *id)
            .collect()
    }
}

/// Dead code elimination
#[derive(Debug, Default)]
pub struct DeadCodeEliminator {
    /// Live variables at each program point
    live_vars: HashMap<u64, HashSet<String>>,
    /// Dead code regions
    dead_regions: Vec<CodeRegion>,
}

/// Code region
#[derive(Debug, Clone)]
pub struct CodeRegion {
    pub start: usize,
    pub end: usize,
}

impl DeadCodeEliminator {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Mark a variable as live at a point
    pub fn mark_live(&mut self, point: u64, var: &str) {
        self.live_vars.entry(point).or_default().insert(var.to_string());
    }
    
    /// Check if variable is live at point
    pub fn is_live(&self, point: u64, var: &str) -> bool {
        self.live_vars.get(&point).map(|vars| vars.contains(var)).unwrap_or(false)
    }
    
    /// Mark a region as dead code
    pub fn mark_dead(&mut self, start: usize, end: usize) {
        self.dead_regions.push(CodeRegion { start, end });
    }
    
    /// Get dead regions
    pub fn dead_regions(&self) -> &[CodeRegion] {
        &self.dead_regions
    }
}

/// Constant folding optimizer
#[derive(Debug, Default)]
pub struct ConstantFolder {
    /// Known constants
    constants: HashMap<String, ConstValue>,
}

/// Constant value
#[derive(Debug, Clone)]
pub enum ConstValue {
    Number(f64),
    String(String),
    Boolean(bool),
    Undefined,
    Null,
}

impl ConstantFolder {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a constant
    pub fn register(&mut self, name: &str, value: ConstValue) {
        self.constants.insert(name.to_string(), value);
    }
    
    /// Get a constant
    pub fn get(&self, name: &str) -> Option<&ConstValue> {
        self.constants.get(name)
    }
    
    /// Fold a binary numeric operation
    pub fn fold_binary(&self, op: BinaryOp, left: f64, right: f64) -> f64 {
        match op {
            BinaryOp::Add => left + right,
            BinaryOp::Sub => left - right,
            BinaryOp::Mul => left * right,
            BinaryOp::Div => left / right,
            BinaryOp::Mod => left % right,
            BinaryOp::Pow => left.powf(right),
        }
    }
    
    /// Fold a unary operation
    pub fn fold_unary(&self, op: UnaryOp, val: f64) -> f64 {
        match op {
            UnaryOp::Neg => -val,
            UnaryOp::Not => if val == 0.0 { 1.0 } else { 0.0 },
        }
    }
}

/// Binary operation
#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod, Pow,
}

/// Unary operation
#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg, Not,
}

/// Escape analysis for stack allocation
#[derive(Debug, Default)]
pub struct EscapeAnalyzer {
    /// Objects that escape
    escaping: HashSet<u64>,
    /// Object allocation sites
    allocations: HashMap<u64, AllocationSite>,
}

/// Allocation site info
#[derive(Debug, Clone)]
pub struct AllocationSite {
    pub id: u64,
    pub escapes: bool,
    pub can_stack_allocate: bool,
}

impl EscapeAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Record an allocation
    pub fn record_allocation(&mut self, id: u64) {
        self.allocations.insert(id, AllocationSite {
            id,
            escapes: false,
            can_stack_allocate: true,
        });
    }
    
    /// Mark object as escaping
    pub fn mark_escaping(&mut self, id: u64) {
        self.escaping.insert(id);
        if let Some(site) = self.allocations.get_mut(&id) {
            site.escapes = true;
            site.can_stack_allocate = false;
        }
    }
    
    /// Check if object escapes
    pub fn escapes(&self, id: u64) -> bool {
        self.escaping.contains(&id)
    }
    
    /// Get objects that can be stack-allocated
    pub fn stack_allocatable(&self) -> Vec<u64> {
        self.allocations.iter()
            .filter(|(_, site)| site.can_stack_allocate)
            .map(|(id, _)| *id)
            .collect()
    }
}

/// Bytecode cache
#[derive(Debug, Default)]
pub struct BytecodeCache {
    /// Cached bytecode by source hash
    cache: HashMap<u64, CachedBytecode>,
    /// Cache hits
    hits: u64,
    /// Cache misses
    misses: u64,
}

/// Cached bytecode entry
#[derive(Debug, Clone)]
pub struct CachedBytecode {
    /// Source hash
    pub hash: u64,
    /// Bytecode
    pub bytecode: Vec<u8>,
    /// Timestamp
    pub timestamp: u64,
}

impl BytecodeCache {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get cached bytecode
    pub fn get(&mut self, hash: u64) -> Option<&CachedBytecode> {
        if let Some(entry) = self.cache.get(&hash) {
            self.hits += 1;
            Some(entry)
        } else {
            self.misses += 1;
            None
        }
    }
    
    /// Store bytecode
    pub fn store(&mut self, hash: u64, bytecode: Vec<u8>, timestamp: u64) {
        self.cache.insert(hash, CachedBytecode {
            hash,
            bytecode,
            timestamp,
        });
    }
    
    /// Clear cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }
    
    /// Get stats
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            entries: self.cache.len(),
            hits: self.hits,
            misses: self.misses,
            hit_rate: if self.hits + self.misses > 0 {
                self.hits as f64 / (self.hits + self.misses) as f64
            } else {
                0.0
            },
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub entries: usize,
    pub hits: u64,
    pub misses: u64,
    pub hit_rate: f64,
}

/// Heap compression for idle contexts
#[derive(Debug, Default)]
pub struct HeapCompressor {
    /// Compressed heaps by context ID
    compressed: HashMap<u64, CompressedHeap>,
}

/// Compressed heap data
#[derive(Debug, Clone)]
pub struct CompressedHeap {
    /// Original size
    pub original_size: usize,
    /// Compressed data
    pub data: Vec<u8>,
    /// Compression ratio
    pub ratio: f64,
}

impl HeapCompressor {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Compress a heap
    pub fn compress(&mut self, context_id: u64, heap: &[u8]) -> usize {
        // Simple RLE compression (placeholder for real LZ4 etc.)
        let compressed = simple_compress(heap);
        let original_size = heap.len();
        let compressed_size = compressed.len();
        
        let ratio = if original_size > 0 {
            compressed_size as f64 / original_size as f64
        } else {
            1.0
        };
        
        self.compressed.insert(context_id, CompressedHeap {
            original_size,
            data: compressed,
            ratio,
        });
        
        compressed_size
    }
    
    /// Decompress a heap
    pub fn decompress(&mut self, context_id: u64) -> Option<Vec<u8>> {
        self.compressed.remove(&context_id).map(|h| simple_decompress(&h.data))
    }
    
    /// Check if context has compressed heap
    pub fn is_compressed(&self, context_id: u64) -> bool {
        self.compressed.contains_key(&context_id)
    }
    
    /// Get memory saved
    pub fn memory_saved(&self) -> usize {
        self.compressed.values()
            .map(|h| h.original_size.saturating_sub(h.data.len()))
            .sum()
    }
}

/// Simple RLE compression (placeholder)
fn simple_compress(data: &[u8]) -> Vec<u8> {
    // For real implementation, use LZ4 or similar
    let mut result = Vec::new();
    let mut i = 0;
    
    while i < data.len() {
        let byte = data[i];
        let mut count = 1u8;
        
        while i + (count as usize) < data.len() 
            && data[i + count as usize] == byte 
            && count < 255 
        {
            count += 1;
        }
        
        result.push(count);
        result.push(byte);
        i += count as usize;
    }
    
    result
}

/// Simple RLE decompression
fn simple_decompress(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::new();
    
    for chunk in data.chunks(2) {
        if chunk.len() == 2 {
            let count = chunk[0];
            let byte = chunk[1];
            for _ in 0..count {
                result.push(byte);
            }
        }
    }
    
    result
}

/// Shared builtins across contexts
#[derive(Debug, Default)]
pub struct SharedBuiltins {
    /// Shared function templates
    functions: HashMap<String, SharedFunction>,
    /// Reference count by context
    ref_counts: HashMap<u64, usize>,
}

/// Shared function template
#[derive(Debug, Clone)]
pub struct SharedFunction {
    pub name: String,
    pub bytecode: Vec<u8>,
}

impl SharedBuiltins {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a shared builtin
    pub fn register(&mut self, name: &str, bytecode: Vec<u8>) {
        self.functions.insert(name.to_string(), SharedFunction {
            name: name.to_string(),
            bytecode,
        });
    }
    
    /// Get a shared builtin
    pub fn get(&self, name: &str) -> Option<&SharedFunction> {
        self.functions.get(name)
    }
    
    /// Link context to shared builtins
    pub fn link_context(&mut self, context_id: u64) {
        *self.ref_counts.entry(context_id).or_insert(0) += 1;
    }
    
    /// Unlink context
    pub fn unlink_context(&mut self, context_id: u64) {
        if let Some(count) = self.ref_counts.get_mut(&context_id) {
            *count = count.saturating_sub(1);
        }
    }
    
    /// Memory saved by sharing
    pub fn memory_saved(&self, contexts: usize) -> usize {
        if contexts <= 1 {
            return 0;
        }
        
        let builtin_size: usize = self.functions.values()
            .map(|f| f.bytecode.len())
            .sum();
        
        // Saved = (contexts - 1) * builtin_size
        (contexts - 1) * builtin_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lazy_compiler() {
        let mut compiler = LazyCompiler::new(3);
        
        // Should not compile until threshold
        assert!(!compiler.record_call(1));
        assert!(!compiler.record_call(1));
        assert!(compiler.record_call(1)); // 3rd call triggers
        
        assert!(compiler.is_compiled(1));
    }
    
    #[test]
    fn test_constant_folder() {
        let folder = ConstantFolder::new();
        
        assert_eq!(folder.fold_binary(BinaryOp::Add, 2.0, 3.0), 5.0);
        assert_eq!(folder.fold_binary(BinaryOp::Mul, 4.0, 5.0), 20.0);
    }
    
    #[test]
    fn test_escape_analyzer() {
        let mut analyzer = EscapeAnalyzer::new();
        
        analyzer.record_allocation(1);
        assert!(!analyzer.escapes(1));
        
        analyzer.mark_escaping(1);
        assert!(analyzer.escapes(1));
    }
    
    #[test]
    fn test_bytecode_cache() {
        let mut cache = BytecodeCache::new();
        
        cache.store(123, vec![1, 2, 3], 0);
        
        assert!(cache.get(123).is_some());
        assert!(cache.get(456).is_none());
        
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
    }
    
    #[test]
    fn test_heap_compression() {
        let mut compressor = HeapCompressor::new();
        
        // Create data with repeated pattern
        let heap = vec![0u8; 1000];
        
        compressor.compress(1, &heap);
        assert!(compressor.is_compressed(1));
        
        let decompressed = compressor.decompress(1).unwrap();
        assert_eq!(decompressed, heap);
    }
}
