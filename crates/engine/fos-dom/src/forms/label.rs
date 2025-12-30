//! Label, Fieldset, and Legend Elements
//!
//! Semantic form grouping elements.

/// Label element - associates text with a form control
#[derive(Debug, Clone, Default)]
pub struct LabelElement {
    /// ID of the associated control (for attribute)
    pub for_id: Option<String>,
    /// Label text content
    pub text: String,
    /// Form ID
    pub form_id: Option<String>,
}

impl LabelElement {
    /// Create a new label
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Default::default()
        }
    }
    
    /// Associate with a control
    pub fn for_control(mut self, id: impl Into<String>) -> Self {
        self.for_id = Some(id.into());
        self
    }
    
    /// Get the control ID this label is for
    pub fn html_for(&self) -> Option<&str> {
        self.for_id.as_deref()
    }
}

/// Fieldset element - groups related form controls
#[derive(Debug, Clone, Default)]
pub struct FieldsetElement {
    /// Fieldset name
    pub name: Option<String>,
    /// Associated form ID
    pub form_id: Option<String>,
    /// Disabled state (disables all descendants)
    pub disabled: bool,
    /// Legend element
    pub legend: Option<LegendElement>,
    /// Child element IDs
    element_ids: Vec<String>,
}

impl FieldsetElement {
    /// Create a new fieldset
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set the legend
    pub fn with_legend(mut self, text: impl Into<String>) -> Self {
        self.legend = Some(LegendElement::new(text));
        self
    }
    
    /// Add an element to the fieldset
    pub fn add_element(&mut self, id: String) {
        self.element_ids.push(id);
    }
    
    /// Get element IDs
    pub fn elements(&self) -> &[String] {
        &self.element_ids
    }
    
    /// Check if fieldset is disabled
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
}

/// Legend element - caption for a fieldset
#[derive(Debug, Clone, Default)]
pub struct LegendElement {
    /// Legend text
    pub text: String,
}

impl LegendElement {
    /// Create a new legend
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_label() {
        let label = LabelElement::new("Username")
            .for_control("username");
        
        assert_eq!(label.html_for(), Some("username"));
        assert_eq!(label.text, "Username");
    }
    
    #[test]
    fn test_fieldset() {
        let fieldset = FieldsetElement::new()
            .with_legend("Personal Information");
        
        assert!(fieldset.legend.is_some());
        assert_eq!(fieldset.legend.unwrap().text, "Personal Information");
    }
}
