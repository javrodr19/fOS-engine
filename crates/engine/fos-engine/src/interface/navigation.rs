//! Navigation Interface
//!
//! Navigation state machine and events.

use std::time::{Duration, Instant};

/// Navigation state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationState {
    /// Idle, no navigation in progress
    Idle,
    /// Started navigation
    Started,
    /// Redirecting
    Redirecting,
    /// Receiving response
    Receiving,
    /// Processing (parsing, rendering)
    Processing,
    /// Complete
    Complete,
    /// Failed
    Failed,
}

/// Navigation entry
#[derive(Debug, Clone)]
pub struct NavigationEntry {
    /// Entry ID
    pub id: u32,
    /// URL
    pub url: String,
    /// Title
    pub title: String,
    /// Timestamp
    pub timestamp: Instant,
    /// State at this entry
    pub state: NavigationState,
}

impl NavigationEntry {
    pub fn new(id: u32, url: &str) -> Self {
        Self {
            id,
            url: url.to_string(),
            title: String::new(),
            timestamp: Instant::now(),
            state: NavigationState::Started,
        }
    }
}

/// Navigation timing metrics
#[derive(Debug, Clone, Copy, Default)]
pub struct NavigationTiming {
    /// Navigation start
    pub start: Option<Instant>,
    /// DNS lookup start
    pub dns_start: Option<Instant>,
    /// DNS lookup end
    pub dns_end: Option<Instant>,
    /// Connection start
    pub connect_start: Option<Instant>,
    /// Connection end (includes TLS)
    pub connect_end: Option<Instant>,
    /// Request sent
    pub request_start: Option<Instant>,
    /// Response start (first byte)
    pub response_start: Option<Instant>,
    /// Response end (last byte)
    pub response_end: Option<Instant>,
    /// DOM content loaded
    pub dom_content_loaded: Option<Instant>,
    /// Load complete
    pub load_complete: Option<Instant>,
}

impl NavigationTiming {
    pub fn new() -> Self {
        Self {
            start: Some(Instant::now()),
            ..Default::default()
        }
    }
    
    /// Time to first byte
    pub fn ttfb(&self) -> Option<Duration> {
        match (self.start, self.response_start) {
            (Some(start), Some(response)) => Some(response.duration_since(start)),
            _ => None,
        }
    }
    
    /// DNS lookup duration
    pub fn dns_duration(&self) -> Option<Duration> {
        match (self.dns_start, self.dns_end) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            _ => None,
        }
    }
    
    /// Connection duration
    pub fn connect_duration(&self) -> Option<Duration> {
        match (self.connect_start, self.connect_end) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            _ => None,
        }
    }
    
    /// Total page load time
    pub fn total_duration(&self) -> Option<Duration> {
        match (self.start, self.load_complete) {
            (Some(start), Some(end)) => Some(end.duration_since(start)),
            _ => None,
        }
    }
    
    /// DOM interactive time
    pub fn dom_interactive(&self) -> Option<Duration> {
        match (self.start, self.dom_content_loaded) {
            (Some(start), Some(dcl)) => Some(dcl.duration_since(start)),
            _ => None,
        }
    }
}

/// Navigation controller
#[derive(Debug)]
pub struct NavigationController {
    /// Navigation history
    history: Vec<NavigationEntry>,
    /// Current index in history
    current_index: i32,
    /// Next entry ID
    next_id: u32,
    /// Current navigation timing
    current_timing: Option<NavigationTiming>,
    /// Pending navigation URL
    pending_url: Option<String>,
}

impl NavigationController {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            current_index: -1,
            next_id: 1,
            current_timing: None,
            pending_url: None,
        }
    }
    
    /// Start navigation to URL
    pub fn navigate(&mut self, url: &str) -> u32 {
        // Truncate forward history
        if self.current_index >= 0 && (self.current_index as usize) < self.history.len() - 1 {
            self.history.truncate(self.current_index as usize + 1);
        }
        
        let id = self.next_id;
        self.next_id += 1;
        
        let entry = NavigationEntry::new(id, url);
        self.history.push(entry);
        self.current_index = self.history.len() as i32 - 1;
        self.current_timing = Some(NavigationTiming::new());
        self.pending_url = Some(url.to_string());
        
        id
    }
    
    /// Mark navigation as committed
    pub fn commit(&mut self, url: &str, title: &str) {
        if let Some(entry) = self.current_entry_mut() {
            entry.url = url.to_string();
            entry.title = title.to_string();
            entry.state = NavigationState::Processing;
        }
        
        if let Some(ref mut timing) = self.current_timing {
            timing.response_start = Some(Instant::now());
        }
        
        self.pending_url = None;
    }
    
    /// Mark DOM content loaded
    pub fn on_dom_content_loaded(&mut self) {
        if let Some(ref mut timing) = self.current_timing {
            timing.dom_content_loaded = Some(Instant::now());
        }
    }
    
    /// Mark fully loaded
    pub fn on_load_complete(&mut self) {
        if let Some(entry) = self.current_entry_mut() {
            entry.state = NavigationState::Complete;
        }
        
        if let Some(ref mut timing) = self.current_timing {
            timing.load_complete = Some(Instant::now());
        }
    }
    
    /// Mark failed
    pub fn on_failed(&mut self) {
        if let Some(entry) = self.current_entry_mut() {
            entry.state = NavigationState::Failed;
        }
        self.pending_url = None;
    }
    
    /// Go back
    pub fn go_back(&mut self) -> Option<&NavigationEntry> {
        if self.can_go_back() {
            self.current_index -= 1;
            self.current_timing = Some(NavigationTiming::new());
            self.current_entry()
        } else {
            None
        }
    }
    
    /// Go forward
    pub fn go_forward(&mut self) -> Option<&NavigationEntry> {
        if self.can_go_forward() {
            self.current_index += 1;
            self.current_timing = Some(NavigationTiming::new());
            self.current_entry()
        } else {
            None
        }
    }
    
    /// Can go back?
    pub fn can_go_back(&self) -> bool {
        self.current_index > 0
    }
    
    /// Can go forward?
    pub fn can_go_forward(&self) -> bool {
        self.current_index >= 0 && (self.current_index as usize) < self.history.len() - 1
    }
    
    /// Get current entry
    pub fn current_entry(&self) -> Option<&NavigationEntry> {
        if self.current_index >= 0 {
            self.history.get(self.current_index as usize)
        } else {
            None
        }
    }
    
    /// Get current entry (mutable)
    fn current_entry_mut(&mut self) -> Option<&mut NavigationEntry> {
        if self.current_index >= 0 {
            self.history.get_mut(self.current_index as usize)
        } else {
            None
        }
    }
    
    /// Get current URL
    pub fn current_url(&self) -> Option<&str> {
        self.current_entry().map(|e| e.url.as_str())
    }
    
    /// Get current title
    pub fn current_title(&self) -> Option<&str> {
        self.current_entry().map(|e| e.title.as_str())
    }
    
    /// Get current timing
    pub fn timing(&self) -> Option<&NavigationTiming> {
        self.current_timing.as_ref()
    }
    
    /// Get history length
    pub fn history_length(&self) -> usize {
        self.history.len()
    }
    
    /// Get pending URL
    pub fn pending_url(&self) -> Option<&str> {
        self.pending_url.as_deref()
    }
}

impl Default for NavigationController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_navigation() {
        let mut nav = NavigationController::new();
        
        nav.navigate("https://example.com");
        assert_eq!(nav.current_url(), Some("https://example.com"));
        assert!(!nav.can_go_back());
        
        nav.navigate("https://example.com/page1");
        assert!(nav.can_go_back());
        assert!(!nav.can_go_forward());
    }
    
    #[test]
    fn test_back_forward() {
        let mut nav = NavigationController::new();
        
        nav.navigate("https://example.com");
        nav.navigate("https://example.com/page1");
        nav.navigate("https://example.com/page2");
        
        nav.go_back();
        assert_eq!(nav.current_url(), Some("https://example.com/page1"));
        assert!(nav.can_go_forward());
        
        nav.go_forward();
        assert_eq!(nav.current_url(), Some("https://example.com/page2"));
    }
    
    #[test]
    fn test_timing() {
        let mut nav = NavigationController::new();
        
        nav.navigate("https://example.com");
        assert!(nav.timing().is_some());
        
        std::thread::sleep(std::time::Duration::from_millis(10));
        nav.on_dom_content_loaded();
        
        let timing = nav.timing().unwrap();
        assert!(timing.dom_interactive().is_some());
    }
}
