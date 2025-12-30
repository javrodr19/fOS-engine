//! Autonomous and Customized Built-in Elements
//!
//! Implementation of Web Components custom element types.

use std::collections::HashMap;

/// Custom element type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomElementType {
    /// Autonomous custom element (extends HTMLElement)
    Autonomous,
    /// Customized built-in element (extends specific HTML element)
    BuiltIn,
}

/// Built-in element that can be extended
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExtendableBuiltIn {
    Button,
    Input,
    Anchor,
    Div,
    Span,
    Paragraph,
    ListItem,
    TableRow,
    TableCell,
    Image,
    Form,
    Select,
    TextArea,
    Other(String),
}

impl ExtendableBuiltIn {
    pub fn from_tag(tag: &str) -> Option<Self> {
        Some(match tag.to_lowercase().as_str() {
            "button" => Self::Button,
            "input" => Self::Input,
            "a" => Self::Anchor,
            "div" => Self::Div,
            "span" => Self::Span,
            "p" => Self::Paragraph,
            "li" => Self::ListItem,
            "tr" => Self::TableRow,
            "td" | "th" => Self::TableCell,
            "img" => Self::Image,
            "form" => Self::Form,
            "select" => Self::Select,
            "textarea" => Self::TextArea,
            _ => Self::Other(tag.to_string()),
        })
    }
    
    pub fn tag_name(&self) -> &str {
        match self {
            Self::Button => "button",
            Self::Input => "input",
            Self::Anchor => "a",
            Self::Div => "div",
            Self::Span => "span",
            Self::Paragraph => "p",
            Self::ListItem => "li",
            Self::TableRow => "tr",
            Self::TableCell => "td",
            Self::Image => "img",
            Self::Form => "form",
            Self::Select => "select",
            Self::TextArea => "textarea",
            Self::Other(s) => s,
        }
    }
}

/// Autonomous custom element definition
#[derive(Debug, Clone)]
pub struct AutonomousElement {
    /// Custom element name (must contain hyphen)
    pub name: String,
    /// Constructor function ID
    pub constructor_id: u64,
    /// Observed attributes  
    pub observed_attributes: Vec<String>,
    /// Form-associated flag
    pub form_associated: bool,
    /// Disable shadow flag
    pub disable_shadow: bool,
    /// Disable internals flag
    pub disable_internals: bool,
}

impl AutonomousElement {
    pub fn new(name: &str, constructor_id: u64) -> Result<Self, CustomElementError> {
        Self::validate_name(name)?;
        
        Ok(Self {
            name: name.to_string(),
            constructor_id,
            observed_attributes: Vec::new(),
            form_associated: false,
            disable_shadow: false,
            disable_internals: false,
        })
    }
    
    fn validate_name(name: &str) -> Result<(), CustomElementError> {
        // Must contain a hyphen
        if !name.contains('-') {
            return Err(CustomElementError::InvalidName(
                "Custom element name must contain a hyphen".to_string()
            ));
        }
        
        // Must start with lowercase ASCII
        if !name.starts_with(|c: char| c.is_ascii_lowercase()) {
            return Err(CustomElementError::InvalidName(
                "Custom element name must start with lowercase letter".to_string()
            ));
        }
        
        // Check for reserved names
        let reserved = ["annotation-xml", "color-profile", "font-face", 
                       "font-face-src", "font-face-uri", "font-face-format",
                       "font-face-name", "missing-glyph"];
        if reserved.contains(&name) {
            return Err(CustomElementError::InvalidName(
                format!("'{}' is a reserved element name", name)
            ));
        }
        
        Ok(())
    }
}

/// Customized built-in element definition
#[derive(Debug, Clone)]
pub struct CustomizedBuiltIn {
    /// Custom element name
    pub name: String,
    /// Extended element type
    pub extends: ExtendableBuiltIn,
    /// Constructor function ID
    pub constructor_id: u64,
    /// Observed attributes
    pub observed_attributes: Vec<String>,
}

impl CustomizedBuiltIn {
    pub fn new(name: &str, extends: &str, constructor_id: u64) -> Result<Self, CustomElementError> {
        // Validate name
        if !name.contains('-') {
            return Err(CustomElementError::InvalidName(
                "Custom element name must contain a hyphen".to_string()
            ));
        }
        
        let extends_type = ExtendableBuiltIn::from_tag(extends)
            .ok_or_else(|| CustomElementError::InvalidExtends(extends.to_string()))?;
        
        Ok(Self {
            name: name.to_string(),
            extends: extends_type,
            constructor_id,
            observed_attributes: Vec::new(),
        })
    }
}

/// Custom element error
#[derive(Debug, Clone, thiserror::Error)]
pub enum CustomElementError {
    #[error("Invalid custom element name: {0}")]
    InvalidName(String),
    
    #[error("Cannot extend element: {0}")]
    InvalidExtends(String),
    
    #[error("Element already defined: {0}")]
    AlreadyDefined(String),
    
    #[error("Invalid constructor")]
    InvalidConstructor,
}

/// Custom element registry (enhanced with autonomous/customized support)
#[derive(Debug, Default)]
pub struct EnhancedCustomElementRegistry {
    /// Autonomous elements
    autonomous: HashMap<String, AutonomousElement>,
    /// Customized built-in elements
    customized: HashMap<String, CustomizedBuiltIn>,
    /// Elements waiting for upgrade
    upgrade_queue: Vec<UpgradeCandidate>,
}

/// Element waiting for upgrade
#[derive(Debug, Clone)]
pub struct UpgradeCandidate {
    pub node_id: u32,
    pub name: String,
}

impl EnhancedCustomElementRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Define an autonomous custom element
    pub fn define_autonomous(&mut self, element: AutonomousElement) -> Result<(), CustomElementError> {
        if self.autonomous.contains_key(&element.name) || self.customized.contains_key(&element.name) {
            return Err(CustomElementError::AlreadyDefined(element.name.clone()));
        }
        
        let name = element.name.clone();
        self.autonomous.insert(name.clone(), element);
        
        // Upgrade queued elements
        self.upgrade_candidates(&name);
        
        Ok(())
    }
    
    /// Define a customized built-in element
    pub fn define_customized(&mut self, element: CustomizedBuiltIn) -> Result<(), CustomElementError> {
        if self.autonomous.contains_key(&element.name) || self.customized.contains_key(&element.name) {
            return Err(CustomElementError::AlreadyDefined(element.name.clone()));
        }
        
        let name = element.name.clone();
        self.customized.insert(name.clone(), element);
        
        // Upgrade queued elements
        self.upgrade_candidates(&name);
        
        Ok(())
    }
    
    /// Get autonomous element definition
    pub fn get_autonomous(&self, name: &str) -> Option<&AutonomousElement> {
        self.autonomous.get(name)
    }
    
    /// Get customized built-in element definition
    pub fn get_customized(&self, name: &str) -> Option<&CustomizedBuiltIn> {
        self.customized.get(name)
    }
    
    /// Check if name is defined
    pub fn is_defined(&self, name: &str) -> bool {
        self.autonomous.contains_key(name) || self.customized.contains_key(name)
    }
    
    /// Queue an element for upgrade
    pub fn queue_upgrade(&mut self, node_id: u32, name: &str) {
        self.upgrade_queue.push(UpgradeCandidate {
            node_id,
            name: name.to_string(),
        });
    }
    
    /// Get elements to upgrade
    fn upgrade_candidates(&mut self, name: &str) -> Vec<u32> {
        let (to_upgrade, remaining): (Vec<_>, Vec<_>) = self.upgrade_queue
            .drain(..)
            .partition(|c| c.name == name);
        
        self.upgrade_queue = remaining;
        to_upgrade.into_iter().map(|c| c.node_id).collect()
    }
    
    /// Wait for element to be defined
    pub fn when_defined(&self, name: &str) -> WhenDefinedFuture {
        WhenDefinedFuture {
            name: name.to_string(),
            defined: self.is_defined(name),
        }
    }
}

/// Future for whenDefined
#[derive(Debug)]
pub struct WhenDefinedFuture {
    pub name: String,
    pub defined: bool,
}

impl WhenDefinedFuture {
    pub fn is_ready(&self) -> bool {
        self.defined
    }
}

/// Element internals (for form-associated custom elements)
#[derive(Debug, Default)]
pub struct ElementInternals {
    /// Form owner ID
    pub form: Option<u32>,
    /// Validation message
    pub validation_message: String,
    /// Validity flags
    pub validity: InternalsValidity,
    /// Will validate
    pub will_validate: bool,
    /// Labels
    pub labels: Vec<u32>,
}

/// Validity state for internals
#[derive(Debug, Default)]
pub struct InternalsValidity {
    pub value_missing: bool,
    pub type_mismatch: bool,
    pub pattern_mismatch: bool,
    pub too_long: bool,
    pub too_short: bool,
    pub range_underflow: bool,
    pub range_overflow: bool,
    pub step_mismatch: bool,
    pub bad_input: bool,
    pub custom_error: bool,
}

impl InternalsValidity {
    pub fn valid(&self) -> bool {
        !self.value_missing &&
        !self.type_mismatch &&
        !self.pattern_mismatch &&
        !self.too_long &&
        !self.too_short &&
        !self.range_underflow &&
        !self.range_overflow &&
        !self.step_mismatch &&
        !self.bad_input &&
        !self.custom_error
    }
}

impl ElementInternals {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn set_validity(&mut self, flags: InternalsValidity, message: Option<&str>) {
        self.validity = flags;
        if let Some(msg) = message {
            self.validation_message = msg.to_string();
        }
    }
    
    pub fn check_validity(&self) -> bool {
        self.validity.valid()
    }
    
    pub fn report_validity(&self) -> bool {
        // In real impl, would show UI
        self.check_validity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_autonomous_element() {
        let elem = AutonomousElement::new("my-element", 1).unwrap();
        assert_eq!(elem.name, "my-element");
    }
    
    #[test]
    fn test_invalid_name() {
        let result = AutonomousElement::new("nohyphen", 1);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_customized_builtin() {
        let elem = CustomizedBuiltIn::new("fancy-button", "button", 1).unwrap();
        assert_eq!(elem.extends, ExtendableBuiltIn::Button);
    }
    
    #[test]
    fn test_registry() {
        let mut registry = EnhancedCustomElementRegistry::new();
        
        let elem = AutonomousElement::new("test-elem", 1).unwrap();
        registry.define_autonomous(elem).unwrap();
        
        assert!(registry.is_defined("test-elem"));
        assert!(registry.get_autonomous("test-elem").is_some());
    }
    
    #[test]
    fn test_element_internals() {
        let mut internals = ElementInternals::new();
        assert!(internals.check_validity());
        
        internals.validity.value_missing = true;
        assert!(!internals.check_validity());
    }
}
