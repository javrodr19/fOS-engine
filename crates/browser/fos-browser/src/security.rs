//! Security Integration
//!
//! Integrates fos-security features: CSP, Sandbox, Privacy, Tracking Protection.

use fos_security::{
    ContentSecurityPolicy, CspViolation,
    SandboxFlags,
    ReferrerPolicy, CookiePolicy, TrackingProtection,
};

/// Security manager for the browser
pub struct SecurityManager {
    /// Content Security Policy for current page
    pub csp: Option<ContentSecurityPolicy>,
    /// Sandbox flags for current context
    pub sandbox: SandboxFlags,
    /// Referrer policy
    pub referrer_policy: ReferrerPolicy,
    /// Cookie policy
    pub cookie_policy: CookiePolicy,
    /// Tracking protection
    pub tracking: TrackingProtection,
    /// Do Not Track enabled
    pub dnt_enabled: bool,
    /// CSP violations log
    violations: Vec<CspViolation>,
}

impl SecurityManager {
    /// Create new security manager with defaults
    pub fn new() -> Self {
        let mut tracking = TrackingProtection::new();
        tracking.enable(); // Enable by default
        
        Self {
            csp: None,
            sandbox: SandboxFlags::new(),
            referrer_policy: ReferrerPolicy::default(),
            cookie_policy: CookiePolicy::default(),
            tracking,
            dnt_enabled: false,
            violations: Vec::new(),
        }
    }
    
    /// Parse CSP from response header
    pub fn parse_csp(&mut self, header: &str) {
        self.csp = Some(ContentSecurityPolicy::parse(header));
        log::debug!("Parsed CSP policy");
    }
    
    /// Parse sandbox attribute from iframe
    pub fn parse_sandbox(&mut self, attribute: &str) {
        self.sandbox = SandboxFlags::parse(attribute);
        log::debug!("Parsed sandbox flags");
    }
    
    /// Parse referrer policy from header or meta tag
    pub fn parse_referrer_policy(&mut self, value: &str) {
        if let Some(policy) = ReferrerPolicy::parse(value) {
            self.referrer_policy = policy;
            log::debug!("Set referrer policy: {:?}", policy);
        }
    }
    
    /// Check if script is allowed
    pub fn allows_script(&self, src: &str, origin: &str) -> bool {
        // Check sandbox first
        if !self.sandbox.flags.is_empty() && !self.sandbox.allows_scripts() {
            return false;
        }
        
        // Check CSP
        if let Some(ref csp) = self.csp {
            return csp.allows("script-src", src, origin);
        }
        
        true
    }
    
    /// Check if inline script is allowed
    pub fn allows_inline_script(&self) -> bool {
        // Check sandbox
        if !self.sandbox.flags.is_empty() && !self.sandbox.allows_scripts() {
            return false;
        }
        
        // Check CSP
        if let Some(ref csp) = self.csp {
            return csp.allows_inline_script();
        }
        
        true
    }
    
    /// Check if eval is allowed
    pub fn allows_eval(&self) -> bool {
        if let Some(ref csp) = self.csp {
            return csp.allows_eval();
        }
        true
    }
    
    /// Check if style is allowed
    pub fn allows_style(&self, src: &str, origin: &str) -> bool {
        if let Some(ref csp) = self.csp {
            return csp.allows("style-src", src, origin);
        }
        true
    }
    
    /// Check if image is allowed
    pub fn allows_image(&self, src: &str, origin: &str) -> bool {
        if let Some(ref csp) = self.csp {
            return csp.allows("img-src", src, origin);
        }
        true
    }
    
    /// Check if connection is allowed (XHR, WebSocket, fetch)
    pub fn allows_connect(&self, url: &str, origin: &str) -> bool {
        if let Some(ref csp) = self.csp {
            return csp.allows("connect-src", url, origin);
        }
        true
    }
    
    /// Check if media is allowed
    pub fn allows_media(&self, src: &str, origin: &str) -> bool {
        if let Some(ref csp) = self.csp {
            return csp.allows("media-src", src, origin);
        }
        true
    }
    
    /// Check if frame is allowed
    pub fn allows_frame(&self, src: &str, origin: &str) -> bool {
        if let Some(ref csp) = self.csp {
            return csp.allows("frame-src", src, origin);
        }
        true
    }
    
    /// Check if URL should be blocked by tracking protection
    pub fn is_tracker(&self, url: &str) -> bool {
        self.tracking.should_block(url)
    }
    
    /// Get referrer for request
    pub fn get_referrer(&self, source_url: &str, dest_url: &str) -> Option<String> {
        self.referrer_policy.compute_referrer(source_url, dest_url)
    }
    
    /// Check if form submission is allowed
    pub fn allows_form_submission(&self) -> bool {
        if !self.sandbox.flags.is_empty() {
            return self.sandbox.allows_forms();
        }
        true
    }
    
    /// Check if popups are allowed
    pub fn allows_popups(&self) -> bool {
        if !self.sandbox.flags.is_empty() {
            return self.sandbox.allows_popups();
        }
        true
    }
    
    /// Check if top navigation is allowed (for iframes)
    pub fn allows_top_navigation(&self) -> bool {
        if !self.sandbox.flags.is_empty() {
            return self.sandbox.allows_top_navigation();
        }
        true
    }
    
    /// Log a CSP violation
    pub fn report_violation(&mut self, violation: CspViolation) {
        log::warn!("CSP violation: {} blocked {}", 
            violation.violated_directive, violation.blocked_uri);
        self.violations.push(violation);
    }
    
    /// Get all violations
    pub fn get_violations(&self) -> &[CspViolation] {
        &self.violations
    }
    
    /// Clear violations
    pub fn clear_violations(&mut self) {
        self.violations.clear();
    }
    
    /// Reset for new page load
    pub fn reset(&mut self) {
        self.csp = None;
        self.sandbox = SandboxFlags::new();
        // Keep referrer_policy, cookie_policy, and tracking as browser-wide settings
        self.violations.clear();
    }
    
    /// Get statistics
    pub fn stats(&self) -> SecurityStats {
        SecurityStats {
            has_csp: self.csp.is_some(),
            is_sandboxed: !self.sandbox.flags.is_empty(),
            tracking_enabled: self.tracking.enabled,
            dnt_enabled: self.dnt_enabled,
            violation_count: self.violations.len(),
        }
    }
}

impl Default for SecurityManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Security statistics
#[derive(Debug, Clone)]
pub struct SecurityStats {
    pub has_csp: bool,
    pub is_sandboxed: bool,
    pub tracking_enabled: bool,
    pub dnt_enabled: bool,
    pub violation_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_security_manager_creation() {
        let manager = SecurityManager::new();
        assert!(manager.csp.is_none());
        assert!(manager.tracking.enabled);
    }
    
    #[test]
    fn test_csp_parsing() {
        let mut manager = SecurityManager::new();
        manager.parse_csp("default-src 'self'; script-src 'self' https://cdn.example.com");
        assert!(manager.csp.is_some());
        assert!(manager.allows_script("https://cdn.example.com/app.js", "https://example.com"));
    }
    
    #[test]
    fn test_sandbox_parsing() {
        let mut manager = SecurityManager::new();
        manager.parse_sandbox("allow-scripts allow-forms");
        assert!(manager.sandbox.allows_scripts());
        assert!(manager.sandbox.allows_forms());
        assert!(!manager.sandbox.allows_popups());
    }
    
    #[test]
    fn test_tracking_protection() {
        let manager = SecurityManager::new();
        assert!(manager.is_tracker("https://www.doubleclick.net/ads.js"));
        assert!(!manager.is_tracker("https://example.com/script.js"));
    }
    
    #[test]
    fn test_referrer_policy() {
        let mut manager = SecurityManager::new();
        manager.parse_referrer_policy("strict-origin");
        let referrer = manager.get_referrer("https://source.com/page", "https://dest.com/");
        assert_eq!(referrer, Some("https://source.com".to_string()));
    }
}
