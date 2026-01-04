//! Text Scaling & Zoom
//!
//! Font size scaling, page zoom, and minimum font size enforcement.

/// Zoom level
#[derive(Debug, Clone, Copy)]
pub struct ZoomLevel {
    pub factor: f64, // 1.0 = 100%
}

impl Default for ZoomLevel {
    fn default() -> Self { Self { factor: 1.0 } }
}

impl ZoomLevel {
    pub fn new(factor: f64) -> Self { Self { factor: factor.clamp(0.25, 5.0) } }
    
    pub fn percentage(&self) -> u32 { (self.factor * 100.0).round() as u32 }
    
    pub fn zoom_in(&mut self) { self.factor = (self.factor + 0.1).min(5.0); }
    pub fn zoom_out(&mut self) { self.factor = (self.factor - 0.1).max(0.25); }
    pub fn reset(&mut self) { self.factor = 1.0; }
    
    pub fn apply(&self, value: f64) -> f64 { value * self.factor }
}

/// Text scaling settings
#[derive(Debug, Clone)]
pub struct TextScalingSettings {
    pub text_zoom: ZoomLevel,
    pub page_zoom: ZoomLevel,
    pub minimum_font_size: Option<f64>,
    pub default_font_size: f64,
    pub default_monospace_size: f64,
}

impl Default for TextScalingSettings {
    fn default() -> Self {
        Self { text_zoom: ZoomLevel::default(), page_zoom: ZoomLevel::default(),
               minimum_font_size: None, default_font_size: 16.0, default_monospace_size: 13.0 }
    }
}

impl TextScalingSettings {
    /// Apply text scaling to a font size
    pub fn apply_text_scale(&self, size: f64) -> f64 {
        let scaled = self.text_zoom.apply(size);
        match self.minimum_font_size {
            Some(min) if scaled < min => min,
            _ => scaled,
        }
    }
    
    /// Apply page zoom to a dimension
    pub fn apply_page_zoom(&self, value: f64) -> f64 { self.page_zoom.apply(value) }
    
    /// Get combined zoom factor
    pub fn combined_factor(&self) -> f64 { self.text_zoom.factor * self.page_zoom.factor }
}

/// Font size preset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontSizePreset {
    VerySmall,
    Small,
    #[default]
    Medium,
    Large,
    VeryLarge,
    Custom,
}

impl FontSizePreset {
    pub fn factor(&self) -> f64 {
        match self {
            Self::VerySmall => 0.75, Self::Small => 0.875, Self::Medium => 1.0,
            Self::Large => 1.25, Self::VeryLarge => 1.5, Self::Custom => 1.0,
        }
    }
}

/// Scaling manager
#[derive(Debug, Default)]
pub struct ScalingManager {
    settings: TextScalingSettings,
    preset: FontSizePreset,
}

impl ScalingManager {
    pub fn new() -> Self { Self { preset: FontSizePreset::Medium, ..Default::default() } }
    
    pub fn settings(&self) -> &TextScalingSettings { &self.settings }
    
    pub fn set_text_zoom(&mut self, factor: f64) { self.settings.text_zoom = ZoomLevel::new(factor); }
    pub fn set_page_zoom(&mut self, factor: f64) { self.settings.page_zoom = ZoomLevel::new(factor); }
    pub fn set_minimum_font_size(&mut self, size: Option<f64>) { self.settings.minimum_font_size = size; }
    
    pub fn apply_preset(&mut self, preset: FontSizePreset) {
        self.preset = preset;
        self.settings.text_zoom = ZoomLevel::new(preset.factor());
    }
    
    pub fn zoom_in_text(&mut self) { self.settings.text_zoom.zoom_in(); self.preset = FontSizePreset::Custom; }
    pub fn zoom_out_text(&mut self) { self.settings.text_zoom.zoom_out(); self.preset = FontSizePreset::Custom; }
    pub fn zoom_in_page(&mut self) { self.settings.page_zoom.zoom_in(); }
    pub fn zoom_out_page(&mut self) { self.settings.page_zoom.zoom_out(); }
    
    pub fn reset_text(&mut self) { self.settings.text_zoom.reset(); self.preset = FontSizePreset::Medium; }
    pub fn reset_page(&mut self) { self.settings.page_zoom.reset(); }
    pub fn reset_all(&mut self) { self.reset_text(); self.reset_page(); }
    
    /// Calculate final font size
    pub fn calculate_font_size(&self, base_size: f64) -> f64 {
        let scaled = self.settings.apply_text_scale(base_size);
        self.settings.apply_page_zoom(scaled)
    }
    
    /// Get scaled viewport dimensions
    pub fn scale_viewport(&self, width: f64, height: f64) -> (f64, f64) {
        let factor = self.settings.page_zoom.factor;
        (width / factor, height / factor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_zoom_level() {
        let mut zoom = ZoomLevel::default();
        assert_eq!(zoom.percentage(), 100);
        zoom.zoom_in();
        assert_eq!(zoom.percentage(), 110);
        zoom.zoom_out();
        assert_eq!(zoom.percentage(), 100);
    }
    
    #[test]
    fn test_text_scaling() {
        let mut settings = TextScalingSettings::default();
        settings.text_zoom = ZoomLevel::new(1.5);
        settings.minimum_font_size = Some(12.0);
        
        assert_eq!(settings.apply_text_scale(16.0), 24.0);
        assert_eq!(settings.apply_text_scale(6.0), 12.0); // Enforced minimum
    }
    
    #[test]
    fn test_scaling_manager() {
        let mut manager = ScalingManager::new();
        manager.apply_preset(FontSizePreset::Large);
        assert_eq!(manager.calculate_font_size(16.0), 20.0);
    }
}
