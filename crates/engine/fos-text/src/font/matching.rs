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
/// Faces drawn in place of a common family a system lacks: free faces with
/// the same metrics (as fontconfig substitutes them), so text takes the
/// space a page was designed for
pub fn metric_aliases(family: &str) -> &'static [&'static str] {
    match family.to_lowercase().as_str() {
        "arial" | "helvetica" => &["Liberation Sans", "Arimo"],
        "times new roman" | "times" => &["Liberation Serif", "Tinos"],
        "courier new" | "courier" => &["Liberation Mono", "Cousine"],
        _ => &[],
    }
}

pub fn resolve_generic_family(family: &str) -> &[&str] {
    match family.to_lowercase().as_str() {
        // As browsers default to (Arial and Times New Roman, or their
        // metric-compatible stand-ins)
        "serif" => &["Times New Roman", "Times", "Liberation Serif", "Tinos", "DejaVu Serif", "Noto Serif"],
        "sans-serif" => &["Arial", "Helvetica", "Liberation Sans", "Arimo", "DejaVu Sans", "Noto Sans"],
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
