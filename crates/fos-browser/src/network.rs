//! Enhanced Networking Layer
//!
//! Integrates fos-net for HTTP caching and security.

use std::time::Duration;
use fos_net::cache::HttpCache;
use fos_security::https::{SecureContext, MixedContentChecker, MixedContentResult};

/// Network manager for the browser
pub struct NetworkManager {
    /// HTTP response cache
    cache: HttpCache,
    /// Mixed content checker
    mixed_content: MixedContentChecker,
    /// User agent string
    user_agent: String,
}

impl NetworkManager {
    /// Create a new network manager
    pub fn new() -> Self {
        Self {
            // 500 entries, 25MB cache
            cache: HttpCache::new(500, 25 * 1024 * 1024),
            mixed_content: MixedContentChecker::new(),
            user_agent: format!(
                "fOS-Browser/0.1 (compatible; fOS Engine; +https://github.com/fosproject)"
            ),
        }
    }
    
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
        
        // Fetch from network
        log::debug!("Fetching from network: {}", url);
        
        let client = reqwest::blocking::Client::builder()
            .user_agent(&self.user_agent)
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| NetworkError::RequestFailed(e.to_string()))?;
        
        let response = client.get(url).send()
            .map_err(|e| NetworkError::RequestFailed(e.to_string()))?;
        
        let status = response.status().as_u16();
        
        if !response.status().is_success() {
            return Err(NetworkError::HttpError(status));
        }
        
        // Parse cache headers - clone the values to avoid borrow issues
        let cache_control = response.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        
        let etag = response.headers()
            .get("etag")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());
        
        let content_type = response.headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        
        let body = response.bytes()
            .map_err(|e| NetworkError::RequestFailed(e.to_string()))?
            .to_vec();
        
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
}

impl Default for NetworkManager {
    fn default() -> Self {
        Self::new()
    }
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
}
