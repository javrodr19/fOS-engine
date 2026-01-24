//! Request Fusion
//!
//! Merge small requests into batched operations for reduced overhead.
//! Smart request grouping by timing and destination.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Maximum batch size
const MAX_BATCH_SIZE: usize = 16;

/// Default fusion window
const DEFAULT_WINDOW: Duration = Duration::from_millis(10);

/// Request to be fused
#[derive(Debug, Clone)]
pub struct FusionRequest {
    /// Request ID
    pub id: u64,
    /// URL
    pub url: String,
    /// Host
    pub host: String,
    /// Method
    pub method: String,
    /// Headers
    pub headers: Vec<(String, String)>,
    /// Body (if any)
    pub body: Option<Vec<u8>>,
    /// Priority (0-7, lower = higher)
    pub priority: u8,
    /// Timestamp when request was added
    pub added: Instant,
}

impl FusionRequest {
    /// Create a new request
    pub fn new(id: u64, url: String, method: &str) -> Self {
        Self {
            id,
            host: extract_host(&url),
            url,
            method: method.to_uppercase(),
            headers: Vec::new(),
            body: None,
            priority: 3,
            added: Instant::now(),
        }
    }
    
    /// Add header
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
    
    /// Set body
    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }
    
    /// Set priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority.min(7);
        self
    }
    
    /// Get estimated size
    pub fn estimated_size(&self) -> usize {
        self.url.len() 
            + self.method.len()
            + self.headers.iter().map(|(k, v)| k.len() + v.len() + 4).sum::<usize>()
            + self.body.as_ref().map(|b| b.len()).unwrap_or(0)
    }
}

/// Fused request batch
#[derive(Debug, Clone)]
pub struct FusedBatch {
    /// Batch ID
    pub id: u64,
    /// Host for this batch
    pub host: String,
    /// Requests in this batch
    pub requests: Vec<FusionRequest>,
    /// Created time
    pub created: Instant,
    /// Total estimated size
    pub size: usize,
}

impl FusedBatch {
    /// Create a new batch
    pub fn new(id: u64, host: String) -> Self {
        Self {
            id,
            host,
            requests: Vec::new(),
            created: Instant::now(),
            size: 0,
        }
    }
    
    /// Add request to batch
    pub fn add(&mut self, request: FusionRequest) {
        self.size += request.estimated_size();
        self.requests.push(request);
    }
    
    /// Check if batch is full
    pub fn is_full(&self) -> bool {
        self.requests.len() >= MAX_BATCH_SIZE
    }
    
    /// Get request count
    pub fn len(&self) -> usize {
        self.requests.len()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }
    
    /// Sort requests by priority
    pub fn sort_by_priority(&mut self) {
        self.requests.sort_by(|a, b| a.priority.cmp(&b.priority));
    }
}

/// Request fusion engine
#[derive(Debug)]
pub struct RequestFusion {
    /// Pending requests by host
    pending: HashMap<String, VecDeque<FusionRequest>>,
    /// Fusion window
    window: Duration,
    /// Next batch ID
    next_batch_id: u64,
    /// Next request ID
    next_request_id: u64,
    /// Statistics
    stats: FusionStats,
    /// Maximum requests per host
    max_per_host: usize,
    /// Enabled
    enabled: bool,
}

/// Fusion statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct FusionStats {
    /// Requests submitted
    pub requests_submitted: u64,
    /// Batches created
    pub batches_created: u64,
    /// Requests fused (saved individual requests)
    pub requests_fused: u64,
    /// Bytes in fused requests
    pub bytes_fused: u64,
}

impl FusionStats {
    /// Get fusion efficiency (requests per batch)
    pub fn fusion_efficiency(&self) -> f64 {
        if self.batches_created == 0 {
            1.0
        } else {
            self.requests_submitted as f64 / self.batches_created as f64
        }
    }
    
    /// Get overhead saved (estimated requests saved)
    pub fn overhead_saved(&self) -> u64 {
        self.requests_fused
    }
}

impl Default for RequestFusion {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestFusion {
    /// Create a new fusion engine
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            window: DEFAULT_WINDOW,
            next_batch_id: 1,
            next_request_id: 1,
            stats: FusionStats::default(),
            max_per_host: MAX_BATCH_SIZE * 2,
            enabled: true,
        }
    }
    
    /// Set fusion window
    pub fn with_window(mut self, window: Duration) -> Self {
        self.window = window;
        self
    }
    
    /// Enable/disable fusion
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    
    /// Submit a request for potential fusion
    pub fn submit(&mut self, url: &str, method: &str) -> u64 {
        let id = self.next_request_id;
        self.next_request_id += 1;
        
        let request = FusionRequest::new(id, url.to_string(), method);
        let host = request.host.clone();
        
        self.pending
            .entry(host)
            .or_default()
            .push_back(request);
        
        self.stats.requests_submitted += 1;
        id
    }
    
    /// Submit a request with custom options
    pub fn submit_request(&mut self, request: FusionRequest) -> u64 {
        let id = request.id;
        let host = request.host.clone();
        
        self.pending
            .entry(host)
            .or_default()
            .push_back(request);
        
        self.stats.requests_submitted += 1;
        id
    }
    
    /// Flush ready batches
    pub fn flush(&mut self) -> Vec<FusedBatch> {
        if !self.enabled {
            return self.flush_all();
        }
        
        let now = Instant::now();
        let mut batches = Vec::new();
        
        for (host, queue) in self.pending.iter_mut() {
            // Check if oldest request has exceeded window
            let should_flush = queue.front()
                .map(|r| now.duration_since(r.added) >= self.window)
                .unwrap_or(false);
            
            // Or if we have enough requests
            let has_enough = queue.len() >= MAX_BATCH_SIZE;
            
            if (should_flush || has_enough) && !queue.is_empty() {
                let mut batch = FusedBatch::new(self.next_batch_id, host.clone());
                self.next_batch_id += 1;
                
                // Take up to MAX_BATCH_SIZE requests
                let count = queue.len().min(MAX_BATCH_SIZE);
                for _ in 0..count {
                    if let Some(request) = queue.pop_front() {
                        self.stats.bytes_fused += request.estimated_size() as u64;
                        batch.add(request);
                    }
                }
                
                if batch.len() > 1 {
                    self.stats.requests_fused += (batch.len() - 1) as u64;
                }
                
                batch.sort_by_priority();
                batches.push(batch);
                self.stats.batches_created += 1;
            }
        }
        
        // Clean up empty host entries
        self.pending.retain(|_, v| !v.is_empty());
        
        batches
    }
    
    /// Force flush all pending requests
    pub fn flush_all(&mut self) -> Vec<FusedBatch> {
        let mut batches = Vec::new();
        
        for (host, queue) in self.pending.drain() {
            if queue.is_empty() {
                continue;
            }
            
            let mut batch = FusedBatch::new(self.next_batch_id, host);
            self.next_batch_id += 1;
            
            for request in queue {
                self.stats.bytes_fused += request.estimated_size() as u64;
                batch.add(request);
            }
            
            if batch.len() > 1 {
                self.stats.requests_fused += (batch.len() - 1) as u64;
            }
            
            batch.sort_by_priority();
            batches.push(batch);
            self.stats.batches_created += 1;
        }
        
        batches
    }
    
    /// Cancel a pending request
    pub fn cancel(&mut self, id: u64) -> bool {
        for queue in self.pending.values_mut() {
            if let Some(pos) = queue.iter().position(|r| r.id == id) {
                queue.remove(pos);
                return true;
            }
        }
        false
    }
    
    /// Get pending count
    pub fn pending_count(&self) -> usize {
        self.pending.values().map(|q| q.len()).sum()
    }
    
    /// Get statistics
    pub fn stats(&self) -> &FusionStats {
        &self.stats
    }
    
    /// Clear all pending
    pub fn clear(&mut self) {
        self.pending.clear();
    }
}

/// Extract host from URL
fn extract_host(url: &str) -> String {
    let url = url.trim_start_matches("https://")
        .trim_start_matches("http://");
    
    url.split('/')
        .next()
        .unwrap_or(url)
        .split(':')
        .next()
        .unwrap_or(url)
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fusion_request() {
        let req = FusionRequest::new(1, "https://example.com/api".into(), "GET")
            .with_header("Accept", "application/json")
            .with_priority(2);
        
        assert_eq!(req.id, 1);
        assert_eq!(req.host, "example.com");
        assert_eq!(req.method, "GET");
        assert_eq!(req.priority, 2);
    }
    
    #[test]
    fn test_fusion_submit() {
        let mut fusion = RequestFusion::new();
        
        let id1 = fusion.submit("https://example.com/a", "GET");
        let id2 = fusion.submit("https://example.com/b", "GET");
        
        assert_ne!(id1, id2);
        assert_eq!(fusion.pending_count(), 2);
    }
    
    #[test]
    fn test_fusion_flush_all() {
        let mut fusion = RequestFusion::new();
        
        fusion.submit("https://example.com/a", "GET");
        fusion.submit("https://example.com/b", "GET");
        fusion.submit("https://other.com/c", "POST");
        
        let batches = fusion.flush_all();
        
        // Should have 2 batches (2 hosts)
        assert_eq!(batches.len(), 2);
        
        let example_batch = batches.iter().find(|b| b.host == "example.com").unwrap();
        assert_eq!(example_batch.len(), 2);
    }
    
    #[test]
    fn test_fusion_cancel() {
        let mut fusion = RequestFusion::new();
        
        let id = fusion.submit("https://example.com/a", "GET");
        assert_eq!(fusion.pending_count(), 1);
        
        assert!(fusion.cancel(id));
        assert_eq!(fusion.pending_count(), 0);
    }
    
    #[test]
    fn test_fusion_stats() {
        let mut fusion = RequestFusion::new();
        
        fusion.submit("https://example.com/a", "GET");
        fusion.submit("https://example.com/b", "GET");
        fusion.flush_all();
        
        let stats = fusion.stats();
        assert_eq!(stats.requests_submitted, 2);
        assert_eq!(stats.batches_created, 1);
        assert_eq!(stats.requests_fused, 1); // 2 requests in 1 batch = 1 saved
    }
    
    #[test]
    fn test_batch_priority_sorting() {
        let mut batch = FusedBatch::new(1, "example.com".into());
        
        batch.add(FusionRequest::new(1, "https://example.com/a".into(), "GET").with_priority(5));
        batch.add(FusionRequest::new(2, "https://example.com/b".into(), "GET").with_priority(1));
        batch.add(FusionRequest::new(3, "https://example.com/c".into(), "GET").with_priority(3));
        
        batch.sort_by_priority();
        
        assert_eq!(batch.requests[0].priority, 1);
        assert_eq!(batch.requests[1].priority, 3);
        assert_eq!(batch.requests[2].priority, 5);
    }
}
