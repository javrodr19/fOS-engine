//! Input Element Implementation
//!
//! Supports all HTML5 input types: text, password, email, number, date, etc.

use super::validation::ValidityState;
use super::FormControl;

/// HTML input types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputType {
    #[default]
    Text,
    Password,
    Email,
    Number,
    Tel,
    Url,
    Search,
    Date,
    Time,
    DatetimeLocal,
    Month,
    Week,
    Color,
    Range,
    File,
    Hidden,
    Checkbox,
    Radio,
    Submit,
    Reset,
    Button,
    Image,
}

impl InputType {
    /// Parse from string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "text" => Self::Text,
            "password" => Self::Password,
            "email" => Self::Email,
            "number" => Self::Number,
            "tel" => Self::Tel,
            "url" => Self::Url,
            "search" => Self::Search,
            "date" => Self::Date,
            "time" => Self::Time,
            "datetime-local" => Self::DatetimeLocal,
            "month" => Self::Month,
            "week" => Self::Week,
            "color" => Self::Color,
            "range" => Self::Range,
            "file" => Self::File,
            "hidden" => Self::Hidden,
            "checkbox" => Self::Checkbox,
            "radio" => Self::Radio,
            "submit" => Self::Submit,
            "reset" => Self::Reset,
            "button" => Self::Button,
            "image" => Self::Image,
            _ => Self::Text,
        }
    }
    
    /// Check if this is a text-like input
    pub fn is_text_like(&self) -> bool {
        matches!(self, Self::Text | Self::Password | Self::Email | 
                       Self::Number | Self::Tel | Self::Url | Self::Search)
    }
    
    /// Check if this is a button type
    pub fn is_button(&self) -> bool {
        matches!(self, Self::Submit | Self::Reset | Self::Button | Self::Image)
    }
}

/// Input value types
#[derive(Debug, Clone)]
pub enum InputValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Date(String), // ISO 8601 format
    Files(Vec<String>), // File paths
}

impl Default for InputValue {
    fn default() -> Self {
        Self::Text(String::new())
    }
}

impl InputValue {
    pub fn as_string(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Number(n) => n.to_string(),
            Self::Boolean(b) => if *b { "on".to_string() } else { String::new() },
            Self::Date(s) => s.clone(),
            Self::Files(f) => f.join(","),
        }
    }
}

/// Input element
#[derive(Debug, Clone, Default)]
pub struct InputElement {
    // Core attributes
    pub input_type: InputType,
    pub name: Option<String>,
    pub value: InputValue,
    pub default_value: String,
    pub form_id: Option<String>,
    
    // State
    pub disabled: bool,
    pub readonly: bool,
    pub required: bool,
    pub autofocus: bool,
    pub autocomplete: String,
    
    // Text input attributes
    pub placeholder: String,
    pub maxlength: Option<u32>,
    pub minlength: Option<u32>,
    pub pattern: Option<String>,
    
    // Number/range attributes
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    
    // Checkbox/radio
    pub checked: bool,
    pub default_checked: bool,
    
    // Selection state
    pub selection_start: usize,
    pub selection_end: usize,
    pub selection_direction: SelectionDirection,
    
    // Validation
    validity: ValidityState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SelectionDirection {
    #[default]
    None,
    Forward,
    Backward,
}

impl InputElement {
    /// Create a new input element
    pub fn new(input_type: InputType) -> Self {
        Self {
            input_type,
            ..Default::default()
        }
    }
    
    /// Create a text input
    pub fn text() -> Self {
        Self::new(InputType::Text)
    }
    
    /// Create a checkbox
    pub fn checkbox() -> Self {
        Self::new(InputType::Checkbox)
    }
    
    /// Set the name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
    
    /// Set placeholder
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }
    
    /// Set required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }
    
    /// Validate the input value
    pub fn validate(&mut self) {
        self.validity = ValidityState::default();
        
        let value_str = self.value.as_string();
        
        // Required check
        if self.required && value_str.is_empty() {
            self.validity.value_missing = true;
        }
        
        // Length checks (for text types)
        if self.input_type.is_text_like() {
            if let Some(max) = self.maxlength {
                if value_str.len() > max as usize {
                    self.validity.too_long = true;
                }
            }
            if let Some(min) = self.minlength {
                if !value_str.is_empty() && value_str.len() < min as usize {
                    self.validity.too_short = true;
                }
            }
        }
        
        // Pattern check
        if let Some(ref _pattern) = self.pattern {
            // Would use regex here
            // For now, simplified
        }
        
        // Type-specific validation
        match self.input_type {
            InputType::Email => {
                if !value_str.is_empty() && !value_str.contains('@') {
                    self.validity.type_mismatch = true;
                }
            }
            InputType::Url => {
                if !value_str.is_empty() && !value_str.starts_with("http") {
                    self.validity.type_mismatch = true;
                }
            }
            InputType::Number | InputType::Range => {
                if let InputValue::Number(n) = &self.value {
                    if let Some(min) = self.min {
                        if *n < min {
                            self.validity.range_underflow = true;
                        }
                    }
                    if let Some(max) = self.max {
                        if *n > max {
                            self.validity.range_overflow = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    
    /// Step up the value (for number/date inputs)
    pub fn step_up(&mut self, n: i32) {
        if let InputValue::Number(val) = &mut self.value {
            let step = self.step.unwrap_or(1.0);
            *val += step * n as f64;
            self.validate();
        }
    }
    
    /// Step down the value
    pub fn step_down(&mut self, n: i32) {
        self.step_up(-n);
    }
    
    /// Select all text
    pub fn select(&mut self) {
        self.selection_start = 0;
        self.selection_end = self.value.as_string().len();
    }
    
    /// Set selection range
    pub fn set_selection_range(&mut self, start: usize, end: usize, direction: SelectionDirection) {
        self.selection_start = start;
        self.selection_end = end;
        self.selection_direction = direction;
    }
}

impl FormControl for InputElement {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    
    fn value(&self) -> String {
        self.value.as_string()
    }
    
    fn set_value(&mut self, value: &str) {
        self.value = match self.input_type {
            InputType::Number | InputType::Range => {
                InputValue::Number(value.parse().unwrap_or(0.0))
            }
            InputType::Checkbox | InputType::Radio => {
                InputValue::Boolean(value == "on" || value == "true")
            }
            _ => InputValue::Text(value.to_string()),
        };
        self.validate();
    }
    
    fn is_disabled(&self) -> bool {
        self.disabled
    }
    
    fn form_id(&self) -> Option<&str> {
        self.form_id.as_deref()
    }
    
    fn validity(&self) -> &ValidityState {
        &self.validity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_input_type_parse() {
        assert_eq!(InputType::parse("text"), InputType::Text);
        assert_eq!(InputType::parse("email"), InputType::Email);
        assert_eq!(InputType::parse("checkbox"), InputType::Checkbox);
    }
    
    #[test]
    fn test_required_validation() {
        let mut input = InputElement::text().required();
        input.validate();
        assert!(input.validity.value_missing);
        
        input.set_value("hello");
        assert!(!input.validity.value_missing);
    }
    
    #[test]
    fn test_email_validation() {
        let mut input = InputElement::new(InputType::Email);
        input.set_value("invalid");
        assert!(input.validity.type_mismatch);
        
        input.set_value("valid@example.com");
        assert!(!input.validity.type_mismatch);
    }
}
