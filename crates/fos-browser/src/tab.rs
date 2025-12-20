//! Tab Management
//!
//! Handles multiple browser tabs.

use crate::page::Page;
use std::collections::HashMap;

/// Tab ID type
pub type TabId = u32;

/// A browser tab
#[derive(Debug)]
pub struct Tab {
    /// Unique ID
    pub id: TabId,
    /// Current URL
    pub url: String,
    /// Page title
    pub title: String,
    /// Page content
    pub page: Option<Page>,
    /// Loading state
    pub loading: bool,
    /// Favicon (if loaded)
    pub favicon: Option<Vec<u8>>,
}

impl Tab {
    /// Create a new tab
    pub fn new(id: TabId, url: &str) -> Self {
        Self {
            id,
            url: url.to_string(),
            title: "New Tab".to_string(),
            page: None,
            loading: false,
            favicon: None,
        }
    }
    
    /// Navigate to URL
    pub fn navigate(&mut self, url: &str) {
        self.url = url.to_string();
        self.loading = true;
        self.title = "Loading...".to_string();
        // Page loading happens async
    }
    
    /// Set page content after loading
    pub fn set_page(&mut self, page: Page) {
        self.title = page.title.clone().unwrap_or_else(|| self.url.clone());
        self.page = Some(page);
        self.loading = false;
    }
    
    /// Get display title (truncated)
    pub fn display_title(&self, max_chars: usize) -> String {
        if self.title.len() > max_chars {
            format!("{}...", &self.title[..max_chars.saturating_sub(3)])
        } else {
            self.title.clone()
        }
    }
}

/// Tab manager
#[derive(Debug)]
pub struct TabManager {
    /// All tabs
    tabs: HashMap<TabId, Tab>,
    /// Tab order
    order: Vec<TabId>,
    /// Active tab ID
    active: Option<TabId>,
    /// Next tab ID
    next_id: TabId,
}

impl TabManager {
    /// Create a new tab manager
    pub fn new() -> Self {
        Self {
            tabs: HashMap::new(),
            order: Vec::new(),
            active: None,
            next_id: 1,
        }
    }
    
    /// Create a new tab
    pub fn new_tab(&mut self, url: &str) -> TabId {
        let id = self.next_id;
        self.next_id += 1;
        
        let mut tab = Tab::new(id, url);
        if url != "about:blank" {
            tab.navigate(url);
        }
        
        self.tabs.insert(id, tab);
        self.order.push(id);
        self.active = Some(id);
        
        id
    }
    
    /// Close a tab
    pub fn close_tab(&mut self, id: TabId) {
        self.tabs.remove(&id);
        self.order.retain(|&i| i != id);
        
        // Update active tab
        if self.active == Some(id) {
            self.active = self.order.last().copied();
        }
    }
    
    /// Close the active tab
    pub fn close_active_tab(&mut self) {
        if let Some(id) = self.active {
            self.close_tab(id);
        }
    }
    
    /// Set active tab
    pub fn set_active(&mut self, id: TabId) {
        if self.tabs.contains_key(&id) {
            self.active = Some(id);
        }
    }
    
    /// Get active tab
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tabs.get(&id))
    }
    
    /// Get active tab mutable
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active.and_then(|id| self.tabs.get_mut(&id))
    }
    
    /// Get all tabs in order
    pub fn tabs_in_order(&self) -> Vec<&Tab> {
        self.order.iter()
            .filter_map(|id| self.tabs.get(id))
            .collect()
    }
    
    /// Reload active tab
    pub fn reload_active(&mut self) {
        if let Some(tab) = self.active_tab_mut() {
            let url = tab.url.clone();
            tab.navigate(&url);
        }
    }
    
    /// Navigate active tab
    pub fn navigate_active(&mut self, url: &str) {
        if let Some(tab) = self.active_tab_mut() {
            tab.navigate(url);
        }
    }
    
    /// Get tab count
    pub fn count(&self) -> usize {
        self.tabs.len()
    }
    
    /// Check if a tab is active
    pub fn is_active(&self, id: TabId) -> bool {
        self.active == Some(id)
    }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}
