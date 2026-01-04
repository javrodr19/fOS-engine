//! XSS Protection & Sanitization
//!
//! HTML sanitization and script injection detection.

use std::collections::{HashMap, HashSet};

/// Sanitizer configuration
#[derive(Debug, Clone)]
pub struct SanitizerConfig {
    pub allowed_elements: HashSet<String>,
    pub blocked_elements: HashSet<String>,
    pub allowed_attributes: HashMap<String, HashSet<String>>,
    pub drop_elements: HashSet<String>,
    pub drop_attributes: HashSet<String>,
    pub allow_custom_elements: bool,
    pub allow_comments: bool,
}

impl Default for SanitizerConfig {
    fn default() -> Self {
        let mut config = Self {
            allowed_elements: HashSet::new(), blocked_elements: HashSet::new(),
            allowed_attributes: HashMap::new(), drop_elements: HashSet::new(),
            drop_attributes: HashSet::new(), allow_custom_elements: false, allow_comments: false,
        };
        
        // Default safe elements
        for tag in ["a", "abbr", "b", "blockquote", "br", "code", "div", "em", "h1", "h2", "h3",
                    "h4", "h5", "h6", "hr", "i", "li", "ol", "p", "pre", "s", "span", "strong",
                    "sub", "sup", "table", "tbody", "td", "tfoot", "th", "thead", "tr", "u", "ul"] {
            config.allowed_elements.insert(tag.into());
        }
        
        // Dangerous elements
        for tag in ["script", "style", "iframe", "frame", "frameset", "object", "embed",
                    "applet", "form", "input", "button", "select", "textarea", "base", "meta", "link"] {
            config.drop_elements.insert(tag.into());
        }
        
        // Dangerous attributes
        for attr in ["onclick", "onerror", "onload", "onmouseover", "onfocus", "onblur",
                     "onsubmit", "onreset", "onchange", "oninput", "onkeydown", "onkeyup",
                     "onkeypress", "onmousedown", "onmouseup", "onmousemove", "ondblclick",
                     "oncontextmenu", "ondrag", "ondrop", "onscroll", "onwheel", "oncopy",
                     "oncut", "onpaste", "onbeforeunload", "formaction", "xlink:href"] {
            config.drop_attributes.insert(attr.into());
        }
        
        // Safe global attributes
        let global_attrs: HashSet<String> = ["class", "id", "title", "lang", "dir", "hidden", "tabindex"]
            .iter().map(|s| s.to_string()).collect();
        config.allowed_attributes.insert("*".into(), global_attrs);
        
        // href for links
        let mut a_attrs: HashSet<String> = ["href", "target", "rel"].iter().map(|s| s.to_string()).collect();
        config.allowed_attributes.insert("a".into(), a_attrs);
        
        config
    }
}

/// HTML sanitizer
#[derive(Debug)]
pub struct Sanitizer {
    config: SanitizerConfig,
}

impl Sanitizer {
    pub fn new(config: SanitizerConfig) -> Self { Self { config } }
    pub fn default_safe() -> Self { Self::new(SanitizerConfig::default()) }
    
    /// Sanitize HTML string
    pub fn sanitize(&self, html: &str) -> String {
        let mut output = String::new();
        let mut in_dropped_element: u32 = 0;
        let mut chars = html.chars().peekable();
        
        while let Some(c) = chars.next() {
            if c == '<' {
                let mut tag = String::new();
                while let Some(&tc) = chars.peek() {
                    if tc == '>' { chars.next(); break; }
                    tag.push(chars.next().unwrap());
                }
                
                let is_closing = tag.starts_with('/');
                let tag_name = tag.trim_start_matches('/').split_whitespace().next()
                    .unwrap_or("").to_lowercase();
                
                if self.config.drop_elements.contains(&tag_name) {
                    if is_closing { in_dropped_element = in_dropped_element.saturating_sub(1); }
                    else { in_dropped_element += 1; }
                    continue;
                }
                
                if in_dropped_element > 0 { continue; }
                
                if !self.config.allowed_elements.contains(&tag_name) &&
                   !self.config.blocked_elements.contains(&tag_name) {
                    if !self.config.allow_custom_elements || !tag_name.contains('-') { continue; }
                }
                
                // Sanitize attributes
                let sanitized = self.sanitize_tag(&tag, &tag_name, is_closing);
                output.push('<');
                output.push_str(&sanitized);
                output.push('>');
            } else if in_dropped_element == 0 {
                output.push(c);
            }
        }
        output
    }
    
    fn sanitize_tag(&self, tag: &str, tag_name: &str, is_closing: bool) -> String {
        if is_closing { return format!("/{}", tag_name); }
        
        let mut result = tag_name.to_string();
        let attrs_start = tag.find(char::is_whitespace);
        
        if let Some(start) = attrs_start {
            let attrs_str = &tag[start..];
            for (name, value) in self.parse_attributes(attrs_str) {
                if self.is_attribute_allowed(&name, tag_name) {
                    let safe_value = self.sanitize_attribute_value(&name, &value);
                    result.push_str(&format!(" {}=\"{}\"", name, safe_value));
                }
            }
        }
        result
    }
    
    fn parse_attributes(&self, attrs: &str) -> Vec<(String, String)> {
        let mut result = Vec::new();
        let mut current_name = String::new();
        let mut current_value = String::new();
        let mut in_value = false;
        let mut quote_char = None;
        
        for c in attrs.chars() {
            match (in_value, quote_char, c) {
                (false, _, '=') => in_value = true,
                (false, _, c) if c.is_whitespace() && !current_name.is_empty() => {
                    result.push((current_name.clone().to_lowercase(), current_value.clone()));
                    current_name.clear(); current_value.clear();
                }
                (false, _, c) if !c.is_whitespace() => current_name.push(c),
                (true, None, '"' | '\'') => quote_char = Some(c),
                (true, Some(q), c) if c == q => {
                    result.push((current_name.clone().to_lowercase(), current_value.clone()));
                    current_name.clear(); current_value.clear();
                    in_value = false; quote_char = None;
                }
                (true, _, c) => current_value.push(c),
                _ => {}
            }
        }
        if !current_name.is_empty() {
            result.push((current_name.to_lowercase(), current_value));
        }
        result
    }
    
    fn is_attribute_allowed(&self, attr: &str, element: &str) -> bool {
        if self.config.drop_attributes.contains(attr) { return false; }
        if attr.starts_with("on") { return false; } // Event handlers
        if let Some(allowed) = self.config.allowed_attributes.get(element) {
            if allowed.contains(attr) { return true; }
        }
        if let Some(global) = self.config.allowed_attributes.get("*") {
            return global.contains(attr);
        }
        false
    }
    
    fn sanitize_attribute_value(&self, attr: &str, value: &str) -> String {
        let value = value.replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;");
        
        // Check for javascript: URLs
        if attr == "href" || attr == "src" {
            let lower = value.to_lowercase().trim().replace(char::is_whitespace, "");
            if lower.starts_with("javascript:") || lower.starts_with("data:text/html") ||
               lower.starts_with("vbscript:") {
                return String::new();
            }
        }
        value
    }
}

/// XSS pattern detector
#[derive(Debug, Default)]
pub struct XssDetector {
    patterns: Vec<String>,
}

impl XssDetector {
    pub fn new() -> Self {
        Self { patterns: vec![
            "<script".into(), "javascript:".into(), "onerror=".into(), "onload=".into(),
            "onclick=".into(), "onmouseover=".into(), "eval(".into(), "expression(".into(),
        ]}
    }
    
    pub fn detect(&self, input: &str) -> bool {
        let lower = input.to_lowercase();
        self.patterns.iter().any(|p| lower.contains(p))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_sanitize_script() {
        let sanitizer = Sanitizer::default_safe();
        let input = "<div><script>alert('xss')</script></div>";
        let output = sanitizer.sanitize(input);
        assert!(!output.contains("script"));
    }
    
    #[test]
    fn test_sanitize_event_handler() {
        let sanitizer = Sanitizer::default_safe();
        let input = "<div onclick=\"alert('xss')\">test</div>";
        let output = sanitizer.sanitize(input);
        assert!(!output.contains("onclick"));
    }
    
    #[test]
    fn test_xss_detection() {
        let detector = XssDetector::new();
        assert!(detector.detect("<script>"));
        assert!(detector.detect("javascript:void(0)"));
        assert!(!detector.detect("normal text"));
    }
}
