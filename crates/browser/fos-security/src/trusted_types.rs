//! Trusted Types
//!
//! DOM sink protection against XSS.

use std::collections::HashMap;

/// Trusted type
#[derive(Debug, Clone)]
pub enum TrustedType {
    Html(String),
    Script(String),
    ScriptUrl(String),
}

impl TrustedType {
    pub fn as_string(&self) -> &str {
        match self { Self::Html(s) | Self::Script(s) | Self::ScriptUrl(s) => s }
    }
}

/// Trusted type policy
pub struct TrustedTypePolicy {
    pub name: String,
}

// Manual impl for Debug since closures don't implement it
impl std::fmt::Debug for TrustedTypePolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrustedTypePolicy").field("name", &self.name).finish()
    }
}

/// Policy options
#[derive(Debug, Clone, Default)]
pub struct PolicyOptions {
    pub create_html: Option<fn(&str, &[String]) -> String>,
    pub create_script: Option<fn(&str, &[String]) -> String>,
    pub create_script_url: Option<fn(&str, &[String]) -> String>,
}

/// Trusted types factory
#[derive(Debug, Default)]
pub struct TrustedTypePolicyFactory {
    policies: HashMap<String, PolicyOptions>,
    default_policy: Option<String>,
    enabled: bool,
}

impl TrustedTypePolicyFactory {
    pub fn new() -> Self { Self { enabled: true, ..Default::default() } }
    
    pub fn create_policy(&mut self, name: &str, options: PolicyOptions) -> Result<(), TrustedTypeError> {
        if self.policies.contains_key(name) {
            return Err(TrustedTypeError::PolicyExists(name.into()));
        }
        self.policies.insert(name.into(), options);
        Ok(())
    }
    
    pub fn get_policy(&self, name: &str) -> Option<&PolicyOptions> {
        self.policies.get(name)
    }
    
    pub fn set_default_policy(&mut self, name: &str) -> Result<(), TrustedTypeError> {
        if !self.policies.contains_key(name) {
            return Err(TrustedTypeError::PolicyNotFound(name.into()));
        }
        self.default_policy = Some(name.into());
        Ok(())
    }
    
    pub fn create_html(&self, policy_name: &str, input: &str, args: &[String]) -> Result<TrustedType, TrustedTypeError> {
        let policy = self.policies.get(policy_name).ok_or_else(|| TrustedTypeError::PolicyNotFound(policy_name.into()))?;
        let func = policy.create_html.ok_or(TrustedTypeError::NoCreateFunction)?;
        Ok(TrustedType::Html(func(input, args)))
    }
    
    pub fn create_script(&self, policy_name: &str, input: &str, args: &[String]) -> Result<TrustedType, TrustedTypeError> {
        let policy = self.policies.get(policy_name).ok_or_else(|| TrustedTypeError::PolicyNotFound(policy_name.into()))?;
        let func = policy.create_script.ok_or(TrustedTypeError::NoCreateFunction)?;
        Ok(TrustedType::Script(func(input, args)))
    }
    
    pub fn is_html(&self, value: &TrustedType) -> bool { matches!(value, TrustedType::Html(_)) }
    pub fn is_script(&self, value: &TrustedType) -> bool { matches!(value, TrustedType::Script(_)) }
    pub fn is_script_url(&self, value: &TrustedType) -> bool { matches!(value, TrustedType::ScriptUrl(_)) }
}

/// Trusted type error
#[derive(Debug, Clone, thiserror::Error)]
pub enum TrustedTypeError {
    #[error("Policy '{0}' already exists")]
    PolicyExists(String),
    #[error("Policy '{0}' not found")]
    PolicyNotFound(String),
    #[error("No create function defined")]
    NoCreateFunction,
    #[error("Value rejected by policy")]
    Rejected,
}

/// DOM sink types that require trusted types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomSink {
    InnerHtml, OuterHtml, InsertAdjacentHtml,
    ScriptText, ScriptSrc,
    IframeSrc, IframeSrcdoc,
    EvalScript, SetTimeout, SetInterval,
    DocumentWrite, DocumentWriteLn,
}

impl DomSink {
    pub fn required_type(&self) -> TrustedTypeKind {
        match self {
            Self::InnerHtml | Self::OuterHtml | Self::InsertAdjacentHtml |
            Self::IframeSrcdoc | Self::DocumentWrite | Self::DocumentWriteLn => TrustedTypeKind::Html,
            Self::ScriptText | Self::EvalScript | Self::SetTimeout | Self::SetInterval => TrustedTypeKind::Script,
            Self::ScriptSrc | Self::IframeSrc => TrustedTypeKind::ScriptUrl,
        }
    }
}

/// Trusted type kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustedTypeKind { Html, Script, ScriptUrl }

/// Enforcement mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnforcementMode { #[default] Report, Enforce }

/// Trusted types enforcer
#[derive(Debug, Default)]
pub struct TrustedTypesEnforcer {
    factory: TrustedTypePolicyFactory,
    mode: EnforcementMode,
    violations: Vec<TrustedTypeViolation>,
}

impl TrustedTypesEnforcer {
    pub fn new() -> Self { Self::default() }
    pub fn set_mode(&mut self, mode: EnforcementMode) { self.mode = mode; }
    pub fn factory(&mut self) -> &mut TrustedTypePolicyFactory { &mut self.factory }
    
    pub fn check_sink(&mut self, sink: DomSink, value: &str) -> Result<(), TrustedTypeError> {
        // In enforce mode, reject raw strings for sensitive sinks
        if self.mode == EnforcementMode::Enforce {
            self.violations.push(TrustedTypeViolation { sink, value: value.into() });
            return Err(TrustedTypeError::Rejected);
        }
        // Report mode - log violation but allow
        self.violations.push(TrustedTypeViolation { sink, value: value.into() });
        Ok(())
    }
    
    pub fn get_violations(&self) -> &[TrustedTypeViolation] { &self.violations }
    pub fn clear_violations(&mut self) { self.violations.clear(); }
}

/// Violation record
#[derive(Debug, Clone)]
pub struct TrustedTypeViolation {
    pub sink: DomSink,
    pub value: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_policy_creation() {
        let mut factory = TrustedTypePolicyFactory::new();
        let options = PolicyOptions { create_html: Some(|s, _| s.to_uppercase()), ..Default::default() };
        assert!(factory.create_policy("test", options).is_ok());
        assert!(factory.create_policy("test", PolicyOptions::default()).is_err());
    }
    
    #[test]
    fn test_sink_type() {
        assert_eq!(DomSink::InnerHtml.required_type(), TrustedTypeKind::Html);
        assert_eq!(DomSink::ScriptSrc.required_type(), TrustedTypeKind::ScriptUrl);
    }
}
