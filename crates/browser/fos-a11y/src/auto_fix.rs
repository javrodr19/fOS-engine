//! Accessibility Auto-Fix
//!
//! Automatic detection of accessibility issues and fix suggestions.
//! Custom implementation with no external dependencies.

use crate::aria::AriaRole;
use crate::high_contrast::ContrastChecker;

/// Issue severity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum IssueSeverity {
    /// Minor issue, suggestion only
    Info,
    /// Should fix for better accessibility
    Warning,
    /// Must fix - fails WCAG AA
    Error,
    /// Critical - major a11y barrier
    Critical,
}

/// Accessibility issue type
#[derive(Debug, Clone)]
pub enum A11yIssue {
    /// Image missing alt text
    MissingAltText {
        element_id: u64,
    },
    /// Interactive element missing accessible name
    MissingLabel {
        element_id: u64,
        role: AriaRole,
    },
    /// Low color contrast
    LowContrast {
        element_id: u64,
        fg_color: (u8, u8, u8),
        bg_color: (u8, u8, u8),
        ratio: f64,
        required_ratio: f64,
    },
    /// Heading level skipped
    HeadingSkip {
        element_id: u64,
        expected_level: u32,
        actual_level: u32,
    },
    /// Focus not visible
    FocusNotVisible {
        element_id: u64,
    },
    /// Touch target too small
    SmallTouchTarget {
        element_id: u64,
        width: f64,
        height: f64,
        required_size: f64,
    },
    /// Missing language attribute
    MissingLang,
    /// Missing page title
    MissingTitle,
    /// Form field missing label
    FormMissingLabel {
        element_id: u64,
        input_type: String,
    },
    /// Link text not descriptive
    VagueLink {
        element_id: u64,
        text: String,
    },
    /// Auto-playing media
    AutoPlayMedia {
        element_id: u64,
    },
}

impl A11yIssue {
    /// Get severity of this issue
    pub fn severity(&self) -> IssueSeverity {
        match self {
            Self::MissingAltText { .. } => IssueSeverity::Error,
            Self::MissingLabel { .. } => IssueSeverity::Error,
            Self::LowContrast { ratio, required_ratio, .. } => {
                if *ratio < required_ratio * 0.5 {
                    IssueSeverity::Critical
                } else {
                    IssueSeverity::Error
                }
            }
            Self::HeadingSkip { .. } => IssueSeverity::Warning,
            Self::FocusNotVisible { .. } => IssueSeverity::Error,
            Self::SmallTouchTarget { .. } => IssueSeverity::Warning,
            Self::MissingLang => IssueSeverity::Error,
            Self::MissingTitle => IssueSeverity::Warning,
            Self::FormMissingLabel { .. } => IssueSeverity::Error,
            Self::VagueLink { .. } => IssueSeverity::Warning,
            Self::AutoPlayMedia { .. } => IssueSeverity::Warning,
        }
    }
    
    /// Get WCAG criteria this affects
    pub fn wcag_criteria(&self) -> &'static str {
        match self {
            Self::MissingAltText { .. } => "1.1.1 Non-text Content",
            Self::MissingLabel { .. } => "4.1.2 Name, Role, Value",
            Self::LowContrast { .. } => "1.4.3 Contrast (Minimum)",
            Self::HeadingSkip { .. } => "1.3.1 Info and Relationships",
            Self::FocusNotVisible { .. } => "2.4.7 Focus Visible",
            Self::SmallTouchTarget { .. } => "2.5.5 Target Size",
            Self::MissingLang => "3.1.1 Language of Page",
            Self::MissingTitle => "2.4.2 Page Titled",
            Self::FormMissingLabel { .. } => "1.3.1 Info and Relationships",
            Self::VagueLink { .. } => "2.4.4 Link Purpose",
            Self::AutoPlayMedia { .. } => "1.4.2 Audio Control",
        }
    }
}

/// Suggested fix for an issue
#[derive(Debug, Clone)]
pub struct SuggestedFix {
    /// Description of the fix
    pub description: String,
    /// Element to modify (if applicable)
    pub element_id: Option<u64>,
    /// Attribute to add/modify
    pub attribute: Option<String>,
    /// Suggested value
    pub value: Option<String>,
}

impl SuggestedFix {
    pub fn new(description: &str) -> Self {
        Self {
            description: description.to_string(),
            element_id: None,
            attribute: None,
            value: None,
        }
    }
    
    pub fn with_attribute(mut self, element_id: u64, attr: &str, value: &str) -> Self {
        self.element_id = Some(element_id);
        self.attribute = Some(attr.to_string());
        self.value = Some(value.to_string());
        self
    }
}

/// Accessibility audit
#[derive(Debug, Default)]
pub struct AccessibilityAudit {
    /// Detected issues
    pub issues: Vec<A11yIssue>,
    /// Whether to check contrast
    check_contrast: bool,
    /// Minimum contrast ratio for text
    min_contrast_ratio: f64,
    /// Minimum touch target size
    min_touch_target: f64,
}

impl AccessibilityAudit {
    pub fn new() -> Self {
        Self {
            issues: Vec::new(),
            check_contrast: true,
            min_contrast_ratio: 4.5, // WCAG AA
            min_touch_target: 44.0,  // WCAG 2.5.5
        }
    }
    
    /// Configure for WCAG AAA (stricter)
    pub fn wcag_aaa(mut self) -> Self {
        self.min_contrast_ratio = 7.0;
        self
    }
    
    /// Add an issue
    pub fn add_issue(&mut self, issue: A11yIssue) {
        self.issues.push(issue);
    }
    
    /// Check an image for alt text
    pub fn check_image(&mut self, element_id: u64, has_alt: bool, alt_text: &str) {
        if !has_alt || alt_text.is_empty() {
            self.add_issue(A11yIssue::MissingAltText { element_id });
        }
    }
    
    /// Check an interactive element for label
    pub fn check_label(&mut self, element_id: u64, role: AriaRole, has_label: bool) {
        if role.is_widget() && !has_label {
            self.add_issue(A11yIssue::MissingLabel { element_id, role });
        }
    }
    
    /// Check color contrast
    pub fn check_contrast(
        &mut self,
        element_id: u64,
        fg: (u8, u8, u8),
        bg: (u8, u8, u8),
        is_large_text: bool,
    ) {
        if !self.check_contrast {
            return;
        }
        
        let fg_lum = ContrastChecker::luminance(fg.0, fg.1, fg.2);
        let bg_lum = ContrastChecker::luminance(bg.0, bg.1, bg.2);
        let ratio = ContrastChecker::contrast_ratio(fg_lum, bg_lum);
        
        let required = if is_large_text { 3.0 } else { self.min_contrast_ratio };
        
        if ratio < required {
            self.add_issue(A11yIssue::LowContrast {
                element_id,
                fg_color: fg,
                bg_color: bg,
                ratio,
                required_ratio: required,
            });
        }
    }
    
    /// Check touch target size
    pub fn check_touch_target(&mut self, element_id: u64, width: f64, height: f64) {
        let min_dim = width.min(height);
        if min_dim < self.min_touch_target {
            self.add_issue(A11yIssue::SmallTouchTarget {
                element_id,
                width,
                height,
                required_size: self.min_touch_target,
            });
        }
    }
    
    /// Get suggested fix for an issue
    pub fn suggest_fix(&self, issue: &A11yIssue) -> SuggestedFix {
        match issue {
            A11yIssue::MissingAltText { element_id } => {
                SuggestedFix::new("Add alt attribute describing the image")
                    .with_attribute(*element_id, "alt", "Descriptive text here")
            }
            A11yIssue::MissingLabel { element_id, role } => {
                SuggestedFix::new(&format!("Add aria-label for {} element", format!("{:?}", role).to_lowercase()))
                    .with_attribute(*element_id, "aria-label", "Descriptive label")
            }
            A11yIssue::LowContrast { fg_color, bg_color, .. } => {
                // Suggest a fix by adjusting foreground
                let suggested = suggest_contrast_fix(*fg_color, *bg_color);
                SuggestedFix::new(&format!(
                    "Increase contrast. Suggested color: rgb({}, {}, {})",
                    suggested.0, suggested.1, suggested.2
                ))
            }
            A11yIssue::HeadingSkip { element_id, expected_level, .. } => {
                SuggestedFix::new(&format!("Use h{} instead", expected_level))
                    .with_attribute(*element_id, "role", "heading")
            }
            A11yIssue::MissingLang => {
                SuggestedFix::new("Add lang attribute to html element")
            }
            A11yIssue::MissingTitle => {
                SuggestedFix::new("Add a descriptive <title> element in <head>")
            }
            _ => SuggestedFix::new("Fix the accessibility issue"),
        }
    }
    
    /// Get issues filtered by severity
    pub fn issues_by_severity(&self, min_severity: IssueSeverity) -> Vec<&A11yIssue> {
        self.issues
            .iter()
            .filter(|i| i.severity() >= min_severity)
            .collect()
    }
    
    /// Get issue count by severity
    pub fn count_by_severity(&self) -> (usize, usize, usize, usize) {
        let mut info = 0;
        let mut warning = 0;
        let mut error = 0;
        let mut critical = 0;
        
        for issue in &self.issues {
            match issue.severity() {
                IssueSeverity::Info => info += 1,
                IssueSeverity::Warning => warning += 1,
                IssueSeverity::Error => error += 1,
                IssueSeverity::Critical => critical += 1,
            }
        }
        
        (info, warning, error, critical)
    }
    
    /// Check if audit passes (no errors or critical)
    pub fn passes(&self) -> bool {
        !self.issues.iter().any(|i| {
            matches!(i.severity(), IssueSeverity::Error | IssueSeverity::Critical)
        })
    }
    
    /// Clear all issues
    pub fn clear(&mut self) {
        self.issues.clear();
    }
}

/// Suggest a better foreground color for contrast
fn suggest_contrast_fix(fg: (u8, u8, u8), bg: (u8, u8, u8)) -> (u8, u8, u8) {
    let bg_lum = ContrastChecker::luminance(bg.0, bg.1, bg.2);
    
    // If background is dark, suggest light foreground
    if bg_lum < 0.5 {
        // Make foreground lighter
        let factor = 1.5f64;
        (
            (fg.0 as f64 * factor).min(255.0) as u8,
            (fg.1 as f64 * factor).min(255.0) as u8,
            (fg.2 as f64 * factor).min(255.0) as u8,
        )
    } else {
        // Make foreground darker
        let factor = 0.5f64;
        (
            (fg.0 as f64 * factor) as u8,
            (fg.1 as f64 * factor) as u8,
            (fg.2 as f64 * factor) as u8,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_accessibility_audit() {
        let mut audit = AccessibilityAudit::new();
        
        audit.check_image(1, true, "");
        audit.check_image(2, true, "A cat sitting");
        
        assert_eq!(audit.issues.len(), 1);
        assert!(matches!(audit.issues[0], A11yIssue::MissingAltText { element_id: 1 }));
    }
    
    #[test]
    fn test_contrast_check() {
        let mut audit = AccessibilityAudit::new();
        
        // Low contrast: gray on white
        audit.check_contrast(1, (150, 150, 150), (255, 255, 255), false);
        
        // High contrast: black on white
        audit.check_contrast(2, (0, 0, 0), (255, 255, 255), false);
        
        assert_eq!(audit.issues.len(), 1);
    }
    
    #[test]
    fn test_severity_filtering() {
        let mut audit = AccessibilityAudit::new();
        
        audit.add_issue(A11yIssue::MissingAltText { element_id: 1 });
        audit.add_issue(A11yIssue::HeadingSkip { 
            element_id: 2, 
            expected_level: 2, 
            actual_level: 4 
        });
        
        let errors = audit.issues_by_severity(IssueSeverity::Error);
        assert_eq!(errors.len(), 1);
    }
}
