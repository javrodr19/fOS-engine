//! Scroll Behavior API
//!
//! Smooth scrolling and scroll snap.

/// Scroll behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollBehavior {
    #[default]
    Auto,
    Smooth,
    Instant,
}

/// Scroll position
#[derive(Debug, Clone, Copy, Default)]
pub struct ScrollPosition {
    pub x: f32,
    pub y: f32,
}

/// Scroll options
#[derive(Debug, Clone, Default)]
pub struct ScrollOptions {
    pub top: Option<f32>,
    pub left: Option<f32>,
    pub behavior: ScrollBehavior,
}

/// Scroll into view options
#[derive(Debug, Clone, Default)]
pub struct ScrollIntoViewOptions {
    pub behavior: ScrollBehavior,
    pub block: ScrollLogicalPosition,
    pub inline: ScrollLogicalPosition,
}

/// Logical scroll position
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollLogicalPosition {
    Start,
    Center,
    End,
    #[default]
    Nearest,
}

/// Scroll snap type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollSnapType {
    #[default]
    None,
    X,
    Y,
    Both,
}

/// Scroll snap strictness
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScrollSnapStrictness {
    #[default]
    Proximity,
    Mandatory,
}

/// Scroll manager
#[derive(Debug, Default)]
pub struct ScrollManager {
    position: ScrollPosition,
    smooth_target: Option<ScrollPosition>,
    animation_progress: f32,
    snap_type: ScrollSnapType,
    snap_strictness: ScrollSnapStrictness,
    snap_points: Vec<ScrollPosition>,
}

impl ScrollManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get current scroll position
    pub fn position(&self) -> ScrollPosition {
        self.position
    }
    
    /// Scroll to position
    pub fn scroll_to(&mut self, options: ScrollOptions) {
        let target = ScrollPosition {
            x: options.left.unwrap_or(self.position.x),
            y: options.top.unwrap_or(self.position.y),
        };
        
        match options.behavior {
            ScrollBehavior::Instant | ScrollBehavior::Auto => {
                self.position = target;
                self.smooth_target = None;
            }
            ScrollBehavior::Smooth => {
                self.smooth_target = Some(target);
                self.animation_progress = 0.0;
            }
        }
    }
    
    /// Scroll by delta
    pub fn scroll_by(&mut self, dx: f32, dy: f32, behavior: ScrollBehavior) {
        self.scroll_to(ScrollOptions {
            left: Some(self.position.x + dx),
            top: Some(self.position.y + dy),
            behavior,
        });
    }
    
    /// Update smooth scroll animation
    pub fn update(&mut self, delta_ms: f32) -> bool {
        if let Some(target) = self.smooth_target {
            self.animation_progress += delta_ms / 300.0; // 300ms duration
            
            if self.animation_progress >= 1.0 {
                self.position = target;
                self.smooth_target = None;
                return true;
            }
            
            // Ease-out interpolation
            let t = 1.0 - (1.0 - self.animation_progress).powi(3);
            self.position.x += (target.x - self.position.x) * t;
            self.position.y += (target.y - self.position.y) * t;
            
            return true;
        }
        false
    }
    
    /// Check if smooth scrolling
    pub fn is_scrolling(&self) -> bool {
        self.smooth_target.is_some()
    }
    
    /// Add snap point
    pub fn add_snap_point(&mut self, point: ScrollPosition) {
        self.snap_points.push(point);
    }
    
    /// Clear snap points
    pub fn clear_snap_points(&mut self) {
        self.snap_points.clear();
    }
    
    /// Set snap type
    pub fn set_snap_type(&mut self, snap_type: ScrollSnapType, strictness: ScrollSnapStrictness) {
        self.snap_type = snap_type;
        self.snap_strictness = strictness;
    }
    
    /// Find nearest snap point
    pub fn snap(&mut self) {
        if self.snap_type == ScrollSnapType::None || self.snap_points.is_empty() {
            return;
        }
        
        let mut nearest = self.snap_points[0];
        let mut min_dist = f32::MAX;
        
        for point in &self.snap_points {
            let dist = match self.snap_type {
                ScrollSnapType::X => (point.x - self.position.x).abs(),
                ScrollSnapType::Y => (point.y - self.position.y).abs(),
                ScrollSnapType::Both => {
                    ((point.x - self.position.x).powi(2) + (point.y - self.position.y).powi(2)).sqrt()
                }
                ScrollSnapType::None => continue,
            };
            
            if dist < min_dist {
                min_dist = dist;
                nearest = *point;
            }
        }
        
        // Only snap if close enough for proximity
        if self.snap_strictness == ScrollSnapStrictness::Proximity && min_dist > 50.0 {
            return;
        }
        
        self.scroll_to(ScrollOptions {
            left: Some(nearest.x),
            top: Some(nearest.y),
            behavior: ScrollBehavior::Smooth,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scroll_to() {
        let mut mgr = ScrollManager::new();
        
        mgr.scroll_to(ScrollOptions {
            left: Some(100.0),
            top: Some(200.0),
            behavior: ScrollBehavior::Instant,
        });
        
        let pos = mgr.position();
        assert_eq!(pos.x, 100.0);
        assert_eq!(pos.y, 200.0);
    }
    
    #[test]
    fn test_smooth_scroll() {
        let mut mgr = ScrollManager::new();
        
        mgr.scroll_to(ScrollOptions {
            left: Some(100.0),
            top: Some(0.0),
            behavior: ScrollBehavior::Smooth,
        });
        
        assert!(mgr.is_scrolling());
        
        // Simulate animation
        for _ in 0..30 {
            mgr.update(16.0);
        }
        
        let pos = mgr.position();
        assert!(pos.x > 90.0); // Should be close to target
    }
}
