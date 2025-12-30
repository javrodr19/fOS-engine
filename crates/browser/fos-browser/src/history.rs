//! History and Location Integration
//!
//! Integrates fos-js history and location APIs for browser navigation.

use fos_js::{HistoryManager, LocationManager};

/// Navigation manager for the browser
pub struct NavigationIntegration {
    /// History manager
    history: HistoryManager,
}

impl NavigationIntegration {
    /// Create new navigation manager with initial URL
    pub fn new(initial_url: &str) -> Self {
        Self {
            history: HistoryManager::new(initial_url),
        }
    }
    
    // === History API ===
    
    /// Push new state to history
    pub fn push_state(&mut self, state: Option<String>, title: &str, url: &str) {
        self.history.push_state(state, title.to_string(), url.to_string());
    }
    
    /// Replace current state
    pub fn replace_state(&mut self, state: Option<String>, title: &str, url: &str) {
        self.history.replace_state(state, title.to_string(), url.to_string());
    }
    
    /// Navigate back in history
    pub fn back(&mut self) -> Option<String> {
        self.history.back().map(|e| e.url.clone())
    }
    
    /// Navigate forward in history
    pub fn forward(&mut self) -> Option<String> {
        self.history.forward().map(|e| e.url.clone())
    }
    
    /// Navigate by delta
    pub fn go(&mut self, delta: i32) -> Option<String> {
        self.history.go(delta).map(|e| e.url.clone())
    }
    
    /// Get history length
    pub fn history_length(&self) -> usize {
        self.history.length()
    }
    
    /// Get current URL from history
    pub fn current_url(&self) -> &str {
        &self.history.current().url
    }
    
    /// Get current state
    pub fn current_state(&self) -> Option<&str> {
        self.history.current().state.as_deref()
    }
    
    // === Location API ===
    
    /// Create a location manager for a URL
    pub fn create_location(url: &str) -> Result<LocationManager, String> {
        LocationManager::new(url).map_err(|e| e.to_string())
    }
    
    /// Get statistics
    pub fn stats(&self) -> NavigationStats {
        NavigationStats {
            history_length: self.history.length(),
        }
    }
}

impl Default for NavigationIntegration {
    fn default() -> Self {
        Self::new("about:blank")
    }
}

/// Navigation statistics
#[derive(Debug, Clone)]
pub struct NavigationStats {
    pub history_length: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_navigation_creation() {
        let nav = NavigationIntegration::new("https://example.com");
        assert_eq!(nav.current_url(), "https://example.com");
    }
    
    #[test]
    fn test_push_state() {
        let mut nav = NavigationIntegration::new("https://example.com");
        nav.push_state(None, "Page 2", "/page2");
        
        assert_eq!(nav.history_length(), 2);
        assert_eq!(nav.current_url(), "/page2");
    }
    
    #[test]
    fn test_back_forward() {
        let mut nav = NavigationIntegration::new("https://example.com");
        nav.push_state(None, "", "/page1");
        nav.push_state(None, "", "/page2");
        
        nav.back();
        assert_eq!(nav.current_url(), "/page1");
        
        nav.forward();
        assert_eq!(nav.current_url(), "/page2");
    }
}
