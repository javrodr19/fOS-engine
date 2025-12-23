//! Events Integration
//!
//! Integrates fos-js input events: keyboard, mouse, touch, drag, clipboard.

use std::collections::HashMap;
use fos_js::{
    KeyModifiers,
    MouseEvent, MouseButton,
    FocusEvent, FocusManager,
    ClipboardEvent,
    TouchEvent, Touch,
    DragEvent, DataTransfer,
};

/// Event handler manager for the browser
pub struct EventManager {
    /// Focus manager
    pub focus: FocusManager,
    /// Keyboard modifiers state
    modifiers: KeyModifiers,
    /// Mouse position
    mouse_x: f64,
    mouse_y: f64,
    /// Mouse button state
    mouse_buttons: u16,
    /// Touch points
    touches: HashMap<u32, Touch>,
    /// Drag data
    drag_data: Option<DataTransfer>,
    /// Event listeners by element ID
    listeners: HashMap<u64, Vec<EventListener>>,
}

/// Event listener registration
#[derive(Debug, Clone)]
pub struct EventListener {
    pub event_type: String,
    pub callback_id: u32,
    pub capture: bool,
    pub once: bool,
    pub passive: bool,
}

impl EventManager {
    /// Create new event manager
    pub fn new() -> Self {
        Self {
            focus: FocusManager::new(),
            modifiers: KeyModifiers::default(),
            mouse_x: 0.0,
            mouse_y: 0.0,
            mouse_buttons: 0,
            touches: HashMap::new(),
            drag_data: None,
            listeners: HashMap::new(),
        }
    }
    
    // === Keyboard Events ===
    
    /// Update modifier state
    pub fn update_modifiers(&mut self, shift: bool, ctrl: bool, alt: bool, meta: bool) {
        self.modifiers = KeyModifiers::from_flags(shift, ctrl, alt, meta);
    }
    
    /// Get current modifiers
    pub fn modifiers(&self) -> KeyModifiers {
        self.modifiers
    }
    
    // === Mouse Events ===
    
    /// Handle mouse move
    pub fn mouse_move(&mut self, x: f64, y: f64) -> MouseEvent {
        let dx = x - self.mouse_x;
        let dy = y - self.mouse_y;
        self.mouse_x = x;
        self.mouse_y = y;
        
        MouseEvent::mouse_move(x, y, dx, dy)
    }
    
    /// Handle mouse down
    pub fn mouse_down(&mut self, button: MouseButton) -> MouseEvent {
        self.mouse_buttons |= button.bit();
        MouseEvent::mouse_down(button, self.mouse_x, self.mouse_y)
    }
    
    /// Handle click
    pub fn click(&self) -> MouseEvent {
        MouseEvent::click(self.mouse_x, self.mouse_y)
    }
    
    // === Touch Events ===
    
    /// Handle touch start
    pub fn touch_start(&mut self, touch: Touch) -> TouchEvent {
        self.touches.insert(touch.identifier, touch.clone());
        TouchEvent::start(touch)
    }
    
    /// Handle touch move
    pub fn touch_move(&mut self, touch: Touch) -> TouchEvent {
        self.touches.insert(touch.identifier, touch.clone());
        TouchEvent::move_event(touch)
    }
    
    /// Handle touch end
    pub fn touch_end(&mut self, touch: Touch) -> TouchEvent {
        self.touches.remove(&touch.identifier);
        TouchEvent::end(touch)
    }
    
    // === Focus Events ===
    
    /// Handle focus
    pub fn set_focus(&mut self, element_id: u32) -> Option<FocusEvent> {
        self.focus.focus(element_id)
    }
    
    /// Handle blur
    pub fn clear_focus(&mut self) -> Option<FocusEvent> {
        self.focus.blur()
    }
    
    /// Get focused element
    pub fn focused(&self) -> Option<u32> {
        self.focus.focused()
    }
    
    /// Focus next element
    pub fn focus_next(&mut self) -> Option<u32> {
        self.focus.focus_next()
    }
    
    /// Focus previous element
    pub fn focus_previous(&mut self) -> Option<u32> {
        self.focus.focus_previous()
    }
    
    /// Set tab order
    pub fn set_tab_order(&mut self, order: Vec<u32>) {
        self.focus.set_tab_order(order);
    }
    
    // === Drag Events ===
    
    /// Start drag operation
    pub fn drag_start(&mut self) -> DragEvent {
        self.drag_data = Some(DataTransfer::new());
        DragEvent::drag_start(self.mouse_x, self.mouse_y)
    }
    
    /// Handle drop
    pub fn drop_event(&mut self) -> DragEvent {
        let data = self.drag_data.take().unwrap_or_default();
        DragEvent::drop(self.mouse_x, self.mouse_y, data)
    }
    
    /// Get drag data
    pub fn get_drag_data(&self) -> Option<&DataTransfer> {
        self.drag_data.as_ref()
    }
    
    /// Set drag data
    pub fn set_drag_data(&mut self, mime_type: &str, data: &str) {
        if let Some(dt) = self.drag_data.as_mut() {
            dt.set_data(mime_type, data);
        }
    }
    
    // === Clipboard Events ===
    
    /// Handle copy
    pub fn copy(&self) -> ClipboardEvent {
        ClipboardEvent::copy()
    }
    
    // === Event Listeners ===
    
    /// Add event listener
    pub fn add_listener(&mut self, element_id: u64, listener: EventListener) {
        self.listeners.entry(element_id).or_default().push(listener);
    }
    
    /// Remove event listener
    pub fn remove_listener(&mut self, element_id: u64, event_type: &str, callback_id: u32) {
        if let Some(listeners) = self.listeners.get_mut(&element_id) {
            listeners.retain(|l| !(l.event_type == event_type && l.callback_id == callback_id));
        }
    }
    
    /// Get listeners for element
    pub fn get_listeners(&self, element_id: u64, event_type: &str) -> Vec<&EventListener> {
        self.listeners.get(&element_id)
            .map(|ls| ls.iter().filter(|l| l.event_type == event_type).collect())
            .unwrap_or_default()
    }
    
    /// Get current mouse position
    pub fn mouse_position(&self) -> (f64, f64) {
        (self.mouse_x, self.mouse_y)
    }
    
    /// Get event statistics
    pub fn stats(&self) -> EventStats {
        EventStats {
            listener_count: self.listeners.values().map(|v| v.len()).sum(),
            touch_count: self.touches.len(),
            has_drag: self.drag_data.is_some(),
        }
    }
}

impl Default for EventManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Event statistics
#[derive(Debug, Clone)]
pub struct EventStats {
    pub listener_count: usize,
    pub touch_count: usize,
    pub has_drag: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_event_manager_creation() {
        let manager = EventManager::new();
        assert_eq!(manager.mouse_position(), (0.0, 0.0));
    }
    
    #[test]
    fn test_mouse_events() {
        let mut manager = EventManager::new();
        
        let _event = manager.mouse_move(100.0, 200.0);
        assert_eq!(manager.mouse_position(), (100.0, 200.0));
    }
    
    #[test]
    fn test_keyboard_modifiers() {
        let mut manager = EventManager::new();
        manager.update_modifiers(true, false, false, false);
        
        assert!(manager.modifiers().shift);
        assert!(!manager.modifiers().ctrl);
    }
    
    #[test]
    fn test_focus() {
        let mut manager = EventManager::new();
        manager.set_tab_order(vec![1, 2, 3]);
        manager.set_focus(1);
        assert_eq!(manager.focused(), Some(1));
    }
}
