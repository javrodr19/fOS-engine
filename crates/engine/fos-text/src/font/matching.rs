//! Font matching and query

use super::{FontWeight, FontStyle};

/// Font query for matching
#[derive(Debug, Clone)]
pub struct FontQuery {
    /// Font families to try (in order)
    pub families: Vec<String>,
    /// Desired weight
    pub weight: FontWeight,
    /// Desired style
    pub style: FontStyle,
}

impl FontQuery {
    /// Create a new font query
    pub fn new(families: &[&str]) -> Self {
        Self {
            families: families.iter().map(|s| s.to_string()).collect(),
            weight: FontWeight::NORMAL,
            style: FontStyle::Normal,
        }
    }
    
    /// Set font weight
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }
    
    /// Set font style
    pub fn style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }
    
    /// Set bold weight
    pub fn bold(self) -> Self {
        self.weight(FontWeight::BOLD)
    }
    
    /// Set italic style
    pub fn italic(self) -> Self {
        self.style(FontStyle::Italic)
    }
}

impl Default for FontQuery {
    fn default() -> Self {
        Self::new(&["sans-serif"])
    }
}

/// Resolve generic font family to system families
pub fn resolve_generic_family(family: &str) -> &[&str] {
    match family.to_lowercase().as_str() {
        "serif" => &["Times New Roman", "Times", "DejaVu Serif", "Noto Serif"],
        "sans-serif" => &["Arial", "Helvetica", "DejaVu Sans", "Noto Sans", "Liberation Sans"],
        "monospace" => &["Courier New", "Consolas", "DejaVu Sans Mono", "Noto Sans Mono"],
        "cursive" => &["Comic Sans MS", "Brush Script MT"],
        "fantasy" => &["Impact", "Papyrus"],
        "system-ui" => &["Segoe UI", "San Francisco", "Ubuntu", "Cantarell"],
        "ui-serif" => &["Georgia", "Times New Roman"],
        "ui-sans-serif" => &["Segoe UI", "SF Pro", "Roboto"],
        "ui-monospace" => &["SF Mono", "Consolas", "Menlo"],
        _ => &[],
    }
}
