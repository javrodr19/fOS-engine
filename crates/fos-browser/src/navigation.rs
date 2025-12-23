//! Navigation Module
//!
//! URL handling, history, and navigation.

use fos_engine::url::{Url, ParseError};
use std::collections::VecDeque;

/// Navigation history
#[derive(Debug)]
pub struct History {
    /// Back stack
    back: VecDeque<String>,
    /// Forward stack
    forward: Vec<String>,
    /// Current URL
    current: Option<String>,
    /// Max history size
    max_size: usize,
}

impl History {
    /// Create new history
    pub fn new() -> Self {
        Self {
            back: VecDeque::new(),
            forward: Vec::new(),
            current: None,
            max_size: 100,
        }
    }
    
    /// Navigate to a new URL
    pub fn navigate(&mut self, url: &str) {
        // Push current to back stack
        if let Some(current) = self.current.take() {
            self.back.push_back(current);
            
            // Limit size
            while self.back.len() > self.max_size {
                self.back.pop_front();
            }
        }
        
        // Clear forward stack
        self.forward.clear();
        
        // Set new current
        self.current = Some(url.to_string());
    }
    
    /// Go back
    pub fn go_back(&mut self) -> Option<String> {
        let prev = self.back.pop_back()?;
        
        // Push current to forward
        if let Some(current) = self.current.take() {
            self.forward.push(current);
        }
        
        self.current = Some(prev.clone());
        Some(prev)
    }
    
    /// Go forward
    pub fn go_forward(&mut self) -> Option<String> {
        let next = self.forward.pop()?;
        
        // Push current to back
        if let Some(current) = self.current.take() {
            self.back.push_back(current);
        }
        
        self.current = Some(next.clone());
        Some(next)
    }
    
    /// Can go back
    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }
    
    /// Can go forward
    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    
    /// Get current URL
    pub fn current(&self) -> Option<&str> {
        self.current.as_deref()
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize and validate a URL
pub fn normalize_url(input: &str) -> Result<String, UrlError> {
    let input = input.trim();
    
    // Handle about: URLs
    if input.starts_with("about:") {
        return Ok(input.to_string());
    }
    
    // Handle fos: URLs
    if input.starts_with("fos:") {
        return Ok(input.to_string());
    }
    
    // Add https:// if no scheme
    let with_scheme = if !input.contains("://") {
        format!("https://{}", input)
    } else {
        input.to_string()
    };
    
    // Parse and validate
    let url = Url::parse(&with_scheme).map_err(|e| UrlError::Parse(e.to_string()))?;
    
    // Only allow http/https/about/fos
    match url.scheme() {
        "http" | "https" | "about" | "fos" => Ok(url.to_string()),
        scheme => Err(UrlError::UnsupportedScheme(scheme.to_string())),
    }
}

/// Resolve a relative URL against a base
pub fn resolve_url(base: &str, relative: &str) -> Result<String, UrlError> {
    let base_url = Url::parse(base).map_err(|e| UrlError::Parse(e.to_string()))?;
    let resolved = base_url.join(relative).map_err(|e| UrlError::Parse(e.to_string()))?;
    Ok(resolved.to_string())
}

/// Extract domain from URL
pub fn extract_domain(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    parsed.host_str().map(|s| s.to_string())
}

/// URL error
#[derive(Debug)]
pub enum UrlError {
    Parse(String),
    UnsupportedScheme(String),
}

impl std::fmt::Display for UrlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UrlError::Parse(e) => write!(f, "URL parse error: {}", e),
            UrlError::UnsupportedScheme(s) => write!(f, "Unsupported scheme: {}", s),
        }
    }
}

impl std::error::Error for UrlError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_normalize_url() {
        assert_eq!(
            normalize_url("example.com").unwrap(),
            "https://example.com/"
        );
        assert_eq!(
            normalize_url("http://example.com").unwrap(),
            "http://example.com/"
        );
        assert_eq!(
            normalize_url("about:blank").unwrap(),
            "about:blank"
        );
    }
    
    #[test]
    fn test_history() {
        let mut history = History::new();
        
        history.navigate("https://a.com");
        history.navigate("https://b.com");
        history.navigate("https://c.com");
        
        assert_eq!(history.current(), Some("https://c.com"));
        assert!(history.can_go_back());
        
        let prev = history.go_back();
        assert_eq!(prev, Some("https://b.com".to_string()));
        assert!(history.can_go_forward());
        
        let next = history.go_forward();
        assert_eq!(next, Some("https://c.com".to_string()));
    }
}
