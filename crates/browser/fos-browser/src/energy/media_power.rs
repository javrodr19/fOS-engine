//! Media Power Optimization
//!
//! Hardware decode priority and adaptive quality for power efficiency.

use std::time::{Duration, Instant};

/// Decoder type selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DecoderType {
    /// Hardware decoder (10x less power)
    #[default]
    Hardware,
    /// Software decoder (fallback)
    Software,
}

impl DecoderType {
    /// Get power multiplier (1.0 = baseline)
    pub fn power_multiplier(&self) -> f32 {
        match self {
            Self::Hardware => 0.1,  // 10x less power
            Self::Software => 1.0,
        }
    }
}

/// Quality levels for adaptive playback
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum QualityLevel {
    /// Low quality (power saving)
    Low = 0,
    /// Medium quality (balanced)
    Medium = 1,
    /// High quality (full power)
    High = 2,
}

impl QualityLevel {
    /// Get resolution cap for this quality
    pub fn max_resolution(&self) -> (u32, u32) {
        match self {
            Self::Low => (854, 480),     // 480p
            Self::Medium => (1280, 720),  // 720p
            Self::High => (u32::MAX, u32::MAX), // No limit
        }
    }
    
    /// Get bitrate factor (1.0 = full)
    pub fn bitrate_factor(&self) -> f32 {
        match self {
            Self::Low => 0.3,
            Self::Medium => 0.6,
            Self::High => 1.0,
        }
    }
    
    /// Get power savings estimate
    pub fn power_factor(&self) -> f32 {
        match self {
            Self::Low => 0.5,
            Self::Medium => 0.75,
            Self::High => 1.0,
        }
    }
}

/// Codec hardware support
#[derive(Debug, Clone)]
pub struct CodecHwSupport {
    /// H.264 hardware decode
    pub h264: bool,
    /// H.265/HEVC hardware decode
    pub h265: bool,
    /// VP8 hardware decode
    pub vp8: bool,
    /// VP9 hardware decode
    pub vp9: bool,
    /// AV1 hardware decode
    pub av1: bool,
}

impl Default for CodecHwSupport {
    fn default() -> Self {
        Self {
            h264: true,  // Nearly universal
            h265: true,  // Common on modern hardware
            vp8: true,
            vp9: true,
            av1: false,  // Not yet universal
        }
    }
}

/// Media power manager
#[derive(Debug)]
pub struct MediaPowerManager {
    /// On battery power
    on_battery: bool,
    /// Battery level (0.0 - 1.0)
    battery_level: f64,
    /// Hardware codec support
    hw_support: CodecHwSupport,
    /// Current quality level
    quality_level: QualityLevel,
    /// Force hardware decode
    force_hardware: bool,
    /// Adaptive quality enabled
    adaptive_quality: bool,
    /// Last quality change
    last_quality_change: Instant,
    /// Quality change cooldown
    quality_cooldown: Duration,
}

impl Default for MediaPowerManager {
    fn default() -> Self {
        Self::new()
    }
}

impl MediaPowerManager {
    /// Create a new media power manager
    pub fn new() -> Self {
        Self {
            on_battery: false,
            battery_level: 1.0,
            hw_support: CodecHwSupport::default(),
            quality_level: QualityLevel::High,
            force_hardware: false,
            adaptive_quality: true,
            last_quality_change: Instant::now(),
            quality_cooldown: Duration::from_secs(30),
        }
    }
    
    /// Update battery status
    pub fn set_battery_status(&mut self, on_battery: bool, level: f64) {
        self.on_battery = on_battery;
        self.battery_level = level;
        
        // Update quality based on battery
        if self.adaptive_quality {
            self.update_quality();
        }
    }
    
    /// Update quality from battery status
    fn update_quality(&mut self) {
        if self.last_quality_change.elapsed() < self.quality_cooldown {
            return; // Avoid rapid changes
        }
        
        let new_quality = self.select_quality();
        if new_quality != self.quality_level {
            self.quality_level = new_quality;
            self.last_quality_change = Instant::now();
        }
    }
    
    /// Select quality based on power state
    pub fn select_quality(&self) -> QualityLevel {
        if !self.on_battery {
            return QualityLevel::High;
        }
        
        if self.battery_level < 0.2 {
            QualityLevel::Low
        } else if self.battery_level < 0.5 {
            QualityLevel::Medium
        } else {
            QualityLevel::High
        }
    }
    
    /// Set hardware codec support
    pub fn set_hw_support(&mut self, support: CodecHwSupport) {
        self.hw_support = support;
    }
    
    /// Check if hardware decode available for codec
    pub fn hw_decoder_available(&self, codec: &str) -> bool {
        match codec.to_lowercase().as_str() {
            "h264" | "avc" | "avc1" => self.hw_support.h264,
            "h265" | "hevc" | "hvc1" => self.hw_support.h265,
            "vp8" => self.hw_support.vp8,
            "vp9" => self.hw_support.vp9,
            "av1" | "av01" => self.hw_support.av1,
            _ => false,
        }
    }
    
    /// Select decoder type for codec
    pub fn select_decoder(&self, codec: &str) -> DecoderType {
        // Always prefer hardware if available (10x less power)
        if self.force_hardware || self.hw_decoder_available(codec) {
            DecoderType::Hardware
        } else {
            DecoderType::Software
        }
    }
    
    /// Get current quality level
    pub fn quality(&self) -> QualityLevel {
        self.quality_level
    }
    
    /// Set quality level manually
    pub fn set_quality(&mut self, level: QualityLevel) {
        self.quality_level = level;
        self.last_quality_change = Instant::now();
    }
    
    /// Enable/disable adaptive quality
    pub fn set_adaptive_quality(&mut self, enabled: bool) {
        self.adaptive_quality = enabled;
    }
    
    /// Force hardware decode
    pub fn set_force_hardware(&mut self, force: bool) {
        self.force_hardware = force;
    }
    
    /// Get power factor for current settings
    pub fn power_factor(&self) -> f32 {
        self.quality_level.power_factor()
    }
    
    /// Check if should reduce quality
    pub fn should_reduce_quality(&self) -> bool {
        self.on_battery && self.battery_level < 0.3 && self.quality_level != QualityLevel::Low
    }
    
    /// Get max resolution for current quality
    pub fn max_resolution(&self) -> (u32, u32) {
        self.quality_level.max_resolution()
    }
    
    /// Get bitrate factor for current quality
    pub fn bitrate_factor(&self) -> f32 {
        self.quality_level.bitrate_factor()
    }
    
    /// Get statistics
    pub fn stats(&self) -> MediaPowerStats {
        MediaPowerStats {
            on_battery: self.on_battery,
            battery_level: self.battery_level,
            quality: self.quality_level,
            hw_h264: self.hw_support.h264,
            hw_h265: self.hw_support.h265,
            hw_vp9: self.hw_support.vp9,
            hw_av1: self.hw_support.av1,
        }
    }
}

/// Media power statistics
#[derive(Debug, Clone, Copy)]
pub struct MediaPowerStats {
    pub on_battery: bool,
    pub battery_level: f64,
    pub quality: QualityLevel,
    pub hw_h264: bool,
    pub hw_h265: bool,
    pub hw_vp9: bool,
    pub hw_av1: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_decoder_power() {
        assert!(DecoderType::Hardware.power_multiplier() < DecoderType::Software.power_multiplier());
    }
    
    #[test]
    fn test_quality_order() {
        assert!(QualityLevel::Low < QualityLevel::Medium);
        assert!(QualityLevel::Medium < QualityLevel::High);
    }
    
    #[test]
    fn test_select_decoder_hw() {
        let manager = MediaPowerManager::new();
        assert_eq!(manager.select_decoder("h264"), DecoderType::Hardware);
        assert_eq!(manager.select_decoder("vp9"), DecoderType::Hardware);
    }
    
    #[test]
    fn test_select_quality_plugged() {
        let manager = MediaPowerManager::new();
        assert_eq!(manager.select_quality(), QualityLevel::High);
    }
    
    #[test]
    fn test_select_quality_low_battery() {
        let mut manager = MediaPowerManager::new();
        manager.set_battery_status(true, 0.1);
        assert_eq!(manager.quality(), QualityLevel::Low);
    }
    
    #[test]
    fn test_select_quality_medium_battery() {
        let mut manager = MediaPowerManager::new();
        manager.set_battery_status(true, 0.4);
        assert_eq!(manager.quality(), QualityLevel::Medium);
    }
    
    #[test]
    fn test_max_resolution() {
        assert_eq!(QualityLevel::Low.max_resolution(), (854, 480));
        assert_eq!(QualityLevel::Medium.max_resolution(), (1280, 720));
    }
}
