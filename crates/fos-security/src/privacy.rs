//! Privacy
//!
//! Referrer policy, tracking protection, cookie policies.

/// Referrer policy
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReferrerPolicy {
    NoReferrer,
    NoReferrerWhenDowngrade,
    Origin,
    OriginWhenCrossOrigin,
    SameOrigin,
    #[default]
    StrictOrigin,
    StrictOriginWhenCrossOrigin,
    UnsafeUrl,
}

impl ReferrerPolicy {
    /// Parse from header
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.to_lowercase().as_str() {
            "no-referrer" => Self::NoReferrer,
            "no-referrer-when-downgrade" => Self::NoReferrerWhenDowngrade,
            "origin" => Self::Origin,
            "origin-when-cross-origin" => Self::OriginWhenCrossOrigin,
            "same-origin" => Self::SameOrigin,
            "strict-origin" => Self::StrictOrigin,
            "strict-origin-when-cross-origin" => Self::StrictOriginWhenCrossOrigin,
            "unsafe-url" => Self::UnsafeUrl,
            _ => return None,
        })
    }
    
    /// Compute referrer for request
    pub fn compute_referrer(&self, source_url: &str, dest_url: &str) -> Option<String> {
        let source_secure = source_url.starts_with("https://");
        let dest_secure = dest_url.starts_with("https://");
        let same_origin = Self::same_origin(source_url, dest_url);
        
        match self {
            Self::NoReferrer => None,
            Self::NoReferrerWhenDowngrade => {
                if source_secure && !dest_secure {
                    None
                } else {
                    Some(source_url.to_string())
                }
            }
            Self::Origin => Some(Self::get_origin(source_url)),
            Self::OriginWhenCrossOrigin => {
                if same_origin {
                    Some(source_url.to_string())
                } else {
                    Some(Self::get_origin(source_url))
                }
            }
            Self::SameOrigin => {
                if same_origin {
                    Some(source_url.to_string())
                } else {
                    None
                }
            }
            Self::StrictOrigin => {
                if source_secure && !dest_secure {
                    None
                } else {
                    Some(Self::get_origin(source_url))
                }
            }
            Self::StrictOriginWhenCrossOrigin => {
                if source_secure && !dest_secure {
                    None
                } else if same_origin {
                    Some(source_url.to_string())
                } else {
                    Some(Self::get_origin(source_url))
                }
            }
            Self::UnsafeUrl => Some(source_url.to_string()),
        }
    }
    
    fn get_origin(url: &str) -> String {
        if let Some(pos) = url.find("://") {
            if let Some(path_pos) = url[pos + 3..].find('/') {
                return url[..pos + 3 + path_pos].to_string();
            }
        }
        url.to_string()
    }
    
    fn same_origin(url1: &str, url2: &str) -> bool {
        Self::get_origin(url1) == Self::get_origin(url2)
    }
}

/// Cookie policy
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CookiePolicy {
    AcceptAll,
    #[default]
    BlockThirdParty,
    BlockAll,
}

/// Tracking protection
#[derive(Debug, Clone, Default)]
pub struct TrackingProtection {
    pub enabled: bool,
    pub strict_mode: bool,
    pub blocked_domains: Vec<String>,
}

impl TrackingProtection {
    pub fn new() -> Self { Self::default() }
    
    /// Enable with default blocklist
    pub fn enable(&mut self) {
        self.enabled = true;
        self.blocked_domains = vec![
            "doubleclick.net".into(),
            "googlesyndication.com".into(),
            "facebook.com/tr".into(),
            "analytics.google.com".into(),
        ];
    }
    
    /// Check if URL should be blocked
    pub fn should_block(&self, url: &str) -> bool {
        if !self.enabled {
            return false;
        }
        
        let url_lower = url.to_lowercase();
        self.blocked_domains.iter().any(|d| url_lower.contains(d))
    }
    
    /// Add domain to blocklist
    pub fn block_domain(&mut self, domain: &str) {
        self.blocked_domains.push(domain.to_lowercase());
    }
}

/// Do Not Track header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoNotTrack {
    NotSet,
    Enabled,  // DNT: 1
    Disabled, // DNT: 0
}

impl Default for DoNotTrack {
    fn default() -> Self { Self::NotSet }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_referrer_policy() {
        let policy = ReferrerPolicy::StrictOriginWhenCrossOrigin;
        
        let referrer = policy.compute_referrer(
            "https://source.com/page",
            "https://source.com/other"
        );
        assert_eq!(referrer, Some("https://source.com/page".to_string()));
        
        let referrer = policy.compute_referrer(
            "https://source.com/page",
            "https://other.com/"
        );
        assert_eq!(referrer, Some("https://source.com".to_string()));
    }
    
    #[test]
    fn test_tracking_protection() {
        let mut tp = TrackingProtection::new();
        tp.enable();
        
        assert!(tp.should_block("https://www.doubleclick.net/ads.js"));
        assert!(!tp.should_block("https://example.com/page"));
    }
}
