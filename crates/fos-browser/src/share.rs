//! Web Share API
//!
//! Native sharing functionality.

/// Share data
#[derive(Debug, Clone, Default)]
pub struct ShareData {
    pub title: Option<String>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub files: Vec<ShareFile>,
}

/// File to share
#[derive(Debug, Clone)]
pub struct ShareFile {
    pub name: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// Share error
#[derive(Debug)]
pub enum ShareError {
    NotSupported,
    Canceled,
    InvalidData,
    PermissionDenied,
}

impl std::fmt::Display for ShareError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotSupported => write!(f, "Share not supported"),
            Self::Canceled => write!(f, "Share canceled"),
            Self::InvalidData => write!(f, "Invalid share data"),
            Self::PermissionDenied => write!(f, "Permission denied"),
        }
    }
}

impl std::error::Error for ShareError {}

/// Share target for receiving shared content
#[derive(Debug, Clone)]
pub struct ShareTarget {
    pub action: String,
    pub method: String,
    pub enctype: String,
    pub params: ShareTargetParams,
}

#[derive(Debug, Clone, Default)]
pub struct ShareTargetParams {
    pub title: Option<String>,
    pub text: Option<String>,
    pub url: Option<String>,
    pub files: Vec<ShareTargetFile>,
}

#[derive(Debug, Clone)]
pub struct ShareTargetFile {
    pub name: String,
    pub accept: Vec<String>,
}

/// Web Share manager
#[derive(Debug, Default)]
pub struct ShareManager {
    share_target: Option<ShareTarget>,
}

impl ShareManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if sharing is supported
    pub fn can_share(&self, data: &ShareData) -> bool {
        // Must have at least one of title, text, url, or files
        data.title.is_some() 
            || data.text.is_some() 
            || data.url.is_some() 
            || !data.files.is_empty()
    }
    
    /// Share content
    #[cfg(target_os = "linux")]
    pub fn share(&self, data: ShareData) -> Result<(), ShareError> {
        if !self.can_share(&data) {
            return Err(ShareError::InvalidData);
        }
        
        // Linux: Try xdg-open for URLs, or zenity for text
        if let Some(url) = &data.url {
            use std::process::Command;
            let _ = Command::new("xdg-open").arg(url).spawn();
            return Ok(());
        }
        
        // For text content, try to copy to clipboard
        if let Some(text) = &data.text {
            use std::process::{Command, Stdio};
            use std::io::Write;
            
            if let Ok(mut child) = Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
            }
        }
        
        Ok(())
    }
    
    #[cfg(not(target_os = "linux"))]
    pub fn share(&self, data: ShareData) -> Result<(), ShareError> {
        if !self.can_share(&data) {
            return Err(ShareError::InvalidData);
        }
        Err(ShareError::NotSupported)
    }
    
    /// Register as share target
    pub fn register_target(&mut self, target: ShareTarget) {
        self.share_target = Some(target);
    }
    
    /// Get share target
    pub fn get_target(&self) -> Option<&ShareTarget> {
        self.share_target.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_can_share() {
        let mgr = ShareManager::new();
        
        assert!(!mgr.can_share(&ShareData::default()));
        
        let data = ShareData {
            url: Some("https://example.com".to_string()),
            ..Default::default()
        };
        assert!(mgr.can_share(&data));
    }
}
