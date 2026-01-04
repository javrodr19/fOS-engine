//! Reduced Motion Support
//!
//! prefers-reduced-motion handling and animation control.

/// Motion preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MotionPreference {
    #[default]
    NoPreference,
    Reduce,
}

impl MotionPreference {
    pub fn from_system() -> Self {
        // Would query OS settings - returning default
        Self::NoPreference
    }
}

/// Animation state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationState { Running, Paused, Cancelled }

/// Reduced motion settings
#[derive(Debug, Clone)]
pub struct ReducedMotionSettings {
    pub preference: MotionPreference,
    pub disable_animations: bool,
    pub disable_auto_play: bool,
    pub reduce_parallax: bool,
    pub reduce_transparency: bool,
    pub static_scroll: bool,
}

impl Default for ReducedMotionSettings {
    fn default() -> Self {
        Self { preference: MotionPreference::NoPreference, disable_animations: false,
               disable_auto_play: false, reduce_parallax: false, reduce_transparency: false, static_scroll: false }
    }
}

impl ReducedMotionSettings {
    pub fn from_preference(pref: MotionPreference) -> Self {
        match pref {
            MotionPreference::NoPreference => Self::default(),
            MotionPreference::Reduce => Self { preference: pref, disable_animations: true,
                disable_auto_play: true, reduce_parallax: true, reduce_transparency: false, static_scroll: true },
        }
    }
    
    pub fn should_reduce(&self) -> bool { self.preference == MotionPreference::Reduce }
}

/// Animation override rules
#[derive(Debug, Clone)]
pub struct AnimationOverride {
    pub duration_scale: f64,    // 0.0 = instant, 1.0 = normal
    pub disable_transform: bool,
    pub disable_opacity: bool,
    pub max_duration_ms: Option<u32>,
}

impl Default for AnimationOverride {
    fn default() -> Self { Self::none() }
}

impl AnimationOverride {
    pub fn instant() -> Self { Self { duration_scale: 0.0, disable_transform: true, disable_opacity: true, max_duration_ms: Some(0) } }
    pub fn subtle() -> Self { Self { duration_scale: 0.3, disable_transform: false, disable_opacity: false, max_duration_ms: Some(150) } }
    pub fn none() -> Self { Self { duration_scale: 1.0, disable_transform: false, disable_opacity: false, max_duration_ms: None } }
    
    pub fn apply_duration(&self, duration_ms: u32) -> u32 {
        let scaled = (duration_ms as f64 * self.duration_scale) as u32;
        match self.max_duration_ms { Some(max) => scaled.min(max), None => scaled }
    }
}

/// Motion manager
#[derive(Debug, Default)]
pub struct MotionManager {
    settings: ReducedMotionSettings,
    override_rules: AnimationOverride,
    paused_animations: Vec<u64>,
}

impl MotionManager {
    pub fn new() -> Self { Self { override_rules: AnimationOverride::none(), ..Default::default() } }
    
    pub fn settings(&self) -> &ReducedMotionSettings { &self.settings }
    
    pub fn update_preference(&mut self, pref: MotionPreference) {
        self.settings = ReducedMotionSettings::from_preference(pref);
        self.override_rules = if pref == MotionPreference::Reduce { AnimationOverride::subtle() } else { AnimationOverride::none() };
    }
    
    pub fn should_animate(&self) -> bool { !self.settings.disable_animations }
    
    pub fn should_autoplay(&self) -> bool { !self.settings.disable_auto_play }
    
    pub fn get_duration(&self, original_ms: u32) -> u32 { self.override_rules.apply_duration(original_ms) }
    
    pub fn pause_animations(&mut self) {
        self.paused_animations.clear();
        // Would collect all running animation IDs
    }
    
    pub fn resume_animations(&mut self) { self.paused_animations.clear(); }
    
    pub fn can_use_parallax(&self) -> bool { !self.settings.reduce_parallax }
    
    pub fn can_use_scroll_animation(&self) -> bool { !self.settings.static_scroll }
}

/// CSS query matcher for prefers-reduced-motion
#[derive(Debug)]
pub struct ReducedMotionQuery {
    pub matches: bool,
}

impl ReducedMotionQuery {
    pub fn new(preference: MotionPreference) -> Self {
        Self { matches: preference == MotionPreference::Reduce }
    }
    
    pub fn check(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        if query.contains("prefers-reduced-motion") {
            // Check no-preference first since "reduce" is in "reduced-motion"
            if query.contains("no-preference") { return !self.matches; }
            if query.contains(": reduce") { return self.matches; }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_motion_preference() {
        let settings = ReducedMotionSettings::from_preference(MotionPreference::Reduce);
        assert!(settings.should_reduce());
        assert!(settings.disable_animations);
    }
    
    #[test]
    fn test_animation_override() {
        let subtle = AnimationOverride::subtle();
        assert_eq!(subtle.apply_duration(1000), 150); // capped at max
        
        let instant = AnimationOverride::instant();
        assert_eq!(instant.apply_duration(1000), 0);
    }
    
    #[test]
    fn test_query_matcher() {
        let query = ReducedMotionQuery::new(MotionPreference::Reduce);
        assert!(query.check("(prefers-reduced-motion: reduce)"));
        assert!(!query.check("(prefers-reduced-motion: no-preference)"));
    }
}
