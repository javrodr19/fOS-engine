//! Progressive Fidelity Rendering (Phase 24.1)
//!
//! Multi-pass rendering that prioritizes interactivity:
//! - Pass 1: Solid boxes (1ms, interactive)
//! - Pass 2: Borders, images (5ms)
//! - Pass 3: Subpixel text, shadows (20ms)
//!
//! Interrupt on scroll and restart from pass 1 for smooth scrolling.

use std::time::{Duration, Instant};

/// Render fidelity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum FidelityLevel {
    /// Pass 1: Solid boxes only (fastest, ~1ms)
    Boxes = 0,
    /// Pass 2: Add borders and basic images (~5ms)
    Borders = 1,
    /// Pass 3: Add shadows and gradients (~10ms)
    Shadows = 2,
    /// Pass 4: Full fidelity with subpixel text (~20ms)
    Full = 3,
}

impl FidelityLevel {
    /// Get target time budget for this level
    pub fn target_time(&self) -> Duration {
        match self {
            FidelityLevel::Boxes => Duration::from_millis(1),
            FidelityLevel::Borders => Duration::from_millis(5),
            FidelityLevel::Shadows => Duration::from_millis(10),
            FidelityLevel::Full => Duration::from_millis(20),
        }
    }
    
    /// Get next fidelity level
    pub fn next(self) -> Option<Self> {
        match self {
            FidelityLevel::Boxes => Some(FidelityLevel::Borders),
            FidelityLevel::Borders => Some(FidelityLevel::Shadows),
            FidelityLevel::Shadows => Some(FidelityLevel::Full),
            FidelityLevel::Full => None,
        }
    }
    
    /// Get previous fidelity level
    pub fn prev(self) -> Option<Self> {
        match self {
            FidelityLevel::Boxes => None,
            FidelityLevel::Borders => Some(FidelityLevel::Boxes),
            FidelityLevel::Shadows => Some(FidelityLevel::Borders),
            FidelityLevel::Full => Some(FidelityLevel::Shadows),
        }
    }
    
    /// All levels from lowest to highest
    pub const ALL: [FidelityLevel; 4] = [
        FidelityLevel::Boxes,
        FidelityLevel::Borders,
        FidelityLevel::Shadows,
        FidelityLevel::Full,
    ];
}

impl Default for FidelityLevel {
    fn default() -> Self {
        FidelityLevel::Boxes
    }
}

/// Render features enabled at each fidelity level
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderFeatures {
    /// Solid background colors
    pub backgrounds: bool,
    /// Borders
    pub borders: bool,
    /// Images (basic quality)
    pub images: bool,
    /// Box shadows
    pub box_shadows: bool,
    /// Text shadows
    pub text_shadows: bool,
    /// Gradients
    pub gradients: bool,
    /// Subpixel text rendering
    pub subpixel_text: bool,
    /// Filters (blur, etc.)
    pub filters: bool,
    /// Blend modes
    pub blend_modes: bool,
}

impl RenderFeatures {
    /// Get features for a fidelity level
    pub fn for_level(level: FidelityLevel) -> Self {
        match level {
            FidelityLevel::Boxes => Self {
                backgrounds: true,
                ..Default::default()
            },
            FidelityLevel::Borders => Self {
                backgrounds: true,
                borders: true,
                images: true,
                ..Default::default()
            },
            FidelityLevel::Shadows => Self {
                backgrounds: true,
                borders: true,
                images: true,
                box_shadows: true,
                gradients: true,
                ..Default::default()
            },
            FidelityLevel::Full => Self {
                backgrounds: true,
                borders: true,
                images: true,
                box_shadows: true,
                text_shadows: true,
                gradients: true,
                subpixel_text: true,
                filters: true,
                blend_modes: true,
            },
        }
    }
}

/// Interrupt signal for progressive rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptReason {
    /// User scrolled
    Scroll,
    /// User interacted (click, key, etc.)
    Interaction,
    /// Time budget exceeded
    TimeExceeded,
    /// New frame requested
    NewFrame,
}

/// Interrupt checker trait
pub trait InterruptChecker {
    /// Check if rendering should be interrupted
    fn should_interrupt(&self) -> Option<InterruptReason>;
}

/// Simple time-based interrupt checker
pub struct TimeInterruptChecker {
    start: Instant,
    budget: Duration,
}

impl TimeInterruptChecker {
    pub fn new(budget: Duration) -> Self {
        Self {
            start: Instant::now(),
            budget,
        }
    }
}

impl InterruptChecker for TimeInterruptChecker {
    fn should_interrupt(&self) -> Option<InterruptReason> {
        if self.start.elapsed() > self.budget {
            Some(InterruptReason::TimeExceeded)
        } else {
            None
        }
    }
}

/// No-op interrupt checker (for benchmarking)
pub struct NoInterrupt;

impl InterruptChecker for NoInterrupt {
    fn should_interrupt(&self) -> Option<InterruptReason> {
        None
    }
}

/// Render pass result
#[derive(Debug, Clone)]
pub struct PassResult {
    /// Which level was completed
    pub level: FidelityLevel,
    /// Time taken for this pass
    pub time: Duration,
    /// Number of elements rendered
    pub elements_rendered: u32,
    /// Whether pass was interrupted
    pub interrupted: Option<InterruptReason>,
}

/// Progressive render state
#[derive(Debug, Clone)]
pub struct ProgressiveRenderState {
    /// Current fidelity level
    pub current_level: FidelityLevel,
    /// Results from each completed pass
    pub passes: Vec<PassResult>,
    /// Total time spent
    pub total_time: Duration,
    /// Is rendering complete?
    pub complete: bool,
}

impl Default for ProgressiveRenderState {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressiveRenderState {
    pub fn new() -> Self {
        Self {
            current_level: FidelityLevel::Boxes,
            passes: Vec::new(),
            total_time: Duration::ZERO,
            complete: false,
        }
    }
    
    /// Reset to start fresh
    pub fn reset(&mut self) {
        self.current_level = FidelityLevel::Boxes;
        self.passes.clear();
        self.total_time = Duration::ZERO;
        self.complete = false;
    }
    
    /// Record a completed pass
    pub fn record_pass(&mut self, result: PassResult) {
        self.total_time += result.time;
        if result.interrupted.is_none() {
            if let Some(next) = result.level.next() {
                self.current_level = next;
            } else {
                self.complete = true;
            }
        }
        self.passes.push(result);
    }
    
    /// Handle interrupt (reset to boxes)
    pub fn handle_interrupt(&mut self, reason: InterruptReason) {
        self.current_level = FidelityLevel::Boxes;
        self.complete = false;
        // Keep pass history for statistics
        self.passes.push(PassResult {
            level: self.current_level,
            time: Duration::ZERO,
            elements_rendered: 0,
            interrupted: Some(reason),
        });
    }
    
    /// Get the features to use for current pass
    pub fn current_features(&self) -> RenderFeatures {
        RenderFeatures::for_level(self.current_level)
    }
}

/// Progressive renderer coordinator
pub struct ProgressiveRenderer {
    /// Current state
    state: ProgressiveRenderState,
    /// Statistics
    stats: RenderStats,
}

/// Render statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct RenderStats {
    /// Total frames started
    pub frames_started: u64,
    /// Total frames completed to full fidelity
    pub frames_completed: u64,
    /// Total interrupts
    pub interrupts: u64,
    /// Average time to first paint (Pass 1)
    pub avg_first_paint_us: u64,
    /// Average time to full fidelity
    pub avg_full_paint_us: u64,
}

impl RenderStats {
    /// Update averages (rolling average)
    pub fn update_first_paint(&mut self, time: Duration) {
        let us = time.as_micros() as u64;
        if self.avg_first_paint_us == 0 {
            self.avg_first_paint_us = us;
        } else {
            // Exponential moving average
            self.avg_first_paint_us = (self.avg_first_paint_us * 7 + us) / 8;
        }
    }
    
    pub fn update_full_paint(&mut self, time: Duration) {
        let us = time.as_micros() as u64;
        if self.avg_full_paint_us == 0 {
            self.avg_full_paint_us = us;
        } else {
            self.avg_full_paint_us = (self.avg_full_paint_us * 7 + us) / 8;
        }
    }
}

impl Default for ProgressiveRenderer {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressiveRenderer {
    pub fn new() -> Self {
        Self {
            state: ProgressiveRenderState::new(),
            stats: RenderStats::default(),
        }
    }
    
    /// Start a new frame
    pub fn start_frame(&mut self) {
        self.state.reset();
        self.stats.frames_started += 1;
    }
    
    /// Handle an interrupt
    pub fn interrupt(&mut self, reason: InterruptReason) {
        self.state.handle_interrupt(reason);
        self.stats.interrupts += 1;
    }
    
    /// Get current state
    pub fn state(&self) -> &ProgressiveRenderState {
        &self.state
    }
    
    /// Get current state mutably
    pub fn state_mut(&mut self) -> &mut ProgressiveRenderState {
        &mut self.state
    }
    
    /// Get statistics
    pub fn stats(&self) -> &RenderStats {
        &self.stats
    }
    
    /// Record pass completion
    pub fn complete_pass(&mut self, result: PassResult) {
        // Update stats
        if result.level == FidelityLevel::Boxes && result.interrupted.is_none() {
            self.stats.update_first_paint(result.time);
        }
        
        self.state.record_pass(result);
        
        if self.state.complete {
            self.stats.frames_completed += 1;
            self.stats.update_full_paint(self.state.total_time);
        }
    }
    
    /// Check if more passes are needed
    pub fn needs_more_passes(&self) -> bool {
        !self.state.complete
    }
    
    /// Get features for next pass
    pub fn next_pass_features(&self) -> RenderFeatures {
        self.state.current_features()
    }
}

/// Render element classification for progressive rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ElementComplexity {
    /// Simple box (pass 1)
    Simple = 0,
    /// Has borders or images (pass 2)
    Medium = 1,
    /// Has shadows or gradients (pass 3)
    Complex = 2,
    /// Has filters or blend modes (pass 4)
    VeryComplex = 3,
}

impl ElementComplexity {
    /// Minimum fidelity level needed to fully render this element
    pub fn minimum_level(self) -> FidelityLevel {
        match self {
            ElementComplexity::Simple => FidelityLevel::Boxes,
            ElementComplexity::Medium => FidelityLevel::Borders,
            ElementComplexity::Complex => FidelityLevel::Shadows,
            ElementComplexity::VeryComplex => FidelityLevel::Full,
        }
    }
    
    /// Should this element be rendered at the given level?
    pub fn should_render_at(self, level: FidelityLevel) -> bool {
        (self as u8) <= (level as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fidelity_levels() {
        assert!(FidelityLevel::Full > FidelityLevel::Boxes);
        assert_eq!(FidelityLevel::Boxes.next(), Some(FidelityLevel::Borders));
        assert_eq!(FidelityLevel::Full.next(), None);
    }
    
    #[test]
    fn test_render_features() {
        let boxes = RenderFeatures::for_level(FidelityLevel::Boxes);
        assert!(boxes.backgrounds);
        assert!(!boxes.borders);
        
        let full = RenderFeatures::for_level(FidelityLevel::Full);
        assert!(full.subpixel_text);
        assert!(full.filters);
    }
    
    #[test]
    fn test_progressive_state() {
        let mut state = ProgressiveRenderState::new();
        
        assert_eq!(state.current_level, FidelityLevel::Boxes);
        assert!(!state.complete);
        
        // Complete first pass
        state.record_pass(PassResult {
            level: FidelityLevel::Boxes,
            time: Duration::from_millis(1),
            elements_rendered: 100,
            interrupted: None,
        });
        
        assert_eq!(state.current_level, FidelityLevel::Borders);
        
        // Interrupt
        state.handle_interrupt(InterruptReason::Scroll);
        assert_eq!(state.current_level, FidelityLevel::Boxes);
    }
    
    #[test]
    fn test_progressive_renderer() {
        let mut renderer = ProgressiveRenderer::new();
        
        renderer.start_frame();
        assert!(renderer.needs_more_passes());
        
        // Complete all passes
        for level in FidelityLevel::ALL {
            renderer.complete_pass(PassResult {
                level,
                time: Duration::from_millis(5),
                elements_rendered: 50,
                interrupted: None,
            });
        }
        
        assert!(!renderer.needs_more_passes());
        assert_eq!(renderer.stats().frames_completed, 1);
    }
    
    #[test]
    fn test_element_complexity() {
        assert!(ElementComplexity::Simple.should_render_at(FidelityLevel::Boxes));
        assert!(!ElementComplexity::Complex.should_render_at(FidelityLevel::Boxes));
        assert!(ElementComplexity::Complex.should_render_at(FidelityLevel::Shadows));
    }
    
    #[test]
    fn test_time_interrupt() {
        let checker = TimeInterruptChecker::new(Duration::from_nanos(1));
        std::thread::sleep(Duration::from_micros(10));
        assert!(checker.should_interrupt().is_some());
    }
}
