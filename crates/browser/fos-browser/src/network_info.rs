//! Network Information API
//!
//! Connection type and quality detection.

/// Connection type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConnectionType {
    #[default]
    Unknown,
    Ethernet,
    Wifi,
    Cellular2G,
    Cellular3G,
    Cellular4G,
    Cellular5G,
    Bluetooth,
    None,
}

impl ConnectionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Ethernet => "ethernet",
            Self::Wifi => "wifi",
            Self::Cellular2G => "2g",
            Self::Cellular3G => "3g",
            Self::Cellular4G => "4g",
            Self::Cellular5G => "5g",
            Self::Bluetooth => "bluetooth",
            Self::None => "none",
        }
    }
}

/// Effective connection type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EffectiveType {
    Slow2G,
    TwoG,
    ThreeG,
    #[default]
    FourG,
}

impl EffectiveType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Slow2G => "slow-2g",
            Self::TwoG => "2g",
            Self::ThreeG => "3g",
            Self::FourG => "4g",
        }
    }
}

/// Network information
#[derive(Debug, Clone)]
pub struct NetworkInformation {
    pub connection_type: ConnectionType,
    pub effective_type: EffectiveType,
    pub downlink: f64,        // Mbps
    pub rtt: u32,             // Round-trip time in ms
    pub save_data: bool,
    pub downlink_max: f64,
}

impl Default for NetworkInformation {
    fn default() -> Self {
        Self {
            connection_type: ConnectionType::Unknown,
            effective_type: EffectiveType::FourG,
            downlink: 10.0,
            rtt: 50,
            save_data: false,
            downlink_max: 100.0,
        }
    }
}

impl NetworkInformation {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Update from system
    pub fn update(&mut self) {
        // In real implementation, read from system APIs
        // For now, try to infer from file existence on Linux
        #[cfg(target_os = "linux")]
        {
            // Check for wifi
            if std::path::Path::new("/sys/class/net/wlan0").exists() {
                self.connection_type = ConnectionType::Wifi;
            } else if std::path::Path::new("/sys/class/net/eth0").exists() {
                self.connection_type = ConnectionType::Ethernet;
            }
        }
    }
    
    /// Is online
    pub fn online(&self) -> bool {
        self.connection_type != ConnectionType::None
    }
    
    /// Estimate effective type from RTT and downlink
    pub fn estimate_effective_type(&self) -> EffectiveType {
        if self.rtt >= 2000 || self.downlink < 0.05 {
            EffectiveType::Slow2G
        } else if self.rtt >= 1400 || self.downlink < 0.07 {
            EffectiveType::TwoG
        } else if self.rtt >= 270 || self.downlink < 1.5 {
            EffectiveType::ThreeG
        } else {
            EffectiveType::FourG
        }
    }
}

/// Network manager
#[derive(Debug, Default)]
pub struct NetworkInfoManager {
    info: NetworkInformation,
    online: bool,
}

impl NetworkInfoManager {
    pub fn new() -> Self {
        Self {
            info: NetworkInformation::new(),
            online: true,
        }
    }
    
    /// Get network information
    pub fn info(&self) -> &NetworkInformation {
        &self.info
    }
    
    /// Check if online
    pub fn is_online(&self) -> bool {
        self.online
    }
    
    /// Set online status
    pub fn set_online(&mut self, online: bool) {
        self.online = online;
        if !online {
            self.info.connection_type = ConnectionType::None;
        }
    }
    
    /// Update from system
    pub fn update(&mut self) {
        self.info.update();
        self.online = self.info.online();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_network_info() {
        let info = NetworkInformation::default();
        assert!(info.online());
        assert_eq!(info.effective_type, EffectiveType::FourG);
    }
}
