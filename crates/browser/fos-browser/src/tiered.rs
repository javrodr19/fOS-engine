//! Tiered Memory Integration
//!
//! Hot/Warm/Cold memory tiers for DOM content management.

use std::collections::HashMap;
use std::time::Instant;

/// Memory tier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// Current viewport - fastest access
    Hot,
    /// ±2 screens - possibly compressed
    Warm,
    /// Rest of document - on disk or dropped
    Cold,
}

/// Tiered memory manager for the browser
pub struct TieredMemoryManager {
    /// Data by ID
    data: HashMap<u64, TieredEntry>,
    /// Viewport Y position
    viewport_y: f32,
    /// Viewport height
    viewport_height: f32,
    /// Stats
    stats: TieredStats,
}

#[derive(Debug)]
struct TieredEntry {
    data: Vec<u8>,
    tier: Tier,
    y: f32,
    height: f32,
    last_access: Instant,
}

/// Tiered memory statistics
#[derive(Debug, Clone, Default)]
pub struct TieredStats {
    pub hot_count: usize,
    pub warm_count: usize,
    pub cold_count: usize,
    pub hot_bytes: usize,
    pub warm_bytes: usize,
    pub total_bytes: usize,
}

impl TieredMemoryManager {
    /// Create new tiered memory manager
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
            viewport_y: 0.0,
            viewport_height: 1000.0,
            stats: TieredStats::default(),
        }
    }
    
    /// Insert data for a node
    pub fn insert(&mut self, id: u64, data: Vec<u8>, y: f32, height: f32) {
        let tier = self.calculate_tier(y, height);
        let entry = TieredEntry {
            data,
            tier,
            y,
            height,
            last_access: Instant::now(),
        };
        self.data.insert(id, entry);
        self.update_stats();
    }
    
    /// Get data for a node
    pub fn get(&mut self, id: u64) -> Option<&[u8]> {
        if let Some(entry) = self.data.get_mut(&id) {
            entry.last_access = Instant::now();
            Some(&entry.data)
        } else {
            None
        }
    }
    
    /// Update viewport position
    pub fn update_viewport(&mut self, y: f32, height: f32) {
        self.viewport_y = y;
        self.viewport_height = height;
        self.migrate_tiers();
    }
    
    /// Calculate tier for a position
    fn calculate_tier(&self, y: f32, height: f32) -> Tier {
        let vp_top = self.viewport_y;
        let vp_bottom = self.viewport_y + self.viewport_height;
        let warm_distance = self.viewport_height * 2.0;
        
        // In viewport = hot
        if y + height >= vp_top && y <= vp_bottom {
            return Tier::Hot;
        }
        
        // Near viewport = warm
        if y + height >= vp_top - warm_distance && y <= vp_bottom + warm_distance {
            return Tier::Warm;
        }
        
        Tier::Cold
    }
    
    /// Migrate entries between tiers
    fn migrate_tiers(&mut self) {
        // Collect new tiers first
        let updates: Vec<_> = self.data.iter()
            .map(|(&id, entry)| (id, self.calculate_tier(entry.y, entry.height)))
            .collect();
        
        // Then apply them
        for (id, new_tier) in updates {
            if let Some(entry) = self.data.get_mut(&id) {
                entry.tier = new_tier;
            }
        }
        self.update_stats();
    }
    
    fn update_stats(&mut self) {
        let mut stats = TieredStats::default();
        
        for entry in self.data.values() {
            let size = entry.data.len();
            stats.total_bytes += size;
            
            match entry.tier {
                Tier::Hot => {
                    stats.hot_count += 1;
                    stats.hot_bytes += size;
                }
                Tier::Warm => {
                    stats.warm_count += 1;
                    stats.warm_bytes += size;
                }
                Tier::Cold => {
                    stats.cold_count += 1;
                }
            }
        }
        
        self.stats = stats;
    }
    
    /// Get statistics
    pub fn stats(&self) -> &TieredStats {
        &self.stats
    }
    
    /// Get node count
    pub fn len(&self) -> usize {
        self.data.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

impl Default for TieredMemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_tiered_memory_creation() {
        let manager = TieredMemoryManager::new();
        assert!(manager.is_empty());
    }
    
    #[test]
    fn test_insert_and_get() {
        let mut manager = TieredMemoryManager::new();
        manager.insert(1, vec![1, 2, 3], 100.0, 50.0);
        
        assert_eq!(manager.get(1), Some([1, 2, 3].as_slice()));
    }
    
    #[test]
    fn test_tier_calculation() {
        let mut manager = TieredMemoryManager::new();
        manager.update_viewport(500.0, 1000.0);
        
        // In viewport = hot
        manager.insert(1, vec![1], 600.0, 100.0);
        assert_eq!(manager.stats().hot_count, 1);
        
        // Far away = cold
        manager.insert(2, vec![2], 5000.0, 100.0);
        assert_eq!(manager.stats().cold_count, 1);
    }
}
