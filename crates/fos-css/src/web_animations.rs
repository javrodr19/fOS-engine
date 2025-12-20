//! Web Animations API
//!
//! Implementation of the Web Animations API for JavaScript access to CSS animations.

use std::collections::HashMap;
use std::sync::Arc;

/// Animation playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayState {
    #[default]
    Idle,
    Running,
    Paused,
    Finished,
}

/// Animation timeline
#[derive(Debug, Clone)]
pub struct AnimationTimeline {
    /// Current time in milliseconds
    pub current_time: Option<f64>,
    /// Timeline phase
    pub phase: TimelinePhase,
}

impl Default for AnimationTimeline {
    fn default() -> Self {
        Self {
            current_time: Some(0.0),
            phase: TimelinePhase::Active,
        }
    }
}

/// Timeline phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimelinePhase {
    Inactive,
    Before,
    #[default]
    Active,
    After,
}

/// Web Animation instance
#[derive(Debug, Clone)]
pub struct Animation {
    /// Animation ID
    pub id: String,
    /// Effect being animated
    pub effect: Option<AnimationEffect>,
    /// Animation timeline
    pub timeline: Option<AnimationTimeline>,
    /// Start time (relative to timeline)
    pub start_time: Option<f64>,
    /// Current time (local)
    pub current_time: Option<f64>,
    /// Playback rate (negative = reverse)
    pub playback_rate: f64,
    /// Play state
    pub play_state: PlayState,
    /// Ready promise resolved
    pub ready: bool,
    /// Finished promise resolved
    pub finished: bool,
    /// Pending flag
    pub pending: bool,
}

impl Default for Animation {
    fn default() -> Self {
        Self {
            id: String::new(),
            effect: None,
            timeline: Some(AnimationTimeline::default()),
            start_time: None,
            current_time: Some(0.0),
            playback_rate: 1.0,
            play_state: PlayState::Idle,
            ready: false,
            finished: false,
            pending: false,
        }
    }
}

impl Animation {
    /// Create a new animation
    pub fn new(id: &str, effect: AnimationEffect) -> Self {
        Self {
            id: id.to_string(),
            effect: Some(effect),
            ..Default::default()
        }
    }
    
    /// Play the animation
    pub fn play(&mut self) {
        if self.play_state == PlayState::Finished {
            self.current_time = Some(0.0);
        }
        self.play_state = PlayState::Running;
        self.pending = true;
        self.finished = false;
    }
    
    /// Pause the animation
    pub fn pause(&mut self) {
        if self.play_state == PlayState::Running {
            self.play_state = PlayState::Paused;
            self.pending = true;
        }
    }
    
    /// Cancel the animation
    pub fn cancel(&mut self) {
        self.play_state = PlayState::Idle;
        self.current_time = None;
        self.start_time = None;
        self.pending = false;
    }
    
    /// Finish the animation
    pub fn finish(&mut self) {
        if self.playback_rate > 0.0 {
            if let Some(effect) = &self.effect {
                self.current_time = Some(effect.computed_timing.end_time);
            }
        } else {
            self.current_time = Some(0.0);
        }
        self.play_state = PlayState::Finished;
        self.finished = true;
    }
    
    /// Reverse playback direction
    pub fn reverse(&mut self) {
        self.playback_rate = -self.playback_rate;
        if self.play_state != PlayState::Running {
            self.play();
        }
    }
    
    /// Update playback rate
    pub fn update_playback_rate(&mut self, rate: f64) {
        self.playback_rate = rate;
    }
    
    /// Advance animation by delta time
    pub fn tick(&mut self, delta_ms: f64) {
        if self.play_state != PlayState::Running {
            return;
        }
        
        if let Some(current) = self.current_time {
            let new_time = current + delta_ms * self.playback_rate;
            
            if let Some(effect) = &self.effect {
                let end = effect.computed_timing.end_time;
                
                // Check bounds
                if self.playback_rate > 0.0 && new_time >= end {
                    self.current_time = Some(end);
                    if effect.computed_timing.fill == FillMode::None {
                        self.play_state = PlayState::Finished;
                        self.finished = true;
                    }
                } else if self.playback_rate < 0.0 && new_time <= 0.0 {
                    self.current_time = Some(0.0);
                    if effect.computed_timing.fill == FillMode::None {
                        self.play_state = PlayState::Finished;
                        self.finished = true;
                    }
                } else {
                    self.current_time = Some(new_time);
                }
            } else {
                self.current_time = Some(new_time);
            }
        }
    }
    
    /// Get current progress (0.0 - 1.0)
    pub fn progress(&self) -> Option<f64> {
        if let (Some(current), Some(effect)) = (&self.current_time, &self.effect) {
            let duration = effect.computed_timing.duration;
            if duration > 0.0 {
                return Some((*current / duration).clamp(0.0, 1.0));
            }
        }
        None
    }
}

/// Animation effect (KeyframeEffect)
#[derive(Debug, Clone)]
pub struct AnimationEffect {
    /// Target element (reference ID)
    pub target: Option<String>,
    /// Keyframes
    pub keyframes: Vec<Keyframe>,
    /// Computed timing
    pub computed_timing: ComputedTiming,
}

/// Single keyframe
#[derive(Debug, Clone)]
pub struct Keyframe {
    /// Offset (0.0 - 1.0)
    pub offset: Option<f64>,
    /// Easing function
    pub easing: String,
    /// Composite operation
    pub composite: CompositeOperation,
    /// Property values
    pub properties: HashMap<String, String>,
}

impl Keyframe {
    pub fn new(offset: f64) -> Self {
        Self {
            offset: Some(offset),
            easing: "linear".to_string(),
            composite: CompositeOperation::Replace,
            properties: HashMap::new(),
        }
    }
    
    pub fn set_property(&mut self, name: &str, value: &str) {
        self.properties.insert(name.to_string(), value.to_string());
    }
}

/// Composite operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompositeOperation {
    #[default]
    Replace,
    Add,
    Accumulate,
}

/// Computed timing properties
#[derive(Debug, Clone)]
pub struct ComputedTiming {
    /// Delay before animation starts (ms)
    pub delay: f64,
    /// End delay after animation (ms)
    pub end_delay: f64,
    /// Duration of one iteration (ms)
    pub duration: f64,
    /// Number of iterations
    pub iterations: f64,
    /// Iteration start offset
    pub iteration_start: f64,
    /// Fill mode
    pub fill: FillMode,
    /// Direction
    pub direction: PlaybackDirection,
    /// Easing
    pub easing: String,
    /// Computed end time
    pub end_time: f64,
}

impl Default for ComputedTiming {
    fn default() -> Self {
        Self {
            delay: 0.0,
            end_delay: 0.0,
            duration: 0.0,
            iterations: 1.0,
            iteration_start: 0.0,
            fill: FillMode::None,
            direction: PlaybackDirection::Normal,
            easing: "linear".to_string(),
            end_time: 0.0,
        }
    }
}

impl ComputedTiming {
    /// Calculate total duration
    pub fn active_duration(&self) -> f64 {
        self.duration * self.iterations
    }
    
    /// Calculate end time
    pub fn calculate_end_time(&self) -> f64 {
        (self.delay + self.active_duration() + self.end_delay).max(0.0)
    }
}

/// Fill mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
    Auto,
}

/// Playback direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

/// Document animations (element.getAnimations())
#[derive(Debug, Default)]
pub struct DocumentAnimations {
    animations: HashMap<String, Animation>,
    next_id: u64,
}

impl DocumentAnimations {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create a new animation
    pub fn create(&mut self, effect: AnimationEffect) -> String {
        let id = format!("animation_{}", self.next_id);
        self.next_id += 1;
        
        let animation = Animation::new(&id, effect);
        self.animations.insert(id.clone(), animation);
        id
    }
    
    /// Get animation by ID
    pub fn get(&self, id: &str) -> Option<&Animation> {
        self.animations.get(id)
    }
    
    /// Get mutable animation by ID
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Animation> {
        self.animations.get_mut(id)
    }
    
    /// Get all animations
    pub fn all(&self) -> impl Iterator<Item = &Animation> {
        self.animations.values()
    }
    
    /// Tick all animations
    pub fn tick(&mut self, delta_ms: f64) {
        for animation in self.animations.values_mut() {
            animation.tick(delta_ms);
        }
    }
    
    /// Remove finished animations
    pub fn cleanup(&mut self) {
        self.animations.retain(|_, a| a.play_state != PlayState::Finished);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_animation_play() {
        let effect = AnimationEffect {
            target: Some("element".to_string()),
            keyframes: vec![],
            computed_timing: ComputedTiming {
                duration: 1000.0,
                ..Default::default()
            },
        };
        
        let mut animation = Animation::new("test", effect);
        assert_eq!(animation.play_state, PlayState::Idle);
        
        animation.play();
        assert_eq!(animation.play_state, PlayState::Running);
        
        animation.pause();
        assert_eq!(animation.play_state, PlayState::Paused);
    }
    
    #[test]
    fn test_animation_tick() {
        let effect = AnimationEffect {
            target: None,
            keyframes: vec![],
            computed_timing: ComputedTiming {
                duration: 1000.0,
                end_time: 1000.0,
                ..Default::default()
            },
        };
        
        let mut animation = Animation::new("test", effect);
        animation.play();
        animation.tick(500.0);
        
        assert_eq!(animation.current_time, Some(500.0));
        assert_eq!(animation.progress(), Some(0.5));
    }
    
    #[test]
    fn test_document_animations() {
        let mut doc = DocumentAnimations::new();
        
        let effect = AnimationEffect {
            target: None,
            keyframes: vec![],
            computed_timing: ComputedTiming::default(),
        };
        
        let id = doc.create(effect);
        assert!(doc.get(&id).is_some());
    }
}
