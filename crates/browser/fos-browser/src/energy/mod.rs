//! Energy Optimization Module
//!
//! Power-aware features for reduced energy consumption.
//! Targets 40% less power consumption than Chromium on mobile/laptop.
//!
//! # Features
//! - Adaptive frame rate based on battery status and content type
//! - GPU power state management
//! - Background tab throttling (timer and network)
//! - CPU frequency hints and workload classification
//! - Wake lock management with auto-release
//! - Media power optimization (hardware decode priority)

pub mod adaptive_renderer;
pub mod gpu_power;
pub mod tab_throttler;
pub mod network_throttle;
pub mod workload;
pub mod scheduler;
pub mod wake_lock;
pub mod idle_detector;
pub mod media_power;

pub use adaptive_renderer::{AdaptiveRenderer, BatteryStatus, ContentType};
pub use gpu_power::{GpuPowerManager, GpuPowerState};
pub use tab_throttler::{TabThrottler, ThrottleLevel};
pub use network_throttle::{BackgroundNetworkPolicy, NetworkThrottler};
pub use workload::{WorkloadType, WorkloadClassifier};
pub use scheduler::{EnergyAwareScheduler, Urgency, CoreType};
pub use wake_lock::{WakeLockManager, WakeLockReason, WakeLockGuard};
pub use idle_detector::{IdleDetector, IdleState};
pub use media_power::{MediaPowerManager, DecoderType, QualityLevel};

use std::time::{Duration, Instant};

/// Central energy manager coordinating all power-saving features
#[derive(Debug)]
pub struct EnergyManager {
    /// Adaptive renderer for frame rate control
    pub adaptive_renderer: AdaptiveRenderer,
    /// GPU power manager
    pub gpu_power: GpuPowerManager,
    /// Tab timer throttler
    pub tab_throttler: TabThrottler,
    /// Network throttler
    pub network_throttler: NetworkThrottler,
    /// Workload classifier
    pub workload_classifier: WorkloadClassifier,
    /// Energy-aware scheduler
    pub scheduler: EnergyAwareScheduler,
    /// Wake lock manager
    pub wake_locks: WakeLockManager,
    /// Idle detector
    pub idle_detector: IdleDetector,
    /// Media power manager
    pub media_power: MediaPowerManager,
    /// Last update time
    last_update: Instant,
}

impl Default for EnergyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EnergyManager {
    /// Create a new energy manager
    pub fn new() -> Self {
        Self {
            adaptive_renderer: AdaptiveRenderer::new(),
            gpu_power: GpuPowerManager::new(),
            tab_throttler: TabThrottler::new(),
            network_throttler: NetworkThrottler::new(),
            workload_classifier: WorkloadClassifier::new(),
            scheduler: EnergyAwareScheduler::new(),
            wake_locks: WakeLockManager::new(),
            idle_detector: IdleDetector::new(),
            media_power: MediaPowerManager::new(),
            last_update: Instant::now(),
        }
    }
    
    /// Update energy state based on current conditions
    pub fn update(&mut self) {
        let now = Instant::now();
        
        // Update workload classification
        let workload = self.workload_classifier.classify();
        
        // Adjust GPU power based on workload
        match workload {
            WorkloadType::Idle => {
                self.gpu_power.set_power_state(GpuPowerState::Idle);
            }
            WorkloadType::LightBrowsing => {
                self.gpu_power.set_power_state(GpuPowerState::LowPower);
            }
            _ => {
                self.gpu_power.set_power_state(GpuPowerState::Active);
            }
        }
        
        // Check for idle state
        if self.idle_detector.detect_idle() {
            self.gpu_power.set_power_state(GpuPowerState::Off);
        }
        
        // Update wake locks
        self.wake_locks.cleanup_expired();
        
        self.last_update = now;
    }
    
    /// Get current power consumption estimate (watts)
    pub fn estimated_power(&self) -> f32 {
        let mut power = 0.0;
        
        // GPU contribution
        power += match self.gpu_power.state() {
            GpuPowerState::Off => 0.0,
            GpuPowerState::Idle => 0.5,
            GpuPowerState::LowPower => 2.0,
            GpuPowerState::Active => 5.0,
        };
        
        // CPU contribution based on workload
        power += match self.workload_classifier.current_workload() {
            WorkloadType::Idle => 1.0,
            WorkloadType::LightBrowsing => 3.0,
            WorkloadType::Interactive => 5.0,
            WorkloadType::HeavyProcessing => 10.0,
            WorkloadType::MediaPlayback => 4.0,
        };
        
        power
    }
    
    /// Get target frame rate based on current conditions
    pub fn target_fps(&self) -> u32 {
        self.adaptive_renderer.get_target_fps()
    }
    
    /// Check if frame should be skipped
    pub fn should_skip_frame(&self) -> bool {
        self.adaptive_renderer.should_skip_frame()
    }
}

/// Energy statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct EnergyStats {
    /// Average power consumption (watts)
    pub avg_power_watts: f32,
    /// Frames skipped due to energy optimization
    pub frames_skipped: u64,
    /// Time spent in low power mode
    pub low_power_time_ms: u64,
    /// Time spent idle
    pub idle_time_ms: u64,
    /// Wake locks acquired
    pub wake_locks_acquired: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_energy_manager_creation() {
        let manager = EnergyManager::new();
        assert_eq!(manager.target_fps(), 60); // Default FPS
    }
    
    #[test]
    fn test_power_estimation() {
        let manager = EnergyManager::new();
        let power = manager.estimated_power();
        assert!(power > 0.0);
        assert!(power < 20.0); // Reasonable upper bound
    }
}
