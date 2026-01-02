//! CORS (Cross-Origin Resource Sharing)
//!
//! Full CORS implementation with preflight caching and header validation.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Origin representation for CORS
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
}

impl Origin {
    /// Create new origin
    pub fn new(scheme: &str, host: &str, port: Option<u16>) -> Self {
        Self {
            scheme: scheme.to_lowercase(),
            host: host.to_lowercase(),
            port,
        }
    }
    
    /// Parse origin from URL
    pub fn from_url(url: &str) -> Option<Self> {
        let url = url.trim();
        let scheme_end = url.find("://")?;
        let scheme = &url[..scheme_end];
        let rest = &url[scheme_end + 3..];
        
        let path_start = rest.find('/').unwrap_or(rest.len());
        let host_port = &rest[..path_start];
        
        let (host, port) = if let Some(colon) = host_port.rfind(':') {
            let potential_port = &host_port[colon + 1..];
            if let Ok(p) = potential_port.parse::<u16>() {
                (&host_port[..colon], Some(p))
            } else {
                (host_port, None)
            }
        } else {
            (host_port, None)
        };
        
        Some(Self::new(scheme, host, port))
    }
    
    /// Get effective port (default for scheme)
    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or_else(|| match self.scheme.as_str() {
            "http" | "ws" => 80,
            "https" | "wss" => 443,
            _ => 0,
        })
    }
    
    /// Check if same origin
    pub fn is_same_origin(&self, other: &Origin) -> bool {
        self.scheme == other.scheme
            && self.host == other.host
            && self.effective_port() == other.effective_port()
    }
    
    /// Check if opaque origin
    pub fn is_opaque(&self) -> bool {
        matches!(self.scheme.as_str(), "data" | "file" | "blob")
    }
    
    /// Serialize to string
    pub fn serialize(&self) -> String {
        if self.is_opaque() {
            return "null".to_string();
        }
        
        let port_str = match (self.scheme.as_str(), self.port) {
            ("http", Some(80)) | ("https", Some(443)) => String::new(),
            (_, Some(p)) => format!(":{}", p),
            (_, None) => String::new(),
        };
        
        format!("{}://{}{}", self.scheme, self.host, port_str)
    }
}

/// CORS request mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CorsMode {
    /// No CORS checks
    #[default]
    NoCors,
    /// Full CORS handling
    Cors,
    /// Same-origin only
    SameOrigin,
    /// Navigation request
    Navigate,
}

/// CORS credentials mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CredentialsMode {
    /// Never send credentials
    Omit,
    /// Send credentials for same-origin
    #[default]
    SameOrigin,
    /// Always send credentials
    Include,
}

/// Result of CORS check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorsCheck {
    /// Same-origin request, no CORS needed
    SameOrigin,
    /// Simple cross-origin request (no preflight)
    SimpleRequest,
    /// Preflight OPTIONS request required
    PreflightRequired,
}

/// CORS preflight request
#[derive(Debug, Clone)]
pub struct PreflightRequest {
    pub origin: Origin,
    pub method: String,
    pub headers: Vec<String>,
}

/// CORS preflight response
#[derive(Debug, Clone, Default)]
pub struct PreflightResponse {
    pub allowed: bool,
    pub allow_origin: Option<String>,
    pub allow_methods: Vec<String>,
    pub allow_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age: Option<u32>,
    pub expose_headers: Vec<String>,
}

/// Cache key for preflight results
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CorsKey {
    origin: String,
    url: String,
}

/// Cached preflight entry
#[derive(Debug, Clone)]
struct CorsEntry {
    response: PreflightResponse,
    expires_at: Instant,
}

/// Simple (CORS-safelisted) methods that don't require preflight
const SIMPLE_METHODS: &[&str] = &["GET", "HEAD", "POST"];

/// Simple (CORS-safelisted) headers that don't require preflight
const SIMPLE_HEADERS: &[&str] = &[
    "accept",
    "accept-language",
    "content-language",
    "content-type",
];

/// Simple content types for POST
const SIMPLE_CONTENT_TYPES: &[&str] = &[
    "application/x-www-form-urlencoded",
    "multipart/form-data",
    "text/plain",
];

/// Forbidden headers that cannot be set
const FORBIDDEN_HEADERS: &[&str] = &[
    "accept-charset",
    "accept-encoding",
    "access-control-request-headers",
    "access-control-request-method",
    "connection",
    "content-length",
    "cookie",
    "cookie2",
    "date",
    "dnt",
    "expect",
    "host",
    "keep-alive",
    "origin",
    "referer",
    "te",
    "trailer",
    "transfer-encoding",
    "upgrade",
    "via",
];

/// CORS handler with preflight cache
#[derive(Debug, Default)]
pub struct CorsHandler {
    /// Current request origin
    origin: Option<Origin>,
    /// Preflight cache
    cache: HashMap<CorsKey, CorsEntry>,
    /// Cache max entries
    max_cache_entries: usize,
}

impl CorsHandler {
    /// Create new CORS handler
    pub fn new() -> Self {
        Self {
            origin: None,
            cache: HashMap::new(),
            max_cache_entries: 100,
        }
    }
    
    /// Set the request origin
    pub fn set_origin(&mut self, origin: Origin) {
        self.origin = Some(origin);
    }
    
    /// Get current origin
    pub fn origin(&self) -> Option<&Origin> {
        self.origin.as_ref()
    }
    
    /// Classify a request for CORS handling
    pub fn classify_request(
        &self,
        target_url: &str,
        method: &str,
        headers: &[(String, String)],
    ) -> CorsCheck {
        // Check same-origin
        if let (Some(origin), Some(target)) = (&self.origin, Origin::from_url(target_url)) {
            if origin.is_same_origin(&target) {
                return CorsCheck::SameOrigin;
            }
        }
        
        // Check if simple request
        if self.is_simple_request(method, headers) {
            return CorsCheck::SimpleRequest;
        }
        
        CorsCheck::PreflightRequired
    }
    
    /// Check if request qualifies as simple (no preflight needed)
    pub fn is_simple_request(&self, method: &str, headers: &[(String, String)]) -> bool {
        // Method must be simple
        let method_upper = method.to_uppercase();
        if !SIMPLE_METHODS.contains(&method_upper.as_str()) {
            return false;
        }
        
        // Check headers
        for (name, value) in headers {
            let name_lower = name.to_lowercase();
            
            // Must be simple header
            if !SIMPLE_HEADERS.contains(&name_lower.as_str()) {
                return false;
            }
            
            // Content-Type must be simple
            if name_lower == "content-type" {
                let content_type = value.split(';').next().unwrap_or("").trim().to_lowercase();
                if !SIMPLE_CONTENT_TYPES.contains(&content_type.as_str()) {
                    return false;
                }
            }
        }
        
        true
    }
    
    /// Check preflight cache
    pub fn check_cache(&mut self, target_url: &str, method: &str, headers: &[String]) -> Option<PreflightResponse> {
        let origin = self.origin.as_ref()?.serialize();
        let key = CorsKey {
            origin,
            url: target_url.to_string(),
        };
        
        // Check if cached and not expired
        if let Some(entry) = self.cache.get(&key) {
            if entry.expires_at > Instant::now() {
                let response = &entry.response;
                
                // Verify method is allowed
                let method_upper = method.to_uppercase();
                if !response.allow_methods.iter().any(|m| m.to_uppercase() == method_upper) {
                    if !SIMPLE_METHODS.contains(&method_upper.as_str()) {
                        return None;
                    }
                }
                
                // Verify headers are allowed
                for header in headers {
                    let header_lower = header.to_lowercase();
                    if !SIMPLE_HEADERS.contains(&header_lower.as_str()) {
                        if !response.allow_headers.iter().any(|h| h.to_lowercase() == header_lower) {
                            return None;
                        }
                    }
                }
                
                return Some(response.clone());
            } else {
                // Expired, remove
                self.cache.remove(&key);
            }
        }
        
        None
    }
    
    /// Cache a preflight response
    pub fn cache_preflight(&mut self, target_url: &str, response: PreflightResponse) {
        let Some(origin) = self.origin.as_ref() else { return };
        
        let key = CorsKey {
            origin: origin.serialize(),
            url: target_url.to_string(),
        };
        
        let max_age = response.max_age.unwrap_or(5);
        let expires_at = Instant::now() + Duration::from_secs(max_age as u64);
        
        // Evict if at capacity
        if self.cache.len() >= self.max_cache_entries {
            self.evict_expired();
        }
        
        self.cache.insert(key, CorsEntry { response, expires_at });
    }
    
    /// Build preflight OPTIONS request headers
    pub fn build_preflight_headers(
        &self,
        method: &str,
        headers: &[String],
    ) -> Vec<(String, String)> {
        let mut preflight_headers = Vec::new();
        
        if let Some(origin) = &self.origin {
            preflight_headers.push(("Origin".to_string(), origin.serialize()));
        }
        
        preflight_headers.push((
            "Access-Control-Request-Method".to_string(),
            method.to_uppercase(),
        ));
        
        if !headers.is_empty() {
            preflight_headers.push((
                "Access-Control-Request-Headers".to_string(),
                headers.join(", "),
            ));
        }
        
        preflight_headers
    }
    
    /// Parse preflight response headers
    pub fn parse_preflight_response(&self, headers: &[(String, String)]) -> PreflightResponse {
        let mut response = PreflightResponse::default();
        
        for (name, value) in headers {
            let name_lower = name.to_lowercase();
            
            match name_lower.as_str() {
                "access-control-allow-origin" => {
                    response.allow_origin = Some(value.clone());
                    response.allowed = true;
                }
                "access-control-allow-methods" => {
                    response.allow_methods = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                "access-control-allow-headers" => {
                    response.allow_headers = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                "access-control-allow-credentials" => {
                    response.allow_credentials = value.eq_ignore_ascii_case("true");
                }
                "access-control-max-age" => {
                    response.max_age = value.parse().ok();
                }
                "access-control-expose-headers" => {
                    response.expose_headers = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .collect();
                }
                _ => {}
            }
        }
        
        response
    }
    
    /// Validate CORS response for actual request
    pub fn validate_response(
        &self,
        response_headers: &[(String, String)],
        credentials: CredentialsMode,
    ) -> Result<PreflightResponse, CorsError> {
        let response = self.parse_preflight_response(response_headers);
        
        // Check Access-Control-Allow-Origin
        let Some(allow_origin) = &response.allow_origin else {
            return Err(CorsError::MissingAllowOrigin);
        };
        
        // Validate origin
        if let Some(origin) = &self.origin {
            let origin_str = origin.serialize();
            if allow_origin != "*" && allow_origin != &origin_str {
                return Err(CorsError::OriginMismatch {
                    expected: origin_str,
                    got: allow_origin.clone(),
                });
            }
            
            // Wildcard not allowed with credentials
            if allow_origin == "*" && credentials == CredentialsMode::Include {
                return Err(CorsError::WildcardWithCredentials);
            }
        }
        
        // Check credentials
        if credentials == CredentialsMode::Include && !response.allow_credentials {
            return Err(CorsError::CredentialsNotAllowed);
        }
        
        Ok(response)
    }
    
    /// Filter response headers based on exposed headers
    pub fn filter_response_headers<'a>(
        &self,
        headers: &'a [(String, String)],
        exposed: &[String],
    ) -> Vec<&'a (String, String)> {
        // CORS-safelisted response headers
        const SAFE_HEADERS: &[&str] = &[
            "cache-control",
            "content-language",
            "content-length",
            "content-type",
            "expires",
            "last-modified",
            "pragma",
        ];
        
        headers
            .iter()
            .filter(|(name, _)| {
                let name_lower = name.to_lowercase();
                SAFE_HEADERS.contains(&name_lower.as_str())
                    || exposed.iter().any(|e| e.to_lowercase() == name_lower)
            })
            .collect()
    }
    
    /// Check if header is forbidden
    pub fn is_forbidden_header(name: &str) -> bool {
        let name_lower = name.to_lowercase();
        FORBIDDEN_HEADERS.contains(&name_lower.as_str())
            || name_lower.starts_with("sec-")
            || name_lower.starts_with("proxy-")
    }
    
    /// Evict expired cache entries
    fn evict_expired(&mut self) {
        let now = Instant::now();
        self.cache.retain(|_, entry| entry.expires_at > now);
    }
    
    /// Clear the preflight cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

/// CORS errors
#[derive(Debug, Clone, thiserror::Error)]
pub enum CorsError {
    #[error("Missing Access-Control-Allow-Origin header")]
    MissingAllowOrigin,
    
    #[error("Origin mismatch: expected {expected}, got {got}")]
    OriginMismatch { expected: String, got: String },
    
    #[error("Wildcard origin not allowed with credentials")]
    WildcardWithCredentials,
    
    #[error("Credentials not allowed by server")]
    CredentialsNotAllowed,
    
    #[error("Method not allowed: {0}")]
    MethodNotAllowed(String),
    
    #[error("Header not allowed: {0}")]
    HeaderNotAllowed(String),
    
    #[error("Preflight request failed")]
    PreflightFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_origin_parse() {
        let origin = Origin::from_url("https://example.com:8080/path").unwrap();
        assert_eq!(origin.scheme, "https");
        assert_eq!(origin.host, "example.com");
        assert_eq!(origin.port, Some(8080));
    }
    
    #[test]
    fn test_origin_same_origin() {
        let o1 = Origin::new("https", "example.com", None);
        let o2 = Origin::new("https", "example.com", Some(443));
        let o3 = Origin::new("http", "example.com", None);
        
        assert!(o1.is_same_origin(&o2));
        assert!(!o1.is_same_origin(&o3));
    }
    
    #[test]
    fn test_simple_request() {
        let handler = CorsHandler::new();
        
        // Simple GET
        assert!(handler.is_simple_request("GET", &[]));
        
        // Simple POST with simple content-type
        let headers = vec![
            ("Content-Type".to_string(), "text/plain".to_string()),
        ];
        assert!(handler.is_simple_request("POST", &headers));
        
        // Non-simple method
        assert!(!handler.is_simple_request("PUT", &[]));
        
        // Non-simple header
        let headers = vec![
            ("X-Custom-Header".to_string(), "value".to_string()),
        ];
        assert!(!handler.is_simple_request("GET", &headers));
    }
    
    #[test]
    fn test_classify_request() {
        let mut handler = CorsHandler::new();
        handler.set_origin(Origin::new("https", "example.com", None));
        
        // Same origin
        let check = handler.classify_request(
            "https://example.com/api",
            "GET",
            &[],
        );
        assert_eq!(check, CorsCheck::SameOrigin);
        
        // Simple cross-origin
        let check = handler.classify_request(
            "https://other.com/api",
            "GET",
            &[],
        );
        assert_eq!(check, CorsCheck::SimpleRequest);
        
        // Preflight required
        let check = handler.classify_request(
            "https://other.com/api",
            "PUT",
            &[],
        );
        assert_eq!(check, CorsCheck::PreflightRequired);
    }
    
    #[test]
    fn test_parse_preflight_response() {
        let handler = CorsHandler::new();
        
        let headers = vec![
            ("Access-Control-Allow-Origin".to_string(), "https://example.com".to_string()),
            ("Access-Control-Allow-Methods".to_string(), "GET, POST, PUT".to_string()),
            ("Access-Control-Allow-Headers".to_string(), "X-Custom-Header".to_string()),
            ("Access-Control-Max-Age".to_string(), "3600".to_string()),
        ];
        
        let response = handler.parse_preflight_response(&headers);
        
        assert!(response.allowed);
        assert_eq!(response.allow_origin, Some("https://example.com".to_string()));
        assert_eq!(response.allow_methods.len(), 3);
        assert_eq!(response.max_age, Some(3600));
    }
    
    #[test]
    fn test_forbidden_headers() {
        assert!(CorsHandler::is_forbidden_header("Cookie"));
        assert!(CorsHandler::is_forbidden_header("Sec-Fetch-Mode"));
        assert!(CorsHandler::is_forbidden_header("Proxy-Authorization"));
        assert!(!CorsHandler::is_forbidden_header("X-Custom"));
    }
    
    #[test]
    fn test_preflight_cache() {
        let mut handler = CorsHandler::new();
        handler.set_origin(Origin::new("https", "example.com", None));
        
        let response = PreflightResponse {
            allowed: true,
            allow_origin: Some("https://example.com".to_string()),
            allow_methods: vec!["GET".to_string(), "PUT".to_string()],
            allow_headers: vec!["X-Custom".to_string()],
            allow_credentials: false,
            max_age: Some(3600),
            expose_headers: Vec::new(),
        };
        
        handler.cache_preflight("https://api.com/", response);
        
        // Should find in cache
        let cached = handler.check_cache(
            "https://api.com/",
            "GET",
            &[],
        );
        assert!(cached.is_some());
        
        // Should not find for different URL
        let cached = handler.check_cache(
            "https://other.com/",
            "GET",
            &[],
        );
        assert!(cached.is_none());
    }
}
