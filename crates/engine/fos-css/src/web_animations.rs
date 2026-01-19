//! Web Animations API
//!
//! Implementation of the Web Animations API for JavaScript access to CSS animations.
//! Uses Fixed16 for deterministic timing calculations.

use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// Fixed-Point Timing for Deterministic Animations
// ============================================================================

/// Fixed-point 16.16 number for deterministic animation timing
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
#[repr(transparent)]
pub struct Fixed16(i32);

impl Fixed16 {
    const FRAC_BITS: u32 = 16;
    const SCALE: i32 = 1 << Self::FRAC_BITS;
    
    pub const ZERO: Fixed16 = Fixed16(0);
    pub const ONE: Fixed16 = Fixed16(Self::SCALE);
    
    #[inline]
    pub const fn from_f64(value: f64) -> Self {
        Self((value * Self::SCALE as f64) as i32)
    }
    
    #[inline]
    pub const fn to_f64(self) -> f64 {
        self.0 as f64 / Self::SCALE as f64
    }
    
    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self(self.0.max(min.0).min(max.0))
    }
    
    #[inline]
    pub fn lerp(self, other: Self, t: Self) -> Self {
        let diff = (other.0 as i64 - self.0 as i64) * t.0 as i64;
        Self(self.0 + (diff >> Self::FRAC_BITS) as i32)
    }
}

impl std::ops::Add for Fixed16 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self { Self(self.0 + rhs.0) }
}

impl std::ops::Sub for Fixed16 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self { Self(self.0 - rhs.0) }
}

impl std::ops::Mul for Fixed16 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self {
        Self(((self.0 as i64 * rhs.0 as i64) >> Self::FRAC_BITS) as i32)
    }
}

impl std::ops::Div for Fixed16 {
    type Output = Self;
    fn div(self, rhs: Self) -> Self {
        if rhs.0 == 0 { return Self::ZERO; }
        Self((((self.0 as i64) << Self::FRAC_BITS) / rhs.0 as i64) as i32)
    }
}

/// Deterministic timing for animations using Fixed16
#[derive(Debug, Clone, Default)]
pub struct DeterministicTiming {
    /// Current time in fixed-point ms
    pub current_time: Fixed16,
    /// Duration in fixed-point ms
    pub duration: Fixed16,
    /// Delay in fixed-point ms
    pub delay: Fixed16,
    /// Playback rate as fixed-point
    pub playback_rate: Fixed16,
}

impl DeterministicTiming {
    pub fn new(duration_ms: f64) -> Self {
        Self {
            current_time: Fixed16::ZERO,
            duration: Fixed16::from_f64(duration_ms),
            delay: Fixed16::ZERO,
            playback_rate: Fixed16::ONE,
        }
    }
    
    /// Get progress (0.0 - 1.0) as Fixed16
    pub fn progress(&self) -> Fixed16 {
        if self.duration.0 == 0 {
            return Fixed16::ONE;
        }
        (self.current_time / self.duration).clamp(Fixed16::ZERO, Fixed16::ONE)
    }
    
    /// Advance time by delta ms
    pub fn tick(&mut self, delta_ms: f64) {
        let delta = Fixed16::from_f64(delta_ms);
        let scaled = delta * self.playback_rate;
        self.current_time = self.current_time + scaled;
    }
    
    /// Check if animation has completed
    pub fn is_complete(&self) -> bool {
        self.current_time.0 >= self.duration.0
    }
    
    /// Reset to beginning
    pub fn reset(&mut self) {
        self.current_time = Fixed16::ZERO;
    }
}

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
    /// Precomputed keyframe cache
    precomputed: HashMap<String, PrecomputedAnimation>,
}

impl DocumentAnimations {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create a new animation
    pub fn create(&mut self, effect: AnimationEffect) -> String {
        let id = format!("animation_{}", self.next_id);
        self.next_id += 1;
        
        // Check if this animation can be precomputed
        if let Some(precomputed) = KeyframePrecompute::precompute(&effect) {
            self.precomputed.insert(id.clone(), precomputed);
        }
        
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
        self.animations.retain(|id, a| {
            let keep = a.play_state != PlayState::Finished;
            if !keep {
                self.precomputed.remove(id);
            }
            keep
        });
    }
    
    /// Get precomputed value at progress (fast path)
    pub fn get_precomputed_value(
        &self,
        animation_id: &str,
        property: &str,
        progress: f64,
    ) -> Option<&PrecomputedValue> {
        let precomputed = self.precomputed.get(animation_id)?;
        let samples = precomputed.samples.get(property)?;
        
        // Find sample index (60 samples = 60fps)
        let index = ((progress * 60.0).floor() as usize).min(samples.len() - 1);
        Some(&samples[index])
    }
    
    /// Check if animation can run on compositor
    pub fn can_run_on_compositor(&self, animation_id: &str) -> bool {
        self.precomputed.get(animation_id)
            .map(|p| p.compositor_compatible)
            .unwrap_or(false)
    }
}

// ============================================================================
// Keyframe Pre-Computation (Phase 4.2)
// ============================================================================

/// Pre-computed animation values for fast lookup
#[derive(Debug, Clone)]
pub struct PrecomputedAnimation {
    /// Samples per property (60fps = 60 samples per second)
    pub samples: HashMap<String, Vec<PrecomputedValue>>,
    /// Total duration in ms
    pub duration_ms: f64,
    /// Can this run on compositor thread?
    pub compositor_compatible: bool,
}

/// Pre-computed value at a specific sample point
#[derive(Debug, Clone)]
pub enum PrecomputedValue {
    /// Numeric value with unit
    Number(f64, Unit),
    /// Transform matrix (flattened 4x4)
    Transform([f32; 16]),
    /// Color (RGBA)
    Color(u8, u8, u8, u8),
    /// Opacity (0.0 - 1.0)
    Opacity(f32),
    /// Filter value
    Filter(Box<str>),
    /// Clip path
    ClipPath(Box<str>),
    /// Generic string value
    String(Box<str>),
}

/// Unit for numeric values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Px,
    Em,
    Rem,
    Percent,
    Deg,
    Rad,
    Ms,
    S,
    None,
}

/// Keyframe pre-computation engine
pub struct KeyframePrecompute;

impl KeyframePrecompute {
    /// Pre-compute animation values at 60fps
    pub fn precompute(effect: &AnimationEffect) -> Option<PrecomputedAnimation> {
        let duration = effect.computed_timing.duration;
        if duration <= 0.0 || effect.keyframes.len() < 2 {
            return None;
        }
        
        // Calculate number of samples (60fps)
        let num_samples = ((duration / 1000.0) * 60.0).ceil() as usize;
        let num_samples = num_samples.max(2).min(3600); // Max 1 minute
        
        let mut samples: HashMap<String, Vec<PrecomputedValue>> = HashMap::new();
        let mut compositor_compatible = true;
        
        // Get all properties from keyframes
        let properties: std::collections::HashSet<&str> = effect.keyframes.iter()
            .flat_map(|kf| kf.properties.keys().map(|s| s.as_str()))
            .collect();
        
        for property in properties {
            // Check if compositor compatible
            if !is_compositor_property(property) {
                compositor_compatible = false;
            }
            
            let mut property_samples = Vec::with_capacity(num_samples);
            
            for i in 0..num_samples {
                let progress = i as f64 / (num_samples - 1) as f64;
                let value = Self::interpolate_at(&effect.keyframes, property, progress);
                property_samples.push(value);
            }
            
            samples.insert(property.to_string(), property_samples);
        }
        
        Some(PrecomputedAnimation {
            samples,
            duration_ms: duration,
            compositor_compatible,
        })
    }
    
    /// Interpolate property value at given progress
    fn interpolate_at(keyframes: &[Keyframe], property: &str, progress: f64) -> PrecomputedValue {
        // Find surrounding keyframes
        let (before, after, local_progress) = Self::find_keyframes(keyframes, property, progress);
        
        let before_value = before.and_then(|kf| kf.properties.get(property));
        let after_value = after.and_then(|kf| kf.properties.get(property));
        
        match (before_value, after_value) {
            (Some(start), Some(end)) => {
                Self::interpolate_values(start, end, local_progress)
            }
            (Some(v), None) | (None, Some(v)) => {
                Self::parse_value(v)
            }
            (None, None) => PrecomputedValue::String("".into()),
        }
    }
    
    /// Find surrounding keyframes for interpolation
    fn find_keyframes<'a>(
        keyframes: &'a [Keyframe],
        property: &str,
        progress: f64,
    ) -> (Option<&'a Keyframe>, Option<&'a Keyframe>, f64) {
        let mut before: Option<&Keyframe> = None;
        let mut after: Option<&Keyframe> = None;
        
        for kf in keyframes {
            let offset = kf.offset.unwrap_or(0.0);
            if !kf.properties.contains_key(property) {
                continue;
            }
            
            if offset <= progress {
                before = Some(kf);
            }
            if offset >= progress && after.is_none() {
                after = Some(kf);
            }
        }
        
        // Calculate local progress
        let local_progress = match (before, after) {
            (Some(b), Some(a)) => {
                let start = b.offset.unwrap_or(0.0);
                let end = a.offset.unwrap_or(1.0);
                if (end - start).abs() < 0.0001 {
                    0.0
                } else {
                    (progress - start) / (end - start)
                }
            }
            _ => progress,
        };
        
        (before, after, local_progress)
    }
    
    /// Interpolate between two values
    fn interpolate_values(start: &str, end: &str, t: f64) -> PrecomputedValue {
        // Try numeric interpolation
        if let (Some((sv, su)), Some((ev, eu))) = (parse_length(start), parse_length(end)) {
            if su == eu {
                let value = sv + (ev - sv) * t;
                return PrecomputedValue::Number(value, su);
            }
        }
        
        // Try color interpolation
        if let (Some(sc), Some(ec)) = (parse_color(start), parse_color(end)) {
            let r = lerp_u8(sc.0, ec.0, t);
            let g = lerp_u8(sc.1, ec.1, t);
            let b = lerp_u8(sc.2, ec.2, t);
            let a = lerp_u8(sc.3, ec.3, t);
            return PrecomputedValue::Color(r, g, b, a);
        }
        
        // Fallback to discrete
        if t < 0.5 {
            Self::parse_value(start)
        } else {
            Self::parse_value(end)
        }
    }
    
    /// Parse a value string
    fn parse_value(s: &str) -> PrecomputedValue {
        if let Some((v, u)) = parse_length(s) {
            return PrecomputedValue::Number(v, u);
        }
        if let Some(c) = parse_color(s) {
            return PrecomputedValue::Color(c.0, c.1, c.2, c.3);
        }
        PrecomputedValue::String(s.into())
    }
}

fn is_compositor_property(property: &str) -> bool {
    matches!(property, 
        "transform" | "opacity" | "filter" | "clip-path" |
        "translate" | "rotate" | "scale"
    )
}

fn parse_length(s: &str) -> Option<(f64, Unit)> {
    let s = s.trim();
    
    if let Some(v) = s.strip_suffix("px") {
        return v.parse().ok().map(|n| (n, Unit::Px));
    }
    if let Some(v) = s.strip_suffix("em") {
        return v.parse().ok().map(|n| (n, Unit::Em));
    }
    if let Some(v) = s.strip_suffix("rem") {
        return v.parse().ok().map(|n| (n, Unit::Rem));
    }
    if let Some(v) = s.strip_suffix('%') {
        return v.parse().ok().map(|n| (n, Unit::Percent));
    }
    if let Some(v) = s.strip_suffix("deg") {
        return v.parse().ok().map(|n| (n, Unit::Deg));
    }
    
    s.parse().ok().map(|n| (n, Unit::None))
}

fn parse_color(s: &str) -> Option<(u8, u8, u8, u8)> {
    let s = s.trim();
    
    // Hex color
    if s.starts_with('#') {
        return parse_hex_color(s);
    }
    
    // Named colors (basic)
    match s {
        "black" => Some((0, 0, 0, 255)),
        "white" => Some((255, 255, 255, 255)),
        "red" => Some((255, 0, 0, 255)),
        "green" => Some((0, 128, 0, 255)),
        "blue" => Some((0, 0, 255, 255)),
        "transparent" => Some((0, 0, 0, 0)),
        _ => None,
    }
}

fn parse_hex_color(s: &str) -> Option<(u8, u8, u8, u8)> {
    let s = s.strip_prefix('#')?;
    
    match s.len() {
        3 => {
            let r = u8::from_str_radix(&s[0..1], 16).ok()? * 17;
            let g = u8::from_str_radix(&s[1..2], 16).ok()? * 17;
            let b = u8::from_str_radix(&s[2..3], 16).ok()? * 17;
            Some((r, g, b, 255))
        }
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some((r, g, b, 255))
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            let a = u8::from_str_radix(&s[6..8], 16).ok()?;
            Some((r, g, b, a))
        }
        _ => None,
    }
}

fn lerp_u8(a: u8, b: u8, t: f64) -> u8 {
    ((a as f64) + ((b as f64) - (a as f64)) * t).round() as u8
}

// ============================================================================
// Compositor Thread Support (Phase 4.1)
// ============================================================================

/// Trait for animations that can run on the compositor thread
pub trait CompositorAnimation {
    /// Check if this animation can run on compositor
    fn is_compositor_compatible(&self) -> bool;
    
    /// Get the compositor-friendly property being animated
    fn compositor_property(&self) -> Option<CompositorProperty>;
    
    /// Sample animation at given progress without main thread
    fn sample_at(&self, progress: f64) -> Option<CompositorValue>;
}

/// Properties that can be animated on compositor thread
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorProperty {
    Transform,
    Opacity,
    Filter,
    ClipPath,
    BackdropFilter,
}

/// Value types for compositor animations
#[derive(Debug, Clone)]
pub enum CompositorValue {
    Transform([f32; 16]),
    Opacity(f32),
    Filter(Box<str>),
    ClipPath(Box<str>),
}

/// Compositor animation controller
#[derive(Debug, Default)]
pub struct CompositorAnimationController {
    /// Active compositor animations
    animations: HashMap<u32, CompositorAnimationState>,
    /// Frame budget in ms
    frame_budget_ms: f32,
}

/// State for an animation running on compositor
#[derive(Debug, Clone)]
pub struct CompositorAnimationState {
    /// Animation ID
    pub id: u32,
    /// Element ID
    pub element_id: u32,
    /// Property being animated
    pub property: CompositorProperty,
    /// Current progress (0.0 - 1.0)
    pub progress: f32,
    /// Duration in ms
    pub duration_ms: f32,
    /// Playback rate
    pub playback_rate: f32,
    /// Pre-computed samples
    pub samples: Vec<CompositorValue>,
}

impl CompositorAnimationController {
    pub fn new() -> Self {
        Self {
            animations: HashMap::new(),
            frame_budget_ms: 16.67, // 60fps
        }
    }
    
    /// Add animation to compositor
    pub fn add(&mut self, state: CompositorAnimationState) {
        self.animations.insert(state.id, state);
    }
    
    /// Remove animation from compositor
    pub fn remove(&mut self, id: u32) {
        self.animations.remove(&id);
    }
    
    /// Tick all compositor animations
    pub fn tick(&mut self, delta_ms: f32) -> Vec<(u32, CompositorValue)> {
        let mut updates = Vec::new();
        
        for (_, state) in &mut self.animations {
            let progress_delta = (delta_ms * state.playback_rate) / state.duration_ms;
            state.progress = (state.progress + progress_delta).min(1.0);
            
            // Get sample at current progress
            let sample_idx = (state.progress * state.samples.len() as f32) as usize;
            let sample_idx = sample_idx.min(state.samples.len() - 1);
            
            if let Some(value) = state.samples.get(sample_idx) {
                updates.push((state.element_id, value.clone()));
            }
        }
        
        // Remove finished animations
        self.animations.retain(|_, s| s.progress < 1.0);
        
        updates
    }
    
    /// Number of active animations
    pub fn active_count(&self) -> usize {
        self.animations.len()
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
    
    #[test]
    fn test_keyframe_precompute() {
        let mut kf1 = Keyframe::new(0.0);
        kf1.set_property("opacity", "0");
        
        let mut kf2 = Keyframe::new(1.0);
        kf2.set_property("opacity", "1");
        
        let effect = AnimationEffect {
            target: None,
            keyframes: vec![kf1, kf2],
            computed_timing: ComputedTiming {
                duration: 1000.0,
                ..Default::default()
            },
        };
        
        let precomputed = KeyframePrecompute::precompute(&effect).unwrap();
        assert!(precomputed.compositor_compatible);
        assert!(precomputed.samples.contains_key("opacity"));
    }
    
    #[test]
    fn test_compositor_controller() {
        let mut controller = CompositorAnimationController::new();
        
        let state = CompositorAnimationState {
            id: 1,
            element_id: 100,
            property: CompositorProperty::Opacity,
            progress: 0.0,
            duration_ms: 1000.0,
            playback_rate: 1.0,
            samples: vec![
                CompositorValue::Opacity(0.0),
                CompositorValue::Opacity(0.5),
                CompositorValue::Opacity(1.0),
            ],
        };
        
        controller.add(state);
        assert_eq!(controller.active_count(), 1);
        
        let updates = controller.tick(500.0);
        assert!(!updates.is_empty());
    }
}

