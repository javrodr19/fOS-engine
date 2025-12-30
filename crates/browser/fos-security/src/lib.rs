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

pub mod origin;
pub mod csp;
pub mod https;
pub mod sandbox;
pub mod privacy;

pub use origin::{Origin, CorsMode, CorsValidator, CorsRequest, CorsResponse};
pub use csp::{ContentSecurityPolicy, CspViolation};
pub use https::{SecureContext, MixedContentChecker, MixedContentResult};
pub use sandbox::{SandboxFlags, SandboxFlag};
pub use privacy::{ReferrerPolicy, CookiePolicy, TrackingProtection};

/// Security error
#[derive(Debug, thiserror::Error)]
pub enum SecurityError {
    #[error("Cross-origin request blocked: {0}")]
    CorsBlocked(String),
    
    #[error("CSP violation: {0}")]
    CspViolation(String),
    
    #[error("Mixed content blocked: {0}")]
    MixedContentBlocked(String),
    
    #[error("Sandbox violation: {0}")]
    SandboxViolation(String),
}
