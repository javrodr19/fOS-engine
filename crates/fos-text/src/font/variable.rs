//! Variable Fonts Support
//!
//! Implementation of OpenType variable fonts (OpenType 1.8+).
//! Variable fonts allow continuous variation along design axes.
//! Uses Fixed16 for deterministic, cross-platform axis calculations.

use std::collections::HashMap;
use super::fixed_point::Fixed16;

/// Variable font axis definition
#[derive(Debug, Clone)]
pub struct FontAxis {
    /// 4-character axis tag (e.g., "wght", "wdth", "slnt")
    pub tag: [u8; 4],
    /// Human-readable name
    pub name: String,
    /// Minimum value (Fixed16 for determinism)
    pub min_value: Fixed16,
    /// Default value (Fixed16 for determinism)
    pub default_value: Fixed16,
    /// Maximum value (Fixed16 for determinism)
    pub max_value: Fixed16,
    /// Whether this is a registered axis
    pub is_registered: bool,
}

impl FontAxis {
    /// Create a new font axis
    pub fn new(tag: [u8; 4], name: &str, min: f32, default: f32, max: f32) -> Self {
        Self {
            tag,
            name: name.to_string(),
            min_value: Fixed16::from_f32(min),
            default_value: Fixed16::from_f32(default),
            max_value: Fixed16::from_f32(max),
            is_registered: is_registered_axis(&tag),
        }
    }
    
    /// Create with Fixed16 values directly
    pub fn new_fixed(tag: [u8; 4], name: &str, min: Fixed16, default: Fixed16, max: Fixed16) -> Self {
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
    
    /// Clamp value to axis range (Fixed16)
    pub fn clamp(&self, value: Fixed16) -> Fixed16 {
        value.clamp(self.min_value, self.max_value)
    }
    
    /// Clamp f32 value to axis range
    pub fn clamp_f32(&self, value: f32) -> f32 {
        self.clamp(Fixed16::from_f32(value)).to_f32()
    }
    
    /// Normalize value to 0..1 range
    pub fn normalize(&self, value: Fixed16) -> Fixed16 {
        let clamped = self.clamp(value);
        let range = self.max_value - self.min_value;
        if range.to_bits() == 0 {
            return Fixed16::ZERO;
        }
        (clamped - self.min_value) / range
    }
    
    /// Get min value as f32 (for compatibility)
    pub fn min_f32(&self) -> f32 {
        self.min_value.to_f32()
    }
    
    /// Get max value as f32 (for compatibility)
    pub fn max_f32(&self) -> f32 {
        self.max_value.to_f32()
    }
    
    /// Get default value as f32 (for compatibility)
    pub fn default_f32(&self) -> f32 {
        self.default_value.to_f32()
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

/// Variable font instance with Fixed16 coordinates
#[derive(Debug, Clone)]
pub struct VariableFontInstance {
    /// Axis values for this instance (Fixed16 for determinism)
    pub coordinates: HashMap<[u8; 4], Fixed16>,
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
    
    /// Set axis value (Fixed16)
    pub fn set_axis(&mut self, tag: [u8; 4], value: Fixed16) {
        self.coordinates.insert(tag, value);
    }
    
    /// Set axis value (f32 convenience)
    pub fn set_axis_f32(&mut self, tag: [u8; 4], value: f32) {
        self.set_axis(tag, Fixed16::from_f32(value));
    }
    
    /// Get axis value (Fixed16)
    pub fn get_axis(&self, tag: &[u8; 4]) -> Option<Fixed16> {
        self.coordinates.get(tag).copied()
    }
    
    /// Get axis value as f32
    pub fn get_axis_f32(&self, tag: &[u8; 4]) -> Option<f32> {
        self.get_axis(tag).map(|v| v.to_f32())
    }
    
    /// Set weight (wght axis)
    pub fn set_weight(&mut self, weight: f32) {
        self.set_axis_f32(axis_tags::WEIGHT, weight);
    }
    
    /// Set width (wdth axis)
    pub fn set_width(&mut self, width: f32) {
        self.set_axis_f32(axis_tags::WIDTH, width);
    }
    
    /// Set slant (slnt axis)
    pub fn set_slant(&mut self, slant: f32) {
        self.set_axis_f32(axis_tags::SLANT, slant);
    }
    
    /// Set italic (ital axis)
    pub fn set_italic(&mut self, italic: f32) {
        self.set_axis_f32(axis_tags::ITALIC, italic);
    }
    
    /// Convert CSS font-weight to axis value
    pub fn from_css_weight(weight: u16) -> Fixed16 {
        Fixed16::from_i32(weight as i32)
    }
    
    /// Convert CSS font-stretch to axis value  
    pub fn from_css_stretch(stretch: &str) -> Fixed16 {
        let value = match stretch {
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
        };
        Fixed16::from_f32(value)
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
    
    /// Interpolate between two instances (deterministic with Fixed16)
    pub fn interpolate(&self, a: &VariableFontInstance, b: &VariableFontInstance, t: Fixed16) -> VariableFontInstance {
        let mut result = VariableFontInstance::new();
        
        for axis in &self.axes {
            let va = a.get_axis(&axis.tag).unwrap_or(axis.default_value);
            let vb = b.get_axis(&axis.tag).unwrap_or(axis.default_value);
            let interpolated = va.lerp(vb, t);
            result.set_axis(axis.tag, axis.clamp(interpolated));
        }
        
        result
    }
    
    /// Interpolate with f32 t value (convenience)
    pub fn interpolate_f32(&self, a: &VariableFontInstance, b: &VariableFontInstance, t: f32) -> VariableFontInstance {
        self.interpolate(a, b, Fixed16::from_f32(t))
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
    fn test_font_axis_fixed16() {
        let axis = FontAxis::new(*b"wght", "Weight", 100.0, 400.0, 900.0);
        assert_eq!(axis.tag_string(), "wght");
        assert!(axis.is_registered);
        
        // Test clamping with Fixed16
        let clamped = axis.clamp(Fixed16::from_f32(50.0));
        assert!((clamped.to_f32() - 100.0).abs() < 0.01);
        
        let clamped = axis.clamp(Fixed16::from_f32(1000.0));
        assert!((clamped.to_f32() - 900.0).abs() < 0.01);
    }
    
    #[test]
    fn test_variable_font_instance_fixed16() {
        let mut instance = VariableFontInstance::new();
        instance.set_weight(700.0);
        
        let weight = instance.get_axis_f32(&axis_tags::WEIGHT);
        assert!((weight.unwrap() - 700.0).abs() < 0.01);
    }
    
    #[test]
    fn test_variable_font_interpolation() {
        let mut font = VariableFont::new("Test Font");
        font.add_axis(FontAxis::new(*b"wght", "Weight", 100.0, 400.0, 900.0));
        
        let mut a = VariableFontInstance::new();
        a.set_weight(100.0);
        
        let mut b = VariableFontInstance::new();
        b.set_weight(900.0);
        
        // Interpolate at 50%
        let result = font.interpolate_f32(&a, &b, 0.5);
        let weight = result.get_axis_f32(&axis_tags::WEIGHT).unwrap();
        assert!((weight - 500.0).abs() < 1.0);
    }
    
    #[test]
    fn test_variable_font_default() {
        let mut font = VariableFont::new("Test Font");
        font.add_axis(FontAxis::new(*b"wght", "Weight", 100.0, 400.0, 900.0));
        font.add_axis(FontAxis::new(*b"wdth", "Width", 75.0, 100.0, 125.0));
        
        assert!(font.has_axis(&axis_tags::WEIGHT));
        assert!(!font.has_axis(&axis_tags::SLANT));
        
        let default = font.default_instance();
        let weight = default.get_axis_f32(&axis_tags::WEIGHT).unwrap();
        assert!((weight - 400.0).abs() < 0.01);
    }
}
