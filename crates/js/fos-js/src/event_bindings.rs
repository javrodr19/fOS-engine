//! Event Bindings for JavaScript
//!
//! Provides JavaScript bindings for the DOM Event system.

use crate::{JsValue, JsError};
use crate::engine_trait::JsContextApi;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Event listener storage
#[derive(Default)]
pub struct EventListenerRegistry {
    /// Map of node ID -> event type -> listener IDs
    listeners: HashMap<usize, HashMap<String, Vec<u64>>>,
    /// Next listener ID
    next_id: u64,
}

impl EventListenerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an event listener
    pub fn add_listener(&mut self, node_id: usize, event_type: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        self.listeners
            .entry(node_id)
            .or_default()
            .entry(event_type.to_string())
            .or_default()
            .push(id);

        id
    }

    /// Remove an event listener
    pub fn remove_listener(&mut self, node_id: usize, event_type: &str, listener_id: u64) -> bool {
        if let Some(node_listeners) = self.listeners.get_mut(&node_id) {
            if let Some(type_listeners) = node_listeners.get_mut(event_type) {
                if let Some(pos) = type_listeners.iter().position(|&id| id == listener_id) {
                    type_listeners.remove(pos);
                    return true;
                }
            }
        }
        false
    }

    /// Get listeners for a node and event type
    pub fn get_listeners(&self, node_id: usize, event_type: &str) -> Vec<u64> {
        self.listeners
            .get(&node_id)
            .and_then(|node| node.get(event_type))
            .cloned()
            .unwrap_or_default()
    }

    /// Clear all listeners for a node
    pub fn clear_node(&mut self, node_id: usize) {
        self.listeners.remove(&node_id);
    }
}

/// Event bindings
pub struct EventBindings;

impl EventBindings {
    /// Install Event API into JavaScript context
    pub fn install<C: JsContextApi>(
        ctx: &C,
        registry: Arc<Mutex<EventListenerRegistry>>,
    ) -> Result<(), JsError> {
        Self::install_event_target(ctx, registry.clone())?;
        Self::install_event_constructors(ctx)?;
        Self::install_event_dispatch(ctx, registry)?;
        Ok(())
    }

    /// Install EventTarget methods
    fn install_event_target<C: JsContextApi>(
        ctx: &C,
        registry: Arc<Mutex<EventListenerRegistry>>,
    ) -> Result<(), JsError> {
        // addEventListener
        let reg = registry.clone();
        ctx.set_global_function("__event_addEventListener", move |args| {
            if args.len() >= 2 {
                if let (Some(node_id), Some(event_type)) = (
                    args[0].as_number(),
                    args[1].as_string(),
                ) {
                    let mut reg = reg.lock().unwrap();
                    let listener_id = reg.add_listener(node_id as usize, event_type);
                    return Ok(JsValue::Number(listener_id as f64));
                }
            }
            Ok(JsValue::Number(-1.0))
        })?;

        // removeEventListener
        let reg = registry.clone();
        ctx.set_global_function("__event_removeEventListener", move |args| {
            if args.len() >= 3 {
                if let (Some(node_id), Some(event_type), Some(listener_id)) = (
                    args[0].as_number(),
                    args[1].as_string(),
                    args[2].as_number(),
                ) {
                    let mut reg = reg.lock().unwrap();
                    let removed = reg.remove_listener(
                        node_id as usize,
                        event_type,
                        listener_id as u64,
                    );
                    return Ok(JsValue::Bool(removed));
                }
            }
            Ok(JsValue::Bool(false))
        })?;

        // hasEventListeners
        let reg = registry;
        ctx.set_global_function("__event_hasEventListeners", move |args| {
            if args.len() >= 2 {
                if let (Some(node_id), Some(event_type)) = (
                    args[0].as_number(),
                    args[1].as_string(),
                ) {
                    let reg = reg.lock().unwrap();
                    let listeners = reg.get_listeners(node_id as usize, event_type);
                    return Ok(JsValue::Bool(!listeners.is_empty()));
                }
            }
            Ok(JsValue::Bool(false))
        })?;

        Ok(())
    }

    /// Install Event constructors
    fn install_event_constructors<C: JsContextApi>(ctx: &C) -> Result<(), JsError> {
        // Event constructor
        ctx.set_global_function("__event_createEvent", move |args| {
            if let Some(event_type) = args.first().and_then(|v| v.as_string()) {
                // Return event data as object
                Ok(JsValue::Object)
            } else {
                Ok(JsValue::Null)
            }
        })?;

        // CustomEvent constructor
        ctx.set_global_function("__event_createCustomEvent", move |args| {
            if let Some(event_type) = args.first().and_then(|v| v.as_string()) {
                // detail is in args[1] if provided
                Ok(JsValue::Object)
            } else {
                Ok(JsValue::Null)
            }
        })?;

        // MouseEvent constructor
        ctx.set_global_function("__event_createMouseEvent", move |args| {
            if let Some(event_type) = args.first().and_then(|v| v.as_string()) {
                Ok(JsValue::Object)
            } else {
                Ok(JsValue::Null)
            }
        })?;

        // KeyboardEvent constructor
        ctx.set_global_function("__event_createKeyboardEvent", move |args| {
            if let Some(event_type) = args.first().and_then(|v| v.as_string()) {
                Ok(JsValue::Object)
            } else {
                Ok(JsValue::Null)
            }
        })?;

        // TouchEvent constructor
        ctx.set_global_function("__event_createTouchEvent", move |args| {
            if let Some(event_type) = args.first().and_then(|v| v.as_string()) {
                Ok(JsValue::Object)
            } else {
                Ok(JsValue::Null)
            }
        })?;

        Ok(())
    }

    /// Install event dispatch
    fn install_event_dispatch<C: JsContextApi>(
        ctx: &C,
        registry: Arc<Mutex<EventListenerRegistry>>,
    ) -> Result<(), JsError> {
        // dispatchEvent
        let reg = registry.clone();
        ctx.set_global_function("__event_dispatchEvent", move |args| {
            if args.len() >= 2 {
                if let (Some(node_id), Some(_event_type)) = (
                    args[0].as_number(),
                    args[1].as_string(),
                ) {
                    // Event dispatching would be handled by the browser
                    // This returns true if the event was not cancelled
                    return Ok(JsValue::Bool(true));
                }
            }
            Ok(JsValue::Bool(false))
        })?;

        // stopPropagation (called during event handling)
        ctx.set_global_function("__event_stopPropagation", move |_args| {
            // Mark event as stopped
            Ok(JsValue::Undefined)
        })?;

        // stopImmediatePropagation
        ctx.set_global_function("__event_stopImmediatePropagation", move |_args| {
            Ok(JsValue::Undefined)
        })?;

        // preventDefault
        ctx.set_global_function("__event_preventDefault", move |_args| {
            Ok(JsValue::Undefined)
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_listener_registry() {
        let mut registry = EventListenerRegistry::new();

        // Add listener
        let id1 = registry.add_listener(1, "click");
        let _id2 = registry.add_listener(1, "click");
        let _id3 = registry.add_listener(1, "keydown");

        assert_eq!(registry.get_listeners(1, "click").len(), 2);
        assert_eq!(registry.get_listeners(1, "keydown").len(), 1);

        // Remove listener
        assert!(registry.remove_listener(1, "click", id1));
        assert_eq!(registry.get_listeners(1, "click").len(), 1);

        // Clear node
        registry.clear_node(1);
        assert!(registry.get_listeners(1, "click").is_empty());
    }
}

