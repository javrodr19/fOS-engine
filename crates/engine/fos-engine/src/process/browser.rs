//! Browser Process
//!
//! Main browser process responsible for UI, navigation, and coordination.

use std::collections::HashMap;
use std::time::Instant;

use super::{ProcessId, ProcessState, TabId};

/// Browser process - controls UI and coordinates child processes
#[derive(Debug)]
pub struct BrowserProcess {
    /// Process ID (always the main process)
    id: ProcessId,
    /// Current state
    state: ProcessState,
    /// Active tabs managed by this browser
    tabs: HashMap<TabId, TabInfo>,
    /// Currently focused tab
    focused_tab: Option<TabId>,
    /// Start time
    start_time: Instant,
    /// Next tab ID
    next_tab_id: u32,
}

/// Information about a tab
#[derive(Debug, Clone)]
pub struct TabInfo {
    /// Tab ID
    pub id: TabId,
    /// Current URL
    pub url: String,
    /// Page title
    pub title: String,
    /// Is loading
    pub loading: bool,
    /// Is pinned
    pub pinned: bool,
    /// Is muted
    pub muted: bool,
}

impl TabInfo {
    pub fn new(id: TabId) -> Self {
        Self {
            id,
            url: String::new(),
            title: String::from("New Tab"),
            loading: false,
            pinned: false,
            muted: false,
        }
    }
}

impl BrowserProcess {
    /// Create a new browser process
    pub fn new() -> Self {
        Self {
            id: ProcessId::new(std::process::id()),
            state: ProcessState::Running,
            tabs: HashMap::new(),
            focused_tab: None,
            start_time: Instant::now(),
            next_tab_id: 1,
        }
    }
    
    /// Get process ID
    pub fn id(&self) -> ProcessId {
        self.id
    }
    
    /// Get current state
    pub fn state(&self) -> ProcessState {
        self.state
    }
    
    /// Get uptime
    pub fn uptime(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }
    
    /// Create a new tab
    pub fn create_tab(&mut self) -> TabId {
        let id = TabId::new(self.next_tab_id);
        self.next_tab_id += 1;
        
        let info = TabInfo::new(id);
        self.tabs.insert(id, info);
        
        // Focus new tab if none focused
        if self.focused_tab.is_none() {
            self.focused_tab = Some(id);
        }
        
        id
    }
    
    /// Close a tab
    pub fn close_tab(&mut self, id: TabId) -> bool {
        if self.tabs.remove(&id).is_some() {
            // Update focus if needed
            if self.focused_tab == Some(id) {
                self.focused_tab = self.tabs.keys().next().copied();
            }
            true
        } else {
            false
        }
    }
    
    /// Get tab info
    pub fn get_tab(&self, id: TabId) -> Option<&TabInfo> {
        self.tabs.get(&id)
    }
    
    /// Get mutable tab info
    pub fn get_tab_mut(&mut self, id: TabId) -> Option<&mut TabInfo> {
        self.tabs.get_mut(&id)
    }
    
    /// Get focused tab
    pub fn focused_tab(&self) -> Option<TabId> {
        self.focused_tab
    }
    
    /// Set focused tab
    pub fn set_focused_tab(&mut self, id: TabId) -> bool {
        if self.tabs.contains_key(&id) {
            self.focused_tab = Some(id);
            true
        } else {
            false
        }
    }
    
    /// Get all tabs
    pub fn tabs(&self) -> impl Iterator<Item = &TabInfo> {
        self.tabs.values()
    }
    
    /// Tab count
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }
    
    /// Navigate a tab to URL
    pub fn navigate(&mut self, id: TabId, url: &str) {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.url = url.to_string();
            tab.loading = true;
        }
    }
    
    /// Update tab title
    pub fn set_title(&mut self, id: TabId, title: &str) {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.title = title.to_string();
        }
    }
    
    /// Mark tab as loaded
    pub fn set_loaded(&mut self, id: TabId) {
        if let Some(tab) = self.tabs.get_mut(&id) {
            tab.loading = false;
        }
    }
    
    /// Begin shutdown
    pub fn shutdown(&mut self) {
        self.state = ProcessState::ShuttingDown;
    }
}

impl Default for BrowserProcess {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_tabs() {
        let mut browser = BrowserProcess::new();
        
        let tab1 = browser.create_tab();
        let tab2 = browser.create_tab();
        
        assert_eq!(browser.tab_count(), 2);
        assert_ne!(tab1, tab2);
        assert_eq!(browser.focused_tab(), Some(tab1));
    }
    
    #[test]
    fn test_close_tab() {
        let mut browser = BrowserProcess::new();
        
        let tab1 = browser.create_tab();
        let tab2 = browser.create_tab();
        
        browser.close_tab(tab1);
        assert_eq!(browser.tab_count(), 1);
        assert_eq!(browser.focused_tab(), Some(tab2));
    }
    
    #[test]
    fn test_navigate() {
        let mut browser = BrowserProcess::new();
        let tab = browser.create_tab();
        
        browser.navigate(tab, "https://example.com");
        
        let info = browser.get_tab(tab).unwrap();
        assert_eq!(info.url, "https://example.com");
        assert!(info.loading);
    }
}
