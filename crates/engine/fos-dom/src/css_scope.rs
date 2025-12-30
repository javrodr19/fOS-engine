//! Shadow DOM CSS Scoping
//!
//! CSS encapsulation for shadow roots.

/// CSS scope mode for shadow DOM
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CssScopeMode {
    #[default]
    Encapsulated,
    Piercing, // For ::slotted, :host, etc.
}

/// Scoped stylesheet
#[derive(Debug, Clone)]
pub struct ScopedStyleSheet {
    /// Scope identifier
    pub scope_id: u32,
    /// CSS rules (scoped)
    pub rules: Vec<ScopedRule>,
    /// Host element selector
    pub host_selector: Option<String>,
}

/// Scoped CSS rule
#[derive(Debug, Clone)]
pub struct ScopedRule {
    /// Original selector
    pub selector: String,
    /// Scoped selector (with scope prefix)
    pub scoped_selector: String,
    /// CSS declarations
    pub declarations: Vec<(String, String)>,
}

impl ScopedStyleSheet {
    /// Create a new scoped stylesheet
    pub fn new(scope_id: u32) -> Self {
        Self {
            scope_id,
            rules: Vec::new(),
            host_selector: None,
        }
    }
    
    /// Add a rule and scope it
    pub fn add_rule(&mut self, selector: &str, declarations: Vec<(&str, &str)>) {
        let scoped_selector = self.scope_selector(selector);
        self.rules.push(ScopedRule {
            selector: selector.to_string(),
            scoped_selector,
            declarations: declarations.iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        });
    }
    
    /// Scope a selector
    pub fn scope_selector(&self, selector: &str) -> String {
        // Handle special selectors
        if selector.starts_with(":host") {
            return self.scope_host_selector(selector);
        }
        
        if selector.starts_with("::slotted") {
            return selector.to_string(); // Slotted handled specially
        }
        
        // Prefix with scope attribute selector
        format!("[data-scope=\"{}\"] {}", self.scope_id, selector)
    }
    
    /// Scope :host selector
    fn scope_host_selector(&self, selector: &str) -> String {
        if selector == ":host" {
            format!("[data-host=\"{}\"]", self.scope_id)
        } else if let Some(paren_content) = selector.strip_prefix(":host(").and_then(|s| s.strip_suffix(')')) {
            format!("[data-host=\"{}\"]:{}", self.scope_id, paren_content)
        } else if selector.starts_with(":host-context(") {
            // :host-context(.theme-dark) matches if ancestor has class
            let inner = selector.strip_prefix(":host-context(")
                .and_then(|s| s.strip_suffix(')'))
                .unwrap_or("");
            format!("{} [data-host=\"{}\"]", inner, self.scope_id)
        } else {
            selector.to_string()
        }
    }
    
    /// Generate adoptable stylesheet
    pub fn to_css(&self) -> String {
        self.rules.iter()
            .map(|rule| {
                let decls = rule.declarations.iter()
                    .map(|(k, v)| format!("{}: {};", k, v))
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{} {{ {} }}", rule.scoped_selector, decls)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// CSS Parts (::part)
#[derive(Debug, Clone, Default)]
pub struct CssParts {
    parts: Vec<(String, String)>, // (part_name, element_selector)
}

impl CssParts {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_part(&mut self, name: &str, element: &str) {
        self.parts.push((name.to_string(), element.to_string()));
    }
    
    pub fn get_part(&self, name: &str) -> Option<&str> {
        self.parts.iter()
            .find(|(n, _)| n == name)
            .map(|(_, e)| e.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scope_selector() {
        let sheet = ScopedStyleSheet::new(42);
        
        assert_eq!(
            sheet.scope_selector("div"),
            "[data-scope=\"42\"] div"
        );
        
        assert_eq!(
            sheet.scope_selector(".btn"),
            "[data-scope=\"42\"] .btn"
        );
    }
    
    #[test]
    fn test_host_selector() {
        let sheet = ScopedStyleSheet::new(42);
        
        assert_eq!(
            sheet.scope_selector(":host"),
            "[data-host=\"42\"]"
        );
        
        assert_eq!(
            sheet.scope_selector(":host(.active)"),
            "[data-host=\"42\"]:.active"
        );
    }
    
    #[test]
    fn test_to_css() {
        let mut sheet = ScopedStyleSheet::new(1);
        sheet.add_rule("p", vec![("color", "red")]);
        
        let css = sheet.to_css();
        assert!(css.contains("[data-scope=\"1\"] p"));
        assert!(css.contains("color: red"));
    }
}
