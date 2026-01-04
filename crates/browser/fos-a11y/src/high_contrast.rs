//! High Contrast Mode
//!
//! Forced colors detection and system color scheme support.

/// Contrast preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContrastPreference {
    #[default]
    NoPreference,
    More,
    Less,
    Custom,
}

/// Color scheme preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorScheme {
    #[default]
    Light,
    Dark,
}

/// Forced colors mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ForcedColorsMode {
    #[default]
    None,
    Active,
}

/// System colors (CSS system color keywords)
#[derive(Debug, Clone)]
pub struct SystemColors {
    pub canvas: String,
    pub canvas_text: String,
    pub link_text: String,
    pub visited_text: String,
    pub active_text: String,
    pub button_face: String,
    pub button_text: String,
    pub button_border: String,
    pub field: String,
    pub field_text: String,
    pub highlight: String,
    pub highlight_text: String,
    pub selected_item: String,
    pub selected_item_text: String,
    pub mark: String,
    pub mark_text: String,
    pub gray_text: String,
}

impl Default for SystemColors {
    fn default() -> Self {
        Self {
            canvas: "#ffffff".into(), canvas_text: "#000000".into(),
            link_text: "#0000ee".into(), visited_text: "#551a8b".into(), active_text: "#ff0000".into(),
            button_face: "#dddfe2".into(), button_text: "#000000".into(), button_border: "#767676".into(),
            field: "#ffffff".into(), field_text: "#000000".into(),
            highlight: "#0078d7".into(), highlight_text: "#ffffff".into(),
            selected_item: "#0078d7".into(), selected_item_text: "#ffffff".into(),
            mark: "#ffff00".into(), mark_text: "#000000".into(), gray_text: "#6b6b6b".into(),
        }
    }
}

impl SystemColors {
    /// High contrast (Windows High Contrast theme)
    pub fn high_contrast() -> Self {
        Self {
            canvas: "#000000".into(), canvas_text: "#ffffff".into(),
            link_text: "#ffff00".into(), visited_text: "#ff00ff".into(), active_text: "#00ffff".into(),
            button_face: "#000000".into(), button_text: "#ffffff".into(), button_border: "#ffffff".into(),
            field: "#000000".into(), field_text: "#ffffff".into(),
            highlight: "#1aebff".into(), highlight_text: "#000000".into(),
            selected_item: "#1aebff".into(), selected_item_text: "#000000".into(),
            mark: "#ffff00".into(), mark_text: "#000000".into(), gray_text: "#00ff00".into(),
        }
    }
    
    /// Inverted colors
    pub fn inverted() -> Self {
        Self {
            canvas: "#000000".into(), canvas_text: "#ffffff".into(),
            link_text: "#9999ff".into(), visited_text: "#ff99ff".into(), active_text: "#00ffff".into(),
            button_face: "#333333".into(), button_text: "#ffffff".into(), button_border: "#888888".into(),
            field: "#222222".into(), field_text: "#ffffff".into(),
            highlight: "#ff8800".into(), highlight_text: "#000000".into(),
            selected_item: "#ff8800".into(), selected_item_text: "#000000".into(),
            mark: "#0000ff".into(), mark_text: "#ffffff".into(), gray_text: "#999999".into(),
        }
    }
}

/// High contrast settings
#[derive(Debug, Clone, Default)]
pub struct HighContrastSettings {
    pub forced_colors: ForcedColorsMode,
    pub color_scheme: ColorScheme,
    pub contrast_preference: ContrastPreference,
    pub system_colors: SystemColors,
}

impl HighContrastSettings {
    pub fn from_system() -> Self {
        // Would query OS settings - returning defaults
        Self::default()
    }
    
    pub fn is_active(&self) -> bool { self.forced_colors == ForcedColorsMode::Active }
    
    pub fn with_high_contrast() -> Self {
        Self { forced_colors: ForcedColorsMode::Active, contrast_preference: ContrastPreference::More,
               system_colors: SystemColors::high_contrast(), ..Default::default() }
    }
}

/// Contrast checker
#[derive(Debug)]
pub struct ContrastChecker;

impl ContrastChecker {
    /// Calculate relative luminance
    pub fn luminance(r: u8, g: u8, b: u8) -> f64 {
        fn channel(c: u8) -> f64 {
            let c = c as f64 / 255.0;
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }
    
    /// Calculate contrast ratio between two colors
    pub fn contrast_ratio(l1: f64, l2: f64) -> f64 {
        let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }
    
    /// Check if contrast meets WCAG AA (4.5:1 for normal text)
    pub fn meets_aa(ratio: f64, large_text: bool) -> bool {
        if large_text { ratio >= 3.0 } else { ratio >= 4.5 }
    }
    
    /// Check if contrast meets WCAG AAA (7:1 for normal text)
    pub fn meets_aaa(ratio: f64, large_text: bool) -> bool {
        if large_text { ratio >= 4.5 } else { ratio >= 7.0 }
    }
}

/// High contrast manager
#[derive(Debug, Default)]
pub struct HighContrastManager {
    settings: HighContrastSettings,
    listeners: Vec<u64>,
}

impl HighContrastManager {
    pub fn new() -> Self { Self::default() }
    
    pub fn settings(&self) -> &HighContrastSettings { &self.settings }
    pub fn update_settings(&mut self, settings: HighContrastSettings) { self.settings = settings; }
    
    pub fn enable_high_contrast(&mut self) { self.settings = HighContrastSettings::with_high_contrast(); }
    pub fn disable_high_contrast(&mut self) { self.settings = HighContrastSettings::default(); }
    
    pub fn add_listener(&mut self, id: u64) { self.listeners.push(id); }
    pub fn remove_listener(&mut self, id: u64) { self.listeners.retain(|&i| i != id); }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_contrast_ratio() {
        let white_lum = ContrastChecker::luminance(255, 255, 255);
        let black_lum = ContrastChecker::luminance(0, 0, 0);
        let ratio = ContrastChecker::contrast_ratio(white_lum, black_lum);
        assert!((ratio - 21.0).abs() < 0.1);
    }
    
    #[test]
    fn test_wcag() {
        assert!(ContrastChecker::meets_aa(5.0, false));
        assert!(!ContrastChecker::meets_aa(3.0, false));
        assert!(ContrastChecker::meets_aa(3.0, true));
    }
    
    #[test]
    fn test_high_contrast_settings() {
        let settings = HighContrastSettings::with_high_contrast();
        assert!(settings.is_active());
        assert_eq!(settings.system_colors.canvas, "#000000");
    }
}
