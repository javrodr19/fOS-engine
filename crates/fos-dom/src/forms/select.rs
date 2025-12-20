//! Select and Option Element Implementation
//!
//! Dropdown and list selection.

use super::validation::ValidityState;
use super::FormControl;

/// Select element
#[derive(Debug, Clone, Default)]
pub struct SelectElement {
    // Core attributes
    pub name: Option<String>,
    pub form_id: Option<String>,
    
    // State
    pub disabled: bool,
    pub required: bool,
    pub autofocus: bool,
    
    // Options
    pub options: Vec<OptionElement>,
    pub option_groups: Vec<OptionGroup>,
    
    // Multiple selection
    pub multiple: bool,
    pub size: u32, // Visible options
    
    // Selected index
    pub selected_index: i32,
    
    // Validation
    validity: ValidityState,
}

/// Option element
#[derive(Debug, Clone, Default)]
pub struct OptionElement {
    pub value: String,
    pub text: String,
    pub selected: bool,
    pub disabled: bool,
    pub default_selected: bool,
}

/// Option group
#[derive(Debug, Clone, Default)]
pub struct OptionGroup {
    pub label: String,
    pub disabled: bool,
    pub options: Vec<OptionElement>,
}

impl OptionElement {
    /// Create a new option
    pub fn new(value: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            text: text.into(),
            ..Default::default()
        }
    }
    
    /// Mark as selected
    pub fn selected(mut self) -> Self {
        self.selected = true;
        self.default_selected = true;
        self
    }
    
    /// Mark as disabled
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

impl SelectElement {
    /// Create a new select element
    pub fn new() -> Self {
        Self {
            size: 1,
            selected_index: -1,
            ..Default::default()
        }
    }
    
    /// Set name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
    
    /// Add an option
    pub fn add_option(&mut self, option: OptionElement) {
        if option.selected && self.selected_index < 0 {
            self.selected_index = self.options.len() as i32;
        }
        self.options.push(option);
    }
    
    /// Add multiple options
    pub fn with_options(mut self, options: Vec<OptionElement>) -> Self {
        for opt in options {
            self.add_option(opt);
        }
        self
    }
    
    /// Get selected option(s)
    pub fn selected_options(&self) -> Vec<&OptionElement> {
        self.options.iter().filter(|o| o.selected).collect()
    }
    
    /// Get selected value
    pub fn selected_value(&self) -> Option<&str> {
        if self.selected_index >= 0 && (self.selected_index as usize) < self.options.len() {
            Some(&self.options[self.selected_index as usize].value)
        } else {
            None
        }
    }
    
    /// Set selected by index
    pub fn set_selected_index(&mut self, index: i32) {
        // Deselect all if not multiple
        if !self.multiple {
            for opt in &mut self.options {
                opt.selected = false;
            }
        }
        
        if index >= 0 && (index as usize) < self.options.len() {
            self.selected_index = index;
            self.options[index as usize].selected = true;
        }
        
        self.validate();
    }
    
    /// Validate
    pub fn validate(&mut self) {
        self.validity = ValidityState::default();
        
        if self.required && self.selected_index < 0 {
            self.validity.value_missing = true;
        }
    }
    
    /// Get length (number of options)
    pub fn length(&self) -> usize {
        self.options.len()
    }
}

impl FormControl for SelectElement {
    fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    
    fn value(&self) -> String {
        self.selected_value().unwrap_or("").to_string()
    }
    
    fn set_value(&mut self, value: &str) {
        for (i, opt) in self.options.iter().enumerate() {
            if opt.value == value {
                self.set_selected_index(i as i32);
                return;
            }
        }
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
    fn test_select_options() {
        let mut select = SelectElement::new()
            .with_name("country")
            .with_options(vec![
                OptionElement::new("us", "United States"),
                OptionElement::new("uk", "United Kingdom").selected(),
                OptionElement::new("de", "Germany"),
            ]);
        
        assert_eq!(select.length(), 3);
        assert_eq!(select.selected_value(), Some("uk"));
    }
    
    #[test]
    fn test_select_change() {
        let mut select = SelectElement::new().with_options(vec![
            OptionElement::new("a", "A"),
            OptionElement::new("b", "B"),
        ]);
        
        select.set_selected_index(1);
        assert_eq!(select.value(), "b");
    }
}
