//! Input Mode & Virtual Keyboard
//!
//! inputmode and enterkeyhint attribute handling.

/// Input mode for virtual keyboards
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    #[default]
    None,
    Text,
    Tel,
    Url,
    Email,
    Numeric,
    Decimal,
    Search,
}

impl InputMode {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" => Self::None, "text" => Self::Text, "tel" => Self::Tel,
            "url" => Self::Url, "email" => Self::Email, "numeric" => Self::Numeric,
            "decimal" => Self::Decimal, "search" => Self::Search,
            _ => Self::Text,
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none", Self::Text => "text", Self::Tel => "tel",
            Self::Url => "url", Self::Email => "email", Self::Numeric => "numeric",
            Self::Decimal => "decimal", Self::Search => "search",
        }
    }
}

/// Enter key hint for virtual keyboards
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnterKeyHint {
    #[default]
    Enter,
    Done,
    Go,
    Next,
    Previous,
    Search,
    Send,
}

impl EnterKeyHint {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "done" => Self::Done, "go" => Self::Go, "next" => Self::Next,
            "previous" => Self::Previous, "search" => Self::Search, "send" => Self::Send,
            _ => Self::Enter,
        }
    }
    
    pub fn label(&self) -> &'static str {
        match self {
            Self::Enter => "Enter", Self::Done => "Done", Self::Go => "Go",
            Self::Next => "Next", Self::Previous => "Previous",
            Self::Search => "Search", Self::Send => "Send",
        }
    }
}

/// Virtual keyboard configuration
#[derive(Debug, Clone, Default)]
pub struct VirtualKeyboardConfig {
    pub input_mode: InputMode,
    pub enter_key_hint: EnterKeyHint,
    pub autocapitalize: AutoCapitalize,
    pub autocorrect: bool,
    pub spellcheck: bool,
}

/// Auto-capitalize mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoCapitalize {
    #[default]
    Sentences,
    None,
    Words,
    Characters,
}

impl AutoCapitalize {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "none" | "off" => Self::None, "words" => Self::Words,
            "characters" | "on" => Self::Characters,
            _ => Self::Sentences,
        }
    }
}

/// Virtual keyboard manager
#[derive(Debug, Default)]
pub struct VirtualKeyboardManager {
    configs: std::collections::HashMap<u64, VirtualKeyboardConfig>,
    visible: bool,
    current_input: Option<u64>,
}

impl VirtualKeyboardManager {
    pub fn new() -> Self { Self::default() }
    
    pub fn register_input(&mut self, id: u64, config: VirtualKeyboardConfig) {
        self.configs.insert(id, config);
    }
    
    pub fn get_config(&self, id: u64) -> Option<&VirtualKeyboardConfig> {
        self.configs.get(&id)
    }
    
    pub fn show(&mut self, input_id: u64) {
        self.current_input = Some(input_id);
        self.visible = true;
    }
    
    pub fn hide(&mut self) {
        self.current_input = None;
        self.visible = false;
    }
    
    pub fn is_visible(&self) -> bool { self.visible }
    pub fn current_input(&self) -> Option<u64> { self.current_input }
    
    pub fn get_keyboard_type(&self) -> InputMode {
        self.current_input
            .and_then(|id| self.configs.get(&id))
            .map(|c| c.input_mode)
            .unwrap_or_default()
    }
}

/// Input configuration from element attributes
#[derive(Debug, Clone, Default)]
pub struct InputConfig {
    pub input_mode: Option<InputMode>,
    pub enter_key_hint: Option<EnterKeyHint>,
    pub autocapitalize: Option<AutoCapitalize>,
    pub autocorrect: Option<bool>,
    pub spellcheck: Option<bool>,
    pub autocomplete: Option<String>,
}

impl InputConfig {
    pub fn from_attributes(attrs: &std::collections::HashMap<String, String>) -> Self {
        Self {
            input_mode: attrs.get("inputmode").map(|s| InputMode::parse(s)),
            enter_key_hint: attrs.get("enterkeyhint").map(|s| EnterKeyHint::parse(s)),
            autocapitalize: attrs.get("autocapitalize").map(|s| AutoCapitalize::parse(s)),
            autocorrect: attrs.get("autocorrect").map(|s| s != "off"),
            spellcheck: attrs.get("spellcheck").map(|s| s == "true"),
            autocomplete: attrs.get("autocomplete").cloned(),
        }
    }
    
    pub fn to_keyboard_config(&self) -> VirtualKeyboardConfig {
        VirtualKeyboardConfig {
            input_mode: self.input_mode.unwrap_or_default(),
            enter_key_hint: self.enter_key_hint.unwrap_or_default(),
            autocapitalize: self.autocapitalize.unwrap_or_default(),
            autocorrect: self.autocorrect.unwrap_or(true),
            spellcheck: self.spellcheck.unwrap_or(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_input_mode() {
        assert_eq!(InputMode::parse("numeric"), InputMode::Numeric);
        assert_eq!(InputMode::parse("email"), InputMode::Email);
    }
    
    #[test]
    fn test_enter_key_hint() {
        assert_eq!(EnterKeyHint::parse("search"), EnterKeyHint::Search);
        assert_eq!(EnterKeyHint::Search.label(), "Search");
    }
    
    #[test]
    fn test_keyboard_manager() {
        let mut manager = VirtualKeyboardManager::new();
        manager.register_input(1, VirtualKeyboardConfig { input_mode: InputMode::Numeric, ..Default::default() });
        manager.show(1);
        assert_eq!(manager.get_keyboard_type(), InputMode::Numeric);
    }
}
