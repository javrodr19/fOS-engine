//! Request Coalescing (Phase 24.7)
//!
//! Batch multiple small requests. Single network round trip.
//! Combine CSS/JS fetches. 50% fewer requests.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Request ID
pub type RequestId = u32;

/// Request priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Request type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestType {
    Document,
    Script,
    Style,
    Image,
    Font,
    Xhr,
    Fetch,
    Other,
}

impl RequestType {
    /// Can requests of this type be coalesced?
    pub fn can_coalesce(&self) -> bool {
        matches!(self, RequestType::Script | RequestType::Style | RequestType::Image)
    }
}

/// Pending request
#[derive(Debug, Clone)]
pub struct PendingRequest {
    pub id: RequestId,
    pub url: String,
    pub request_type: RequestType,
    pub priority: Priority,
    pub host: String,
    pub created_at: Instant,
}

impl PendingRequest {
    pub fn new(id: RequestId, url: String, request_type: RequestType) -> Self {
        // Extract host from URL
        let host = extract_host(&url).unwrap_or_default();
        
        Self {
            id,
            url,
            request_type,
            priority: Priority::Normal,
            host,
            created_at: Instant::now(),
        }
    }
    
    pub fn with_priority(mut self, priority: Priority) -> Self {
        self.priority = priority;
        self
    }
}

fn extract_host(url: &str) -> Option<String> {
    let start = if url.starts_with("https://") {
        8
    } else if url.starts_with("http://") {
        7
    } else {
        return None;
    };
    
    let rest = url.get(start..)?;
    let end = rest.find('/').unwrap_or(rest.len());
    let host_port = &rest[..end];
    
    // Remove port
    let end = host_port.find(':').unwrap_or(host_port.len());
    Some(host_port[..end].to_string())
}

/// Coalesced request batch
#[derive(Debug, Clone)]
pub struct CoalescedBatch {
    /// Batch ID
    pub id: u32,
    /// Host for all requests
    pub host: String,
    /// Requests in this batch
    pub requests: Vec<PendingRequest>,
    /// Type of requests
    pub request_type: RequestType,
}

impl CoalescedBatch {
    pub fn new(id: u32, host: String, request_type: RequestType) -> Self {
        Self {
            id,
            host,
            requests: Vec::new(),
            request_type,
        }
    }
    
    pub fn add(&mut self, request: PendingRequest) {
        self.requests.push(request);
    }
    
    pub fn len(&self) -> usize {
        self.requests.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }
    
    /// Get all URLs
    pub fn urls(&self) -> Vec<&str> {
        self.requests.iter().map(|r| r.url.as_str()).collect()
    }
}

/// Request coalescer
#[derive(Debug)]
pub struct RequestCoalescer {
    /// Pending requests by host
    pending: HashMap<String, Vec<PendingRequest>>,
    /// Coalesce window
    window: Duration,
    /// Max batch size
    max_batch_size: usize,
    /// Next batch ID
    next_batch_id: u32,
    /// Next request ID
    next_request_id: RequestId,
    /// Statistics
    stats: CoalescerStats,
}

/// Coalescer statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct CoalescerStats {
    pub requests_received: u64,
    pub requests_coalesced: u64,
    pub batches_created: u64,
    pub requests_sent_immediately: u64,
}

impl CoalescerStats {
    pub fn coalesce_ratio(&self) -> f64 {
        if self.requests_received == 0 {
            0.0
        } else {
            self.requests_coalesced as f64 / self.requests_received as f64
        }
    }
    
    pub fn requests_saved(&self) -> u64 {
        self.requests_coalesced.saturating_sub(self.batches_created)
    }
}

impl Default for RequestCoalescer {
    fn default() -> Self {
        Self::new()
    }
}

impl RequestCoalescer {
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
            window: Duration::from_millis(10),
            max_batch_size: 6,
            next_batch_id: 0,
            next_request_id: 0,
            stats: CoalescerStats::default(),
        }
    }
    
    /// Set coalesce window
    pub fn with_window(mut self, window: Duration) -> Self {
        self.window = window;
        self
    }
    
    /// Set max batch size
    pub fn with_max_batch_size(mut self, size: usize) -> Self {
        self.max_batch_size = size;
        self
    }
    
    /// Add a request
    pub fn add(&mut self, url: String, request_type: RequestType, priority: Priority) -> RequestId {
        let id = self.next_request_id;
        self.next_request_id += 1;
        
        self.stats.requests_received += 1;
        
        let request = PendingRequest::new(id, url, request_type).with_priority(priority);
        
        // Critical priority requests aren't coalesced
        if priority == Priority::Critical || !request_type.can_coalesce() {
            self.stats.requests_sent_immediately += 1;
            return id;
        }
        
        let host = request.host.clone();
        self.pending.entry(host).or_default().push(request);
        
        id
    }
    
    /// Flush ready batches
    pub fn flush(&mut self) -> Vec<CoalescedBatch> {
        let now = Instant::now();
        let mut batches = Vec::new();
        
        let hosts: Vec<_> = self.pending.keys().cloned().collect();
        
        for host in hosts {
            if let Some(requests) = self.pending.get_mut(&host) {
                // Check if window expired for oldest request
                let should_flush = requests.first()
                    .map(|r| now.duration_since(r.created_at) >= self.window)
                    .unwrap_or(false);
                
                if should_flush || requests.len() >= self.max_batch_size {
                    // Group by request type
                    let mut by_type: HashMap<RequestType, Vec<PendingRequest>> = HashMap::new();
                    
                    for req in requests.drain(..) {
                        by_type.entry(req.request_type).or_default().push(req);
                    }
                    
                    for (req_type, reqs) in by_type {
                        for chunk in reqs.chunks(self.max_batch_size) {
                            let batch_id = self.next_batch_id;
                            self.next_batch_id += 1;
                            
                            let mut batch = CoalescedBatch::new(batch_id, host.clone(), req_type);
                            for req in chunk {
                                batch.add(req.clone());
                            }
                            
                            self.stats.requests_coalesced += batch.len() as u64;
                            self.stats.batches_created += 1;
                            
                            batches.push(batch);
                        }
                    }
                }
            }
        }
        
        // Clean up empty entries
        self.pending.retain(|_, v| !v.is_empty());
        
        batches
    }
    
    /// Force flush all pending
    pub fn flush_all(&mut self) -> Vec<CoalescedBatch> {
        // Temporarily set window to 0
        let old_window = self.window;
        self.window = Duration::ZERO;
        
        let batches = self.flush();
        
        self.window = old_window;
        batches
    }
    
    /// Get statistics
    pub fn stats(&self) -> &CoalescerStats {
        &self.stats
    }
    
    /// Pending count
    pub fn pending_count(&self) -> usize {
        self.pending.values().map(|v| v.len()).sum()
    }
}

/// DNS prefetch manager
#[derive(Debug)]
pub struct DnsPrefetcher {
    /// Resolved domains
    resolved: HashMap<String, ResolvedDns>,
    /// Pending resolutions
    pending: VecDeque<String>,
    /// Max cached
    max_cached: usize,
    /// Statistics
    stats: DnsStats,
}

#[derive(Debug, Clone)]
pub struct ResolvedDns {
    pub domain: String,
    pub ips: Vec<String>,
    pub resolved_at: Instant,
    pub ttl: Duration,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DnsStats {
    pub prefetches: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

impl Default for DnsPrefetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl DnsPrefetcher {
    pub fn new() -> Self {
        Self {
            resolved: HashMap::new(),
            pending: VecDeque::new(),
            max_cached: 100,
            stats: DnsStats::default(),
        }
    }
    
    /// Queue domain for prefetch
    pub fn prefetch(&mut self, domain: &str) {
        if self.resolved.contains_key(domain) {
            return;
        }
        
        if !self.pending.iter().any(|d| d == domain) {
            self.pending.push_back(domain.to_string());
            self.stats.prefetches += 1;
        }
    }
    
    /// Get next domain to resolve
    pub fn next_pending(&mut self) -> Option<String> {
        self.pending.pop_front()
    }
    
    /// Record resolution result
    pub fn record_resolution(&mut self, domain: String, ips: Vec<String>, ttl: Duration) {
        // Evict old entries if needed
        if self.resolved.len() >= self.max_cached {
            if let Some(oldest) = self.resolved.keys().next().cloned() {
                self.resolved.remove(&oldest);
            }
        }
        
        self.resolved.insert(domain.clone(), ResolvedDns {
            domain,
            ips,
            resolved_at: Instant::now(),
            ttl,
        });
    }
    
    /// Lookup domain
    pub fn lookup(&mut self, domain: &str) -> Option<&ResolvedDns> {
        if let Some(entry) = self.resolved.get(domain) {
            // Check TTL
            if entry.resolved_at.elapsed() < entry.ttl {
                self.stats.cache_hits += 1;
                return Some(entry);
            }
        }
        
        self.stats.cache_misses += 1;
        None
    }
    
    /// Get statistics
    pub fn stats(&self) -> &DnsStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_request_coalescing() {
        let mut coalescer = RequestCoalescer::new()
            .with_window(Duration::from_millis(1))
            .with_max_batch_size(3);
        
        // Add requests to same host
        coalescer.add("https://example.com/a.js".into(), RequestType::Script, Priority::Normal);
        coalescer.add("https://example.com/b.js".into(), RequestType::Script, Priority::Normal);
        coalescer.add("https://example.com/c.js".into(), RequestType::Script, Priority::Normal);
        
        // Force flush
        std::thread::sleep(Duration::from_millis(5));
        let batches = coalescer.flush();
        
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].len(), 3);
    }
    
    #[test]
    fn test_critical_not_coalesced() {
        let mut coalescer = RequestCoalescer::new();
        
        coalescer.add("https://example.com/critical.js".into(), RequestType::Script, Priority::Critical);
        
        assert_eq!(coalescer.stats().requests_sent_immediately, 1);
        assert_eq!(coalescer.pending_count(), 0);
    }
    
    #[test]
    fn test_dns_prefetcher() {
        let mut prefetcher = DnsPrefetcher::new();
        
        prefetcher.prefetch("example.com");
        
        let domain = prefetcher.next_pending().unwrap();
        assert_eq!(domain, "example.com");
        
        prefetcher.record_resolution(
            "example.com".into(),
            vec!["93.184.216.34".into()],
            Duration::from_secs(300),
        );
        
        let result = prefetcher.lookup("example.com");
        assert!(result.is_some());
        assert_eq!(prefetcher.stats().cache_hits, 1);
    }
}
