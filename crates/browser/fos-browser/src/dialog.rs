//! Dialog API
//!
//! HTML dialog element and modal support.

use fos_dom::NodeId;

/// Dialog state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DialogState {
    #[default]
    Closed,
    Open,
    Modal,
}

/// Dialog element
#[derive(Debug, Clone)]
pub struct Dialog {
    pub id: u64,
    pub node_id: Option<NodeId>,
    pub state: DialogState,
    pub return_value: String,
}

impl Dialog {
    pub fn new(id: u64) -> Self {
        Self {
            id,
            node_id: None,
            state: DialogState::Closed,
            return_value: String::new(),
        }
    }
    
    pub fn with_node(id: u64, node_id: NodeId) -> Self {
        Self {
            id,
            node_id: Some(node_id),
            state: DialogState::Closed,
            return_value: String::new(),
        }
    }
    
    /// Check if open
    pub fn is_open(&self) -> bool {
        self.state != DialogState::Closed
    }
    
    /// Check if modal
    pub fn is_modal(&self) -> bool {
        self.state == DialogState::Modal
    }
}

/// Dialog manager
#[derive(Debug, Default)]
pub struct DialogManager {
    dialogs: Vec<Dialog>,
    modal_stack: Vec<u64>,
    next_id: u64,
}

impl DialogManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Register a dialog
    pub fn register(&mut self, node_id: Option<NodeId>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let dialog = if let Some(nid) = node_id {
            Dialog::with_node(id, nid)
        } else {
            Dialog::new(id)
        };
        
        self.dialogs.push(dialog);
        id
    }
    
    /// Show dialog
    pub fn show(&mut self, id: u64) -> bool {
        if let Some(dialog) = self.dialogs.iter_mut().find(|d| d.id == id) {
            if dialog.state == DialogState::Closed {
                dialog.state = DialogState::Open;
                return true;
            }
        }
        false
    }
    
    /// Show as modal
    pub fn show_modal(&mut self, id: u64) -> bool {
        if let Some(dialog) = self.dialogs.iter_mut().find(|d| d.id == id) {
            if dialog.state == DialogState::Closed {
                dialog.state = DialogState::Modal;
                self.modal_stack.push(id);
                return true;
            }
        }
        false
    }
    
    /// Close dialog
    pub fn close(&mut self, id: u64, return_value: Option<&str>) -> bool {
        if let Some(dialog) = self.dialogs.iter_mut().find(|d| d.id == id) {
            if dialog.state != DialogState::Closed {
                if let Some(rv) = return_value {
                    dialog.return_value = rv.to_string();
                }
                dialog.state = DialogState::Closed;
                self.modal_stack.retain(|&mid| mid != id);
                return true;
            }
        }
        false
    }
    
    /// Get dialog
    pub fn get(&self, id: u64) -> Option<&Dialog> {
        self.dialogs.iter().find(|d| d.id == id)
    }
    
    /// Get active modal
    pub fn active_modal(&self) -> Option<u64> {
        self.modal_stack.last().copied()
    }
    
    /// Check if any modal is open
    pub fn has_modal(&self) -> bool {
        !self.modal_stack.is_empty()
    }
    
    /// Unregister dialog
    pub fn unregister(&mut self, id: u64) {
        self.dialogs.retain(|d| d.id != id);
        self.modal_stack.retain(|&mid| mid != id);
    }
}

/// Alert, confirm, prompt built-ins
#[derive(Debug)]
pub struct BuiltinDialogs {
    pending_alert: Option<String>,
    pending_confirm: Option<(String, bool)>,
    pending_prompt: Option<(String, Option<String>)>,
}

impl Default for BuiltinDialogs {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltinDialogs {
    pub fn new() -> Self {
        Self {
            pending_alert: None,
            pending_confirm: None,
            pending_prompt: None,
        }
    }
    
    /// Show alert
    pub fn alert(&mut self, message: &str) {
        self.pending_alert = Some(message.to_string());
    }
    
    /// Show confirm
    pub fn confirm(&mut self, message: &str) {
        self.pending_confirm = Some((message.to_string(), false));
    }
    
    /// Show prompt
    pub fn prompt(&mut self, message: &str, default: Option<&str>) {
        self.pending_prompt = Some((message.to_string(), default.map(String::from)));
    }
    
    /// Check for pending dialogs
    pub fn has_pending(&self) -> bool {
        self.pending_alert.is_some() 
            || self.pending_confirm.is_some() 
            || self.pending_prompt.is_some()
    }
    
    /// Get pending alert
    pub fn get_alert(&self) -> Option<&str> {
        self.pending_alert.as_deref()
    }
    
    /// Dismiss alert
    pub fn dismiss_alert(&mut self) {
        self.pending_alert = None;
    }
    
    /// Get pending confirm
    pub fn get_confirm(&self) -> Option<&str> {
        self.pending_confirm.as_ref().map(|(m, _)| m.as_str())
    }
    
    /// Respond to confirm
    pub fn respond_confirm(&mut self, result: bool) {
        self.pending_confirm = None;
    }
    
    /// Get pending prompt
    pub fn get_prompt(&self) -> Option<(&str, Option<&str>)> {
        self.pending_prompt.as_ref().map(|(m, d)| (m.as_str(), d.as_deref()))
    }
    
    /// Respond to prompt
    pub fn respond_prompt(&mut self, _result: Option<&str>) {
        self.pending_prompt = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dialog_manager() {
        let mut mgr = DialogManager::new();
        
        let id = mgr.register(None);
        assert!(!mgr.get(id).unwrap().is_open());
        
        mgr.show_modal(id);
        assert!(mgr.get(id).unwrap().is_modal());
        assert!(mgr.has_modal());
        
        mgr.close(id, Some("ok"));
        assert!(!mgr.get(id).unwrap().is_open());
        assert!(!mgr.has_modal());
    }
}
