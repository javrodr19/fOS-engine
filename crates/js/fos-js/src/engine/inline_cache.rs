//! Inline Caching
//!
//! Optimizes property access by caching property lookup results.
//! When the same property is accessed repeatedly on objects with the
//! same "hidden class" (shape), we can skip the hash table lookup.

use std::collections::HashMap;
use super::integration::InternedString;

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

/// Inline cache entry for property access
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
    /// Caches indexed by bytecode offset
    caches: HashMap<u32, InlineCache>,
    /// Shape registry
    shapes: ShapeRegistry,
}

impl InlineCacheManager {
    pub fn new() -> Self { Self::default() }
    
    /// Get or create cache for bytecode offset
    pub fn get_cache(&self, offset: u32) -> Option<&InlineCache> {
        self.caches.get(&offset)
    }
    
    /// Update cache after property access
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
    }
    
    /// Get shape registry
    pub fn shapes(&self) -> &ShapeRegistry { &self.shapes }
    pub fn shapes_mut(&mut self) -> &mut ShapeRegistry { &mut self.shapes }
    
    /// Get cache stats
    pub fn stats(&self) -> InlineCacheStats {
        let mut total_hits = 0u64;
        let mut total_misses = 0u64;
        for cache in self.caches.values() {
            total_hits += cache.hits as u64;
            total_misses += cache.misses as u64;
        }
        InlineCacheStats {
            cache_count: self.caches.len(),
            total_hits,
            total_misses,
            hit_rate: if total_hits + total_misses > 0 {
                total_hits as f64 / (total_hits + total_misses) as f64
            } else { 0.0 },
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
}
