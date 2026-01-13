//! Media Preferences
//!
//! CSS media query preference support.
//! Custom implementation with no external dependencies.

/// Data usage preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DataPreference {
    #[default]
    NoPreference,
    Reduce,
}

/// Transparency preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransparencyPreference {
    #[default]
    NoPreference,
    Reduce,
}

/// Color scheme preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorSchemePreference {
    Light,
    #[default]
    Dark,
}

/// Contrast preference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContrastPref {
    #[default]
    NoPreference,
    More,
    Less,
    Custom,
}

/// Combined media preferences
#[derive(Debug, Clone, Default)]
pub struct MediaPreferences {
    /// prefers-reduced-motion
    pub reduced_motion: bool,
    /// prefers-reduced-data  
    pub reduced_data: DataPreference,
    /// prefers-reduced-transparency
    pub reduced_transparency: TransparencyPreference,
    /// prefers-color-scheme
    pub color_scheme: ColorSchemePreference,
    /// prefers-contrast
    pub contrast: ContrastPref,
    /// forced-colors
    pub forced_colors: bool,
    /// inverted-colors
    pub inverted_colors: bool,
}

impl MediaPreferences {
    pub fn new() -> Self { Self::default() }
    
    /// Query from system settings
    pub fn from_system() -> Self {
        // Would query OS preferences
        // For now, return defaults
        Self::default()
    }
    
    /// Check if a media query matches
    pub fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        
        // prefers-reduced-motion
        if query.contains("prefers-reduced-motion") {
            if query.contains("reduce") && !query.contains("no-preference") {
                return self.reduced_motion;
            }
            if query.contains("no-preference") {
                return !self.reduced_motion;
            }
        }
        
        // prefers-reduced-data
        if query.contains("prefers-reduced-data") {
            if query.contains("reduce") && !query.contains("no-preference") {
                return self.reduced_data == DataPreference::Reduce;
            }
            if query.contains("no-preference") {
                return self.reduced_data == DataPreference::NoPreference;
            }
        }
        
        // prefers-reduced-transparency
        if query.contains("prefers-reduced-transparency") {
            if query.contains("reduce") && !query.contains("no-preference") {
                return self.reduced_transparency == TransparencyPreference::Reduce;
            }
            if query.contains("no-preference") {
                return self.reduced_transparency == TransparencyPreference::NoPreference;
            }
        }
        
        // prefers-color-scheme
        if query.contains("prefers-color-scheme") {
            if query.contains("dark") {
                return self.color_scheme == ColorSchemePreference::Dark;
            }
            if query.contains("light") {
                return self.color_scheme == ColorSchemePreference::Light;
            }
        }
        
        // prefers-contrast
        if query.contains("prefers-contrast") {
            if query.contains("more") {
                return self.contrast == ContrastPref::More;
            }
            if query.contains("less") {
                return self.contrast == ContrastPref::Less;
            }
            if query.contains("custom") {
                return self.contrast == ContrastPref::Custom;
            }
            if query.contains("no-preference") {
                return self.contrast == ContrastPref::NoPreference;
            }
        }
        
        // forced-colors
        if query.contains("forced-colors") {
            if query.contains("active") {
                return self.forced_colors;
            }
            if query.contains("none") {
                return !self.forced_colors;
            }
        }
        
        // inverted-colors
        if query.contains("inverted-colors") {
            if query.contains("inverted") && !query.contains("none") {
                return self.inverted_colors;
            }
            if query.contains("none") {
                return !self.inverted_colors;
            }
        }
        
        false
    }
    
    /// Set reduced motion
    pub fn set_reduced_motion(&mut self, reduce: bool) {
        self.reduced_motion = reduce;
    }
    
    /// Set reduced data
    pub fn set_reduced_data(&mut self, pref: DataPreference) {
        self.reduced_data = pref;
    }
    
    /// Set reduced transparency
    pub fn set_reduced_transparency(&mut self, pref: TransparencyPreference) {
        self.reduced_transparency = pref;
    }
    
    /// Set color scheme
    pub fn set_color_scheme(&mut self, scheme: ColorSchemePreference) {
        self.color_scheme = scheme;
    }
    
    /// Set contrast preference
    pub fn set_contrast(&mut self, contrast: ContrastPref) {
        self.contrast = contrast;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_media_query_matching() {
        let mut prefs = MediaPreferences::new();
        
        // Test reduced motion
        prefs.reduced_motion = true;
        assert!(prefs.matches("(prefers-reduced-motion: reduce)"));
        assert!(!prefs.matches("(prefers-reduced-motion: no-preference)"));
        
        prefs.reduced_motion = false;
        assert!(!prefs.matches("(prefers-reduced-motion: reduce)"));
        assert!(prefs.matches("(prefers-reduced-motion: no-preference)"));
    }
    
    #[test]
    fn test_color_scheme() {
        let mut prefs = MediaPreferences::new();
        
        prefs.color_scheme = ColorSchemePreference::Dark;
        assert!(prefs.matches("(prefers-color-scheme: dark)"));
        assert!(!prefs.matches("(prefers-color-scheme: light)"));
        
        prefs.color_scheme = ColorSchemePreference::Light;
        assert!(!prefs.matches("(prefers-color-scheme: dark)"));
        assert!(prefs.matches("(prefers-color-scheme: light)"));
    }
    
    #[test]
    fn test_reduced_data() {
        let mut prefs = MediaPreferences::new();
        
        prefs.reduced_data = DataPreference::Reduce;
        assert!(prefs.matches("(prefers-reduced-data: reduce)"));
        
        prefs.reduced_data = DataPreference::NoPreference;
        assert!(prefs.matches("(prefers-reduced-data: no-preference)"));
    }
}
