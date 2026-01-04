//! fOS Security
//!
//! Security APIs for the fOS browser engine.
//!
//! Features:
//! - Same-Origin Policy and CORS
//! - Content Security Policy
//! - HTTPS and mixed content
//! - Sandbox
//! - Privacy (referrer, tracking)
//! - Subresource Integrity (SRI)
//! - Permissions Policy
//! - Trusted Types
//! - Credential Management
//! - XSS Protection
//! - Cross-Origin Isolation (COOP/COEP)

pub mod origin;
pub mod csp;
pub mod https;
pub mod sandbox;
pub mod privacy;
pub mod subresource_integrity;
pub mod permissions_policy;
pub mod trusted_types;
pub mod credential_api;
pub mod xss_protection;
pub mod coop_coep;

pub use origin::{Origin, CorsMode, CorsValidator, CorsRequest, CorsResponse};
pub use csp::{ContentSecurityPolicy, CspViolation};
pub use https::{SecureContext, MixedContentChecker, MixedContentResult};
pub use sandbox::{SandboxFlags, SandboxFlag};
pub use privacy::{ReferrerPolicy, CookiePolicy, TrackingProtection};
pub use subresource_integrity::{SriValidator, IntegrityMetadata, IntegrityAlgorithm, SriResult};
pub use permissions_policy::{PermissionsPolicy, Feature, Allowlist};
pub use trusted_types::{TrustedTypePolicyFactory, TrustedType, TrustedTypesEnforcer};
pub use credential_api::{CredentialManager, Credential, PasswordCredential};
pub use xss_protection::{Sanitizer, SanitizerConfig, XssDetector};
pub use coop_coep::{CrossOriginIsolation, CoopPolicy, CoepPolicy, IsolationEnforcer};

/// Security error
#[derive(Debug, Clone, thiserror::Error)]
pub enum SecurityError {
    #[error("Cross-origin request blocked: {0}")]
    CorsBlocked(String),
    
    #[error("CSP violation: {0}")]
    CspViolation(String),
    
    #[error("Mixed content blocked: {0}")]
    MixedContentBlocked(String),
    
    #[error("Sandbox violation: {0}")]
    SandboxViolation(String),
    
    #[error("SRI validation failed: {0}")]
    IntegrityFailed(String),
    
    #[error("Trusted types violation: {0}")]
    TrustedTypesViolation(String),
}
