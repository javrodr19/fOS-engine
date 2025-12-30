//! Input Events Module
//!
//! Keyboard, mouse, touch, drag, and focus event handling.

mod keyboard;
mod mouse;
mod focus;
mod clipboard;
mod touch;
mod drag;

pub use keyboard::{KeyboardEvent, Key, KeyModifiers};
pub use mouse::{MouseEvent, MouseButton};
pub use focus::{FocusEvent, FocusManager};
pub use clipboard::{ClipboardEvent, ClipboardData};
pub use touch::{TouchEvent, Touch, TouchEventType};
pub use drag::{DragEvent, DataTransfer, DropEffect};

/// Base event trait
pub trait Event {
    /// Get event type name
    fn event_type(&self) -> &str;
    
    /// Check if event bubbles
    fn bubbles(&self) -> bool;
    
    /// Check if event is cancelable
    fn cancelable(&self) -> bool;
    
    /// Check if default was prevented
    fn default_prevented(&self) -> bool;
    
    /// Prevent default action
    fn prevent_default(&mut self);
    
    /// Stop propagation
    fn stop_propagation(&mut self);
    
    /// Stop immediate propagation
    fn stop_immediate_propagation(&mut self);
    
    /// Get event timestamp
    fn timestamp(&self) -> f64;
}

/// Event target trait
pub trait EventTarget {
    /// Add event listener
    fn add_event_listener(&mut self, event_type: &str, callback_id: u32, options: ListenerOptions);
    
    /// Remove event listener
    fn remove_event_listener(&mut self, event_type: &str, callback_id: u32);
    
    /// Dispatch event
    fn dispatch_event(&mut self, event: &mut dyn Event) -> bool;
}

/// Event listener options
#[derive(Debug, Clone, Copy, Default)]
pub struct ListenerOptions {
    pub capture: bool,
    pub once: bool,
    pub passive: bool,
}
