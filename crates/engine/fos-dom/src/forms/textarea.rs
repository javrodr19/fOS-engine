//! Textarea Element Implementation
//!
//! Multi-line text input.

use super::validation::ValidityState;
use super::FormControl;

/// Textarea element
#[derive(Debug, Clone, Default)]
pub struct TextareaElement {
    // Core attributes
    pub name: Option<String>,
    pub value: String,
    pub default_value: String,
    pub form_id: Option<String>,
    
    // State
    pub disabled: bool,
    pub readonly: bool,
    pub required: bool,
    pub autofocus: bool,
    
    // Size
    pub rows: u32,
    pub cols: u32,
    
    // Constraints
    pub maxlength: Option<u32>,
    pub minlength: Option<u32>,
    pub placeholder: String,
    pub wrap: WrapMode,
    
    // Selection
    pub selection_start: usize,
    pub selection_end: usize,
    
    // Validation
    validity: ValidityState,
}

/// Text wrap mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WrapMode {
    #[default]
    Soft,
    Hard,
    Off,
}

impl TextareaElement {
    /// Create a new textarea
    pub fn new() -> Self {
        Self {
            rows: 2,
            cols: 20,
            ..Default::default()
        }
    }
    
    /// Set dimensions
    pub fn with_size(mut self, rows: u32, cols: u32) -> Self {
        self.rows = rows;
        self.cols = cols;
        self
    }
    
    /// Set name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
    
    /// Validate
    pub fn validate(&mut self) {
        self.validity = ValidityState::default();
        
        if self.required && self.value.is_empty() {
            self.validity.value_missing = true;
        }
        
        if let Some(max) = self.maxlength {
            if self.value.len() > max as usize {
                self.validity.too_long = true;
            }
        }
        
        if let Some(min) = self.minlength {
            if !self.value.is_empty() && self.value.len() < min as usize {
                self.validity.too_short = true;
            }
        }
    }
    
    /// Get text length
    pub fn text_length(&self) -> usize {
        self.value.len()
    }
    
    /// Select all text
    pub fn select(&mut self) {
        self.selection_start = 0;
        self.selection_end = self.value.len();
    }
}

impl FormControl for TextareaElement {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    
    fn value(&self) -> String {
        self.value.clone()
    }
    
    fn set_value(&mut self, value: &str) {
        self.value = value.to_string();
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
    fn test_textarea_basic() {
        let mut ta = TextareaElement::new()
            .with_name("comment")
            .with_size(5, 40);
        
        assert_eq!(ta.rows, 5);
        assert_eq!(ta.cols, 40);
        
        ta.set_value("Hello world");
        assert_eq!(ta.text_length(), 11);
    }
}
