//! Focus Events
//!
//! Focus management and focus-related events.

/// Focus event
#[derive(Debug, Clone)]
pub struct FocusEvent {
    pub event_type: FocusEventType,
    pub related_target_id: Option<u32>,
    
    // Event state
    pub bubbles: bool,
    pub cancelable: bool,
    pub timestamp: f64,
}

/// Focus event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusEventType {
    Focus,    // Doesn't bubble
    Blur,     // Doesn't bubble
    FocusIn,  // Bubbles
    FocusOut, // Bubbles
}

impl FocusEvent {
    pub fn new(event_type: FocusEventType) -> Self {
        let bubbles = matches!(event_type, FocusEventType::FocusIn | FocusEventType::FocusOut);
        Self {
            event_type,
            related_target_id: None,
            bubbles,
            cancelable: false,
            timestamp: 0.0,
        }
    }
}

/// Focus manager for tracking focused element
#[derive(Debug, Default)]
pub struct FocusManager {
    /// Currently focused element ID
    focused_id: Option<u32>,
    /// Tab index order
    tab_order: Vec<u32>,
    /// Focus trap stack (for modals)
    trap_stack: Vec<FocusTrap>,
}

/// Focus trap for modal dialogs
#[derive(Debug)]
pub struct FocusTrap {
    pub container_id: u32,
    pub first_id: u32,
    pub last_id: u32,
    pub previous_focus: Option<u32>,
}

impl FocusManager {
    /// Create a new focus manager
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get currently focused element
    pub fn focused(&self) -> Option<u32> {
        self.focused_id
    }
    
    /// Set focus to an element
    pub fn focus(&mut self, id: u32) -> Option<FocusEvent> {
        if self.focused_id == Some(id) {
            return None;
        }
        
        let old = self.focused_id;
        self.focused_id = Some(id);
        
        Some(FocusEvent {
            event_type: FocusEventType::Focus,
            related_target_id: old,
            bubbles: false,
            cancelable: false,
            timestamp: 0.0,
        })
    }
    
    /// Remove focus
    pub fn blur(&mut self) -> Option<FocusEvent> {
        let old = self.focused_id.take()?;
        Some(FocusEvent {
            event_type: FocusEventType::Blur,
            related_target_id: Some(old),
            bubbles: false,
            cancelable: false,
            timestamp: 0.0,
        })
    }
    
    /// Move focus to next element
    pub fn focus_next(&mut self) -> Option<u32> {
        if self.tab_order.is_empty() {
            return None;
        }
        
        let current_idx = self.focused_id
            .and_then(|id| self.tab_order.iter().position(|&x| x == id))
            .unwrap_or(0);
        
        let next_idx = (current_idx + 1) % self.tab_order.len();
        let next_id = self.tab_order[next_idx];
        self.focus(next_id);
        Some(next_id)
    }
    
    /// Move focus to previous element
    pub fn focus_previous(&mut self) -> Option<u32> {
        if self.tab_order.is_empty() {
            return None;
        }
        
        let current_idx = self.focused_id
            .and_then(|id| self.tab_order.iter().position(|&x| x == id))
            .unwrap_or(0);
        
        let prev_idx = if current_idx == 0 { 
            self.tab_order.len() - 1 
        } else { 
            current_idx - 1 
        };
        let prev_id = self.tab_order[prev_idx];
        self.focus(prev_id);
        Some(prev_id)
    }
    
    /// Update tab order
    pub fn set_tab_order(&mut self, order: Vec<u32>) {
        self.tab_order = order;
    }
    
    /// Push a focus trap (for modals)
    pub fn push_trap(&mut self, trap: FocusTrap) {
        self.trap_stack.push(trap);
    }
    
    /// Pop focus trap
    pub fn pop_trap(&mut self) -> Option<FocusTrap> {
        let trap = self.trap_stack.pop()?;
        if let Some(prev) = trap.previous_focus {
            self.focus(prev);
        }
        Some(trap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_focus_manager() {
        let mut fm = FocusManager::new();
        
        fm.set_tab_order(vec![1, 2, 3, 4]);
        
        fm.focus(1);
        assert_eq!(fm.focused(), Some(1));
        
        fm.focus_next();
        assert_eq!(fm.focused(), Some(2));
    }
}
