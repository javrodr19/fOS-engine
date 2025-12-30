//! CSS Animations module
//!
//! Provides transitions and keyframe animations.

use std::time::Duration;

/// Easing/timing function for animations
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum TimingFunction {
    /// Linear interpolation
    #[default]
    Linear,
    /// Ease (default CSS)
    Ease,
    /// Ease-in (slow start)
    EaseIn,
    /// Ease-out (slow end)
    EaseOut,
    /// Ease-in-out
    EaseInOut,
    /// Cubic bezier curve
    CubicBezier(f32, f32, f32, f32),
    /// Step function
    Steps(u32, StepPosition),
}

/// Step position for step timing
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StepPosition {
    #[default]
    End,
    Start,
    Both,
    None,
}

impl TimingFunction {
    /// Evaluate the timing function at progress t (0.0 to 1.0)
    pub fn evaluate(&self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::Ease => cubic_bezier(0.25, 0.1, 0.25, 1.0, t),
            Self::EaseIn => cubic_bezier(0.42, 0.0, 1.0, 1.0, t),
            Self::EaseOut => cubic_bezier(0.0, 0.0, 0.58, 1.0, t),
            Self::EaseInOut => cubic_bezier(0.42, 0.0, 0.58, 1.0, t),
            Self::CubicBezier(x1, y1, x2, y2) => cubic_bezier(*x1, *y1, *x2, *y2, t),
            Self::Steps(steps, position) => step_function(*steps, *position, t),
        }
    }
}

/// Cubic bezier evaluation
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    // Newton-Raphson method to find t for x, then evaluate y
    let mut guess = t;
    for _ in 0..8 {
        let x = bezier_sample(x1, x2, guess);
        let dx = bezier_derivative(x1, x2, guess);
        if dx.abs() < 1e-6 {
            break;
        }
        guess -= (x - t) / dx;
        guess = guess.clamp(0.0, 1.0);
    }
    bezier_sample(y1, y2, guess)
}

fn bezier_sample(p1: f32, p2: f32, t: f32) -> f32 {
    let t2 = t * t;
    let t3 = t2 * t;
    let mt = 1.0 - t;
    let mt2 = mt * mt;
    3.0 * mt2 * t * p1 + 3.0 * mt * t2 * p2 + t3
}

fn bezier_derivative(p1: f32, p2: f32, t: f32) -> f32 {
    let t2 = t * t;
    let mt = 1.0 - t;
    3.0 * mt * mt * p1 + 6.0 * mt * t * (p2 - p1) + 3.0 * t2 * (1.0 - p2)
}

fn step_function(steps: u32, position: StepPosition, t: f32) -> f32 {
    let steps = steps.max(1) as f32;
    match position {
        StepPosition::Start => (t * steps).ceil() / steps,
        StepPosition::End => (t * steps).floor() / steps,
        StepPosition::Both => {
            if t <= 0.0 { 0.0 }
            else if t >= 1.0 { 1.0 }
            else { ((t * (steps + 1.0)).floor() - 1.0) / steps }
        }
        StepPosition::None => {
            ((t * steps).floor() + 0.5) / steps
        }
    }
}

/// CSS Transition definition
#[derive(Debug, Clone)]
pub struct Transition {
    /// Property being transitioned (or "all")
    pub property: String,
    /// Duration
    pub duration: Duration,
    /// Timing function
    pub timing: TimingFunction,
    /// Delay before starting
    pub delay: Duration,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            property: "all".to_string(),
            duration: Duration::from_millis(0),
            timing: TimingFunction::Ease,
            delay: Duration::from_millis(0),
        }
    }
}

impl Transition {
    /// Create a transition for a property
    pub fn new(property: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            property: property.into(),
            duration: Duration::from_millis(duration_ms),
            ..Default::default()
        }
    }
    
    /// Set timing function
    pub fn with_timing(mut self, timing: TimingFunction) -> Self {
        self.timing = timing;
        self
    }
    
    /// Set delay
    pub fn with_delay(mut self, delay_ms: u64) -> Self {
        self.delay = Duration::from_millis(delay_ms);
        self
    }
}

/// Keyframe in an animation
#[derive(Debug, Clone)]
pub struct Keyframe {
    /// Position (0.0 = 0%, 1.0 = 100%)
    pub offset: f32,
    /// Properties at this keyframe (name -> value as string)
    pub properties: std::collections::HashMap<String, AnimatedValue>,
    /// Optional timing function to next keyframe
    pub timing: Option<TimingFunction>,
}

impl Keyframe {
    pub fn at(offset: f32) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            properties: std::collections::HashMap::new(),
            timing: None,
        }
    }
    
    pub fn with_property(mut self, name: impl Into<String>, value: AnimatedValue) -> Self {
        self.properties.insert(name.into(), value);
        self
    }
}

/// Animatable value types
#[derive(Debug, Clone, PartialEq)]
pub enum AnimatedValue {
    Number(f32),
    Color { r: u8, g: u8, b: u8, a: u8 },
    Length(f32), // pixels
    Percentage(f32),
}

impl AnimatedValue {
    /// Interpolate between two values
    pub fn interpolate(&self, other: &AnimatedValue, t: f32) -> Option<AnimatedValue> {
        match (self, other) {
            (AnimatedValue::Number(a), AnimatedValue::Number(b)) => {
                Some(AnimatedValue::Number(a + (b - a) * t))
            }
            (AnimatedValue::Length(a), AnimatedValue::Length(b)) => {
                Some(AnimatedValue::Length(a + (b - a) * t))
            }
            (AnimatedValue::Percentage(a), AnimatedValue::Percentage(b)) => {
                Some(AnimatedValue::Percentage(a + (b - a) * t))
            }
            (AnimatedValue::Color { r: r1, g: g1, b: b1, a: a1 },
             AnimatedValue::Color { r: r2, g: g2, b: b2, a: a2 }) => {
                Some(AnimatedValue::Color {
                    r: (*r1 as f32 + (*r2 as f32 - *r1 as f32) * t) as u8,
                    g: (*g1 as f32 + (*g2 as f32 - *g1 as f32) * t) as u8,
                    b: (*b1 as f32 + (*b2 as f32 - *b1 as f32) * t) as u8,
                    a: (*a1 as f32 + (*a2 as f32 - *a1 as f32) * t) as u8,
                })
            }
            _ => None, // Incompatible types
        }
    }
}

/// Keyframe animation definition
#[derive(Debug, Clone)]
pub struct KeyframeAnimation {
    /// Animation name
    pub name: String,
    /// Keyframes (sorted by offset)
    pub keyframes: Vec<Keyframe>,
    /// Total duration
    pub duration: Duration,
    /// Timing function (default for between keyframes)
    pub timing: TimingFunction,
    /// Iteration count (0 = infinite)
    pub iterations: u32,
    /// Play direction
    pub direction: AnimationDirection,
    /// Fill mode
    pub fill_mode: FillMode,
    /// Delay
    pub delay: Duration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AnimationDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FillMode {
    #[default]
    None,
    Forwards,
    Backwards,
    Both,
}

impl KeyframeAnimation {
    /// Create a new animation
    pub fn new(name: impl Into<String>, duration_ms: u64) -> Self {
        Self {
            name: name.into(),
            keyframes: Vec::new(),
            duration: Duration::from_millis(duration_ms),
            timing: TimingFunction::Ease,
            iterations: 1,
            direction: AnimationDirection::Normal,
            fill_mode: FillMode::None,
            delay: Duration::ZERO,
        }
    }
    
    /// Add a keyframe
    pub fn with_keyframe(mut self, keyframe: Keyframe) -> Self {
        self.keyframes.push(keyframe);
        self.keyframes.sort_by(|a, b| a.offset.partial_cmp(&b.offset).unwrap());
        self
    }
    
    /// Set iterations (0 = infinite)
    pub fn with_iterations(mut self, count: u32) -> Self {
        self.iterations = count;
        self
    }
    
    /// Set direction
    pub fn with_direction(mut self, direction: AnimationDirection) -> Self {
        self.direction = direction;
        self
    }
    
    /// Get interpolated value at time t (0.0 to 1.0 of one iteration)
    pub fn sample(&self, property: &str, t: f32) -> Option<AnimatedValue> {
        if self.keyframes.is_empty() {
            return None;
        }
        
        // Find surrounding keyframes
        let mut prev: Option<&Keyframe> = None;
        let mut next: Option<&Keyframe> = None;
        
        for kf in &self.keyframes {
            if kf.offset <= t {
                prev = Some(kf);
            }
            if kf.offset >= t && next.is_none() {
                next = Some(kf);
            }
        }
        
        match (prev, next) {
            (Some(p), Some(n)) if p.offset != n.offset => {
                let local_t = (t - p.offset) / (n.offset - p.offset);
                let timing = p.timing.unwrap_or(self.timing);
                let eased_t = timing.evaluate(local_t);
                
                let from = p.properties.get(property)?;
                let to = n.properties.get(property)?;
                from.interpolate(to, eased_t)
            }
            (Some(p), _) => p.properties.get(property).cloned(),
            (_, Some(n)) => n.properties.get(property).cloned(),
            _ => None,
        }
    }
}

/// Active animation instance
#[derive(Debug, Clone)]
pub struct AnimationInstance {
    /// The animation definition
    pub animation: KeyframeAnimation,
    /// Start time (elapsed since animation started)
    pub elapsed: Duration,
    /// Current iteration
    pub iteration: u32,
    /// Is paused
    pub paused: bool,
    /// Is finished
    pub finished: bool,
}

impl AnimationInstance {
    pub fn new(animation: KeyframeAnimation) -> Self {
        Self {
            animation,
            elapsed: Duration::ZERO,
            iteration: 0,
            paused: false,
            finished: false,
        }
    }
    
    /// Update animation with delta time, returns current progress (0.0-1.0)
    pub fn update(&mut self, dt: Duration) -> f32 {
        if self.paused || self.finished {
            return self.current_progress();
        }
        
        self.elapsed += dt;
        
        // Handle delay
        if self.elapsed < self.animation.delay {
            return 0.0;
        }
        
        let active_time = self.elapsed - self.animation.delay;
        let duration = self.animation.duration.as_secs_f32();
        
        if duration <= 0.0 {
            self.finished = true;
            return 1.0;
        }
        
        let total_progress = active_time.as_secs_f32() / duration;
        let iteration = total_progress.floor() as u32;
        
        // Check if finished
        if self.animation.iterations > 0 && iteration >= self.animation.iterations {
            self.finished = true;
            self.iteration = self.animation.iterations - 1;
            return 1.0;
        }
        
        self.iteration = iteration;
        self.current_progress()
    }
    
    /// Get current progress within current iteration (0.0-1.0)
    pub fn current_progress(&self) -> f32 {
        if self.animation.duration.as_secs_f32() <= 0.0 {
            return 1.0;
        }
        
        let active_time = if self.elapsed > self.animation.delay {
            self.elapsed - self.animation.delay
        } else {
            Duration::ZERO
        };
        
        let duration = self.animation.duration.as_secs_f32();
        let mut progress = (active_time.as_secs_f32() % duration) / duration;
        
        // Handle direction
        match self.animation.direction {
            AnimationDirection::Normal => {}
            AnimationDirection::Reverse => progress = 1.0 - progress,
            AnimationDirection::Alternate => {
                if self.iteration % 2 == 1 {
                    progress = 1.0 - progress;
                }
            }
            AnimationDirection::AlternateReverse => {
                if self.iteration % 2 == 0 {
                    progress = 1.0 - progress;
                }
            }
        }
        
        progress
    }
    
    /// Get animated value for a property
    pub fn get_value(&self, property: &str) -> Option<AnimatedValue> {
        self.animation.sample(property, self.current_progress())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_linear_timing() {
        let timing = TimingFunction::Linear;
        assert_eq!(timing.evaluate(0.0), 0.0);
        assert_eq!(timing.evaluate(0.5), 0.5);
        assert_eq!(timing.evaluate(1.0), 1.0);
    }
    
    #[test]
    fn test_ease_timing() {
        let timing = TimingFunction::Ease;
        // Ease at 0.5 should be roughly between 0.5 and 0.9 (easing accelerates in middle)
        let v = timing.evaluate(0.5);
        assert!(v > 0.3, "ease(0.5) = {} should be > 0.3", v);
        assert!(v < 0.95, "ease(0.5) = {} should be < 0.95", v);
    }
    
    #[test]
    fn test_value_interpolation() {
        let a = AnimatedValue::Number(0.0);
        let b = AnimatedValue::Number(100.0);
        
        let mid = a.interpolate(&b, 0.5).unwrap();
        assert_eq!(mid, AnimatedValue::Number(50.0));
    }
    
    #[test]
    fn test_color_interpolation() {
        let white = AnimatedValue::Color { r: 255, g: 255, b: 255, a: 255 };
        let black = AnimatedValue::Color { r: 0, g: 0, b: 0, a: 255 };
        
        let mid = white.interpolate(&black, 0.5).unwrap();
        if let AnimatedValue::Color { r, g, b, .. } = mid {
            assert!(r > 120 && r < 135);
            assert!(g > 120 && g < 135);
            assert!(b > 120 && b < 135);
        }
    }
    
    #[test]
    fn test_keyframe_animation() {
        // Use linear timing for predictable test
        let mut anim = KeyframeAnimation::new("fade", 1000)
            .with_keyframe(Keyframe::at(0.0).with_property("opacity", AnimatedValue::Number(0.0)))
            .with_keyframe(Keyframe::at(1.0).with_property("opacity", AnimatedValue::Number(1.0)));
        anim.timing = TimingFunction::Linear;
        
        let mid = anim.sample("opacity", 0.5).unwrap();
        if let AnimatedValue::Number(v) = mid {
            assert!(v > 0.4 && v < 0.6, "opacity at 0.5 = {}", v);
        }
    }
    
    #[test]
    fn test_transition() {
        let t = Transition::new("opacity", 300)
            .with_timing(TimingFunction::EaseOut)
            .with_delay(100);
        
        assert_eq!(t.property, "opacity");
        assert_eq!(t.duration, Duration::from_millis(300));
        assert_eq!(t.delay, Duration::from_millis(100));
    }
}
