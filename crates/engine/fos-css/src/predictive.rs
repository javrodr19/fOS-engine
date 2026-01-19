//! Predictive Styling
//!
//! Pre-compute likely states (:hover, :focus, :active) to reduce
//! style recalculation latency. Speculatively resolve styles on idle.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

// ============================================================================
// Predicted States
// ============================================================================

/// Pseudo-class state that can be predicted
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PredictableState {
    Hover,
    Focus,
    FocusVisible,
    FocusWithin,
    Active,
    Visited,
    Checked,
    Disabled,
    Empty,
}

impl PredictableState {
    /// All predictable states
    pub const ALL: &'static [PredictableState] = &[
        PredictableState::Hover,
        PredictableState::Focus,
        PredictableState::FocusVisible,
        PredictableState::FocusWithin,
        PredictableState::Active,
    ];
    
    /// States commonly triggered by user interaction
    pub const INTERACTIVE: &'static [PredictableState] = &[
        PredictableState::Hover,
        PredictableState::Focus,
        PredictableState::Active,
    ];
    
    /// CSS pseudo-class name
    pub fn as_pseudo_class(&self) -> &'static str {
        match self {
            Self::Hover => "hover",
            Self::Focus => "focus",
            Self::FocusVisible => "focus-visible",
            Self::FocusWithin => "focus-within",
            Self::Active => "active",
            Self::Visited => "visited",
            Self::Checked => "checked",
            Self::Disabled => "disabled",
            Self::Empty => "empty",
        }
    }
}

/// Set of active states for an element
#[derive(Debug, Clone, Default)]
pub struct StateSet {
    states: HashSet<PredictableState>,
}

impl StateSet {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn with_state(mut self, state: PredictableState) -> Self {
        self.states.insert(state);
        self
    }
    
    pub fn has(&self, state: PredictableState) -> bool {
        self.states.contains(&state)
    }
    
    pub fn add(&mut self, state: PredictableState) {
        self.states.insert(state);
    }
    
    pub fn remove(&mut self, state: PredictableState) {
        self.states.remove(&state);
    }
    
    pub fn toggle(&mut self, state: PredictableState) {
        if self.states.contains(&state) {
            self.states.remove(&state);
        } else {
            self.states.insert(state);
        }
    }
    
    /// Create a hash for cache lookup
    pub fn to_hash(&self) -> u64 {
        let mut hash = 0u64;
        for state in &self.states {
            hash |= 1 << (*state as u8);
        }
        hash
    }
    
    /// Iterate over states
    pub fn iter(&self) -> impl Iterator<Item = &PredictableState> {
        self.states.iter()
    }
}

// ============================================================================
// Predicted Style Cache
// ============================================================================

/// Cache key for predicted styles
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PredictionKey {
    element_id: u32,
    state_hash: u64,
}

/// Cached predicted style
#[derive(Debug, Clone)]
pub struct PredictedStyle {
    /// Property values (property name -> value)
    pub properties: HashMap<Box<str>, Box<str>>,
    /// Validation token (invalidated on DOM/style changes)
    pub validation_token: u64,
}

/// Cache for predicted styles
#[derive(Debug)]
pub struct PredictedStyleCache {
    /// Cached predictions
    cache: HashMap<PredictionKey, PredictedStyle>,
    /// Maximum cache size
    max_size: usize,
    /// Current validation token
    validation_token: u64,
    /// Statistics
    stats: PredictionCacheStats,
}

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct PredictionCacheStats {
    pub hits: u64,
    pub misses: u64,
    pub predictions: u64,
    pub invalidations: u64,
}

impl PredictionCacheStats {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 { 0.0 } else { self.hits as f64 / total as f64 }
    }
}

impl Default for PredictedStyleCache {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl PredictedStyleCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: HashMap::with_capacity(max_size),
            max_size,
            validation_token: 0,
            stats: PredictionCacheStats::default(),
        }
    }
    
    /// Get a predicted style
    pub fn get(&mut self, element_id: u32, states: &StateSet) -> Option<&PredictedStyle> {
        let key = PredictionKey {
            element_id,
            state_hash: states.to_hash(),
        };
        
        if let Some(predicted) = self.cache.get(&key) {
            if predicted.validation_token == self.validation_token {
                self.stats.hits += 1;
                return Some(predicted);
            }
        }
        
        self.stats.misses += 1;
        None
    }
    
    /// Store a predicted style
    pub fn insert(&mut self, element_id: u32, states: &StateSet, style: PredictedStyle) {
        if self.cache.len() >= self.max_size {
            // Simple eviction: remove oldest (this could be LRU)
            if let Some(key) = self.cache.keys().next().cloned() {
                self.cache.remove(&key);
            }
        }
        
        let key = PredictionKey {
            element_id,
            state_hash: states.to_hash(),
        };
        
        self.cache.insert(key, PredictedStyle {
            validation_token: self.validation_token,
            ..style
        });
        
        self.stats.predictions += 1;
    }
    
    /// Invalidate all predictions
    pub fn invalidate_all(&mut self) {
        self.validation_token += 1;
        self.stats.invalidations += 1;
    }
    
    /// Invalidate predictions for specific elements
    pub fn invalidate_elements(&mut self, element_ids: &[u32]) {
        for id in element_ids {
            self.cache.retain(|k, _| k.element_id != *id);
        }
        self.stats.invalidations += 1;
    }
    
    /// Get statistics
    pub fn stats(&self) -> &PredictionCacheStats {
        &self.stats
    }
    
    /// Clear the cache
    pub fn clear(&mut self) {
        self.cache.clear();
        self.validation_token += 1;
    }
}

// ============================================================================
// Predictive Style Engine
// ============================================================================

/// Element info for prediction
pub trait PredictionContext {
    /// Get element's current computed style
    fn get_computed_style(&self, element_id: u32) -> Option<HashMap<Box<str>, Box<str>>>;
    
    /// Compute style with given states
    fn compute_with_states(
        &self,
        element_id: u32,
        states: &StateSet,
    ) -> Option<HashMap<Box<str>, Box<str>>>;
    
    /// Check if element is interactive (button, link, input, etc.)
    fn is_interactive(&self, element_id: u32) -> bool;
    
    /// Check if element has state-dependent styles
    fn has_state_styles(&self, element_id: u32, state: PredictableState) -> bool;
    
    /// Get visible elements
    fn get_visible_elements(&self) -> Vec<u32>;
}

/// Predictive styling engine
#[derive(Debug)]
pub struct PredictiveStyleEngine {
    /// Prediction cache
    cache: PredictedStyleCache,
    /// Elements queued for prediction
    prediction_queue: VecDeque<PredictionRequest>,
    /// Maximum predictions per idle cycle
    max_predictions_per_cycle: usize,
    /// States to predict
    states_to_predict: Vec<PredictableState>,
}

/// Request for style prediction
#[derive(Debug, Clone)]
struct PredictionRequest {
    element_id: u32,
    state: PredictableState,
    priority: u8,
}

impl Default for PredictiveStyleEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PredictiveStyleEngine {
    pub fn new() -> Self {
        Self {
            cache: PredictedStyleCache::default(),
            prediction_queue: VecDeque::with_capacity(1024),
            max_predictions_per_cycle: 50,
            states_to_predict: PredictableState::INTERACTIVE.to_vec(),
        }
    }
    
    /// Queue elements for prediction
    pub fn queue_for_prediction(&mut self, element_ids: &[u32], context: &dyn PredictionContext) {
        for &element_id in element_ids {
            // Only predict for interactive elements
            if !context.is_interactive(element_id) {
                continue;
            }
            
            for &state in &self.states_to_predict {
                // Only predict if element has state-dependent styles
                if context.has_state_styles(element_id, state) {
                    self.prediction_queue.push_back(PredictionRequest {
                        element_id,
                        state,
                        priority: match state {
                            PredictableState::Hover => 1,
                            PredictableState::Focus => 2,
                            PredictableState::Active => 3,
                            _ => 5,
                        },
                    });
                }
            }
        }
        
        // Sort by priority
        let mut queue: Vec<_> = self.prediction_queue.drain(..).collect();
        queue.sort_by_key(|r| r.priority);
        self.prediction_queue = queue.into();
    }
    
    /// Process predictions during idle time
    pub fn process_idle(&mut self, context: &dyn PredictionContext) -> usize {
        let mut processed = 0;
        
        while processed < self.max_predictions_per_cycle {
            let request = match self.prediction_queue.pop_front() {
                Some(r) => r,
                None => break,
            };
            
            // Check if already cached
            let states = StateSet::new().with_state(request.state);
            if self.cache.get(request.element_id, &states).is_some() {
                continue;
            }
            
            // Compute and cache
            if let Some(style) = context.compute_with_states(request.element_id, &states) {
                self.cache.insert(request.element_id, &states, PredictedStyle {
                    properties: style,
                    validation_token: 0, // Will be set by insert
                });
                processed += 1;
            }
        }
        
        processed
    }
    
    /// Get predicted style for a state change
    pub fn get_predicted_style(
        &mut self,
        element_id: u32,
        states: &StateSet,
    ) -> Option<&PredictedStyle> {
        self.cache.get(element_id, states)
    }
    
    /// Apply predicted style when state changes
    pub fn apply_state_change(
        &mut self,
        element_id: u32,
        new_states: &StateSet,
    ) -> Option<&HashMap<Box<str>, Box<str>>> {
        self.cache.get(element_id, new_states)
            .map(|p| &p.properties)
    }
    
    /// Invalidate predictions on style sheet change
    pub fn invalidate_on_stylesheet_change(&mut self) {
        self.cache.invalidate_all();
        self.prediction_queue.clear();
    }
    
    /// Invalidate predictions on DOM change
    pub fn invalidate_on_dom_change(&mut self, affected_elements: &[u32]) {
        self.cache.invalidate_elements(affected_elements);
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> &PredictionCacheStats {
        self.cache.stats()
    }
    
    /// Number of pending predictions
    pub fn pending_count(&self) -> usize {
        self.prediction_queue.len()
    }
    
    /// Set maximum predictions per cycle
    pub fn set_max_per_cycle(&mut self, max: usize) {
        self.max_predictions_per_cycle = max;
    }
}

// ============================================================================
// Off-Screen Element Prediction
// ============================================================================

/// Predict styles for off-screen elements (e.g., for scroll anchoring)
#[derive(Debug)]
pub struct OffScreenPredictor {
    /// Elements near viewport
    near_viewport: HashSet<u32>,
    /// Prediction distance (pixels from viewport)
    prediction_distance: f32,
}

impl Default for OffScreenPredictor {
    fn default() -> Self {
        Self::new()
    }
}

impl OffScreenPredictor {
    pub fn new() -> Self {
        Self {
            near_viewport: HashSet::new(),
            prediction_distance: 500.0,
        }
    }
    
    /// Update viewport position
    pub fn update_viewport(
        &mut self,
        viewport_top: f32,
        viewport_bottom: f32,
        all_elements: &[(u32, f32, f32)], // (id, top, bottom)
    ) {
        self.near_viewport.clear();
        
        let predict_top = viewport_top - self.prediction_distance;
        let predict_bottom = viewport_bottom + self.prediction_distance;
        
        for &(id, top, bottom) in all_elements {
            if bottom >= predict_top && top <= predict_bottom {
                self.near_viewport.insert(id);
            }
        }
    }
    
    /// Get elements that should have styles predicted
    pub fn get_prediction_targets(&self) -> impl Iterator<Item = u32> + '_ {
        self.near_viewport.iter().copied()
    }
    
    /// Set prediction distance
    pub fn set_distance(&mut self, distance: f32) {
        self.prediction_distance = distance;
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_state_set() {
        let mut states = StateSet::new();
        
        states.add(PredictableState::Hover);
        assert!(states.has(PredictableState::Hover));
        assert!(!states.has(PredictableState::Focus));
        
        states.add(PredictableState::Focus);
        assert!(states.has(PredictableState::Focus));
        
        let hash = states.to_hash();
        assert!(hash != 0);
    }
    
    #[test]
    fn test_prediction_cache() {
        let mut cache = PredictedStyleCache::new(100);
        
        let states = StateSet::new().with_state(PredictableState::Hover);
        let mut props = HashMap::new();
        props.insert("color".into(), "red".into());
        
        cache.insert(1, &states, PredictedStyle {
            properties: props,
            validation_token: 0,
        });
        
        assert!(cache.get(1, &states).is_some());
        assert_eq!(cache.stats().hits, 1);
        
        cache.invalidate_all();
        assert!(cache.get(1, &states).is_none());
    }
    
    #[test]
    fn test_off_screen_predictor() {
        let mut predictor = OffScreenPredictor::new();
        
        let elements = vec![
            (1, 0.0, 100.0),
            (2, 100.0, 200.0),
            (3, 1000.0, 1100.0),
            (4, 2000.0, 2100.0),
        ];
        
        // Viewport from 500 to 600
        predictor.update_viewport(500.0, 600.0, &elements);
        
        let targets: Vec<_> = predictor.get_prediction_targets().collect();
        
        // Element 3 (1000-1100) should be in prediction range (600 + 500 = 1100)
        assert!(targets.contains(&3));
        // Element 4 (2000-2100) should not be in range
        assert!(!targets.contains(&4));
    }
}
