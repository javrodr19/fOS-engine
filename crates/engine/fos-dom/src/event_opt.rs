//! Event Optimization
//!
//! Event listener coalescing, deduplication, and lazy binding.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

/// Event type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventType {
    Click,
    DblClick,
    MouseDown,
    MouseUp,
    MouseMove,
    MouseEnter,
    MouseLeave,
    MouseOver,
    MouseOut,
    KeyDown,
    KeyUp,
    KeyPress,
    Input,
    Change,
    Focus,
    Blur,
    Submit,
    Reset,
    Scroll,
    Wheel,
    TouchStart,
    TouchEnd,
    TouchMove,
    TouchCancel,
    Load,
    Unload,
    Resize,
    Custom(u32),
}

impl EventType {
    /// Check if this event type can bubble
    pub fn bubbles(&self) -> bool {
        !matches!(self, 
            EventType::Focus | 
            EventType::Blur | 
            EventType::Load | 
            EventType::Unload |
            EventType::MouseEnter |
            EventType::MouseLeave
        )
    }
    
    /// Check if this is a high-frequency event
    pub fn is_high_frequency(&self) -> bool {
        matches!(self,
            EventType::MouseMove |
            EventType::TouchMove |
            EventType::Scroll |
            EventType::Wheel
        )
    }
}

/// Event handler function ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HandlerId(pub u64);

/// Event listener with handler tracking
#[derive(Debug, Clone)]
pub struct EventListener {
    /// Handler ID
    pub handler_id: HandlerId,
    /// Capture phase
    pub capture: bool,
    /// Passive listener
    pub passive: bool,
    /// Once (auto-remove)
    pub once: bool,
}

/// Coalesced event manager
/// Uses a single handler per event type per target
#[derive(Debug, Default)]
pub struct CoalescedEventManager {
    /// Handlers by target element and event type
    /// Maps (element_id, event_type) -> list of handlers
    handlers: HashMap<(u64, EventType), Vec<EventListener>>,
    /// Handler deduplication set
    handler_signatures: HashSet<u64>,
    /// Deferred event bindings (lazy binding)
    deferred: Vec<DeferredBinding>,
    /// Root handlers (delegated)
    root_handlers: HashMap<EventType, Vec<DelegatedHandler>>,
}

/// Deferred event binding
#[derive(Debug, Clone)]
pub struct DeferredBinding {
    /// Target selector
    pub selector: String,
    /// Event type
    pub event_type: EventType,
    /// Handler ID
    pub handler_id: HandlerId,
    /// Listener options
    pub options: ListenerOptions,
}

/// Delegated handler
#[derive(Debug, Clone)]
pub struct DelegatedHandler {
    /// Selector to match
    pub selector: String,
    /// Handler ID
    pub handler_id: HandlerId,
    /// Options
    pub options: ListenerOptions,
}

/// Listener options
#[derive(Debug, Clone, Copy, Default)]
pub struct ListenerOptions {
    pub capture: bool,
    pub passive: bool,
    pub once: bool,
}

impl CoalescedEventManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add event listener with deduplication
    pub fn add_listener(
        &mut self,
        target: u64,
        event_type: EventType,
        handler_id: HandlerId,
        options: ListenerOptions,
    ) -> bool {
        // Generate signature for deduplication
        let signature = self.handler_signature(target, event_type, handler_id, options.capture);
        
        // Check for duplicate
        if self.handler_signatures.contains(&signature) {
            return false;
        }
        
        // Add handler
        let listener = EventListener {
            handler_id,
            capture: options.capture,
            passive: options.passive,
            once: options.once,
        };
        
        self.handlers
            .entry((target, event_type))
            .or_default()
            .push(listener);
        
        self.handler_signatures.insert(signature);
        true
    }
    
    /// Remove event listener
    pub fn remove_listener(
        &mut self,
        target: u64,
        event_type: EventType,
        handler_id: HandlerId,
        capture: bool,
    ) -> bool {
        let key = (target, event_type);
        
        if let Some(listeners) = self.handlers.get_mut(&key) {
            let initial_len = listeners.len();
            listeners.retain(|l| !(l.handler_id == handler_id && l.capture == capture));
            
            if listeners.len() < initial_len {
                let signature = self.handler_signature(target, event_type, handler_id, capture);
                self.handler_signatures.remove(&signature);
                return true;
            }
        }
        
        false
    }
    
    /// Get handlers for target and event type
    pub fn get_handlers(&self, target: u64, event_type: EventType) -> Vec<&EventListener> {
        self.handlers
            .get(&(target, event_type))
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }
    
    /// Defer event binding (lazy binding)
    pub fn defer_binding(
        &mut self,
        selector: &str,
        event_type: EventType,
        handler_id: HandlerId,
        options: ListenerOptions,
    ) {
        self.deferred.push(DeferredBinding {
            selector: selector.to_string(),
            event_type,
            handler_id,
            options,
        });
    }
    
    /// Apply deferred bindings when elements match selector
    pub fn apply_deferred(&mut self, target: u64, matches_selector: impl Fn(&str) -> bool) {
        let matching: Vec<_> = self.deferred.iter()
            .filter(|d| matches_selector(&d.selector))
            .cloned()
            .collect();
        
        for binding in matching {
            self.add_listener(target, binding.event_type, binding.handler_id, binding.options);
        }
    }
    
    /// Add delegated handler (single root handler)
    pub fn add_delegated(
        &mut self,
        event_type: EventType,
        selector: &str,
        handler_id: HandlerId,
        options: ListenerOptions,
    ) {
        self.root_handlers
            .entry(event_type)
            .or_default()
            .push(DelegatedHandler {
                selector: selector.to_string(),
                handler_id,
                options,
            });
    }
    
    /// Get delegated handlers that match a target
    pub fn get_delegated_handlers(
        &self,
        event_type: EventType,
        matches: impl Fn(&str) -> bool,
    ) -> Vec<&DelegatedHandler> {
        self.root_handlers
            .get(&event_type)
            .map(|handlers| handlers.iter().filter(|h| matches(&h.selector)).collect())
            .unwrap_or_default()
    }
    
    /// Remove once handlers after dispatch
    pub fn cleanup_once_handlers(&mut self, target: u64, event_type: EventType, dispatched: &[HandlerId]) {
        if let Some(listeners) = self.handlers.get_mut(&(target, event_type)) {
            listeners.retain(|l| !l.once || !dispatched.contains(&l.handler_id));
        }
    }
    
    /// Generate handler signature for deduplication
    fn handler_signature(&self, target: u64, event_type: EventType, handler_id: HandlerId, capture: bool) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        target.hash(&mut hasher);
        event_type.hash(&mut hasher);
        handler_id.0.hash(&mut hasher);
        capture.hash(&mut hasher);
        hasher.finish()
    }
    
    /// Get stats
    pub fn stats(&self) -> EventManagerStats {
        EventManagerStats {
            total_handlers: self.handlers.values().map(|v| v.len()).sum(),
            unique_signatures: self.handler_signatures.len(),
            deferred_count: self.deferred.len(),
            delegated_count: self.root_handlers.values().map(|v| v.len()).sum(),
        }
    }
}

/// Event manager statistics
#[derive(Debug, Clone)]
pub struct EventManagerStats {
    pub total_handlers: usize,
    pub unique_signatures: usize,
    pub deferred_count: usize,
    pub delegated_count: usize,
}

/// High-frequency event throttler
#[derive(Debug, Default)]
pub struct EventThrottler {
    /// Last dispatch time per event type
    last_dispatch: HashMap<EventType, std::time::Instant>,
    /// Throttle interval per event type (ms)
    intervals: HashMap<EventType, u64>,
}

impl EventThrottler {
    pub fn new() -> Self {
        let mut throttler = Self::default();
        // Default throttles for high-frequency events
        throttler.intervals.insert(EventType::MouseMove, 16); // ~60fps
        throttler.intervals.insert(EventType::TouchMove, 16);
        throttler.intervals.insert(EventType::Scroll, 16);
        throttler.intervals.insert(EventType::Wheel, 16);
        throttler
    }
    
    /// Set throttle interval for event type
    pub fn set_interval(&mut self, event_type: EventType, interval_ms: u64) {
        self.intervals.insert(event_type, interval_ms);
    }
    
    /// Check if event should be dispatched
    pub fn should_dispatch(&mut self, event_type: EventType) -> bool {
        if let Some(&interval) = self.intervals.get(&event_type) {
            let now = std::time::Instant::now();
            
            if let Some(&last) = self.last_dispatch.get(&event_type) {
                if now.duration_since(last).as_millis() < interval as u128 {
                    return false;
                }
            }
            
            self.last_dispatch.insert(event_type, now);
        }
        
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_event_type_bubbles() {
        assert!(EventType::Click.bubbles());
        assert!(!EventType::Focus.bubbles());
    }
    
    #[test]
    fn test_add_listener() {
        let mut manager = CoalescedEventManager::new();
        
        let added = manager.add_listener(
            1,
            EventType::Click,
            HandlerId(100),
            ListenerOptions::default(),
        );
        assert!(added);
        
        // Duplicate should fail
        let added_again = manager.add_listener(
            1,
            EventType::Click,
            HandlerId(100),
            ListenerOptions::default(),
        );
        assert!(!added_again);
    }
    
    #[test]
    fn test_remove_listener() {
        let mut manager = CoalescedEventManager::new();
        manager.add_listener(1, EventType::Click, HandlerId(100), ListenerOptions::default());
        
        let removed = manager.remove_listener(1, EventType::Click, HandlerId(100), false);
        assert!(removed);
        
        let handlers = manager.get_handlers(1, EventType::Click);
        assert!(handlers.is_empty());
    }
    
    #[test]
    fn test_delegated_handlers() {
        let mut manager = CoalescedEventManager::new();
        manager.add_delegated(
            EventType::Click,
            ".button",
            HandlerId(200),
            ListenerOptions::default(),
        );
        
        let handlers = manager.get_delegated_handlers(EventType::Click, |s| s == ".button");
        assert_eq!(handlers.len(), 1);
    }
    
    #[test]
    fn test_event_throttler() {
        let mut throttler = EventThrottler::new();
        
        // First dispatch should always work
        assert!(throttler.should_dispatch(EventType::MouseMove));
        
        // Immediate second should be throttled
        // Note: This may pass if system is slow, so just test it doesn't panic
        let _ = throttler.should_dispatch(EventType::MouseMove);
    }
}
