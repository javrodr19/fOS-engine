//! DOM Observers
//!
//! MutationObserver, IntersectionObserver, ResizeObserver.

use crate::NodeId;
use crate::geometry::DOMRect;

/// Mutation observer
#[derive(Debug)]
pub struct MutationObserver {
    callback_id: u32,
    options: MutationObserverInit,
    observed: Vec<NodeId>,
    records: Vec<MutationRecord>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationType {
    Attributes,
    CharacterData,
    ChildList,
}

impl MutationObserver {
    pub fn new(callback_id: u32) -> Self {
        Self {
            callback_id,
            options: MutationObserverInit::default(),
            observed: Vec::new(),
            records: Vec::new(),
        }
    }
    
    pub fn observe(&mut self, target: NodeId, options: MutationObserverInit) {
        self.options = options;
        if !self.observed.contains(&target) {
            self.observed.push(target);
        }
    }
    
    pub fn disconnect(&mut self) {
        self.observed.clear();
    }
    
    pub fn take_records(&mut self) -> Vec<MutationRecord> {
        std::mem::take(&mut self.records)
    }
    
    pub fn push_record(&mut self, record: MutationRecord) {
        self.records.push(record);
    }
}

/// Intersection observer
#[derive(Debug)]
pub struct IntersectionObserver {
    callback_id: u32,
    root: Option<NodeId>,
    root_margin: String,
    thresholds: Vec<f64>,
    observed: Vec<NodeId>,
}

/// Intersection observer entry
#[derive(Debug, Clone)]
pub struct IntersectionObserverEntry {
    pub target: NodeId,
    pub bounding_client_rect: DOMRect,
    pub intersection_rect: DOMRect,
    pub root_bounds: Option<DOMRect>,
    pub is_intersecting: bool,
    pub intersection_ratio: f64,
    pub time: f64,
}

impl IntersectionObserver {
    pub fn new(callback_id: u32, root: Option<NodeId>, root_margin: &str, thresholds: Vec<f64>) -> Self {
        Self {
            callback_id,
            root,
            root_margin: root_margin.to_string(),
            thresholds: if thresholds.is_empty() { vec![0.0] } else { thresholds },
            observed: Vec::new(),
        }
    }
    
    pub fn observe(&mut self, target: NodeId) {
        if !self.observed.contains(&target) {
            self.observed.push(target);
        }
    }
    
    pub fn unobserve(&mut self, target: NodeId) {
        self.observed.retain(|&id| id != target);
    }
    
    pub fn disconnect(&mut self) {
        self.observed.clear();
    }
    
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }
    
    pub fn thresholds(&self) -> &[f64] {
        &self.thresholds
    }
}

/// Resize observer
#[derive(Debug)]
pub struct ResizeObserver {
    callback_id: u32,
    observed: Vec<NodeId>,
}

/// Resize observer entry
#[derive(Debug, Clone)]
pub struct ResizeObserverEntry {
    pub target: NodeId,
    pub content_rect: DOMRect,
    pub border_box_size: Vec<ResizeObserverSize>,
    pub content_box_size: Vec<ResizeObserverSize>,
}

/// Resize observer size
#[derive(Debug, Clone, Copy)]
pub struct ResizeObserverSize {
    pub inline_size: f64,
    pub block_size: f64,
}

impl ResizeObserver {
    pub fn new(callback_id: u32) -> Self {
        Self {
            callback_id,
            observed: Vec::new(),
        }
    }
    
    pub fn observe(&mut self, target: NodeId) {
        if !self.observed.contains(&target) {
            self.observed.push(target);
        }
    }
    
    pub fn unobserve(&mut self, target: NodeId) {
        self.observed.retain(|&id| id != target);
    }
    
    pub fn disconnect(&mut self) {
        self.observed.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mutation_observer() {
        let mut observer = MutationObserver::new(1);
        observer.observe(NodeId(1), MutationObserverInit {
            child_list: true,
            attributes: true,
            ..Default::default()
        });
        
        assert_eq!(observer.observed.len(), 1);
        
        observer.disconnect();
        assert!(observer.observed.is_empty());
    }
    
    #[test]
    fn test_intersection_observer() {
        let mut observer = IntersectionObserver::new(
            1, None, "0px", vec![0.0, 0.5, 1.0]
        );
        
        observer.observe(NodeId(1));
        observer.observe(NodeId(2));
        
        assert_eq!(observer.thresholds().len(), 3);
        
        observer.unobserve(NodeId(1));
        assert_eq!(observer.observed.len(), 1);
    }
}
