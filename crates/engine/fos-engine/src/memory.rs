//! Memory Management
//!
//! Memory pressure monitoring, hibernation, and optimization.

use std::collections::HashMap;

/// Memory manager
#[derive(Debug)]
pub struct MemoryManager {
    pub pressure_level: PressureLevel,
    pub stats: MemoryStats,
    pub limits: MemoryLimits,
    hibernated_tabs: HashMap<u64, HibernatedTab>,
    cache_limits: CacheLimits,
}

/// Memory pressure level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PressureLevel {
    Normal,
    Moderate,
    Critical,
}

/// Memory statistics
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    pub heap_used: usize,
    pub heap_total: usize,
    pub dom_nodes: usize,
    pub layout_objects: usize,
    pub image_cache: usize,
    pub font_cache: usize,
    pub script_cache: usize,
}

/// Memory limits
#[derive(Debug, Clone)]
pub struct MemoryLimits {
    pub max_heap: usize,
    pub max_image_cache: usize,
    pub max_font_cache: usize,
    pub tab_limit: usize,
    pub hibernation_threshold: f64,
}

impl Default for MemoryLimits {
    fn default() -> Self {
        Self {
            max_heap: 512 * 1024 * 1024, // 512MB
            max_image_cache: 128 * 1024 * 1024, // 128MB
            max_font_cache: 32 * 1024 * 1024, // 32MB
            tab_limit: 10,
            hibernation_threshold: 0.7, // 70%
        }
    }
}

/// Cache limits
#[derive(Debug, Clone)]
pub struct CacheLimits {
    pub resources: usize,
    pub styles: usize,
    pub layouts: usize,
}

impl Default for CacheLimits {
    fn default() -> Self {
        Self {
            resources: 100,
            styles: 1000,
            layouts: 500,
        }
    }
}

/// Hibernated tab
#[derive(Debug, Clone)]
pub struct HibernatedTab {
    pub id: u64,
    pub url: String,
    pub scroll_position: f64,
    pub serialized_size: usize,
}

impl MemoryManager {
    pub fn new() -> Self {
        Self {
            pressure_level: PressureLevel::Normal,
            stats: MemoryStats::default(),
            limits: MemoryLimits::default(),
            hibernated_tabs: HashMap::new(),
            cache_limits: CacheLimits::default(),
        }
    }
    
    /// Update memory stats
    pub fn update_stats(&mut self, stats: MemoryStats) {
        self.stats = stats;
        self.update_pressure_level();
    }
    
    fn update_pressure_level(&mut self) {
        let usage = self.stats.heap_used as f64 / self.limits.max_heap as f64;
        
        self.pressure_level = if usage > 0.9 {
            PressureLevel::Critical
        } else if usage > self.limits.hibernation_threshold {
            PressureLevel::Moderate
        } else {
            PressureLevel::Normal
        };
    }
    
    /// Get memory usage percentage
    pub fn usage_percentage(&self) -> f64 {
        self.stats.heap_used as f64 / self.limits.max_heap as f64 * 100.0
    }
    
    /// Should hibernate tabs?
    pub fn should_hibernate(&self) -> bool {
        self.pressure_level != PressureLevel::Normal
    }
    
    /// Hibernate a tab
    pub fn hibernate_tab(&mut self, id: u64, url: &str, scroll: f64, size: usize) {
        self.hibernated_tabs.insert(id, HibernatedTab {
            id,
            url: url.to_string(),
            scroll_position: scroll,
            serialized_size: size,
        });
    }
    
    /// Wake a tab
    pub fn wake_tab(&mut self, id: u64) -> Option<HibernatedTab> {
        self.hibernated_tabs.remove(&id)
    }
    
    /// Get hibernated tabs
    pub fn get_hibernated(&self) -> Vec<&HibernatedTab> {
        self.hibernated_tabs.values().collect()
    }
    
    /// Reduce cache limits under pressure
    pub fn apply_pressure_response(&mut self) {
        match self.pressure_level {
            PressureLevel::Critical => {
                self.cache_limits.resources = 20;
                self.cache_limits.styles = 200;
                self.cache_limits.layouts = 100;
            }
            PressureLevel::Moderate => {
                self.cache_limits.resources = 50;
                self.cache_limits.styles = 500;
                self.cache_limits.layouts = 250;
            }
            PressureLevel::Normal => {
                self.cache_limits = CacheLimits::default();
            }
        }
    }
    
    /// Get total memory used
    pub fn total_used(&self) -> usize {
        self.stats.heap_used + 
        self.stats.image_cache + 
        self.stats.font_cache +
        self.stats.script_cache
    }
}

impl Default for MemoryManager {
    fn default() -> Self { Self::new() }
}

/// Resource deduplication
#[derive(Debug, Default)]
pub struct ResourceDeduplicator {
    hashes: HashMap<[u8; 32], u64>, // SHA256 -> resource ID
    refs: HashMap<u64, usize>, // resource ID -> ref count
}

impl ResourceDeduplicator {
    pub fn new() -> Self { Self::default() }
    
    /// Check if resource exists
    pub fn get(&self, hash: &[u8; 32]) -> Option<u64> {
        self.hashes.get(hash).copied()
    }
    
    /// Add resource
    pub fn add(&mut self, hash: [u8; 32], id: u64) {
        self.hashes.insert(hash, id);
        *self.refs.entry(id).or_insert(0) += 1;
    }
    
    /// Release reference
    pub fn release(&mut self, id: u64) -> bool {
        if let Some(count) = self.refs.get_mut(&id) {
            *count -= 1;
            if *count == 0 {
                self.refs.remove(&id);
                self.hashes.retain(|_, v| *v != id);
                return true; // Can free
            }
        }
        false
    }
    
    /// Get deduplication stats
    pub fn stats(&self) -> (usize, usize) {
        (self.hashes.len(), self.refs.values().sum())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_manager() {
        let mut mm = MemoryManager::new();
        
        mm.update_stats(MemoryStats {
            heap_used: 400 * 1024 * 1024,
            heap_total: 512 * 1024 * 1024,
            ..Default::default()
        });
        
        assert_eq!(mm.pressure_level, PressureLevel::Moderate);
        assert!(mm.should_hibernate());
    }
    
    #[test]
    fn test_deduplication() {
        let mut dedup = ResourceDeduplicator::new();
        let hash = [0u8; 32];
        
        dedup.add(hash, 1);
        assert_eq!(dedup.get(&hash), Some(1));
    }
}
