//! Alternative Input Methods
//!
//! Support for switch access and voice control.
//! Custom implementation with no external dependencies.

use std::collections::HashMap;

/// Switch access scanning mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScanMode {
    /// Automatic scanning with timer
    #[default]
    Auto,
    /// Manual step-by-step scanning
    Manual,
    /// Group scanning (hierarchical)
    Group,
    /// Point scanning (x-y grid)
    Point,
}

/// Switch access configuration
#[derive(Debug, Clone)]
pub struct SwitchAccessConfig {
    pub scan_mode: ScanMode,
    pub scan_speed_ms: u32,
    pub auto_scan_delay_ms: u32,
    pub highlight_color: String,
    pub highlight_width: f64,
    pub sound_feedback: bool,
}

impl Default for SwitchAccessConfig {
    fn default() -> Self {
        Self {
            scan_mode: ScanMode::Auto,
            scan_speed_ms: 1000,
            auto_scan_delay_ms: 500,
            highlight_color: "#ff6600".to_string(),
            highlight_width: 4.0,
            sound_feedback: true,
        }
    }
}

/// Switch access manager
#[derive(Debug, Default)]
pub struct SwitchAccessManager {
    config: SwitchAccessConfig,
    enabled: bool,
    /// Current group being scanned (for group mode)
    current_group: Option<u64>,
    /// Current element index in scan order
    current_index: usize,
    /// Elements in scan order
    scan_order: Vec<u64>,
    /// Is scanning currently active
    scanning: bool,
}

impl SwitchAccessManager {
    pub fn new() -> Self { Self::default() }
    
    /// Enable switch access
    pub fn enable(&mut self) {
        self.enabled = true;
    }
    
    /// Disable switch access
    pub fn disable(&mut self) {
        self.enabled = false;
        self.scanning = false;
    }
    
    /// Check if enabled
    pub fn is_enabled(&self) -> bool { self.enabled }
    
    /// Set configuration
    pub fn set_config(&mut self, config: SwitchAccessConfig) {
        self.config = config;
    }
    
    /// Get configuration
    pub fn config(&self) -> &SwitchAccessConfig { &self.config }
    
    /// Set scan order (list of focusable element IDs)
    pub fn set_scan_order(&mut self, elements: Vec<u64>) {
        self.scan_order = elements;
        self.current_index = 0;
    }
    
    /// Start scanning
    pub fn start_scan(&mut self) {
        if !self.enabled || self.scan_order.is_empty() {
            return;
        }
        self.scanning = true;
        self.current_index = 0;
    }
    
    /// Stop scanning
    pub fn stop_scan(&mut self) {
        self.scanning = false;
    }
    
    /// Move to next element (switch 1 or auto timer)
    pub fn next(&mut self) -> Option<u64> {
        if !self.scanning || self.scan_order.is_empty() {
            return None;
        }
        
        self.current_index = (self.current_index + 1) % self.scan_order.len();
        Some(self.scan_order[self.current_index])
    }
    
    /// Move to previous element
    pub fn prev(&mut self) -> Option<u64> {
        if !self.scanning || self.scan_order.is_empty() {
            return None;
        }
        
        if self.current_index == 0 {
            self.current_index = self.scan_order.len() - 1;
        } else {
            self.current_index -= 1;
        }
        Some(self.scan_order[self.current_index])
    }
    
    /// Select current element (switch 2)
    pub fn select(&self) -> Option<u64> {
        if self.scanning && !self.scan_order.is_empty() {
            Some(self.scan_order[self.current_index])
        } else {
            None
        }
    }
    
    /// Get current highlighted element
    pub fn current(&self) -> Option<u64> {
        if self.scanning && !self.scan_order.is_empty() {
            Some(self.scan_order[self.current_index])
        } else {
            None
        }
    }
    
    /// Enter a group (for group scanning)
    pub fn enter_group(&mut self, group_id: u64) {
        self.current_group = Some(group_id);
        self.current_index = 0;
    }
    
    /// Exit current group
    pub fn exit_group(&mut self) {
        self.current_group = None;
    }
}

/// Voice control action
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceAction {
    /// Click an element
    Click(String),
    /// Type text
    Type(String),
    /// Scroll direction
    Scroll(ScrollDirection),
    /// Navigate to element by number
    GoTo(u32),
    /// Press a key
    Press(String),
    /// Custom command
    Custom(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Voice control manager
#[derive(Debug, Default)]
pub struct VoiceControlManager {
    enabled: bool,
    /// Command → action mapping
    commands: HashMap<String, VoiceAction>,
    /// Show overlay labels
    show_labels: bool,
    /// Numbered elements for "click <number>" commands
    numbered_elements: Vec<u64>,
}

impl VoiceControlManager {
    pub fn new() -> Self {
        let mut manager = Self::default();
        manager.register_default_commands();
        manager
    }
    
    fn register_default_commands(&mut self) {
        // Navigation
        self.commands.insert("scroll up".into(), VoiceAction::Scroll(ScrollDirection::Up));
        self.commands.insert("scroll down".into(), VoiceAction::Scroll(ScrollDirection::Down));
        self.commands.insert("page up".into(), VoiceAction::Press("PageUp".into()));
        self.commands.insert("page down".into(), VoiceAction::Press("PageDown".into()));
        self.commands.insert("go back".into(), VoiceAction::Press("Alt+Left".into()));
        self.commands.insert("go forward".into(), VoiceAction::Press("Alt+Right".into()));
        
        // Actions
        self.commands.insert("press enter".into(), VoiceAction::Press("Enter".into()));
        self.commands.insert("press escape".into(), VoiceAction::Press("Escape".into()));
        self.commands.insert("press tab".into(), VoiceAction::Press("Tab".into()));
    }
    
    /// Enable voice control
    pub fn enable(&mut self) {
        self.enabled = true;
    }
    
    /// Disable voice control
    pub fn disable(&mut self) {
        self.enabled = false;
    }
    
    /// Check if enabled
    pub fn is_enabled(&self) -> bool { self.enabled }
    
    /// Toggle overlay labels
    pub fn set_show_labels(&mut self, show: bool) {
        self.show_labels = show;
    }
    
    /// Check if labels are shown
    pub fn show_labels(&self) -> bool { self.show_labels }
    
    /// Update numbered elements (for "click 5" type commands)
    pub fn set_numbered_elements(&mut self, elements: Vec<u64>) {
        self.numbered_elements = elements;
    }
    
    /// Get element by number
    pub fn get_element_by_number(&self, number: u32) -> Option<u64> {
        self.numbered_elements.get((number - 1) as usize).copied()
    }
    
    /// Register a custom command
    pub fn register_command(&mut self, phrase: &str, action: VoiceAction) {
        self.commands.insert(phrase.to_lowercase(), action);
    }
    
    /// Process voice input (returns action if recognized)
    pub fn process_input(&self, text: &str) -> Option<VoiceAction> {
        if !self.enabled {
            return None;
        }
        
        let text = text.to_lowercase().trim().to_string();
        
        // Check for "click <name>" pattern
        if let Some(name) = text.strip_prefix("click ") {
            // Check if it's a number
            if let Ok(num) = name.parse::<u32>() {
                return Some(VoiceAction::GoTo(num));
            }
            return Some(VoiceAction::Click(name.to_string()));
        }
        
        // Check for "type <text>" pattern
        if let Some(typed_text) = text.strip_prefix("type ") {
            return Some(VoiceAction::Type(typed_text.to_string()));
        }
        
        // Check registered commands
        self.commands.get(&text).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_switch_access() {
        let mut switch = SwitchAccessManager::new();
        switch.enable();
        switch.set_scan_order(vec![1, 2, 3, 4]);
        switch.start_scan();
        
        assert_eq!(switch.current(), Some(1));
        assert_eq!(switch.next(), Some(2));
        assert_eq!(switch.next(), Some(3));
        assert_eq!(switch.select(), Some(3));
    }
    
    #[test]
    fn test_voice_control() {
        let mut voice = VoiceControlManager::new();
        voice.enable();
        
        assert!(matches!(
            voice.process_input("scroll up"),
            Some(VoiceAction::Scroll(ScrollDirection::Up))
        ));
        
        assert!(matches!(
            voice.process_input("click submit"),
            Some(VoiceAction::Click(ref s)) if s == "submit"
        ));
        
        assert!(matches!(
            voice.process_input("type hello world"),
            Some(VoiceAction::Type(ref s)) if s == "hello world"
        ));
    }
}
