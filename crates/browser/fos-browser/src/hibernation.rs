//! Tab Hibernation
//!
//! Memory-efficient tab suspension for background tabs.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Tab hibernation state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HibernationState {
    Active,
    Idle,
    Hibernating,
    Hibernated,
    Restoring,
}

/// Tab snapshot for hibernation
#[derive(Debug, Clone)]
pub struct TabSnapshot {
    pub url: String,
    pub title: String,
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub form_data: Vec<(String, String)>,
    pub created_at: Instant,
    pub compressed_dom: Option<Vec<u8>>,
}

impl TabSnapshot {
    pub fn new(url: &str, title: &str) -> Self {
        Self {
            url: url.to_string(),
            title: title.to_string(),
            scroll_x: 0.0,
            scroll_y: 0.0,
            form_data: Vec::new(),
            created_at: Instant::now(),
            compressed_dom: None,
        }
    }
    
    pub fn with_scroll(mut self, x: f32, y: f32) -> Self {
        self.scroll_x = x;
        self.scroll_y = y;
        self
    }
    
    pub fn memory_size(&self) -> usize {
        self.url.len() + self.title.len() + 
        self.form_data.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>() +
        self.compressed_dom.as_ref().map(|d| d.len()).unwrap_or(0)
    }
}

/// Hibernation policy
#[derive(Debug, Clone)]
pub struct HibernationPolicy {
    pub idle_threshold: Duration,
    pub min_tab_count: usize,
    pub memory_threshold_mb: usize,
    pub exclude_audible: bool,
    pub exclude_pinned: bool,
}

impl Default for HibernationPolicy {
    fn default() -> Self {
        Self {
            idle_threshold: Duration::from_secs(300), // 5 minutes
            min_tab_count: 3,
            memory_threshold_mb: 500,
            exclude_audible: true,
            exclude_pinned: true,
        }
    }
}

/// Memory pressure level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MemoryPressure {
    None = 0,
    Moderate = 1,
    Critical = 2,
}

/// Tab hibernator
#[derive(Debug)]
pub struct TabHibernator {
    states: HashMap<u32, TabState>,
    snapshots: HashMap<u32, TabSnapshot>,
    policy: HibernationPolicy,
    memory_pressure: MemoryPressure,
    stats: HibernationStats,
}

#[derive(Debug)]
struct TabState {
    state: HibernationState,
    last_active: Instant,
    is_audible: bool,
    is_pinned: bool,
    memory_usage: usize,
}

/// Statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct HibernationStats {
    pub tabs_hibernated: u64,
    pub tabs_restored: u64,
    pub memory_freed: u64,
    pub failed_hibernations: u64,
}

impl Default for TabHibernator {
    fn default() -> Self { Self::new() }
}

impl TabHibernator {
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            snapshots: HashMap::new(),
            policy: HibernationPolicy::default(),
            memory_pressure: MemoryPressure::None,
            stats: HibernationStats::default(),
        }
    }
    
    pub fn set_policy(&mut self, policy: HibernationPolicy) { self.policy = policy; }
    pub fn set_memory_pressure(&mut self, pressure: MemoryPressure) { self.memory_pressure = pressure; }
    
    /// Register tab
    pub fn register_tab(&mut self, tab_id: u32) {
        self.states.insert(tab_id, TabState {
            state: HibernationState::Active,
            last_active: Instant::now(),
            is_audible: false,
            is_pinned: false,
            memory_usage: 0,
        });
    }
    
    /// Unregister tab
    pub fn unregister_tab(&mut self, tab_id: u32) {
        self.states.remove(&tab_id);
        self.snapshots.remove(&tab_id);
    }
    
    /// Mark tab as active
    pub fn activate_tab(&mut self, tab_id: u32) {
        if let Some(state) = self.states.get_mut(&tab_id) {
            state.last_active = Instant::now();
            state.state = HibernationState::Active;
        }
    }
    
    /// Mark tab as idle
    pub fn deactivate_tab(&mut self, tab_id: u32) {
        if let Some(state) = self.states.get_mut(&tab_id) {
            state.state = HibernationState::Idle;
        }
    }
    
    /// Set tab attributes
    pub fn set_audible(&mut self, tab_id: u32, audible: bool) {
        if let Some(state) = self.states.get_mut(&tab_id) { state.is_audible = audible; }
    }
    
    pub fn set_pinned(&mut self, tab_id: u32, pinned: bool) {
        if let Some(state) = self.states.get_mut(&tab_id) { state.is_pinned = pinned; }
    }
    
    pub fn set_memory_usage(&mut self, tab_id: u32, bytes: usize) {
        if let Some(state) = self.states.get_mut(&tab_id) { state.memory_usage = bytes; }
    }
    
    /// Get tabs eligible for hibernation
    pub fn get_hibernation_candidates(&self) -> Vec<u32> {
        if self.states.len() < self.policy.min_tab_count { return Vec::new(); }
        
        let now = Instant::now();
        let threshold = match self.memory_pressure {
            MemoryPressure::None => self.policy.idle_threshold,
            MemoryPressure::Moderate => self.policy.idle_threshold / 2,
            MemoryPressure::Critical => Duration::from_secs(30),
        };
        
        self.states.iter()
            .filter(|(_, s)| {
                s.state == HibernationState::Idle &&
                now.duration_since(s.last_active) >= threshold &&
                (!self.policy.exclude_audible || !s.is_audible) &&
                (!self.policy.exclude_pinned || !s.is_pinned)
            })
            .map(|(&id, _)| id)
            .collect()
    }
    
    /// Hibernate tab
    pub fn hibernate(&mut self, tab_id: u32, snapshot: TabSnapshot) -> Result<(), HibernateError> {
        let state = self.states.get_mut(&tab_id).ok_or(HibernateError::TabNotFound)?;
        
        if state.state == HibernationState::Hibernated {
            return Err(HibernateError::AlreadyHibernated);
        }
        
        let memory_freed = state.memory_usage as u64;
        state.state = HibernationState::Hibernated;
        self.snapshots.insert(tab_id, snapshot);
        
        self.stats.tabs_hibernated += 1;
        self.stats.memory_freed += memory_freed;
        
        Ok(())
    }
    
    /// Restore tab
    pub fn restore(&mut self, tab_id: u32) -> Result<TabSnapshot, HibernateError> {
        let state = self.states.get_mut(&tab_id).ok_or(HibernateError::TabNotFound)?;
        
        if state.state != HibernationState::Hibernated {
            return Err(HibernateError::NotHibernated);
        }
        
        state.state = HibernationState::Restoring;
        
        let snapshot = self.snapshots.remove(&tab_id).ok_or(HibernateError::NoSnapshot)?;
        
        state.state = HibernationState::Active;
        state.last_active = Instant::now();
        
        self.stats.tabs_restored += 1;
        
        Ok(snapshot)
    }
    
    /// Get state
    pub fn get_state(&self, tab_id: u32) -> Option<HibernationState> {
        self.states.get(&tab_id).map(|s| s.state)
    }
    
    /// Is hibernated
    pub fn is_hibernated(&self, tab_id: u32) -> bool {
        self.get_state(tab_id) == Some(HibernationState::Hibernated)
    }
    
    /// Get stats
    pub fn stats(&self) -> &HibernationStats { &self.stats }
    
    /// Tab count
    pub fn tab_count(&self) -> usize { self.states.len() }
    
    /// Hibernated count
    pub fn hibernated_count(&self) -> usize {
        self.states.values().filter(|s| s.state == HibernationState::Hibernated).count()
    }
}

/// Hibernation error
#[derive(Debug, Clone, Copy)]
pub enum HibernateError {
    TabNotFound,
    AlreadyHibernated,
    NotHibernated,
    NoSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_hibernation() {
        let mut hibernator = TabHibernator::new();
        
        hibernator.register_tab(1);
        hibernator.register_tab(2);
        hibernator.register_tab(3);
        hibernator.register_tab(4);
        
        hibernator.deactivate_tab(1);
        
        // Fast forward by modifying policy
        hibernator.policy.idle_threshold = Duration::ZERO;
        
        let candidates = hibernator.get_hibernation_candidates();
        assert!(candidates.contains(&1));
        
        let snapshot = TabSnapshot::new("https://example.com", "Example");
        hibernator.hibernate(1, snapshot).unwrap();
        
        assert!(hibernator.is_hibernated(1));
    }
    
    #[test]
    fn test_restore() {
        let mut hibernator = TabHibernator::new();
        hibernator.register_tab(1);
        
        let snapshot = TabSnapshot::new("https://test.com", "Test").with_scroll(0.0, 500.0);
        hibernator.states.get_mut(&1).unwrap().state = HibernationState::Hibernated;
        hibernator.snapshots.insert(1, snapshot);
        
        let restored = hibernator.restore(1).unwrap();
        assert_eq!(restored.scroll_y, 500.0);
        assert!(!hibernator.is_hibernated(1));
    }
    
    #[test]
    fn test_exclude_audible() {
        let mut hibernator = TabHibernator::new();
        hibernator.register_tab(1);
        hibernator.register_tab(2);
        hibernator.register_tab(3);
        
        hibernator.deactivate_tab(1);
        hibernator.set_audible(1, true);
        
        hibernator.policy.idle_threshold = Duration::ZERO;
        
        let candidates = hibernator.get_hibernation_candidates();
        assert!(!candidates.contains(&1));
    }
}
