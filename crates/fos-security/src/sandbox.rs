//! Sandbox
//!
//! iframe sandbox and permission restrictions.

use std::collections::HashSet;

/// Sandbox flags
#[derive(Debug, Clone, Default)]
pub struct SandboxFlags {
    pub flags: HashSet<SandboxFlag>,
}

/// Individual sandbox flag
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SandboxFlag {
    AllowForms,
    AllowModals,
    AllowOrientationLock,
    AllowPointerLock,
    AllowPopups,
    AllowPopupsToEscapeSandbox,
    AllowPresentation,
    AllowSameOrigin,
    AllowScripts,
    AllowTopNavigation,
    AllowTopNavigationByUserActivation,
    AllowDownloads,
}

impl SandboxFlags {
    /// Create new sandbox (fully restricted)
    pub fn new() -> Self { Self::default() }
    
    /// Parse sandbox attribute
    pub fn parse(attribute: &str) -> Self {
        let mut flags = Self::new();
        
        for token in attribute.split_whitespace() {
            if let Some(flag) = Self::parse_flag(token) {
                flags.flags.insert(flag);
            }
        }
        
        flags
    }
    
    fn parse_flag(token: &str) -> Option<SandboxFlag> {
        Some(match token.to_lowercase().as_str() {
            "allow-forms" => SandboxFlag::AllowForms,
            "allow-modals" => SandboxFlag::AllowModals,
            "allow-orientation-lock" => SandboxFlag::AllowOrientationLock,
            "allow-pointer-lock" => SandboxFlag::AllowPointerLock,
            "allow-popups" => SandboxFlag::AllowPopups,
            "allow-popups-to-escape-sandbox" => SandboxFlag::AllowPopupsToEscapeSandbox,
            "allow-presentation" => SandboxFlag::AllowPresentation,
            "allow-same-origin" => SandboxFlag::AllowSameOrigin,
            "allow-scripts" => SandboxFlag::AllowScripts,
            "allow-top-navigation" => SandboxFlag::AllowTopNavigation,
            "allow-top-navigation-by-user-activation" => SandboxFlag::AllowTopNavigationByUserActivation,
            "allow-downloads" => SandboxFlag::AllowDownloads,
            _ => return None,
        })
    }
    
    /// Check if flag is set
    pub fn has(&self, flag: SandboxFlag) -> bool {
        self.flags.contains(&flag)
    }
    
    /// Allow scripts
    pub fn allows_scripts(&self) -> bool {
        self.has(SandboxFlag::AllowScripts)
    }
    
    /// Allow forms
    pub fn allows_forms(&self) -> bool {
        self.has(SandboxFlag::AllowForms)
    }
    
    /// Allow same origin
    pub fn allows_same_origin(&self) -> bool {
        self.has(SandboxFlag::AllowSameOrigin)
    }
    
    /// Allow popups
    pub fn allows_popups(&self) -> bool {
        self.has(SandboxFlag::AllowPopups)
    }
    
    /// Allow top navigation
    pub fn allows_top_navigation(&self) -> bool {
        self.has(SandboxFlag::AllowTopNavigation)
    }
    
    /// Serialize to attribute
    pub fn serialize(&self) -> String {
        self.flags.iter()
            .map(|f| match f {
                SandboxFlag::AllowForms => "allow-forms",
                SandboxFlag::AllowModals => "allow-modals",
                SandboxFlag::AllowOrientationLock => "allow-orientation-lock",
                SandboxFlag::AllowPointerLock => "allow-pointer-lock",
                SandboxFlag::AllowPopups => "allow-popups",
                SandboxFlag::AllowPopupsToEscapeSandbox => "allow-popups-to-escape-sandbox",
                SandboxFlag::AllowPresentation => "allow-presentation",
                SandboxFlag::AllowSameOrigin => "allow-same-origin",
                SandboxFlag::AllowScripts => "allow-scripts",
                SandboxFlag::AllowTopNavigation => "allow-top-navigation",
                SandboxFlag::AllowTopNavigationByUserActivation => "allow-top-navigation-by-user-activation",
                SandboxFlag::AllowDownloads => "allow-downloads",
            })
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Sandboxed browsing context
#[derive(Debug)]
pub struct SandboxedContext {
    pub flags: SandboxFlags,
    pub origin: Option<super::origin::Origin>,
}

impl SandboxedContext {
    pub fn new(flags: SandboxFlags) -> Self {
        Self { flags, origin: None }
    }
    
    /// Check if navigation allowed
    pub fn can_navigate_to(&self, _target: &str) -> bool {
        self.flags.allows_top_navigation()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_sandbox() {
        let flags = SandboxFlags::parse("allow-scripts allow-same-origin");
        
        assert!(flags.allows_scripts());
        assert!(flags.allows_same_origin());
        assert!(!flags.allows_forms());
    }
    
    #[test]
    fn test_empty_sandbox() {
        let flags = SandboxFlags::new();
        
        assert!(!flags.allows_scripts());
        assert!(!flags.allows_forms());
    }
}
