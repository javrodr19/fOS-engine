//! Enhanced Networking Layer
//!
//! Integrates fos-net for HTTP caching, HTTP/2, and security.
//!
//! A single HTTP client is kept for the whole browser session, so
//! keep-alive connections are reused. All requests (pages, subresources,
//! and the requests page scripts make) share one cookie jar.

use std::time::Duration;
use std::collections::HashMap;
use fos_net::cache::HttpCache;
use fos_net::client::blocking::Client;
use fos_net::http2::Http2Connection;
use fos_net::network_opt::{PredictiveDns, RequestCoalescer};
use fos_net::{CookieContext, CookieJar, SharedCookieJar};
use fos_security::https::{SecureContext, MixedContentChecker, MixedContentResult};
use crate::charset;

/// User agent sent with every request. The `Mozilla/5.0` prefix is what
/// servers expect from browsers; some reject clients without it.
const USER_AGENT: &str = "Mozilla/5.0 (compatible; fOS-Browser/0.1; +https://github.com/fosproject)";

/// Most subresource requests `fetch_many` runs at once
const MAX_PARALLEL_FETCHES: usize = 6;

/// Accept header for document navigations
const ACCEPT_DOCUMENT: &str = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";

/// Network manager for the browser
/// Integrates HTTP caching, HTTP/2 multiplexing, predictive DNS, and security
pub struct NetworkManager {
    /// HTTP response cache
    cache: HttpCache,
    /// Mixed content checker
    mixed_content: MixedContentChecker,
    /// User agent string
    user_agent: String,
    /// Session HTTP client (keep-alive connection pool)
    client: Client,
    /// The session's cookies
    cookies: SharedCookieJar,
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
        let cookies = CookieJar::shared();
        let client = Self::new_client(&cookies);

        Self {
            // 500 entries, 25MB cache
            cache: HttpCache::new(500, 25 * 1024 * 1024),
            mixed_content: MixedContentChecker::new(),
            user_agent: USER_AGENT.to_string(),
            client,
            cookies,
            http2_pool: HashMap::new(),
            predictive_dns: PredictiveDns::new(),
            coalescer: RequestCoalescer::new(5, 50), // Batch 5 requests or 50ms
        }
    }

    fn new_client(cookies: &SharedCookieJar) -> Client {
        Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .default_header("Accept-Language", "en-US,en;q=0.9")
            .cookie_jar(cookies.clone())
            .build()
            .unwrap_or_default()
    }

    /// User agent string sent with requests
    pub fn user_agent(&self) -> &str {
        &self.user_agent
    }

    /// The session's cookie jar (pages share it with their scripts)
    pub fn cookie_jar(&self) -> &SharedCookieJar {
        &self.cookies
    }

    // === HTTP/2 Connection Pool ===

    /// Get or create HTTP/2 connection for a host
    pub fn get_http2_connection(&mut self, host: &str) -> Option<&mut Http2Connection> {
        if !self.http2_pool.contains_key(host) {
            // Create new HTTP/2 connection
            self.http2_pool.insert(host.to_string(), Http2Connection::new_client());
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

    /// Fetch a URL with caching. Non-2xx responses are errors.
    pub fn fetch(&mut self, url: &str, page_url: Option<&str>) -> Result<FetchResult, NetworkError> {
        let result = self.fetch_any_status(url, page_url, None, false)?;
        if !(200..300).contains(&result.status) {
            return Err(NetworkError::HttpError(result.status));
        }
        Ok(result)
    }

    /// Fetch a URL, returning the response for any HTTP status. A
    /// `navigation` loads a page, followed from `page_url` (None when the
    /// user started it); otherwise `page_url` is the page loading a
    /// subresource.
    fn fetch_any_status(
        &mut self,
        url: &str,
        page_url: Option<&str>,
        accept: Option<&str>,
        navigation: bool,
    ) -> Result<FetchResult, NetworkError> {
        if let Some(hit) = self.cached(url) {
            return Ok(hit);
        }
        // A navigation replaces the page, so the page's mixed-content
        // rules do not apply to it
        let url = self.admit(url, if navigation { None } else { page_url })?;
        if let Some(hit) = self.cached(&url) {
            return Ok(hit);
        }

        // Fetch from network
        log::debug!("Fetching from network: {}", url);
        let mut headers = request_headers(page_url, &url);
        if let Some(accept) = accept {
            headers.push(("Accept".to_string(), accept.to_string()));
        }
        let context = if navigation {
            CookieContext::navigation(page_url, "GET")
        } else {
            CookieContext::subresource(page_url, "GET")
        };
        let response = self.client.request_in("GET", &url, Some(headers), None, &context)
            .map_err(|e| NetworkError::RequestFailed(format!("{}", e)))?;
        Ok(self.store(&url, response))
    }

    /// A cached response for `url`
    fn cached(&mut self, url: &str) -> Option<FetchResult> {
        let entry = self.cache.get(url)?;
        log::debug!("Cache hit for {}", url);
        Some(FetchResult {
            body: entry.body.clone(),
            content_type: entry.content_type.clone(),
            from_cache: true,
            status: 200,
            url: url.to_string(),
        })
    }

    /// Decide whether the page at `page_url` may load `url`: the URL to
    /// fetch (possibly upgraded to HTTPS), or why not
    fn admit(&mut self, url: &str, page_url: Option<&str>) -> Result<String, NetworkError> {
        let mut url = url.to_string();
        // Check mixed content if we have a page context
        if let Some(page) = page_url {
            if SecureContext::is_potentially_trustworthy(page) {
                let content_type = MixedContentChecker::get_content_type("fetch");
                match self.mixed_content.should_block(page, &url, content_type) {
                    MixedContentResult::Block => {
                        log::warn!("Mixed content blocked: {}", url);
                        return Err(NetworkError::MixedContentBlocked(url));
                    }
                    MixedContentResult::Upgrade => {
                        if let Some(upgraded) = url.strip_prefix("http://") {
                            url = format!("https://{}", upgraded);
                            log::info!("Upgraded to HTTPS: {}", url);
                        }
                    }
                    MixedContentResult::Warn => log::warn!("Mixed content warning: {}", url),
                    MixedContentResult::Allow => {}
                }
            }
        }
        // Prefetch DNS for this host (learns patterns)
        if let Ok(parsed) = fos_engine::url::Url::parse(&url) {
            if let Some(host) = parsed.host_str() {
                self.predictive_dns.prefetch(host);
            }
        }
        Ok(url)
    }

    /// Turn a response into a result, caching it when allowed
    fn store(&mut self, url: &str, response: fos_net::Response) -> FetchResult {
        let status = response.status;
        let cache_control = response.header("cache-control").unwrap_or_default().to_ascii_lowercase();
        let etag = response.header("etag").map(str::to_owned);
        let content_type = response.header("content-type")
            .unwrap_or("application/octet-stream")
            .to_string();
        let final_url = if response.url.is_empty() { url.to_string() } else { response.url };
        let body = response.body;

        // Store in cache if cacheable. Redirected responses are not cached
        // under the original URL so the final URL is never lost.
        let cacheable = status == 200
            && final_url == url
            && !cache_control.contains("no-store")
            && !cache_control.contains("no-cache");
        if cacheable {
            let max_age = parse_max_age(&cache_control).unwrap_or(Duration::from_secs(300));
            self.cache.put(url, body.clone(), &content_type, etag, max_age);
            log::debug!("Cached response for {} ({} bytes, TTL {:?})", url, body.len(), max_age);
        }

        FetchResult {
            body,
            content_type,
            from_cache: false,
            status,
            url: final_url,
        }
    }

    /// Fetch several subresources of the page at `page_url` (stylesheets)
    /// at once: cache hits are answered directly, the rest are requested
    /// in parallel. Non-2xx responses are errors.
    pub fn fetch_many(&mut self, urls: &[String], page_url: Option<&str>) -> Vec<Result<FetchResult, NetworkError>> {
        let mut out: Vec<Option<Result<FetchResult, NetworkError>>> = urls.iter().map(|u| self.cached(u).map(Ok)).collect();
        let mut pending: Vec<(usize, String)> = Vec::new();
        for (i, url) in urls.iter().enumerate() {
            if out[i].is_some() {
                continue;
            }
            match self.admit(url, page_url) {
                Ok(u) => match self.cached(&u) {
                    Some(hit) => out[i] = Some(Ok(hit)),
                    None => pending.push((i, u)),
                },
                Err(e) => out[i] = Some(Err(e)),
            }
        }

        if pending.len() == 1 {
            // One request: the session client can reuse a connection
            let (i, url) = pending.pop().unwrap();
            let context = CookieContext::subresource(page_url, "GET");
            out[i] = Some(
                self.client.request_in("GET", &url, Some(request_headers(page_url, &url)), None, &context)
                    .map(|r| self.store(&url, r))
                    .map_err(|e| NetworkError::RequestFailed(e.to_string())),
            );
        } else if !pending.is_empty() {
            // A few connections at a time (as browsers limit them per
            // host); each worker reuses its connection for the next URL
            let cookies = &self.cookies;
            let workers = pending.len().min(MAX_PARALLEL_FETCHES);
            let queue = std::sync::Mutex::new(pending.into_iter().collect::<std::collections::VecDeque<_>>());
            let responses: Vec<(usize, String, Result<fos_net::Response, String>)> = std::thread::scope(|scope| {
                let handles: Vec<_> = (0..workers)
                    .map(|_| {
                        let queue = &queue;
                        scope.spawn(move || {
                            let mut client = Self::new_client(cookies);
                            let mut done = Vec::new();
                            loop {
                                let next = queue.lock().unwrap_or_else(|p| p.into_inner()).pop_front();
                                let Some((i, url)) = next else { break };
                                let context = CookieContext::subresource(page_url, "GET");
                                let headers = request_headers(page_url, &url);
                                let result = client.request_in("GET", &url, Some(headers), None, &context).map_err(|e| e.to_string());
                                done.push((i, url, result));
                            }
                            done
                        })
                    })
                    .collect();
                handles.into_iter().filter_map(|h| h.join().ok()).flatten().collect()
            });
            for (i, url, result) in responses {
                out[i] = Some(match result {
                    Ok(r) => Ok(self.store(&url, r)),
                    Err(e) => Err(NetworkError::RequestFailed(e)),
                });
            }
        }

        out.into_iter()
            .map(|r| match r {
                Some(Ok(r)) if !(200..300).contains(&r.status) => Err(NetworkError::HttpError(r.status)),
                Some(r) => r,
                None => Err(NetworkError::RequestFailed("request did not complete".into())),
            })
            .collect()
    }

    /// Fetch HTML page (convenience method)
    pub fn fetch_html(&mut self, url: &str) -> Result<String, NetworkError> {
        Ok(self.fetch_page(url)?.html)
    }

    /// Fetch a document for display.
    ///
    /// Error statuses still return the server's page (like any browser shows
    /// a site's 404 page). The body is decoded using its declared or sniffed
    /// character encoding; plain text is wrapped for display, and other
    /// content types produce a short explanatory page.
    pub fn fetch_page(&mut self, url: &str) -> Result<FetchedPage, NetworkError> {
        self.fetch_page_from(url, None)
    }

    /// Fetch a document for display, navigating from the page at
    /// `initiator` (a link followed, a script's navigation); None when the
    /// user started it. This decides the cookies sent and the Referer.
    pub fn fetch_page_from(&mut self, url: &str, initiator: Option<&str>) -> Result<FetchedPage, NetworkError> {
        let result = self.fetch_any_status(url, initiator, Some(ACCEPT_DOCUMENT), true)?;
        let mime = result.content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        let html = if mime.is_empty() || mime.contains("html") || mime.contains("xml") {
            charset::decode_html(result.body, Some(&result.content_type))
        } else if mime.starts_with("text/") {
            let text = charset::decode_html(result.body, Some(&result.content_type));
            format!("<html><body><pre>{}</pre></body></html>", escape_html(&text))
        } else {
            format!(
                "<html><head><title>{0}</title></head><body><h1>Cannot display this file</h1>\
                 <p>The resource at {1} has type <code>{0}</code> ({2} bytes).</p></body></html>",
                escape_html(&mime),
                escape_html(&result.url),
                result.body.len(),
            )
        };

        Ok(FetchedPage {
            html,
            url: result.url,
            status: result.status,
            from_cache: result.from_cache,
        })
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

    /// Release memory held for performance (response cache, idle connections)
    pub fn trim_memory(&mut self) {
        self.cache.clear();
        self.client.clear_idle_connections();
        self.http2_pool.clear();
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

/// Headers a request from the page at `page_url` carries (its Referer)
fn request_headers(page_url: Option<&str>, url: &str) -> Vec<(String, String)> {
    page_url
        .and_then(|page| crate::script_fetch::referrer_for(page, url))
        .map(|r| vec![("Referer".to_string(), r)])
        .unwrap_or_default()
}

/// Escape text for inclusion in HTML
fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 16);
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
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
    /// Final URL after redirects
    pub url: String,
}

/// A document fetched for display
#[derive(Debug)]
pub struct FetchedPage {
    /// Decoded HTML
    pub html: String,
    /// Final URL after redirects (the base URL for relative links)
    pub url: String,
    /// HTTP status code
    pub status: u16,
    /// Whether this came from cache
    pub from_cache: bool,
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
        assert!(manager.user_agent().starts_with("Mozilla/5.0"));
    }

    #[test]
    fn test_network_stats() {
        let manager = NetworkManager::new();
        let stats = manager.stats();
        assert_eq!(stats.cache_entries, 0);
        assert_eq!(stats.http2_connections, 0);
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("<a href=\"x\">&</a>"), "&lt;a href=&quot;x&quot;&gt;&amp;&lt;/a&gt;");
    }
}
