//! GPU Power Management
//!
//! Controls GPU power states for energy efficiency.

use std::time::{Duration, Instant};

/// GPU power states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GpuPowerState {
    /// Full performance, all clocks active
    #[default]
    Active,
    /// Reduced clock speeds
    LowPower,
    /// Minimal power, ready to resume
    Idle,
    /// GPU context released, maximum savings
    Off,
}

impl GpuPowerState {
    /// Get approximate power multiplier (1.0 = full power)
    pub fn power_multiplier(&self) -> f32 {
        match self {
            Self::Active => 1.0,
            Self::LowPower => 0.5,
            Self::Idle => 0.1,
            Self::Off => 0.0,
        }
    }
    
    /// Get resume time estimate
    pub fn resume_time(&self) -> Duration {
        match self {
            Self::Active => Duration::ZERO,
            Self::LowPower => Duration::from_millis(1),
            Self::Idle => Duration::from_millis(10),
            Self::Off => Duration::from_millis(100),
        }
    }
}

/// GPU power manager
#[derive(Debug)]
pub struct GpuPowerManager {
    /// Current power state
    current_state: GpuPowerState,
    /// Target power state
    target_state: GpuPowerState,
    /// Last state change time
    last_change: Instant,
    /// Minimum time before state change
    state_change_delay: Duration,
    /// Context is initialized
    context_initialized: bool,
    /// Transitions count
    transitions: u64,
    /// Time in each state
    time_in_state: [Duration; 4],
    state_enter_time: Instant,
}

impl Default for GpuPowerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GpuPowerManager {
    /// Create a new GPU power manager
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            current_state: GpuPowerState::Idle,
            target_state: GpuPowerState::Idle,
            last_change: now,
            state_change_delay: Duration::from_millis(100),
            context_initialized: false,
            transitions: 0,
            time_in_state: [Duration::ZERO; 4],
            state_enter_time: now,
        }
    }
    
    /// Get current power state
    pub fn state(&self) -> GpuPowerState {
        self.current_state
    }
    
    /// Request a power state change
    pub fn set_power_state(&mut self, state: GpuPowerState) {
        self.target_state = state;
        
        // Apply immediately if enough time has passed
        if self.last_change.elapsed() >= self.state_change_delay {
            self.apply_state_change();
        }
    }
    
    /// Apply pending state change
    fn apply_state_change(&mut self) {
        if self.current_state == self.target_state {
            return;
        }
        
        // Record time in previous state
        let state_idx = self.current_state as usize;
        self.time_in_state[state_idx] += self.state_enter_time.elapsed();
        
        match self.target_state {
            GpuPowerState::Off => self.release_context(),
            GpuPowerState::Idle => self.flush_and_idle(),
            GpuPowerState::LowPower => self.set_low_clocks(),
            GpuPowerState::Active => self.set_full_clocks(),
        }
        
        self.current_state = self.target_state;
        self.last_change = Instant::now();
        self.state_enter_time = Instant::now();
        self.transitions += 1;
    }
    
    /// Release GPU context for maximum power savings
    fn release_context(&mut self) {
        self.context_initialized = false;
        // In a real implementation, this would release the GPU context
        // wgpu::Device::destroy() or similar
    }
    
    /// Flush work and enter idle state
    fn flush_and_idle(&mut self) {
        // In a real implementation:
        // - Flush all pending work
        // - Allow GPU to enter power-saving idle
    }
    
    /// Set reduced clock speeds
    fn set_low_clocks(&mut self) {
        // Platform-specific:
        // - On desktop: hint to driver for power saving
        // - On mobile: may use platform-specific APIs
    }
    
    /// Set full clock speeds
    fn set_full_clocks(&mut self) {
        if !self.context_initialized {
            self.initialize_context();
        }
        // Ensure GPU is running at full speed
    }
    
    /// Initialize GPU context
    fn initialize_context(&mut self) {
        self.context_initialized = true;
        // In a real implementation, this would create the GPU context
    }
    
    /// Update power manager (call periodically)
    pub fn update(&mut self) {
        if self.target_state != self.current_state 
            && self.last_change.elapsed() >= self.state_change_delay 
        {
            self.apply_state_change();
        }
    }
    
    /// Check if context needs reinitialization
    pub fn needs_context(&self) -> bool {
        self.current_state == GpuPowerState::Off && !self.context_initialized
    }
    
    /// Get statistics
    pub fn stats(&self) -> GpuPowerStats {
        GpuPowerStats {
            current_state: self.current_state,
            transitions: self.transitions,
            time_active: self.time_in_state[GpuPowerState::Active as usize],
            time_low_power: self.time_in_state[GpuPowerState::LowPower as usize],
            time_idle: self.time_in_state[GpuPowerState::Idle as usize],
            time_off: self.time_in_state[GpuPowerState::Off as usize],
        }
    }
    
    /// Get power usage estimate (0.0 - 1.0)
    pub fn power_usage(&self) -> f32 {
        self.current_state.power_multiplier()
    }
    
    /// Request full power for rendering
    pub fn request_render(&mut self) {
        self.set_power_state(GpuPowerState::Active);
    }
    
    /// Notify rendering complete, can reduce power
    pub fn render_complete(&mut self) {
        if self.current_state == GpuPowerState::Active {
            self.set_power_state(GpuPowerState::LowPower);
        }
    }
    
    /// Notify no rendering needed
    pub fn no_render_needed(&mut self) {
        self.set_power_state(GpuPowerState::Idle);
    }
}

/// GPU power statistics
#[derive(Debug, Clone, Copy)]
pub struct GpuPowerStats {
    pub current_state: GpuPowerState,
    pub transitions: u64,
    pub time_active: Duration,
    pub time_low_power: Duration,
    pub time_idle: Duration,
    pub time_off: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_power_state_multiplier() {
        assert_eq!(GpuPowerState::Active.power_multiplier(), 1.0);
        assert_eq!(GpuPowerState::Off.power_multiplier(), 0.0);
    }
    
    #[test]
    fn test_initial_state() {
        let manager = GpuPowerManager::new();
        assert_eq!(manager.state(), GpuPowerState::Idle);
    }
    
    #[test]
    fn test_state_change() {
        let mut manager = GpuPowerManager::new();
        manager.state_change_delay = Duration::ZERO; // Instant changes for test
        
        manager.set_power_state(GpuPowerState::Active);
        assert_eq!(manager.state(), GpuPowerState::Active);
        
        manager.set_power_state(GpuPowerState::Off);
        assert_eq!(manager.state(), GpuPowerState::Off);
    }
    
    #[test]
    fn test_render_workflow() {
        let mut manager = GpuPowerManager::new();
        manager.state_change_delay = Duration::ZERO;
        
        manager.request_render();
        assert_eq!(manager.state(), GpuPowerState::Active);
        
        manager.render_complete();
        assert_eq!(manager.state(), GpuPowerState::LowPower);
        
        manager.no_render_needed();
        assert_eq!(manager.state(), GpuPowerState::Idle);
    }
}
