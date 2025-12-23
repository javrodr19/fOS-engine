//! HTTP Client
//!
//! Main HTTP client integrating TCP, TLS, HTTP/1.1, HTTP/2, and cookies.
//! Replaces reqwest with a custom zero-dependency implementation.

use std::io::{self, BufReader, Read, Write};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::tcp::{TcpConnection, TcpConfig};
use crate::tls::{TlsStream, TlsConfig, TlsState};
use crate::http1::{Http1Request, Http1Response, Http1Parser, HttpVersion};
use crate::cookies::{CookieJar, Cookie};
use crate::connection_pool::{ConnectionPool, HostKey, PoolConfig, AcquireResult, ConnId};
use crate::{Response, NetError};

/// HTTP client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// User agent string
    pub user_agent: String,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Request timeout
    pub request_timeout: Duration,
    /// Max redirects to follow (0 = disable)
    pub max_redirects: u32,
    /// Enable cookies
    pub cookies_enabled: bool,
    /// Enable keep-alive
    pub keep_alive: bool,
    /// Default headers
    pub default_headers: Vec<(String, String)>,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            user_agent: "fOS-Engine/0.1".into(),
            connect_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(60),
            max_redirects: 10,
            cookies_enabled: true,
            keep_alive: true,
            default_headers: Vec::new(),
        }
    }
}

/// HTTP client builder
pub struct HttpClientBuilder {
    config: ClientConfig,
}

impl HttpClientBuilder {
    pub fn new() -> Self {
        Self {
            config: ClientConfig::default(),
        }
    }
    
    pub fn user_agent(mut self, ua: &str) -> Self {
        self.config.user_agent = ua.to_string();
        self
    }
    
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.config.connect_timeout = timeout;
        self
    }
    
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.config.request_timeout = timeout;
        self
    }
    
    pub fn max_redirects(mut self, max: u32) -> Self {
        self.config.max_redirects = max;
        self
    }
    
    pub fn cookie_store(mut self, enabled: bool) -> Self {
        self.config.cookies_enabled = enabled;
        self
    }
    
    pub fn default_header(mut self, name: &str, value: &str) -> Self {
        self.config.default_headers.push((name.to_string(), value.to_string()));
        self
    }
    
    pub fn build(self) -> HttpClient {
        HttpClient::with_config(self.config)
    }
}

impl Default for HttpClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// HTTP client
pub struct HttpClient {
    /// Configuration
    config: ClientConfig,
    /// Cookie jar
    cookies: CookieJar,
    /// Connection pool
    pool: ConnectionPool,
}

impl HttpClient {
    /// Create a new HTTP client with default settings
    pub fn new() -> Self {
        Self::builder().build()
    }
    
    /// Create a client builder
    pub fn builder() -> HttpClientBuilder {
        HttpClientBuilder::new()
    }
    
    /// Create with custom config
    pub fn with_config(config: ClientConfig) -> Self {
        let pool_config = PoolConfig {
            connect_timeout: config.connect_timeout,
            ..Default::default()
        };
        
        Self {
            config,
            cookies: CookieJar::new(),
            pool: ConnectionPool::new(pool_config),
        }
    }
    
    /// Make a GET request
    pub fn get(&mut self, url: &str) -> Result<Response, NetError> {
        self.request("GET", url, None, None)
    }
    
    /// Make a POST request
    pub fn post(&mut self, url: &str, body: Option<Vec<u8>>) -> Result<Response, NetError> {
        self.request("POST", url, None, body)
    }
    
    /// Make an HTTP request
    pub fn request(
        &mut self,
        method: &str,
        url: &str,
        headers: Option<Vec<(String, String)>>,
        body: Option<Vec<u8>>,
    ) -> Result<Response, NetError> {
        self.request_with_redirects(method, url, headers, body, 0)
    }
    
    fn request_with_redirects(
        &mut self,
        method: &str,
        url: &str,
        headers: Option<Vec<(String, String)>>,
        body: Option<Vec<u8>>,
        redirect_count: u32,
    ) -> Result<Response, NetError> {
        // Parse URL
        let parsed = UrlParts::parse(url)?;
        
        // Build request
        let mut req = Http1Request::new(method, &parsed.path_and_query());
        
        // Add Host header
        req = req.header("Host", &parsed.host_with_port());
        
        // Add User-Agent
        req = req.header("User-Agent", &self.config.user_agent);
        
        // Add default headers
        for (name, value) in &self.config.default_headers {
            req = req.header(name, value);
        }
        
        // Add custom headers
        if let Some(hdrs) = headers {
            for (name, value) in hdrs {
                req = req.header(&name, &value);
            }
        }
        
        // Add cookies
        if self.config.cookies_enabled {
            if let Some(cookie_header) = self.cookies.get_cookie_header(&parsed.host, &parsed.path, parsed.is_https) {
                req = req.header("Cookie", &cookie_header);
            }
        }
        
        // Add body
        if let Some(b) = body.clone() {
            req = req.body(b);
        }
        
        // Add Connection header
        if self.config.keep_alive {
            req = req.header("Connection", "keep-alive");
        } else {
            req = req.header("Connection", "close");
        }
        
        // Make the connection
        let response = self.execute_request(&parsed, req)?;
        
        // Store cookies from response
        if self.config.cookies_enabled {
            for (name, value) in &response.headers {
                if name.eq_ignore_ascii_case("set-cookie") {
                    self.cookies.add_from_header(value, &parsed.host);
                }
            }
        }
        
        // Handle redirects
        if response.status >= 300 && response.status < 400 && redirect_count < self.config.max_redirects {
            if let Some(location) = response.headers.iter()
                .find(|(n, _)| n.eq_ignore_ascii_case("location"))
                .map(|(_, v)| v.as_str())
            {
                let new_url = Self::resolve_redirect(url, location);
                
                // For 307/308, preserve method and body
                let (new_method, new_body) = if response.status == 307 || response.status == 308 {
                    (method, body)
                } else {
                    ("GET", None)
                };
                
                return self.request_with_redirects(new_method, &new_url, None, new_body, redirect_count + 1);
            }
        }
        
        Ok(response)
    }
    
    fn execute_request(&mut self, url: &UrlParts, req: Http1Request) -> Result<Response, NetError> {
        let port = url.port.unwrap_or(if url.is_https { 443 } else { 80 });
        let addr = format!("{}:{}", url.host, port);
        
        // Connect
        let tcp_config = TcpConfig {
            connect_timeout: self.config.connect_timeout,
            read_timeout: Some(self.config.request_timeout),
            write_timeout: Some(self.config.request_timeout),
            ..Default::default()
        };
        
        let mut stream = TcpConnection::connect_with_config(&addr, tcp_config)
            .map_err(|e| NetError::Network(format!("Connection failed: {}", e)))?;
        
        if url.is_https {
            // Upgrade to TLS
            let tls = TlsStream::connect(stream, &url.host, TlsConfig::default())
                .map_err(|e| NetError::Network(format!("TLS failed: {}", e)))?;
            
            self.send_and_receive_tls(tls, req)
        } else {
            self.send_and_receive_tcp(stream, req)
        }
    }
    
    fn send_and_receive_tcp(&self, mut stream: TcpConnection, req: Http1Request) -> Result<Response, NetError> {
        // Send request
        req.write_to(&mut stream)
            .map_err(|e| NetError::Network(format!("Write failed: {}", e)))?;
        
        // Read response
        let mut reader = BufReader::new(stream);
        let resp = Http1Parser::parse(&mut reader)
            .map_err(|e| NetError::Network(format!("Parse failed: {}", e)))?;
        
        Ok(Response {
            status: resp.status,
            headers: resp.headers,
            body: resp.body,
        })
    }
    
    fn send_and_receive_tls(&self, mut stream: TlsStream, req: Http1Request) -> Result<Response, NetError> {
        // Send request
        req.write_to(&mut stream)
            .map_err(|e| NetError::Network(format!("Write failed: {}", e)))?;
        
        // Read response
        let mut reader = BufReader::new(stream);
        let resp = Http1Parser::parse(&mut reader)
            .map_err(|e| NetError::Network(format!("Parse failed: {}", e)))?;
        
        Ok(Response {
            status: resp.status,
            headers: resp.headers,
            body: resp.body,
        })
    }
    
    fn resolve_redirect(base_url: &str, location: &str) -> String {
        if location.starts_with("http://") || location.starts_with("https://") {
            location.to_string()
        } else if location.starts_with('/') {
            // Absolute path
            if let Ok(parsed) = UrlParts::parse(base_url) {
                format!("{}://{}{}", 
                    if parsed.is_https { "https" } else { "http" },
                    parsed.host_with_port(),
                    location)
            } else {
                location.to_string()
            }
        } else {
            // Relative path
            if let Some(last_slash) = base_url.rfind('/') {
                format!("{}/{}", &base_url[..last_slash], location)
            } else {
                location.to_string()
            }
        }
    }
    
    /// Get cookie jar reference
    pub fn cookies(&self) -> &CookieJar {
        &self.cookies
    }
    
    /// Get mutable cookie jar
    pub fn cookies_mut(&mut self) -> &mut CookieJar {
        &mut self.cookies
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple URL parsing (for internal use)
#[derive(Debug)]
struct UrlParts {
    is_https: bool,
    host: String,
    port: Option<u16>,
    path: String,
    query: Option<String>,
}

impl UrlParts {
    fn parse(url: &str) -> Result<Self, NetError> {
        let is_https = url.starts_with("https://");
        let is_http = url.starts_with("http://");
        
        if !is_https && !is_http {
            return Err(NetError::InvalidUrl(format!("Invalid scheme: {}", url)));
        }
        
        let rest = if is_https { &url[8..] } else { &url[7..] };
        
        // Split at first /
        let (host_port, path_query) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        
        // Parse host:port
        let (host, port) = if let Some(colon) = host_port.rfind(':') {
            let h = &host_port[..colon];
            let p: u16 = host_port[colon + 1..].parse()
                .map_err(|_| NetError::InvalidUrl("Invalid port".into()))?;
            (h.to_string(), Some(p))
        } else {
            (host_port.to_string(), None)
        };
        
        // Parse path?query
        let (path, query) = match path_query.find('?') {
            Some(i) => (&path_query[..i], Some(path_query[i + 1..].to_string())),
            None => (path_query, None),
        };
        
        Ok(Self {
            is_https,
            host,
            port,
            path: path.to_string(),
            query,
        })
    }
    
    fn path_and_query(&self) -> String {
        match &self.query {
            Some(q) => format!("{}?{}", self.path, q),
            None => self.path.clone(),
        }
    }
    
    fn host_with_port(&self) -> String {
        match self.port {
            Some(p) => format!("{}:{}", self.host, p),
            None => self.host.clone(),
        }
    }
}

// Blocking API for sync contexts
pub mod blocking {
    use super::*;
    
    /// Blocking HTTP client (for sync code)
    pub struct Client {
        inner: HttpClient,
    }
    
    impl Client {
        pub fn new() -> Self {
            Self {
                inner: HttpClient::new(),
            }
        }
        
        pub fn builder() -> ClientBuilder {
            ClientBuilder::new()
        }
        
        pub fn get(&mut self, url: &str) -> Result<Response, NetError> {
            self.inner.get(url)
        }
        
        pub fn post(&mut self, url: &str, body: Option<Vec<u8>>) -> Result<Response, NetError> {
            self.inner.post(url, body)
        }
        
        pub fn request(
            &mut self,
            method: &str,
            url: &str,
            headers: Option<Vec<(String, String)>>,
            body: Option<Vec<u8>>,
        ) -> Result<Response, NetError> {
            self.inner.request(method, url, headers, body)
        }
    }
    
    impl Default for Client {
        fn default() -> Self {
            Self::new()
        }
    }
    
    pub struct ClientBuilder {
        inner: HttpClientBuilder,
    }
    
    impl ClientBuilder {
        pub fn new() -> Self {
            Self {
                inner: HttpClientBuilder::new(),
            }
        }
        
        pub fn user_agent(mut self, ua: &str) -> Self {
            self.inner = self.inner.user_agent(ua);
            self
        }
        
        pub fn timeout(mut self, timeout: Duration) -> Self {
            self.inner = self.inner.request_timeout(timeout);
            self
        }
        
        pub fn build(self) -> Result<Client, NetError> {
            Ok(Client {
                inner: self.inner.build(),
            })
        }
    }
    
    impl Default for ClientBuilder {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_url_parse() {
        let url = UrlParts::parse("https://example.com/path?query=1").unwrap();
        assert!(url.is_https);
        assert_eq!(url.host, "example.com");
        assert_eq!(url.path, "/path");
        assert_eq!(url.query, Some("query=1".to_string()));
    }
    
    #[test]
    fn test_url_with_port() {
        let url = UrlParts::parse("http://localhost:8080/api").unwrap();
        assert!(!url.is_https);
        assert_eq!(url.host, "localhost");
        assert_eq!(url.port, Some(8080));
    }
    
    #[test]
    fn test_client_builder() {
        let client = HttpClient::builder()
            .user_agent("TestAgent/1.0")
            .max_redirects(5)
            .build();
        
        assert_eq!(client.config.user_agent, "TestAgent/1.0");
        assert_eq!(client.config.max_redirects, 5);
    }
    
    #[test]
    fn test_redirect_resolution() {
        // Absolute URL
        assert_eq!(
            HttpClient::resolve_redirect("http://example.com/page", "https://other.com/new"),
            "https://other.com/new"
        );
        
        // Absolute path
        assert_eq!(
            HttpClient::resolve_redirect("http://example.com/old/path", "/new/path"),
            "http://example.com/new/path"
        );
    }
}
