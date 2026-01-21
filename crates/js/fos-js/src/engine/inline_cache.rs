//! Inline Caching
//!
//! Optimizes property access by caching property lookup results.
//! When the same property is accessed repeatedly on objects with the
//! same "hidden class" (shape), we can skip the hash table lookup.
//!
//! ## Cache States
//! - **Uninitialized**: No cache entry yet
//! - **Monomorphic**: Single shape seen (fast path)
//! - **Polymorphic**: 2-4 shapes seen (small linear search)
//! - **Megamorphic**: Many shapes seen (fallback to slow path)

use std::collections::HashMap;
use super::integration::InternedString;

/// Maximum entries in polymorphic cache before going megamorphic
const MAX_POLYMORPHIC_ENTRIES: usize = 4;

/// Hidden class / Shape ID
/// 
/// Objects with the same set of properties in the same order
/// share the same shape. This enables inline caching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShapeId(u32);

impl ShapeId {
    pub fn new(id: u32) -> Self { Self(id) }
    pub fn id(&self) -> u32 { self.0 }
}

/// Cache entry for a single shape
#[derive(Debug, Clone, Copy)]
pub struct CacheEntry {
    /// Shape this entry is for
    pub shape: ShapeId,
    /// Cached property slot offset
    pub slot: u16,
    /// Hit count for this entry
    pub hits: u32,
}

impl CacheEntry {
    pub fn new(shape: ShapeId, slot: u16) -> Self {
        Self { shape, slot, hits: 0 }
    }
}

/// Polymorphic inline cache state
#[derive(Debug, Clone)]
pub enum PolymorphicIC {
    /// No cache entry yet
    Uninitialized,
    /// Single shape seen - fastest path
    Monomorphic {
        shape: ShapeId,
        offset: u32,
        hits: u32,
    },
    /// 2-4 shapes seen - small linear search
    Polymorphic {
        entries: [Option<CacheEntry>; MAX_POLYMORPHIC_ENTRIES],
        entry_count: u8,
    },
    /// Too many shapes - fall back to slow path
    Megamorphic {
        /// Total accesses in megamorphic state
        accesses: u32,
    },
}

impl Default for PolymorphicIC {
    fn default() -> Self {
        Self::Uninitialized
    }
}

impl PolymorphicIC {
    /// Create new uninitialized cache
    pub fn new() -> Self {
        Self::Uninitialized
    }

    /// Try to lookup in cache, returns slot offset if hit
    pub fn lookup(&mut self, shape: ShapeId) -> Option<u32> {
        match self {
            PolymorphicIC::Uninitialized => None,
            PolymorphicIC::Monomorphic { shape: cached_shape, offset, hits } => {
                if *cached_shape == shape {
                    *hits = hits.saturating_add(1);
                    Some(*offset)
                } else {
                    None
                }
            }
            PolymorphicIC::Polymorphic { entries, .. } => {
                for entry in entries.iter_mut().flatten() {
                    if entry.shape == shape {
                        entry.hits = entry.hits.saturating_add(1);
                        return Some(entry.slot as u32);
                    }
                }
                None
            }
            PolymorphicIC::Megamorphic { accesses } => {
                *accesses = accesses.saturating_add(1);
                None
            }
        }
    }

    /// Update cache with new shape/offset mapping
    pub fn update(&mut self, shape: ShapeId, slot: u16) {
        match self {
            PolymorphicIC::Uninitialized => {
                *self = PolymorphicIC::Monomorphic {
                    shape,
                    offset: slot as u32,
                    hits: 1,
                };
            }
            PolymorphicIC::Monomorphic { shape: cached_shape, offset, hits } => {
                if *cached_shape == shape {
                    *hits = hits.saturating_add(1);
                } else {
                    // Transition to polymorphic
                    let mut entries = [None; MAX_POLYMORPHIC_ENTRIES];
                    entries[0] = Some(CacheEntry {
                        shape: *cached_shape,
                        slot: *offset as u16,
                        hits: *hits,
                    });
                    entries[1] = Some(CacheEntry::new(shape, slot));
                    *self = PolymorphicIC::Polymorphic {
                        entries,
                        entry_count: 2,
                    };
                }
            }
            PolymorphicIC::Polymorphic { entries, entry_count } => {
                // Check if shape already exists
                for entry in entries.iter_mut().flatten() {
                    if entry.shape == shape {
                        entry.hits = entry.hits.saturating_add(1);
                        return;
                    }
                }
                
                // Add new entry if space
                if (*entry_count as usize) < MAX_POLYMORPHIC_ENTRIES {
                    entries[*entry_count as usize] = Some(CacheEntry::new(shape, slot));
                    *entry_count += 1;
                } else {
                    // Transition to megamorphic
                    *self = PolymorphicIC::Megamorphic { accesses: 1 };
                }
            }
            PolymorphicIC::Megamorphic { accesses } => {
                *accesses = accesses.saturating_add(1);
            }
        }
    }

    /// Check if cache is monomorphic (best for optimization)
    pub fn is_monomorphic(&self) -> bool {
        matches!(self, PolymorphicIC::Monomorphic { .. })
    }

    /// Check if cache is polymorphic
    pub fn is_polymorphic(&self) -> bool {
        matches!(self, PolymorphicIC::Polymorphic { .. })
    }

    /// Check if cache is megamorphic (should use slow path)
    pub fn is_megamorphic(&self) -> bool {
        matches!(self, PolymorphicIC::Megamorphic { .. })
    }

    /// Get total hit count
    pub fn total_hits(&self) -> u32 {
        match self {
            PolymorphicIC::Uninitialized => 0,
            PolymorphicIC::Monomorphic { hits, .. } => *hits,
            PolymorphicIC::Polymorphic { entries, .. } => {
                entries.iter().flatten().map(|e| e.hits).sum()
            }
            PolymorphicIC::Megamorphic { accesses } => *accesses,
        }
    }
}

/// Legacy inline cache entry for property access (kept for compatibility)
#[derive(Debug, Clone)]
pub struct InlineCache {
    /// Expected object shape
    pub shape: ShapeId,
    /// Cached property slot offset
    pub slot: u16,
    /// Hit count for profiling
    pub hits: u32,
    /// Miss count for invalidation
    pub misses: u32,
}

impl InlineCache {
    pub fn new(shape: ShapeId, slot: u16) -> Self {
        Self { shape, slot, hits: 0, misses: 0 }
    }
    
    /// Record a cache hit
    pub fn hit(&mut self) {
        self.hits = self.hits.saturating_add(1);
    }
    
    /// Record a cache miss
    pub fn miss(&mut self) {
        self.misses = self.misses.saturating_add(1);
    }
    
    /// Check if cache is effective (>80% hit rate)
    pub fn is_effective(&self) -> bool {
        let total = self.hits + self.misses;
        if total < 10 { return true; } // Too few samples
        (self.hits as f64 / total as f64) > 0.8
    }
}

/// Property lookup cache manager
#[derive(Debug, Default)]
pub struct InlineCacheManager {
    /// Legacy caches indexed by bytecode offset
    caches: HashMap<u32, InlineCache>,
    /// Polymorphic caches indexed by bytecode offset
    poly_caches: HashMap<u32, PolymorphicIC>,
    /// Shape registry
    shapes: ShapeRegistry,
}

impl InlineCacheManager {
    pub fn new() -> Self { Self::default() }
    
    /// Get or create cache for bytecode offset
    pub fn get_cache(&self, offset: u32) -> Option<&InlineCache> {
        self.caches.get(&offset)
    }

    /// Get polymorphic cache for offset
    pub fn get_poly_cache(&self, offset: u32) -> Option<&PolymorphicIC> {
        self.poly_caches.get(&offset)
    }

    /// Get mutable polymorphic cache, creating if needed
    pub fn get_or_create_poly_cache(&mut self, offset: u32) -> &mut PolymorphicIC {
        self.poly_caches.entry(offset).or_default()
    }

    /// Lookup in polymorphic cache
    pub fn poly_lookup(&mut self, offset: u32, shape: ShapeId) -> Option<u32> {
        self.poly_caches.get_mut(&offset).and_then(|c| c.lookup(shape))
    }

    /// Update polymorphic cache
    pub fn poly_update(&mut self, offset: u32, shape: ShapeId, slot: u16) {
        self.poly_caches.entry(offset).or_default().update(shape, slot);
    }
    
    /// Update cache after property access (legacy API)
    pub fn update_cache(&mut self, offset: u32, shape: ShapeId, slot: u16) {
        let cache = self.caches
            .entry(offset)
            .or_insert_with(|| InlineCache::new(shape, slot));
        
        if cache.shape == shape && cache.slot == slot {
            cache.hit();
        } else {
            cache.miss();
            // Update cache if new shape is more common
            if cache.misses > cache.hits {
                *cache = InlineCache::new(shape, slot);
            }
        }

        // Also update polymorphic cache
        self.poly_update(offset, shape, slot);
    }
    
    /// Get shape registry
    pub fn shapes(&self) -> &ShapeRegistry { &self.shapes }
    pub fn shapes_mut(&mut self) -> &mut ShapeRegistry { &mut self.shapes }
    
    /// Get cache stats
    pub fn stats(&self) -> InlineCacheStats {
        let mut total_hits = 0u64;
        let mut total_misses = 0u64;
        let mut mono_count = 0;
        let mut poly_count = 0;
        let mut mega_count = 0;

        for cache in self.caches.values() {
            total_hits += cache.hits as u64;
            total_misses += cache.misses as u64;
        }

        for poly_cache in self.poly_caches.values() {
            match poly_cache {
                PolymorphicIC::Monomorphic { .. } => mono_count += 1,
                PolymorphicIC::Polymorphic { .. } => poly_count += 1,
                PolymorphicIC::Megamorphic { .. } => mega_count += 1,
                _ => {}
            }
        }

        InlineCacheStats {
            cache_count: self.caches.len(),
            total_hits,
            total_misses,
            hit_rate: if total_hits + total_misses > 0 {
                total_hits as f64 / (total_hits + total_misses) as f64
            } else { 0.0 },
            monomorphic_count: mono_count,
            polymorphic_count: poly_count,
            megamorphic_count: mega_count,
        }
    }
}

/// Shape registry for hidden classes
#[derive(Debug, Default)]
pub struct ShapeRegistry {
    /// All registered shapes
    shapes: Vec<Shape>,
    /// Lookup from shape descriptor to ID
    lookup: HashMap<Vec<InternedString>, ShapeId>,
}

impl ShapeRegistry {
    pub fn new() -> Self { Self::default() }
    
    /// Get or create shape for property set
    pub fn get_shape(&mut self, properties: &[InternedString]) -> ShapeId {
        let props_vec = properties.to_vec();
        if let Some(&id) = self.lookup.get(&props_vec) {
            return id;
        }
        
        let id = ShapeId::new(self.shapes.len() as u32);
        self.shapes.push(Shape {
            id,
            properties: props_vec.clone(),
            transitions: HashMap::new(),
        });
        self.lookup.insert(props_vec, id);
        id
    }
    
    /// Get shape by ID
    pub fn get(&self, id: ShapeId) -> Option<&Shape> {
        self.shapes.get(id.0 as usize)
    }
    
    /// Get property slot in shape
    pub fn property_slot(&self, shape: ShapeId, prop: InternedString) -> Option<u16> {
        self.get(shape)
            .and_then(|s| s.properties.iter().position(|p| *p == prop))
            .map(|pos| pos as u16)
    }
    
    /// Add transition from one shape to another when property is added
    pub fn add_transition(&mut self, from: ShapeId, prop: InternedString, to: ShapeId) {
        if let Some(shape) = self.shapes.get_mut(from.0 as usize) {
            shape.transitions.insert(prop, to);
        }
    }
    
    /// Get transition for adding property
    pub fn get_transition(&self, from: ShapeId, prop: InternedString) -> Option<ShapeId> {
        self.get(from).and_then(|s| s.transitions.get(&prop).copied())
    }
}

/// Object shape (hidden class)
#[derive(Debug, Clone)]
pub struct Shape {
    id: ShapeId,
    /// Ordered list of property names
    properties: Vec<InternedString>,
    /// Transitions to other shapes when properties are added
    transitions: HashMap<InternedString, ShapeId>,
}

impl Shape {
    pub fn id(&self) -> ShapeId { self.id }
    pub fn properties(&self) -> &[InternedString] { &self.properties }
    pub fn len(&self) -> usize { self.properties.len() }
    pub fn is_empty(&self) -> bool { self.properties.is_empty() }
}

/// Inline cache statistics
#[derive(Debug, Clone)]
pub struct InlineCacheStats {
    pub cache_count: usize,
    pub total_hits: u64,
    pub total_misses: u64,
    pub hit_rate: f64,
    /// Number of monomorphic caches (single shape - optimal)
    pub monomorphic_count: usize,
    /// Number of polymorphic caches (2-4 shapes)
    pub polymorphic_count: usize,
    /// Number of megamorphic caches (many shapes - slow)
    pub megamorphic_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::integration::StringInterner;
    
    #[test]
    fn test_shape_registry() {
        let mut interner = StringInterner::new();
        let mut registry = ShapeRegistry::new();
        
        let x = interner.intern("x");
        let y = interner.intern("y");
        
        let shape1 = registry.get_shape(&[x]);
        let shape2 = registry.get_shape(&[x, y]);
        let shape1_again = registry.get_shape(&[x]);
        
        assert_eq!(shape1, shape1_again);
        assert_ne!(shape1, shape2);
    }
    
    #[test]
    fn test_property_slot() {
        let mut interner = StringInterner::new();
        let mut registry = ShapeRegistry::new();
        
        let x = interner.intern("x");
        let y = interner.intern("y");
        
        let shape = registry.get_shape(&[x, y]);
        assert_eq!(registry.property_slot(shape, x), Some(0));
        assert_eq!(registry.property_slot(shape, y), Some(1));
    }
    
    #[test]
    fn test_inline_cache() {
        let mut cache_mgr = InlineCacheManager::new();
        let shape = ShapeId::new(0);
        
        // All calls with same shape/slot increment hits
        cache_mgr.update_cache(100, shape, 0);
        cache_mgr.update_cache(100, shape, 0);
        cache_mgr.update_cache(100, shape, 0);
        cache_mgr.update_cache(100, shape, 0);
        
        let cache = cache_mgr.get_cache(100).unwrap();
        // 4 calls = 4 hits (including first call which creates matching entry)
        assert_eq!(cache.hits, 4);
        assert_eq!(cache.misses, 0);
    }

    #[test]
    fn test_polymorphic_ic_monomorphic() {
        let mut ic = PolymorphicIC::new();
        let shape = ShapeId::new(0);
        
        // First update creates monomorphic cache
        ic.update(shape, 5);
        assert!(ic.is_monomorphic());
        
        // Lookups should hit
        assert_eq!(ic.lookup(shape), Some(5));
        assert_eq!(ic.lookup(shape), Some(5));
        
        assert_eq!(ic.total_hits(), 3); // 1 from update + 2 from lookups
    }

    #[test]
    fn test_polymorphic_ic_transition_to_polymorphic() {
        let mut ic = PolymorphicIC::new();
        let shape1 = ShapeId::new(0);
        let shape2 = ShapeId::new(1);
        
        ic.update(shape1, 5);
        assert!(ic.is_monomorphic());
        
        // Different shape triggers transition
        ic.update(shape2, 10);
        assert!(ic.is_polymorphic());
        
        // Both shapes should be found
        assert_eq!(ic.lookup(shape1), Some(5));
        assert_eq!(ic.lookup(shape2), Some(10));
    }

    #[test]
    fn test_polymorphic_ic_transition_to_megamorphic() {
        let mut ic = PolymorphicIC::new();
        
        // Add 5 different shapes (exceeds MAX_POLYMORPHIC_ENTRIES = 4)
        for i in 0..5 {
            ic.update(ShapeId::new(i), i as u16);
        }
        
        assert!(ic.is_megamorphic());
        
        // Lookups return None in megamorphic state
        assert_eq!(ic.lookup(ShapeId::new(0)), None);
    }

    #[test]
    fn test_polymorphic_ic_cache_manager() {
        let mut cache_mgr = InlineCacheManager::new();
        let shape1 = ShapeId::new(0);
        let shape2 = ShapeId::new(1);
        
        // Use polymorphic API
        cache_mgr.poly_update(100, shape1, 5);
        cache_mgr.poly_update(100, shape2, 10);
        
        assert_eq!(cache_mgr.poly_lookup(100, shape1), Some(5));
        assert_eq!(cache_mgr.poly_lookup(100, shape2), Some(10));
        
        // Check stats
        let stats = cache_mgr.stats();
        assert_eq!(stats.polymorphic_count, 1);
    }
}
