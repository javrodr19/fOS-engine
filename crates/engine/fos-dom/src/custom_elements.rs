//! Custom Elements v1
//!
//! Custom element registry, lifecycle callbacks, and ElementInternals.
//! Uses StringInterner for memory-efficient name storage.

use std::collections::HashMap;
use crate::{InternedString, NodeId};

/// Custom elements registry with interned names
#[derive(Debug, Default)]
pub struct CustomElementRegistry {
    /// Definitions keyed by interned name
    definitions: HashMap<InternedString, CustomElementDefinition>,
    /// Name to interned string mapping for lookup
    name_index: HashMap<String, InternedString>,
    /// whenDefined callbacks (callback IDs waiting for definition)
    when_defined: HashMap<String, Vec<u32>>,
    /// Upgrade candidates (elements waiting to be upgraded)
    upgrade_candidates: Vec<UpgradeCandidate>,
}

/// Custom element definition with interned strings
#[derive(Debug, Clone)]
pub struct CustomElementDefinition {
    /// Element name (interned)
    pub name: InternedString,
    /// Original name string for display
    pub name_str: String,
    /// JS constructor callback ID
    pub constructor_id: u32,
    /// Base element this extends (for customized built-ins)
    pub extends: Option<InternedString>,
    /// Observed attributes (interned for efficient comparison)
    pub observed_attributes: Vec<InternedString>,
    /// Whether this element is form-associated
    pub form_associated: bool,
    /// Disable shadow root attachment
    pub disable_shadow: bool,
    /// Disable ElementInternals access
    pub disable_internals: bool,
}

/// Element waiting to be upgraded
#[derive(Debug, Clone)]
pub struct UpgradeCandidate {
    pub node_id: NodeId,
    pub name: InternedString,
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

/// Lifecycle callback info with interned attribute names
#[derive(Debug, Clone)]
pub struct LifecycleCallbackInfo {
    pub callback: LifecycleCallback,
    pub element_id: NodeId,
    pub attribute_name: Option<InternedString>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

/// Pending callback queue for batch processing
#[derive(Debug, Default)]
pub struct CallbackQueue {
    callbacks: Vec<LifecycleCallbackInfo>,
}

impl CallbackQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn enqueue(&mut self, info: LifecycleCallbackInfo) {
        self.callbacks.push(info);
    }

    pub fn drain(&mut self) -> Vec<LifecycleCallbackInfo> {
        std::mem::take(&mut self.callbacks)
    }

    pub fn is_empty(&self) -> bool {
        self.callbacks.is_empty()
    }

    pub fn len(&self) -> usize {
        self.callbacks.len()
    }
}

impl CustomElementRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Define a custom element
    pub fn define(
        &mut self,
        name: &str,
        name_interned: InternedString,
        constructor_id: u32,
        options: CustomElementOptions,
    ) -> Result<(), CustomElementError> {
        // Validate name
        if !Self::is_valid_name(name) {
            return Err(CustomElementError::InvalidName);
        }

        // Check if already defined
        if self.name_index.contains_key(name) {
            return Err(CustomElementError::AlreadyDefined);
        }

        let definition = CustomElementDefinition {
            name: name_interned,
            name_str: name.to_string(),
            constructor_id,
            extends: options.extends,
            observed_attributes: options.observed_attributes,
            form_associated: options.form_associated,
            disable_shadow: options.disable_shadow,
            disable_internals: options.disable_internals,
        };

        self.definitions.insert(name_interned, definition);
        self.name_index.insert(name.to_string(), name_interned);

        // Trigger whenDefined callbacks
        if let Some(callbacks) = self.when_defined.remove(name) {
            // These would be invoked via JS runtime
            let _ = callbacks;
        }

        Ok(())
    }

    /// Define using string name (legacy API)
    pub fn define_str(
        &mut self,
        name: &str,
        constructor_id: u32,
        options: CustomElementOptionsLegacy,
    ) -> Result<(), CustomElementError> {
        // Validate name
        if !Self::is_valid_name(name) {
            return Err(CustomElementError::InvalidName);
        }

        // Check if already defined
        if self.name_index.contains_key(name) {
            return Err(CustomElementError::AlreadyDefined);
        }

        // Use a placeholder interned string (real impl would use actual interner)
        let name_interned = InternedString(self.definitions.len() as u32 + 1000);

        let definition = CustomElementDefinition {
            name: name_interned,
            name_str: name.to_string(),
            constructor_id,
            extends: None, // Would need to intern
            observed_attributes: Vec::new(), // Would need to intern
            form_associated: options.form_associated,
            disable_shadow: options.disable_shadow,
            disable_internals: options.disable_internals,
        };

        self.definitions.insert(name_interned, definition);
        self.name_index.insert(name.to_string(), name_interned);

        // Trigger whenDefined callbacks
        if let Some(callbacks) = self.when_defined.remove(name) {
            let _ = callbacks;
        }

        Ok(())
    }

    /// Get element definition by interned name
    pub fn get(&self, name: InternedString) -> Option<&CustomElementDefinition> {
        self.definitions.get(&name)
    }

    /// Get element definition by string name
    pub fn get_by_name(&self, name: &str) -> Option<&CustomElementDefinition> {
        self.name_index.get(name)
            .and_then(|interned| self.definitions.get(interned))
    }

    /// Check if element is defined
    pub fn is_defined(&self, name: &str) -> bool {
        self.name_index.contains_key(name)
    }

    /// Register whenDefined callback
    pub fn when_defined(&mut self, name: &str, callback_id: u32) {
        if self.is_defined(name) {
            // Already defined, would invoke immediately
            return;
        }

        self.when_defined
            .entry(name.to_string())
            .or_default()
            .push(callback_id);
    }

    /// Add an upgrade candidate
    pub fn add_upgrade_candidate(&mut self, node_id: NodeId, name: InternedString) {
        self.upgrade_candidates.push(UpgradeCandidate { node_id, name });
    }

    /// Get and clear upgrade candidates for a definition
    pub fn take_upgrade_candidates(&mut self, name: InternedString) -> Vec<UpgradeCandidate> {
        let (matching, remaining): (Vec<_>, Vec<_>) = self.upgrade_candidates
            .drain(..)
            .partition(|c| c.name == name);
        self.upgrade_candidates = remaining;
        matching
    }

    /// Upgrade an element
    pub fn upgrade(&self, _element_id: NodeId) {
        // Would trigger constructor and lifecycle callbacks
    }

    /// Validate custom element name
    pub fn is_valid_name(name: &str) -> bool {
        // Must contain hyphen
        if !name.contains('-') {
            return false;
        }

        // Must start with lowercase letter
        if !name.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false) {
            return false;
        }

        // Reserved names
        const RESERVED: &[&str] = &[
            "annotation-xml", "color-profile", "font-face",
            "font-face-src", "font-face-uri", "font-face-format",
            "font-face-name", "missing-glyph",
        ];
        if RESERVED.contains(&name) {
            return false;
        }

        // Must be valid XML name
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    }
}

/// Custom element options with interned strings
#[derive(Debug, Clone, Default)]
pub struct CustomElementOptions {
    pub extends: Option<InternedString>,
    pub observed_attributes: Vec<InternedString>,
    pub form_associated: bool,
    pub disable_shadow: bool,
    pub disable_internals: bool,
}

/// Legacy options with string types
#[derive(Debug, Clone, Default)]
pub struct CustomElementOptionsLegacy {
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

impl std::fmt::Display for CustomElementError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName => write!(f, "Invalid custom element name"),
            Self::AlreadyDefined => write!(f, "Custom element already defined"),
            Self::ExtensionNotAllowed => write!(f, "Extension not allowed for this element"),
        }
    }
}

impl std::error::Error for CustomElementError {}

/// ElementInternals - provides form participation and accessibility for custom elements
#[derive(Debug)]
pub struct ElementInternals {
    /// Associated element
    pub element: NodeId,
    /// Form value
    value: Option<FormValue>,
    /// Custom validity state
    validity: ValidityState,
    /// Validation message
    validation_message: String,
    /// ARIA attributes
    aria: AriaMap,
    /// Associated form (if form-associated)
    pub form: Option<NodeId>,
    /// Labels associated with this element
    pub labels: Vec<NodeId>,
}

/// Form value for form-associated custom elements
#[derive(Debug, Clone)]
pub enum FormValue {
    String(String),
    File(FileValue),
    FormData(Vec<(String, String)>),
}

/// Placeholder for file value
#[derive(Debug, Clone)]
pub struct FileValue {
    pub name: String,
    pub size: u64,
}

/// Validity state for form validation
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
}

impl ValidityState {
    pub fn valid(&self) -> bool {
        !self.value_missing
            && !self.type_mismatch
            && !self.pattern_mismatch
            && !self.too_long
            && !self.too_short
            && !self.range_underflow
            && !self.range_overflow
            && !self.step_mismatch
            && !self.bad_input
            && !self.custom_error
    }
}

/// ARIA attribute map
#[derive(Debug, Default)]
pub struct AriaMap {
    attrs: HashMap<String, String>,
}

impl AriaMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, name: &str, value: &str) {
        self.attrs.insert(name.to_string(), value.to_string());
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.attrs.get(name).map(String::as_str)
    }

    pub fn remove(&mut self, name: &str) -> Option<String> {
        self.attrs.remove(name)
    }
}

impl ElementInternals {
    pub fn new(element: NodeId) -> Self {
        Self {
            element,
            value: None,
            validity: ValidityState::default(),
            validation_message: String::new(),
            aria: AriaMap::new(),
            form: None,
            labels: Vec::new(),
        }
    }

    /// Set form value
    pub fn set_form_value(&mut self, value: FormValue) {
        self.value = Some(value);
    }

    /// Get form value
    pub fn form_value(&self) -> Option<&FormValue> {
        self.value.as_ref()
    }

    /// Set custom validity
    pub fn set_validity(&mut self, flags: ValidityState, message: &str) {
        self.validity = flags;
        self.validation_message = message.to_string();
    }

    /// Check validity
    pub fn check_validity(&self) -> bool {
        self.validity.valid()
    }

    /// Report validity
    pub fn report_validity(&self) -> bool {
        self.validity.valid()
    }

    /// Get validation message
    pub fn validation_message(&self) -> &str {
        &self.validation_message
    }

    /// Set ARIA attribute
    pub fn set_aria(&mut self, name: &str, value: &str) {
        self.aria.set(name, value);
    }

    /// Get ARIA attribute
    pub fn get_aria(&self, name: &str) -> Option<&str> {
        self.aria.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_names() {
        assert!(CustomElementRegistry::is_valid_name("my-element"));
        assert!(CustomElementRegistry::is_valid_name("app-header"));
        assert!(CustomElementRegistry::is_valid_name("x-foo-bar"));
        assert!(!CustomElementRegistry::is_valid_name("myelement")); // no hyphen
        assert!(!CustomElementRegistry::is_valid_name("My-Element")); // uppercase
        assert!(!CustomElementRegistry::is_valid_name("1-element")); // starts with number
        assert!(!CustomElementRegistry::is_valid_name("font-face")); // reserved
    }

    #[test]
    fn test_define() {
        let mut registry = CustomElementRegistry::new();

        assert!(registry.define_str("my-element", 1, CustomElementOptionsLegacy::default()).is_ok());
        assert!(registry.is_defined("my-element"));

        // Duplicate
        assert_eq!(
            registry.define_str("my-element", 2, CustomElementOptionsLegacy::default()),
            Err(CustomElementError::AlreadyDefined)
        );
    }

    #[test]
    fn test_callback_queue() {
        let mut queue = CallbackQueue::new();
        assert!(queue.is_empty());

        queue.enqueue(LifecycleCallbackInfo {
            callback: LifecycleCallback::Connected,
            element_id: NodeId(1),
            attribute_name: None,
            old_value: None,
            new_value: None,
        });

        assert_eq!(queue.len(), 1);

        let callbacks = queue.drain();
        assert_eq!(callbacks.len(), 1);
        assert!(queue.is_empty());
    }

    #[test]
    fn test_element_internals() {
        let mut internals = ElementInternals::new(NodeId(1));

        assert!(internals.check_validity());

        internals.set_validity(
            ValidityState { value_missing: true, ..Default::default() },
            "Value is required"
        );

        assert!(!internals.check_validity());
        assert_eq!(internals.validation_message(), "Value is required");
    }

    #[test]
    fn test_validity_state() {
        let valid = ValidityState::default();
        assert!(valid.valid());

        let invalid = ValidityState {
            type_mismatch: true,
            ..Default::default()
        };
        assert!(!invalid.valid());
    }
}
