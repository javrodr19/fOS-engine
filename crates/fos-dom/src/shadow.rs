//! Shadow DOM
//!
//! Shadow root, slots, and scoped styles.

use crate::NodeId;

/// Shadow root mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShadowRootMode {
    #[default]
    Open,
    Closed,
}

/// Shadow root
#[derive(Debug, Clone)]
pub struct ShadowRoot {
    pub host: NodeId,
    pub mode: ShadowRootMode,
    pub delegates_focus: bool,
    pub slot_assignment: SlotAssignmentMode,
    children: Vec<NodeId>,
    slots: Vec<Slot>,
}

/// Slot assignment mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotAssignmentMode {
    #[default]
    Named,
    Manual,
}

/// Slot element
#[derive(Debug, Clone)]
pub struct Slot {
    pub name: String,
    pub assigned_nodes: Vec<NodeId>,
}

impl ShadowRoot {
    /// Create a new shadow root
    pub fn new(host: NodeId, mode: ShadowRootMode) -> Self {
        Self {
            host,
            mode,
            delegates_focus: false,
            slot_assignment: SlotAssignmentMode::Named,
            children: Vec::new(),
            slots: Vec::new(),
        }
    }
    
    /// Get children
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }
    
    /// Add a child
    pub fn append_child(&mut self, child: NodeId) {
        self.children.push(child);
    }
    
    /// Find slot by name
    pub fn get_slot(&self, name: &str) -> Option<&Slot> {
        self.slots.iter().find(|s| s.name == name)
    }
    
    /// Add a slot
    pub fn add_slot(&mut self, slot: Slot) {
        self.slots.push(slot);
    }
    
    /// Get assigned nodes for slot
    pub fn assigned_nodes(&self, slot_name: &str) -> Vec<NodeId> {
        self.get_slot(slot_name)
            .map(|s| s.assigned_nodes.clone())
            .unwrap_or_default()
    }
}

impl Slot {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            assigned_nodes: Vec::new(),
        }
    }
    
    pub fn default_slot() -> Self {
        Self::new("")
    }
    
    pub fn assign(&mut self, node: NodeId) {
        if !self.assigned_nodes.contains(&node) {
            self.assigned_nodes.push(node);
        }
    }
}

/// Element that can have shadow
pub trait Shadowable {
    /// Attach shadow root
    fn attach_shadow(&mut self, mode: ShadowRootMode) -> &mut ShadowRoot;
    
    /// Get shadow root
    fn shadow_root(&self) -> Option<&ShadowRoot>;
    
    /// Get shadow root mutably
    fn shadow_root_mut(&mut self) -> Option<&mut ShadowRoot>;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_shadow_root() {
        let mut shadow = ShadowRoot::new(NodeId(1), ShadowRootMode::Open);
        shadow.append_child(NodeId(2));
        shadow.append_child(NodeId(3));
        
        assert_eq!(shadow.children().len(), 2);
        assert_eq!(shadow.mode, ShadowRootMode::Open);
    }
    
    #[test]
    fn test_slot() {
        let mut slot = Slot::new("header");
        slot.assign(NodeId(10));
        slot.assign(NodeId(11));
        
        assert_eq!(slot.assigned_nodes.len(), 2);
    }
}
