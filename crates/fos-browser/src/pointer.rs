//! Pointer Events API
//!
//! Unified input handling for mouse, touch, and pen.

/// Pointer type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointerType {
    #[default]
    Mouse,
    Pen,
    Touch,
}

/// Pointer event
#[derive(Debug, Clone)]
pub struct PointerEvent {
    pub event_type: PointerEventType,
    pub pointer_id: u64,
    pub pointer_type: PointerType,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub pressure: f32,
    pub tangential_pressure: f32,
    pub tilt_x: i32,
    pub tilt_y: i32,
    pub twist: i32,
    pub is_primary: bool,
    pub button: i16,
    pub buttons: u16,
    pub modifiers: Modifiers,
}

/// Pointer event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerEventType {
    PointerDown,
    PointerUp,
    PointerMove,
    PointerOver,
    PointerOut,
    PointerEnter,
    PointerLeave,
    PointerCancel,
    GotPointerCapture,
    LostPointerCapture,
}

/// Keyboard/mouse modifiers
#[derive(Debug, Clone, Copy, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub meta: bool,
}

impl Default for PointerEvent {
    fn default() -> Self {
        Self {
            event_type: PointerEventType::PointerMove,
            pointer_id: 0,
            pointer_type: PointerType::Mouse,
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            pressure: 0.0,
            tangential_pressure: 0.0,
            tilt_x: 0,
            tilt_y: 0,
            twist: 0,
            is_primary: true,
            button: -1,
            buttons: 0,
            modifiers: Modifiers::default(),
        }
    }
}

/// Pointer manager
#[derive(Debug, Default)]
pub struct PointerManager {
    active_pointers: Vec<ActivePointer>,
    captured_element: Option<(u64, u64)>, // (pointer_id, element_id)
    next_pointer_id: u64,
}

#[derive(Debug)]
struct ActivePointer {
    id: u64,
    pointer_type: PointerType,
    is_primary: bool,
    x: f32,
    y: f32,
    buttons: u16,
}

impl PointerManager {
    pub fn new() -> Self {
        Self {
            next_pointer_id: 1,
            ..Default::default()
        }
    }
    
    /// Handle pointer down
    pub fn pointer_down(&mut self, pointer_type: PointerType, x: f32, y: f32, button: u16) -> PointerEvent {
        let is_primary = !self.active_pointers.iter().any(|p| p.pointer_type == pointer_type);
        
        let id = self.next_pointer_id;
        self.next_pointer_id += 1;
        
        self.active_pointers.push(ActivePointer {
            id,
            pointer_type,
            is_primary,
            x,
            y,
            buttons: button,
        });
        
        PointerEvent {
            event_type: PointerEventType::PointerDown,
            pointer_id: id,
            pointer_type,
            x,
            y,
            is_primary,
            button: button as i16,
            buttons: button,
            pressure: 0.5,
            ..Default::default()
        }
    }
    
    /// Handle pointer move
    pub fn pointer_move(&mut self, pointer_id: u64, x: f32, y: f32) -> Option<PointerEvent> {
        let pointer = self.active_pointers.iter_mut().find(|p| p.id == pointer_id)?;
        pointer.x = x;
        pointer.y = y;
        
        Some(PointerEvent {
            event_type: PointerEventType::PointerMove,
            pointer_id,
            pointer_type: pointer.pointer_type,
            x,
            y,
            is_primary: pointer.is_primary,
            buttons: pointer.buttons,
            ..Default::default()
        })
    }
    
    /// Handle pointer up
    pub fn pointer_up(&mut self, pointer_id: u64) -> Option<PointerEvent> {
        let idx = self.active_pointers.iter().position(|p| p.id == pointer_id)?;
        let pointer = self.active_pointers.remove(idx);
        
        // Release capture
        if self.captured_element.map(|(pid, _)| pid) == Some(pointer_id) {
            self.captured_element = None;
        }
        
        Some(PointerEvent {
            event_type: PointerEventType::PointerUp,
            pointer_id,
            pointer_type: pointer.pointer_type,
            x: pointer.x,
            y: pointer.y,
            is_primary: pointer.is_primary,
            ..Default::default()
        })
    }
    
    /// Capture pointer to element
    pub fn set_capture(&mut self, pointer_id: u64, element_id: u64) -> bool {
        if self.active_pointers.iter().any(|p| p.id == pointer_id) {
            self.captured_element = Some((pointer_id, element_id));
            true
        } else {
            false
        }
    }
    
    /// Release pointer capture
    pub fn release_capture(&mut self, pointer_id: u64) -> bool {
        if self.captured_element.map(|(pid, _)| pid) == Some(pointer_id) {
            self.captured_element = None;
            true
        } else {
            false
        }
    }
    
    /// Get captured element
    pub fn get_capture(&self, pointer_id: u64) -> Option<u64> {
        self.captured_element
            .filter(|(pid, _)| *pid == pointer_id)
            .map(|(_, eid)| eid)
    }
    
    /// Get active pointer count
    pub fn active_count(&self) -> usize {
        self.active_pointers.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pointer_events() {
        let mut mgr = PointerManager::new();
        
        let down = mgr.pointer_down(PointerType::Mouse, 100.0, 100.0, 1);
        assert_eq!(down.event_type, PointerEventType::PointerDown);
        assert!(down.is_primary);
        
        let pid = down.pointer_id;
        
        let mv = mgr.pointer_move(pid, 150.0, 150.0).unwrap();
        assert_eq!(mv.x, 150.0);
        
        let up = mgr.pointer_up(pid).unwrap();
        assert_eq!(up.event_type, PointerEventType::PointerUp);
    }
}
