//! Permissions API
//!
//! Unified permission management for browser features.

use std::collections::HashMap;

/// Permission state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    Granted,
    Denied,
    Prompt,
}

/// Permission types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PermissionName {
    Geolocation,
    Notifications,
    Push,
    Midi,
    Camera,
    Microphone,
    SpeakerSelection,
    DeviceInfo,
    BackgroundFetch,
    BackgroundSync,
    Bluetooth,
    PersistentStorage,
    AmbientLightSensor,
    Accelerometer,
    Gyroscope,
    Magnetometer,
    ClipboardRead,
    ClipboardWrite,
    ScreenWakeLock,
    DisplayCapture,
}

impl PermissionName {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "geolocation" => Some(Self::Geolocation),
            "notifications" => Some(Self::Notifications),
            "push" => Some(Self::Push),
            "midi" => Some(Self::Midi),
            "camera" => Some(Self::Camera),
            "microphone" => Some(Self::Microphone),
            "speaker-selection" => Some(Self::SpeakerSelection),
            "device-info" => Some(Self::DeviceInfo),
            "background-fetch" => Some(Self::BackgroundFetch),
            "background-sync" => Some(Self::BackgroundSync),
            "bluetooth" => Some(Self::Bluetooth),
            "persistent-storage" => Some(Self::PersistentStorage),
            "ambient-light-sensor" => Some(Self::AmbientLightSensor),
            "accelerometer" => Some(Self::Accelerometer),
            "gyroscope" => Some(Self::Gyroscope),
            "magnetometer" => Some(Self::Magnetometer),
            "clipboard-read" => Some(Self::ClipboardRead),
            "clipboard-write" => Some(Self::ClipboardWrite),
            "screen-wake-lock" => Some(Self::ScreenWakeLock),
            "display-capture" => Some(Self::DisplayCapture),
            _ => None,
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Geolocation => "geolocation",
            Self::Notifications => "notifications",
            Self::Push => "push",
            Self::Midi => "midi",
            Self::Camera => "camera",
            Self::Microphone => "microphone",
            Self::SpeakerSelection => "speaker-selection",
            Self::DeviceInfo => "device-info",
            Self::BackgroundFetch => "background-fetch",
            Self::BackgroundSync => "background-sync",
            Self::Bluetooth => "bluetooth",
            Self::PersistentStorage => "persistent-storage",
            Self::AmbientLightSensor => "ambient-light-sensor",
            Self::Accelerometer => "accelerometer",
            Self::Gyroscope => "gyroscope",
            Self::Magnetometer => "magnetometer",
            Self::ClipboardRead => "clipboard-read",
            Self::ClipboardWrite => "clipboard-write",
            Self::ScreenWakeLock => "screen-wake-lock",
            Self::DisplayCapture => "display-capture",
        }
    }
}

/// Permission descriptor
#[derive(Debug, Clone)]
pub struct PermissionDescriptor {
    pub name: PermissionName,
    pub user_visible_only: bool, // For push
    pub sysex: bool,             // For midi
}

impl PermissionDescriptor {
    pub fn new(name: PermissionName) -> Self {
        Self {
            name,
            user_visible_only: false,
            sysex: false,
        }
    }
}

/// Permission status
#[derive(Debug, Clone)]
pub struct PermissionStatus {
    pub name: PermissionName,
    pub state: PermissionState,
}

/// Permissions manager
#[derive(Debug, Default)]
pub struct PermissionsManager {
    /// Permissions by origin -> permission name
    permissions: HashMap<String, HashMap<PermissionName, PermissionState>>,
    /// Default permissions (not prompting)
    auto_grant: Vec<PermissionName>,
    auto_deny: Vec<PermissionName>,
}

impl PermissionsManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Query permission state
    pub fn query(&self, origin: &str, descriptor: &PermissionDescriptor) -> PermissionStatus {
        let state = self.permissions
            .get(origin)
            .and_then(|perms| perms.get(&descriptor.name))
            .copied()
            .unwrap_or(PermissionState::Prompt);
        
        PermissionStatus {
            name: descriptor.name.clone(),
            state,
        }
    }
    
    /// Request permission
    pub fn request(&mut self, origin: &str, descriptor: &PermissionDescriptor) -> PermissionState {
        // Check auto policies
        if self.auto_deny.contains(&descriptor.name) {
            return PermissionState::Denied;
        }
        if self.auto_grant.contains(&descriptor.name) {
            self.set_permission(origin, descriptor.name.clone(), PermissionState::Granted);
            return PermissionState::Granted;
        }
        
        // Check existing permission
        if let Some(state) = self.permissions
            .get(origin)
            .and_then(|perms| perms.get(&descriptor.name)) 
        {
            if *state != PermissionState::Prompt {
                return *state;
            }
        }
        
        // In a real browser, show permission prompt
        // For now, default to granted (dev mode)
        let state = PermissionState::Granted;
        self.set_permission(origin, descriptor.name.clone(), state);
        state
    }
    
    /// Revoke permission
    pub fn revoke(&mut self, origin: &str, descriptor: &PermissionDescriptor) -> PermissionState {
        self.set_permission(origin, descriptor.name.clone(), PermissionState::Prompt);
        PermissionState::Prompt
    }
    
    /// Set permission directly
    pub fn set_permission(&mut self, origin: &str, name: PermissionName, state: PermissionState) {
        self.permissions
            .entry(origin.to_string())
            .or_default()
            .insert(name, state);
    }
    
    /// Clear all permissions for origin
    pub fn clear_origin(&mut self, origin: &str) {
        self.permissions.remove(origin);
    }
    
    /// Set auto-grant policy
    pub fn auto_grant(&mut self, name: PermissionName) {
        let name_clone = name.clone();
        if !self.auto_grant.contains(&name) {
            self.auto_grant.push(name);
        }
        self.auto_deny.retain(|n| *n != name_clone);
    }
    
    /// Set auto-deny policy
    pub fn auto_deny(&mut self, name: PermissionName) {
        let name_clone = name.clone();
        if !self.auto_deny.contains(&name) {
            self.auto_deny.push(name);
        }
        self.auto_grant.retain(|n| *n != name_clone);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_permissions() {
        let mut mgr = PermissionsManager::new();
        let origin = "https://example.com";
        
        // Default is prompt
        let desc = PermissionDescriptor::new(PermissionName::Geolocation);
        let status = mgr.query(origin, &desc);
        assert_eq!(status.state, PermissionState::Prompt);
        
        // Request grants (in dev mode)
        let state = mgr.request(origin, &desc);
        assert_eq!(state, PermissionState::Granted);
        
        // Query shows granted
        let status = mgr.query(origin, &desc);
        assert_eq!(status.state, PermissionState::Granted);
        
        // Revoke returns to prompt
        mgr.revoke(origin, &desc);
        let status = mgr.query(origin, &desc);
        assert_eq!(status.state, PermissionState::Prompt);
    }
}
