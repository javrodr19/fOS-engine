//! Permissions API
//!
//! Device permission management.

use std::collections::HashMap;

/// Permissions manager
#[derive(Debug, Default)]
pub struct Permissions {
    states: HashMap<String, PermissionState>,
}

/// Permission state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionState {
    Prompt,
    Granted,
    Denied,
}

/// Permission descriptor
#[derive(Debug, Clone)]
pub struct PermissionDescriptor {
    pub name: String,
    pub user_visible_only: Option<bool>, // for push
    pub sysex: Option<bool>, // for midi
}

/// Query result
#[derive(Debug, Clone)]
pub struct PermissionStatus {
    pub state: PermissionState,
    pub name: String,
}

impl Permissions {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Query a permission
    pub fn query(&self, descriptor: &PermissionDescriptor) -> PermissionStatus {
        let state = self.states.get(&descriptor.name)
            .copied()
            .unwrap_or(PermissionState::Prompt);
        
        PermissionStatus {
            state,
            name: descriptor.name.clone(),
        }
    }
    
    /// Request a permission
    pub fn request(&mut self, descriptor: &PermissionDescriptor) -> PermissionStatus {
        // Would prompt user
        let state = PermissionState::Prompt;
        self.states.insert(descriptor.name.clone(), state);
        
        PermissionStatus {
            state,
            name: descriptor.name.clone(),
        }
    }
    
    /// Revoke a permission
    pub fn revoke(&mut self, descriptor: &PermissionDescriptor) -> PermissionStatus {
        self.states.insert(descriptor.name.clone(), PermissionState::Prompt);
        
        PermissionStatus {
            state: PermissionState::Prompt,
            name: descriptor.name.clone(),
        }
    }
    
    /// Set permission state (internal)
    pub fn set_state(&mut self, name: &str, state: PermissionState) {
        self.states.insert(name.to_string(), state);
    }
}

/// Well-known permission names
pub mod permission_names {
    pub const GEOLOCATION: &str = "geolocation";
    pub const NOTIFICATIONS: &str = "notifications";
    pub const CAMERA: &str = "camera";
    pub const MICROPHONE: &str = "microphone";
    pub const CLIPBOARD_READ: &str = "clipboard-read";
    pub const CLIPBOARD_WRITE: &str = "clipboard-write";
    pub const PUSH: &str = "push";
    pub const MIDI: &str = "midi";
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_permissions() {
        let mut perms = Permissions::new();
        
        let desc = PermissionDescriptor {
            name: "geolocation".into(),
            user_visible_only: None,
            sysex: None,
        };
        
        let status = perms.query(&desc);
        assert_eq!(status.state, PermissionState::Prompt);
        
        perms.set_state("geolocation", PermissionState::Granted);
        let status = perms.query(&desc);
        assert_eq!(status.state, PermissionState::Granted);
    }
}
