//! Tab Timer Throttling
//!
//! Throttles timers and requestAnimationFrame for background tabs.

use std::collections::HashMap;
use std::time::Duration;

/// Throttle level for tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThrottleLevel {
    /// Full speed, focused tab
    #[default]
    None,
    /// Light throttle, audible/visible tab
    Light,
    /// Heavy throttle, background tab
    Heavy,
    /// Suspended, hibernated tab
    Suspended,
}

impl ThrottleLevel {
    /// Get timer resolution for this throttle level
    pub fn timer_resolution(&self) -> Duration {
        match self {
            Self::None => Duration::from_millis(4),       // Standard 4ms minimum
            Self::Light => Duration::from_millis(100),     // Reduced for background
            Self::Heavy => Duration::from_secs(1),         // Heavily throttled
            Self::Suspended => Duration::MAX,              // No timers
        }
    }
    
    /// Get RAF (requestAnimationFrame) interval
    pub fn raf_interval(&self) -> Option<Duration> {
        match self {
            Self::None => Some(Duration::from_micros(16667)),    // ~60fps
            Self::Light => Some(Duration::from_millis(100)),      // ~10fps
            Self::Heavy => None,                                   // Paused
            Self::Suspended => None,                               // Paused
        }
    }
}

/// Tab state for throttling decisions
#[derive(Debug, Clone, Default)]
pub struct TabThrottleState {
    /// Tab is focused
    pub focused: bool,
    /// Tab is visible (in viewport)
    pub visible: bool,
    /// Tab is audible (playing audio)
    pub audible: bool,
    /// Tab has active animations
    pub has_animations: bool,
    /// Tab is hibernated
    pub hibernated: bool,
}

impl TabThrottleState {
    /// Determine throttle level from state
    pub fn throttle_level(&self) -> ThrottleLevel {
        if self.hibernated {
            ThrottleLevel::Suspended
        } else if self.focused {
            ThrottleLevel::None
        } else if self.audible || self.visible {
            ThrottleLevel::Light
        } else {
            ThrottleLevel::Heavy
        }
    }
}

/// Tab throttler for power efficiency
#[derive(Debug, Default)]
pub struct TabThrottler {
    /// Per-tab throttle states
    tabs: HashMap<u32, TabThrottleState>,
    /// Currently focused tab
    focused_tab: Option<u32>,
}

impl TabThrottler {
    /// Create a new tab throttler
    pub fn new() -> Self {
        Self {
            tabs: HashMap::new(),
            focused_tab: None,
        }
    }
    
    /// Register a tab
    pub fn register_tab(&mut self, tab_id: u32) {
        self.tabs.insert(tab_id, TabThrottleState::default());
    }
    
    /// Unregister a tab
    pub fn unregister_tab(&mut self, tab_id: u32) {
        self.tabs.remove(&tab_id);
        if self.focused_tab == Some(tab_id) {
            self.focused_tab = None;
        }
    }
    
    /// Set focused tab
    pub fn set_focused(&mut self, tab_id: u32) {
        // Unfocus previous
        if let Some(prev) = self.focused_tab {
            if let Some(state) = self.tabs.get_mut(&prev) {
                state.focused = false;
            }
        }
        
        // Focus new
        if let Some(state) = self.tabs.get_mut(&tab_id) {
            state.focused = true;
        }
        self.focused_tab = Some(tab_id);
    }
    
    /// Set tab visibility
    pub fn set_visible(&mut self, tab_id: u32, visible: bool) {
        if let Some(state) = self.tabs.get_mut(&tab_id) {
            state.visible = visible;
        }
    }
    
    /// Set tab audible state
    pub fn set_audible(&mut self, tab_id: u32, audible: bool) {
        if let Some(state) = self.tabs.get_mut(&tab_id) {
            state.audible = audible;
        }
    }
    
    /// Set tab animation state
    pub fn set_has_animations(&mut self, tab_id: u32, has_animations: bool) {
        if let Some(state) = self.tabs.get_mut(&tab_id) {
            state.has_animations = has_animations;
        }
    }
    
    /// Set tab hibernated state
    pub fn set_hibernated(&mut self, tab_id: u32, hibernated: bool) {
        if let Some(state) = self.tabs.get_mut(&tab_id) {
            state.hibernated = hibernated;
        }
    }
    
    /// Get timer throttle for a tab
    pub fn get_timer_throttle(&self, tab_id: u32) -> Duration {
        self.tabs
            .get(&tab_id)
            .map(|s| s.throttle_level().timer_resolution())
            .unwrap_or(Duration::from_millis(4))
    }
    
    /// Get RAF interval for a tab (None = paused)
    pub fn get_raf_interval(&self, tab_id: u32) -> Option<Duration> {
        self.tabs
            .get(&tab_id)
            .and_then(|s| s.throttle_level().raf_interval())
    }
    
    /// Get throttle level for a tab
    pub fn get_throttle_level(&self, tab_id: u32) -> ThrottleLevel {
        self.tabs
            .get(&tab_id)
            .map(|s| s.throttle_level())
            .unwrap_or(ThrottleLevel::None)
    }
    
    /// Check if timer should fire
    pub fn should_fire_timer(&self, tab_id: u32, elapsed: Duration) -> bool {
        elapsed >= self.get_timer_throttle(tab_id)
    }
    
    /// Check if RAF should run
    pub fn should_run_raf(&self, tab_id: u32) -> bool {
        self.get_raf_interval(tab_id).is_some()
    }
    
    /// Get count of throttled tabs
    pub fn throttled_count(&self) -> usize {
        self.tabs.values()
            .filter(|s| s.throttle_level() != ThrottleLevel::None)
            .count()
    }
    
    /// Get count of suspended tabs
    pub fn suspended_count(&self) -> usize {
        self.tabs.values()
            .filter(|s| s.throttle_level() == ThrottleLevel::Suspended)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_throttle_level_timer() {
        assert_eq!(ThrottleLevel::None.timer_resolution(), Duration::from_millis(4));
        assert_eq!(ThrottleLevel::Heavy.timer_resolution(), Duration::from_secs(1));
    }
    
    #[test]
    fn test_tab_state_focused() {
        let state = TabThrottleState {
            focused: true,
            ..Default::default()
        };
        assert_eq!(state.throttle_level(), ThrottleLevel::None);
    }
    
    #[test]
    fn test_tab_state_background() {
        let state = TabThrottleState::default();
        assert_eq!(state.throttle_level(), ThrottleLevel::Heavy);
    }
    
    #[test]
    fn test_tab_state_audible() {
        let state = TabThrottleState {
            audible: true,
            ..Default::default()
        };
        assert_eq!(state.throttle_level(), ThrottleLevel::Light);
    }
    
    #[test]
    fn test_throttler_focus() {
        let mut throttler = TabThrottler::new();
        throttler.register_tab(1);
        throttler.register_tab(2);
        
        throttler.set_focused(1);
        assert_eq!(throttler.get_throttle_level(1), ThrottleLevel::None);
        assert_eq!(throttler.get_throttle_level(2), ThrottleLevel::Heavy);
        
        throttler.set_focused(2);
        assert_eq!(throttler.get_throttle_level(1), ThrottleLevel::Heavy);
        assert_eq!(throttler.get_throttle_level(2), ThrottleLevel::None);
    }
}
