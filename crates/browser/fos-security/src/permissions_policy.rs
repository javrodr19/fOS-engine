//! Permissions Policy (Feature Policy)
//!
//! Feature permission declarations and enforcement.

use std::collections::HashMap;

/// Controlled feature
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    Accelerometer, AmbientLightSensor, Autoplay, Battery, Camera, DisplayCapture,
    DocumentDomain, EncryptedMedia, Fullscreen, Gamepad, Geolocation, Gyroscope,
    Magnetometer, Microphone, Midi, Payment, PictureInPicture, PublickeyCredentials,
    ScreenWakeLock, SpeakerSelection, SyncXhr, Usb, WebShare, XrSpatialTracking,
}

impl Feature {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_lowercase().replace('-', "").as_str() {
            "accelerometer" => Self::Accelerometer, "ambientlightsensor" => Self::AmbientLightSensor,
            "autoplay" => Self::Autoplay, "battery" => Self::Battery, "camera" => Self::Camera,
            "displaycapture" => Self::DisplayCapture, "documentdomain" => Self::DocumentDomain,
            "encryptedmedia" => Self::EncryptedMedia, "fullscreen" => Self::Fullscreen,
            "gamepad" => Self::Gamepad, "geolocation" => Self::Geolocation, "gyroscope" => Self::Gyroscope,
            "magnetometer" => Self::Magnetometer, "microphone" => Self::Microphone, "midi" => Self::Midi,
            "payment" => Self::Payment, "pictureinpicture" => Self::PictureInPicture,
            "publickeycredentials" => Self::PublickeyCredentials, "screenwakelock" => Self::ScreenWakeLock,
            "speakerselection" => Self::SpeakerSelection, "syncxhr" => Self::SyncXhr, "usb" => Self::Usb,
            "webshare" => Self::WebShare, "xrspatialtracking" => Self::XrSpatialTracking,
            _ => return None,
        })
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Accelerometer => "accelerometer", Self::AmbientLightSensor => "ambient-light-sensor",
            Self::Autoplay => "autoplay", Self::Battery => "battery", Self::Camera => "camera",
            Self::DisplayCapture => "display-capture", Self::DocumentDomain => "document-domain",
            Self::EncryptedMedia => "encrypted-media", Self::Fullscreen => "fullscreen",
            Self::Gamepad => "gamepad", Self::Geolocation => "geolocation", Self::Gyroscope => "gyroscope",
            Self::Magnetometer => "magnetometer", Self::Microphone => "microphone", Self::Midi => "midi",
            Self::Payment => "payment", Self::PictureInPicture => "picture-in-picture",
            Self::PublickeyCredentials => "publickey-credentials", Self::ScreenWakeLock => "screen-wake-lock",
            Self::SpeakerSelection => "speaker-selection", Self::SyncXhr => "sync-xhr", Self::Usb => "usb",
            Self::WebShare => "web-share", Self::XrSpatialTracking => "xr-spatial-tracking",
        }
    }
}

/// Allowlist for a feature
#[derive(Debug, Clone, PartialEq)]
pub enum Allowlist {
    All,              // *
    None,             // 'none' or ()
    Self_,            // 'self'
    Origins(Vec<String>),
}

impl Allowlist {
    pub fn allows(&self, origin: &str, is_self: bool) -> bool {
        match self {
            Self::All => true,
            Self::None => false,
            Self::Self_ => is_self,
            Self::Origins(list) => list.iter().any(|o| o == origin || o == "'self'" && is_self),
        }
    }
}

/// Permissions policy
#[derive(Debug, Clone, Default)]
pub struct PermissionsPolicy {
    pub directives: HashMap<Feature, Allowlist>,
}

impl PermissionsPolicy {
    pub fn new() -> Self { Self::default() }
    
    /// Parse from Permissions-Policy header
    pub fn parse(header: &str) -> Self {
        let mut policy = Self::new();
        for directive in header.split(',').map(str::trim) {
            if let Some((name, value)) = directive.split_once('=') {
                if let Some(feature) = Feature::parse(name.trim()) {
                    let allowlist = Self::parse_allowlist(value.trim());
                    policy.directives.insert(feature, allowlist);
                }
            }
        }
        policy
    }
    
    fn parse_allowlist(value: &str) -> Allowlist {
        let value = value.trim_matches(|c| c == '(' || c == ')');
        if value.is_empty() { return Allowlist::None; }
        if value == "*" { return Allowlist::All; }
        if value == "'self'" || value == "self" { return Allowlist::Self_; }
        
        let origins: Vec<String> = value.split_whitespace()
            .map(|s| s.trim_matches('"').to_string())
            .collect();
        if origins.is_empty() { Allowlist::None } else { Allowlist::Origins(origins) }
    }
    
    /// Parse from iframe allow attribute
    pub fn parse_allow(allow: &str) -> Self {
        let mut policy = Self::new();
        for part in allow.split(';').map(str::trim) {
            let mut tokens = part.split_whitespace();
            if let Some(name) = tokens.next() {
                if let Some(feature) = Feature::parse(name) {
                    let origins: Vec<String> = tokens.map(|s| s.to_string()).collect();
                    let allowlist = if origins.is_empty() {
                        Allowlist::Self_
                    } else if origins.iter().any(|o| o == "*") {
                        Allowlist::All
                    } else {
                        Allowlist::Origins(origins)
                    };
                    policy.directives.insert(feature, allowlist);
                }
            }
        }
        policy
    }
    
    /// Check if feature is allowed
    pub fn is_allowed(&self, feature: Feature, origin: &str, is_self: bool) -> bool {
        self.directives.get(&feature).map(|a| a.allows(origin, is_self)).unwrap_or(true)
    }
    
    /// Merge with inherited policy
    pub fn inherit(&mut self, parent: &PermissionsPolicy) {
        for (feature, allowlist) in &parent.directives {
            self.directives.entry(*feature).or_insert_with(|| allowlist.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_feature_parse() {
        assert_eq!(Feature::parse("geolocation"), Some(Feature::Geolocation));
        assert_eq!(Feature::parse("camera"), Some(Feature::Camera));
    }
    
    #[test]
    fn test_policy_parse() {
        let policy = PermissionsPolicy::parse("geolocation=(), camera=(self)");
        assert_eq!(policy.directives.get(&Feature::Geolocation), Some(&Allowlist::None));
    }
    
    #[test]
    fn test_allow_attribute() {
        let policy = PermissionsPolicy::parse_allow("fullscreen; camera 'self'");
        assert!(policy.is_allowed(Feature::Fullscreen, "https://example.com", true));
    }
}
