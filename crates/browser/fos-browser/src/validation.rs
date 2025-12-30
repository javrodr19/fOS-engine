//! Input Validation
//!
//! HTML5 form input validation.

use std::collections::HashMap;

/// Validation constraint violation
#[derive(Debug, Clone)]
pub struct ValidityState {
    pub value_missing: bool,
    pub type_mismatch: bool,
    pub pattern_mismatch: bool,
    pub too_long: bool,
    pub too_short: bool,
    pub range_underflow: bool,
    pub range_overflow: bool,
    pub step_mismatch: bool,
    pub bad_input: bool,
    pub custom_error: Option<String>,
}

impl Default for ValidityState {
    fn default() -> Self {
        Self {
            value_missing: false,
            type_mismatch: false,
            pattern_mismatch: false,
            too_long: false,
            too_short: false,
            range_underflow: false,
            range_overflow: false,
            step_mismatch: false,
            bad_input: false,
            custom_error: None,
        }
    }
}

impl ValidityState {
    pub fn is_valid(&self) -> bool {
        !self.value_missing
            && !self.type_mismatch
            && !self.pattern_mismatch
            && !self.too_long
            && !self.too_short
            && !self.range_underflow
            && !self.range_overflow
            && !self.step_mismatch
            && !self.bad_input
            && self.custom_error.is_none()
    }
    
    pub fn validation_message(&self) -> String {
        if let Some(ref msg) = self.custom_error {
            return msg.clone();
        }
        if self.value_missing { return "Please fill out this field.".to_string(); }
        if self.type_mismatch { return "Please enter a valid value.".to_string(); }
        if self.pattern_mismatch { return "Please match the requested format.".to_string(); }
        if self.too_long { return "Please shorten this text.".to_string(); }
        if self.too_short { return "Please lengthen this text.".to_string(); }
        if self.range_underflow { return "Value must be greater.".to_string(); }
        if self.range_overflow { return "Value must be less.".to_string(); }
        if self.step_mismatch { return "Please enter a valid value.".to_string(); }
        if self.bad_input { return "Please enter a valid value.".to_string(); }
        String::new()
    }
}

/// Input type for validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    Text,
    Email,
    Url,
    Number,
    Tel,
    Date,
    Time,
    Color,
    Range,
    Password,
    Search,
    Hidden,
}

/// Validation constraints
#[derive(Debug, Clone, Default)]
pub struct ValidationConstraints {
    pub required: bool,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub pattern: Option<String>,
}

/// Input validator
#[derive(Debug, Default)]
pub struct InputValidator {
    custom_validators: HashMap<String, fn(&str) -> Option<String>>,
}

impl InputValidator {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Validate an input value
    pub fn validate(
        &self,
        value: &str,
        input_type: InputType,
        constraints: &ValidationConstraints,
    ) -> ValidityState {
        let mut state = ValidityState::default();
        
        // Check required
        if constraints.required && value.is_empty() {
            state.value_missing = true;
            return state;
        }
        
        // Skip other checks for empty optional fields
        if value.is_empty() {
            return state;
        }
        
        // Type-specific validation
        match input_type {
            InputType::Email => {
                if !value.contains('@') || !value.contains('.') {
                    state.type_mismatch = true;
                }
            }
            InputType::Url => {
                if !value.starts_with("http://") && !value.starts_with("https://") {
                    state.type_mismatch = true;
                }
            }
            InputType::Number | InputType::Range => {
                if let Ok(num) = value.parse::<f64>() {
                    if let Some(min) = constraints.min {
                        if num < min { state.range_underflow = true; }
                    }
                    if let Some(max) = constraints.max {
                        if num > max { state.range_overflow = true; }
                    }
                    if let Some(step) = constraints.step {
                        let base = constraints.min.unwrap_or(0.0);
                        let diff = num - base;
                        if (diff / step).fract().abs() > 1e-10 {
                            state.step_mismatch = true;
                        }
                    }
                } else {
                    state.bad_input = true;
                }
            }
            InputType::Tel => {
                // Simple phone validation
                if !value.chars().all(|c| c.is_ascii_digit() || c == '+' || c == '-' || c == ' ') {
                    state.type_mismatch = true;
                }
            }
            _ => {}
        }
        
        // Length constraints
        if let Some(min) = constraints.min_length {
            if value.len() < min { state.too_short = true; }
        }
        if let Some(max) = constraints.max_length {
            if value.len() > max { state.too_long = true; }
        }
        
        // Pattern matching (simple substring for now)
        if let Some(ref pattern) = constraints.pattern {
            // In real implementation, use regex
            if !value.contains(pattern.as_str()) {
                state.pattern_mismatch = true;
            }
        }
        
        state
    }
    
    /// Register custom validator
    pub fn register_custom(&mut self, name: &str, validator: fn(&str) -> Option<String>) {
        self.custom_validators.insert(name.to_string(), validator);
    }
    
    /// Run custom validator
    pub fn validate_custom(&self, name: &str, value: &str) -> Option<String> {
        self.custom_validators.get(name).and_then(|f| f(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_email_validation() {
        let validator = InputValidator::new();
        let constraints = ValidationConstraints::default();
        
        let result = validator.validate("test@example.com", InputType::Email, &constraints);
        assert!(result.is_valid());
        
        let result = validator.validate("invalid", InputType::Email, &constraints);
        assert!(result.type_mismatch);
    }
    
    #[test]
    fn test_required() {
        let validator = InputValidator::new();
        let constraints = ValidationConstraints { required: true, ..Default::default() };
        
        let result = validator.validate("", InputType::Text, &constraints);
        assert!(result.value_missing);
    }
}
