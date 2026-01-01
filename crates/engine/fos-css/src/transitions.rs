//! CSS Transitions
//!
//! Implements CSS transitions with Fixed-Point timing for deterministic animation.
//! https://www.w3.org/TR/css-transitions-1/

use std::collections::HashMap;

// Local Fixed16 to avoid circular dependency with fos-engine
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
#[repr(transparent)]
pub struct Fixed16(i32);

impl Fixed16 {
    const FRAC_BITS: u32 = 16;
    const SCALE: i32 = 1 << Self::FRAC_BITS;
    
    pub const ZERO: Fixed16 = Fixed16(0);
    pub const ONE: Fixed16 = Fixed16(Self::SCALE);
    
    #[inline]
    pub const fn from_f32(value: f32) -> Self {
        Self((value * Self::SCALE as f32) as i32)
    }
    
    #[inline]
    pub const fn to_f32(self) -> f32 {
        self.0 as f32 / Self::SCALE as f32
    }
    
    #[inline]
    pub fn lerp(self, other: Self, t: Self) -> Self {
        let diff = (other.0 as i64 - self.0 as i64) * t.0 as i64;
        Self(self.0 + (diff >> Self::FRAC_BITS) as i32)
    }
    
    #[inline]
    pub fn clamp(self, min: Self, max: Self) -> Self {
        Self(self.0.max(min.0).min(max.0))
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

/// CSS transition timing function
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum TimingFunction {
    /// linear
    #[default]
    Linear,
    /// ease
    Ease,
    /// ease-in
    EaseIn,
    /// ease-out
    EaseOut,
    /// ease-in-out
    EaseInOut,
    /// cubic-bezier(x1, y1, x2, y2)
    CubicBezier(f32, f32, f32, f32),
    /// steps(count, position)
    Steps(u32, StepPosition),
}

/// Step position for steps() timing function
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StepPosition {
    #[default]
    End,
    Start,
    JumpNone,
    JumpBoth,
}

impl TimingFunction {
    /// Evaluate the timing function at time t (0.0 to 1.0)
    pub fn evaluate(&self, t: Fixed16) -> Fixed16 {
        let t_f32 = t.to_f32().clamp(0.0, 1.0);
        let result = match self {
            TimingFunction::Linear => t_f32,
            TimingFunction::Ease => cubic_bezier(0.25, 0.1, 0.25, 1.0, t_f32),
            TimingFunction::EaseIn => cubic_bezier(0.42, 0.0, 1.0, 1.0, t_f32),
            TimingFunction::EaseOut => cubic_bezier(0.0, 0.0, 0.58, 1.0, t_f32),
            TimingFunction::EaseInOut => cubic_bezier(0.42, 0.0, 0.58, 1.0, t_f32),
            TimingFunction::CubicBezier(x1, y1, x2, y2) => cubic_bezier(*x1, *y1, *x2, *y2, t_f32),
            TimingFunction::Steps(steps, position) => {
                step_function(*steps, *position, t_f32)
            }
        };
        Fixed16::from_f32(result)
    }
}

/// Cubic bezier evaluation (simplified Newton-Raphson)
fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, t: f32) -> f32 {
    // Find t for x using Newton's method
    let mut guess = t;
    for _ in 0..8 {
        let x = bezier_component(x1, x2, guess) - t;
        if x.abs() < 0.0001 { break; }
        let dx = bezier_derivative(x1, x2, guess);
        if dx.abs() < 0.0001 { break; }
        guess -= x / dx;
    }
    bezier_component(y1, y2, guess)
}

fn bezier_component(p1: f32, p2: f32, t: f32) -> f32 {
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
        StepPosition::JumpNone => {
            let s = steps - 1.0;
            if s <= 0.0 { t } else { (t * s).floor() / s }
        }
        StepPosition::JumpBoth => {
            let s = steps + 1.0;
            ((t * s).floor() / s).min(1.0)
        }
    }
}

/// A CSS transition definition
#[derive(Debug, Clone)]
pub struct Transition {
    /// Property being transitioned (empty string = all)
    pub property: String,
    /// Duration in milliseconds (Fixed16)
    pub duration_ms: Fixed16,
    /// Delay in milliseconds (Fixed16)
    pub delay_ms: Fixed16,
    /// Timing function
    pub timing: TimingFunction,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            property: String::new(),
            duration_ms: Fixed16::ZERO,
            delay_ms: Fixed16::ZERO,
            timing: TimingFunction::Ease,
        }
    }
}

impl Transition {
    /// Create a new transition
    pub fn new(property: &str, duration_ms: f32) -> Self {
        Self {
            property: property.to_string(),
            duration_ms: Fixed16::from_f32(duration_ms),
            delay_ms: Fixed16::ZERO,
            timing: TimingFunction::Ease,
        }
    }
    
    /// Set delay
    pub fn with_delay(mut self, delay_ms: f32) -> Self {
        self.delay_ms = Fixed16::from_f32(delay_ms);
        self
    }
    
    /// Set timing function
    pub fn with_timing(mut self, timing: TimingFunction) -> Self {
        self.timing = timing;
        self
    }
    
    /// Parse from CSS value string
    pub fn parse(value: &str) -> Option<Transition> {
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }
        
        let mut transition = Transition::default();
        let mut idx = 0;
        
        // First part is property name
        if !parts.is_empty() {
            transition.property = parts[0].to_string();
            idx += 1;
        }
        
        // Parse duration
        if idx < parts.len() {
            if let Some(dur) = parse_time(parts[idx]) {
                transition.duration_ms = Fixed16::from_f32(dur);
                idx += 1;
            }
        }
        
        // Parse timing function or delay
        while idx < parts.len() {
            let part = parts[idx];
            if let Some(timing) = parse_timing_function(part) {
                transition.timing = timing;
            } else if let Some(delay) = parse_time(part) {
                transition.delay_ms = Fixed16::from_f32(delay);
            }
            idx += 1;
        }
        
        Some(transition)
    }
}

fn parse_time(s: &str) -> Option<f32> {
    if s.ends_with("ms") {
        s.trim_end_matches("ms").parse().ok()
    } else if s.ends_with('s') {
        s.trim_end_matches('s').parse::<f32>().ok().map(|v| v * 1000.0)
    } else {
        None
    }
}

fn parse_timing_function(s: &str) -> Option<TimingFunction> {
    match s {
        "linear" => Some(TimingFunction::Linear),
        "ease" => Some(TimingFunction::Ease),
        "ease-in" => Some(TimingFunction::EaseIn),
        "ease-out" => Some(TimingFunction::EaseOut),
        "ease-in-out" => Some(TimingFunction::EaseInOut),
        _ if s.starts_with("cubic-bezier(") => {
            let inner = s.trim_start_matches("cubic-bezier(").trim_end_matches(')');
            let vals: Vec<f32> = inner.split(',').filter_map(|v| v.trim().parse().ok()).collect();
            if vals.len() == 4 {
                Some(TimingFunction::CubicBezier(vals[0], vals[1], vals[2], vals[3]))
            } else {
                None
            }
        }
        _ if s.starts_with("steps(") => {
            let inner = s.trim_start_matches("steps(").trim_end_matches(')');
            let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
            let steps = parts.first()?.parse().ok()?;
            let position = match parts.get(1).map(|s| *s) {
                Some("start") => StepPosition::Start,
                Some("jump-none") => StepPosition::JumpNone,
                Some("jump-both") => StepPosition::JumpBoth,
                _ => StepPosition::End,
            };
            Some(TimingFunction::Steps(steps, position))
        }
        _ => None,
    }
}

/// An active transition in progress
#[derive(Debug, Clone)]
pub struct ActiveTransition {
    /// Property being transitioned
    pub property: String,
    /// Start value (as f32 for interpolation)
    pub start_value: f32,
    /// End value
    pub end_value: f32,
    /// Elapsed time in ms
    pub elapsed_ms: Fixed16,
    /// Total duration
    pub duration_ms: Fixed16,
    /// Delay remaining
    pub delay_ms: Fixed16,
    /// Timing function
    pub timing: TimingFunction,
}

impl ActiveTransition {
    /// Create a new active transition
    pub fn new(property: &str, start: f32, end: f32, def: &Transition) -> Self {
        Self {
            property: property.to_string(),
            start_value: start,
            end_value: end,
            elapsed_ms: Fixed16::ZERO,
            duration_ms: def.duration_ms,
            delay_ms: def.delay_ms,
            timing: def.timing,
        }
    }
    
    /// Check if transition has started (past delay)
    pub fn has_started(&self) -> bool {
        self.delay_ms.0 <= 0
    }
    
    /// Check if transition is complete
    pub fn is_complete(&self) -> bool {
        self.has_started() && self.elapsed_ms.0 >= self.duration_ms.0
    }
    
    /// Get current interpolated value
    pub fn current_value(&self) -> f32 {
        if !self.has_started() {
            return self.start_value;
        }
        if self.is_complete() {
            return self.end_value;
        }
        
        let progress = if self.duration_ms.0 == 0 {
            Fixed16::ONE
        } else {
            self.elapsed_ms / self.duration_ms
        };
        
        let eased = self.timing.evaluate(progress);
        let t = eased.to_f32();
        
        self.start_value + (self.end_value - self.start_value) * t
    }
    
    /// Advance the transition by delta milliseconds
    pub fn tick(&mut self, delta_ms: Fixed16) {
        if self.delay_ms.0 > 0 {
            self.delay_ms = Fixed16(self.delay_ms.0 - delta_ms.0);
            if self.delay_ms.0 < 0 {
                // Overflow elapsed time into transition
                self.elapsed_ms = Fixed16(-self.delay_ms.0);
                self.delay_ms = Fixed16::ZERO;
            }
        } else {
            self.elapsed_ms = self.elapsed_ms + delta_ms;
        }
    }
}

/// Engine managing multiple transitions
#[derive(Debug, Default)]
pub struct TransitionEngine {
    /// Active transitions keyed by element ID and property
    transitions: HashMap<(u64, String), ActiveTransition>,
    /// Transition definitions per element
    definitions: HashMap<u64, Vec<Transition>>,
}

impl TransitionEngine {
    /// Create a new transition engine
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set transition definitions for an element
    pub fn set_definitions(&mut self, element_id: u64, defs: Vec<Transition>) {
        self.definitions.insert(element_id, defs);
    }
    
    /// Start a transition for a property change
    pub fn start_transition(
        &mut self,
        element_id: u64,
        property: &str,
        start_value: f32,
        end_value: f32,
    ) {
        // Find matching transition definition
        let def = self.definitions.get(&element_id)
            .and_then(|defs| {
                defs.iter().find(|d| d.property == property || d.property == "all" || d.property.is_empty())
            });
        
        if let Some(def) = def {
            let key = (element_id, property.to_string());
            let active = ActiveTransition::new(property, start_value, end_value, def);
            self.transitions.insert(key, active);
        }
    }
    
    /// Get current value for a transitioning property
    pub fn get_value(&self, element_id: u64, property: &str) -> Option<f32> {
        let key = (element_id, property.to_string());
        self.transitions.get(&key).map(|t| t.current_value())
    }
    
    /// Advance all transitions by delta time
    pub fn tick(&mut self, delta_ms: f32) {
        let delta = Fixed16::from_f32(delta_ms);
        
        // Tick and remove completed
        self.transitions.retain(|_, t| {
            t.tick(delta);
            !t.is_complete()
        });
    }
    
    /// Check if any transitions are active
    pub fn has_active_transitions(&self) -> bool {
        !self.transitions.is_empty()
    }
    
    /// Get all active transitions for an element
    pub fn get_element_transitions(&self, element_id: u64) -> Vec<&ActiveTransition> {
        self.transitions.iter()
            .filter(|((id, _), _)| *id == element_id)
            .map(|(_, t)| t)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fixed16_basic() {
        let a = Fixed16::from_f32(10.5);
        let b = Fixed16::from_f32(2.0);
        
        assert!((a.to_f32() - 10.5).abs() < 0.001);
        assert!((b.to_f32() - 2.0).abs() < 0.001);
        
        let sum = a + b;
        assert!((sum.to_f32() - 12.5).abs() < 0.001);
    }
    
    #[test]
    fn test_timing_linear() {
        let timing = TimingFunction::Linear;
        
        let t0 = timing.evaluate(Fixed16::ZERO);
        let t50 = timing.evaluate(Fixed16::from_f32(0.5));
        let t100 = timing.evaluate(Fixed16::ONE);
        
        assert!((t0.to_f32() - 0.0).abs() < 0.01);
        assert!((t50.to_f32() - 0.5).abs() < 0.01);
        assert!((t100.to_f32() - 1.0).abs() < 0.01);
    }
    
    #[test]
    fn test_timing_ease() {
        let timing = TimingFunction::Ease;
        
        let t50 = timing.evaluate(Fixed16::from_f32(0.5));
        // Ease should be > 0.5 at midpoint due to fast start
        assert!(t50.to_f32() > 0.5);
    }
    
    #[test]
    fn test_transition_parse() {
        let t = Transition::parse("opacity 300ms ease-in").unwrap();
        
        assert_eq!(t.property, "opacity");
        assert!((t.duration_ms.to_f32() - 300.0).abs() < 0.1);
        assert_eq!(t.timing, TimingFunction::EaseIn);
    }
    
    #[test]
    fn test_transition_with_delay() {
        let t = Transition::parse("transform 500ms ease 100ms").unwrap();
        
        assert_eq!(t.property, "transform");
        assert!((t.duration_ms.to_f32() - 500.0).abs() < 0.1);
        assert!((t.delay_ms.to_f32() - 100.0).abs() < 0.1);
    }
    
    #[test]
    fn test_active_transition() {
        let def = Transition::new("opacity", 300.0)
            .with_timing(TimingFunction::Linear); // Use linear for predictable test
        let mut active = ActiveTransition::new("opacity", 0.0, 1.0, &def);
        
        assert_eq!(active.current_value(), 0.0);
        
        active.tick(Fixed16::from_f32(150.0)); // 50%
        let val = active.current_value();
        assert!(val > 0.4 && val < 0.6, "val={} expected ~0.5", val);
        
        active.tick(Fixed16::from_f32(200.0)); // past 100%
        assert!(active.is_complete());
        assert!((active.current_value() - 1.0).abs() < 0.01);
    }
    
    #[test]
    fn test_transition_engine() {
        let mut engine = TransitionEngine::new();
        
        engine.set_definitions(1, vec![
            Transition::new("opacity", 300.0)
                .with_timing(TimingFunction::Linear), // Use linear for predictable test
        ]);
        
        engine.start_transition(1, "opacity", 0.0, 1.0);
        
        assert!(engine.has_active_transitions());
        
        engine.tick(150.0);
        let val = engine.get_value(1, "opacity").unwrap();
        assert!(val > 0.4 && val < 0.6, "val={} expected ~0.5", val);
        
        engine.tick(200.0);
        assert!(!engine.has_active_transitions()); // completed and removed
    }
}
