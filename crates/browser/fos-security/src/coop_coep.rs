//! Cross-Origin Isolation (COOP/COEP)
//!
//! Cross-Origin-Opener-Policy and Cross-Origin-Embedder-Policy.

/// Cross-Origin-Opener-Policy value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoopPolicy {
    #[default]
    UnsafeNone,
    SameOriginAllowPopups,
    SameOrigin,
    SameOriginPlusCoep,
}

impl CoopPolicy {
    pub fn parse(header: &str) -> Self {
        match header.to_lowercase().trim() {
            "same-origin" => Self::SameOrigin,
            "same-origin-allow-popups" => Self::SameOriginAllowPopups,
            "same-origin-plus-coep" => Self::SameOriginPlusCoep,
            _ => Self::UnsafeNone,
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnsafeNone => "unsafe-none", Self::SameOriginAllowPopups => "same-origin-allow-popups",
            Self::SameOrigin => "same-origin", Self::SameOriginPlusCoep => "same-origin-plus-coep",
        }
    }
}

/// Cross-Origin-Embedder-Policy value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CoepPolicy {
    #[default]
    UnsafeNone,
    RequireCorp,
    CredentialLess,
}

impl CoepPolicy {
    pub fn parse(header: &str) -> Self {
        match header.to_lowercase().trim() {
            "require-corp" => Self::RequireCorp,
            "credentialless" => Self::CredentialLess,
            _ => Self::UnsafeNone,
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::UnsafeNone => "unsafe-none", Self::RequireCorp => "require-corp",
            Self::CredentialLess => "credentialless",
        }
    }
}

/// Cross-Origin-Resource-Policy value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CorpPolicy {
    #[default]
    None,
    SameSite,
    SameOrigin,
    CrossOrigin,
}

impl CorpPolicy {
    pub fn parse(header: &str) -> Self {
        match header.to_lowercase().trim() {
            "same-site" => Self::SameSite, "same-origin" => Self::SameOrigin,
            "cross-origin" => Self::CrossOrigin, _ => Self::None,
        }
    }
}

/// Cross-origin isolation state
#[derive(Debug, Clone, Default)]
pub struct CrossOriginIsolation {
    pub coop: CoopPolicy,
    pub coep: CoepPolicy,
    pub is_isolated: bool,
}

impl CrossOriginIsolation {
    pub fn new() -> Self { Self::default() }
    
    /// Update from response headers
    pub fn from_headers(headers: &std::collections::HashMap<String, String>) -> Self {
        let coop = headers.get("cross-origin-opener-policy").map(|s| CoopPolicy::parse(s)).unwrap_or_default();
        let coep = headers.get("cross-origin-embedder-policy").map(|s| CoepPolicy::parse(s)).unwrap_or_default();
        let is_isolated = Self::check_isolation(coop, coep);
        Self { coop, coep, is_isolated }
    }
    
    fn check_isolation(coop: CoopPolicy, coep: CoepPolicy) -> bool {
        (coop == CoopPolicy::SameOrigin || coop == CoopPolicy::SameOriginPlusCoep) &&
        (coep == CoepPolicy::RequireCorp || coep == CoepPolicy::CredentialLess)
    }
    
    /// Check if SharedArrayBuffer can be used
    pub fn can_use_shared_array_buffer(&self) -> bool { self.is_isolated }
    
    /// Check if high-resolution timers can be used
    pub fn can_use_high_res_timers(&self) -> bool { self.is_isolated }
}

/// COOP/COEP enforcer
#[derive(Debug, Default)]
pub struct IsolationEnforcer {
    document_isolation: CrossOriginIsolation,
    violations: Vec<IsolationViolation>,
}

impl IsolationEnforcer {
    pub fn new() -> Self { Self::default() }
    
    pub fn set_document_isolation(&mut self, isolation: CrossOriginIsolation) {
        self.document_isolation = isolation;
    }
    
    /// Check if a resource can be embedded
    pub fn can_embed(&mut self, resource_corp: CorpPolicy, resource_origin: &str, document_origin: &str) -> bool {
        match self.document_isolation.coep {
            CoepPolicy::UnsafeNone => true,
            CoepPolicy::RequireCorp => self.check_corp(resource_corp, resource_origin, document_origin),
            CoepPolicy::CredentialLess => true, // Credentials stripped
        }
    }
    
    fn check_corp(&mut self, corp: CorpPolicy, resource_origin: &str, doc_origin: &str) -> bool {
        match corp {
            CorpPolicy::CrossOrigin => true,
            CorpPolicy::SameOrigin => {
                if resource_origin == doc_origin { true }
                else {
                    self.violations.push(IsolationViolation::CorpBlocked { resource_origin: resource_origin.into() });
                    false
                }
            }
            CorpPolicy::SameSite => {
                let res_site = get_site(resource_origin);
                let doc_site = get_site(doc_origin);
                if res_site == doc_site { true }
                else {
                    self.violations.push(IsolationViolation::CorpBlocked { resource_origin: resource_origin.into() });
                    false
                }
            }
            CorpPolicy::None => {
                self.violations.push(IsolationViolation::MissingCorp { resource_origin: resource_origin.into() });
                false
            }
        }
    }
    
    /// Check if popup can be opened
    pub fn can_open_popup(&self, popup_origin: &str, opener_origin: &str) -> bool {
        match self.document_isolation.coop {
            CoopPolicy::UnsafeNone => true,
            CoopPolicy::SameOriginAllowPopups => true, // Popups allowed
            CoopPolicy::SameOrigin | CoopPolicy::SameOriginPlusCoep => popup_origin == opener_origin,
        }
    }
    
    pub fn get_violations(&self) -> &[IsolationViolation] { &self.violations }
    pub fn is_isolated(&self) -> bool { self.document_isolation.is_isolated }
}

/// Isolation violation
#[derive(Debug, Clone)]
pub enum IsolationViolation {
    CorpBlocked { resource_origin: String },
    MissingCorp { resource_origin: String },
    CoopMismatch { opener_origin: String },
}

fn get_site(origin: &str) -> &str {
    origin.rsplit('.').take(2).collect::<Vec<_>>().into_iter().rev()
        .collect::<Vec<_>>().join(".").leak()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_coop_parse() {
        assert_eq!(CoopPolicy::parse("same-origin"), CoopPolicy::SameOrigin);
        assert_eq!(CoopPolicy::parse("invalid"), CoopPolicy::UnsafeNone);
    }
    
    #[test]
    fn test_coep_parse() {
        assert_eq!(CoepPolicy::parse("require-corp"), CoepPolicy::RequireCorp);
        assert_eq!(CoepPolicy::parse("credentialless"), CoepPolicy::CredentialLess);
    }
    
    #[test]
    fn test_isolation() {
        let isolation = CrossOriginIsolation {
            coop: CoopPolicy::SameOrigin, coep: CoepPolicy::RequireCorp, is_isolated: true,
        };
        assert!(isolation.can_use_shared_array_buffer());
    }
}
