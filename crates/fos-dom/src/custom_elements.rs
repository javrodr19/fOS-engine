//! Custom Elements
//!
//! Custom element registry and lifecycle callbacks.

use std::collections::HashMap;

/// Custom elements registry
#[derive(Debug, Default)]
pub struct CustomElementRegistry {
    definitions: HashMap<String, CustomElementDefinition>,
    when_defined: HashMap<String, Vec<u32>>, // callback IDs
}

/// Custom element definition
#[derive(Debug, Clone)]
pub struct CustomElementDefinition {
    pub name: String,
    pub constructor_id: u32, // JS callback ID
    pub extends: Option<String>,
    pub observed_attributes: Vec<String>,
    pub form_associated: bool,
    pub disable_shadow: bool,
    pub disable_internals: bool,
}

/// Custom element lifecycle callbacks
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleCallback {
    Connected,
    Disconnected,
    Adopted,
    AttributeChanged,
    FormAssociated,
    FormDisabled,
    FormReset,
    FormStateRestore,
}

/// Lifecycle callback info
#[derive(Debug, Clone)]
pub struct LifecycleCallbackInfo {
    pub callback: LifecycleCallback,
    pub element_id: u32,
    pub attribute_name: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

impl CustomElementRegistry {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Define a custom element
    pub fn define(
        &mut self,
        name: &str,
        constructor_id: u32,
        options: CustomElementOptions,
    ) -> Result<(), CustomElementError> {
        // Validate name (must contain hyphen, lowercase, not reserved)
        if !Self::is_valid_name(name) {
            return Err(CustomElementError::InvalidName);
        }
        
        // Check if already defined
        if self.definitions.contains_key(name) {
            return Err(CustomElementError::AlreadyDefined);
        }
        
        let definition = CustomElementDefinition {
            name: name.to_string(),
            constructor_id,
            extends: options.extends,
            observed_attributes: options.observed_attributes,
            form_associated: options.form_associated,
            disable_shadow: options.disable_shadow,
            disable_internals: options.disable_internals,
        };
        
        self.definitions.insert(name.to_string(), definition);
        
        // Trigger whenDefined callbacks
        if let Some(callbacks) = self.when_defined.remove(name) {
            // Would invoke callbacks here
            let _ = callbacks;
        }
        
        Ok(())
    }
    
    /// Get element definition
    pub fn get(&self, name: &str) -> Option<&CustomElementDefinition> {
        self.definitions.get(name)
    }
    
    /// Check if element is defined
    pub fn is_defined(&self, name: &str) -> bool {
        self.definitions.contains_key(name)
    }
    
    /// Register whenDefined callback
    pub fn when_defined(&mut self, name: &str, callback_id: u32) {
        if self.is_defined(name) {
            // Already defined, would invoke immediately
            return;
        }
        
        self.when_defined
            .entry(name.to_string())
            .or_insert_with(Vec::new)
            .push(callback_id);
    }
    
    /// Upgrade an element
    pub fn upgrade(&self, _element_id: u32) {
        // Would upgrade element to custom element
    }
    
    /// Validate custom element name
    fn is_valid_name(name: &str) -> bool {
        // Must contain hyphen
        if !name.contains('-') {
            return false;
        }
        
        // Must start with lowercase letter
        if !name.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false) {
            return false;
        }
        
        // Reserved names
        let reserved = ["annotation-xml", "color-profile", "font-face", 
                       "font-face-src", "font-face-uri", "font-face-format",
                       "font-face-name", "missing-glyph"];
        if reserved.contains(&name) {
            return false;
        }
        
        true
    }
}

/// Custom element options
#[derive(Debug, Clone, Default)]
pub struct CustomElementOptions {
    pub extends: Option<String>,
    pub observed_attributes: Vec<String>,
    pub form_associated: bool,
    pub disable_shadow: bool,
    pub disable_internals: bool,
}

/// Custom element errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CustomElementError {
    InvalidName,
    AlreadyDefined,
    ExtensionNotAllowed,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_valid_names() {
        assert!(CustomElementRegistry::is_valid_name("my-element"));
        assert!(CustomElementRegistry::is_valid_name("app-header"));
        assert!(!CustomElementRegistry::is_valid_name("myelement")); // no hyphen
        assert!(!CustomElementRegistry::is_valid_name("My-Element")); // uppercase
    }
    
    #[test]
    fn test_define() {
        let mut registry = CustomElementRegistry::new();
        
        assert!(registry.define("my-element", 1, CustomElementOptions::default()).is_ok());
        assert!(registry.is_defined("my-element"));
        
        // Duplicate
        assert_eq!(
            registry.define("my-element", 2, CustomElementOptions::default()),
            Err(CustomElementError::AlreadyDefined)
        );
    }
}
