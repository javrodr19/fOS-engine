//! Vibration API
//!
//! Implementation of Navigator.vibrate() for haptic feedback.

/// Vibration pattern (durations in ms)
#[derive(Debug, Clone)]
pub struct VibrationPattern {
    /// Alternating vibrate/pause durations in ms
    pub pattern: Vec<u64>,
}

impl VibrationPattern {
    /// Create single vibration
    pub fn single(duration_ms: u64) -> Self {
        Self {
            pattern: vec![duration_ms],
        }
    }
    
    /// Create pattern from durations
    pub fn pattern(durations: Vec<u64>) -> Self {
        Self { pattern: durations }
    }
    
    /// Total duration
    pub fn total_duration(&self) -> u64 {
        self.pattern.iter().sum()
    }
}

/// Vibration controller
#[derive(Debug, Default)]
pub struct VibrationController {
    /// Is vibration supported
    pub supported: bool,
    /// Current pattern being played
    current_pattern: Option<VibrationPattern>,
    /// Is currently vibrating
    pub is_vibrating: bool,
}

impl VibrationController {
    pub fn new(supported: bool) -> Self {
        Self {
            supported,
            ..Default::default()
        }
    }
    
    /// Vibrate with a single duration
    pub fn vibrate(&mut self, duration_ms: u64) -> bool {
        if !self.supported {
            return false;
        }
        
        self.current_pattern = Some(VibrationPattern::single(duration_ms));
        self.is_vibrating = true;
        true
    }
    
    /// Vibrate with a pattern
    pub fn vibrate_pattern(&mut self, pattern: VibrationPattern) -> bool {
        if !self.supported || pattern.pattern.is_empty() {
            return self.cancel();
        }
        
        // Clamp pattern length
        let clamped: Vec<u64> = pattern.pattern.into_iter().take(100).collect();
        
        self.current_pattern = Some(VibrationPattern { pattern: clamped });
        self.is_vibrating = true;
        true
    }
    
    /// Cancel vibration
    pub fn cancel(&mut self) -> bool {
        self.current_pattern = None;
        self.is_vibrating = false;
        true
    }
    
    /// Get current pattern
    pub fn current(&self) -> Option<&VibrationPattern> {
        self.current_pattern.as_ref()
    }
}

/// Permission prompt manager
#[derive(Debug, Default)]
pub struct PermissionPromptManager {
    /// Pending prompts
    pending: Vec<PermissionPrompt>,
    /// Prompt results
    results: std::collections::HashMap<u64, PermissionPromptResult>,
    /// Next prompt ID
    next_id: u64,
}

/// Permission prompt
#[derive(Debug, Clone)]
pub struct PermissionPrompt {
    pub id: u64,
    pub permission: PermissionType,
    pub origin: String,
    pub timestamp: u64,
}

/// Permission type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PermissionType {
    Geolocation,
    Notifications,
    Camera,
    Microphone,
    PersistentStorage,
    Push,
    Midi,
    BackgroundSync,
    Clipboard,
}

impl PermissionType {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Geolocation => "Location",
            Self::Notifications => "Notifications",
            Self::Camera => "Camera",
            Self::Microphone => "Microphone",
            Self::PersistentStorage => "Storage",
            Self::Push => "Push Notifications",
            Self::Midi => "MIDI Devices",
            Self::BackgroundSync => "Background Sync",
            Self::Clipboard => "Clipboard",
        }
    }
}

/// Permission prompt result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionPromptResult {
    Granted,
    Denied,
    Dismissed,
}

impl PermissionPromptManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Request permission (shows prompt)
    pub fn request(&mut self, permission: PermissionType, origin: &str) -> u64 {
        self.next_id += 1;
        
        let prompt = PermissionPrompt {
            id: self.next_id,
            permission,
            origin: origin.to_string(),
            timestamp: 0, // Would be actual timestamp
        };
        
        self.pending.push(prompt);
        self.next_id
    }
    
    /// Get pending prompts
    pub fn get_pending(&self) -> &[PermissionPrompt] {
        &self.pending
    }
    
    /// Resolve a prompt
    pub fn resolve(&mut self, prompt_id: u64, result: PermissionPromptResult) {
        self.pending.retain(|p| p.id != prompt_id);
        self.results.insert(prompt_id, result);
    }
    
    /// Get prompt result
    pub fn get_result(&self, prompt_id: u64) -> Option<PermissionPromptResult> {
        self.results.get(&prompt_id).copied()
    }
    
    /// Get message for prompt
    pub fn get_prompt_message(prompt: &PermissionPrompt) -> String {
        format!(
            "{} wants to access your {}",
            prompt.origin,
            prompt.permission.display_name()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_vibration() {
        let mut controller = VibrationController::new(true);
        
        assert!(controller.vibrate(200));
        assert!(controller.is_vibrating);
        
        controller.cancel();
        assert!(!controller.is_vibrating);
    }
    
    #[test]
    fn test_vibration_unsupported() {
        let mut controller = VibrationController::new(false);
        assert!(!controller.vibrate(200));
    }
    
    #[test]
    fn test_vibration_pattern() {
        let mut controller = VibrationController::new(true);
        
        let pattern = VibrationPattern::pattern(vec![100, 50, 100, 50, 200]);
        assert!(controller.vibrate_pattern(pattern));
        
        let current = controller.current().unwrap();
        assert_eq!(current.total_duration(), 500);
    }
    
    #[test]
    fn test_permission_prompt() {
        let mut manager = PermissionPromptManager::new();
        
        let id = manager.request(PermissionType::Notifications, "https://example.com");
        assert_eq!(manager.get_pending().len(), 1);
        
        manager.resolve(id, PermissionPromptResult::Granted);
        assert_eq!(manager.get_pending().len(), 0);
        assert_eq!(manager.get_result(id), Some(PermissionPromptResult::Granted));
    }
}
