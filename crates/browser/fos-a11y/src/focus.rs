//! Focus Management
//!
//! Keyboard navigation and focus handling.

/// Focus manager
#[derive(Debug, Default)]
pub struct FocusManager {
    focused_id: Option<u64>,
    focus_order: Vec<u64>,
    focus_trap: Option<u64>, // Container trapping focus
}

impl FocusManager {
    pub fn new() -> Self { Self::default() }
    
    /// Set focus order
    pub fn set_focus_order(&mut self, order: Vec<u64>) {
        self.focus_order = order;
    }
    
    /// Focus element
    pub fn focus(&mut self, id: u64) -> bool {
        if self.focus_order.contains(&id) || self.focus_order.is_empty() {
            self.focused_id = Some(id);
            true
        } else {
            false
        }
    }
    
    /// Get focused element
    pub fn get_focused(&self) -> Option<u64> {
        self.focused_id
    }
    
    /// Focus next element
    pub fn focus_next(&mut self) -> Option<u64> {
        if self.focus_order.is_empty() {
            return self.focused_id;
        }
        
        let next = match self.focused_id {
            Some(current) => {
                let pos = self.focus_order.iter().position(|&id| id == current);
                match pos {
                    Some(p) if p + 1 < self.focus_order.len() => self.focus_order[p + 1],
                    _ => self.focus_order[0], // Wrap around
                }
            }
            None => self.focus_order[0],
        };
        
        // Check focus trap
        if let Some(trap) = self.focus_trap {
            if !self.is_within_trap(next, trap) {
                return self.focused_id;
            }
        }
        
        self.focused_id = Some(next);
        self.focused_id
    }
    
    /// Focus previous element
    pub fn focus_prev(&mut self) -> Option<u64> {
        if self.focus_order.is_empty() {
            return self.focused_id;
        }
        
        let prev = match self.focused_id {
            Some(current) => {
                let pos = self.focus_order.iter().position(|&id| id == current);
                match pos {
                    Some(0) => *self.focus_order.last().unwrap(),
                    Some(p) => self.focus_order[p - 1],
                    None => *self.focus_order.last().unwrap(),
                }
            }
            None => *self.focus_order.last().unwrap(),
        };
        
        self.focused_id = Some(prev);
        self.focused_id
    }
    
    /// Set focus trap
    pub fn set_focus_trap(&mut self, container_id: u64) {
        self.focus_trap = Some(container_id);
    }
    
    /// Release focus trap
    pub fn release_focus_trap(&mut self) {
        self.focus_trap = None;
    }
    
    fn is_within_trap(&self, _id: u64, _trap: u64) -> bool {
        // Would check if ID is within trap container
        true
    }
    
    /// Blur current focus
    pub fn blur(&mut self) {
        self.focused_id = None;
    }
}

/// Tab index
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabIndex {
    NotFocusable,       // tabindex="-1" or not set
    Sequential(i32),    // tabindex="0" or positive
}

impl TabIndex {
    pub fn parse(value: &str) -> Self {
        match value.parse::<i32>() {
            Ok(n) if n < 0 => Self::NotFocusable,
            Ok(n) => Self::Sequential(n),
            Err(_) => Self::NotFocusable,
        }
    }
    
    pub fn is_focusable(&self) -> bool {
        matches!(self, Self::Sequential(_))
    }
}

/// Skip link
#[derive(Debug, Clone)]
pub struct SkipLink {
    pub label: String,
    pub target_id: String,
}

impl SkipLink {
    pub fn new(label: &str, target: &str) -> Self {
        Self {
            label: label.to_string(),
            target_id: target.to_string(),
        }
    }
}

/// Focus indicator style
#[derive(Debug, Clone)]
pub struct FocusIndicator {
    pub color: String,
    pub width: f64,
    pub offset: f64,
    pub style: FocusStyle,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum FocusStyle {
    #[default]
    Outline,
    Ring,
    Underline,
}

impl Default for FocusIndicator {
    fn default() -> Self {
        Self {
            color: "#0066ff".to_string(),
            width: 2.0,
            offset: 2.0,
            style: FocusStyle::Outline,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_focus_manager() {
        let mut fm = FocusManager::new();
        fm.set_focus_order(vec![1, 2, 3, 4]);
        
        fm.focus_next();
        assert_eq!(fm.get_focused(), Some(1));
        
        fm.focus_next();
        assert_eq!(fm.get_focused(), Some(2));
        
        fm.focus_prev();
        assert_eq!(fm.get_focused(), Some(1));
    }
    
    #[test]
    fn test_tab_index() {
        assert!(!TabIndex::parse("-1").is_focusable());
        assert!(TabIndex::parse("0").is_focusable());
        assert!(TabIndex::parse("5").is_focusable());
    }
}
