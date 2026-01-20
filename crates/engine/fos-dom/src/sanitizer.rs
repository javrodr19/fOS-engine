//! HTML Sanitizer API
//!
//! W3C Sanitizer API implementation for safe HTML handling.
//! Removes dangerous elements/attributes to prevent XSS attacks.

use std::collections::HashSet;

/// Default elements that are always blocked
const BLOCKED_ELEMENTS: &[&str] = &[
    "script", "style", "iframe", "object", "embed", "applet",
    "base", "meta", "link", "frame", "frameset", "noscript",
];

/// Default dangerous attributes
const BLOCKED_ATTRS: &[&str] = &[
    "onclick", "onload", "onerror", "onmouseover", "onmouseout",
    "onfocus", "onblur", "onkeydown", "onkeyup", "onkeypress",
    "onsubmit", "onreset", "onchange", "oninput", "oncontextmenu",
];

/// URL schemes that are allowed in href/src attributes
const SAFE_URL_SCHEMES: &[&str] = &["http", "https", "mailto", "tel", "data"];

/// Sanitizer configuration
#[derive(Debug, Clone)]
pub struct SanitizerConfig {
    /// Elements to allow (if empty, allow all non-blocked)
    pub allow_elements: HashSet<String>,
    /// Elements to explicitly block
    pub block_elements: HashSet<String>,
    /// Attributes to allow (if empty, allow all non-blocked)
    pub allow_attrs: HashSet<String>,
    /// Attributes to explicitly block
    pub block_attrs: HashSet<String>,
    /// Whether to allow custom elements
    pub allow_custom_elements: bool,
    /// Whether to allow comments
    pub allow_comments: bool,
    /// Whether to allow data: URLs
    pub allow_data_urls: bool,
}

impl Default for SanitizerConfig {
    fn default() -> Self {
        Self {
            allow_elements: HashSet::new(),
            block_elements: BLOCKED_ELEMENTS.iter().map(|s| s.to_string()).collect(),
            allow_attrs: HashSet::new(),
            block_attrs: BLOCKED_ATTRS.iter().map(|s| s.to_string()).collect(),
            allow_custom_elements: false,
            allow_comments: false,
            allow_data_urls: false,
        }
    }
}

impl SanitizerConfig {
    /// Create a new config with default security settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a permissive config (use with caution)
    pub fn permissive() -> Self {
        Self {
            allow_elements: HashSet::new(),
            block_elements: ["script", "style"].iter().map(|s| s.to_string()).collect(),
            allow_attrs: HashSet::new(),
            block_attrs: HashSet::new(),
            allow_custom_elements: true,
            allow_comments: true,
            allow_data_urls: true,
        }
    }

    /// Allow specific element
    pub fn allow_element(mut self, tag: &str) -> Self {
        self.allow_elements.insert(tag.to_lowercase());
        self.block_elements.remove(&tag.to_lowercase());
        self
    }

    /// Block specific element
    pub fn block_element(mut self, tag: &str) -> Self {
        self.block_elements.insert(tag.to_lowercase());
        self.allow_elements.remove(&tag.to_lowercase());
        self
    }

    /// Allow specific attribute
    pub fn allow_attr(mut self, attr: &str) -> Self {
        self.allow_attrs.insert(attr.to_lowercase());
        self.block_attrs.remove(&attr.to_lowercase());
        self
    }

    /// Block specific attribute
    pub fn block_attr(mut self, attr: &str) -> Self {
        self.block_attrs.insert(attr.to_lowercase());
        self.allow_attrs.remove(&attr.to_lowercase());
        self
    }
}

/// Result of sanitization
#[derive(Debug, Clone)]
pub struct SanitizeResult {
    /// Sanitized HTML string
    pub html: String,
    /// Number of elements removed
    pub elements_removed: usize,
    /// Number of attributes removed
    pub attrs_removed: usize,
    /// Specific items that were removed
    pub removed_items: Vec<RemovedItem>,
}

/// An item removed during sanitization
#[derive(Debug, Clone)]
pub enum RemovedItem {
    Element { tag: String, reason: String },
    Attribute { name: String, element: String, reason: String },
    Comment,
}

/// HTML Sanitizer
#[derive(Debug, Clone)]
pub struct Sanitizer {
    config: SanitizerConfig,
}

impl Default for Sanitizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Sanitizer {
    /// Create a new sanitizer with default config
    pub fn new() -> Self {
        Self {
            config: SanitizerConfig::default(),
        }
    }

    /// Create a sanitizer with custom config
    pub fn with_config(config: SanitizerConfig) -> Self {
        Self { config }
    }

    /// Check if an element is allowed
    pub fn is_element_allowed(&self, tag: &str) -> bool {
        let tag_lower = tag.to_lowercase();

        // Explicitly blocked
        if self.config.block_elements.contains(&tag_lower) {
            return false;
        }

        // Custom elements check
        if tag.contains('-') && !self.config.allow_custom_elements {
            return false;
        }

        // If allow list is specified, element must be in it
        if !self.config.allow_elements.is_empty() {
            return self.config.allow_elements.contains(&tag_lower);
        }

        true
    }

    /// Check if an attribute is allowed
    pub fn is_attr_allowed(&self, attr: &str, _element: &str) -> bool {
        let attr_lower = attr.to_lowercase();

        // Explicitly blocked
        if self.config.block_attrs.contains(&attr_lower) {
            return false;
        }

        // Block all event handlers (on*)
        if attr_lower.starts_with("on") {
            return false;
        }

        // If allow list is specified, attribute must be in it
        if !self.config.allow_attrs.is_empty() {
            return self.config.allow_attrs.contains(&attr_lower);
        }

        true
    }

    /// Check if a URL is safe
    pub fn is_url_safe(&self, url: &str) -> bool {
        let url_trimmed = url.trim().to_lowercase();

        // Check for javascript: URLs
        if url_trimmed.starts_with("javascript:") {
            return false;
        }

        // Check for vbscript: URLs
        if url_trimmed.starts_with("vbscript:") {
            return false;
        }

        // Check data: URLs
        if url_trimmed.starts_with("data:") && !self.config.allow_data_urls {
            return false;
        }

        true
    }

    /// Sanitize an HTML string
    pub fn sanitize(&self, html: &str) -> SanitizeResult {
        let mut result = SanitizeResult {
            html: String::with_capacity(html.len()),
            elements_removed: 0,
            attrs_removed: 0,
            removed_items: Vec::new(),
        };

        let mut in_blocked_element = 0;
        let mut blocked_element_name = String::new();
        let mut i = 0;
        let bytes = html.as_bytes();

        while i < bytes.len() {
            if bytes[i] == b'<' {
                // Parse tag
                let tag_start = i;
                i += 1;

                // Check for comment
                if i + 2 < bytes.len() && &bytes[i..i + 3] == b"!--" {
                    // Find comment end
                    let mut comment_end = i + 3;
                    while comment_end + 2 < bytes.len() {
                        if &bytes[comment_end..comment_end + 3] == b"-->" {
                            comment_end += 3;
                            break;
                        }
                        comment_end += 1;
                    }

                    if self.config.allow_comments && in_blocked_element == 0 {
                        result.html.push_str(&html[tag_start..comment_end]);
                    } else if !self.config.allow_comments {
                        result.removed_items.push(RemovedItem::Comment);
                    }

                    i = comment_end;
                    continue;
                }

                // Check for closing tag
                let is_closing = i < bytes.len() && bytes[i] == b'/';
                if is_closing {
                    i += 1;
                }

                // Extract tag name
                let name_start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-') {
                    i += 1;
                }
                let tag_name = &html[name_start..i];

                // Find end of tag
                while i < bytes.len() && bytes[i] != b'>' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1; // Skip '>'
                }

                let tag_lower = tag_name.to_lowercase();

                if is_closing {
                    // Closing tag
                    if in_blocked_element > 0 && tag_lower == blocked_element_name {
                        in_blocked_element -= 1;
                        if in_blocked_element == 0 {
                            blocked_element_name.clear();
                        }
                    } else if in_blocked_element == 0 && self.is_element_allowed(tag_name) {
                        result.html.push_str(&html[tag_start..i]);
                    }
                } else {
                    // Opening tag
                    if in_blocked_element > 0 {
                        // Already inside blocked element, check for nesting
                        if tag_lower == blocked_element_name {
                            in_blocked_element += 1;
                        }
                    } else if !self.is_element_allowed(tag_name) {
                        // Start blocking
                        in_blocked_element = 1;
                        blocked_element_name = tag_lower.clone();
                        result.elements_removed += 1;
                        result.removed_items.push(RemovedItem::Element {
                            tag: tag_name.to_string(),
                            reason: "blocked element".to_string(),
                        });
                    } else {
                        // Allowed element - sanitize attributes
                        let sanitized = self.sanitize_tag(&html[tag_start..i], tag_name, &mut result);
                        result.html.push_str(&sanitized);
                    }
                }
            } else if in_blocked_element == 0 {
                // Regular text content
                result.html.push(bytes[i] as char);
                i += 1;
            } else {
                i += 1;
            }
        }

        result
    }

    /// Sanitize a single tag's attributes
    fn sanitize_tag(&self, tag: &str, tag_name: &str, result: &mut SanitizeResult) -> String {
        // Simple attribute parser
        let mut output = String::with_capacity(tag.len());
        output.push('<');
        output.push_str(tag_name);

        // Find attributes section
        let attr_start = tag_name.len() + 1;
        if attr_start < tag.len() - 1 {
            let attrs_section = &tag[attr_start..tag.len() - 1].trim();
            
            // Parse and filter attributes
            let mut remaining = *attrs_section;
            while !remaining.is_empty() {
                remaining = remaining.trim_start();
                if remaining.is_empty() || remaining.starts_with('/') {
                    break;
                }

                // Find attribute name
                let name_end = remaining.find(|c: char| c == '=' || c.is_whitespace() || c == '/' || c == '>')
                    .unwrap_or(remaining.len());
                let attr_name = &remaining[..name_end];
                remaining = &remaining[name_end..];

                // Find value if present
                let mut attr_value = String::new();
                remaining = remaining.trim_start();
                if remaining.starts_with('=') {
                    remaining = &remaining[1..].trim_start();
                    
                    if remaining.starts_with('"') {
                        // Quoted value
                        remaining = &remaining[1..];
                        if let Some(end) = remaining.find('"') {
                            attr_value = remaining[..end].to_string();
                            remaining = &remaining[end + 1..];
                        }
                    } else if remaining.starts_with('\'') {
                        remaining = &remaining[1..];
                        if let Some(end) = remaining.find('\'') {
                            attr_value = remaining[..end].to_string();
                            remaining = &remaining[end + 1..];
                        }
                    } else {
                        // Unquoted value
                        let end = remaining.find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                            .unwrap_or(remaining.len());
                        attr_value = remaining[..end].to_string();
                        remaining = &remaining[end..];
                    }
                }

                // Check if attribute is allowed
                if !attr_name.is_empty() && self.is_attr_allowed(attr_name, tag_name) {
                    // Check URL safety for href/src
                    let attr_lower = attr_name.to_lowercase();
                    if (attr_lower == "href" || attr_lower == "src" || attr_lower == "action")
                        && !self.is_url_safe(&attr_value)
                    {
                        result.attrs_removed += 1;
                        result.removed_items.push(RemovedItem::Attribute {
                            name: attr_name.to_string(),
                            element: tag_name.to_string(),
                            reason: "unsafe URL".to_string(),
                        });
                        continue;
                    }

                    output.push(' ');
                    output.push_str(attr_name);
                    if !attr_value.is_empty() {
                        output.push_str("=\"");
                        output.push_str(&attr_value);
                        output.push('"');
                    }
                } else if !attr_name.is_empty() {
                    result.attrs_removed += 1;
                    result.removed_items.push(RemovedItem::Attribute {
                        name: attr_name.to_string(),
                        element: tag_name.to_string(),
                        reason: "blocked attribute".to_string(),
                    });
                }
            }
        }

        // Handle self-closing
        if tag.trim_end().ends_with("/>") {
            output.push_str(" />");
        } else {
            output.push('>');
        }

        output
    }

    /// Sanitize and return only the string
    pub fn sanitize_to_string(&self, html: &str) -> String {
        self.sanitize(html).html
    }

    /// Sanitize HTML intended for a specific element context
    pub fn sanitize_for(&self, html: &str, element: &str) -> SanitizeResult {
        // For specific contexts, we may apply stricter rules
        match element.to_lowercase().as_str() {
            "script" | "style" => {
                // Never allow content for these
                SanitizeResult {
                    html: String::new(),
                    elements_removed: 1,
                    attrs_removed: 0,
                    removed_items: vec![RemovedItem::Element {
                        tag: element.to_string(),
                        reason: "blocked context".to_string(),
                    }],
                }
            }
            _ => self.sanitize(html),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitizer_default() {
        let sanitizer = Sanitizer::new();
        
        let result = sanitizer.sanitize("<div>Hello</div>");
        assert_eq!(result.html, "<div>Hello</div>");
        assert_eq!(result.elements_removed, 0);
    }

    #[test]
    fn test_sanitizer_removes_script() {
        let sanitizer = Sanitizer::new();
        
        let result = sanitizer.sanitize("<div>Hello<script>alert(1)</script>World</div>");
        assert_eq!(result.html, "<div>HelloWorld</div>");
        assert_eq!(result.elements_removed, 1);
    }

    #[test]
    fn test_sanitizer_removes_onclick() {
        let sanitizer = Sanitizer::new();
        
        let result = sanitizer.sanitize(r#"<div onclick="alert(1)">Click me</div>"#);
        assert_eq!(result.html, "<div>Click me</div>");
        assert_eq!(result.attrs_removed, 1);
    }

    #[test]
    fn test_sanitizer_removes_javascript_url() {
        let sanitizer = Sanitizer::new();
        
        let result = sanitizer.sanitize(r#"<a href="javascript:alert(1)">Link</a>"#);
        assert_eq!(result.html, "<a>Link</a>");
        assert_eq!(result.attrs_removed, 1);
    }

    #[test]
    fn test_sanitizer_allows_safe_url() {
        let sanitizer = Sanitizer::new();
        
        let result = sanitizer.sanitize(r#"<a href="https://example.com">Link</a>"#);
        assert_eq!(result.html, r#"<a href="https://example.com">Link</a>"#);
        assert_eq!(result.attrs_removed, 0);
    }

    #[test]
    fn test_sanitizer_nested_blocked() {
        let sanitizer = Sanitizer::new();
        
        let result = sanitizer.sanitize("<div><script><script>nested</script></script></div>");
        assert_eq!(result.html, "<div></div>");
    }

    #[test]
    fn test_custom_config() {
        let config = SanitizerConfig::new()
            .block_element("div")
            .allow_attr("class");
        let sanitizer = Sanitizer::with_config(config);
        
        let result = sanitizer.sanitize(r#"<div class="foo">Hello</div><span class="bar">World</span>"#);
        assert!(result.html.contains("span"));
        assert!(!result.html.contains("div"));
    }

    #[test]
    fn test_sanitize_for_script_context() {
        let sanitizer = Sanitizer::new();
        
        let result = sanitizer.sanitize_for("<div>content</div>", "script");
        assert_eq!(result.html, "");
    }

    #[test]
    fn test_xss_vectors() {
        let sanitizer = Sanitizer::new();
        
        // Various XSS attempts
        let vectors = [
            r#"<img src=x onerror=alert(1)>"#,
            r#"<svg onload=alert(1)>"#,
            r#"<body onload=alert(1)>"#,
            r#"<input onfocus=alert(1) autofocus>"#,
            r#"<a href="javascript:alert(1)">xss</a>"#,
        ];

        for vector in vectors {
            let result = sanitizer.sanitize(vector);
            // Should not contain any event handlers
            assert!(!result.html.to_lowercase().contains("on"));
            assert!(!result.html.to_lowercase().contains("javascript:"));
        }
    }
}
