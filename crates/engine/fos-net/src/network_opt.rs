//! Network Optimization Module
//!
//! Request coalescing, predictive DNS, global connection pool, delta sync.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Request coalescing - batches small requests
#[derive(Debug, Default)]
pub struct RequestCoalescer {
    /// Pending requests by origin
    pending: HashMap<String, Vec<PendingRequest>>,
    /// Batch size threshold
    batch_threshold: usize,
    /// Batch timeout (ms)
    batch_timeout_ms: u64,
    /// Last batch time
    last_batch: HashMap<String, Instant>,
}

/// Pending request for coalescing
#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub id: u64,
    pub url: String,
    pub method: String,
    pub body: Option<Vec<u8>>,
    pub queued_at: Instant,
}

impl RequestCoalescer {
    pub fn new(batch_threshold: usize, timeout_ms: u64) -> Self {
        Self {
            batch_threshold,
            batch_timeout_ms: timeout_ms,
            ..Default::default()
        }
    }
    
    /// Queue a request for coalescing
    pub fn queue(&mut self, origin: &str, request: PendingRequest) {
        let requests = self.pending.entry(origin.to_string()).or_default();
        requests.push(request);
    }
    
    /// Check if batch is ready
    pub fn is_batch_ready(&self, origin: &str) -> bool {
        if let Some(requests) = self.pending.get(origin) {
            if requests.len() >= self.batch_threshold {
                return true;
            }
            
            if let Some(first) = requests.first() {
                if first.queued_at.elapsed() > Duration::from_millis(self.batch_timeout_ms) {
                    return true;
                }
            }
        }
        false
    }
    
    /// Get ready batch
    pub fn get_batch(&mut self, origin: &str) -> Vec<PendingRequest> {
        self.pending.remove(origin).unwrap_or_default()
    }
    
    /// Get all ready batches
    pub fn get_all_ready(&mut self) -> Vec<(String, Vec<PendingRequest>)> {
        let ready_origins: Vec<String> = self.pending.keys()
            .filter(|o| self.is_batch_ready(o))
            .cloned()
            .collect();
        
        ready_origins.into_iter()
            .map(|o| {
                let batch = self.pending.remove(&o).unwrap_or_default();
                (o, batch)
            })
            .collect()
    }
}

/// Predictive DNS resolver
#[derive(Debug, Default)]
pub struct PredictiveDns {
    /// Pre-resolved hosts
    cache: HashMap<String, DnsEntry>,
    /// Pending prefetch requests
    prefetch_queue: VecDeque<String>,
    /// Host access patterns (for prediction)
    access_patterns: HashMap<String, Vec<String>>,
}

/// DNS cache entry
#[derive(Debug, Clone)]
pub struct DnsEntry {
    pub addresses: Vec<String>,
    pub resolved_at: Instant,
    pub ttl: Duration,
}

impl PredictiveDns {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Prefetch DNS for a host
    pub fn prefetch(&mut self, host: &str) {
        if !self.cache.contains_key(host) {
            self.prefetch_queue.push_back(host.to_string());
        }
    }
    
    /// Record host access (for pattern learning)
    pub fn record_access(&mut self, from_page: &str, to_host: &str) {
        let destinations = self.access_patterns.entry(from_page.to_string()).or_default();
        if !destinations.contains(&to_host.to_string()) {
            destinations.push(to_host.to_string());
        }
    }
    
    /// Predict and prefetch hosts based on current page
    pub fn predict_and_prefetch(&mut self, current_page: &str) {
        let destinations: Vec<String> = self.access_patterns
            .get(current_page)
            .map(|d| d.iter().take(5).cloned().collect())
            .unwrap_or_default();
        
        for host in destinations {
            self.prefetch(&host);
        }
    }
    
    /// Get cached DNS entry
    pub fn get(&self, host: &str) -> Option<&DnsEntry> {
        self.cache.get(host).filter(|e| e.resolved_at.elapsed() < e.ttl)
    }
    
    /// Store DNS entry
    pub fn store(&mut self, host: &str, addresses: Vec<String>, ttl: Duration) {
        self.cache.insert(host.to_string(), DnsEntry {
            addresses,
            resolved_at: Instant::now(),
            ttl,
        });
    }
    
    /// Get pending prefetch
    pub fn pop_prefetch(&mut self) -> Option<String> {
        self.prefetch_queue.pop_front()
    }
}

/// Delta sync protocol
#[derive(Debug, Default)]
pub struct DeltaSync {
    /// Resource versions
    versions: HashMap<String, ResourceVersion>,
}

/// Resource version tracking
#[derive(Debug, Clone)]
pub struct ResourceVersion {
    pub url: String,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub content_hash: u64,
    pub size: usize,
}

impl DeltaSync {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get version info for delta request
    pub fn get_version(&self, url: &str) -> Option<&ResourceVersion> {
        self.versions.get(url)
    }
    
    /// Store version info
    pub fn store_version(&mut self, version: ResourceVersion) {
        self.versions.insert(version.url.clone(), version);
    }
    
    /// Apply binary delta to cached content
    pub fn apply_delta(&self, base: &[u8], delta: &[u8]) -> Vec<u8> {
        // Simplified delta application (real impl would use bsdiff or similar)
        // Format: [op:1][len:4][data:len]*
        let mut result = base.to_vec();
        let mut i = 0;
        
        while i + 5 <= delta.len() {
            let op = delta[i];
            let len = u32::from_be_bytes([delta[i+1], delta[i+2], delta[i+3], delta[i+4]]) as usize;
            i += 5;
            
            match op {
                0 => { /* Copy from base - no-op for simplified version */ }
                1 => {
                    // Insert from delta
                    if i + len <= delta.len() {
                        result.extend_from_slice(&delta[i..i+len]);
                        i += len;
                    }
                }
                _ => break,
            }
        }
        
        result
    }
}

/// Cross-tab resource sharing
#[derive(Debug, Default)]
pub struct CrossTabCache {
    /// Shared immutable resources (content-addressed)
    resources: HashMap<u64, SharedResource>,
    /// Reference counts
    ref_counts: HashMap<u64, usize>,
}

/// Shared resource
#[derive(Debug, Clone)]
pub struct SharedResource {
    pub hash: u64,
    pub content_type: String,
    pub data: Vec<u8>,
    pub immutable: bool,
}

impl CrossTabCache {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Store resource (content-addressed)
    pub fn store(&mut self, content_type: &str, data: Vec<u8>) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        let hash = hasher.finish();
        
        if !self.resources.contains_key(&hash) {
            self.resources.insert(hash, SharedResource {
                hash,
                content_type: content_type.to_string(),
                data,
                immutable: true,
            });
        }
        
        *self.ref_counts.entry(hash).or_insert(0) += 1;
        hash
    }
    
    /// Get resource by hash
    pub fn get(&self, hash: u64) -> Option<&SharedResource> {
        self.resources.get(&hash)
    }
    
    /// Release reference
    pub fn release(&mut self, hash: u64) {
        if let Some(count) = self.ref_counts.get_mut(&hash) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                self.resources.remove(&hash);
                self.ref_counts.remove(&hash);
            }
        }
    }
    
    /// Memory saved by sharing
    pub fn memory_saved(&self) -> usize {
        self.resources.values()
            .filter_map(|r| {
                let refs = self.ref_counts.get(&r.hash).copied().unwrap_or(0);
                if refs > 1 {
                    Some(r.data.len() * (refs - 1))
                } else {
                    None
                }
            })
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_request_coalescer() {
        let mut coalescer = RequestCoalescer::new(3, 1000);
        
        for i in 0..3 {
            coalescer.queue("example.com", PendingRequest {
                id: i,
                url: format!("/api/{}", i),
                method: "GET".to_string(),
                body: None,
                queued_at: Instant::now(),
            });
        }
        
        assert!(coalescer.is_batch_ready("example.com"));
        let batch = coalescer.get_batch("example.com");
        assert_eq!(batch.len(), 3);
    }
    
    #[test]
    fn test_predictive_dns() {
        let mut dns = PredictiveDns::new();
        
        dns.record_access("/login", "api.example.com");
        dns.record_access("/login", "cdn.example.com");
        
        dns.predict_and_prefetch("/login");
        
        assert!(dns.prefetch_queue.contains(&"api.example.com".to_string()));
    }
    
    #[test]
    fn test_cross_tab_cache() {
        let mut cache = CrossTabCache::new();
        
        let data = b"Hello, World!".to_vec();
        let hash1 = cache.store("text/plain", data.clone());
        let hash2 = cache.store("text/plain", data.clone());
        
        assert_eq!(hash1, hash2); // Same content = same hash
        assert!(cache.memory_saved() > 0); // Saved by deduplication
    }
}
