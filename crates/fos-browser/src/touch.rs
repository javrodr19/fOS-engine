//! Touch events and gestures
//!
//! Multi-touch input handling for touch devices.

use std::collections::HashMap;

/// A single touch point
#[derive(Debug, Clone)]
pub struct Touch {
    pub identifier: u64,
    pub x: f32,
    pub y: f32,
    pub radius_x: f32,
    pub radius_y: f32,
    pub rotation_angle: f32,
    pub force: f32,
    pub target_element: Option<u64>,
}

impl Touch {
    pub fn new(identifier: u64, x: f32, y: f32) -> Self {
        Self {
            identifier,
            x,
            y,
            radius_x: 1.0,
            radius_y: 1.0,
            rotation_angle: 0.0,
            force: 1.0,
            target_element: None,
        }
    }
}

/// Touch event
#[derive(Debug, Clone)]
pub struct TouchEvent {
    pub event_type: TouchEventType,
    /// All currently active touches
    pub touches: Vec<Touch>,
    /// Touches that triggered this event
    pub changed_touches: Vec<Touch>,
    /// Touches on the target element
    pub target_touches: Vec<Touch>,
    pub ctrl_key: bool,
    pub alt_key: bool,
    pub shift_key: bool,
    pub meta_key: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEventType {
    TouchStart,
    TouchMove,
    TouchEnd,
    TouchCancel,
}

/// Touch manager
#[derive(Debug, Default)]
pub struct TouchManager {
    active_touches: HashMap<u64, Touch>,
    next_identifier: u64,
}

impl TouchManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Begin a new touch
    pub fn touch_start(&mut self, x: f32, y: f32, target: Option<u64>) -> TouchEvent {
        let id = self.next_identifier;
        self.next_identifier += 1;
        
        let mut touch = Touch::new(id, x, y);
        touch.target_element = target;
        
        self.active_touches.insert(id, touch.clone());
        
        TouchEvent {
            event_type: TouchEventType::TouchStart,
            touches: self.all_touches(),
            changed_touches: vec![touch],
            target_touches: Vec::new(),
            ctrl_key: false,
            alt_key: false,
            shift_key: false,
            meta_key: false,
        }
    }
    
    /// Move a touch
    pub fn touch_move(&mut self, identifier: u64, x: f32, y: f32) -> Option<TouchEvent> {
        // Update the touch position
        let touch = self.active_touches.get_mut(&identifier)?;
        touch.x = x;
        touch.y = y;
        let changed = touch.clone();
        
        // Now we can safely borrow self again
        let all = self.all_touches();
        
        Some(TouchEvent {
            event_type: TouchEventType::TouchMove,
            touches: all,
            changed_touches: vec![changed],
            target_touches: Vec::new(),
            ctrl_key: false,
            alt_key: false,
            shift_key: false,
            meta_key: false,
        })
    }
    
    /// End a touch
    pub fn touch_end(&mut self, identifier: u64) -> Option<TouchEvent> {
        let touch = self.active_touches.remove(&identifier)?;
        
        Some(TouchEvent {
            event_type: TouchEventType::TouchEnd,
            touches: self.all_touches(),
            changed_touches: vec![touch],
            target_touches: Vec::new(),
            ctrl_key: false,
            alt_key: false,
            shift_key: false,
            meta_key: false,
        })
    }
    
    /// Cancel all touches
    pub fn touch_cancel(&mut self) -> TouchEvent {
        let touches: Vec<_> = self.active_touches.drain().map(|(_, t)| t).collect();
        
        TouchEvent {
            event_type: TouchEventType::TouchCancel,
            touches: Vec::new(),
            changed_touches: touches,
            target_touches: Vec::new(),
            ctrl_key: false,
            alt_key: false,
            shift_key: false,
            meta_key: false,
        }
    }
    
    fn all_touches(&self) -> Vec<Touch> {
        self.active_touches.values().cloned().collect()
    }
    
    /// Get active touch count
    pub fn touch_count(&self) -> usize {
        self.active_touches.len()
    }
    
    /// Check for gestures
    pub fn detect_gesture(&self) -> Option<Gesture> {
        let touches: Vec<_> = self.active_touches.values().collect();
        
        match touches.len() {
            1 => None, // Single touch, no gesture yet
            2 => {
                // Pinch/zoom detection
                let t0 = &touches[0];
                let t1 = &touches[1];
                let distance = ((t1.x - t0.x).powi(2) + (t1.y - t0.y).powi(2)).sqrt();
                Some(Gesture::Pinch { distance })
            }
            _ => None,
        }
    }
}

/// Recognized gestures
#[derive(Debug, Clone)]
pub enum Gesture {
    Tap { x: f32, y: f32 },
    DoubleTap { x: f32, y: f32 },
    LongPress { x: f32, y: f32 },
    Swipe { direction: SwipeDirection, velocity: f32 },
    Pinch { distance: f32 },
    Rotate { angle: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    Up,
    Down,
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_touch_events() {
        let mut mgr = TouchManager::new();
        
        // Start touch
        let event = mgr.touch_start(100.0, 100.0, None);
        assert_eq!(event.event_type, TouchEventType::TouchStart);
        assert_eq!(mgr.touch_count(), 1);
        
        let id = event.changed_touches[0].identifier;
        
        // Move touch
        let event = mgr.touch_move(id, 150.0, 150.0).unwrap();
        assert_eq!(event.event_type, TouchEventType::TouchMove);
        
        // End touch
        let event = mgr.touch_end(id).unwrap();
        assert_eq!(event.event_type, TouchEventType::TouchEnd);
        assert_eq!(mgr.touch_count(), 0);
    }
}
