//! Page Visibility API
//!
//! Track document visibility state.

use std::time::Instant;

/// Visibility state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VisibilityState {
    #[default]
    Visible,
    Hidden,
    Prerender,
}

/// Document visibility
#[derive(Debug)]
pub struct DocumentVisibility {
    state: VisibilityState,
    hidden: bool,
    last_change: Option<Instant>,
    hidden_duration_ms: u64,
}

impl Default for DocumentVisibility {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentVisibility {
    pub fn new() -> Self {
        Self {
            state: VisibilityState::Visible,
            hidden: false,
            last_change: None,
            hidden_duration_ms: 0,
        }
    }
    
    /// Get current visibility state
    pub fn visibility_state(&self) -> VisibilityState {
        self.state
    }
    
    /// Check if document is hidden
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
    
    /// Set visibility state
    pub fn set_state(&mut self, state: VisibilityState) -> bool {
        if self.state == state {
            return false;
        }
        
        let now = Instant::now();
        
        // Track hidden duration
        if self.state == VisibilityState::Hidden {
            if let Some(last) = self.last_change {
                self.hidden_duration_ms += last.elapsed().as_millis() as u64;
            }
        }
        
        self.state = state;
        self.hidden = state == VisibilityState::Hidden;
        self.last_change = Some(now);
        
        true // Changed
    }
    
    /// Get total hidden duration
    pub fn hidden_duration(&self) -> u64 {
        let mut duration = self.hidden_duration_ms;
        
        if self.state == VisibilityState::Hidden {
            if let Some(last) = self.last_change {
                duration += last.elapsed().as_millis() as u64;
            }
        }
        
        duration
    }
}

/// Page visibility manager
#[derive(Debug, Default)]
pub struct PageVisibilityManager {
    documents: Vec<(u64, DocumentVisibility)>,
}

impl PageVisibilityManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a document
    pub fn register(&mut self, doc_id: u64) {
        self.documents.push((doc_id, DocumentVisibility::new()));
    }
    
    /// Unregister a document
    pub fn unregister(&mut self, doc_id: u64) {
        self.documents.retain(|(id, _)| *id != doc_id);
    }
    
    /// Get document visibility
    pub fn get(&self, doc_id: u64) -> Option<&DocumentVisibility> {
        self.documents.iter()
            .find(|(id, _)| *id == doc_id)
            .map(|(_, v)| v)
    }
    
    /// Get mutable document visibility
    pub fn get_mut(&mut self, doc_id: u64) -> Option<&mut DocumentVisibility> {
        self.documents.iter_mut()
            .find(|(id, _)| *id == doc_id)
            .map(|(_, v)| v)
    }
    
    /// Set all tabs to hidden (window minimized)
    pub fn hide_all(&mut self) {
        for (_, vis) in &mut self.documents {
            vis.set_state(VisibilityState::Hidden);
        }
    }
    
    /// Set all tabs to visible (window restored)
    pub fn show_all(&mut self) {
        for (_, vis) in &mut self.documents {
            vis.set_state(VisibilityState::Visible);
        }
    }
    
    /// Handle tab switch
    pub fn switch_tab(&mut self, active_doc_id: u64) {
        for (id, vis) in &mut self.documents {
            if *id == active_doc_id {
                vis.set_state(VisibilityState::Visible);
            } else {
                vis.set_state(VisibilityState::Hidden);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_visibility() {
        let mut mgr = PageVisibilityManager::new();
        
        mgr.register(1);
        mgr.register(2);
        
        // Initially visible
        assert_eq!(mgr.get(1).unwrap().visibility_state(), VisibilityState::Visible);
        
        // Switch to tab 2
        mgr.switch_tab(2);
        assert_eq!(mgr.get(1).unwrap().visibility_state(), VisibilityState::Hidden);
        assert_eq!(mgr.get(2).unwrap().visibility_state(), VisibilityState::Visible);
    }
}
