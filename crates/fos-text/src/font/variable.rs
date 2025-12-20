//! Variable Fonts Support
//!
//! Implementation of OpenType variable fonts (OpenType 1.8+).
//! Variable fonts allow continuous variation along design axes.

use std::collections::HashMap;

/// Variable font axis definition
#[derive(Debug, Clone)]
pub struct FontAxis {
    /// 4-character axis tag (e.g., "wght", "wdth", "slnt")
    pub tag: [u8; 4],
    /// Human-readable name
    pub name: String,
    /// Minimum value
    pub min_value: f32,
    /// Default value
    pub default_value: f32,
    /// Maximum value
    pub max_value: f32,
    /// Whether this is a registered axis
    pub is_registered: bool,
}

impl FontAxis {
    /// Create a new font axis
    pub fn new(tag: [u8; 4], name: &str, min: f32, default: f32, max: f32) -> Self {
        Self {
            tag,
            name: name.to_string(),
            min_value: min,
            default_value: default,
            max_value: max,
            is_registered: is_registered_axis(&tag),
        }
    }
    
    /// Get axis tag as string
    pub fn tag_string(&self) -> String {
        String::from_utf8_lossy(&self.tag).to_string()
    }
    
    /// Clamp value to axis range
    pub fn clamp(&self, value: f32) -> f32 {
        value.clamp(self.min_value, self.max_value)
    }
    
    /// Normalize value to 0..1 range
    pub fn normalize(&self, value: f32) -> f32 {
        let clamped = self.clamp(value);
        (clamped - self.min_value) / (self.max_value - self.min_value)
    }
}

/// Check if axis tag is a registered OpenType axis
fn is_registered_axis(tag: &[u8; 4]) -> bool {
    matches!(tag,
        b"wght" | // Weight
        b"wdth" | // Width
        b"slnt" | // Slant
        b"ital" | // Italic
        b"opsz"   // Optical Size
    )
}

/// Common registered axis tags
pub mod axis_tags {
    pub const WEIGHT: [u8; 4] = *b"wght";
    pub const WIDTH: [u8; 4] = *b"wdth";
    pub const SLANT: [u8; 4] = *b"slnt";
    pub const ITALIC: [u8; 4] = *b"ital";
    pub const OPTICAL_SIZE: [u8; 4] = *b"opsz";
}

/// Variable font instance
#[derive(Debug, Clone)]
pub struct VariableFontInstance {
    /// Axis values for this instance
    pub coordinates: HashMap<[u8; 4], f32>,
}

impl Default for VariableFontInstance {
    fn default() -> Self {
        Self::new()
    }
}

impl VariableFontInstance {
    /// Create a new instance with default values
    pub fn new() -> Self {
        Self {
            coordinates: HashMap::new(),
        }
    }
    
    /// Set axis value
    pub fn set_axis(&mut self, tag: [u8; 4], value: f32) {
        self.coordinates.insert(tag, value);
    }
    
    /// Get axis value
    pub fn get_axis(&self, tag: &[u8; 4]) -> Option<f32> {
        self.coordinates.get(tag).copied()
    }
    
    /// Set weight (wght axis)
    pub fn set_weight(&mut self, weight: f32) {
        self.set_axis(axis_tags::WEIGHT, weight);
    }
    
    /// Set width (wdth axis)
    pub fn set_width(&mut self, width: f32) {
        self.set_axis(axis_tags::WIDTH, width);
    }
    
    /// Set slant (slnt axis)
    pub fn set_slant(&mut self, slant: f32) {
        self.set_axis(axis_tags::SLANT, slant);
    }
    
    /// Set italic (ital axis)
    pub fn set_italic(&mut self, italic: f32) {
        self.set_axis(axis_tags::ITALIC, italic);
    }
    
    /// Convert CSS font-weight to axis value
    pub fn from_css_weight(weight: u16) -> f32 {
        weight as f32
    }
    
    /// Convert CSS font-stretch to axis value  
    pub fn from_css_stretch(stretch: &str) -> f32 {
        match stretch {
            "ultra-condensed" => 50.0,
            "extra-condensed" => 62.5,
            "condensed" => 75.0,
            "semi-condensed" => 87.5,
            "normal" => 100.0,
            "semi-expanded" => 112.5,
            "expanded" => 125.0,
            "extra-expanded" => 150.0,
            "ultra-expanded" => 200.0,
            _ => 100.0,
        }
    }
}

/// Variable font definition with all axes
#[derive(Debug, Clone)]
pub struct VariableFont {
    /// Font name
    pub name: String,
    /// Available axes
    pub axes: Vec<FontAxis>,
    /// Named instances (presets)
    pub named_instances: Vec<NamedInstance>,
}

impl VariableFont {
    /// Create a new variable font definition
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            axes: Vec::new(),
            named_instances: Vec::new(),
        }
    }
    
    /// Add an axis
    pub fn add_axis(&mut self, axis: FontAxis) {
        self.axes.push(axis);
    }
    
    /// Add a named instance
    pub fn add_named_instance(&mut self, instance: NamedInstance) {
        self.named_instances.push(instance);
    }
    
    /// Get axis by tag
    pub fn get_axis(&self, tag: &[u8; 4]) -> Option<&FontAxis> {
        self.axes.iter().find(|a| &a.tag == tag)
    }
    
    /// Check if font has specific axis
    pub fn has_axis(&self, tag: &[u8; 4]) -> bool {
        self.axes.iter().any(|a| &a.tag == tag)
    }
    
    /// Get default instance
    pub fn default_instance(&self) -> VariableFontInstance {
        let mut instance = VariableFontInstance::new();
        for axis in &self.axes {
            instance.set_axis(axis.tag, axis.default_value);
        }
        instance
    }
    
    /// Interpolate between two instances
    pub fn interpolate(&self, a: &VariableFontInstance, b: &VariableFontInstance, t: f32) -> VariableFontInstance {
        let mut result = VariableFontInstance::new();
        
        for axis in &self.axes {
            let va = a.get_axis(&axis.tag).unwrap_or(axis.default_value);
            let vb = b.get_axis(&axis.tag).unwrap_or(axis.default_value);
            let interpolated = va + (vb - va) * t;
            result.set_axis(axis.tag, axis.clamp(interpolated));
        }
        
        result
    }
}

/// Named instance (preset) for variable fonts
#[derive(Debug, Clone)]
pub struct NamedInstance {
    /// Instance name (e.g., "Bold", "Light Italic")
    pub name: String,
    /// Axis coordinates
    pub coordinates: VariableFontInstance,
}

impl NamedInstance {
    pub fn new(name: &str, coordinates: VariableFontInstance) -> Self {
        Self {
            name: name.to_string(),
            coordinates,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_font_axis() {
        let axis = FontAxis::new(*b"wght", "Weight", 100.0, 400.0, 900.0);
        assert_eq!(axis.tag_string(), "wght");
        assert!(axis.is_registered);
        assert_eq!(axis.clamp(50.0), 100.0);
        assert_eq!(axis.clamp(1000.0), 900.0);
    }
    
    #[test]
    fn test_variable_font_instance() {
        let mut instance = VariableFontInstance::new();
        instance.set_weight(700.0);
        assert_eq!(instance.get_axis(&axis_tags::WEIGHT), Some(700.0));
    }
    
    #[test]
    fn test_variable_font() {
        let mut font = VariableFont::new("Test Font");
        font.add_axis(FontAxis::new(*b"wght", "Weight", 100.0, 400.0, 900.0));
        font.add_axis(FontAxis::new(*b"wdth", "Width", 75.0, 100.0, 125.0));
        
        assert!(font.has_axis(&axis_tags::WEIGHT));
        assert!(!font.has_axis(&axis_tags::SLANT));
        
        let default = font.default_instance();
        assert_eq!(default.get_axis(&axis_tags::WEIGHT), Some(400.0));
    }
}
