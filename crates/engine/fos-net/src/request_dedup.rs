//! Request Deduplication
//!
//! Deduplicate identical concurrent network requests.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::hash::{Hash, Hasher};

/// Request key for deduplication
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestKey {
    /// URL
    pub url: String,
    /// HTTP method
    pub method: String,
    /// Body hash (for POST/PUT)
    pub body_hash: u64,
}

impl RequestKey {
    /// Create key for GET request
    pub fn get(url: &str) -> Self {
        Self { url: url.to_string(), method: "GET".to_string(), body_hash: 0 }
    }
    
    /// Create key with body
    pub fn with_body(url: &str, method: &str, body: &[u8]) -> Self {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        body.hash(&mut hasher);
        Self { url: url.to_string(), method: method.to_string(), body_hash: hasher.finish() }
    }
}

/// Response data
#[derive(Debug, Clone)]
pub struct DeduplicatedResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Arc<Vec<u8>>,
    pub was_deduplicated: bool,
}

impl DeduplicatedResponse {
    pub fn new(status: u16, headers: Vec<(String, String)>, body: Vec<u8>) -> Self {
        Self { status, headers, body: Arc::new(body), was_deduplicated: false }
    }
    
    pub fn mark_deduplicated(mut self) -> Self {
        self.was_deduplicated = true;
        self
    }
}

/// Request deduplicator
#[derive(Debug)]
pub struct RequestDeduplicator {
    pending: Arc<Mutex<HashMap<RequestKey, PendingRequestState>>>,
    stats: Arc<Mutex<DeduplicationStats>>,
}

#[derive(Debug)]
struct PendingRequestState {
    subscriber_count: usize,
    started_at: std::time::Instant,
}

/// Statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct DeduplicationStats {
    pub total_requests: u64,
    pub deduplicated: u64,
    pub unique: u64,
    pub bytes_saved: u64,
}

impl DeduplicationStats {
    pub fn dedup_rate(&self) -> f64 {
        if self.total_requests == 0 { 0.0 }
        else { self.deduplicated as f64 / self.total_requests as f64 }
    }
}

impl Default for RequestDeduplicator {
    fn default() -> Self { Self::new() }
}

impl RequestDeduplicator {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            stats: Arc::new(Mutex::new(DeduplicationStats::default())),
        }
    }
    
    /// Check if request is in flight
    pub fn is_pending(&self, key: &RequestKey) -> bool {
        self.pending.lock().unwrap().contains_key(key)
    }
    
    /// Start a request (returns true if this is first request for key)
    pub fn start_request(&self, key: RequestKey) -> bool {
        let mut pending = self.pending.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();
        
        stats.total_requests += 1;
        
        if let Some(state) = pending.get_mut(&key) {
            state.subscriber_count += 1;
            stats.deduplicated += 1;
            false // Already pending
        } else {
            pending.insert(key, PendingRequestState {
                subscriber_count: 1,
                started_at: std::time::Instant::now(),
            });
            stats.unique += 1;
            true // First request
        }
    }
    
    /// Complete a request
    pub fn complete_request(&self, key: &RequestKey, body_size: usize) -> usize {
        let mut pending = self.pending.lock().unwrap();
        
        if let Some(state) = pending.remove(key) {
            let deduplicated_count = state.subscriber_count.saturating_sub(1);
            if deduplicated_count > 0 {
                let mut stats = self.stats.lock().unwrap();
                stats.bytes_saved += (body_size * deduplicated_count) as u64;
            }
            state.subscriber_count
        } else {
            0
        }
    }
    
    /// Cancel request
    pub fn cancel_request(&self, key: &RequestKey) {
        let mut pending = self.pending.lock().unwrap();
        if let Some(state) = pending.get_mut(key) {
            state.subscriber_count = state.subscriber_count.saturating_sub(1);
            if state.subscriber_count == 0 {
                pending.remove(key);
            }
        }
    }
    
    /// Get stats
    pub fn stats(&self) -> DeduplicationStats {
        *self.stats.lock().unwrap()
    }
    
    /// Pending request count
    pub fn pending_count(&self) -> usize {
        self.pending.lock().unwrap().len()
    }
}

/// Simple in-flight request tracker for synchronous use
#[derive(Debug, Default)]
pub struct SimpleDeduplicator {
    pending: HashMap<RequestKey, u32>,
    stats: DeduplicationStats,
}

impl SimpleDeduplicator {
    pub fn new() -> Self { Self::default() }
    
    /// Try to start request, returns false if already pending
    pub fn try_start(&mut self, key: RequestKey) -> bool {
        self.stats.total_requests += 1;
        if let Some(count) = self.pending.get_mut(&key) {
            *count += 1;
            self.stats.deduplicated += 1;
            false
        } else {
            self.pending.insert(key, 1);
            self.stats.unique += 1;
            true
        }
    }
    
    /// Complete request
    pub fn complete(&mut self, key: &RequestKey) -> u32 {
        self.pending.remove(key).unwrap_or(0)
    }
    
    pub fn stats(&self) -> &DeduplicationStats { &self.stats }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dedup_same_url() {
        let dedup = RequestDeduplicator::new();
        
        let key = RequestKey::get("https://example.com/api");
        
        assert!(dedup.start_request(key.clone())); // First
        assert!(!dedup.start_request(key.clone())); // Deduplicated
        assert!(!dedup.start_request(key.clone())); // Deduplicated
        
        assert!(dedup.is_pending(&key));
        
        let count = dedup.complete_request(&key, 1000);
        assert_eq!(count, 3);
        
        let stats = dedup.stats();
        assert_eq!(stats.unique, 1);
        assert_eq!(stats.deduplicated, 2);
    }
    
    #[test]
    fn test_different_urls() {
        let dedup = RequestDeduplicator::new();
        
        let key1 = RequestKey::get("https://example.com/a");
        let key2 = RequestKey::get("https://example.com/b");
        
        assert!(dedup.start_request(key1.clone()));
        assert!(dedup.start_request(key2.clone()));
        
        assert_eq!(dedup.pending_count(), 2);
    }
    
    #[test]
    fn test_simple_dedup() {
        let mut dedup = SimpleDeduplicator::new();
        
        let key = RequestKey::get("https://test.com");
        
        assert!(dedup.try_start(key.clone()));
        assert!(!dedup.try_start(key.clone()));
        
        assert_eq!(dedup.complete(&key), 2);
    }
}
