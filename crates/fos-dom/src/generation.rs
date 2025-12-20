//! DOM Generation IDs (Phase 24.1)
//!
//! Each node has a generation counter that increments on any mutation.
//! If unchanged, all cached values remain valid. Enables O(1) subtree
//! validation for cache invalidation.
//!
//! # Use Cases
//! - Layout cache invalidation
//! - Style recalculation skipping
//! - Event delegation validation
//! - MutationObserver optimization

use std::sync::atomic::{AtomicU32, Ordering};

/// Generation counter - incremented on every mutation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct Generation(u32);

impl Generation {
    /// Initial generation (never mutated)
    pub const INITIAL: Self = Generation(0);
    
    /// Create a new generation
    #[inline]
    pub const fn new(value: u32) -> Self {
        Generation(value)
    }
    
    /// Get the raw value
    #[inline]
    pub const fn value(self) -> u32 {
        self.0
    }
    
    /// Get the next generation
    #[inline]
    pub const fn next(self) -> Self {
        Generation(self.0.wrapping_add(1))
    }
    
    /// Check if this generation is newer than another
    #[inline]
    pub fn is_newer_than(self, other: Self) -> bool {
        // Handle wraparound
        let diff = self.0.wrapping_sub(other.0);
        diff > 0 && diff < u32::MAX / 2
    }
}

impl Default for Generation {
    fn default() -> Self {
        Self::INITIAL
    }
}

/// Atomic generation for thread-safe updates
#[derive(Debug)]
pub struct AtomicGeneration(AtomicU32);

impl Default for AtomicGeneration {
    fn default() -> Self {
        Self::new()
    }
}

impl AtomicGeneration {
    /// Create a new atomic generation
    pub const fn new() -> Self {
        AtomicGeneration(AtomicU32::new(0))
    }
    
    /// Get current generation
    #[inline]
    pub fn get(&self) -> Generation {
        Generation(self.0.load(Ordering::Acquire))
    }
    
    /// Increment and return new generation
    #[inline]
    pub fn bump(&self) -> Generation {
        let new = self.0.fetch_add(1, Ordering::Release).wrapping_add(1);
        Generation(new)
    }
    
    /// Set to specific generation
    #[inline]
    pub fn set(&self, gen: Generation) {
        self.0.store(gen.0, Ordering::Release);
    }
    
    /// Check if this has been updated since a given generation
    #[inline]
    pub fn is_changed_since(&self, gen: Generation) -> bool {
        self.get().is_newer_than(gen)
    }
}

/// Trait for objects with generation tracking
pub trait Versioned {
    /// Get the current generation
    fn generation(&self) -> Generation;
    
    /// Bump the generation (mark as mutated)
    fn bump_generation(&mut self) -> Generation;
    
    /// Check if unchanged since a given generation
    fn is_unchanged_since(&self, gen: Generation) -> bool {
        self.generation() == gen
    }
    
    /// Check if changed since a given generation
    fn is_changed_since(&self, gen: Generation) -> bool {
        !self.is_unchanged_since(gen)
    }
}

/// Generation tracker for a node
#[derive(Debug, Clone, Copy, Default)]
pub struct NodeGeneration {
    /// This node's own generation
    own: Generation,
    /// Subtree generation (max of all descendants)
    subtree: Generation,
}

impl NodeGeneration {
    /// Create new node generation
    pub const fn new() -> Self {
        Self {
            own: Generation::INITIAL,
            subtree: Generation::INITIAL,
        }
    }
    
    /// Get own generation
    #[inline]
    pub fn own(&self) -> Generation {
        self.own
    }
    
    /// Get subtree generation
    #[inline]
    pub fn subtree(&self) -> Generation {
        self.subtree
    }
    
    /// Bump own generation
    #[inline]
    pub fn bump_own(&mut self) -> Generation {
        self.own = self.own.next();
        self.subtree = self.subtree.max(self.own);
        self.own
    }
    
    /// Update subtree generation from child
    #[inline]
    pub fn update_subtree(&mut self, child_subtree: Generation) {
        if child_subtree.is_newer_than(self.subtree) {
            self.subtree = child_subtree;
        }
    }
    
    /// Check if any child has been modified
    #[inline]
    pub fn is_subtree_changed_since(&self, gen: Generation) -> bool {
        self.subtree.is_newer_than(gen)
    }
    
    /// Memory size
    #[inline]
    pub const fn memory_size() -> usize {
        std::mem::size_of::<Self>() // 8 bytes
    }
}

/// Global generation source for a document
#[derive(Debug)]
pub struct GenerationSource {
    /// Current global generation
    current: AtomicGeneration,
    /// Statistics
    mutations: AtomicU32,
}

impl Default for GenerationSource {
    fn default() -> Self {
        Self::new()
    }
}

impl GenerationSource {
    /// Create a new generation source
    pub const fn new() -> Self {
        Self {
            current: AtomicGeneration::new(),
            mutations: AtomicU32::new(0),
        }
    }
    
    /// Get the current generation
    #[inline]
    pub fn current(&self) -> Generation {
        self.current.get()
    }
    
    /// Allocate a new generation (for a mutation)
    #[inline]
    pub fn allocate(&self) -> Generation {
        self.mutations.fetch_add(1, Ordering::Relaxed);
        self.current.bump()
    }
    
    /// Get mutation count
    #[inline]
    pub fn mutation_count(&self) -> u32 {
        self.mutations.load(Ordering::Relaxed)
    }
}

/// Cached value with generation tracking
#[derive(Debug, Clone)]
pub struct Cached<T> {
    /// The cached value
    value: T,
    /// Generation when this was computed
    generation: Generation,
}

impl<T> Cached<T> {
    /// Create a new cached value
    pub fn new(value: T, generation: Generation) -> Self {
        Self { value, generation }
    }
    
    /// Get the value if still valid
    pub fn get_if_valid(&self, current: Generation) -> Option<&T> {
        if self.generation == current {
            Some(&self.value)
        } else {
            None
        }
    }
    
    /// Get the value (even if stale)
    pub fn get(&self) -> &T {
        &self.value
    }
    
    /// Get the generation
    pub fn generation(&self) -> Generation {
        self.generation
    }
    
    /// Check if valid
    pub fn is_valid(&self, current: Generation) -> bool {
        self.generation == current
    }
    
    /// Update the cached value
    pub fn update(&mut self, value: T, generation: Generation) {
        self.value = value;
        self.generation = generation;
    }
}

impl<T: Default> Default for Cached<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            generation: Generation::INITIAL,
        }
    }
}

/// Generation-based dirty flag
#[derive(Debug, Clone, Copy)]
pub struct DirtyFlag {
    /// Last cleaned generation
    last_clean: Generation,
}

impl Default for DirtyFlag {
    fn default() -> Self {
        Self::new()
    }
}

impl DirtyFlag {
    /// Create a new dirty flag (starts dirty)
    pub const fn new() -> Self {
        Self {
            last_clean: Generation::INITIAL,
        }
    }
    
    /// Mark as clean at the given generation
    #[inline]
    pub fn mark_clean(&mut self, gen: Generation) {
        self.last_clean = gen;
    }
    
    /// Check if dirty relative to a generation
    #[inline]
    pub fn is_dirty(&self, current: Generation) -> bool {
        current.is_newer_than(self.last_clean)
    }
    
    /// Check if clean
    #[inline]
    pub fn is_clean(&self, current: Generation) -> bool {
        !self.is_dirty(current)
    }
}

/// Multi-level generation tracking (for different invalidation domains)
#[derive(Debug, Clone, Copy, Default)]
pub struct MultiGeneration {
    /// Layout generation
    pub layout: Generation,
    /// Style generation
    pub style: Generation,
    /// Paint generation
    pub paint: Generation,
    /// Animation generation
    pub animation: Generation,
}

impl MultiGeneration {
    /// Create new multi-generation tracker
    pub const fn new() -> Self {
        Self {
            layout: Generation::INITIAL,
            style: Generation::INITIAL,
            paint: Generation::INITIAL,
            animation: Generation::INITIAL,
        }
    }
    
    /// Bump all generations
    pub fn bump_all(&mut self, gen: Generation) {
        self.layout = gen;
        self.style = gen;
        self.paint = gen;
        self.animation = gen;
    }
    
    /// Check if any domain is dirty
    pub fn any_dirty(&self, current: &MultiGeneration) -> bool {
        self.layout != current.layout
            || self.style != current.style
            || self.paint != current.paint
            || self.animation != current.animation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generation_ordering() {
        let g1 = Generation::new(0);
        let g2 = Generation::new(1);
        let g3 = Generation::new(100);
        
        assert!(g2.is_newer_than(g1));
        assert!(g3.is_newer_than(g2));
        assert!(!g1.is_newer_than(g2));
    }
    
    #[test]
    fn test_generation_wraparound() {
        let almost_max = Generation::new(u32::MAX - 1);
        let max = Generation::new(u32::MAX);
        let wrapped = max.next();
        
        assert_eq!(wrapped.value(), 0);
        assert!(max.is_newer_than(almost_max));
    }
    
    #[test]
    fn test_atomic_generation() {
        let gen = AtomicGeneration::new();
        
        assert_eq!(gen.get(), Generation::INITIAL);
        
        let g1 = gen.bump();
        assert_eq!(g1, Generation::new(1));
        
        let g2 = gen.bump();
        assert_eq!(g2, Generation::new(2));
        
        assert!(gen.is_changed_since(Generation::INITIAL));
    }
    
    #[test]
    fn test_node_generation() {
        let mut ng = NodeGeneration::new();
        
        assert_eq!(ng.own(), Generation::INITIAL);
        assert_eq!(ng.subtree(), Generation::INITIAL);
        
        let g1 = ng.bump_own();
        assert_eq!(g1, Generation::new(1));
        assert_eq!(ng.subtree(), Generation::new(1));
        
        // Simulate child update
        ng.update_subtree(Generation::new(5));
        assert_eq!(ng.subtree(), Generation::new(5));
        assert!(ng.is_subtree_changed_since(Generation::new(2)));
    }
    
    #[test]
    fn test_cached_value() {
        let mut cache: Cached<i32> = Cached::new(42, Generation::new(1));
        
        assert_eq!(cache.get(), &42);
        assert!(cache.get_if_valid(Generation::new(1)).is_some());
        assert!(cache.get_if_valid(Generation::new(2)).is_none());
        
        cache.update(99, Generation::new(2));
        assert_eq!(cache.get(), &99);
        assert!(cache.is_valid(Generation::new(2)));
    }
    
    #[test]
    fn test_dirty_flag() {
        let mut flag = DirtyFlag::new();
        
        assert!(flag.is_dirty(Generation::new(1)));
        
        flag.mark_clean(Generation::new(1));
        assert!(flag.is_clean(Generation::new(1)));
        assert!(flag.is_dirty(Generation::new(2)));
    }
    
    #[test]
    fn test_generation_source() {
        let source = GenerationSource::new();
        
        let g1 = source.allocate();
        let g2 = source.allocate();
        let g3 = source.allocate();
        
        assert!(g2.is_newer_than(g1));
        assert!(g3.is_newer_than(g2));
        assert_eq!(source.mutation_count(), 3);
    }
}
