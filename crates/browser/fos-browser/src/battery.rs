//! Battery Status API
//!
//! Device battery information.

use std::time::Duration;

/// Battery status
#[derive(Debug, Clone)]
pub struct BatteryManager {
    pub charging: bool,
    pub charging_time: Duration,
    pub discharging_time: Duration,
    pub level: f64,
}

impl Default for BatteryManager {
    fn default() -> Self {
        Self::new()
    }
}

impl BatteryManager {
    pub fn new() -> Self {
        Self {
            charging: false,
            charging_time: Duration::MAX,
            discharging_time: Duration::MAX,
            level: 1.0,
        }
    }
    
    /// Get charging status
    pub fn is_charging(&self) -> bool {
        self.charging
    }
    
    /// Get level (0.0 to 1.0)
    pub fn level(&self) -> f64 {
        self.level
    }
    
    /// Get level as percentage
    pub fn level_percent(&self) -> u8 {
        (self.level * 100.0).round() as u8
    }
    
    /// Get time until full (seconds)
    pub fn charging_time(&self) -> Option<u64> {
        if self.charging && self.charging_time != Duration::MAX {
            Some(self.charging_time.as_secs())
        } else {
            None
        }
    }
    
    /// Get time until empty (seconds)
    pub fn discharging_time(&self) -> Option<u64> {
        if !self.charging && self.discharging_time != Duration::MAX {
            Some(self.discharging_time.as_secs())
        } else {
            None
        }
    }
    
    /// Update from system (Linux)
    #[cfg(target_os = "linux")]
    pub fn update(&mut self) {
        // Try to read from /sys/class/power_supply/BAT0
        if let Ok(capacity) = std::fs::read_to_string("/sys/class/power_supply/BAT0/capacity") {
            if let Ok(level) = capacity.trim().parse::<u8>() {
                self.level = level as f64 / 100.0;
            }
        }
        
        if let Ok(status) = std::fs::read_to_string("/sys/class/power_supply/BAT0/status") {
            self.charging = status.trim() == "Charging";
        }
    }
    
    #[cfg(not(target_os = "linux"))]
    pub fn update(&mut self) {
        // No battery info on other platforms
    }
}

/// Battery event
#[derive(Debug, Clone)]
pub enum BatteryEvent {
    ChargingChange(bool),
    ChargingTimeChange(Option<u64>),
    DischargingTimeChange(Option<u64>),
    LevelChange(f64),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_battery() {
        let battery = BatteryManager::new();
        
        assert_eq!(battery.level(), 1.0);
        assert_eq!(battery.level_percent(), 100);
    }
}
