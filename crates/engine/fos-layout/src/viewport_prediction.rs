//! Viewport Prediction (Phase 24.3)
//!
//! Predict scroll direction. Pre-layout 2 screens ahead.
//! Evict opposite direction. Smooth scrolling guaranteed.

use std::collections::VecDeque;

/// Viewport position
#[derive(Debug, Clone, Copy, Default)]
pub struct ViewportPosition {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ViewportPosition {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    /// Check if a rect is visible in this viewport
    pub fn is_visible(&self, rect: &ViewportPosition) -> bool {
        rect.x < self.x + self.width &&
        rect.x + rect.width > self.x &&
        rect.y < self.y + self.height &&
        rect.y + rect.height > self.y
    }
    
    /// Get expanded viewport (for prefetching)
    pub fn expanded(&self, screens: f32) -> ViewportPosition {
        ViewportPosition {
            x: self.x - self.width * screens,
            y: self.y - self.height * screens,
            width: self.width * (1.0 + 2.0 * screens),
            height: self.height * (1.0 + 2.0 * screens),
        }
    }
    
    /// Get viewport above
    pub fn above(&self, screens: f32) -> ViewportPosition {
        ViewportPosition {
            x: self.x,
            y: self.y - self.height * screens,
            width: self.width,
            height: self.height * screens,
        }
    }
    
    /// Get viewport below
    pub fn below(&self, screens: f32) -> ViewportPosition {
        ViewportPosition {
            x: self.x,
            y: self.y + self.height,
            width: self.width,
            height: self.height * screens,
        }
    }
}

/// Scroll direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
    None,
}

/// Scroll event
#[derive(Debug, Clone, Copy)]
pub struct ScrollEvent {
    pub delta_x: f32,
    pub delta_y: f32,
    pub timestamp_ms: u64,
}

/// Scroll velocity
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollVelocity {
    pub vx: f32,
    pub vy: f32,
}

impl ScrollVelocity {
    pub fn direction(&self) -> ScrollDirection {
        if self.vy.abs() > self.vx.abs() {
            if self.vy > 0.5 {
                ScrollDirection::Down
            } else if self.vy < -0.5 {
                ScrollDirection::Up
            } else {
                ScrollDirection::None
            }
        } else {
            if self.vx > 0.5 {
                ScrollDirection::Right
            } else if self.vx < -0.5 {
                ScrollDirection::Left
            } else {
                ScrollDirection::None
            }
        }
    }
    
    pub fn magnitude(&self) -> f32 {
        (self.vx * self.vx + self.vy * self.vy).sqrt()
    }
}

/// Viewport predictor
#[derive(Debug)]
pub struct ViewportPredictor {
    /// Recent scroll events
    scroll_history: VecDeque<ScrollEvent>,
    /// Maximum history size
    max_history: usize,
    /// Current viewport
    current: ViewportPosition,
    /// Current velocity
    velocity: ScrollVelocity,
    /// Prediction lookahead (screens)
    lookahead_screens: f32,
    /// Statistics
    stats: PredictorStats,
}

/// Predictor statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct PredictorStats {
    pub predictions_made: u64,
    pub predictions_correct: u64,
    pub prefetch_hits: u64,
    pub prefetch_misses: u64,
}

impl PredictorStats {
    pub fn accuracy(&self) -> f64 {
        if self.predictions_made == 0 {
            0.0
        } else {
            self.predictions_correct as f64 / self.predictions_made as f64
        }
    }
}

impl Default for ViewportPredictor {
    fn default() -> Self {
        Self::new()
    }
}

impl ViewportPredictor {
    pub fn new() -> Self {
        Self {
            scroll_history: VecDeque::new(),
            max_history: 10,
            current: ViewportPosition::default(),
            velocity: ScrollVelocity::default(),
            lookahead_screens: 2.0,
            stats: PredictorStats::default(),
        }
    }
    
    /// Set lookahead screens
    pub fn with_lookahead(mut self, screens: f32) -> Self {
        self.lookahead_screens = screens;
        self
    }
    
    /// Update current viewport
    pub fn update_viewport(&mut self, viewport: ViewportPosition) {
        self.current = viewport;
    }
    
    /// Record a scroll event
    pub fn record_scroll(&mut self, event: ScrollEvent) {
        self.scroll_history.push_back(event);
        
        // Trim history
        while self.scroll_history.len() > self.max_history {
            self.scroll_history.pop_front();
        }
        
        // Update velocity
        self.update_velocity();
    }
    
    /// Update velocity from history
    fn update_velocity(&mut self) {
        if self.scroll_history.len() < 2 {
            self.velocity = ScrollVelocity::default();
            return;
        }
        
        // Calculate weighted average velocity
        let mut total_dx = 0.0f32;
        let mut total_dy = 0.0f32;
        let mut total_weight = 0.0f32;
        
        let events: Vec<_> = self.scroll_history.iter().collect();
        for i in 1..events.len() {
            let dt = (events[i].timestamp_ms - events[i-1].timestamp_ms) as f32;
            if dt > 0.0 {
                let weight = 1.0 / (events.len() - i) as f32; // More weight to recent
                total_dx += events[i].delta_x * weight;
                total_dy += events[i].delta_y * weight;
                total_weight += weight;
            }
        }
        
        if total_weight > 0.0 {
            self.velocity = ScrollVelocity {
                vx: total_dx / total_weight,
                vy: total_dy / total_weight,
            };
        }
    }
    
    /// Predict scroll direction
    pub fn predict_direction(&self) -> ScrollDirection {
        self.velocity.direction()
    }
    
    /// Get prefetch region based on prediction
    pub fn prefetch_region(&mut self) -> ViewportPosition {
        self.stats.predictions_made += 1;
        
        let direction = self.predict_direction();
        
        match direction {
            ScrollDirection::Down => self.current.below(self.lookahead_screens),
            ScrollDirection::Up => self.current.above(self.lookahead_screens),
            ScrollDirection::Left | ScrollDirection::Right => {
                // Horizontal scrolling - expand in direction
                self.current.expanded(self.lookahead_screens / 2.0)
            }
            ScrollDirection::None => {
                // No clear direction - expand in all directions
                self.current.expanded(self.lookahead_screens / 2.0)
            }
        }
    }
    
    /// Get eviction region (opposite of scroll direction)
    pub fn eviction_region(&self) -> Option<ViewportPosition> {
        let direction = self.predict_direction();
        
        match direction {
            ScrollDirection::Down => Some(self.current.above(self.lookahead_screens)),
            ScrollDirection::Up => Some(self.current.below(self.lookahead_screens)),
            _ => None, // Don't evict for horizontal/no direction
        }
    }
    
    /// Predict future viewport position
    pub fn predict_viewport(&self, ms_ahead: f32) -> ViewportPosition {
        let factor = ms_ahead / 16.0; // Assuming ~16ms per frame
        
        ViewportPosition {
            x: self.current.x + self.velocity.vx * factor,
            y: self.current.y + self.velocity.vy * factor,
            width: self.current.width,
            height: self.current.height,
        }
    }
    
    /// Record prediction result
    pub fn record_result(&mut self, prediction_correct: bool) {
        if prediction_correct {
            self.stats.predictions_correct += 1;
            self.stats.prefetch_hits += 1;
        } else {
            self.stats.prefetch_misses += 1;
        }
    }
    
    /// Get current velocity
    pub fn velocity(&self) -> ScrollVelocity {
        self.velocity
    }
    
    /// Get statistics
    pub fn stats(&self) -> &PredictorStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scroll_direction() {
        let mut predictor = ViewportPredictor::new();
        
        predictor.update_viewport(ViewportPosition::new(0.0, 0.0, 800.0, 600.0));
        
        // Simulate scrolling down
        for i in 0..5 {
            predictor.record_scroll(ScrollEvent {
                delta_x: 0.0,
                delta_y: 50.0,
                timestamp_ms: (i * 16) as u64,
            });
        }
        
        assert_eq!(predictor.predict_direction(), ScrollDirection::Down);
    }
    
    #[test]
    fn test_prefetch_region() {
        let mut predictor = ViewportPredictor::new().with_lookahead(2.0);
        
        predictor.update_viewport(ViewportPosition::new(0.0, 100.0, 800.0, 600.0));
        
        // Simulate scrolling down
        for i in 0..3 {
            predictor.record_scroll(ScrollEvent {
                delta_x: 0.0,
                delta_y: 30.0,
                timestamp_ms: (i * 16) as u64,
            });
        }
        
        let prefetch = predictor.prefetch_region();
        
        // Should be below current viewport
        assert!(prefetch.y >= 100.0 + 600.0);
    }
    
    #[test]
    fn test_predict_viewport() {
        let mut predictor = ViewportPredictor::new();
        
        predictor.update_viewport(ViewportPosition::new(0.0, 0.0, 800.0, 600.0));
        
        // Set velocity
        for i in 0..5 {
            predictor.record_scroll(ScrollEvent {
                delta_x: 0.0,
                delta_y: 16.0,
                timestamp_ms: (i * 16) as u64,
            });
        }
        
        let future = predictor.predict_viewport(100.0);
        
        // Should be lower than current
        assert!(future.y > 0.0);
    }
}
