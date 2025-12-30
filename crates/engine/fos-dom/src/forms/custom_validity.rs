//! Custom Validity API
//!
//! Extended form validation with custom validity support.

use std::collections::HashMap;

/// Validity state for a form control
#[derive(Debug, Clone, Default)]
pub struct ValidityState {
    /// Whether value is too long
    pub too_long: bool,
    /// Whether value is too short
    pub too_short: bool,
    /// Whether value is missing (required but empty)
    pub value_missing: bool,
    /// Whether value doesn't match pattern
    pub pattern_mismatch: bool,
    /// Whether value is below minimum
    pub range_underflow: bool,
    /// Whether value is above maximum
    pub range_overflow: bool,
    /// Whether value doesn't match step
    pub step_mismatch: bool,
    /// Whether value has invalid type
    pub type_mismatch: bool,
    /// Whether value fails custom validation
    pub custom_error: bool,
    /// Whether value has bad input
    pub bad_input: bool,
    /// Custom error message
    custom_message: String,
}

impl ValidityState {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if all constraints are satisfied
    pub fn valid(&self) -> bool {
        !self.too_long &&
        !self.too_short &&
        !self.value_missing &&
        !self.pattern_mismatch &&
        !self.range_underflow &&
        !self.range_overflow &&
        !self.step_mismatch &&
        !self.type_mismatch &&
        !self.custom_error &&
        !self.bad_input
    }
    
    /// Set custom error message
    pub fn set_custom_validity(&mut self, message: &str) {
        self.custom_message = message.to_string();
        self.custom_error = !message.is_empty();
    }
    
    /// Get custom error message
    pub fn get_custom_error(&self) -> &str {
        &self.custom_message
    }
    
    /// Clear custom error
    pub fn clear_custom_validity(&mut self) {
        self.custom_message.clear();
        self.custom_error = false;
    }
    
    /// Get validation message
    pub fn validation_message(&self) -> String {
        if !self.custom_message.is_empty() {
            return self.custom_message.clone();
        }
        
        if self.value_missing {
            return "Please fill out this field.".to_string();
        }
        
        if self.type_mismatch {
            return "Please enter a valid value.".to_string();
        }
        
        if self.pattern_mismatch {
            return "Please match the requested format.".to_string();
        }
        
        if self.too_long {
            return "Please shorten this text.".to_string();
        }
        
        if self.too_short {
            return "Please lengthen this text.".to_string();
        }
        
        if self.range_underflow {
            return "Value must be greater or equal to minimum.".to_string();
        }
        
        if self.range_overflow {
            return "Value must be less or equal to maximum.".to_string();
        }
        
        if self.step_mismatch {
            return "Please enter a valid value.".to_string();
        }
        
        if self.bad_input {
            return "Please enter a valid value.".to_string();
        }
        
        String::new()
    }
}

/// Form control with validation
#[derive(Debug, Clone)]
pub struct FormControl {
    /// Element ID
    pub id: u64,
    /// Current value
    pub value: String,
    /// Validity state
    pub validity: ValidityState,
    /// Whether validation is disabled
    pub no_validate: bool,
    /// Validation constraints
    pub constraints: ValidationConstraints,
}

/// Validation constraints for a form control
#[derive(Debug, Clone, Default)]
pub struct ValidationConstraints {
    /// Required attribute
    pub required: bool,
    /// Pattern regex
    pub pattern: Option<String>,
    /// Minimum length
    pub min_length: Option<usize>,
    /// Maximum length
    pub max_length: Option<usize>,
    /// Minimum value (for numeric)
    pub min: Option<f64>,
    /// Maximum value (for numeric)
    pub max: Option<f64>,
    /// Step value
    pub step: Option<f64>,
    /// Input type
    pub input_type: InputType,
}

/// Input type for validation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputType {
    #[default]
    Text,
    Email,
    Url,
    Number,
    Tel,
    Date,
    Time,
    DateTimeLocal,
    Month,
    Week,
    Color,
    File,
    Password,
    Search,
    Hidden,
}

impl FormControl {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            value: String::new(),
            validity: ValidityState::new(),
            no_validate: false,
            constraints: ValidationConstraints::default(),
        }
    }
    
    /// Check validity and update state
    pub fn check_validity(&mut self) -> bool {
        if self.no_validate {
            return true;
        }
        
        // Reset validity
        self.validity = ValidityState::new();
        
        // Check required
        if self.constraints.required && self.value.is_empty() {
            self.validity.value_missing = true;
        }
        
        // Check min length
        if let Some(min) = self.constraints.min_length {
            if !self.value.is_empty() && self.value.chars().count() < min {
                self.validity.too_short = true;
            }
        }
        
        // Check max length
        if let Some(max) = self.constraints.max_length {
            if self.value.chars().count() > max {
                self.validity.too_long = true;
            }
        }
        
        // Check pattern
        if let Some(ref pattern) = self.constraints.pattern {
            if !self.value.is_empty() && !self.matches_pattern(&self.value, pattern) {
                self.validity.pattern_mismatch = true;
            }
        }
        
        // Check type
        if !self.value.is_empty() {
            match self.constraints.input_type {
                InputType::Email => {
                    if !self.is_valid_email(&self.value) {
                        self.validity.type_mismatch = true;
                    }
                }
                InputType::Url => {
                    if !self.is_valid_url(&self.value) {
                        self.validity.type_mismatch = true;
                    }
                }
                InputType::Number => {
                    if self.value.parse::<f64>().is_err() {
                        self.validity.bad_input = true;
                    } else {
                        self.check_numeric_constraints();
                    }
                }
                _ => {}
            }
        }
        
        self.validity.valid()
    }
    
    fn check_numeric_constraints(&mut self) {
        if let Ok(num) = self.value.parse::<f64>() {
            if let Some(min) = self.constraints.min {
                if num < min {
                    self.validity.range_underflow = true;
                }
            }
            
            if let Some(max) = self.constraints.max {
                if num > max {
                    self.validity.range_overflow = true;
                }
            }
            
            if let Some(step) = self.constraints.step {
                let base = self.constraints.min.unwrap_or(0.0);
                let diff = num - base;
                if (diff % step).abs() > f64::EPSILON {
                    self.validity.step_mismatch = true;
                }
            }
        }
    }
    
    fn matches_pattern(&self, value: &str, pattern: &str) -> bool {
        // Simplified - full impl would use regex
        value.contains(pattern) || pattern.is_empty()
    }
    
    fn is_valid_email(&self, value: &str) -> bool {
        // Simplified email validation
        value.contains('@') && value.contains('.') && value.len() > 5
    }
    
    fn is_valid_url(&self, value: &str) -> bool {
        // Simplified URL validation
        value.starts_with("http://") || 
        value.starts_with("https://") ||
        value.starts_with("ftp://")
    }
    
    /// Set custom validity
    pub fn set_custom_validity(&mut self, message: &str) {
        self.validity.set_custom_validity(message);
    }
    
    /// Report validity (shows UI if invalid)
    pub fn report_validity(&mut self) -> bool {
        let valid = self.check_validity();
        // In a real impl, this would trigger UI feedback
        valid
    }
    
    /// Get will validate status
    pub fn will_validate(&self) -> bool {
        !self.no_validate
    }
}

/// Valid/Invalid pseudo-class state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationPseudoClass {
    Valid,
    Invalid,
    UserValid,
    UserInvalid,
}

impl ValidationPseudoClass {
    /// Check if element matches pseudo-class
    pub fn matches(element_valid: bool, user_interacted: bool, pseudo: ValidationPseudoClass) -> bool {
        match pseudo {
            ValidationPseudoClass::Valid => element_valid,
            ValidationPseudoClass::Invalid => !element_valid,
            ValidationPseudoClass::UserValid => user_interacted && element_valid,
            ValidationPseudoClass::UserInvalid => user_interacted && !element_valid,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validity_state() {
        let mut state = ValidityState::new();
        assert!(state.valid());
        
        state.value_missing = true;
        assert!(!state.valid());
    }
    
    #[test]
    fn test_custom_validity() {
        let mut state = ValidityState::new();
        state.set_custom_validity("Custom error");
        
        assert!(!state.valid());
        assert!(state.custom_error);
        assert_eq!(state.get_custom_error(), "Custom error");
    }
    
    #[test]
    fn test_form_control_required() {
        let mut control = FormControl::new(1);
        control.constraints.required = true;
        
        assert!(!control.check_validity());
        assert!(control.validity.value_missing);
    }
    
    #[test]
    fn test_form_control_email() {
        let mut control = FormControl::new(1);
        control.constraints.input_type = InputType::Email;
        control.value = "invalid".to_string();
        
        assert!(!control.check_validity());
        assert!(control.validity.type_mismatch);
    }
    
    #[test]
    fn test_pseudo_class_matching() {
        assert!(ValidationPseudoClass::matches(true, false, ValidationPseudoClass::Valid));
        assert!(ValidationPseudoClass::matches(false, false, ValidationPseudoClass::Invalid));
        assert!(ValidationPseudoClass::matches(true, true, ValidationPseudoClass::UserValid));
    }
}
