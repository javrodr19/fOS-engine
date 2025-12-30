//! Intersection Observer API
//!
//! Observe element visibility and intersection with viewport.

use std::collections::HashMap;
use fos_dom::NodeId;

/// Intersection observer options
#[derive(Debug, Clone)]
pub struct IntersectionObserverOptions {
    /// Root element (None = viewport)
    pub root: Option<NodeId>,
    /// Root margin
    pub root_margin: String,
    /// Thresholds to trigger callback
    pub threshold: Vec<f32>,
}

impl Default for IntersectionObserverOptions {
    fn default() -> Self {
        Self {
            root: None,
            root_margin: "0px".to_string(),
            threshold: vec![0.0],
        }
    }
}

/// Intersection observer entry
#[derive(Debug, Clone)]
pub struct IntersectionObserverEntry {
    pub target: NodeId,
    pub bounding_client_rect: DOMRect,
    pub intersection_rect: DOMRect,
    pub root_bounds: Option<DOMRect>,
    pub intersection_ratio: f32,
    pub is_intersecting: bool,
    pub time: f64,
}

/// DOM rect
#[derive(Debug, Clone, Copy, Default)]
pub struct DOMRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl DOMRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self { x, y, width, height }
    }
    
    pub fn top(&self) -> f32 { self.y }
    pub fn left(&self) -> f32 { self.x }
    pub fn right(&self) -> f32 { self.x + self.width }
    pub fn bottom(&self) -> f32 { self.y + self.height }
    
    /// Calculate intersection with another rect
    pub fn intersect(&self, other: &DOMRect) -> Option<DOMRect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        
        if right > x && bottom > y {
            Some(DOMRect {
                x,
                y,
                width: right - x,
                height: bottom - y,
            })
        } else {
            None
        }
    }
    
    pub fn area(&self) -> f32 {
        self.width * self.height
    }
}

/// Intersection observer
#[derive(Debug)]
pub struct IntersectionObserver {
    id: u64,
    options: IntersectionObserverOptions,
    observed: HashMap<NodeId, Option<f32>>, // Last ratio
    pending_entries: Vec<IntersectionObserverEntry>,
}

static mut NEXT_INTERSECTION_ID: u64 = 1;

impl IntersectionObserver {
    pub fn new(options: IntersectionObserverOptions) -> Self {
        let id = unsafe {
            let id = NEXT_INTERSECTION_ID;
            NEXT_INTERSECTION_ID += 1;
            id
        };
        Self {
            id,
            options,
            observed: HashMap::new(),
            pending_entries: Vec::new(),
        }
    }
    
    pub fn id(&self) -> u64 {
        self.id
    }
    
    /// Observe an element
    pub fn observe(&mut self, target: NodeId) {
        self.observed.insert(target, None);
    }
    
    /// Stop observing
    pub fn unobserve(&mut self, target: NodeId) {
        self.observed.remove(&target);
    }
    
    /// Disconnect all
    pub fn disconnect(&mut self) {
        self.observed.clear();
        self.pending_entries.clear();
    }
    
    /// Check intersections
    pub fn check_intersections(
        &mut self,
        viewport: DOMRect,
        element_rects: &HashMap<NodeId, DOMRect>,
        time: f64,
    ) {
        for (node, last_ratio) in &mut self.observed {
            if let Some(rect) = element_rects.get(node) {
                let intersection = rect.intersect(&viewport);
                let ratio = intersection
                    .map(|i| i.area() / rect.area())
                    .unwrap_or(0.0);
                
                // Check if crossed threshold
                let should_notify = match *last_ratio {
                    Some(lr) => {
                        self.options.threshold.iter().any(|&t| {
                            (lr < t && ratio >= t) || (lr >= t && ratio < t)
                        })
                    }
                    None => true,
                };
                
                if should_notify {
                    *last_ratio = Some(ratio);
                    
                    self.pending_entries.push(IntersectionObserverEntry {
                        target: *node,
                        bounding_client_rect: *rect,
                        intersection_rect: intersection.unwrap_or_default(),
                        root_bounds: Some(viewport),
                        intersection_ratio: ratio,
                        is_intersecting: ratio > 0.0,
                        time,
                    });
                }
            }
        }
    }
    
    /// Take pending entries
    pub fn take_entries(&mut self) -> Vec<IntersectionObserverEntry> {
        std::mem::take(&mut self.pending_entries)
    }
    
    pub fn has_pending(&self) -> bool {
        !self.pending_entries.is_empty()
    }
}

/// Intersection observer manager
#[derive(Debug, Default)]
pub struct IntersectionObserverManager {
    observers: Vec<IntersectionObserver>,
}

impl IntersectionObserverManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create observer
    pub fn create(&mut self, options: IntersectionObserverOptions) -> u64 {
        let observer = IntersectionObserver::new(options);
        let id = observer.id();
        self.observers.push(observer);
        id
    }
    
    /// Get observer
    pub fn get(&mut self, id: u64) -> Option<&mut IntersectionObserver> {
        self.observers.iter_mut().find(|o| o.id() == id)
    }
    
    /// Remove observer
    pub fn remove(&mut self, id: u64) {
        self.observers.retain(|o| o.id() != id);
    }
    
    /// Process all observers
    pub fn process(
        &mut self,
        viewport: DOMRect,
        element_rects: &HashMap<NodeId, DOMRect>,
        time: f64,
    ) -> Vec<(u64, Vec<IntersectionObserverEntry>)> {
        let mut results = Vec::new();
        for observer in &mut self.observers {
            observer.check_intersections(viewport, element_rects, time);
            if observer.has_pending() {
                results.push((observer.id(), observer.take_entries()));
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_intersection_observer() {
        let mut observer = IntersectionObserver::new(IntersectionObserverOptions::default());
        let node = NodeId::from_raw_parts(1, 0);
        
        observer.observe(node);
        
        let viewport = DOMRect::new(0.0, 0.0, 800.0, 600.0);
        let mut rects = HashMap::new();
        rects.insert(node, DOMRect::new(100.0, 100.0, 200.0, 200.0));
        
        observer.check_intersections(viewport, &rects, 0.0);
        assert!(observer.has_pending());
        
        let entries = observer.take_entries();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].is_intersecting);
    }
}
