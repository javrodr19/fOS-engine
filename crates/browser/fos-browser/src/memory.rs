//! Memory Integration
//!
//! Integrates fos-engine memory management: pressure monitoring, tab hibernation, resource deduplication.

use fos_engine::{
    MemoryManager, MemoryStats, PressureLevel, Arena,
};

/// Memory integration for the browser
pub struct MemoryIntegration {
    /// Core memory manager
    pub manager: MemoryManager,
    /// Tab data arena
    tab_arena: Arena<TabSnapshot>,
    /// Resource deduplication enabled
    pub dedup_enabled: bool,
    /// Last pressure check
    last_check: std::time::Instant,
}

/// Snapshot of tab state for hibernation
#[derive(Debug, Clone)]
pub struct TabSnapshot {
    pub tab_id: u64,
    pub url: String,
    pub title: String,
    pub scroll_y: f64,
    pub form_data: Vec<(String, String)>,
}

impl MemoryIntegration {
    /// Create new memory integration
    pub fn new() -> Self {
        Self {
            manager: MemoryManager::new(),
            tab_arena: Arena::new(),
            dedup_enabled: true,
            last_check: std::time::Instant::now(),
        }
    }
    
    /// Update memory statistics
    pub fn update_stats(&mut self, 
        heap_used: usize, 
        dom_nodes: usize, 
        layout_objects: usize
    ) {
        self.manager.update_stats(MemoryStats {
            heap_used,
            heap_total: self.manager.limits.max_heap,
            dom_nodes,
            layout_objects,
            image_cache: 0,
            font_cache: 0,
            script_cache: 0,
        });
    }
    
    /// Get current pressure level
    pub fn pressure_level(&self) -> PressureLevel {
        self.manager.pressure_level
    }
    
    /// Check if under memory pressure
    pub fn is_under_pressure(&self) -> bool {
        self.manager.should_hibernate()
    }
    
    /// Get memory usage percentage
    pub fn usage_percent(&self) -> f64 {
        self.manager.usage_percentage()
    }
    
    /// Should check for pressure?
    pub fn should_check_pressure(&self) -> bool {
        self.last_check.elapsed() > std::time::Duration::from_secs(30)
    }
    
    /// Mark pressure checked
    pub fn mark_pressure_checked(&mut self) {
        self.last_check = std::time::Instant::now();
    }
    
    /// Hibernate a tab
    pub fn hibernate_tab(&mut self, tab_id: u64, url: &str, title: &str, scroll_y: f64) -> usize {
        let snapshot = TabSnapshot {
            tab_id,
            url: url.to_string(),
            title: title.to_string(),
            scroll_y,
            form_data: Vec::new(),
        };
        
        let arena_id = self.tab_arena.alloc(snapshot);
        self.manager.hibernate_tab(tab_id, url, scroll_y, 0);
        
        log::info!("Hibernated tab {} ({})", tab_id, url);
        arena_id
    }
    
    /// Wake a tab from hibernation
    pub fn wake_tab(&mut self, tab_id: u64) -> Option<(String, f64)> {
        if let Some(hibernated) = self.manager.wake_tab(tab_id) {
            log::info!("Woke tab {} ({})", tab_id, hibernated.url);
            Some((hibernated.url, hibernated.scroll_position))
        } else {
            None
        }
    }
    
    /// Get hibernated tab count
    pub fn hibernated_count(&self) -> usize {
        self.manager.get_hibernated().len()
    }
    
    /// Apply memory pressure response
    pub fn apply_pressure_response(&mut self) {
        let level = self.manager.pressure_level;
        self.manager.apply_pressure_response();
        
        match level {
            PressureLevel::Critical => {
                log::warn!("Critical memory pressure - reducing caches");
            }
            PressureLevel::Moderate => {
                log::info!("Moderate memory pressure - reducing caches");
            }
            PressureLevel::Normal => {}
        }
    }
    
    /// Get memory breakdown
    pub fn memory_breakdown(&self) -> MemoryBreakdown {
        let stats = &self.manager.stats;
        MemoryBreakdown {
            heap_used: stats.heap_used,
            heap_total: stats.heap_total,
            dom_nodes: stats.dom_nodes,
            layout_objects: stats.layout_objects,
            hibernated_tabs: self.hibernated_count(),
            usage_percent: self.usage_percent(),
            pressure_level: self.pressure_level(),
        }
    }
    
    /// Get stats for logging
    pub fn log_stats(&self) -> String {
        format!(
            "Memory: {:.1}% ({}/{} MB), {} hibernated",
            self.usage_percent(),
            self.manager.stats.heap_used / 1024 / 1024,
            self.manager.limits.max_heap / 1024 / 1024,
            self.hibernated_count()
        )
    }
}

impl Default for MemoryIntegration {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory breakdown for display
#[derive(Debug, Clone)]
pub struct MemoryBreakdown {
    pub heap_used: usize,
    pub heap_total: usize,
    pub dom_nodes: usize,
    pub layout_objects: usize,
    pub hibernated_tabs: usize,
    pub usage_percent: f64,
    pub pressure_level: PressureLevel,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_integration_creation() {
        let mem = MemoryIntegration::new();
        assert_eq!(mem.pressure_level(), PressureLevel::Normal);
        assert!(!mem.is_under_pressure());
    }
    
    #[test]
    fn test_memory_pressure() {
        let mut mem = MemoryIntegration::new();
        
        // Simulate high memory usage (80%)
        let high_usage = (mem.manager.limits.max_heap as f64 * 0.8) as usize;
        mem.update_stats(high_usage, 1000, 500);
        
        assert_eq!(mem.pressure_level(), PressureLevel::Moderate);
        assert!(mem.is_under_pressure());
    }
    
    #[test]
    fn test_tab_hibernation() {
        let mut mem = MemoryIntegration::new();
        
        let _arena_id = mem.hibernate_tab(1, "https://example.com", "Example", 100.0);
        assert_eq!(mem.hibernated_count(), 1);
        
        let result = mem.wake_tab(1);
        assert!(result.is_some());
        assert_eq!(mem.hibernated_count(), 0);
    }
}
