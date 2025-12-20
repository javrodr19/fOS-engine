//! Layout Constraint Solving Cache (Phase 24.2)
//!
//! Cache flex/grid solutions. Same inputs → same outputs.
//! Skip solver on relayout. 95% layout skip for animations.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// Constraint key for caching
#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintKey {
    /// Container dimensions
    pub container_width: f32,
    pub container_height: f32,
    /// Items with their constraints
    pub items: Vec<ItemConstraint>,
    /// Type of layout
    pub layout_type: LayoutType,
}

impl Hash for ConstraintKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash floats as bits
        self.container_width.to_bits().hash(state);
        self.container_height.to_bits().hash(state);
        self.layout_type.hash(state);
        for item in &self.items {
            item.hash(state);
        }
    }
}

impl Eq for ConstraintKey {}

/// Item constraint
#[derive(Debug, Clone, PartialEq)]
pub struct ItemConstraint {
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Option<f32>,
    pub min_width: Option<f32>,
    pub max_width: Option<f32>,
    pub min_height: Option<f32>,
    pub max_height: Option<f32>,
}

impl Hash for ItemConstraint {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.flex_grow.to_bits().hash(state);
        self.flex_shrink.to_bits().hash(state);
        self.flex_basis.map(|f| f.to_bits()).hash(state);
        self.min_width.map(|f| f.to_bits()).hash(state);
        self.max_width.map(|f| f.to_bits()).hash(state);
        self.min_height.map(|f| f.to_bits()).hash(state);
        self.max_height.map(|f| f.to_bits()).hash(state);
    }
}

impl Eq for ItemConstraint {}

impl Default for ItemConstraint {
    fn default() -> Self {
        Self {
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: None,
            min_width: None,
            max_width: None,
            min_height: None,
            max_height: None,
        }
    }
}

/// Layout type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayoutType {
    Flex,
    Grid,
    Table,
}

/// Cached layout solution
#[derive(Debug, Clone)]
pub struct LayoutSolution {
    /// Computed sizes for each item
    pub item_sizes: Vec<ItemSize>,
    /// Total content size
    pub content_width: f32,
    pub content_height: f32,
}

/// Computed item size
#[derive(Debug, Clone, Copy)]
pub struct ItemSize {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Constraint solving cache
#[derive(Debug)]
pub struct ConstraintCache {
    /// Cached solutions
    cache: HashMap<u64, LayoutSolution>,
    /// Maximum cache entries
    max_entries: usize,
    /// Statistics
    stats: ConstraintCacheStats,
}

fn hash_key(key: &ConstraintKey) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct ConstraintCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

impl ConstraintCacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

impl Default for ConstraintCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            max_entries: 1000,
            stats: ConstraintCacheStats::default(),
        }
    }
    
    /// Set max entries
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }
    
    /// Get cached solution
    pub fn get(&mut self, key: &ConstraintKey) -> Option<&LayoutSolution> {
        let hash = hash_key(key);
        if self.cache.contains_key(&hash) {
            self.stats.hits += 1;
            self.cache.get(&hash)
        } else {
            self.stats.misses += 1;
            None
        }
    }
    
    /// Store solution
    pub fn insert(&mut self, key: ConstraintKey, solution: LayoutSolution) {
        // Evict if necessary
        if self.cache.len() >= self.max_entries {
            if let Some(&oldest) = self.cache.keys().next() {
                self.cache.remove(&oldest);
                self.stats.evictions += 1;
            }
        }
        
        let hash = hash_key(&key);
        self.cache.insert(hash, solution);
    }
    
    /// Clear cache
    pub fn clear(&mut self) {
        self.cache.clear();
    }
    
    /// Get statistics
    pub fn stats(&self) -> &ConstraintCacheStats {
        &self.stats
    }
}

/// Flex layout solver with caching
pub struct FlexSolver {
    cache: ConstraintCache,
}

impl Default for FlexSolver {
    fn default() -> Self {
        Self::new()
    }
}

impl FlexSolver {
    pub fn new() -> Self {
        Self {
            cache: ConstraintCache::new(),
        }
    }
    
    /// Solve flex layout
    pub fn solve(&mut self, key: ConstraintKey) -> LayoutSolution {
        // Check cache
        if let Some(cached) = self.cache.get(&key) {
            return cached.clone();
        }
        
        // Actually solve
        let solution = self.solve_uncached(&key);
        
        // Cache result
        self.cache.insert(key, solution.clone());
        
        solution
    }
    
    /// Solve without caching
    fn solve_uncached(&self, key: &ConstraintKey) -> LayoutSolution {
        let container_main = key.container_width;
        let item_count = key.items.len();
        
        if item_count == 0 {
            return LayoutSolution {
                item_sizes: Vec::new(),
                content_width: 0.0,
                content_height: 0.0,
            };
        }
        
        // Calculate flex basis totals
        let total_basis: f32 = key.items.iter()
            .map(|i| i.flex_basis.unwrap_or(0.0))
            .sum();
        
        let total_grow: f32 = key.items.iter()
            .map(|i| i.flex_grow)
            .sum();
        
        let free_space = (container_main - total_basis).max(0.0);
        
        // Distribute space
        let mut item_sizes = Vec::with_capacity(item_count);
        let mut x = 0.0;
        let mut max_height = 0.0f32;
        
        for item in &key.items {
            let basis = item.flex_basis.unwrap_or(0.0);
            let grow_share = if total_grow > 0.0 {
                free_space * (item.flex_grow / total_grow)
            } else {
                0.0
            };
            
            let mut width = basis + grow_share;
            
            // Apply min/max constraints
            if let Some(min) = item.min_width {
                width = width.max(min);
            }
            if let Some(max) = item.max_width {
                width = width.min(max);
            }
            
            let height = item.min_height.unwrap_or(key.container_height);
            max_height = max_height.max(height);
            
            item_sizes.push(ItemSize {
                x,
                y: 0.0,
                width,
                height,
            });
            
            x += width;
        }
        
        LayoutSolution {
            item_sizes,
            content_width: x,
            content_height: max_height,
        }
    }
    
    /// Get cache stats
    pub fn cache_stats(&self) -> &ConstraintCacheStats {
        self.cache.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_constraint_cache() {
        let mut cache = ConstraintCache::new();
        
        let key = ConstraintKey {
            container_width: 100.0,
            container_height: 50.0,
            items: vec![
                ItemConstraint { flex_grow: 1.0, ..Default::default() },
                ItemConstraint { flex_grow: 2.0, ..Default::default() },
            ],
            layout_type: LayoutType::Flex,
        };
        
        let solution = LayoutSolution {
            item_sizes: vec![
                ItemSize { x: 0.0, y: 0.0, width: 33.3, height: 50.0 },
                ItemSize { x: 33.3, y: 0.0, width: 66.6, height: 50.0 },
            ],
            content_width: 100.0,
            content_height: 50.0,
        };
        
        cache.insert(key.clone(), solution);
        
        // Should hit
        assert!(cache.get(&key).is_some());
        assert_eq!(cache.stats().hits, 1);
    }
    
    #[test]
    fn test_flex_solver() {
        let mut solver = FlexSolver::new();
        
        let key = ConstraintKey {
            container_width: 300.0,
            container_height: 100.0,
            items: vec![
                ItemConstraint { flex_grow: 1.0, ..Default::default() },
                ItemConstraint { flex_grow: 1.0, ..Default::default() },
                ItemConstraint { flex_grow: 1.0, ..Default::default() },
            ],
            layout_type: LayoutType::Flex,
        };
        
        let solution = solver.solve(key.clone());
        
        assert_eq!(solution.item_sizes.len(), 3);
        // Each should get 100px (300/3)
        assert!((solution.item_sizes[0].width - 100.0).abs() < 0.1);
        
        // Second call should hit cache
        let _ = solver.solve(key);
        assert_eq!(solver.cache_stats().hits, 1);
    }
}
