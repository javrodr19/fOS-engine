//! Mouse Events
//!
//! MouseEvent implementation with button states and coordinates.

/// Mouse event
#[derive(Debug, Clone)]
pub struct MouseEvent {
    pub event_type: MouseEventType,
    pub button: MouseButton,
    pub buttons: u16, // Bitmask of pressed buttons
    
    // Coordinates
    pub client_x: f64,
    pub client_y: f64,
    pub page_x: f64,
    pub page_y: f64,
    pub screen_x: f64,
    pub screen_y: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub movement_x: f64,
    pub movement_y: f64,
    
    // Modifiers
    pub shift_key: bool,
    pub ctrl_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
    
    // Event state
    pub bubbles: bool,
    pub cancelable: bool,
    default_prevented: bool,
    propagation_stopped: bool,
    pub timestamp: f64,
    
    // Related target (for enter/leave)
    pub related_target_id: Option<u32>,
}

/// Mouse event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseEventType {
    Click,
    DblClick,
    MouseDown,
    MouseUp,
    MouseMove,
    MouseEnter,
    MouseLeave,
    MouseOver,
    MouseOut,
    ContextMenu,
    Wheel,
}

/// Mouse buttons
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Primary button (usually left)
    Primary,
    /// Auxiliary button (usually middle/wheel)
    Auxiliary,
    /// Secondary button (usually right)
    Secondary,
    /// Fourth button (usually back)
    Fourth,
    /// Fifth button (usually forward)
    Fifth,
    /// No button
    None,
}

impl MouseButton {
    /// Convert from button number (0-4)
    pub fn from_number(n: i16) -> Self {
        match n {
            0 => Self::Primary,
            1 => Self::Auxiliary,
            2 => Self::Secondary,
            3 => Self::Fourth,
            4 => Self::Fifth,
            _ => Self::None,
        }
    }
    
    /// Convert to button number
    pub fn to_number(&self) -> i16 {
        match self {
            Self::Primary => 0,
            Self::Auxiliary => 1,
            Self::Secondary => 2,
            Self::Fourth => 3,
            Self::Fifth => 4,
            Self::None => -1,
        }
    }
    
    /// Get bit for buttons bitmask
    pub fn bit(&self) -> u16 {
        match self {
            Self::Primary => 1,
            Self::Auxiliary => 4,
            Self::Secondary => 2,
            Self::Fourth => 8,
            Self::Fifth => 16,
            Self::None => 0,
        }
    }
}

impl Default for MouseEvent {
    fn default() -> Self {
        Self {
            event_type: MouseEventType::Click,
            button: MouseButton::None,
            buttons: 0,
            client_x: 0.0,
            client_y: 0.0,
            page_x: 0.0,
            page_y: 0.0,
            screen_x: 0.0,
            screen_y: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
            movement_x: 0.0,
            movement_y: 0.0,
            shift_key: false,
            ctrl_key: false,
            alt_key: false,
            meta_key: false,
            bubbles: true,
            cancelable: true,
            default_prevented: false,
            propagation_stopped: false,
            timestamp: 0.0,
            related_target_id: None,
        }
    }
}

impl MouseEvent {
    /// Create a click event
    pub fn click(x: f64, y: f64) -> Self {
        Self {
            event_type: MouseEventType::Click,
            button: MouseButton::Primary,
            buttons: 0,
            client_x: x,
            client_y: y,
            page_x: x,
            page_y: y,
            ..Default::default()
        }
    }
    
    /// Create a mouse down event
    pub fn mouse_down(button: MouseButton, x: f64, y: f64) -> Self {
        Self {
            event_type: MouseEventType::MouseDown,
            button,
            buttons: button.bit(),
            client_x: x,
            client_y: y,
            page_x: x,
            page_y: y,
            ..Default::default()
        }
    }
    
    /// Create a mouse move event
    pub fn mouse_move(x: f64, y: f64, dx: f64, dy: f64) -> Self {
        Self {
            event_type: MouseEventType::MouseMove,
            client_x: x,
            client_y: y,
            page_x: x,
            page_y: y,
            movement_x: dx,
            movement_y: dy,
            ..Default::default()
        }
    }
    
    /// Prevent default action
    pub fn prevent_default(&mut self) {
        self.default_prevented = true;
    }
    
    /// Stop event propagation
    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }
    
    /// Check if any modifier key is pressed
    pub fn any_modifier(&self) -> bool {
        self.shift_key || self.ctrl_key || self.alt_key || self.meta_key
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_click_event() {
        let event = MouseEvent::click(100.0, 200.0);
        assert_eq!(event.event_type, MouseEventType::Click);
        assert_eq!(event.client_x, 100.0);
        assert_eq!(event.client_y, 200.0);
    }
    
    #[test]
    fn test_button_conversion() {
        assert_eq!(MouseButton::from_number(0), MouseButton::Primary);
        assert_eq!(MouseButton::Secondary.to_number(), 2);
    }
}
