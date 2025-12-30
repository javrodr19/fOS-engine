//! Mutation Observer API
//!
//! Observe DOM changes.

use fos_dom::NodeId;
use std::collections::HashMap;

/// Mutation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationType {
    Attributes,
    CharacterData,
    ChildList,
}

/// Mutation record
#[derive(Debug, Clone)]
pub struct MutationRecord {
    pub mutation_type: MutationType,
    pub target: NodeId,
    pub added_nodes: Vec<NodeId>,
    pub removed_nodes: Vec<NodeId>,
    pub previous_sibling: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub attribute_name: Option<String>,
    pub attribute_namespace: Option<String>,
    pub old_value: Option<String>,
}

/// Mutation observer options
#[derive(Debug, Clone, Default)]
pub struct MutationObserverInit {
    pub child_list: bool,
    pub attributes: bool,
    pub character_data: bool,
    pub subtree: bool,
    pub attribute_old_value: bool,
    pub character_data_old_value: bool,
    pub attribute_filter: Option<Vec<String>>,
}

/// Mutation observer
#[derive(Debug)]
pub struct MutationObserver {
    id: u64,
    observations: HashMap<NodeId, MutationObserverInit>,
    pending_records: Vec<MutationRecord>,
}

static mut NEXT_OBSERVER_ID: u64 = 1;

impl MutationObserver {
    pub fn new() -> Self {
        let id = unsafe {
            let id = NEXT_OBSERVER_ID;
            NEXT_OBSERVER_ID += 1;
            id
        };
        Self {
            id,
            observations: HashMap::new(),
            pending_records: Vec::new(),
        }
    }
    
    pub fn id(&self) -> u64 {
        self.id
    }
    
    /// Observe a target
    pub fn observe(&mut self, target: NodeId, options: MutationObserverInit) {
        self.observations.insert(target, options);
    }
    
    /// Stop observing a target
    pub fn disconnect(&mut self) {
        self.observations.clear();
        self.pending_records.clear();
    }
    
    /// Take pending records
    pub fn take_records(&mut self) -> Vec<MutationRecord> {
        std::mem::take(&mut self.pending_records)
    }
    
    /// Check if observing node
    pub fn is_observing(&self, node: NodeId) -> bool {
        self.observations.contains_key(&node)
    }
    
    /// Record a mutation
    pub fn record(&mut self, mutation: MutationRecord) {
        // Check if we should observe this mutation
        let should_record = self.observations.iter().any(|(&target, options)| {
            let matches_target = target == mutation.target;
            let matches_type = match mutation.mutation_type {
                MutationType::Attributes => options.attributes,
                MutationType::CharacterData => options.character_data,
                MutationType::ChildList => options.child_list,
            };
            
            // Check attribute filter
            let passes_filter = if mutation.mutation_type == MutationType::Attributes {
                if let (Some(ref filter), Some(ref attr)) = (&options.attribute_filter, &mutation.attribute_name) {
                    filter.contains(attr)
                } else {
                    true
                }
            } else {
                true
            };
            
            matches_target && matches_type && passes_filter
        });
        
        if should_record {
            self.pending_records.push(mutation);
        }
    }
    
    /// Has pending records
    pub fn has_pending(&self) -> bool {
        !self.pending_records.is_empty()
    }
}

impl Default for MutationObserver {
    fn default() -> Self {
        Self::new()
    }
}

/// Mutation observer manager
#[derive(Debug, Default)]
pub struct MutationObserverManager {
    observers: Vec<MutationObserver>,
}

impl MutationObserverManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create observer
    pub fn create(&mut self) -> u64 {
        let observer = MutationObserver::new();
        let id = observer.id();
        self.observers.push(observer);
        id
    }
    
    /// Get observer
    pub fn get(&mut self, id: u64) -> Option<&mut MutationObserver> {
        self.observers.iter_mut().find(|o| o.id() == id)
    }
    
    /// Remove observer
    pub fn remove(&mut self, id: u64) {
        self.observers.retain(|o| o.id() != id);
    }
    
    /// Notify all observers of a mutation
    pub fn notify(&mut self, mutation: MutationRecord) {
        for observer in &mut self.observers {
            observer.record(mutation.clone());
        }
    }
    
    /// Notify attribute change
    pub fn notify_attribute_change(&mut self, target: NodeId, name: &str, old_value: Option<String>) {
        self.notify(MutationRecord {
            mutation_type: MutationType::Attributes,
            target,
            added_nodes: Vec::new(),
            removed_nodes: Vec::new(),
            previous_sibling: None,
            next_sibling: None,
            attribute_name: Some(name.to_string()),
            attribute_namespace: None,
            old_value,
        });
    }
    
    /// Notify child list change
    pub fn notify_child_change(
        &mut self,
        target: NodeId,
        added: Vec<NodeId>,
        removed: Vec<NodeId>,
    ) {
        self.notify(MutationRecord {
            mutation_type: MutationType::ChildList,
            target,
            added_nodes: added,
            removed_nodes: removed,
            previous_sibling: None,
            next_sibling: None,
            attribute_name: None,
            attribute_namespace: None,
            old_value: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mutation_observer() {
        let mut mgr = MutationObserverManager::new();
        let id = mgr.create();
        
        let node = NodeId::from_raw_parts(1, 0);
        
        if let Some(observer) = mgr.get(id) {
            observer.observe(node, MutationObserverInit {
                attributes: true,
                ..Default::default()
            });
        }
        
        mgr.notify_attribute_change(node, "class", Some("old-class".to_string()));
        
        if let Some(observer) = mgr.get(id) {
            let records = observer.take_records();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].attribute_name, Some("class".to_string()));
        }
    }
}
