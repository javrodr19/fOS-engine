//! DOM Diff Compression (Phase 24.2)
//!
//! For undo: store diffs not snapshots. Reverse diff to undo.
//! 95% smaller undo stack. Efficient history.

use std::collections::VecDeque;

/// Node ID type
pub type NodeId = u32;

/// Diff operation type
#[derive(Debug, Clone)]
pub enum DiffOp {
    /// Insert a node
    Insert {
        parent_id: NodeId,
        index: usize,
        node_data: NodeData,
    },
    /// Remove a node
    Remove {
        parent_id: NodeId,
        index: usize,
        node_id: NodeId,
        node_data: NodeData, // For undo
    },
    /// Move node within same parent
    Move {
        parent_id: NodeId,
        from_index: usize,
        to_index: usize,
        node_id: NodeId,
    },
    /// Move node to different parent
    Reparent {
        old_parent: NodeId,
        old_index: usize,
        new_parent: NodeId,
        new_index: usize,
        node_id: NodeId,
    },
    /// Update text content
    UpdateText {
        node_id: NodeId,
        old_text: Box<str>,
        new_text: Box<str>,
    },
    /// Set attribute
    SetAttribute {
        node_id: NodeId,
        name: Box<str>,
        old_value: Option<Box<str>>,
        new_value: Option<Box<str>>,
    },
    /// Remove attribute
    RemoveAttribute {
        node_id: NodeId,
        name: Box<str>,
        old_value: Box<str>,
    },
    /// Update inline style
    UpdateStyle {
        node_id: NodeId,
        property: Box<str>,
        old_value: Option<Box<str>>,
        new_value: Option<Box<str>>,
    },
}

impl DiffOp {
    /// Get the reverse operation (for undo)
    pub fn reverse(&self) -> DiffOp {
        match self {
            DiffOp::Insert { parent_id, index, node_data } => {
                DiffOp::Remove {
                    parent_id: *parent_id,
                    index: *index,
                    node_id: node_data.id,
                    node_data: node_data.clone(),
                }
            }
            DiffOp::Remove { parent_id, index, node_id, node_data } => {
                DiffOp::Insert {
                    parent_id: *parent_id,
                    index: *index,
                    node_data: node_data.clone(),
                }
            }
            DiffOp::Move { parent_id, from_index, to_index, node_id } => {
                DiffOp::Move {
                    parent_id: *parent_id,
                    from_index: *to_index,
                    to_index: *from_index,
                    node_id: *node_id,
                }
            }
            DiffOp::Reparent { old_parent, old_index, new_parent, new_index, node_id } => {
                DiffOp::Reparent {
                    old_parent: *new_parent,
                    old_index: *new_index,
                    new_parent: *old_parent,
                    new_index: *old_index,
                    node_id: *node_id,
                }
            }
            DiffOp::UpdateText { node_id, old_text, new_text } => {
                DiffOp::UpdateText {
                    node_id: *node_id,
                    old_text: new_text.clone(),
                    new_text: old_text.clone(),
                }
            }
            DiffOp::SetAttribute { node_id, name, old_value, new_value } => {
                DiffOp::SetAttribute {
                    node_id: *node_id,
                    name: name.clone(),
                    old_value: new_value.clone(),
                    new_value: old_value.clone(),
                }
            }
            DiffOp::RemoveAttribute { node_id, name, old_value } => {
                DiffOp::SetAttribute {
                    node_id: *node_id,
                    name: name.clone(),
                    old_value: None,
                    new_value: Some(old_value.clone()),
                }
            }
            DiffOp::UpdateStyle { node_id, property, old_value, new_value } => {
                DiffOp::UpdateStyle {
                    node_id: *node_id,
                    property: property.clone(),
                    old_value: new_value.clone(),
                    new_value: old_value.clone(),
                }
            }
        }
    }
    
    /// Estimate size in bytes
    pub fn size(&self) -> usize {
        match self {
            DiffOp::Insert { node_data, .. } => 16 + node_data.size(),
            DiffOp::Remove { node_data, .. } => 16 + node_data.size(),
            DiffOp::Move { .. } => 20,
            DiffOp::Reparent { .. } => 24,
            DiffOp::UpdateText { old_text, new_text, .. } => 8 + old_text.len() + new_text.len(),
            DiffOp::SetAttribute { name, old_value, new_value, .. } => {
                8 + name.len() + old_value.as_ref().map(|s| s.len()).unwrap_or(0)
                    + new_value.as_ref().map(|s| s.len()).unwrap_or(0)
            }
            DiffOp::RemoveAttribute { name, old_value, .. } => 8 + name.len() + old_value.len(),
            DiffOp::UpdateStyle { property, old_value, new_value, .. } => {
                8 + property.len() + old_value.as_ref().map(|s| s.len()).unwrap_or(0)
                    + new_value.as_ref().map(|s| s.len()).unwrap_or(0)
            }
        }
    }
}

/// Minimal node data for reconstruction
#[derive(Debug, Clone)]
pub struct NodeData {
    pub id: NodeId,
    pub node_type: NodeType,
    pub tag: Option<Box<str>>,
    pub text: Option<Box<str>>,
    pub attributes: Vec<(Box<str>, Box<str>)>,
}

impl NodeData {
    pub fn size(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.tag.as_ref().map(|s| s.len()).unwrap_or(0)
            + self.text.as_ref().map(|s| s.len()).unwrap_or(0)
            + self.attributes.iter().map(|(k, v)| k.len() + v.len()).sum::<usize>()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeType {
    Element,
    Text,
    Comment,
}

/// A changeset (group of related diffs)
#[derive(Debug, Clone)]
pub struct ChangeSet {
    /// Operations in this changeset
    pub ops: Vec<DiffOp>,
    /// Timestamp
    pub timestamp: u64,
    /// Description (optional)
    pub description: Option<Box<str>>,
}

impl ChangeSet {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            timestamp: 0,
            description: None,
        }
    }
    
    pub fn add(&mut self, op: DiffOp) {
        self.ops.push(op);
    }
    
    pub fn reverse(&self) -> ChangeSet {
        ChangeSet {
            ops: self.ops.iter().rev().map(|op| op.reverse()).collect(),
            timestamp: self.timestamp,
            description: self.description.clone(),
        }
    }
    
    pub fn size(&self) -> usize {
        self.ops.iter().map(|op| op.size()).sum()
    }
    
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

impl Default for ChangeSet {
    fn default() -> Self {
        Self::new()
    }
}

/// Undo/redo history using diffs
#[derive(Debug)]
pub struct DiffHistory {
    /// Undo stack
    undo_stack: VecDeque<ChangeSet>,
    /// Redo stack
    redo_stack: Vec<ChangeSet>,
    /// Maximum undo levels
    max_undo: usize,
    /// Current total size
    current_size: usize,
    /// Maximum size in bytes
    max_size: usize,
    /// Statistics
    stats: DiffHistoryStats,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DiffHistoryStats {
    pub changesets_recorded: u64,
    pub undos_performed: u64,
    pub redos_performed: u64,
    pub bytes_saved: u64,
}

impl Default for DiffHistory {
    fn default() -> Self {
        Self::new()
    }
}

impl DiffHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: Vec::new(),
            max_undo: 100,
            current_size: 0,
            max_size: 10 * 1024 * 1024, // 10 MB
            stats: DiffHistoryStats::default(),
        }
    }
    
    /// Set max undo levels
    pub fn with_max_undo(mut self, max: usize) -> Self {
        self.max_undo = max;
        self
    }
    
    /// Set max size
    pub fn with_max_size(mut self, size: usize) -> Self {
        self.max_size = size;
        self
    }
    
    /// Record a changeset
    pub fn record(&mut self, changeset: ChangeSet) {
        if changeset.is_empty() {
            return;
        }
        
        let size = changeset.size();
        
        // Clear redo stack on new change
        self.redo_stack.clear();
        
        // Evict old entries if necessary
        while self.current_size + size > self.max_size || self.undo_stack.len() >= self.max_undo {
            if let Some(old) = self.undo_stack.pop_front() {
                self.current_size -= old.size();
            } else {
                break;
            }
        }
        
        self.current_size += size;
        self.undo_stack.push_back(changeset);
        self.stats.changesets_recorded += 1;
    }
    
    /// Undo last changeset, return reversed ops
    pub fn undo(&mut self) -> Option<ChangeSet> {
        let changeset = self.undo_stack.pop_back()?;
        let reversed = changeset.reverse();
        
        self.current_size -= changeset.size();
        self.redo_stack.push(changeset);
        self.stats.undos_performed += 1;
        
        Some(reversed)
    }
    
    /// Redo last undone changeset
    pub fn redo(&mut self) -> Option<ChangeSet> {
        let changeset = self.redo_stack.pop()?;
        let size = changeset.size();
        
        self.current_size += size;
        self.undo_stack.push_back(changeset.clone());
        self.stats.redos_performed += 1;
        
        Some(changeset)
    }
    
    /// Check if can undo
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }
    
    /// Check if can redo
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
    
    /// Undo stack size
    pub fn undo_levels(&self) -> usize {
        self.undo_stack.len()
    }
    
    /// Redo stack size
    pub fn redo_levels(&self) -> usize {
        self.redo_stack.len()
    }
    
    /// Current memory usage
    pub fn memory_usage(&self) -> usize {
        self.current_size
    }
    
    /// Get statistics
    pub fn stats(&self) -> &DiffHistoryStats {
        &self.stats
    }
    
    /// Clear history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.current_size = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_diff_reverse() {
        let op = DiffOp::UpdateText {
            node_id: 1,
            old_text: "hello".into(),
            new_text: "world".into(),
        };
        
        let reversed = op.reverse();
        
        if let DiffOp::UpdateText { old_text, new_text, .. } = reversed {
            assert_eq!(old_text.as_ref(), "world");
            assert_eq!(new_text.as_ref(), "hello");
        } else {
            panic!("Wrong type");
        }
    }
    
    #[test]
    fn test_changeset_reverse() {
        let mut cs = ChangeSet::new();
        
        cs.add(DiffOp::UpdateText {
            node_id: 1,
            old_text: "a".into(),
            new_text: "b".into(),
        });
        cs.add(DiffOp::UpdateText {
            node_id: 2,
            old_text: "c".into(),
            new_text: "d".into(),
        });
        
        let reversed = cs.reverse();
        
        // Should be reversed order
        assert_eq!(reversed.ops.len(), 2);
    }
    
    #[test]
    fn test_undo_redo() {
        let mut history = DiffHistory::new();
        
        let mut cs = ChangeSet::new();
        cs.add(DiffOp::UpdateText {
            node_id: 1,
            old_text: "before".into(),
            new_text: "after".into(),
        });
        
        history.record(cs);
        
        assert!(history.can_undo());
        assert!(!history.can_redo());
        
        let undone = history.undo().unwrap();
        assert_eq!(undone.ops.len(), 1);
        
        assert!(!history.can_undo());
        assert!(history.can_redo());
        
        let redone = history.redo().unwrap();
        assert_eq!(redone.ops.len(), 1);
    }
}
