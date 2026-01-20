//! Adaptive Renderer
//!
//! Adjusts frame rate based on battery status and content type.

use std::time::{Duration, Instant};

/// Battery status levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BatteryStatus {
    /// Plugged in, charging
    Charging,
    /// Battery level > 50%
    #[default]
    Normal,
    /// Battery level 20-50%
    Low,
    /// Battery level < 20%
    Critical,
}

impl BatteryStatus {
    /// Create from battery level (0.0 to 1.0)
    pub fn from_level(level: f64, charging: bool) -> Self {
        if charging {
            Self::Charging
        } else if level < 0.2 {
            Self::Critical
        } else if level < 0.5 {
            Self::Low
        } else {
            Self::Normal
        }
    }
}

/// Content type affecting frame rate decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentType {
    /// Static page, no animations
    #[default]
    Static,
    /// CSS/JS animations active
    Animation,
    /// Video playback
    Video,
    /// User interaction (scrolling, typing)
    Interactive,
}

/// Adaptive renderer for power-efficient frame rate
#[derive(Debug)]
pub struct AdaptiveRenderer {
    /// Target frame rate
    target_fps: u32,
    /// Current battery status
    battery_status: BatteryStatus,
    /// Current content type
    content_type: ContentType,
    /// Has pending animations
    has_pending_animations: bool,
    /// Has pending paints
    has_pending_paints: bool,
    /// Has recent user input
    has_user_input: bool,
    /// Last frame time
    last_frame: Instant,
    /// Last user input time
    last_input: Instant,
    /// Frames rendered
    frames_rendered: u64,
    /// Frames skipped
    frames_skipped: u64,
}

impl Default for AdaptiveRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl AdaptiveRenderer {
    /// Create a new adaptive renderer
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            target_fps: 60,
            battery_status: BatteryStatus::Normal,
            content_type: ContentType::Static,
            has_pending_animations: false,
            has_pending_paints: false,
            has_user_input: false,
            last_frame: now,
            last_input: now,
            frames_rendered: 0,
            frames_skipped: 0,
        }
    }
    
    /// Update battery status
    pub fn set_battery_status(&mut self, status: BatteryStatus) {
        self.battery_status = status;
    }
    
    /// Update battery from level and charging state
    pub fn update_battery(&mut self, level: f64, charging: bool) {
        self.battery_status = BatteryStatus::from_level(level, charging);
    }
    
    /// Set content type
    pub fn set_content_type(&mut self, content_type: ContentType) {
        self.content_type = content_type;
    }
    
    /// Mark animation pending
    pub fn set_pending_animations(&mut self, pending: bool) {
        self.has_pending_animations = pending;
        if pending {
            self.content_type = ContentType::Animation;
        }
    }
    
    /// Mark paint pending
    pub fn set_pending_paints(&mut self, pending: bool) {
        self.has_pending_paints = pending;
    }
    
    /// Record user input
    pub fn record_user_input(&mut self) {
        self.has_user_input = true;
        self.last_input = Instant::now();
        self.content_type = ContentType::Interactive;
    }
    
    /// Get target FPS based on battery and content
    pub fn get_target_fps(&self) -> u32 {
        match (self.battery_status, self.content_type) {
            // Critical battery: always low FPS
            (BatteryStatus::Critical, _) => 30,
            // Low battery with static content: low FPS
            (BatteryStatus::Low, ContentType::Static) => 30,
            // Static content: can reduce FPS
            (_, ContentType::Static) => 30,
            // Video needs 60fps for smooth playback
            (_, ContentType::Video) => 60,
            // Animation needs smooth rendering
            (_, ContentType::Animation) => 60,
            // Interactive needs responsiveness
            (_, ContentType::Interactive) => 60,
        }
    }
    
    /// Check if frame should be skipped
    pub fn should_skip_frame(&self) -> bool {
        // Never skip if there's pending work
        if self.has_pending_animations || self.has_pending_paints {
            return false;
        }
        
        // Never skip with recent user input
        if self.last_input.elapsed() < Duration::from_millis(100) {
            return false;
        }
        
        // Skip for static content with no changes
        self.content_type == ContentType::Static && !self.has_user_input
    }
    
    /// Get frame interval based on target FPS
    pub fn frame_interval(&self) -> Duration {
        Duration::from_micros(1_000_000 / self.get_target_fps() as u64)
    }
    
    /// Check if enough time has passed for next frame
    pub fn should_render_frame(&self) -> bool {
        self.last_frame.elapsed() >= self.frame_interval()
    }
    
    /// Mark frame as rendered
    pub fn frame_rendered(&mut self) {
        self.last_frame = Instant::now();
        self.frames_rendered += 1;
        // Clear user input flag after some time
        if self.last_input.elapsed() > Duration::from_millis(100) {
            self.has_user_input = false;
        }
    }
    
    /// Mark frame as skipped
    pub fn frame_skipped(&mut self) {
        self.frames_skipped += 1;
    }
    
    /// Get frame statistics
    pub fn stats(&self) -> AdaptiveStats {
        AdaptiveStats {
            frames_rendered: self.frames_rendered,
            frames_skipped: self.frames_skipped,
            current_fps: self.get_target_fps(),
            battery_status: self.battery_status,
            content_type: self.content_type,
        }
    }
    
    /// Reset state for new page
    pub fn reset(&mut self) {
        self.content_type = ContentType::Static;
        self.has_pending_animations = false;
        self.has_pending_paints = false;
        self.has_user_input = false;
    }
}

/// Adaptive renderer statistics
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveStats {
    pub frames_rendered: u64,
    pub frames_skipped: u64,
    pub current_fps: u32,
    pub battery_status: BatteryStatus,
    pub content_type: ContentType,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_battery_status_from_level() {
        assert_eq!(BatteryStatus::from_level(0.1, false), BatteryStatus::Critical);
        assert_eq!(BatteryStatus::from_level(0.3, false), BatteryStatus::Low);
        assert_eq!(BatteryStatus::from_level(0.8, false), BatteryStatus::Normal);
        assert_eq!(BatteryStatus::from_level(0.1, true), BatteryStatus::Charging);
    }
    
    #[test]
    fn test_target_fps_static() {
        let renderer = AdaptiveRenderer::new();
        assert_eq!(renderer.get_target_fps(), 30); // Static content
    }
    
    #[test]
    fn test_target_fps_animation() {
        let mut renderer = AdaptiveRenderer::new();
        renderer.set_content_type(ContentType::Animation);
        assert_eq!(renderer.get_target_fps(), 60);
    }
    
    #[test]
    fn test_target_fps_low_battery() {
        let mut renderer = AdaptiveRenderer::new();
        renderer.set_battery_status(BatteryStatus::Critical);
        renderer.set_content_type(ContentType::Animation);
        assert_eq!(renderer.get_target_fps(), 30);
    }
    
    #[test]
    fn test_should_skip_frame() {
        let mut renderer = AdaptiveRenderer::new();
        // Static content with no input should skip
        assert!(renderer.should_skip_frame());
        
        // With pending animations, don't skip
        renderer.set_pending_animations(true);
        assert!(!renderer.should_skip_frame());
    }
    
    #[test]
    fn test_frame_interval() {
        let renderer = AdaptiveRenderer::new();
        let interval = renderer.frame_interval();
        // At 30 FPS, interval should be ~33ms
        assert!(interval.as_millis() >= 30);
        assert!(interval.as_millis() <= 40);
    }
}
