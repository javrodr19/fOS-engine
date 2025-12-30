//! Content Security Policy
//!
//! CSP parsing and enforcement.

use std::collections::HashMap;

/// CSP directive
#[derive(Debug, Clone, Default)]
pub struct ContentSecurityPolicy {
    pub directives: HashMap<String, Vec<String>>,
    pub report_only: bool,
}

/// CSP directive names
pub const DEFAULT_SRC: &str = "default-src";
pub const SCRIPT_SRC: &str = "script-src";
pub const STYLE_SRC: &str = "style-src";
pub const IMG_SRC: &str = "img-src";
pub const FONT_SRC: &str = "font-src";
pub const CONNECT_SRC: &str = "connect-src";
pub const MEDIA_SRC: &str = "media-src";
pub const OBJECT_SRC: &str = "object-src";
pub const FRAME_SRC: &str = "frame-src";
pub const CHILD_SRC: &str = "child-src";
pub const WORKER_SRC: &str = "worker-src";
pub const MANIFEST_SRC: &str = "manifest-src";
pub const BASE_URI: &str = "base-uri";
pub const FORM_ACTION: &str = "form-action";
pub const FRAME_ANCESTORS: &str = "frame-ancestors";
pub const REPORT_URI: &str = "report-uri";
pub const REPORT_TO: &str = "report-to";
pub const UPGRADE_INSECURE: &str = "upgrade-insecure-requests";
pub const BLOCK_ALL_MIXED: &str = "block-all-mixed-content";

/// CSP source keywords
pub const SELF: &str = "'self'";
pub const NONE: &str = "'none'";
pub const UNSAFE_INLINE: &str = "'unsafe-inline'";
pub const UNSAFE_EVAL: &str = "'unsafe-eval'";
pub const STRICT_DYNAMIC: &str = "'strict-dynamic'";
pub const UNSAFE_HASHES: &str = "'unsafe-hashes'";

impl ContentSecurityPolicy {
    pub fn new() -> Self { Self::default() }
    
    /// Parse CSP header
    pub fn parse(header: &str) -> Self {
        let mut policy = Self::new();
        
        for directive_str in header.split(';') {
            let directive_str = directive_str.trim();
            if directive_str.is_empty() {
                continue;
            }
            
            let parts: Vec<&str> = directive_str.split_whitespace().collect();
            if parts.is_empty() {
                continue;
            }
            
            let name = parts[0].to_lowercase();
            let values: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
            
            policy.directives.insert(name, values);
        }
        
        policy
    }
    
    /// Get directive values
    pub fn get(&self, directive: &str) -> Option<&Vec<String>> {
        self.directives.get(directive)
            .or_else(|| self.directives.get(DEFAULT_SRC))
    }
    
    /// Check if source is allowed for directive
    pub fn allows(&self, directive: &str, source: &str, origin: &str) -> bool {
        let values = match self.get(directive) {
            Some(v) => v,
            None => return true, // No restriction
        };
        
        for value in values {
            if self.source_matches(value, source, origin) {
                return true;
            }
        }
        
        false
    }
    
    fn source_matches(&self, pattern: &str, source: &str, origin: &str) -> bool {
        match pattern {
            NONE => false,
            SELF => source.starts_with(origin),
            "*" => true,
            p if p.starts_with("'nonce-") => {
                // Would check nonce
                false
            }
            p if p.starts_with("'sha256-") || p.starts_with("'sha384-") || p.starts_with("'sha512-") => {
                // Would check hash
                false
            }
            p => {
                // URL pattern matching
                if p.ends_with('*') {
                    source.starts_with(&p[..p.len()-1])
                } else {
                    source.starts_with(p)
                }
            }
        }
    }
    
    /// Check if inline scripts allowed
    pub fn allows_inline_script(&self) -> bool {
        self.get(SCRIPT_SRC)
            .map(|v| v.iter().any(|s| s == UNSAFE_INLINE))
            .unwrap_or(true)
    }
    
    /// Check if eval allowed
    pub fn allows_eval(&self) -> bool {
        self.get(SCRIPT_SRC)
            .map(|v| v.iter().any(|s| s == UNSAFE_EVAL))
            .unwrap_or(true)
    }
    
    /// Serialize to header
    pub fn serialize(&self) -> String {
        self.directives.iter()
            .map(|(k, v)| format!("{} {}", k, v.join(" ")))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// CSP violation report
#[derive(Debug, Clone)]
pub struct CspViolation {
    pub document_uri: String,
    pub violated_directive: String,
    pub effective_directive: String,
    pub original_policy: String,
    pub blocked_uri: String,
    pub status_code: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_csp() {
        let csp = ContentSecurityPolicy::parse("default-src 'self'; script-src 'self' https://cdn.example.com");
        
        assert!(csp.directives.contains_key("default-src"));
        assert!(csp.directives.contains_key("script-src"));
    }
    
    #[test]
    fn test_allows() {
        let csp = ContentSecurityPolicy::parse("default-src 'self'; img-src https://images.example.com");
        
        assert!(csp.allows("img-src", "https://images.example.com/foo.png", "https://example.com"));
        assert!(!csp.allows("img-src", "https://evil.com/foo.png", "https://example.com"));
    }
}
