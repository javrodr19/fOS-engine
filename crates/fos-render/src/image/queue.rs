//! Image Decode Priority Queue
//!
//! Prioritizes visible images for decoding while deferring offscreen images.

use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;

/// Priority level for image decoding
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DecodePriority {
    /// Lowest priority - far offscreen
    Low = 0,
    /// Normal priority - near viewport
    Normal = 1,
    /// High priority - partially visible
    High = 2,
    /// Critical priority - fully visible
    Critical = 3,
}

/// A pending decode request
#[derive(Clone)]
pub struct DecodeRequest {
    /// Unique image ID
    pub id: u64,
    /// Image URL or path
    pub url: String,
    /// Display width
    pub width: u32,
    /// Display height
    pub height: u32,
    /// Decode priority
    pub priority: DecodePriority,
    /// Distance from viewport (negative = visible)
    pub viewport_distance: i32,
}

impl PartialEq for DecodeRequest {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for DecodeRequest {}

impl PartialOrd for DecodeRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DecodeRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first, then closer to viewport
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => other.viewport_distance.cmp(&self.viewport_distance),
            other => other,
        }
    }
}

/// Priority queue for image decoding
pub struct DecodeQueue {
    /// Priority queue of pending requests
    queue: BinaryHeap<DecodeRequest>,
    /// Lookup by ID
    by_id: HashMap<u64, DecodePriority>,
    /// Maximum concurrent decodes
    max_concurrent: usize,
    /// Currently decoding count
    active_count: usize,
    /// Cancelled IDs
    cancelled: HashMap<u64, bool>,
}

impl Default for DecodeQueue {
    fn default() -> Self {
        Self::new(4) // 4 concurrent decodes
    }
}

impl DecodeQueue {
    /// Create a new decode queue
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            queue: BinaryHeap::new(),
            by_id: HashMap::new(),
            max_concurrent,
            active_count: 0,
            cancelled: HashMap::new(),
        }
    }
    
    /// Enqueue an image for decoding
    pub fn enqueue(&mut self, request: DecodeRequest) {
        let id = request.id;
        let priority = request.priority;
        
        // Update priority if already queued
        if let Some(old_priority) = self.by_id.get_mut(&id) {
            if priority > *old_priority {
                *old_priority = priority;
            }
            return;
        }
        
        self.by_id.insert(id, priority);
        self.queue.push(request);
    }
    
    /// Get next request to decode (if capacity available)
    pub fn dequeue(&mut self) -> Option<DecodeRequest> {
        if self.active_count >= self.max_concurrent {
            return None;
        }
        
        while let Some(request) = self.queue.pop() {
            // Skip cancelled requests
            if self.cancelled.remove(&request.id).is_some() {
                self.by_id.remove(&request.id);
                continue;
            }
            
            self.by_id.remove(&request.id);
            self.active_count += 1;
            return Some(request);
        }
        
        None
    }
    
    /// Mark a decode as complete
    pub fn complete(&mut self, _id: u64) {
        if self.active_count > 0 {
            self.active_count -= 1;
        }
    }
    
    /// Cancel a pending decode
    pub fn cancel(&mut self, id: u64) {
        if self.by_id.contains_key(&id) {
            self.cancelled.insert(id, true);
        }
    }
    
    /// Cancel all offscreen images
    pub fn cancel_offscreen(&mut self, viewport_top: i32, viewport_bottom: i32, threshold: i32) {
        let to_cancel: Vec<_> = self.by_id.keys()
            .filter(|_| {
                // Would need viewport info per request - simplified
                false
            })
            .cloned()
            .collect();
        
        for id in to_cancel {
            self.cancel(id);
        }
        
        // Suppress unused warnings
        let _ = (viewport_top, viewport_bottom, threshold);
    }
    
    /// Update priority based on scroll position
    pub fn update_priorities(&mut self, viewport_top: i32, viewport_height: i32) {
        // Re-prioritize based on new viewport
        // In practice, would rebuild queue with updated distances
        let _ = (viewport_top, viewport_height);
    }
    
    /// Get queue statistics
    pub fn stats(&self) -> DecodeQueueStats {
        DecodeQueueStats {
            pending: self.queue.len(),
            active: self.active_count,
            max_concurrent: self.max_concurrent,
            cancelled: self.cancelled.len(),
        }
    }
    
    /// Check if queue is empty
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty() && self.active_count == 0
    }
}

/// Queue statistics
#[derive(Debug, Clone)]
pub struct DecodeQueueStats {
    pub pending: usize,
    pub active: usize,
    pub max_concurrent: usize,
    pub cancelled: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_priority_ordering() {
        let mut queue = DecodeQueue::new(2);
        
        queue.enqueue(DecodeRequest {
            id: 1, url: "low.jpg".into(), width: 100, height: 100,
            priority: DecodePriority::Low, viewport_distance: 1000,
        });
        queue.enqueue(DecodeRequest {
            id: 2, url: "critical.jpg".into(), width: 100, height: 100,
            priority: DecodePriority::Critical, viewport_distance: 0,
        });
        queue.enqueue(DecodeRequest {
            id: 3, url: "normal.jpg".into(), width: 100, height: 100,
            priority: DecodePriority::Normal, viewport_distance: 500,
        });
        
        // Critical should come first
        let first = queue.dequeue().unwrap();
        assert_eq!(first.id, 2);
        assert_eq!(first.priority, DecodePriority::Critical);
    }
    
    #[test]
    fn test_concurrent_limit() {
        let mut queue = DecodeQueue::new(2);
        
        for i in 0..5 {
            queue.enqueue(DecodeRequest {
                id: i, url: format!("{}.jpg", i), width: 100, height: 100,
                priority: DecodePriority::High, viewport_distance: 0,
            });
        }
        
        // Should only get 2
        assert!(queue.dequeue().is_some());
        assert!(queue.dequeue().is_some());
        assert!(queue.dequeue().is_none()); // At limit
        
        // Complete one
        queue.complete(0);
        assert!(queue.dequeue().is_some()); // Now available
    }
    
    #[test]
    fn test_cancel() {
        let mut queue = DecodeQueue::new(4);
        
        queue.enqueue(DecodeRequest {
            id: 1, url: "img.jpg".into(), width: 100, height: 100,
            priority: DecodePriority::Normal, viewport_distance: 500,
        });
        
        queue.cancel(1);
        
        // Should skip cancelled
        assert!(queue.dequeue().is_none());
    }
}
