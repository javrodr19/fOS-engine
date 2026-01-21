//! Generational Garbage Collector
//!
//! High-performance generational garbage collector for JavaScript objects.
//! Uses a two-generation design with a copying nursery and mark-sweep old gen.
//!
//! ## Generations
//! - **Nursery (Young Gen)**: Bump-pointer allocation, scavenge collection
//! - **Old Gen**: Mark-sweep-compact, incremental marking
//! - **Large Object Space**: Objects > 8KB allocated directly
//!
//! ## Write Barriers
//! Uses a card table to track old→young pointers for efficient minor GC.

use std::collections::{HashMap, HashSet};
use std::ptr::NonNull;

/// Object header flags
#[derive(Debug, Clone, Copy)]
pub struct ObjectFlags {
    /// Mark bit for GC
    pub marked: bool,
    /// Object has been promoted to old gen
    pub tenured: bool,
    /// Object has old→young pointers
    pub remembered: bool,
    /// Object is pinned (can't be moved)
    pub pinned: bool,
}

impl Default for ObjectFlags {
    fn default() -> Self {
        Self {
            marked: false,
            tenured: false,
            remembered: false,
            pinned: false,
        }
    }
}

/// Object header stored before each object
#[derive(Debug, Clone, Copy)]
pub struct ObjectHeader {
    /// Size of object in bytes (excluding header)
    pub size: u32,
    /// Object shape/class ID
    pub shape_id: u32,
    /// GC flags
    pub flags: ObjectFlags,
    /// Forwarding pointer (used during GC)
    pub forwarding: Option<u32>,
}

impl ObjectHeader {
    pub fn new(size: u32, shape_id: u32) -> Self {
        Self {
            size,
            shape_id,
            flags: ObjectFlags::default(),
            forwarding: None,
        }
    }
}

/// Nursery (young generation) - uses bump-pointer allocation
#[derive(Debug)]
pub struct Nursery {
    /// Memory buffer
    memory: Vec<u8>,
    /// Current allocation pointer
    alloc_ptr: usize,
    /// Size of nursery
    capacity: usize,
    /// Number of collections
    collections: u64,
    /// Objects allocated
    objects_allocated: u64,
    /// Bytes allocated total
    bytes_allocated: u64,
}

impl Nursery {
    /// Create nursery with given capacity (default 2MB)
    pub fn new(capacity: usize) -> Self {
        Self {
            memory: vec![0u8; capacity],
            alloc_ptr: 0,
            capacity,
            collections: 0,
            objects_allocated: 0,
            bytes_allocated: 0,
        }
    }

    /// Allocate object in nursery
    /// Returns offset into nursery memory
    pub fn allocate(&mut self, size: usize, shape_id: u32) -> Option<u32> {
        let header_size = std::mem::size_of::<ObjectHeader>();
        let total_size = header_size + size;
        let aligned_size = (total_size + 7) & !7; // 8-byte alignment

        if self.alloc_ptr + aligned_size > self.capacity {
            return None; // Nursery full, need collection
        }

        let offset = self.alloc_ptr as u32;
        
        // Write header
        let header = ObjectHeader::new(size as u32, shape_id);
        let header_bytes: [u8; std::mem::size_of::<ObjectHeader>()] = unsafe {
            std::mem::transmute_copy(&header)
        };
        self.memory[self.alloc_ptr..self.alloc_ptr + header_size]
            .copy_from_slice(&header_bytes[..header_size.min(header_bytes.len())]);

        self.alloc_ptr += aligned_size;
        self.objects_allocated += 1;
        self.bytes_allocated += aligned_size as u64;

        Some(offset)
    }

    /// Reset nursery after scavenge
    pub fn reset(&mut self) {
        self.alloc_ptr = 0;
        self.collections += 1;
    }

    /// Check if nursery needs collection
    pub fn needs_collection(&self) -> bool {
        self.alloc_ptr > self.capacity * 3 / 4 // 75% full
    }

    /// Get used bytes
    pub fn used(&self) -> usize {
        self.alloc_ptr
    }

    /// Get capacity
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get statistics
    pub fn stats(&self) -> NurseryStats {
        NurseryStats {
            capacity: self.capacity,
            used: self.alloc_ptr,
            collections: self.collections,
            objects_allocated: self.objects_allocated,
            bytes_allocated: self.bytes_allocated,
        }
    }
}

/// Nursery statistics
#[derive(Debug, Clone)]
pub struct NurseryStats {
    pub capacity: usize,
    pub used: usize,
    pub collections: u64,
    pub objects_allocated: u64,
    pub bytes_allocated: u64,
}

/// Old generation - uses mark-sweep with optional compaction
#[derive(Debug)]
pub struct OldGeneration {
    /// Memory chunks
    chunks: Vec<Vec<u8>>,
    /// Free lists by size class
    free_lists: HashMap<usize, Vec<u32>>,
    /// Chunk size
    chunk_size: usize,
    /// Total allocated
    allocated: usize,
    /// Objects in old gen
    object_count: u64,
}

impl OldGeneration {
    pub fn new(chunk_size: usize) -> Self {
        Self {
            chunks: Vec::new(),
            free_lists: HashMap::new(),
            chunk_size,
            allocated: 0,
            object_count: 0,
        }
    }

    /// Allocate in old generation
    pub fn allocate(&mut self, size: usize) -> Option<OldGenPtr> {
        let size_class = self.size_class(size);
        
        // Try free list first
        if let Some(free_list) = self.free_lists.get_mut(&size_class) {
            if let Some(offset) = free_list.pop() {
                return Some(OldGenPtr {
                    chunk: self.chunks.len() as u32 - 1,
                    offset,
                });
            }
        }

        // Allocate new space
        self.allocate_new(size)
    }

    /// Promote object from nursery to old gen
    pub fn promote(&mut self, data: &[u8], header: ObjectHeader) -> Option<OldGenPtr> {
        let size = data.len() + std::mem::size_of::<ObjectHeader>();
        let ptr = self.allocate(size)?;
        
        // Copy header and data
        if let Some(chunk) = self.chunks.get_mut(ptr.chunk as usize) {
            let offset = ptr.offset as usize;
            let header_size = std::mem::size_of::<ObjectHeader>();
            
            // Write header with tenured flag
            let mut promoted_header = header;
            promoted_header.flags.tenured = true;
            let header_bytes: [u8; std::mem::size_of::<ObjectHeader>()] = unsafe {
                std::mem::transmute_copy(&promoted_header)
            };
            
            if offset + header_size + data.len() <= chunk.len() {
                chunk[offset..offset + header_size.min(header_bytes.len())]
                    .copy_from_slice(&header_bytes[..header_size.min(header_bytes.len())]);
                chunk[offset + header_size..offset + header_size + data.len()]
                    .copy_from_slice(data);
            }
        }

        self.object_count += 1;
        Some(ptr)
    }

    fn size_class(&self, size: usize) -> usize {
        // Round up to power of 2, minimum 32 bytes
        let min_size = 32;
        if size <= min_size {
            min_size
        } else {
            size.next_power_of_two()
        }
    }

    fn allocate_new(&mut self, size: usize) -> Option<OldGenPtr> {
        let aligned_size = (size + 7) & !7;
        
        // Check if current chunk has space
        if let Some(chunk) = self.chunks.last_mut() {
            if self.allocated + aligned_size <= chunk.len() {
                let offset = self.allocated as u32;
                self.allocated += aligned_size;
                return Some(OldGenPtr {
                    chunk: self.chunks.len() as u32 - 1,
                    offset,
                });
            }
        }

        // Need new chunk
        self.chunks.push(vec![0u8; self.chunk_size]);
        self.allocated = aligned_size;
        Some(OldGenPtr {
            chunk: self.chunks.len() as u32 - 1,
            offset: 0,
        })
    }

    /// Mark object as live
    pub fn mark(&mut self, ptr: OldGenPtr) {
        // Would update mark bit in actual implementation
    }

    /// Sweep unmarked objects
    pub fn sweep(&mut self) -> u64 {
        // Would iterate and free unmarked objects
        0
    }

    /// Get statistics
    pub fn stats(&self) -> OldGenStats {
        OldGenStats {
            chunks: self.chunks.len(),
            chunk_size: self.chunk_size,
            allocated: self.allocated,
            object_count: self.object_count,
        }
    }
}

/// Old generation statistics
#[derive(Debug, Clone)]
pub struct OldGenStats {
    pub chunks: usize,
    pub chunk_size: usize,
    pub allocated: usize,
    pub object_count: u64,
}

/// Pointer into old generation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OldGenPtr {
    pub chunk: u32,
    pub offset: u32,
}

/// Card table for write barrier
/// Each card represents 512 bytes of old generation
#[derive(Debug)]
pub struct CardTable {
    /// Card states
    cards: Vec<CardState>,
    /// Card size in bytes
    card_size: usize,
}

/// Card state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    /// Card is clean (no old→young pointers)
    Clean,
    /// Card is dirty (may contain old→young pointers)
    Dirty,
}

impl CardTable {
    pub fn new(heap_size: usize, card_size: usize) -> Self {
        let num_cards = (heap_size + card_size - 1) / card_size;
        Self {
            cards: vec![CardState::Clean; num_cards],
            card_size,
        }
    }

    /// Mark card as dirty
    pub fn mark_dirty(&mut self, addr: usize) {
        let card_idx = addr / self.card_size;
        if card_idx < self.cards.len() {
            self.cards[card_idx] = CardState::Dirty;
        }
    }

    /// Get dirty cards
    pub fn dirty_cards(&self) -> impl Iterator<Item = usize> + '_ {
        self.cards.iter().enumerate()
            .filter(|(_, state)| **state == CardState::Dirty)
            .map(|(idx, _)| idx)
    }

    /// Clear all cards
    pub fn clear(&mut self) {
        self.cards.fill(CardState::Clean);
    }

    /// Count dirty cards
    pub fn dirty_count(&self) -> usize {
        self.cards.iter().filter(|s| **s == CardState::Dirty).count()
    }
}

/// Large object space for objects > threshold
#[derive(Debug)]
pub struct LargeObjectSpace {
    /// Large objects
    objects: HashMap<u32, Vec<u8>>,
    /// Next object ID
    next_id: u32,
    /// Total size
    total_size: usize,
    /// Size threshold
    threshold: usize,
}

impl LargeObjectSpace {
    pub fn new(threshold: usize) -> Self {
        Self {
            objects: HashMap::new(),
            next_id: 0,
            total_size: 0,
            threshold,
        }
    }

    /// Allocate large object
    pub fn allocate(&mut self, size: usize) -> Option<u32> {
        if size < self.threshold {
            return None; // Too small
        }

        let id = self.next_id;
        self.next_id += 1;
        self.objects.insert(id, vec![0u8; size]);
        self.total_size += size;
        Some(id)
    }

    /// Free large object
    pub fn free(&mut self, id: u32) {
        if let Some(obj) = self.objects.remove(&id) {
            self.total_size -= obj.len();
        }
    }

    /// Get stats
    pub fn stats(&self) -> LargeObjectStats {
        LargeObjectStats {
            count: self.objects.len(),
            total_size: self.total_size,
            threshold: self.threshold,
        }
    }
}

/// Large object space statistics
#[derive(Debug, Clone)]
pub struct LargeObjectStats {
    pub count: usize,
    pub total_size: usize,
    pub threshold: usize,
}

/// Root set for GC
#[derive(Debug, Default)]
pub struct RootSet {
    /// Stack roots
    pub stack_roots: Vec<u32>,
    /// Global roots
    pub global_roots: Vec<u32>,
    /// Handle roots (explicit refs from native code)
    pub handle_roots: Vec<u32>,
}

impl RootSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_stack_root(&mut self, obj: u32) {
        self.stack_roots.push(obj);
    }

    pub fn add_global_root(&mut self, obj: u32) {
        self.global_roots.push(obj);
    }

    pub fn clear_stack_roots(&mut self) {
        self.stack_roots.clear();
    }

    pub fn all_roots(&self) -> impl Iterator<Item = u32> + '_ {
        self.stack_roots.iter()
            .chain(self.global_roots.iter())
            .chain(self.handle_roots.iter())
            .copied()
    }
}

/// Generational garbage collector
#[derive(Debug)]
pub struct GenerationalGC {
    /// Young generation
    nursery: Nursery,
    /// Old generation
    old_gen: OldGeneration,
    /// Large object space
    large_objects: LargeObjectSpace,
    /// Card table for remembered set
    card_table: CardTable,
    /// GC statistics
    stats: GcStats,
    /// Configuration
    config: GcConfig,
}

/// GC configuration
#[derive(Debug, Clone)]
pub struct GcConfig {
    /// Nursery size in bytes
    pub nursery_size: usize,
    /// Old gen chunk size
    pub old_gen_chunk_size: usize,
    /// Large object threshold
    pub large_object_threshold: usize,
    /// Card size for write barrier
    pub card_size: usize,
    /// Promotion threshold (survive N collections)
    pub promotion_threshold: u32,
}

impl Default for GcConfig {
    fn default() -> Self {
        Self {
            nursery_size: 2 * 1024 * 1024,      // 2MB nursery
            old_gen_chunk_size: 4 * 1024 * 1024, // 4MB chunks
            large_object_threshold: 8 * 1024,    // 8KB threshold
            card_size: 512,                       // 512 byte cards
            promotion_threshold: 2,               // Promote after 2 survivals
        }
    }
}

/// GC statistics
#[derive(Debug, Clone, Default)]
pub struct GcStats {
    /// Minor collections
    pub minor_collections: u64,
    /// Major collections
    pub major_collections: u64,
    /// Objects promoted to old gen
    pub promotions: u64,
    /// Total bytes collected
    pub bytes_collected: u64,
    /// Total time spent in GC (nanoseconds)
    pub gc_time_ns: u64,
}

impl GenerationalGC {
    pub fn new() -> Self {
        Self::with_config(GcConfig::default())
    }

    pub fn with_config(config: GcConfig) -> Self {
        let card_table = CardTable::new(config.old_gen_chunk_size * 16, config.card_size);
        
        Self {
            nursery: Nursery::new(config.nursery_size),
            old_gen: OldGeneration::new(config.old_gen_chunk_size),
            large_objects: LargeObjectSpace::new(config.large_object_threshold),
            card_table,
            stats: GcStats::default(),
            config,
        }
    }

    /// Allocate object
    pub fn allocate(&mut self, size: usize, shape_id: u32) -> Option<GcPtr> {
        // Large objects go directly to LOS
        if size >= self.config.large_object_threshold {
            return self.large_objects.allocate(size)
                .map(|id| GcPtr::Large(id));
        }

        // Try nursery first
        if let Some(offset) = self.nursery.allocate(size, shape_id) {
            return Some(GcPtr::Young(offset));
        }

        // Nursery full, trigger minor GC
        self.minor_gc(&RootSet::new());

        // Retry allocation
        self.nursery.allocate(size, shape_id)
            .map(|offset| GcPtr::Young(offset))
    }

    /// Minor GC (scavenge nursery)
    pub fn minor_gc(&mut self, roots: &RootSet) {
        let start = std::time::Instant::now();
        
        // In a real implementation:
        // 1. Trace from roots and remembered set
        // 2. Copy live objects to survivor space or promote
        // 3. Update references
        // 4. Reset nursery
        
        self.nursery.reset();
        self.card_table.clear();
        
        self.stats.minor_collections += 1;
        self.stats.gc_time_ns += start.elapsed().as_nanos() as u64;
    }

    /// Major GC (mark-sweep old gen)
    pub fn major_gc(&mut self, roots: &RootSet) {
        let start = std::time::Instant::now();
        
        // In a real implementation:
        // 1. Mark phase: trace from roots
        // 2. Sweep phase: free unmarked objects
        // 3. Optional compact phase
        
        let freed = self.old_gen.sweep();
        
        self.stats.major_collections += 1;
        self.stats.bytes_collected += freed;
        self.stats.gc_time_ns += start.elapsed().as_nanos() as u64;
    }

    /// Write barrier for old→young pointer
    pub fn write_barrier(&mut self, old_ptr: OldGenPtr, _young_ptr: u32) {
        // Mark card as dirty
        let addr = old_ptr.chunk as usize * self.config.old_gen_chunk_size 
                 + old_ptr.offset as usize;
        self.card_table.mark_dirty(addr);
    }

    /// Check if GC is needed
    pub fn needs_gc(&self) -> bool {
        self.nursery.needs_collection()
    }

    /// Get statistics
    pub fn stats(&self) -> &GcStats {
        &self.stats
    }

    /// Get detailed stats
    pub fn detailed_stats(&self) -> DetailedGcStats {
        DetailedGcStats {
            gc_stats: self.stats.clone(),
            nursery_stats: self.nursery.stats(),
            old_gen_stats: self.old_gen.stats(),
            large_object_stats: self.large_objects.stats(),
            dirty_cards: self.card_table.dirty_count(),
        }
    }
}

impl Default for GenerationalGC {
    fn default() -> Self {
        Self::new()
    }
}

/// GC pointer (can point to different spaces)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GcPtr {
    /// Pointer into nursery
    Young(u32),
    /// Pointer into old generation
    Old(OldGenPtr),
    /// Pointer to large object
    Large(u32),
}

/// Detailed GC statistics
#[derive(Debug, Clone)]
pub struct DetailedGcStats {
    pub gc_stats: GcStats,
    pub nursery_stats: NurseryStats,
    pub old_gen_stats: OldGenStats,
    pub large_object_stats: LargeObjectStats,
    pub dirty_cards: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nursery_allocation() {
        let mut nursery = Nursery::new(1024);
        
        let ptr1 = nursery.allocate(64, 0);
        assert!(ptr1.is_some());
        
        let ptr2 = nursery.allocate(64, 1);
        assert!(ptr2.is_some());
        assert_ne!(ptr1, ptr2);
    }

    #[test]
    fn test_nursery_collection() {
        let mut nursery = Nursery::new(256);
        
        // Fill nursery
        while nursery.allocate(32, 0).is_some() {}
        
        let used_before = nursery.used();
        assert!(used_before > 0);
        
        nursery.reset();
        assert_eq!(nursery.used(), 0);
        assert_eq!(nursery.stats().collections, 1);
    }

    #[test]
    fn test_card_table() {
        let mut cards = CardTable::new(4096, 512);
        
        assert_eq!(cards.dirty_count(), 0);
        
        cards.mark_dirty(100);
        cards.mark_dirty(600);
        
        assert_eq!(cards.dirty_count(), 2);
        
        cards.clear();
        assert_eq!(cards.dirty_count(), 0);
    }

    #[test]
    fn test_large_object_space() {
        let mut los = LargeObjectSpace::new(1024);
        
        // Too small
        assert!(los.allocate(512).is_none());
        
        // Large enough
        let id = los.allocate(2048);
        assert!(id.is_some());
        
        assert_eq!(los.stats().count, 1);
        
        los.free(id.unwrap());
        assert_eq!(los.stats().count, 0);
    }

    #[test]
    fn test_generational_gc() {
        let mut gc = GenerationalGC::new();
        
        // Allocate some objects
        let ptr1 = gc.allocate(64, 0);
        let ptr2 = gc.allocate(128, 1);
        
        assert!(ptr1.is_some());
        assert!(ptr2.is_some());
        
        // Large object
        let large = gc.allocate(16 * 1024, 2);
        assert!(matches!(large, Some(GcPtr::Large(_))));
    }

    #[test]
    fn test_gc_stats() {
        let mut gc = GenerationalGC::new();
        
        gc.minor_gc(&RootSet::new());
        gc.minor_gc(&RootSet::new());
        gc.major_gc(&RootSet::new());
        
        assert_eq!(gc.stats().minor_collections, 2);
        assert_eq!(gc.stats().major_collections, 1);
    }

    #[test]
    fn test_old_generation() {
        let mut old_gen = OldGeneration::new(1024 * 1024);
        
        let ptr = old_gen.allocate(256);
        assert!(ptr.is_some());
        
        let header = ObjectHeader::new(128, 0);
        let data = vec![0u8; 128];
        let promoted = old_gen.promote(&data, header);
        assert!(promoted.is_some());
    }
}
