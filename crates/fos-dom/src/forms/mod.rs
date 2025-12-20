//! Form Elements Module
//!
//! Implements HTML form elements: input, textarea, select, button, form.

mod input;
mod textarea;
mod select;
mod form;
mod validation;
mod label;
pub mod selection;
pub mod custom_validity;

pub use input::{InputElement, InputType, InputValue};
pub use textarea::TextareaElement;
pub use select::{SelectElement, OptionElement, OptionGroup};
pub use form::{FormElement, FormData, FormMethod, FormEnctype};
pub use validation::{ValidityState, ConstraintValidation};
pub use label::{LabelElement, FieldsetElement, LegendElement};
pub use selection::{Selection, Range, InputSelection, SelectionDirection, SelectionType};
pub use custom_validity::{FormControl as ValidatedFormControl, ValidationConstraints, ValidationPseudoClass};

/// Trait for form control elements
pub trait FormControl {
    /// Get the element's name
    fn name(&self) -> Option<&str>;
    
    /// Get the element's value
    fn value(&self) -> String;
    
    /// Set the element's value
    fn set_value(&mut self, value: &str);
    
    /// Check if the element is disabled
    fn is_disabled(&self) -> bool;
    
    /// Get the form owner
    fn form_id(&self) -> Option<&str>;
    
    /// Get validity state
    fn validity(&self) -> &ValidityState;
    
    /// Check validity
    fn check_validity(&self) -> bool {
        self.validity().is_valid()
    }
    
    /// Report validity (with UI feedback)
    fn report_validity(&self) -> bool {
        self.check_validity()
    }
}
