//! Fetch API Implementation
//!
//! Web Fetch API for making HTTP requests.

use std::collections::HashMap;

/// HTTP method
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl HttpMethod {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "DELETE" => HttpMethod::Delete,
            "PATCH" => HttpMethod::Patch,
            "HEAD" => HttpMethod::Head,
            "OPTIONS" => HttpMethod::Options,
            _ => HttpMethod::Get,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
        }
    }
}

/// Request credentials mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestCredentials {
    Omit,
    #[default]
    SameOrigin,
    Include,
}

/// Request cache mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestCache {
    #[default]
    Default,
    NoStore,
    Reload,
    NoCache,
    ForceCache,
    OnlyIfCached,
}

/// Request mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestMode {
    #[default]
    Cors,
    NoCors,
    SameOrigin,
    Navigate,
}

/// Request redirect mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestRedirect {
    #[default]
    Follow,
    Error,
    Manual,
}

/// Headers collection
#[derive(Debug, Clone, Default)]
pub struct Headers {
    entries: HashMap<String, String>,
}

impl Headers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create from iterator
    pub fn from_entries<I, K, V>(iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let entries = iter.into_iter()
            .map(|(k, v)| (k.into().to_lowercase(), v.into()))
            .collect();
        Self { entries }
    }

    /// Append a header value
    pub fn append(&mut self, name: &str, value: &str) {
        let key = name.to_lowercase();
        if let Some(existing) = self.entries.get_mut(&key) {
            existing.push_str(", ");
            existing.push_str(value);
        } else {
            self.entries.insert(key, value.to_string());
        }
    }

    /// Delete a header
    pub fn delete(&mut self, name: &str) {
        self.entries.remove(&name.to_lowercase());
    }

    /// Get a header value
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries.get(&name.to_lowercase()).map(|s| s.as_str())
    }

    /// Check if header exists
    pub fn has(&self, name: &str) -> bool {
        self.entries.contains_key(&name.to_lowercase())
    }

    /// Set a header value (replaces any existing)
    pub fn set(&mut self, name: &str, value: &str) {
        self.entries.insert(name.to_lowercase(), value.to_string());
    }

    /// Get all header names
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(|s| s.as_str())
    }

    /// Get all header values
    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.entries.values().map(|s| s.as_str())
    }

    /// Iterate over entries
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// Convert to HashMap
    pub fn to_hash_map(&self) -> HashMap<String, String> {
        self.entries.clone()
    }
}

/// Request body types
#[derive(Debug, Clone)]
pub enum RequestBody {
    None,
    Text(String),
    Bytes(Vec<u8>),
    FormData(HashMap<String, String>),
    Json(String),
}

/// Fetch Request
#[derive(Debug, Clone)]
pub struct Request {
    /// Request URL
    pub url: String,
    /// HTTP method
    pub method: HttpMethod,
    /// Request headers
    pub headers: Headers,
    /// Request body
    pub body: RequestBody,
    /// Credentials mode
    pub credentials: RequestCredentials,
    /// Cache mode
    pub cache: RequestCache,
    /// Request mode
    pub mode: RequestMode,
    /// Redirect mode
    pub redirect: RequestRedirect,
    /// Referrer
    pub referrer: String,
    /// Referrer policy
    pub referrer_policy: String,
    /// Integrity metadata
    pub integrity: String,
    /// Keep-alive
    pub keepalive: bool,
    /// Signal for abort controller
    pub signal: Option<u64>,
}

impl Request {
    /// Create a new GET request
    pub fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            method: HttpMethod::Get,
            headers: Headers::new(),
            body: RequestBody::None,
            credentials: RequestCredentials::SameOrigin,
            cache: RequestCache::Default,
            mode: RequestMode::Cors,
            redirect: RequestRedirect::Follow,
            referrer: "about:client".to_string(),
            referrer_policy: String::new(),
            integrity: String::new(),
            keepalive: false,
            signal: None,
        }
    }

    /// Create with options
    pub fn with_options(url: &str, method: HttpMethod, body: RequestBody) -> Self {
        Self {
            url: url.to_string(),
            method,
            headers: Headers::new(),
            body,
            credentials: RequestCredentials::SameOrigin,
            cache: RequestCache::Default,
            mode: RequestMode::Cors,
            redirect: RequestRedirect::Follow,
            referrer: "about:client".to_string(),
            referrer_policy: String::new(),
            integrity: String::new(),
            keepalive: false,
            signal: None,
        }
    }

    /// Clone the request (for reading body)
    pub fn clone_request(&self) -> Self {
        self.clone()
    }

    /// Get text body
    pub fn text(&self) -> Option<String> {
        match &self.body {
            RequestBody::Text(s) => Some(s.clone()),
            RequestBody::Json(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Get JSON body
    pub fn json(&self) -> Option<String> {
        match &self.body {
            RequestBody::Json(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// Get bytes body
    pub fn bytes(&self) -> Option<Vec<u8>> {
        match &self.body {
            RequestBody::Bytes(b) => Some(b.clone()),
            RequestBody::Text(s) => Some(s.as_bytes().to_vec()),
            RequestBody::Json(s) => Some(s.as_bytes().to_vec()),
            _ => None,
        }
    }
}

/// Response type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponseType {
    #[default]
    Basic,
    Cors,
    Error,
    Opaque,
    OpaqueRedirect,
}

/// Fetch Response
#[derive(Debug, Clone)]
pub struct Response {
    /// Response status code
    pub status: u16,
    /// Status text
    pub status_text: String,
    /// Response headers
    pub headers: Headers,
    /// Response body
    body: Vec<u8>,
    /// Response type
    pub response_type: ResponseType,
    /// Response URL
    pub url: String,
    /// Redirected flag
    pub redirected: bool,
    /// Body used flag
    body_used: bool,
}

impl Response {
    /// Create a new response
    pub fn new(status: u16, body: Vec<u8>) -> Self {
        Self {
            status,
            status_text: Self::status_text_for(status),
            headers: Headers::new(),
            body,
            response_type: ResponseType::Basic,
            url: String::new(),
            redirected: false,
            body_used: false,
        }
    }

    /// Create from text
    pub fn from_text(status: u16, text: &str) -> Self {
        let mut response = Self::new(status, text.as_bytes().to_vec());
        response.headers.set("content-type", "text/plain;charset=UTF-8");
        response
    }

    /// Create from JSON
    pub fn from_json(status: u16, json: &str) -> Self {
        let mut response = Self::new(status, json.as_bytes().to_vec());
        response.headers.set("content-type", "application/json;charset=UTF-8");
        response
    }

    /// Create error response
    pub fn error() -> Self {
        Self {
            status: 0,
            status_text: String::new(),
            headers: Headers::new(),
            body: Vec::new(),
            response_type: ResponseType::Error,
            url: String::new(),
            redirected: false,
            body_used: false,
        }
    }

    /// Create redirect response
    pub fn redirect(url: &str, status: u16) -> Self {
        let status = if status == 301 || status == 302 || status == 303 || status == 307 || status == 308 {
            status
        } else {
            302
        };
        let mut response = Self::new(status, Vec::new());
        response.headers.set("Location", url);
        response
    }

    /// Check if response is ok (200-299)
    pub fn ok(&self) -> bool {
        self.status >= 200 && self.status < 300
    }

    /// Get response as text
    pub fn text(&mut self) -> Option<String> {
        if self.body_used {
            return None;
        }
        self.body_used = true;
        String::from_utf8(self.body.clone()).ok()
    }

    /// Get response as JSON string
    pub fn json(&mut self) -> Option<String> {
        self.text()
    }

    /// Get response as bytes
    pub fn bytes(&mut self) -> Option<Vec<u8>> {
        if self.body_used {
            return None;
        }
        self.body_used = true;
        Some(self.body.clone())
    }

    /// Get response as array buffer
    pub fn array_buffer(&mut self) -> Option<Vec<u8>> {
        self.bytes()
    }

    /// Clone the response (body is not cloned if used)
    pub fn clone_response(&self) -> Result<Self, &'static str> {
        if self.body_used {
            return Err("Body already used");
        }
        Ok(self.clone())
    }

    fn status_text_for(status: u16) -> String {
        match status {
            100 => "Continue",
            101 => "Switching Protocols",
            200 => "OK",
            201 => "Created",
            202 => "Accepted",
            204 => "No Content",
            301 => "Moved Permanently",
            302 => "Found",
            303 => "See Other",
            304 => "Not Modified",
            307 => "Temporary Redirect",
            308 => "Permanent Redirect",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            405 => "Method Not Allowed",
            408 => "Request Timeout",
            409 => "Conflict",
            410 => "Gone",
            500 => "Internal Server Error",
            501 => "Not Implemented",
            502 => "Bad Gateway",
            503 => "Service Unavailable",
            504 => "Gateway Timeout",
            _ => "Unknown",
        }.to_string()
    }
}

/// Fetch function result
pub enum FetchResult {
    Pending,
    Success(Response),
    Error(String),
}

/// Fetch manager for handling outstanding requests
#[derive(Default)]
pub struct FetchManager {
    /// Outstanding requests
    pending: HashMap<u64, FetchResult>,
    /// Next request ID
    next_id: u64,
}

impl FetchManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a fetch request
    pub fn fetch(&mut self, request: Request) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        // In a real implementation, this would start an async HTTP request
        // For now, we mark it as pending
        self.pending.insert(id, FetchResult::Pending);

        // Simulate immediate response for testing
        // In production, this would be done asynchronously
        let response = Response::from_text(200, "OK");
        self.pending.insert(id, FetchResult::Success(response));

        id
    }

    /// Get the result of a fetch request
    pub fn get_result(&mut self, id: u64) -> Option<FetchResult> {
        self.pending.remove(&id)
    }

    /// Check if a request is still pending
    pub fn is_pending(&self, id: u64) -> bool {
        matches!(self.pending.get(&id), Some(FetchResult::Pending))
    }

    /// Abort a fetch request
    pub fn abort(&mut self, id: u64) {
        self.pending.remove(&id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_headers() {
        let mut headers = Headers::new();
        headers.set("Content-Type", "application/json");
        headers.set("Authorization", "Bearer token");

        assert_eq!(headers.get("content-type"), Some("application/json"));
        assert!(headers.has("authorization"));

        headers.append("Accept", "text/html");
        headers.append("Accept", "application/json");
        assert!(headers.get("accept").unwrap().contains(','));
    }

    #[test]
    fn test_request() {
        let request = Request::with_options(
            "https://api.example.com/data",
            HttpMethod::Post,
            RequestBody::Json(r#"{"key": "value"}"#.to_string()),
        );

        assert_eq!(request.method, HttpMethod::Post);
        assert!(request.json().is_some());
    }

    #[test]
    fn test_response() {
        let mut response = Response::from_json(200, r#"{"success": true}"#);
        
        assert!(response.ok());
        assert_eq!(response.status, 200);
        
        let text = response.text().unwrap();
        assert!(text.contains("success"));
        
        // Body should be used now
        assert!(response.text().is_none());
    }

    #[test]
    fn test_fetch_manager() {
        let mut manager = FetchManager::new();
        let request = Request::new("https://example.com");
        
        let id = manager.fetch(request);
        
        match manager.get_result(id) {
            Some(FetchResult::Success(response)) => {
                assert!(response.ok());
            }
            _ => panic!("Expected success"),
        }
    }
}
