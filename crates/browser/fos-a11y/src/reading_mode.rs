//! Reading Mode
//!
//! Simplified reading view for better accessibility.
//! Custom implementation with no external dependencies.

/// Reading mode configuration
#[derive(Debug, Clone)]
pub struct ReadingModeSettings {
    /// Font size multiplier
    pub font_size_factor: f64,
    /// Line height multiplier
    pub line_height: f64,
    /// Maximum content width in characters
    pub max_width_ch: u32,
    /// Remove images
    pub hide_images: bool,
    /// Remove sidebars and navigation
    pub hide_chrome: bool,
    /// Use high contrast colors
    pub high_contrast: bool,
    /// Font family to use
    pub font_family: FontFamily,
    /// Text alignment
    pub text_align: TextAlign,
    /// Background color (as hex)
    pub background_color: String,
    /// Text color (as hex)
    pub text_color: String,
    /// Link color (as hex)
    pub link_color: String,
}

/// Font family options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FontFamily {
    #[default]
    SystemDefault,
    Serif,
    SansSerif,
    Monospace,
    Dyslexic, // OpenDyslexic-style
}

impl FontFamily {
    pub fn css_value(&self) -> &'static str {
        match self {
            Self::SystemDefault => "system-ui, sans-serif",
            Self::Serif => "Georgia, 'Times New Roman', serif",
            Self::SansSerif => "Arial, Helvetica, sans-serif",
            Self::Monospace => "'Courier New', Consolas, monospace",
            Self::Dyslexic => "'OpenDyslexic', 'Comic Sans MS', cursive",
        }
    }
}

/// Text alignment options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Justify,
    Center,
}

impl TextAlign {
    pub fn css_value(&self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Justify => "justify",
            Self::Center => "center",
        }
    }
}

impl Default for ReadingModeSettings {
    fn default() -> Self {
        Self {
            font_size_factor: 1.2,
            line_height: 1.6,
            max_width_ch: 70,
            hide_images: false,
            hide_chrome: true,
            high_contrast: false,
            font_family: FontFamily::SansSerif,
            text_align: TextAlign::Left,
            background_color: "#fefefe".to_string(),
            text_color: "#1a1a1a".to_string(),
            link_color: "#0066cc".to_string(),
        }
    }
}

impl ReadingModeSettings {
    /// Dark theme preset
    pub fn dark() -> Self {
        Self {
            background_color: "#1a1a1a".to_string(),
            text_color: "#e0e0e0".to_string(),
            link_color: "#6699ff".to_string(),
            ..Default::default()
        }
    }
    
    /// Sepia theme preset
    pub fn sepia() -> Self {
        Self {
            background_color: "#f5e6c8".to_string(),
            text_color: "#3d2b1f".to_string(),
            link_color: "#8b4513".to_string(),
            ..Default::default()
        }
    }
    
    /// High contrast preset
    pub fn high_contrast_mode() -> Self {
        Self {
            background_color: "#000000".to_string(),
            text_color: "#ffffff".to_string(),
            link_color: "#ffff00".to_string(),
            high_contrast: true,
            ..Default::default()
        }
    }
    
    /// Dyslexia-friendly preset
    pub fn dyslexia_friendly() -> Self {
        Self {
            font_family: FontFamily::Dyslexic,
            line_height: 2.0,
            font_size_factor: 1.3,
            max_width_ch: 60,
            background_color: "#faf8f0".to_string(),
            text_color: "#333333".to_string(),
            ..Default::default()
        }
    }
    
    /// Generate CSS for reading mode
    pub fn to_css(&self) -> String {
        format!(
r#"
.reading-mode {{
    font-family: {font_family};
    font-size: {font_size}em;
    line-height: {line_height};
    max-width: {max_width}ch;
    margin: 0 auto;
    padding: 2em;
    background-color: {bg};
    color: {fg};
    text-align: {text_align};
}}

.reading-mode a {{
    color: {link};
}}

.reading-mode img {{
    max-width: 100%;
    height: auto;
    {hide_images}
}}

.reading-mode nav,
.reading-mode aside,
.reading-mode header,
.reading-mode footer,
.reading-mode .sidebar,
.reading-mode .advertisement {{
    {hide_chrome}
}}
"#,
            font_family = self.font_family.css_value(),
            font_size = self.font_size_factor,
            line_height = self.line_height,
            max_width = self.max_width_ch,
            bg = self.background_color,
            fg = self.text_color,
            link = self.link_color,
            text_align = self.text_align.css_value(),
            hide_images = if self.hide_images { "display: none !important;" } else { "" },
            hide_chrome = if self.hide_chrome { "display: none !important;" } else { "" },
        )
    }
}

/// Reading mode controller
#[derive(Debug, Default)]
pub struct ReadingMode {
    enabled: bool,
    settings: ReadingModeSettings,
}

impl ReadingMode {
    pub fn new() -> Self { Self::default() }
    
    /// Enable reading mode
    pub fn enable(&mut self) {
        self.enabled = true;
    }
    
    /// Disable reading mode
    pub fn disable(&mut self) {
        self.enabled = false;
    }
    
    /// Toggle reading mode
    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }
    
    /// Check if enabled
    pub fn is_enabled(&self) -> bool { self.enabled }
    
    /// Get settings
    pub fn settings(&self) -> &ReadingModeSettings { &self.settings }
    
    /// Get mutable settings
    pub fn settings_mut(&mut self) -> &mut ReadingModeSettings { &mut self.settings }
    
    /// Apply a preset
    pub fn apply_preset(&mut self, preset: ReadingPreset) {
        self.settings = match preset {
            ReadingPreset::Default => ReadingModeSettings::default(),
            ReadingPreset::Dark => ReadingModeSettings::dark(),
            ReadingPreset::Sepia => ReadingModeSettings::sepia(),
            ReadingPreset::HighContrast => ReadingModeSettings::high_contrast_mode(),
            ReadingPreset::DyslexiaFriendly => ReadingModeSettings::dyslexia_friendly(),
        };
    }
    
    /// Increase font size
    pub fn increase_font_size(&mut self) {
        self.settings.font_size_factor = (self.settings.font_size_factor + 0.1).min(3.0);
    }
    
    /// Decrease font size
    pub fn decrease_font_size(&mut self) {
        self.settings.font_size_factor = (self.settings.font_size_factor - 0.1).max(0.5);
    }
    
    /// Get CSS if enabled
    pub fn get_css(&self) -> Option<String> {
        if self.enabled {
            Some(self.settings.to_css())
        } else {
            None
        }
    }
}

/// Reading mode presets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingPreset {
    Default,
    Dark,
    Sepia,
    HighContrast,
    DyslexiaFriendly,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_reading_mode() {
        let mut rm = ReadingMode::new();
        assert!(!rm.is_enabled());
        
        rm.enable();
        assert!(rm.is_enabled());
        
        rm.toggle();
        assert!(!rm.is_enabled());
    }
    
    #[test]
    fn test_css_generation() {
        let settings = ReadingModeSettings::default();
        let css = settings.to_css();
        
        assert!(css.contains("font-size:"));
        assert!(css.contains("line-height:"));
        assert!(css.contains("max-width:"));
    }
    
    #[test]
    fn test_presets() {
        let mut rm = ReadingMode::new();
        
        rm.apply_preset(ReadingPreset::Dark);
        assert_eq!(rm.settings().background_color, "#1a1a1a");
        
        rm.apply_preset(ReadingPreset::Sepia);
        assert_eq!(rm.settings().background_color, "#f5e6c8");
    }
}
