//! Touch Events
//!
//! Touch event handling for mobile/tablet support.

/// Touch event
#[derive(Debug, Clone)]
pub struct TouchEvent {
    pub event_type: TouchEventType,
    /// All active touches
    pub touches: Vec<Touch>,
    /// Touches that started on target
    pub target_touches: Vec<Touch>,
    /// Touches that changed
    pub changed_touches: Vec<Touch>,
    
    // Modifiers
    pub shift_key: bool,
    pub ctrl_key: bool,
    pub alt_key: bool,
    pub meta_key: bool,
    
    // Event state
    pub bubbles: bool,
    pub cancelable: bool,
    default_prevented: bool,
    pub timestamp: f64,
}

/// Touch event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEventType {
    TouchStart,
    TouchMove,
    TouchEnd,
    TouchCancel,
}

/// Single touch point
#[derive(Debug, Clone)]
pub struct Touch {
    /// Unique identifier for this touch
    pub identifier: u32,
    /// Target element ID
    pub target_id: u32,
    /// X relative to viewport
    pub client_x: f64,
    /// Y relative to viewport
    pub client_y: f64,
    /// X relative to page
    pub page_x: f64,
    /// Y relative to page
    pub page_y: f64,
    /// X relative to screen
    pub screen_x: f64,
    /// Y relative to screen
    pub screen_y: f64,
    /// Contact radius X
    pub radius_x: f64,
    /// Contact radius Y
    pub radius_y: f64,
    /// Rotation angle
    pub rotation_angle: f64,
    /// Pressure (0.0 - 1.0)
    pub force: f64,
}

impl Touch {
    /// Create a new touch point
    pub fn new(id: u32, x: f64, y: f64) -> Self {
        Self {
            identifier: id,
            target_id: 0,
            client_x: x,
            client_y: y,
            page_x: x,
            page_y: y,
            screen_x: x,
            screen_y: y,
            radius_x: 1.0,
            radius_y: 1.0,
            rotation_angle: 0.0,
            force: 1.0,
        }
    }
}

impl Default for TouchEvent {
    fn default() -> Self {
        Self {
            event_type: TouchEventType::TouchStart,
            touches: Vec::new(),
            target_touches: Vec::new(),
            changed_touches: Vec::new(),
            shift_key: false,
            ctrl_key: false,
            alt_key: false,
            meta_key: false,
            bubbles: true,
            cancelable: true,
            default_prevented: false,
            timestamp: 0.0,
        }
    }
}

impl TouchEvent {
    /// Create a touch start event
    pub fn start(touch: Touch) -> Self {
        Self {
            event_type: TouchEventType::TouchStart,
            touches: vec![touch.clone()],
            target_touches: vec![touch.clone()],
            changed_touches: vec![touch],
            ..Default::default()
        }
    }
    
    /// Create a touch move event
    pub fn move_event(touch: Touch) -> Self {
        Self {
            event_type: TouchEventType::TouchMove,
            touches: vec![touch.clone()],
            changed_touches: vec![touch],
            ..Default::default()
        }
    }
    
    /// Create a touch end event
    pub fn end(touch: Touch) -> Self {
        Self {
            event_type: TouchEventType::TouchEnd,
            changed_touches: vec![touch],
            ..Default::default()
        }
    }
    
    /// Prevent default action
    pub fn prevent_default(&mut self) {
        self.default_prevented = true;
    }
    
    /// Check if this is a tap (single quick touch)
    pub fn is_tap(&self) -> bool {
        self.event_type == TouchEventType::TouchEnd && 
        self.changed_touches.len() == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_touch_event() {
        let touch = Touch::new(1, 100.0, 200.0);
        let event = TouchEvent::start(touch);
        
        assert_eq!(event.event_type, TouchEventType::TouchStart);
        assert_eq!(event.touches.len(), 1);
    }
}
