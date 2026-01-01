//! Shadow DOM v1
//!
//! Shadow root, slots, and scoped styles with structural sharing via CowTree.
//!
//! Key features:
//! - Shadow encapsulation (open/closed modes)
//! - Slot-based content distribution
//! - CowTree for efficient cloning and sharing
//! - Slotchange event tracking
//! - Declarative shadow DOM support

use crate::NodeId;
use std::sync::Arc;

/// Shadow root mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShadowRootMode {
    #[default]
    Open,
    Closed,
}

/// Slot assignment mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlotAssignmentMode {
    #[default]
    Named,
    Manual,
}

/// Shadow root initialization options
#[derive(Debug, Clone, Default)]
pub struct ShadowRootInit {
    pub mode: ShadowRootMode,
    pub delegates_focus: bool,
    pub slot_assignment: SlotAssignmentMode,
    /// Cloneable (for declarative shadow DOM)
    pub clonable: bool,
    /// Serializable (for getHTML())
    pub serializable: bool,
}

/// CowTree node for structural sharing
#[derive(Debug, Clone)]
pub struct CowNode {
    pub id: NodeId,
    pub children: Arc<Vec<CowNode>>,
}

impl CowNode {
    pub fn new(id: NodeId) -> Self {
        Self {
            id,
            children: Arc::new(Vec::new()),
        }
    }

    pub fn with_children(id: NodeId, children: Vec<CowNode>) -> Self {
        Self {
            id,
            children: Arc::new(children),
        }
    }

    /// Clone with structural sharing
    pub fn cow_clone(&self) -> Self {
        Self {
            id: self.id,
            children: Arc::clone(&self.children),
        }
    }

    /// Mutate children (triggers copy-on-write if shared)
    pub fn mutate_children(&mut self) -> &mut Vec<CowNode> {
        Arc::make_mut(&mut self.children)
    }

    /// Check if this tree is shared
    pub fn is_shared(&self) -> bool {
        Arc::strong_count(&self.children) > 1
    }
}

/// Shadow root with CowTree support
#[derive(Debug, Clone)]
pub struct ShadowRoot {
    /// Host element
    pub host: NodeId,
    /// Mode (open/closed)
    pub mode: ShadowRootMode,
    /// Whether focus is delegated to the first focusable element
    pub delegates_focus: bool,
    /// Slot assignment mode
    pub slot_assignment: SlotAssignmentMode,
    /// Whether this shadow root is clonable
    pub clonable: bool,
    /// Whether this shadow root is serializable
    pub serializable: bool,
    /// Children using CowTree for structural sharing
    children: Arc<Vec<NodeId>>,
    /// Slots in this shadow root
    slots: Vec<Slot>,
    /// Pending slotchange events
    pending_slotchange: Vec<String>,
    /// Whether this came from declarative shadow DOM
    pub declarative: bool,
}

/// Slot element with slotchange tracking
#[derive(Debug, Clone)]
pub struct Slot {
    /// Slot name (empty string for default slot)
    pub name: String,
    /// Node ID of the slot element
    pub node_id: NodeId,
    /// Assigned nodes (light DOM children with matching slot attribute)
    pub assigned_nodes: Vec<NodeId>,
    /// Previous assigned nodes (for slotchange detection)
    prev_assigned: Vec<NodeId>,
    /// Fallback content (shown when no nodes are assigned)
    pub fallback_content: Vec<NodeId>,
}

impl ShadowRoot {
    /// Create a new shadow root
    pub fn new(host: NodeId, mode: ShadowRootMode) -> Self {
        Self {
            host,
            mode,
            delegates_focus: false,
            slot_assignment: SlotAssignmentMode::Named,
            clonable: false,
            serializable: false,
            children: Arc::new(Vec::new()),
            slots: Vec::new(),
            pending_slotchange: Vec::new(),
            declarative: false,
        }
    }

    /// Create from init options
    pub fn from_init(host: NodeId, init: ShadowRootInit) -> Self {
        Self {
            host,
            mode: init.mode,
            delegates_focus: init.delegates_focus,
            slot_assignment: init.slot_assignment,
            clonable: init.clonable,
            serializable: init.serializable,
            children: Arc::new(Vec::new()),
            slots: Vec::new(),
            pending_slotchange: Vec::new(),
            declarative: false,
        }
    }

    /// Create a declarative shadow root
    pub fn declarative(host: NodeId, mode: ShadowRootMode) -> Self {
        let mut root = Self::new(host, mode);
        root.declarative = true;
        root.clonable = true;
        root
    }

    // --- Child management with COW ---

    /// Get children (shared reference)
    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    /// Append a child (triggers COW if shared)
    pub fn append_child(&mut self, child: NodeId) {
        Arc::make_mut(&mut self.children).push(child);
    }

    /// Insert child at index
    pub fn insert_child(&mut self, index: usize, child: NodeId) {
        let children = Arc::make_mut(&mut self.children);
        if index <= children.len() {
            children.insert(index, child);
        }
    }

    /// Remove a child by ID
    pub fn remove_child(&mut self, child: NodeId) -> bool {
        let children = Arc::make_mut(&mut self.children);
        if let Some(pos) = children.iter().position(|&id| id == child) {
            children.remove(pos);
            true
        } else {
            false
        }
    }

    /// Clone with structural sharing (COW)
    pub fn cow_clone(&self) -> Self {
        Self {
            host: self.host,
            mode: self.mode,
            delegates_focus: self.delegates_focus,
            slot_assignment: self.slot_assignment,
            clonable: self.clonable,
            serializable: self.serializable,
            children: Arc::clone(&self.children),
            slots: self.slots.clone(),
            pending_slotchange: Vec::new(),
            declarative: self.declarative,
        }
    }

    /// Check if children are shared with another shadow root
    pub fn is_shared(&self) -> bool {
        Arc::strong_count(&self.children) > 1
    }

    // --- Slot management ---

    /// Find slot by name
    pub fn get_slot(&self, name: &str) -> Option<&Slot> {
        self.slots.iter().find(|s| s.name == name)
    }

    /// Find slot by name (mutable)
    pub fn get_slot_mut(&mut self, name: &str) -> Option<&mut Slot> {
        self.slots.iter_mut().find(|s| s.name == name)
    }

    /// Add a slot
    pub fn add_slot(&mut self, slot: Slot) {
        // Check for duplicate
        if !self.slots.iter().any(|s| s.name == slot.name) {
            self.slots.push(slot);
        }
    }

    /// Remove a slot by name
    pub fn remove_slot(&mut self, name: &str) -> Option<Slot> {
        let pos = self.slots.iter().position(|s| s.name == name);
        pos.map(|i| self.slots.remove(i))
    }

    /// Get all slots
    pub fn slots(&self) -> &[Slot] {
        &self.slots
    }

    /// Get assigned nodes for a slot
    pub fn assigned_nodes(&self, slot_name: &str) -> Vec<NodeId> {
        self.get_slot(slot_name)
            .map(|s| s.assigned_nodes.clone())
            .unwrap_or_default()
    }

    /// Get assigned nodes with flatten option
    pub fn assigned_nodes_flattened(&self, slot_name: &str) -> Vec<NodeId> {
        if let Some(slot) = self.get_slot(slot_name) {
            if slot.assigned_nodes.is_empty() {
                // Return fallback content
                slot.fallback_content.clone()
            } else {
                slot.assigned_nodes.clone()
            }
        } else {
            Vec::new()
        }
    }

    /// Get assigned elements (filter nodes to elements only)
    pub fn assigned_elements(&self, slot_name: &str) -> Vec<NodeId> {
        // In a full implementation, this would filter by node type
        self.assigned_nodes(slot_name)
    }

    // --- Slotchange events ---

    /// Check for pending slotchange events
    pub fn take_pending_slotchange(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_slotchange)
    }

    /// Check if there are pending slotchange events
    pub fn has_pending_slotchange(&self) -> bool {
        !self.pending_slotchange.is_empty()
    }
}

impl Slot {
    /// Create a new named slot
    pub fn new(name: impl Into<String>, node_id: NodeId) -> Self {
        Self {
            name: name.into(),
            node_id,
            assigned_nodes: Vec::new(),
            prev_assigned: Vec::new(),
            fallback_content: Vec::new(),
        }
    }

    /// Create the default (unnamed) slot
    pub fn default_slot(node_id: NodeId) -> Self {
        Self::new("", node_id)
    }

    /// Assign a node to this slot
    pub fn assign(&mut self, node: NodeId) -> bool {
        if !self.assigned_nodes.contains(&node) {
            self.assigned_nodes.push(node);
            true
        } else {
            false
        }
    }

    /// Remove a node from this slot
    pub fn unassign(&mut self, node: NodeId) -> bool {
        if let Some(pos) = self.assigned_nodes.iter().position(|&id| id == node) {
            self.assigned_nodes.remove(pos);
            true
        } else {
            false
        }
    }

    /// Clear all assigned nodes
    pub fn clear_assigned(&mut self) {
        self.assigned_nodes.clear();
    }

    /// Check if slotchange should fire
    pub fn check_slotchange(&mut self) -> bool {
        let changed = self.assigned_nodes != self.prev_assigned;
        if changed {
            self.prev_assigned = self.assigned_nodes.clone();
        }
        changed
    }

    /// Add fallback content
    pub fn add_fallback(&mut self, node: NodeId) {
        self.fallback_content.push(node);
    }

    /// Get effective content (assigned or fallback)
    pub fn effective_content(&self) -> &[NodeId] {
        if self.assigned_nodes.is_empty() {
            &self.fallback_content
        } else {
            &self.assigned_nodes
        }
    }

    /// Check if slot is using fallback content
    pub fn is_using_fallback(&self) -> bool {
        self.assigned_nodes.is_empty() && !self.fallback_content.is_empty()
    }
}

/// Element that can have shadow
pub trait Shadowable {
    /// Attach shadow root
    fn attach_shadow(&mut self, init: ShadowRootInit) -> Result<&mut ShadowRoot, ShadowError>;

    /// Get shadow root (returns None for closed mode if not internal)
    fn shadow_root(&self) -> Option<&ShadowRoot>;

    /// Get shadow root mutably
    fn shadow_root_mut(&mut self) -> Option<&mut ShadowRoot>;

    /// Check if element can have shadow
    fn can_attach_shadow(&self) -> bool;
}

/// Shadow DOM errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShadowError {
    /// Element already has a shadow root
    AlreadyAttached,
    /// Element cannot have shadow root attached
    NotSupported,
    /// Invalid mode for this operation
    InvalidMode,
}

impl std::fmt::Display for ShadowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyAttached => write!(f, "Element already has a shadow root"),
            Self::NotSupported => write!(f, "Element does not support shadow root"),
            Self::InvalidMode => write!(f, "Invalid shadow root mode"),
        }
    }
}

impl std::error::Error for ShadowError {}

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
    fn test_shadow_root_cow() {
        let mut shadow1 = ShadowRoot::new(NodeId(1), ShadowRootMode::Open);
        shadow1.append_child(NodeId(2));
        shadow1.append_child(NodeId(3));

        // Clone with structural sharing
        let shadow2 = shadow1.cow_clone();
        assert!(shadow1.is_shared());
        assert!(shadow2.is_shared());

        // Modify original (triggers COW)
        shadow1.append_child(NodeId(4));
        assert_eq!(shadow1.children().len(), 3);
        assert_eq!(shadow2.children().len(), 2);
    }

    #[test]
    fn test_slot() {
        let mut slot = Slot::new("header", NodeId(5));
        slot.assign(NodeId(10));
        slot.assign(NodeId(11));

        assert_eq!(slot.assigned_nodes.len(), 2);
        assert!(!slot.assign(NodeId(10))); // Duplicate
    }

    #[test]
    fn test_slot_fallback() {
        let mut slot = Slot::new("content", NodeId(5));
        slot.add_fallback(NodeId(100));
        slot.add_fallback(NodeId(101));

        assert!(slot.is_using_fallback());
        assert_eq!(slot.effective_content(), &[NodeId(100), NodeId(101)]);

        slot.assign(NodeId(200));
        assert!(!slot.is_using_fallback());
        assert_eq!(slot.effective_content(), &[NodeId(200)]);
    }

    #[test]
    fn test_slotchange_detection() {
        let mut slot = Slot::new("test", NodeId(5));
        
        // No change initially
        assert!(!slot.check_slotchange());

        // Assign node - should trigger change
        slot.assign(NodeId(10));
        assert!(slot.check_slotchange());

        // No change after check
        assert!(!slot.check_slotchange());

        // Assign another - should trigger change
        slot.assign(NodeId(11));
        assert!(slot.check_slotchange());
    }

    #[test]
    fn test_declarative_shadow() {
        let shadow = ShadowRoot::declarative(NodeId(1), ShadowRootMode::Open);
        
        assert!(shadow.declarative);
        assert!(shadow.clonable);
    }

    #[test]
    fn test_shadow_root_from_init() {
        let init = ShadowRootInit {
            mode: ShadowRootMode::Closed,
            delegates_focus: true,
            slot_assignment: SlotAssignmentMode::Manual,
            clonable: true,
            serializable: true,
        };

        let shadow = ShadowRoot::from_init(NodeId(1), init);
        
        assert_eq!(shadow.mode, ShadowRootMode::Closed);
        assert!(shadow.delegates_focus);
        assert_eq!(shadow.slot_assignment, SlotAssignmentMode::Manual);
        assert!(shadow.clonable);
        assert!(shadow.serializable);
    }
}
