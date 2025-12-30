//! DOM Events
//!
//! DOM mutation and change events.

use crate::NodeId;

/// DOM event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomEventType {
    // Node mutation events
    DOMNodeInserted,
    DOMNodeRemoved,
    DOMNodeInsertedIntoDocument,
    DOMNodeRemovedFromDocument,
    DOMSubtreeModified,
    DOMAttrModified,
    DOMCharacterDataModified,
    
    // Content change
    DOMContentLoaded,
    
    // Load events
    Load,
    Unload,
    BeforeUnload,
    
    // Ready state
    ReadyStateChange,
}

/// DOM event
#[derive(Debug, Clone)]
pub struct DomEvent {
    pub event_type: DomEventType,
    pub target: NodeId,
    pub current_target: Option<NodeId>,
    pub related_node: Option<NodeId>,
    pub prev_value: Option<String>,
    pub new_value: Option<String>,
    pub attr_name: Option<String>,
    pub bubbles: bool,
    pub cancelable: bool,
    pub timestamp: f64,
    default_prevented: bool,
    propagation_stopped: bool,
}

impl DomEvent {
    /// Create node inserted event
    pub fn node_inserted(target: NodeId, parent: NodeId) -> Self {
        Self {
            event_type: DomEventType::DOMNodeInserted,
            target,
            current_target: Some(parent),
            related_node: Some(parent),
            prev_value: None,
            new_value: None,
            attr_name: None,
            bubbles: true,
            cancelable: false,
            timestamp: 0.0,
            default_prevented: false,
            propagation_stopped: false,
        }
    }
    
    /// Create node removed event
    pub fn node_removed(target: NodeId, parent: NodeId) -> Self {
        Self {
            event_type: DomEventType::DOMNodeRemoved,
            target,
            current_target: Some(parent),
            related_node: Some(parent),
            prev_value: None,
            new_value: None,
            attr_name: None,
            bubbles: true,
            cancelable: false,
            timestamp: 0.0,
            default_prevented: false,
            propagation_stopped: false,
        }
    }
    
    /// Create attribute modified event
    pub fn attr_modified(target: NodeId, name: &str, old_value: Option<&str>, new_value: Option<&str>) -> Self {
        Self {
            event_type: DomEventType::DOMAttrModified,
            target,
            current_target: Some(target),
            related_node: None,
            prev_value: old_value.map(|s| s.to_string()),
            new_value: new_value.map(|s| s.to_string()),
            attr_name: Some(name.to_string()),
            bubbles: true,
            cancelable: false,
            timestamp: 0.0,
            default_prevented: false,
            propagation_stopped: false,
        }
    }
    
    /// Create character data modified event
    pub fn char_data_modified(target: NodeId, old_value: &str, new_value: &str) -> Self {
        Self {
            event_type: DomEventType::DOMCharacterDataModified,
            target,
            current_target: Some(target),
            related_node: None,
            prev_value: Some(old_value.to_string()),
            new_value: Some(new_value.to_string()),
            attr_name: None,
            bubbles: true,
            cancelable: false,
            timestamp: 0.0,
            default_prevented: false,
            propagation_stopped: false,
        }
    }
    
    /// Create subtree modified event
    pub fn subtree_modified(target: NodeId) -> Self {
        Self {
            event_type: DomEventType::DOMSubtreeModified,
            target,
            current_target: Some(target),
            related_node: None,
            prev_value: None,
            new_value: None,
            attr_name: None,
            bubbles: true,
            cancelable: false,
            timestamp: 0.0,
            default_prevented: false,
            propagation_stopped: false,
        }
    }
    
    /// Create DOMContentLoaded event
    pub fn content_loaded(target: NodeId) -> Self {
        Self {
            event_type: DomEventType::DOMContentLoaded,
            target,
            current_target: Some(target),
            related_node: None,
            prev_value: None,
            new_value: None,
            attr_name: None,
            bubbles: true,
            cancelable: false,
            timestamp: 0.0,
            default_prevented: false,
            propagation_stopped: false,
        }
    }
    
    /// Prevent default action
    pub fn prevent_default(&mut self) {
        if self.cancelable {
            self.default_prevented = true;
        }
    }
    
    /// Stop propagation
    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }
    
    /// Check if default was prevented
    pub fn is_default_prevented(&self) -> bool {
        self.default_prevented
    }
}

/// Event dispatcher trait
pub trait EventDispatcher {
    fn dispatch_event(&mut self, event: DomEvent) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_node_inserted_event() {
        let event = DomEvent::node_inserted(NodeId(5), NodeId(1));
        
        assert_eq!(event.event_type, DomEventType::DOMNodeInserted);
        assert_eq!(event.target, NodeId(5));
        assert_eq!(event.related_node, Some(NodeId(1)));
    }
    
    #[test]
    fn test_attr_modified_event() {
        let event = DomEvent::attr_modified(
            NodeId(1), "class", 
            Some("old"), 
            Some("new")
        );
        
        assert_eq!(event.attr_name, Some("class".to_string()));
        assert_eq!(event.prev_value, Some("old".to_string()));
        assert_eq!(event.new_value, Some("new".to_string()));
    }
}
