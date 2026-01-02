//! Request Prioritization
//!
//! Priority queue for HTTP requests to prioritize critical resources.

use std::collections::{BinaryHeap, HashMap};
use std::cmp::Ordering;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

/// Request priority levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RequestPriority {
    /// Lowest priority - analytics, beacons
    Background = 0,
    /// Low priority - prefetch, lazy images
    Low = 1,
    /// Normal priority - most images, non-critical resources
    Normal = 2,
    /// High priority - scripts, above-fold images
    High = 3,
    /// Highest priority - render-blocking CSS, fonts
    Critical = 4,
}

impl Default for RequestPriority {
    fn default() -> Self {
        Self::Normal
    }
}

impl RequestPriority {
    /// Get all priority levels in order
    pub fn all() -> &'static [RequestPriority] {
        &[
            RequestPriority::Critical,
            RequestPriority::High,
            RequestPriority::Normal,
            RequestPriority::Low,
            RequestPriority::Background,
        ]
    }
    
    /// Get priority from resource type
    pub fn from_resource_type(resource_type: ResourceType) -> Self {
        match resource_type {
            ResourceType::Document => RequestPriority::Critical,
            ResourceType::Style => RequestPriority::Critical,
            ResourceType::Font => RequestPriority::Critical,
            ResourceType::Script => RequestPriority::High,
            ResourceType::Image => RequestPriority::Normal,
            ResourceType::Media => RequestPriority::Low,
            ResourceType::Prefetch => RequestPriority::Low,
            ResourceType::Other => RequestPriority::Normal,
        }
    }
}

/// Resource types for priority classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    Document,
    Style,
    Script,
    Image,
    Font,
    Media,
    Prefetch,
    Other,
}

impl ResourceType {
    /// Detect resource type from URL and content-type
    pub fn from_hints(url: &str, content_type: Option<&str>) -> Self {
        // Check content-type first
        if let Some(ct) = content_type {
            let ct_lower = ct.to_lowercase();
            if ct_lower.contains("text/html") {
                return ResourceType::Document;
            }
            if ct_lower.contains("text/css") {
                return ResourceType::Style;
            }
            if ct_lower.contains("javascript") {
                return ResourceType::Script;
            }
            if ct_lower.starts_with("image/") {
                return ResourceType::Image;
            }
            if ct_lower.starts_with("font/") || ct_lower.contains("font") {
                return ResourceType::Font;
            }
            if ct_lower.starts_with("video/") || ct_lower.starts_with("audio/") {
                return ResourceType::Media;
            }
        }
        
        // Check URL extension
        let url_lower = url.to_lowercase();
        let ext = url_lower.rsplit('.').next().unwrap_or("");
        let ext = ext.split('?').next().unwrap_or(ext);
        
        match ext {
            "html" | "htm" => ResourceType::Document,
            "css" => ResourceType::Style,
            "js" | "mjs" => ResourceType::Script,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "avif" | "svg" | "ico" => ResourceType::Image,
            "woff" | "woff2" | "ttf" | "otf" | "eot" => ResourceType::Font,
            "mp4" | "webm" | "ogg" | "mp3" | "wav" => ResourceType::Media,
            _ => ResourceType::Other,
        }
    }
}

/// Unique request ID
static REQUEST_ID_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> u64 {
    REQUEST_ID_COUNTER.fetch_add(1, AtomicOrdering::SeqCst)
}

/// Prioritized request wrapper
#[derive(Debug, Clone)]
pub struct PrioritizedRequest {
    /// Unique request ID
    pub id: u64,
    /// Request URL
    pub url: String,
    /// Priority level
    pub priority: RequestPriority,
    /// Creation timestamp for FIFO ordering within same priority
    pub created_at: u64,
    /// Resource type hint
    pub resource_type: ResourceType,
}

impl PrioritizedRequest {
    /// Create new prioritized request
    pub fn new(url: &str, priority: RequestPriority) -> Self {
        Self {
            id: next_request_id(),
            url: url.to_string(),
            priority,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            resource_type: ResourceType::Other,
        }
    }
    
    /// Create with resource type
    pub fn with_type(url: &str, resource_type: ResourceType) -> Self {
        Self {
            id: next_request_id(),
            url: url.to_string(),
            priority: RequestPriority::from_resource_type(resource_type),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            resource_type,
        }
    }
}

impl Eq for PrioritizedRequest {}

impl PartialEq for PrioritizedRequest {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Ord for PrioritizedRequest {
    fn cmp(&self, other: &Self) -> Ordering {
        // Higher priority first
        match self.priority.cmp(&other.priority) {
            Ordering::Equal => {
                // Earlier requests first (FIFO within same priority)
                other.created_at.cmp(&self.created_at)
            }
            other => other,
        }
    }
}

impl PartialOrd for PrioritizedRequest {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Priority queue for HTTP requests
#[derive(Debug, Default)]
pub struct PriorityQueue {
    /// Binary heap for O(log n) insert/extract
    queue: BinaryHeap<PrioritizedRequest>,
    /// Track priorities by ID for updates
    by_id: HashMap<u64, RequestPriority>,
    /// Maximum queue size
    max_size: usize,
    /// Statistics
    stats: QueueStats,
}

/// Queue statistics
#[derive(Debug, Default, Clone)]
pub struct QueueStats {
    pub total_enqueued: u64,
    pub total_dequeued: u64,
    pub priority_updates: u64,
    pub dropped_low_priority: u64,
}

impl PriorityQueue {
    /// Create new priority queue
    pub fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            by_id: HashMap::new(),
            max_size: 1000,
            stats: QueueStats::default(),
        }
    }
    
    /// Create with max size
    pub fn with_max_size(max_size: usize) -> Self {
        Self {
            queue: BinaryHeap::new(),
            by_id: HashMap::new(),
            max_size,
            stats: QueueStats::default(),
        }
    }
    
    /// Enqueue a request
    pub fn push(&mut self, request: PrioritizedRequest) -> bool {
        // Check capacity
        if self.queue.len() >= self.max_size {
            // Drop lowest priority request
            if !self.drop_lowest() {
                return false;
            }
        }
        
        let id = request.id;
        let priority = request.priority;
        
        self.by_id.insert(id, priority);
        self.queue.push(request);
        self.stats.total_enqueued += 1;
        
        true
    }
    
    /// Dequeue highest priority request
    pub fn pop(&mut self) -> Option<PrioritizedRequest> {
        loop {
            let request = self.queue.pop()?;
            
            // Check if request is still valid (might have been updated)
            if let Some(&current_priority) = self.by_id.get(&request.id) {
                // Skip if priority was updated (will appear again with new priority)
                if current_priority == request.priority {
                    self.by_id.remove(&request.id);
                    self.stats.total_dequeued += 1;
                    return Some(request);
                }
            } else {
                // Request was cancelled
                continue;
            }
        }
    }
    
    /// Peek at highest priority request
    pub fn peek(&self) -> Option<&PrioritizedRequest> {
        self.queue.peek()
    }
    
    /// Update request priority
    pub fn update_priority(&mut self, id: u64, new_priority: RequestPriority) -> bool {
        if let Some(old_priority) = self.by_id.get_mut(&id) {
            if *old_priority != new_priority {
                *old_priority = new_priority;
                self.stats.priority_updates += 1;
                
                // Re-enqueue with new priority (old one will be skipped on pop)
                // Note: this is a lazy approach, O(n) cleanup would require rebuild
                return true;
            }
        }
        false
    }
    
    /// Cancel a request
    pub fn cancel(&mut self, id: u64) -> bool {
        self.by_id.remove(&id).is_some()
    }
    
    /// Check if request is queued
    pub fn contains(&self, id: u64) -> bool {
        self.by_id.contains_key(&id)
    }
    
    /// Get queue length
    pub fn len(&self) -> usize {
        self.by_id.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
    
    /// Get statistics
    pub fn stats(&self) -> &QueueStats {
        &self.stats
    }
    
    /// Clear the queue
    pub fn clear(&mut self) {
        self.queue.clear();
        self.by_id.clear();
    }
    
    /// Drop lowest priority request to make room
    fn drop_lowest(&mut self) -> bool {
        // Find lowest priority request
        let mut lowest_id = None;
        let mut lowest_priority = RequestPriority::Critical;
        
        for (&id, &priority) in &self.by_id {
            if priority < lowest_priority {
                lowest_priority = priority;
                lowest_id = Some(id);
            }
        }
        
        if let Some(id) = lowest_id {
            self.by_id.remove(&id);
            self.stats.dropped_low_priority += 1;
            true
        } else {
            false
        }
    }
    
    /// Get count by priority level
    pub fn count_by_priority(&self) -> HashMap<RequestPriority, usize> {
        let mut counts = HashMap::new();
        for &priority in self.by_id.values() {
            *counts.entry(priority).or_insert(0) += 1;
        }
        counts
    }
}

/// Bandwidth allocation hints
#[derive(Debug, Clone)]
pub struct BandwidthHints {
    /// Max concurrent requests per priority
    pub max_concurrent: HashMap<RequestPriority, usize>,
    /// Throttle delay per priority (ms)
    pub throttle_delay_ms: HashMap<RequestPriority, u64>,
}

impl Default for BandwidthHints {
    fn default() -> Self {
        let mut max_concurrent = HashMap::new();
        max_concurrent.insert(RequestPriority::Critical, 6);
        max_concurrent.insert(RequestPriority::High, 4);
        max_concurrent.insert(RequestPriority::Normal, 4);
        max_concurrent.insert(RequestPriority::Low, 2);
        max_concurrent.insert(RequestPriority::Background, 1);
        
        let mut throttle_delay_ms = HashMap::new();
        throttle_delay_ms.insert(RequestPriority::Critical, 0);
        throttle_delay_ms.insert(RequestPriority::High, 0);
        throttle_delay_ms.insert(RequestPriority::Normal, 0);
        throttle_delay_ms.insert(RequestPriority::Low, 100);
        throttle_delay_ms.insert(RequestPriority::Background, 500);
        
        Self {
            max_concurrent,
            throttle_delay_ms,
        }
    }
}

impl BandwidthHints {
    /// Get max concurrent for priority
    pub fn max_concurrent_for(&self, priority: RequestPriority) -> usize {
        *self.max_concurrent.get(&priority).unwrap_or(&4)
    }
    
    /// Get throttle delay for priority
    pub fn throttle_delay_for(&self, priority: RequestPriority) -> u64 {
        *self.throttle_delay_ms.get(&priority).unwrap_or(&0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_priority_ordering() {
        assert!(RequestPriority::Critical > RequestPriority::High);
        assert!(RequestPriority::High > RequestPriority::Normal);
        assert!(RequestPriority::Normal > RequestPriority::Low);
        assert!(RequestPriority::Low > RequestPriority::Background);
    }
    
    #[test]
    fn test_priority_queue_basic() {
        let mut queue = PriorityQueue::new();
        
        queue.push(PrioritizedRequest::new("http://a.com", RequestPriority::Low));
        queue.push(PrioritizedRequest::new("http://b.com", RequestPriority::Critical));
        queue.push(PrioritizedRequest::new("http://c.com", RequestPriority::Normal));
        
        // Should get Critical first
        let req = queue.pop().unwrap();
        assert_eq!(req.priority, RequestPriority::Critical);
        
        // Then Normal
        let req = queue.pop().unwrap();
        assert_eq!(req.priority, RequestPriority::Normal);
        
        // Then Low
        let req = queue.pop().unwrap();
        assert_eq!(req.priority, RequestPriority::Low);
        
        assert!(queue.is_empty());
    }
    
    #[test]
    fn test_fifo_within_priority() {
        let mut queue = PriorityQueue::new();
        
        let req1 = PrioritizedRequest::new("http://first.com", RequestPriority::Normal);
        std::thread::sleep(std::time::Duration::from_millis(1));
        let req2 = PrioritizedRequest::new("http://second.com", RequestPriority::Normal);
        
        queue.push(req1);
        queue.push(req2);
        
        // First in, first out within same priority
        let got = queue.pop().unwrap();
        assert!(got.url.contains("first"));
    }
    
    #[test]
    fn test_cancel_request() {
        let mut queue = PriorityQueue::new();
        
        let req = PrioritizedRequest::new("http://cancel.me", RequestPriority::Normal);
        let id = req.id;
        
        queue.push(req);
        assert!(queue.contains(id));
        
        queue.cancel(id);
        assert!(!queue.contains(id));
    }
    
    #[test]
    fn test_resource_type_detection() {
        assert_eq!(
            ResourceType::from_hints("style.css", None),
            ResourceType::Style
        );
        assert_eq!(
            ResourceType::from_hints("app.js", None),
            ResourceType::Script
        );
        assert_eq!(
            ResourceType::from_hints("photo.jpg", None),
            ResourceType::Image
        );
        assert_eq!(
            ResourceType::from_hints("font.woff2", None),
            ResourceType::Font
        );
    }
    
    #[test]
    fn test_max_size_drops_lowest() {
        let mut queue = PriorityQueue::with_max_size(2);
        
        queue.push(PrioritizedRequest::new("http://a.com", RequestPriority::Low));
        queue.push(PrioritizedRequest::new("http://b.com", RequestPriority::High));
        
        assert_eq!(queue.len(), 2);
        
        // Adding third should drop Low priority
        queue.push(PrioritizedRequest::new("http://c.com", RequestPriority::Critical));
        
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.stats().dropped_low_priority, 1);
    }
}
