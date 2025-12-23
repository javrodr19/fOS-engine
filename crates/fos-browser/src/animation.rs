//! Web Animations API
//!
//! CSS transitions and animations via JavaScript.

use std::collections::HashMap;

/// Animation playback state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AnimationPlayState {
    #[default]
    Idle,
    Running,
    Paused,
    Finished,
}

/// Animation fill mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
    Auto,
}

/// Animation direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlaybackDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

/// Easing function
#[derive(Debug, Clone, PartialEq)]
pub enum Easing {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    CubicBezier(f32, f32, f32, f32),
    Steps(u32, StepPosition),
}

impl Default for Easing {
    fn default() -> Self {
        Self::Linear
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepPosition {
    #[default]
    End,
    Start,
    JumpNone,
    JumpBoth,
}

/// Keyframe
#[derive(Debug, Clone)]
pub struct Keyframe {
    pub offset: Option<f32>,
    pub easing: Easing,
    pub composite: CompositeOperation,
    pub properties: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompositeOperation {
    #[default]
    Replace,
    Add,
    Accumulate,
}

/// Animation timing options
#[derive(Debug, Clone)]
pub struct AnimationTiming {
    pub duration: f64,
    pub delay: f64,
    pub end_delay: f64,
    pub iterations: f64,
    pub direction: PlaybackDirection,
    pub fill: FillMode,
    pub easing: Easing,
}

impl Default for AnimationTiming {
    fn default() -> Self {
        Self {
            duration: 0.0,
            delay: 0.0,
            end_delay: 0.0,
            iterations: 1.0,
            direction: PlaybackDirection::Normal,
            fill: FillMode::None,
            easing: Easing::Linear,
        }
    }
}

/// Web animation
#[derive(Debug)]
pub struct Animation {
    pub id: u64,
    pub target: u64, // Element ID
    pub keyframes: Vec<Keyframe>,
    pub timing: AnimationTiming,
    pub play_state: AnimationPlayState,
    pub current_time: f64,
    pub start_time: Option<f64>,
    pub playback_rate: f64,
}

static mut NEXT_ANIMATION_ID: u64 = 1;

impl Animation {
    pub fn new(target: u64, keyframes: Vec<Keyframe>, timing: AnimationTiming) -> Self {
        let id = unsafe {
            let id = NEXT_ANIMATION_ID;
            NEXT_ANIMATION_ID += 1;
            id
        };
        Self {
            id,
            target,
            keyframes,
            timing,
            play_state: AnimationPlayState::Idle,
            current_time: 0.0,
            start_time: None,
            playback_rate: 1.0,
        }
    }
    
    /// Play the animation
    pub fn play(&mut self, now: f64) {
        if self.start_time.is_none() {
            self.start_time = Some(now);
        }
        self.play_state = AnimationPlayState::Running;
    }
    
    /// Pause the animation
    pub fn pause(&mut self) {
        self.play_state = AnimationPlayState::Paused;
    }
    
    /// Cancel the animation
    pub fn cancel(&mut self) {
        self.play_state = AnimationPlayState::Idle;
        self.current_time = 0.0;
        self.start_time = None;
    }
    
    /// Finish the animation
    pub fn finish(&mut self) {
        self.play_state = AnimationPlayState::Finished;
        self.current_time = self.timing.duration * self.timing.iterations;
    }
    
    /// Reverse playback direction
    pub fn reverse(&mut self) {
        self.playback_rate = -self.playback_rate;
    }
    
    /// Update animation time
    pub fn update(&mut self, now: f64) -> Option<HashMap<String, String>> {
        if self.play_state != AnimationPlayState::Running {
            return None;
        }
        
        let elapsed = now - self.start_time.unwrap_or(now) - self.timing.delay;
        if elapsed < 0.0 {
            return None; // Still in delay
        }
        
        self.current_time = elapsed * self.playback_rate;
        
        let total_duration = self.timing.duration * self.timing.iterations;
        if self.current_time >= total_duration {
            self.play_state = AnimationPlayState::Finished;
            self.current_time = total_duration;
        }
        
        // Calculate progress
        let iteration_progress = if self.timing.duration > 0.0 {
            (self.current_time % self.timing.duration) / self.timing.duration
        } else {
            1.0
        };
        
        // Apply direction
        let directed_progress = match self.timing.direction {
            PlaybackDirection::Normal => iteration_progress,
            PlaybackDirection::Reverse => 1.0 - iteration_progress,
            PlaybackDirection::Alternate => {
                let iteration = (self.current_time / self.timing.duration) as u32;
                if iteration % 2 == 0 { iteration_progress } else { 1.0 - iteration_progress }
            }
            PlaybackDirection::AlternateReverse => {
                let iteration = (self.current_time / self.timing.duration) as u32;
                if iteration % 2 == 0 { 1.0 - iteration_progress } else { iteration_progress }
            }
        };
        
        // Interpolate keyframes
        Some(self.interpolate(directed_progress))
    }
    
    fn interpolate(&self, progress: f64) -> HashMap<String, String> {
        let mut result = HashMap::new();
        
        if self.keyframes.len() < 2 {
            if let Some(kf) = self.keyframes.first() {
                return kf.properties.clone();
            }
            return result;
        }
        
        // Find surrounding keyframes
        let progress = progress as f32;
        let mut from_idx = 0;
        let mut to_idx = 1;
        
        for (i, kf) in self.keyframes.iter().enumerate() {
            let offset = kf.offset.unwrap_or(i as f32 / (self.keyframes.len() - 1) as f32);
            if offset <= progress {
                from_idx = i;
            } else {
                to_idx = i;
                break;
            }
        }
        
        let from = &self.keyframes[from_idx];
        let to = &self.keyframes[to_idx.min(self.keyframes.len() - 1)];
        
        // Simple string interpolation (real impl would parse values)
        let from_offset = from.offset.unwrap_or(from_idx as f32 / (self.keyframes.len() - 1) as f32);
        let to_offset = to.offset.unwrap_or(to_idx as f32 / (self.keyframes.len() - 1) as f32);
        let local_progress = if (to_offset - from_offset).abs() > 0.001 {
            (progress - from_offset) / (to_offset - from_offset)
        } else {
            1.0
        };
        
        for (prop, _) in &from.properties {
            // Use 'to' value at end, 'from' at start
            if local_progress >= 0.5 {
                if let Some(v) = to.properties.get(prop) {
                    result.insert(prop.clone(), v.clone());
                }
            } else if let Some(v) = from.properties.get(prop) {
                result.insert(prop.clone(), v.clone());
            }
        }
        
        result
    }
}

/// Animation manager
#[derive(Debug, Default)]
pub struct AnimationManager {
    animations: Vec<Animation>,
}

impl AnimationManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create and start animation
    pub fn animate(&mut self, target: u64, keyframes: Vec<Keyframe>, timing: AnimationTiming, now: f64) -> u64 {
        let mut anim = Animation::new(target, keyframes, timing);
        let id = anim.id;
        anim.play(now);
        self.animations.push(anim);
        id
    }
    
    /// Get animation
    pub fn get(&mut self, id: u64) -> Option<&mut Animation> {
        self.animations.iter_mut().find(|a| a.id == id)
    }
    
    /// Remove animation
    pub fn remove(&mut self, id: u64) {
        self.animations.retain(|a| a.id != id);
    }
    
    /// Update all animations
    pub fn update(&mut self, now: f64) -> Vec<(u64, HashMap<String, String>)> {
        let mut results = Vec::new();
        
        for anim in &mut self.animations {
            if let Some(styles) = anim.update(now) {
                results.push((anim.target, styles));
            }
        }
        
        // Remove finished animations
        self.animations.retain(|a| a.play_state != AnimationPlayState::Finished);
        
        results
    }
    
    /// Get animations for element
    pub fn get_animations(&self, target: u64) -> Vec<&Animation> {
        self.animations.iter().filter(|a| a.target == target).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_animation() {
        let mut mgr = AnimationManager::new();
        
        let keyframes = vec![
            Keyframe {
                offset: Some(0.0),
                easing: Easing::Linear,
                composite: CompositeOperation::Replace,
                properties: {
                    let mut h = HashMap::new();
                    h.insert("opacity".to_string(), "0".to_string());
                    h
                },
            },
            Keyframe {
                offset: Some(1.0),
                easing: Easing::Linear,
                composite: CompositeOperation::Replace,
                properties: {
                    let mut h = HashMap::new();
                    h.insert("opacity".to_string(), "1".to_string());
                    h
                },
            },
        ];
        
        let timing = AnimationTiming {
            duration: 1000.0,
            ..Default::default()
        };
        
        let id = mgr.animate(1, keyframes, timing, 0.0);
        assert!(mgr.get(id).is_some());
    }
}
