//! Resize Observer API
//!
//! Observe element size changes.

use std::collections::HashMap;
use fos_dom::NodeId;

/// Observed element size
#[derive(Debug, Clone, Copy)]
pub struct ResizeObserverSize {
    pub inline_size: f32,
    pub block_size: f32,
}

/// Content box vs border box
#[derive(Debug, Clone, Copy, Default)]
pub enum ResizeObserverBoxOptions {
    #[default]
    ContentBox,
    BorderBox,
    DevicePixelContentBox,
}

/// Resize observer entry
#[derive(Debug, Clone)]
pub struct ResizeObserverEntry {
    pub target: NodeId,
    pub content_rect: ContentRect,
    pub content_box_size: Vec<ResizeObserverSize>,
    pub border_box_size: Vec<ResizeObserverSize>,
    pub device_pixel_content_box_size: Vec<ResizeObserverSize>,
}

/// Content rectangle
#[derive(Debug, Clone, Copy, Default)]
pub struct ContentRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Resize observer
#[derive(Debug, Default)]
pub struct ResizeObserver {
    id: u64,
    observed: HashMap<NodeId, ObservedElement>,
    pending_entries: Vec<ResizeObserverEntry>,
}

#[derive(Debug)]
struct ObservedElement {
    box_options: ResizeObserverBoxOptions,
    last_size: Option<(f32, f32)>,
}

static mut NEXT_OBSERVER_ID: u64 = 1;

impl ResizeObserver {
    pub fn new() -> Self {
        let id = unsafe {
            let id = NEXT_OBSERVER_ID;
            NEXT_OBSERVER_ID += 1;
            id
        };
        Self {
            id,
            observed: HashMap::new(),
            pending_entries: Vec::new(),
        }
    }
    
    pub fn id(&self) -> u64 {
        self.id
    }
    
    /// Observe an element
    pub fn observe(&mut self, target: NodeId, options: ResizeObserverBoxOptions) {
        self.observed.insert(target, ObservedElement {
            box_options: options,
            last_size: None,
        });
    }
    
    /// Stop observing an element
    pub fn unobserve(&mut self, target: NodeId) {
        self.observed.remove(&target);
    }
    
    /// Disconnect all observations
    pub fn disconnect(&mut self) {
        self.observed.clear();
        self.pending_entries.clear();
    }
    
    /// Check for size changes
    pub fn check_sizes(&mut self, sizes: &HashMap<NodeId, (f32, f32)>) {
        for (node, observed) in &mut self.observed {
            if let Some(&(width, height)) = sizes.get(node) {
                let changed = match observed.last_size {
                    Some((lw, lh)) => (lw - width).abs() > 0.01 || (lh - height).abs() > 0.01,
                    None => true,
                };
                
                if changed {
                    observed.last_size = Some((width, height));
                    
                    let size = ResizeObserverSize {
                        inline_size: width,
                        block_size: height,
                    };
                    
                    self.pending_entries.push(ResizeObserverEntry {
                        target: *node,
                        content_rect: ContentRect {
                            x: 0.0,
                            y: 0.0,
                            width,
                            height,
                            top: 0.0,
                            right: width,
                            bottom: height,
                            left: 0.0,
                        },
                        content_box_size: vec![size],
                        border_box_size: vec![size],
                        device_pixel_content_box_size: vec![size],
                    });
                }
            }
        }
    }
    
    /// Get pending entries and clear
    pub fn take_entries(&mut self) -> Vec<ResizeObserverEntry> {
        std::mem::take(&mut self.pending_entries)
    }
    
    /// Check if has pending entries
    pub fn has_pending(&self) -> bool {
        !self.pending_entries.is_empty()
    }
}

/// Resize observer manager
#[derive(Debug, Default)]
pub struct ResizeObserverManager {
    observers: Vec<ResizeObserver>,
}

impl ResizeObserverManager {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create new observer
    pub fn create(&mut self) -> u64 {
        let observer = ResizeObserver::new();
        let id = observer.id();
        self.observers.push(observer);
        id
    }
    
    /// Get observer by ID
    pub fn get(&mut self, id: u64) -> Option<&mut ResizeObserver> {
        self.observers.iter_mut().find(|o| o.id() == id)
    }
    
    /// Remove observer
    pub fn remove(&mut self, id: u64) {
        self.observers.retain(|o| o.id() != id);
    }
    
    /// Process all observers
    pub fn process(&mut self, sizes: &HashMap<NodeId, (f32, f32)>) -> Vec<(u64, Vec<ResizeObserverEntry>)> {
        let mut results = Vec::new();
        for observer in &mut self.observers {
            observer.check_sizes(sizes);
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
    fn test_resize_observer() {
        let mut observer = ResizeObserver::new();
        let node = NodeId::from_raw_parts(1, 0);
        
        observer.observe(node, ResizeObserverBoxOptions::ContentBox);
        
        let mut sizes = HashMap::new();
        sizes.insert(node, (100.0, 200.0));
        
        observer.check_sizes(&sizes);
        assert!(observer.has_pending());
        
        let entries = observer.take_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content_rect.width, 100.0);
    }
}
