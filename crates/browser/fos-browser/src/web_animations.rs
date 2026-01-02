//! Web Animations API with Fixed-Point Timing
//!
//! Uses Fixed16 (16.16 fixed-point) for deterministic, cross-platform
//! timing calculations that produce identical results regardless of
//! floating-point implementation differences.

use std::collections::HashMap;
use fos_engine::fixed_point::Fixed16;

/// Fixed-point time (milliseconds in 16.16 format)
pub type FixedTime = Fixed16;

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

/// Easing function with fixed-point support
#[derive(Debug, Clone, PartialEq)]
pub enum Easing {
    Linear,
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Cubic bezier with fixed-point control points
    CubicBezier(Fixed16, Fixed16, Fixed16, Fixed16),
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

impl Easing {
    /// Apply easing function to progress (0-1 fixed-point)
    pub fn apply(&self, t: Fixed16) -> Fixed16 {
        match self {
            Easing::Linear => t,
            Easing::Ease => Self::cubic_bezier_fixed(
                Fixed16::from_f32(0.25),
                Fixed16::from_f32(0.1),
                Fixed16::from_f32(0.25),
                Fixed16::ONE,
                t,
            ),
            Easing::EaseIn => Self::cubic_bezier_fixed(
                Fixed16::from_f32(0.42),
                Fixed16::ZERO,
                Fixed16::ONE,
                Fixed16::ONE,
                t,
            ),
            Easing::EaseOut => Self::cubic_bezier_fixed(
                Fixed16::ZERO,
                Fixed16::ZERO,
                Fixed16::from_f32(0.58),
                Fixed16::ONE,
                t,
            ),
            Easing::EaseInOut => Self::cubic_bezier_fixed(
                Fixed16::from_f32(0.42),
                Fixed16::ZERO,
                Fixed16::from_f32(0.58),
                Fixed16::ONE,
                t,
            ),
            Easing::CubicBezier(x1, y1, x2, y2) => {
                Self::cubic_bezier_fixed(*x1, *y1, *x2, *y2, t)
            }
            Easing::Steps(steps, pos) => Self::steps_fixed(*steps, *pos, t),
        }
    }

    /// Fixed-point cubic bezier approximation
    fn cubic_bezier_fixed(x1: Fixed16, y1: Fixed16, x2: Fixed16, y2: Fixed16, t: Fixed16) -> Fixed16 {
        // Simplified cubic bezier using fixed-point
        // B(t) = (1-t)³P₀ + 3(1-t)²tP₁ + 3(1-t)t²P₂ + t³P₃
        let one = Fixed16::ONE;
        let one_minus_t = one - t;
        let one_minus_t_sq = one_minus_t * one_minus_t;
        let one_minus_t_cu = one_minus_t_sq * one_minus_t;
        let t_sq = t * t;
        let t_cu = t_sq * t;
        let three = Fixed16::from_i32(3);

        // Calculate Y value (we're approximating x ≈ t for simplicity)
        let term1 = one_minus_t_cu * Fixed16::ZERO; // P0 = 0
        let term2 = three * one_minus_t_sq * t * y1;
        let term3 = three * one_minus_t * t_sq * y2;
        let term4 = t_cu * one; // P3 = 1

        term1 + term2 + term3 + term4
    }

    /// Fixed-point steps function
    fn steps_fixed(steps: u32, pos: StepPosition, t: Fixed16) -> Fixed16 {
        if steps == 0 {
            return t;
        }

        let steps_fixed = Fixed16::from_i32(steps as i32);
        let step_size = Fixed16::ONE / steps_fixed;

        let current_step = (t / step_size).to_i32() as u32;
        let current_step = current_step.min(steps - 1);

        match pos {
            StepPosition::Start => Fixed16::from_i32((current_step + 1) as i32) * step_size,
            StepPosition::End => Fixed16::from_i32(current_step as i32) * step_size,
            StepPosition::JumpNone => {
                if current_step == 0 {
                    Fixed16::ZERO
                } else {
                    Fixed16::from_i32(current_step as i32) * step_size
                }
            }
            StepPosition::JumpBoth => {
                Fixed16::from_i32((current_step + 1) as i32) / Fixed16::from_i32((steps + 1) as i32)
            }
        }
    }
}

/// Keyframe with fixed-point offset
#[derive(Debug, Clone)]
pub struct Keyframe {
    /// Offset (0.0-1.0) as fixed-point
    pub offset: Option<Fixed16>,
    pub easing: Easing,
    pub composite: CompositeOperation,
    pub properties: HashMap<String, AnimatedValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompositeOperation {
    #[default]
    Replace,
    Add,
    Accumulate,
}

/// Animated value with fixed-point precision
#[derive(Debug, Clone)]
pub enum AnimatedValue {
    /// Numeric value (e.g., opacity, scale)
    Number(Fixed16),
    /// Length with unit
    Length(Fixed16, LengthUnit),
    /// Color (RGBA, each 0-255)
    Color(u8, u8, u8, u8),
    /// Transform components
    Transform(Vec<TransformFunction>),
    /// String (for discrete properties)
    Discrete(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthUnit {
    Px,
    Em,
    Rem,
    Percent,
    Vw,
    Vh,
}

#[derive(Debug, Clone)]
pub enum TransformFunction {
    TranslateX(Fixed16),
    TranslateY(Fixed16),
    Scale(Fixed16),
    ScaleX(Fixed16),
    ScaleY(Fixed16),
    Rotate(Fixed16), // Degrees
    SkewX(Fixed16),
    SkewY(Fixed16),
}

impl AnimatedValue {
    /// Interpolate between two values
    pub fn interpolate(&self, to: &AnimatedValue, t: Fixed16) -> AnimatedValue {
        match (self, to) {
            (AnimatedValue::Number(a), AnimatedValue::Number(b)) => {
                AnimatedValue::Number(Self::lerp_fixed(*a, *b, t))
            }
            (AnimatedValue::Length(a, unit), AnimatedValue::Length(b, _)) => {
                AnimatedValue::Length(Self::lerp_fixed(*a, *b, t), *unit)
            }
            (AnimatedValue::Color(r1, g1, b1, a1), AnimatedValue::Color(r2, g2, b2, a2)) => {
                AnimatedValue::Color(
                    Self::lerp_u8(*r1, *r2, t),
                    Self::lerp_u8(*g1, *g2, t),
                    Self::lerp_u8(*b1, *b2, t),
                    Self::lerp_u8(*a1, *a2, t),
                )
            }
            // For discrete values, switch at 50%
            (a, b) => {
                if t >= Fixed16::from_f32(0.5) {
                    b.clone()
                } else {
                    a.clone()
                }
            }
        }
    }

    fn lerp_fixed(a: Fixed16, b: Fixed16, t: Fixed16) -> Fixed16 {
        a + (b - a) * t
    }

    fn lerp_u8(a: u8, b: u8, t: Fixed16) -> u8 {
        let a = Fixed16::from_i32(a as i32);
        let b = Fixed16::from_i32(b as i32);
        let result = a + (b - a) * t;
        result.to_i32().clamp(0, 255) as u8
    }
}

/// Animation timing with fixed-point precision
#[derive(Debug, Clone)]
pub struct AnimationTiming {
    /// Duration in milliseconds (fixed-point)
    pub duration: FixedTime,
    /// Start delay (fixed-point)
    pub delay: FixedTime,
    /// End delay (fixed-point)
    pub end_delay: FixedTime,
    /// Number of iterations (fixed-point for fractional)
    pub iterations: Fixed16,
    pub direction: PlaybackDirection,
    pub fill: FillMode,
    pub easing: Easing,
}

impl Default for AnimationTiming {
    fn default() -> Self {
        Self {
            duration: Fixed16::ZERO,
            delay: Fixed16::ZERO,
            end_delay: Fixed16::ZERO,
            iterations: Fixed16::ONE,
            direction: PlaybackDirection::Normal,
            fill: FillMode::None,
            easing: Easing::Linear,
        }
    }
}

/// Web animation with fixed-point timing
#[derive(Debug)]
pub struct FixedAnimation {
    pub id: u64,
    pub target: u64,
    pub keyframes: Vec<Keyframe>,
    pub timing: AnimationTiming,
    pub play_state: AnimationPlayState,
    /// Current time (fixed-point milliseconds)
    pub current_time: FixedTime,
    /// Start time (fixed-point milliseconds)
    pub start_time: Option<FixedTime>,
    /// Playback rate (fixed-point, 1.0 = normal)
    pub playback_rate: Fixed16,
}

static mut NEXT_FIXED_ANIM_ID: u64 = 1;

impl FixedAnimation {
    pub fn new(target: u64, keyframes: Vec<Keyframe>, timing: AnimationTiming) -> Self {
        let id = unsafe {
            let id = NEXT_FIXED_ANIM_ID;
            NEXT_FIXED_ANIM_ID += 1;
            id
        };
        Self {
            id,
            target,
            keyframes,
            timing,
            play_state: AnimationPlayState::Idle,
            current_time: Fixed16::ZERO,
            start_time: None,
            playback_rate: Fixed16::ONE,
        }
    }

    /// Play animation
    pub fn play(&mut self, now: FixedTime) {
        if self.start_time.is_none() {
            self.start_time = Some(now);
        }
        self.play_state = AnimationPlayState::Running;
    }

    /// Pause animation
    pub fn pause(&mut self) {
        self.play_state = AnimationPlayState::Paused;
    }

    /// Cancel animation
    pub fn cancel(&mut self) {
        self.play_state = AnimationPlayState::Idle;
        self.current_time = Fixed16::ZERO;
        self.start_time = None;
    }

    /// Finish animation immediately
    pub fn finish(&mut self) {
        self.play_state = AnimationPlayState::Finished;
        self.current_time = self.timing.duration * self.timing.iterations;
    }

    /// Reverse playback
    pub fn reverse(&mut self) {
        self.playback_rate = Fixed16::ZERO - self.playback_rate;
    }

    /// Update animation (returns interpolated values)
    pub fn update(&mut self, now: FixedTime) -> Option<HashMap<String, AnimatedValue>> {
        if self.play_state != AnimationPlayState::Running {
            return None;
        }

        let elapsed = now - self.start_time.unwrap_or(now) - self.timing.delay;
        if elapsed < Fixed16::ZERO {
            // Still in delay period
            return if self.timing.fill == FillMode::Backwards || self.timing.fill == FillMode::Both {
                Some(self.interpolate_at(Fixed16::ZERO))
            } else {
                None
            };
        }

        self.current_time = elapsed * self.playback_rate;

        let total_duration = self.timing.duration * self.timing.iterations;
        if self.current_time >= total_duration {
            self.play_state = AnimationPlayState::Finished;
            self.current_time = total_duration;

            return if self.timing.fill == FillMode::Forwards || self.timing.fill == FillMode::Both {
                Some(self.interpolate_at(Fixed16::ONE))
            } else {
                None
            };
        }

        // Calculate iteration progress
        let iteration_progress = if self.timing.duration > Fixed16::ZERO {
            let modulo = self.current_time.to_i32() % self.timing.duration.to_i32().max(1);
            Fixed16::from_i32(modulo) / self.timing.duration
        } else {
            Fixed16::ONE
        };

        // Apply direction
        let directed_progress = self.apply_direction(iteration_progress);

        // Apply easing
        let eased_progress = self.timing.easing.apply(directed_progress);

        Some(self.interpolate_at(eased_progress))
    }

    fn apply_direction(&self, progress: Fixed16) -> Fixed16 {
        let iteration = if self.timing.duration > Fixed16::ZERO {
            (self.current_time / self.timing.duration).to_i32() as u32
        } else {
            0
        };

        match self.timing.direction {
            PlaybackDirection::Normal => progress,
            PlaybackDirection::Reverse => Fixed16::ONE - progress,
            PlaybackDirection::Alternate => {
                if iteration % 2 == 0 { progress } else { Fixed16::ONE - progress }
            }
            PlaybackDirection::AlternateReverse => {
                if iteration % 2 == 0 { Fixed16::ONE - progress } else { progress }
            }
        }
    }

    fn interpolate_at(&self, progress: Fixed16) -> HashMap<String, AnimatedValue> {
        let mut result = HashMap::new();

        if self.keyframes.len() < 2 {
            return result;
        }

        // Find surrounding keyframes
        let mut from_idx = 0;
        let mut to_idx = 1;
        let kf_count = self.keyframes.len();

        for (i, kf) in self.keyframes.iter().enumerate() {
            let offset = kf.offset.unwrap_or_else(|| {
                Fixed16::from_i32(i as i32) / Fixed16::from_i32((kf_count - 1) as i32)
            });
            if offset <= progress {
                from_idx = i;
            } else {
                to_idx = i;
                break;
            }
        }

        let from = &self.keyframes[from_idx];
        let to = &self.keyframes[to_idx.min(kf_count - 1)];

        let from_offset = from.offset.unwrap_or_else(|| {
            Fixed16::from_i32(from_idx as i32) / Fixed16::from_i32((kf_count - 1) as i32)
        });
        let to_offset = to.offset.unwrap_or_else(|| {
            Fixed16::from_i32(to_idx as i32) / Fixed16::from_i32((kf_count - 1) as i32)
        });

        let local_progress = if to_offset != from_offset {
            (progress - from_offset) / (to_offset - from_offset)
        } else {
            Fixed16::ONE
        };

        // Apply keyframe easing
        let eased_local = from.easing.apply(local_progress);

        // Interpolate all properties
        for (prop, from_val) in &from.properties {
            if let Some(to_val) = to.properties.get(prop) {
                result.insert(prop.clone(), from_val.interpolate(to_val, eased_local));
            }
        }

        result
    }
}

/// Animation manager with fixed-point timing
#[derive(Debug, Default)]
pub struct FixedAnimationManager {
    animations: Vec<FixedAnimation>,
}

impl FixedAnimationManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create and start animation
    pub fn animate(
        &mut self,
        target: u64,
        keyframes: Vec<Keyframe>,
        timing: AnimationTiming,
        now: FixedTime,
    ) -> u64 {
        let mut anim = FixedAnimation::new(target, keyframes, timing);
        let id = anim.id;
        anim.play(now);
        self.animations.push(anim);
        id
    }

    /// Get animation by ID
    pub fn get(&mut self, id: u64) -> Option<&mut FixedAnimation> {
        self.animations.iter_mut().find(|a| a.id == id)
    }

    /// Remove animation
    pub fn remove(&mut self, id: u64) {
        self.animations.retain(|a| a.id != id);
    }

    /// Update all animations
    pub fn update(&mut self, now: FixedTime) -> Vec<(u64, HashMap<String, AnimatedValue>)> {
        let mut results = Vec::new();

        for anim in &mut self.animations {
            if let Some(values) = anim.update(now) {
                results.push((anim.target, values));
            }
        }

        // Keep finished animations for fill mode
        self.animations.retain(|a| {
            a.play_state != AnimationPlayState::Finished
                || a.timing.fill == FillMode::Forwards
                || a.timing.fill == FillMode::Both
        });

        results
    }

    /// Get all animations for element
    pub fn get_animations(&self, target: u64) -> Vec<&FixedAnimation> {
        self.animations.iter().filter(|a| a.target == target).collect()
    }

    /// Cancel all animations
    pub fn cancel_all(&mut self) {
        for anim in &mut self.animations {
            anim.cancel();
        }
        self.animations.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_easing_linear() {
        let easing = Easing::Linear;
        assert_eq!(easing.apply(Fixed16::ZERO), Fixed16::ZERO);
        assert_eq!(easing.apply(Fixed16::ONE), Fixed16::ONE);
        assert_eq!(easing.apply(Fixed16::from_f32(0.5)), Fixed16::from_f32(0.5));
    }

    #[test]
    fn test_fixed_animation() {
        let mut mgr = FixedAnimationManager::new();

        let keyframes = vec![
            Keyframe {
                offset: Some(Fixed16::ZERO),
                easing: Easing::Linear,
                composite: CompositeOperation::Replace,
                properties: {
                    let mut h = HashMap::new();
                    h.insert("opacity".to_string(), AnimatedValue::Number(Fixed16::ZERO));
                    h
                },
            },
            Keyframe {
                offset: Some(Fixed16::ONE),
                easing: Easing::Linear,
                composite: CompositeOperation::Replace,
                properties: {
                    let mut h = HashMap::new();
                    h.insert("opacity".to_string(), AnimatedValue::Number(Fixed16::ONE));
                    h
                },
            },
        ];

        let timing = AnimationTiming {
            duration: Fixed16::from_i32(1000),
            ..Default::default()
        };

        let id = mgr.animate(1, keyframes, timing, Fixed16::ZERO);
        assert!(mgr.get(id).is_some());
    }

    #[test]
    fn test_animated_value_interpolation() {
        let a = AnimatedValue::Number(Fixed16::ZERO);
        let b = AnimatedValue::Number(Fixed16::ONE);

        if let AnimatedValue::Number(v) = a.interpolate(&b, Fixed16::from_f32(0.5)) {
            // Should be approximately 0.5
            assert!(v > Fixed16::from_f32(0.4) && v < Fixed16::from_f32(0.6));
        }
    }
}
