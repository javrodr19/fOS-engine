//! Predictive Networking
//!
//! DNS prefetch, preconnect, resource prediction, and request coalescing.
//! Wraps fos-net network optimization features.

use std::collections::{HashSet, VecDeque};

// Re-export core types from fos-net
pub use fos_net::network_opt::{
    PredictiveDns, DnsEntry,
    CrossTabCache, SharedResource,
    DeltaSync, ResourceVersion,
    RequestCoalescer, PendingRequest,
};

/// Predictive network manager for the browser
/// Integrates DNS prediction, cross-tab caching, and resource hints
#[derive(Debug, Default)]
pub struct PredictiveNetwork {
    /// Predictive DNS from fos-net
    dns: PredictiveDns,
    /// Cross-tab resource cache from fos-net
    cross_tab: CrossTabCache,
    /// Delta sync for efficient updates
    delta: DeltaSync,
    /// Request coalescer for batching
    coalescer: RequestCoalescer,
    /// Currently visible page links (for prefetch prediction)
    page_links: HashSet<String>,
    /// Preconnected hosts
    preconnected: HashSet<String>,
}

impl PredictiveNetwork {
    pub fn new() -> Self {
        Self {
            dns: PredictiveDns::new(),
            cross_tab: CrossTabCache::new(),
            delta: DeltaSync::new(),
            coalescer: RequestCoalescer::new(5, 100), // Batch 5 requests or 100ms timeout
            page_links: HashSet::new(),
            preconnected: HashSet::new(),
        }
    }
    
    // === DNS Prefetching ===
    
    /// Record a page visit for prediction learning
    pub fn record_visit(&mut self, url: &str) {
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                // Use fos-net predictive DNS
                self.dns.prefetch(host);
                
                // Learn navigation patterns
                self.dns.record_access(parsed.path(), host);
            }
        }
    }
    
    /// Predict and prefetch DNS for likely next navigations
    pub fn predict_for_page(&mut self, current_path: &str) {
        self.dns.predict_and_prefetch(current_path);
    }
    
    /// Process pending DNS prefetch requests
    pub fn process_dns_prefetch(&mut self) -> Option<String> {
        self.dns.pop_prefetch()
    }
    
    /// Store resolved DNS entry
    pub fn store_dns(&mut self, host: &str, addresses: Vec<String>, ttl_secs: u64) {
        self.dns.store(host, addresses, std::time::Duration::from_secs(ttl_secs));
    }
    
    /// Get cached DNS entry
    pub fn get_dns(&self, host: &str) -> Option<&DnsEntry> {
        self.dns.get(host)
    }
    
    // === Page Link Management ===
    
    /// Update links found on current page
    pub fn update_page_links(&mut self, links: Vec<String>) {
        self.page_links.clear();
        for link in links {
            if let Ok(parsed) = url::Url::parse(&link) {
                if let Some(host) = parsed.host_str() {
                    self.page_links.insert(host.to_string());
                    // Prefetch DNS for all visible links
                    self.dns.prefetch(host);
                }
            }
        }
    }
    
    /// Get predicted next navigations
    pub fn predict_next(&self) -> Vec<String> {
        self.page_links.iter().take(5).cloned().collect()
    }
    
    // === Preconnect ===
    
    /// Preconnect to a host (TCP + TLS handshake)
    pub fn preconnect(&mut self, host: &str) {
        if !self.preconnected.contains(host) {
            self.preconnected.insert(host.to_string());
            log::debug!("Preconnected to {}", host);
        }
    }
    
    /// Check if host is preconnected
    pub fn is_preconnected(&self, host: &str) -> bool {
        self.preconnected.contains(host)
    }
    
    // === Cross-Tab Resource Sharing ===
    
    /// Store a resource in cross-tab cache (content-addressed)
    pub fn share_resource(&mut self, content_type: &str, data: Vec<u8>) -> u64 {
        self.cross_tab.store(content_type, data)
    }
    
    /// Get shared resource by hash
    pub fn get_shared_resource(&self, hash: u64) -> Option<&SharedResource> {
        self.cross_tab.get(hash)
    }
    
    /// Release reference to shared resource
    pub fn release_shared(&mut self, hash: u64) {
        self.cross_tab.release(hash);
    }
    
    /// Memory saved by cross-tab deduplication
    pub fn memory_saved(&self) -> usize {
        self.cross_tab.memory_saved()
    }
    
    // === Delta Sync ===
    
    /// Get version info for a resource (for conditional requests)
    pub fn get_resource_version(&self, url: &str) -> Option<&ResourceVersion> {
        self.delta.get_version(url)
    }
    
    /// Store resource version info
    pub fn store_resource_version(&mut self, version: ResourceVersion) {
        self.delta.store_version(version);
    }
    
    /// Apply binary delta to cached content
    pub fn apply_delta(&self, base: &[u8], delta: &[u8]) -> Vec<u8> {
        self.delta.apply_delta(base, delta)
    }
    
    // === Request Coalescing ===
    
    /// Queue a request for potential batching
    pub fn queue_request(&mut self, origin: &str, request: PendingRequest) {
        self.coalescer.queue(origin, request);
    }
    
    /// Get all ready request batches
    pub fn get_ready_batches(&mut self) -> Vec<(String, Vec<PendingRequest>)> {
        self.coalescer.get_all_ready()
    }
    
    // === Cleanup ===
    
    /// Clear all prediction data (for privacy)
    pub fn clear(&mut self) {
        self.page_links.clear();
        self.preconnected.clear();
        // Note: DNS cache and cross-tab cache persist for performance
    }
    
    /// Full reset (clears everything)
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

/// Resource hints from HTML <link> elements
#[derive(Debug, Clone)]
pub struct ResourceHint {
    pub url: String,
    pub hint_type: HintType,
    pub crossorigin: bool,
    pub r#as: Option<String>, // Resource type (script, style, image, etc.)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintType {
    DnsPrefetch,
    Preconnect,
    Prefetch,
    Preload,
    Prerender,
    Modulepreload,
}

impl ResourceHint {
    /// Parse from <link> attributes
    pub fn from_link(rel: &str, href: &str, crossorigin: bool, r#as: Option<&str>) -> Option<Self> {
        let hint_type = match rel {
            "dns-prefetch" => HintType::DnsPrefetch,
            "preconnect" => HintType::Preconnect,
            "prefetch" => HintType::Prefetch,
            "preload" => HintType::Preload,
            "prerender" => HintType::Prerender,
            "modulepreload" => HintType::Modulepreload,
            _ => return None,
        };
        
        Some(Self {
            url: href.to_string(),
            hint_type,
            crossorigin,
            r#as: r#as.map(String::from),
        })
    }
    
    /// Apply this hint to the predictive network
    pub fn apply(&self, network: &mut PredictiveNetwork) {
        if let Ok(parsed) = url::Url::parse(&self.url) {
            if let Some(host) = parsed.host_str() {
                match self.hint_type {
                    HintType::DnsPrefetch => {
                        network.dns.prefetch(host);
                    }
                    HintType::Preconnect => {
                        network.dns.prefetch(host);
                        network.preconnect(host);
                    }
                    HintType::Prefetch | HintType::Preload | HintType::Modulepreload => {
                        network.dns.prefetch(host);
                        // Could queue fetch request here
                    }
                    HintType::Prerender => {
                        // Full page prerender - expensive, use sparingly
                        network.dns.prefetch(host);
                        network.preconnect(host);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_predictive_network() {
        let mut net = PredictiveNetwork::new();
        
        net.record_visit("https://example.com/page1");
        net.update_page_links(vec![
            "https://example.com/page2".to_string(),
            "https://other.com/".to_string(),
        ]);
        
        assert!(net.page_links.contains("example.com"));
        assert!(net.page_links.contains("other.com"));
    }
    
    #[test]
    fn test_cross_tab_cache() {
        let mut net = PredictiveNetwork::new();
        
        let data = b"Hello, World!".to_vec();
        let hash1 = net.share_resource("text/plain", data.clone());
        let hash2 = net.share_resource("text/plain", data.clone());
        
        // Same content = same hash (content-addressed)
        assert_eq!(hash1, hash2);
    }
    
    #[test]
    fn test_resource_hint_parsing() {
        let hint = ResourceHint::from_link("preconnect", "https://cdn.example.com", true, None);
        assert!(hint.is_some());
        assert_eq!(hint.unwrap().hint_type, HintType::Preconnect);
    }
}
