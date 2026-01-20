//! Idle Detection
//!
//! Detects when the browser is idle for power management.

use std::time::{Duration, Instant};

/// Idle state levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IdleState {
    /// Active user interaction
    #[default]
    Active,
    /// Short idle (< 30s)
    ShortIdle,
    /// Long idle (30s - 5min)
    LongIdle,
    /// Extended idle (> 5min)
    ExtendedIdle,
}

impl IdleState {
    /// Get minimum idle duration for this state
    pub fn min_duration(&self) -> Duration {
        match self {
            Self::Active => Duration::ZERO,
            Self::ShortIdle => Duration::from_secs(5),
            Self::LongIdle => Duration::from_secs(30),
            Self::ExtendedIdle => Duration::from_secs(300),
        }
    }
    
    /// Get power reduction factor (1.0 = no reduction)
    pub fn power_factor(&self) -> f32 {
        match self {
            Self::Active => 1.0,
            Self::ShortIdle => 0.8,
            Self::LongIdle => 0.5,
            Self::ExtendedIdle => 0.2,
        }
    }
}

/// Configuration for idle detection
#[derive(Debug, Clone)]
pub struct IdleConfig {
    /// Time since last input to consider idle
    pub input_timeout: Duration,
    /// Time since last paint to consider idle
    pub paint_timeout: Duration,
    /// Time since last animation to consider idle
    pub animation_timeout: Duration,
    /// Time since last media activity to consider idle
    pub media_timeout: Duration,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            input_timeout: Duration::from_secs(30),
            paint_timeout: Duration::from_secs(1),
            animation_timeout: Duration::from_secs(5),
            media_timeout: Duration::from_secs(0), // Media must be explicitly stopped
        }
    }
}

/// Idle detector
#[derive(Debug)]
pub struct IdleDetector {
    /// Configuration
    config: IdleConfig,
    /// Last input time
    last_input: Instant,
    /// Last paint time
    last_paint: Instant,
    /// Last animation time
    last_animation: Instant,
    /// Is media playing
    media_playing: bool,
    /// Active animations count
    active_animations: u32,
    /// Current idle state
    current_state: IdleState,
    /// Last state change time
    last_state_change: Instant,
}

impl Default for IdleDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl IdleDetector {
    /// Create a new idle detector
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            config: IdleConfig::default(),
            last_input: now,
            last_paint: now,
            last_animation: now,
            media_playing: false,
            active_animations: 0,
            current_state: IdleState::Active,
            last_state_change: now,
        }
    }
    
    /// Create with custom config
    pub fn with_config(config: IdleConfig) -> Self {
        let mut detector = Self::new();
        detector.config = config;
        detector
    }
    
    /// Record user input
    pub fn record_input(&mut self) {
        self.last_input = Instant::now();
        self.update_state();
    }
    
    /// Record paint
    pub fn record_paint(&mut self) {
        self.last_paint = Instant::now();
    }
    
    /// Record animation activity
    pub fn record_animation(&mut self) {
        self.last_animation = Instant::now();
    }
    
    /// Set animation count
    pub fn set_active_animations(&mut self, count: u32) {
        self.active_animations = count;
        if count > 0 {
            self.last_animation = Instant::now();
        }
    }
    
    /// Set media playing state
    pub fn set_media_playing(&mut self, playing: bool) {
        self.media_playing = playing;
    }
    
    /// Check if idle
    pub fn detect_idle(&self) -> bool {
        let no_input = self.last_input.elapsed() > self.config.input_timeout;
        let no_animation = !self.has_active_animations();
        let no_media = !self.is_playing_media();
        let no_visible_change = self.last_paint.elapsed() > self.config.paint_timeout;
        
        no_input && no_animation && no_media && no_visible_change
    }
    
    /// Check if there are active animations
    pub fn has_active_animations(&self) -> bool {
        self.active_animations > 0 || 
        self.last_animation.elapsed() < self.config.animation_timeout
    }
    
    /// Check if media is playing
    pub fn is_playing_media(&self) -> bool {
        self.media_playing
    }
    
    /// Get current idle state
    pub fn state(&self) -> IdleState {
        self.current_state
    }
    
    /// Update idle state
    fn update_state(&mut self) {
        let old_state = self.current_state;
        
        if !self.detect_idle() {
            self.current_state = IdleState::Active;
        } else {
            let idle_duration = self.last_input.elapsed()
                .min(self.last_paint.elapsed())
                .min(self.last_animation.elapsed());
            
            self.current_state = if idle_duration >= IdleState::ExtendedIdle.min_duration() {
                IdleState::ExtendedIdle
            } else if idle_duration >= IdleState::LongIdle.min_duration() {
                IdleState::LongIdle
            } else if idle_duration >= IdleState::ShortIdle.min_duration() {
                IdleState::ShortIdle
            } else {
                IdleState::Active
            };
        }
        
        if self.current_state != old_state {
            self.last_state_change = Instant::now();
        }
    }
    
    /// Update and get current state
    pub fn update(&mut self) -> IdleState {
        self.update_state();
        self.current_state
    }
    
    /// Get time since last input
    pub fn time_since_input(&self) -> Duration {
        self.last_input.elapsed()
    }
    
    /// Get time since last paint
    pub fn time_since_paint(&self) -> Duration {
        self.last_paint.elapsed()
    }
    
    /// Get time in current state
    pub fn time_in_state(&self) -> Duration {
        self.last_state_change.elapsed()
    }
    
    /// Get power reduction factor based on idle state
    pub fn power_factor(&self) -> f32 {
        self.current_state.power_factor()
    }
    
    /// Reset all activity timestamps (simulates wakeup)
    pub fn reset(&mut self) {
        let now = Instant::now();
        self.last_input = now;
        self.last_paint = now;
        self.last_animation = now;
        self.current_state = IdleState::Active;
        self.last_state_change = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_initial_state() {
        let detector = IdleDetector::new();
        assert_eq!(detector.state(), IdleState::Active);
        assert!(!detector.detect_idle());
    }
    
    #[test]
    fn test_idle_states() {
        assert!(IdleState::ShortIdle.min_duration() < IdleState::LongIdle.min_duration());
        assert!(IdleState::LongIdle.min_duration() < IdleState::ExtendedIdle.min_duration());
    }
    
    #[test]
    fn test_power_factors() {
        assert!(IdleState::Active.power_factor() > IdleState::ExtendedIdle.power_factor());
    }
    
    #[test]
    fn test_media_prevents_idle() {
        let mut detector = IdleDetector::new();
        detector.last_input = Instant::now() - Duration::from_secs(60);
        detector.last_paint = Instant::now() - Duration::from_secs(60);
        
        detector.set_media_playing(true);
        assert!(!detector.detect_idle());
        
        detector.set_media_playing(false);
        assert!(detector.detect_idle());
    }
    
    #[test]
    fn test_animation_prevents_idle() {
        let mut detector = IdleDetector::new();
        detector.last_input = Instant::now() - Duration::from_secs(60);
        detector.last_paint = Instant::now() - Duration::from_secs(60);
        
        detector.set_active_animations(1);
        assert!(!detector.detect_idle());
    }
    
    #[test]
    fn test_record_input_resets() {
        let mut detector = IdleDetector::new();
        detector.last_input = Instant::now() - Duration::from_secs(60);
        
        detector.record_input();
        assert!(!detector.detect_idle());
        assert_eq!(detector.state(), IdleState::Active);
    }
}
