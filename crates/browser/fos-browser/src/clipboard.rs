//! Clipboard API integration
//!
//! Read/write to system clipboard.

#[cfg(target_os = "linux")]
use std::process::{Command, Stdio};

/// Clipboard manager
#[derive(Debug, Default)]
pub struct Clipboard;

impl Clipboard {
    pub fn new() -> Self {
        Self
    }
    
    /// Read text from clipboard
    pub fn read_text(&self) -> Option<String> {
        #[cfg(target_os = "linux")]
        {
            // Try xclip first, then xsel
            let output = Command::new("xclip")
                .args(["-selection", "clipboard", "-o"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .output()
                .or_else(|_| {
                    Command::new("xsel")
                        .args(["--clipboard", "--output"])
                        .stdout(Stdio::piped())
                        .stderr(Stdio::null())
                        .output()
                })
                .ok()?;
            
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }
    
    /// Write text to clipboard
    pub fn write_text(&self, text: &str) -> bool {
        #[cfg(target_os = "linux")]
        {
            use std::io::Write;
            
            // Try xclip first, then xsel
            let result = Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .or_else(|_| {
                    Command::new("xsel")
                        .args(["--clipboard", "--input"])
                        .stdin(Stdio::piped())
                        .stderr(Stdio::null())
                        .spawn()
                });
            
            if let Ok(mut child) = result {
                if let Some(stdin) = child.stdin.as_mut() {
                    if stdin.write_all(text.as_bytes()).is_ok() {
                        return child.wait().map(|s| s.success()).unwrap_or(false);
                    }
                }
            }
            false
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }
    
    /// Check if clipboard is available
    pub fn is_available(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            Command::new("which")
                .arg("xclip")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
            ||
            Command::new("which")
                .arg("xsel")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        }
        
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_clipboard_available() {
        let clipboard = Clipboard::new();
        // Just check that the method doesn't panic
        let _ = clipboard.is_available();
    }
}
