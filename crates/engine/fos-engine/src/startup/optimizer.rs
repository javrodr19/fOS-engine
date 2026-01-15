//! Startup Optimizer
//!
//! Optimizations for fast browser startup.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Subsystem trait for lazy initialization
pub trait Subsystem: Send + Sync {
    /// Initialize the subsystem
    fn init(&self);
    
    /// Get subsystem name
    fn name(&self) -> &'static str;
    
    /// Priority (higher = initialize earlier)
    fn priority(&self) -> u8 {
        50
    }
}

/// Lazy initializer wrapper
pub struct LazyInit<T> {
    /// Initialization function
    init_fn: Option<Box<dyn Fn() -> T + Send + Sync>>,
    /// Cached value
    value: Option<T>,
    /// Initialized flag
    initialized: AtomicBool,
}

impl<T: std::fmt::Debug> std::fmt::Debug for LazyInit<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LazyInit")
            .field("value", &self.value)
            .field("initialized", &self.initialized.load(Ordering::Relaxed))
            .finish()
    }
}

impl<T> LazyInit<T> {
    /// Create new lazy initializer
    pub fn new<F>(init_fn: F) -> Self
    where
        F: Fn() -> T + Send + Sync + 'static,
    {
        Self {
            init_fn: Some(Box::new(init_fn)),
            value: None,
            initialized: AtomicBool::new(false),
        }
    }
    
    /// Get or initialize value
    pub fn get(&mut self) -> &T {
        if !self.initialized.load(Ordering::Acquire) {
            if let Some(ref init_fn) = self.init_fn {
                self.value = Some(init_fn());
                self.initialized.store(true, Ordering::Release);
            }
        }
        self.value.as_ref().expect("LazyInit: value should exist after initialization")
    }
    
    /// Get mutable reference (initializes if needed)
    pub fn get_mut(&mut self) -> &mut T {
        if !self.initialized.load(Ordering::Acquire) {
            if let Some(ref init_fn) = self.init_fn {
                self.value = Some(init_fn());
                self.initialized.store(true, Ordering::Release);
            }
        }
        self.value.as_mut().expect("LazyInit: value should exist after initialization")
    }
    
    /// Check if initialized
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }
    
    /// Force initialization
    pub fn init(&mut self) {
        let _ = self.get();
    }
}

/// Startup phase
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StartupPhase {
    /// Pre-initialization
    PreInit,
    /// Core initialization
    Core,
    /// UI initialization  
    Ui,
    /// Network initialization
    Network,
    /// Post-initialization
    PostInit,
    /// Ready
    Ready,
}

impl StartupPhase {
    pub fn name(&self) -> &'static str {
        match self {
            Self::PreInit => "pre-init",
            Self::Core => "core",
            Self::Ui => "ui",
            Self::Network => "network",
            Self::PostInit => "post-init",
            Self::Ready => "ready",
        }
    }
}

/// Startup timing
#[derive(Debug, Clone, Default)]
pub struct StartupTiming {
    /// Phase start times
    phase_starts: HashMap<String, Instant>,
    /// Phase end times
    phase_ends: HashMap<String, Instant>,
    /// Overall start
    start: Option<Instant>,
    /// First paint time
    first_paint: Option<Instant>,
    /// Fully interactive time
    interactive: Option<Instant>,
}

impl StartupTiming {
    pub fn new() -> Self {
        Self {
            start: Some(Instant::now()),
            ..Default::default()
        }
    }
    
    /// Mark phase start
    pub fn phase_start(&mut self, phase: StartupPhase) {
        self.phase_starts.insert(phase.name().to_string(), Instant::now());
    }
    
    /// Mark phase end
    pub fn phase_end(&mut self, phase: StartupPhase) {
        self.phase_ends.insert(phase.name().to_string(), Instant::now());
    }
    
    /// Mark first paint
    pub fn mark_first_paint(&mut self) {
        self.first_paint = Some(Instant::now());
    }
    
    /// Mark interactive
    pub fn mark_interactive(&mut self) {
        self.interactive = Some(Instant::now());
    }
    
    /// Get time to first paint
    pub fn time_to_first_paint(&self) -> Option<Duration> {
        match (self.start, self.first_paint) {
            (Some(start), Some(fp)) => Some(fp.duration_since(start)),
            _ => None,
        }
    }
    
    /// Get time to interactive
    pub fn time_to_interactive(&self) -> Option<Duration> {
        match (self.start, self.interactive) {
            (Some(start), Some(i)) => Some(i.duration_since(start)),
            _ => None,
        }
    }
    
    /// Get phase duration
    pub fn phase_duration(&self, phase: StartupPhase) -> Option<Duration> {
        let name = phase.name();
        match (self.phase_starts.get(name), self.phase_ends.get(name)) {
            (Some(start), Some(end)) => Some(end.duration_since(*start)),
            _ => None,
        }
    }
    
    /// Get total startup time
    pub fn total_time(&self) -> Option<Duration> {
        self.start.map(|s| s.elapsed())
    }
}

/// Startup optimizer
#[derive(Debug)]
pub struct StartupOptimizer {
    /// Current phase
    current_phase: StartupPhase,
    /// Timing information
    timing: StartupTiming,
    /// Configuration
    config: StartupConfig,
}

/// Startup configuration
#[derive(Debug, Clone)]
pub struct StartupConfig {
    /// Enable prefork (pre-spawn renderer processes)
    pub prefork_enabled: bool,
    /// Number of preforked processes
    pub prefork_count: usize,
    /// Enable lazy subsystem initialization
    pub lazy_init: bool,
    /// Target first paint time (ms)
    pub target_first_paint_ms: u32,
    /// Target interactive time (ms)
    pub target_interactive_ms: u32,
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            prefork_enabled: true,
            prefork_count: 2,
            lazy_init: true,
            target_first_paint_ms: 80,
            target_interactive_ms: 200,
        }
    }
}

impl StartupOptimizer {
    /// Create new optimizer
    pub fn new(config: StartupConfig) -> Self {
        Self {
            current_phase: StartupPhase::PreInit,
            timing: StartupTiming::new(),
            config,
        }
    }
    
    /// Create with default config
    pub fn default_config() -> Self {
        Self::new(StartupConfig::default())
    }
    
    /// Get current phase
    pub fn current_phase(&self) -> StartupPhase {
        self.current_phase
    }
    
    /// Get timing
    pub fn timing(&self) -> &StartupTiming {
        &self.timing
    }
    
    /// Get config
    pub fn config(&self) -> &StartupConfig {
        &self.config
    }
    
    /// Start a phase
    pub fn begin_phase(&mut self, phase: StartupPhase) {
        self.timing.phase_start(phase);
        self.current_phase = phase;
    }
    
    /// End current phase
    pub fn end_phase(&mut self) {
        self.timing.phase_end(self.current_phase);
    }
    
    /// Mark first paint
    pub fn first_paint(&mut self) {
        self.timing.mark_first_paint();
    }
    
    /// Mark interactive
    pub fn interactive(&mut self) {
        self.timing.mark_interactive();
    }
    
    /// Transition to ready
    pub fn ready(&mut self) {
        self.end_phase();
        self.current_phase = StartupPhase::Ready;
    }
    
    /// Is startup complete?
    pub fn is_ready(&self) -> bool {
        self.current_phase == StartupPhase::Ready
    }
    
    /// Check if we met targets
    pub fn met_targets(&self) -> StartupReport {
        let ttfp = self.timing.time_to_first_paint();
        let tti = self.timing.time_to_interactive();
        
        StartupReport {
            time_to_first_paint_ms: ttfp.map(|d| d.as_millis() as u32),
            time_to_interactive_ms: tti.map(|d| d.as_millis() as u32),
            first_paint_target_met: ttfp
                .map(|d| d.as_millis() as u32 <= self.config.target_first_paint_ms)
                .unwrap_or(false),
            interactive_target_met: tti
                .map(|d| d.as_millis() as u32 <= self.config.target_interactive_ms)
                .unwrap_or(false),
        }
    }
}

/// Startup report
#[derive(Debug, Clone, Copy)]
pub struct StartupReport {
    pub time_to_first_paint_ms: Option<u32>,
    pub time_to_interactive_ms: Option<u32>,
    pub first_paint_target_met: bool,
    pub interactive_target_met: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_lazy_init() {
        let mut counter = 0u32;
        let mut lazy = LazyInit::new(|| {
            42
        });
        
        assert!(!lazy.is_initialized());
        
        let val = lazy.get();
        assert_eq!(*val, 42);
        assert!(lazy.is_initialized());
    }
    
    #[test]
    fn test_startup_phases() {
        let mut optimizer = StartupOptimizer::default_config();
        
        assert_eq!(optimizer.current_phase(), StartupPhase::PreInit);
        
        optimizer.begin_phase(StartupPhase::Core);
        assert_eq!(optimizer.current_phase(), StartupPhase::Core);
        
        optimizer.end_phase();
        optimizer.begin_phase(StartupPhase::Ui);
        
        std::thread::sleep(std::time::Duration::from_millis(1));
        optimizer.first_paint();
        
        optimizer.ready();
        assert!(optimizer.is_ready());
    }
    
    #[test]
    fn test_timing() {
        let mut optimizer = StartupOptimizer::default_config();
        
        optimizer.begin_phase(StartupPhase::Core);
        std::thread::sleep(std::time::Duration::from_millis(5));
        optimizer.end_phase();
        
        let dur = optimizer.timing().phase_duration(StartupPhase::Core);
        assert!(dur.is_some());
        assert!(dur.unwrap().as_millis() >= 5);
    }
}
