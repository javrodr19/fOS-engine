//! HTTPS and Secure Contexts
//!
//! HTTPS enforcement and mixed content blocking.

/// Secure context
#[derive(Debug, Clone)]
pub struct SecureContext {
    pub is_secure: bool,
    pub ancestor_origins_secure: bool,
}

impl SecureContext {
    pub fn new(url: &str) -> Self {
        let is_secure = Self::is_potentially_trustworthy(url);
        Self {
            is_secure,
            ancestor_origins_secure: true, // Would check ancestors
        }
    }
    
    /// Check if URL is potentially trustworthy
    pub fn is_potentially_trustworthy(url: &str) -> bool {
        let url_lower = url.to_lowercase();
        
        // Secure schemes
        if url_lower.starts_with("https://") || url_lower.starts_with("wss://") {
            return true;
        }
        
        // Localhost is trustworthy
        if url_lower.contains("://localhost") || 
           url_lower.contains("://127.0.0.1") ||
           url_lower.contains("://[::1]") {
            return true;
        }
        
        // file:// is trustworthy
        if url_lower.starts_with("file://") {
            return true;
        }
        
        false
    }
    
    /// Check if context is secure
    pub fn check(&self) -> bool {
        self.is_secure && self.ancestor_origins_secure
    }
}

/// Mixed content type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixedContentType {
    /// Can be blocked or allowed (images, video, audio)
    Optionally_Blockable,
    /// Must be blocked (scripts, stylesheets, iframes)
    Blockable,
}

/// Mixed content checker
#[derive(Debug, Default)]
pub struct MixedContentChecker {
    pub strict_mode: bool,
    pub upgrade_insecure: bool,
}

impl MixedContentChecker {
    pub fn new() -> Self { Self::default() }
    
    /// Check if request should be blocked
    pub fn should_block(&self, page_url: &str, resource_url: &str, content_type: MixedContentType) -> MixedContentResult {
        // If page is not HTTPS, no blocking needed
        if !page_url.to_lowercase().starts_with("https://") {
            return MixedContentResult::Allow;
        }
        
        // If resource is HTTPS, allow
        if resource_url.to_lowercase().starts_with("https://") {
            return MixedContentResult::Allow;
        }
        
        // Upgrade if enabled
        if self.upgrade_insecure {
            return MixedContentResult::Upgrade;
        }
        
        // Block based on content type
        match content_type {
            MixedContentType::Blockable => MixedContentResult::Block,
            MixedContentType::Optionally_Blockable => {
                if self.strict_mode {
                    MixedContentResult::Block
                } else {
                    MixedContentResult::Warn
                }
            }
        }
    }
    
    /// Get mixed content type for resource
    pub fn get_content_type(resource_type: &str) -> MixedContentType {
        match resource_type.to_lowercase().as_str() {
            "script" | "stylesheet" | "iframe" | "object" | "embed" | "fetch" | "xhr" => {
                MixedContentType::Blockable
            }
            _ => MixedContentType::Optionally_Blockable,
        }
    }
}

/// Mixed content check result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MixedContentResult {
    Allow,
    Warn,
    Block,
    Upgrade,
}

/// Certificate info
#[derive(Debug, Clone)]
pub struct CertificateInfo {
    pub subject: String,
    pub issuer: String,
    pub valid_from: String,
    pub valid_to: String,
    pub is_valid: bool,
    pub is_ev: bool, // Extended Validation
}

impl Default for CertificateInfo {
    fn default() -> Self {
        Self {
            subject: String::new(),
            issuer: String::new(),
            valid_from: String::new(),
            valid_to: String::new(),
            is_valid: false,
            is_ev: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_secure_context() {
        assert!(SecureContext::is_potentially_trustworthy("https://example.com"));
        assert!(SecureContext::is_potentially_trustworthy("http://localhost"));
        assert!(!SecureContext::is_potentially_trustworthy("http://example.com"));
    }
    
    #[test]
    fn test_mixed_content() {
        let checker = MixedContentChecker::new();
        
        let result = checker.should_block(
            "https://secure.com",
            "http://insecure.com/script.js",
            MixedContentType::Blockable
        );
        
        assert_eq!(result, MixedContentResult::Block);
    }
}
