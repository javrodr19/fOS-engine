//! Keyboard Events
//!
//! KeyboardEvent implementation with key codes and modifiers.

/// Keyboard event
#[derive(Debug, Clone)]
pub struct KeyboardEvent {
    pub event_type: KeyboardEventType,
    pub key: Key,
    pub code: String,
    pub modifiers: KeyModifiers,
    pub repeat: bool,
    pub is_composing: bool,
    
    // Event state
    pub bubbles: bool,
    pub cancelable: bool,
    default_prevented: bool,
    propagation_stopped: bool,
    pub timestamp: f64,
}

/// Keyboard event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardEventType {
    KeyDown,
    KeyUp,
    KeyPress, // Deprecated but still used
}

/// Key value
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    // Letters
    Character(char),
    
    // Function keys
    F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12,
    
    // Navigation
    ArrowUp, ArrowDown, ArrowLeft, ArrowRight,
    Home, End, PageUp, PageDown,
    
    // Editing
    Backspace, Delete, Insert,
    Enter, Tab, Escape,
    
    // Modifiers (not usually received as key events)
    Shift, Control, Alt, Meta,
    CapsLock, NumLock, ScrollLock,
    
    // Whitespace
    Space,
    
    // Other
    Unidentified(String),
}

impl Key {
    /// Parse from key string
    pub fn parse(s: &str) -> Self {
        match s {
            "ArrowUp" => Self::ArrowUp,
            "ArrowDown" => Self::ArrowDown,
            "ArrowLeft" => Self::ArrowLeft,
            "ArrowRight" => Self::ArrowRight,
            "Backspace" => Self::Backspace,
            "Delete" => Self::Delete,
            "Enter" => Self::Enter,
            "Tab" => Self::Tab,
            "Escape" => Self::Escape,
            "Home" => Self::Home,
            "End" => Self::End,
            "PageUp" => Self::PageUp,
            "PageDown" => Self::PageDown,
            " " => Self::Space,
            s if s.len() == 1 => Self::Character(s.chars().next().unwrap()),
            s if s.starts_with('F') && s.len() <= 3 => {
                match s.parse::<u8>() {
                    Ok(1) => Self::F1,
                    Ok(2) => Self::F2,
                    Ok(3) => Self::F3,
                    Ok(4) => Self::F4,
                    Ok(5) => Self::F5,
                    Ok(6) => Self::F6,
                    Ok(7) => Self::F7,
                    Ok(8) => Self::F8,
                    Ok(9) => Self::F9,
                    Ok(10) => Self::F10,
                    Ok(11) => Self::F11,
                    Ok(12) => Self::F12,
                    _ => Self::Unidentified(s.to_string()),
                }
            }
            s => Self::Unidentified(s.to_string()),
        }
    }
    
    /// Convert to key value string
    pub fn to_key_string(&self) -> String {
        match self {
            Self::Character(c) => c.to_string(),
            Self::ArrowUp => "ArrowUp".to_string(),
            Self::ArrowDown => "ArrowDown".to_string(),
            Self::ArrowLeft => "ArrowLeft".to_string(),
            Self::ArrowRight => "ArrowRight".to_string(),
            Self::Backspace => "Backspace".to_string(),
            Self::Delete => "Delete".to_string(),
            Self::Enter => "Enter".to_string(),
            Self::Tab => "Tab".to_string(),
            Self::Escape => "Escape".to_string(),
            Self::Space => " ".to_string(),
            Self::F1 => "F1".to_string(),
            // ... etc
            Self::Unidentified(s) => s.clone(),
            _ => "Unidentified".to_string(),
        }
    }
}

/// Key modifiers
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyModifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool, // Cmd on Mac, Win on Windows
}

impl KeyModifiers {
    /// Check if any modifier is pressed
    pub fn any(&self) -> bool {
        self.shift || self.ctrl || self.alt || self.meta
    }
    
    /// Create from booleans
    pub fn from_flags(shift: bool, ctrl: bool, alt: bool, meta: bool) -> Self {
        Self { shift, ctrl, alt, meta }
    }
}

impl KeyboardEvent {
    /// Create a new keyboard event
    pub fn new(event_type: KeyboardEventType, key: Key) -> Self {
        Self {
            event_type,
            key,
            code: String::new(),
            modifiers: KeyModifiers::default(),
            repeat: false,
            is_composing: false,
            bubbles: true,
            cancelable: true,
            default_prevented: false,
            propagation_stopped: false,
            timestamp: 0.0,
        }
    }
    
    /// Add modifiers
    pub fn with_modifiers(mut self, modifiers: KeyModifiers) -> Self {
        self.modifiers = modifiers;
        self
    }
    
    /// Check if Ctrl+Key (or Cmd+Key on Mac)
    pub fn is_command_key(&self, key: &Key) -> bool {
        &self.key == key && (self.modifiers.ctrl || self.modifiers.meta)
    }
    
    /// Prevent default
    pub fn prevent_default(&mut self) {
        self.default_prevented = true;
    }
    
    /// Stop propagation
    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_key_parse() {
        assert_eq!(Key::parse("ArrowUp"), Key::ArrowUp);
        assert_eq!(Key::parse("a"), Key::Character('a'));
        assert_eq!(Key::parse("Enter"), Key::Enter);
    }
    
    #[test]
    fn test_modifiers() {
        let mods = KeyModifiers::from_flags(true, true, false, false);
        assert!(mods.shift);
        assert!(mods.ctrl);
        assert!(mods.any());
    }
}
