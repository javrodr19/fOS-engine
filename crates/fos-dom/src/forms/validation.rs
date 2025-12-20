//! Form Validation
//!
//! Constraint Validation API implementation.

/// Validity state for form controls
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidityState {
    /// The element's value is missing (for required)
    pub value_missing: bool,
    /// The element's value doesn't match the type
    pub type_mismatch: bool,
    /// The element's value doesn't match the pattern
    pub pattern_mismatch: bool,
    /// The element's value is too long
    pub too_long: bool,
    /// The element's value is too short
    pub too_short: bool,
    /// The element's value is below the minimum
    pub range_underflow: bool,
    /// The element's value is above the maximum
    pub range_overflow: bool,
    /// The element's value doesn't match step
    pub step_mismatch: bool,
    /// The element has a bad input format
    pub bad_input: bool,
    /// Custom validity message set
    pub custom_error: bool,
}

impl ValidityState {
    /// Check if the element is valid
    pub fn is_valid(&self) -> bool {
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

/// Constraint validation support
pub trait ConstraintValidation {
    /// Get validity state
    fn validity(&self) -> &ValidityState;
    
    /// Check if valid
    fn check_validity(&self) -> bool {
        self.validity().is_valid()
    }
    
    /// Report validity (may show UI)
    fn report_validity(&self) -> bool {
        self.check_validity()
    }
    
    /// Set custom validity message
    fn set_custom_validity(&mut self, message: &str);
    
    /// Get validation message
    fn validation_message(&self) -> String;
    
    /// Will validate (if form would validate this control)
    fn will_validate(&self) -> bool;
}

/// Validation constraints
#[derive(Debug, Clone, Default)]
pub struct ValidationConstraints {
    pub required: bool,
    pub pattern: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub min_length: Option<u32>,
    pub max_length: Option<u32>,
    pub step: Option<f64>,
}

impl ValidationConstraints {
    /// Validate a string value
    pub fn validate_string(&self, value: &str) -> ValidityState {
        let mut state = ValidityState::default();
        
        if self.required && value.is_empty() {
            state.value_missing = true;
        }
        
        if let Some(max) = self.max_length {
            if value.len() > max as usize {
                state.too_long = true;
            }
        }
        
        if let Some(min) = self.min_length {
            if !value.is_empty() && value.len() < min as usize {
                state.too_short = true;
            }
        }
        
        // Pattern matching would use regex
        
        state
    }
    
    /// Validate a numeric value
    pub fn validate_number(&self, value: f64) -> ValidityState {
        let mut state = ValidityState::default();
        
        if let Some(min) = self.min {
            if value < min {
                state.range_underflow = true;
            }
        }
        
        if let Some(max) = self.max {
            if value > max {
                state.range_overflow = true;
            }
        }
        
        if let Some(step) = self.step {
            let base = self.min.unwrap_or(0.0);
            let diff = value - base;
            if (diff % step).abs() > 1e-10 {
                state.step_mismatch = true;
            }
        }
        
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validity_state_valid() {
        let state = ValidityState::default();
        assert!(state.is_valid());
    }
    
    #[test]
    fn test_validity_state_invalid() {
        let state = ValidityState {
            value_missing: true,
            ..Default::default()
        };
        assert!(!state.is_valid());
    }
    
    #[test]
    fn test_string_validation() {
        let constraints = ValidationConstraints {
            required: true,
            min_length: Some(3),
            max_length: Some(10),
            ..Default::default()
        };
        
        let result = constraints.validate_string("");
        assert!(result.value_missing);
        
        let result = constraints.validate_string("ab");
        assert!(result.too_short);
        
        let result = constraints.validate_string("hello");
        assert!(result.is_valid());
    }
    
    #[test]
    fn test_number_validation() {
        let constraints = ValidationConstraints {
            min: Some(0.0),
            max: Some(100.0),
            step: Some(5.0),
            ..Default::default()
        };
        
        let result = constraints.validate_number(-1.0);
        assert!(result.range_underflow);
        
        let result = constraints.validate_number(101.0);
        assert!(result.range_overflow);
        
        let result = constraints.validate_number(50.0);
        assert!(result.is_valid());
    }
}
