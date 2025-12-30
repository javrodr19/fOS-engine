//! Enhanced Networking Layer
//!
//! Integrates fos-net for HTTP caching, HTTP/2, and security.

use std::time::Duration;
use std::collections::HashMap;
use fos_net::cache::HttpCache;
use fos_net::http2::Http2Connection;
use fos_net::network_opt::{PredictiveDns, RequestCoalescer};
use fos_security::https::{SecureContext, MixedContentChecker, MixedContentResult};

/// Network manager for the browser
/// Integrates HTTP caching, HTTP/2 multiplexing, predictive DNS, and security
pub struct NetworkManager {
    /// HTTP response cache
    cache: HttpCache,
    /// Mixed content checker
    mixed_content: MixedContentChecker,
    /// User agent string
    user_agent: String,
    /// HTTP/2 connection pool by origin
    http2_pool: HashMap<String, Http2Connection>,
    /// Predictive DNS resolver
    predictive_dns: PredictiveDns,
    /// Request coalescer for batching
    coalescer: RequestCoalescer,
}

impl NetworkManager {
    /// Create a new network manager with all fos-net features
    pub fn new() -> Self {
        Self {
            // 500 entries, 25MB cache
            cache: HttpCache::new(500, 25 * 1024 * 1024),
            mixed_content: MixedContentChecker::new(),
            user_agent: format!(
                "fOS-Browser/0.1 (compatible; fOS Engine; +https://github.com/fosproject)"
            ),
            http2_pool: HashMap::new(),
            predictive_dns: PredictiveDns::new(),
            coalescer: RequestCoalescer::new(5, 50), // Batch 5 requests or 50ms
        }
    }
    
    // === HTTP/2 Connection Pool ===
    
    /// Get or create HTTP/2 connection for a host
    pub fn get_http2_connection(&mut self, host: &str) -> Option<&mut Http2Connection> {
        if !self.http2_pool.contains_key(host) {
            // Create new HTTP/2 connection
            self.http2_pool.insert(host.to_string(), Http2Connection::new());
        }
        self.http2_pool.get_mut(host)
    }
    
    /// Check if HTTP/2 is available for host
    pub fn has_http2(&self, host: &str) -> bool {
        self.http2_pool.contains_key(host)
    }
    
    // === Predictive DNS ===
    
    /// Prefetch DNS for a host (call for visible links)
    pub fn prefetch_dns(&mut self, host: &str) {
        self.predictive_dns.prefetch(host);
    }
    
    /// Record navigation for pattern learning
    pub fn record_navigation(&mut self, from_page: &str, to_host: &str) {
        self.predictive_dns.record_access(from_page, to_host);
    }
    
    /// Predict and prefetch DNS based on current page
    pub fn predict_dns(&mut self, current_path: &str) {
        self.predictive_dns.predict_and_prefetch(current_path);
    }
    
    /// Process pending DNS prefetch
    pub fn process_dns_prefetch(&mut self) -> Option<String> {
        self.predictive_dns.pop_prefetch()
    }
    
    // === Fetch with caching ===
    
    /// Fetch a URL with caching
    pub fn fetch(&mut self, url: &str, page_url: Option<&str>) -> Result<FetchResult, NetworkError> {
        // Check cache first
        if let Some(entry) = self.cache.get(url) {
            log::debug!("Cache hit for {}", url);
            return Ok(FetchResult {
                body: entry.body.clone(),
                content_type: entry.content_type.clone(),
                from_cache: true,
                status: 200,
            });
        }
        
        // Check mixed content if we have a page context
        if let Some(page) = page_url {
            let page_secure = SecureContext::is_potentially_trustworthy(page);
            if page_secure {
                let content_type = MixedContentChecker::get_content_type("fetch");
                let result = self.mixed_content.should_block(page, url, content_type);
                
                match result {
                    MixedContentResult::Block => {
                        log::warn!("Mixed content blocked: {}", url);
                        return Err(NetworkError::MixedContentBlocked(url.to_string()));
                    }
                    MixedContentResult::Upgrade => {
                        // Upgrade to HTTPS
                        if let Some(upgraded) = url.strip_prefix("http://") {
                            let new_url = format!("https://{}", upgraded);
                            log::info!("Upgraded to HTTPS: {}", new_url);
                            return self.fetch(&new_url, page_url);
                        }
                    }
                    MixedContentResult::Warn => {
                        log::warn!("Mixed content warning: {}", url);
                        // Continue with fetch but warn
                    }
                    MixedContentResult::Allow => {}
                }
            }
        }
        
        // Prefetch DNS for this host (learns patterns)
        if let Ok(parsed) = fos_engine::url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                self.predictive_dns.prefetch(host);
            }
        }
        
        // Fetch from network
        log::debug!("Fetching from network: {}", url);
        
        let mut client = fos_net::client::blocking::Client::new();
        
        let response = client.get(url)
            .map_err(|e| NetworkError::RequestFailed(format!("{}", e)))?;
        
        let status = response.status;
        
        if !response.is_success() {
            return Err(NetworkError::HttpError(status));
        }
        
        // Parse cache headers
        let cache_control = response.headers.iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("cache-control"))
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        
        let etag = response.headers.iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("etag"))
            .map(|(_, v)| v.clone());
        
        let content_type = response.headers.iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        
        let body = response.body;
        
        // Determine cache TTL
        let max_age = parse_max_age(&cache_control).unwrap_or(Duration::from_secs(300));
        
        // Store in cache if cacheable
        if !cache_control.contains("no-store") && status == 200 {
            self.cache.put(url, body.clone(), &content_type, etag, max_age);
            log::debug!("Cached response for {} ({} bytes, TTL {:?})", url, body.len(), max_age);
        }
        
        Ok(FetchResult {
            body,
            content_type,
            from_cache: false,
            status,
        })
    }
    
    /// Fetch HTML page (convenience method)
    pub fn fetch_html(&mut self, url: &str) -> Result<String, NetworkError> {
        let result = self.fetch(url, None)?;
        String::from_utf8(result.body)
            .map_err(|e| NetworkError::InvalidEncoding(e.to_string()))
    }
    
    /// Check if a URL is cached
    pub fn is_cached(&self, url: &str) -> bool {
        self.cache.contains(url)
    }
    
    /// Get cache statistics
    pub fn cache_stats(&self) -> fos_net::cache::CacheStats {
        self.cache.stats()
    }
    
    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
    
    /// Clean up expired entries
    pub fn cleanup(&mut self) {
        self.cache.cleanup();
    }
    
    /// Get network statistics
    pub fn stats(&self) -> NetworkStats {
        NetworkStats {
            cache_entries: self.cache.stats().entry_count,
            cache_size_bytes: self.cache.stats().total_size,
            http2_connections: self.http2_pool.len(),
        }
    }
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub cache_entries: usize,
    pub cache_size_bytes: usize,
    pub http2_connections: usize,
}

/// Result of a fetch operation
#[derive(Debug)]
pub struct FetchResult {
    /// Response body
    pub body: Vec<u8>,
    /// Content type
    pub content_type: String,
    /// Whether this came from cache
    pub from_cache: bool,
    /// HTTP status code
    pub status: u16,
}

/// Network error
#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("HTTP error: {0}")]
    HttpError(u16),
    
    #[error("Request failed: {0}")]
    RequestFailed(String),
    
    #[error("Mixed content blocked: {0}")]
    MixedContentBlocked(String),
    
    #[error("CORS blocked: {0}")]
    CorsBlocked(String),
    
    #[error("Invalid encoding: {0}")]
    InvalidEncoding(String),
}

/// Parse max-age from Cache-Control header
fn parse_max_age(cache_control: &str) -> Option<Duration> {
    for directive in cache_control.split(',') {
        let directive = directive.trim();
        if let Some(value) = directive.strip_prefix("max-age=") {
            if let Ok(secs) = value.trim().parse::<u64>() {
                return Some(Duration::from_secs(secs));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_max_age() {
        assert_eq!(parse_max_age("max-age=3600"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_max_age("public, max-age=86400"), Some(Duration::from_secs(86400)));
        assert_eq!(parse_max_age("no-cache"), None);
    }
    
    #[test]
    fn test_network_manager_creation() {
        let manager = NetworkManager::new();
        assert!(!manager.is_cached("https://example.com"));
    }
    
    #[test]
    fn test_network_stats() {
        let manager = NetworkManager::new();
        let stats = manager.stats();
        assert_eq!(stats.cache_entries, 0);
        assert_eq!(stats.http2_connections, 0);
    }
}
