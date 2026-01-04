//! Constraint Validation API
//!
//! Full HTML5 constraint validation implementation.

use std::collections::HashMap;

/// Validity state
#[derive(Debug, Clone, Default)]
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
    pub custom_error: bool,
    custom_message: String,
}

impl ValidityState {
    pub fn valid(&self) -> bool {
        !self.value_missing && !self.type_mismatch && !self.pattern_mismatch &&
        !self.too_long && !self.too_short && !self.range_underflow &&
        !self.range_overflow && !self.step_mismatch && !self.bad_input && !self.custom_error
    }
    
    pub fn validation_message(&self) -> String {
        if self.custom_error { return self.custom_message.clone(); }
        if self.value_missing { return "Please fill out this field.".into(); }
        if self.type_mismatch { return "Please enter a valid value.".into(); }
        if self.pattern_mismatch { return "Please match the requested format.".into(); }
        if self.too_long { return "Please shorten this text.".into(); }
        if self.too_short { return "Please lengthen this text.".into(); }
        if self.range_underflow { return "Value must be greater.".into(); }
        if self.range_overflow { return "Value must be less.".into(); }
        if self.step_mismatch { return "Please enter a valid value.".into(); }
        if self.bad_input { return "Please enter a valid value.".into(); }
        String::new()
    }
    
    pub fn set_custom_validity(&mut self, message: &str) {
        self.custom_message = message.to_string();
        self.custom_error = !message.is_empty();
    }
}

/// Input type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputType {
    Text, Search, Tel, Url, Email, Password, Date, Month, Week, Time,
    DatetimeLocal, Number, Range, Color, Checkbox, Radio, File, Hidden,
    Submit, Reset, Button, Image,
}

impl InputType {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "search" => Self::Search, "tel" => Self::Tel, "url" => Self::Url,
            "email" => Self::Email, "password" => Self::Password, "date" => Self::Date,
            "month" => Self::Month, "week" => Self::Week, "time" => Self::Time,
            "datetime-local" => Self::DatetimeLocal, "number" => Self::Number,
            "range" => Self::Range, "color" => Self::Color, "checkbox" => Self::Checkbox,
            "radio" => Self::Radio, "file" => Self::File, "hidden" => Self::Hidden,
            "submit" => Self::Submit, "reset" => Self::Reset, "button" => Self::Button,
            "image" => Self::Image, _ => Self::Text,
        }
    }
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
    pub multiple: bool,
}

/// Validatable element
#[derive(Debug, Clone)]
pub struct ValidatableElement {
    pub id: u64,
    pub name: String,
    pub input_type: Option<InputType>,
    pub value: String,
    pub constraints: ValidationConstraints,
    pub validity: ValidityState,
}

impl ValidatableElement {
    pub fn new(id: u64, name: &str) -> Self {
        Self { id, name: name.into(), input_type: None, value: String::new(),
               constraints: ValidationConstraints::default(), validity: ValidityState::default() }
    }
    
    pub fn check_validity(&mut self) -> bool {
        self.validate();
        self.validity.valid()
    }
    
    pub fn validate(&mut self) {
        self.validity = ValidityState::default();
        if self.constraints.required && self.value.is_empty() {
            self.validity.value_missing = true; return;
        }
        if self.value.is_empty() { return; }
        
        if let Some(input_type) = self.input_type {
            match input_type {
                InputType::Email => if !self.value.contains('@') { self.validity.type_mismatch = true; }
                InputType::Url => if !self.value.starts_with("http") { self.validity.type_mismatch = true; }
                InputType::Number | InputType::Range => {
                    match self.value.parse::<f64>() {
                        Ok(n) => {
                            if let Some(min) = self.constraints.min { if n < min { self.validity.range_underflow = true; } }
                            if let Some(max) = self.constraints.max { if n > max { self.validity.range_overflow = true; } }
                        }
                        Err(_) => self.validity.bad_input = true,
                    }
                }
                _ => {}
            }
        }
        if let Some(min) = self.constraints.min_length { if self.value.len() < min { self.validity.too_short = true; } }
        if let Some(max) = self.constraints.max_length { if self.value.len() > max { self.validity.too_long = true; } }
    }
}

/// Form validator
#[derive(Debug, Default)]
pub struct FormValidator {
    elements: HashMap<u64, ValidatableElement>,
    pub novalidate: bool,
}

impl FormValidator {
    pub fn new() -> Self { Self::default() }
    pub fn add_element(&mut self, elem: ValidatableElement) { self.elements.insert(elem.id, elem); }
    pub fn get_element_mut(&mut self, id: u64) -> Option<&mut ValidatableElement> { self.elements.get_mut(&id) }
    pub fn check_validity(&mut self) -> bool {
        if self.novalidate { return true; }
        self.elements.values_mut().all(|e| e.check_validity())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_validity() {
        let mut elem = ValidatableElement::new(1, "email");
        elem.input_type = Some(InputType::Email);
        elem.constraints.required = true;
        elem.value = "test@example.com".into();
        assert!(elem.check_validity());
    }
}
