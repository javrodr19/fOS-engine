//! Memory Budget
//!
//! Per-component memory limits and pressure monitoring.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Memory budget configuration
#[derive(Debug, Clone)]
pub struct MemoryBudget {
    /// Total system memory limit (bytes)
    pub total_system: usize,
    /// Per-tab limit (bytes)
    pub per_tab: usize,
    /// DOM limit per page (bytes)
    pub dom_per_page: usize,
    /// JS heap limit per page (bytes)
    pub js_heap_per_page: usize,
    /// Layout cache limit (bytes)
    pub layout_cache: usize,
    /// GPU texture limit (bytes)
    pub gpu_textures: usize,
}

impl Default for MemoryBudget {
    fn default() -> Self {
        // Conservative defaults (tighter than Chromium)
        Self {
            total_system: 512 * 1024 * 1024,     // 512MB total
            per_tab: 50 * 1024 * 1024,            // 50MB per tab
            dom_per_page: 10 * 1024 * 1024,       // 10MB DOM
            js_heap_per_page: 30 * 1024 * 1024,   // 30MB JS heap
            layout_cache: 5 * 1024 * 1024,        // 5MB layout cache
            gpu_textures: 100 * 1024 * 1024,      // 100MB GPU
        }
    }
}

impl MemoryBudget {
    /// Create with custom total limit
    pub fn with_total(total_mb: usize) -> Self {
        let total = total_mb * 1024 * 1024;
        Self {
            total_system: total,
            per_tab: total / 10,
            dom_per_page: total / 50,
            js_heap_per_page: total / 17,
            layout_cache: total / 100,
            gpu_textures: total / 5,
        }
    }
    
    /// Create minimal budget
    pub fn minimal() -> Self {
        Self {
            total_system: 128 * 1024 * 1024,     // 128MB total
            per_tab: 20 * 1024 * 1024,            // 20MB per tab
            dom_per_page: 5 * 1024 * 1024,        // 5MB DOM
            js_heap_per_page: 10 * 1024 * 1024,   // 10MB JS heap
            layout_cache: 2 * 1024 * 1024,        // 2MB layout cache
            gpu_textures: 50 * 1024 * 1024,       // 50MB GPU
        }
    }
}

/// Tab memory usage
#[derive(Debug, Clone, Copy, Default)]
pub struct TabMemoryUsage {
    /// Last activity time
    pub last_active: Option<Instant>,
    /// Total memory used
    pub total_bytes: usize,
    /// DOM memory
    pub dom_bytes: usize,
    /// JS heap memory
    pub js_heap_bytes: usize,
    /// Layout cache memory
    pub layout_bytes: usize,
}

/// Memory pressure levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryPressureLevel {
    /// Normal operation (< 50% usage)
    None = 0,
    /// Moderate pressure (50-80% usage)
    Moderate = 1,
    /// Critical pressure (> 80% usage)
    Critical = 2,
}

/// Memory pressure monitor
#[derive(Debug)]
pub struct MemoryMonitor {
    /// Memory budget
    budget: MemoryBudget,
    /// Tab memory usage
    tabs: HashMap<u32, TabMemoryUsage>,
    /// Current total usage
    current_usage: usize,
    /// Current pressure level
    pressure: MemoryPressureLevel,
    /// Idle timeout for hibernation
    idle_timeout: Duration,
}

impl MemoryMonitor {
    /// Create new monitor
    pub fn new(budget: MemoryBudget) -> Self {
        Self {
            budget,
            tabs: HashMap::new(),
            current_usage: 0,
            pressure: MemoryPressureLevel::None,
            idle_timeout: Duration::from_secs(300), // 5 minutes
        }
    }
    
    /// Get current budget
    pub fn budget(&self) -> &MemoryBudget {
        &self.budget
    }
    
    /// Set idle timeout for hibernation
    pub fn set_idle_timeout(&mut self, timeout: Duration) {
        self.idle_timeout = timeout;
    }
    
    /// Register a tab
    pub fn register_tab(&mut self, tab_id: u32) {
        self.tabs.insert(tab_id, TabMemoryUsage {
            last_active: Some(Instant::now()),
            ..Default::default()
        });
    }
    
    /// Unregister a tab
    pub fn unregister_tab(&mut self, tab_id: u32) {
        if let Some(usage) = self.tabs.remove(&tab_id) {
            self.current_usage = self.current_usage.saturating_sub(usage.total_bytes);
        }
        self.update_pressure();
    }
    
    /// Update tab memory usage
    pub fn update_tab(&mut self, tab_id: u32, usage: TabMemoryUsage) {
        if let Some(old) = self.tabs.get(&tab_id) {
            self.current_usage = self.current_usage.saturating_sub(old.total_bytes);
        }
        self.current_usage += usage.total_bytes;
        self.tabs.insert(tab_id, usage);
        self.update_pressure();
    }
    
    /// Mark tab as active
    pub fn touch_tab(&mut self, tab_id: u32) {
        if let Some(usage) = self.tabs.get_mut(&tab_id) {
            usage.last_active = Some(Instant::now());
        }
    }
    
    /// Get current memory pressure
    pub fn pressure(&self) -> MemoryPressureLevel {
        self.pressure
    }
    
    /// Get memory pressure ratio (0.0 - 1.0)
    pub fn pressure_ratio(&self) -> f64 {
        if self.budget.total_system == 0 {
            return 1.0;
        }
        self.current_usage as f64 / self.budget.total_system as f64
    }
    
    /// Update pressure level
    fn update_pressure(&mut self) {
        let ratio = self.pressure_ratio();
        self.pressure = if ratio > 0.8 {
            MemoryPressureLevel::Critical
        } else if ratio > 0.5 {
            MemoryPressureLevel::Moderate
        } else {
            MemoryPressureLevel::None
        };
    }
    
    /// Check if tab should be hibernated
    pub fn should_hibernate(&self, tab_id: u32) -> bool {
        if let Some(usage) = self.tabs.get(&tab_id) {
            // Hibernate if: idle for too long AND under memory pressure
            let is_idle = usage.last_active
                .map(|t| t.elapsed() > self.idle_timeout)
                .unwrap_or(true);
            
            is_idle && self.pressure >= MemoryPressureLevel::Moderate
        } else {
            false
        }
    }
    
    /// Get hibernation candidates (sorted by priority)
    pub fn hibernation_candidates(&self) -> Vec<u32> {
        let mut candidates: Vec<_> = self.tabs.iter()
            .filter(|(_, usage)| {
                usage.last_active
                    .map(|t| t.elapsed() > self.idle_timeout)
                    .unwrap_or(true)
            })
            .map(|(&id, usage)| (id, usage.total_bytes, usage.last_active))
            .collect();
        
        // Sort by: oldest first, then largest memory
        candidates.sort_by(|a, b| {
            match (a.2, b.2) {
                (Some(t1), Some(t2)) => t2.cmp(&t1), // Oldest first
                (None, Some(_)) => std::cmp::Ordering::Less,
                (Some(_), None) => std::cmp::Ordering::Greater,
                (None, None) => b.1.cmp(&a.1), // Largest first
            }
        });
        
        candidates.into_iter().map(|(id, _, _)| id).collect()
    }
    
    /// Get total memory usage
    pub fn total_usage(&self) -> usize {
        self.current_usage
    }
    
    /// Get remaining memory
    pub fn remaining(&self) -> usize {
        self.budget.total_system.saturating_sub(self.current_usage)
    }
    
    /// Tab count
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_memory_budget_default() {
        let budget = MemoryBudget::default();
        assert_eq!(budget.per_tab, 50 * 1024 * 1024);
    }
    
    #[test]
    fn test_memory_monitor() {
        let budget = MemoryBudget::with_total(100); // 100MB
        let mut monitor = MemoryMonitor::new(budget);
        
        monitor.register_tab(1);
        monitor.register_tab(2);
        
        assert_eq!(monitor.tab_count(), 2);
        assert_eq!(monitor.pressure(), MemoryPressureLevel::None);
    }
    
    #[test]
    fn test_pressure_levels() {
        let budget = MemoryBudget {
            total_system: 100 * 1024 * 1024,
            ..Default::default()
        };
        let mut monitor = MemoryMonitor::new(budget);
        
        monitor.register_tab(1);
        
        // 40% usage - no pressure
        monitor.update_tab(1, TabMemoryUsage {
            total_bytes: 40 * 1024 * 1024,
            ..Default::default()
        });
        assert_eq!(monitor.pressure(), MemoryPressureLevel::None);
        
        // 60% usage - moderate
        monitor.update_tab(1, TabMemoryUsage {
            total_bytes: 60 * 1024 * 1024,
            ..Default::default()
        });
        assert_eq!(monitor.pressure(), MemoryPressureLevel::Moderate);
        
        // 90% usage - critical
        monitor.update_tab(1, TabMemoryUsage {
            total_bytes: 90 * 1024 * 1024,
            ..Default::default()
        });
        assert_eq!(monitor.pressure(), MemoryPressureLevel::Critical);
    }
}
