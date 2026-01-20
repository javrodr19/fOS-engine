//! Network Panel
//!
//! Request logging and inspection.

use std::collections::HashMap;

/// Network request
#[derive(Debug, Clone)]
pub struct NetworkRequest {
    pub id: u64,
    pub url: String,
    pub method: String,
    pub request_headers: HashMap<String, String>,
    pub request_body: Option<Vec<u8>>,
    pub status: RequestStatus,
    pub timing: RequestTiming,
}

/// Request status
#[derive(Debug, Clone)]
pub enum RequestStatus {
    Pending,
    Complete { status_code: u16, status_text: String },
    Failed { error: String },
    Cancelled,
}

/// Request timing
#[derive(Debug, Clone, Default)]
pub struct RequestTiming {
    pub start_time: u64,
    pub dns_start: Option<u64>,
    pub dns_end: Option<u64>,
    pub connect_start: Option<u64>,
    pub connect_end: Option<u64>,
    pub ssl_start: Option<u64>,
    pub ssl_end: Option<u64>,
    pub send_start: Option<u64>,
    pub send_end: Option<u64>,
    pub receive_start: Option<u64>,
    pub receive_end: Option<u64>,
}

impl RequestTiming {
    pub fn total_time(&self) -> Option<u64> {
        self.receive_end.map(|end| end - self.start_time)
    }
}

/// Network response
#[derive(Debug, Clone)]
pub struct NetworkResponse {
    pub request_id: u64,
    pub status_code: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub content_type: Option<String>,
    pub content_length: Option<usize>,
}

/// Response preview type for DevTools display
#[derive(Debug, Clone)]
pub enum ResponsePreview {
    /// JSON response with parsed structure
    Json(String),
    /// HTML response with formatted display
    Html(String),
    /// Image with base64 data and dimensions
    Image { 
        base64: String, 
        width: u32, 
        height: u32, 
        format: String,
    },
    /// Plain text
    Text(String),
    /// Binary data (hex preview)
    Binary { 
        size: usize, 
        hex_preview: String,
    },
    /// Font file
    Font {
        family: String,
        format: String,
    },
    /// No preview available
    None,
}

impl ResponsePreview {
    /// Generate preview from response
    pub fn from_response(response: &NetworkResponse) -> Self {
        let content_type = response.content_type.as_deref().unwrap_or("");
        let body = match &response.body {
            Some(b) => b,
            None => return Self::None,
        };
        
        if content_type.contains("application/json") || content_type.contains("text/json") {
            if let Ok(text) = std::str::from_utf8(body) {
                return Self::Json(text.to_string());
            }
        }
        
        if content_type.contains("text/html") {
            if let Ok(text) = std::str::from_utf8(body) {
                return Self::Html(text.to_string());
            }
        }
        
        if content_type.starts_with("image/") {
            let format = content_type.strip_prefix("image/").unwrap_or("unknown").to_string();
            return Self::Image {
                base64: base64_encode(body),
                width: 0, // Would parse from image
                height: 0,
                format,
            };
        }
        
        if content_type.starts_with("text/") {
            if let Ok(text) = std::str::from_utf8(body) {
                return Self::Text(text.to_string());
            }
        }
        
        if content_type.contains("font") {
            return Self::Font {
                family: "unknown".to_string(),
                format: content_type.to_string(),
            };
        }
        
        // Binary fallback
        let hex_preview: String = body.iter()
            .take(64)
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");
        
        Self::Binary {
            size: body.len(),
            hex_preview,
        }
    }
}

/// Simple base64 encoding
fn base64_encode(data: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();
    
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        
        result.push(ALPHABET[b0 >> 2] as char);
        result.push(ALPHABET[((b0 & 0x03) << 4) | (b1 >> 4)] as char);
        
        if chunk.len() > 1 {
            result.push(ALPHABET[((b1 & 0x0f) << 2) | (b2 >> 6)] as char);
        } else {
            result.push('=');
        }
        
        if chunk.len() > 2 {
            result.push(ALPHABET[b2 & 0x3f] as char);
        } else {
            result.push('=');
        }
    }
    
    result
}

/// HTTP Cookie
#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<u64>,
    pub size: usize,
    pub http_only: bool,
    pub secure: bool,
    pub same_site: SameSite,
    pub priority: CookiePriority,
}

/// SameSite attribute
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SameSite {
    Strict,
    Lax,
    #[default]
    None,
}

/// Cookie priority
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CookiePriority {
    Low,
    #[default]
    Medium,
    High,
}

impl Cookie {
    /// Parse Set-Cookie header
    pub fn parse(header: &str, request_domain: &str) -> Option<Self> {
        let mut parts = header.split(';');
        let name_value = parts.next()?.trim();
        let (name, value) = name_value.split_once('=')?;
        
        let mut cookie = Cookie {
            name: name.trim().to_string(),
            value: value.trim().to_string(),
            domain: request_domain.to_string(),
            path: "/".to_string(),
            expires: None,
            size: name.len() + value.len(),
            http_only: false,
            secure: false,
            same_site: SameSite::None,
            priority: CookiePriority::Medium,
        };
        
        for attr in parts {
            let attr = attr.trim().to_lowercase();
            if attr == "httponly" {
                cookie.http_only = true;
            } else if attr == "secure" {
                cookie.secure = true;
            } else if attr.starts_with("samesite=") {
                cookie.same_site = match attr.strip_prefix("samesite=") {
                    Some("strict") => SameSite::Strict,
                    Some("lax") => SameSite::Lax,
                    _ => SameSite::None,
                };
            } else if attr.starts_with("domain=") {
                if let Some(d) = attr.strip_prefix("domain=") {
                    cookie.domain = d.to_string();
                }
            } else if attr.starts_with("path=") {
                if let Some(p) = attr.strip_prefix("path=") {
                    cookie.path = p.to_string();
                }
            }
        }
        
        Some(cookie)
    }
}

/// Network throttle configuration
#[derive(Debug, Clone, Copy)]
pub struct NetworkThrottle {
    /// Download speed in bytes per second
    pub download_bps: u64,
    /// Upload speed in bytes per second
    pub upload_bps: u64,
    /// Latency in milliseconds
    pub latency_ms: u32,
    /// Packet loss percentage (0-100)
    pub packet_loss: u8,
    /// Whether the connection is offline
    pub offline: bool,
}

impl NetworkThrottle {
    /// Slow 3G preset
    pub const SLOW_3G: Self = Self {
        download_bps: 50_000,
        upload_bps: 25_000,
        latency_ms: 400,
        packet_loss: 0,
        offline: false,
    };
    
    /// Fast 3G preset
    pub const FAST_3G: Self = Self {
        download_bps: 150_000,
        upload_bps: 75_000,
        latency_ms: 150,
        packet_loss: 0,
        offline: false,
    };
    
    /// Regular 4G preset
    pub const REGULAR_4G: Self = Self {
        download_bps: 4_000_000,
        upload_bps: 1_000_000,
        latency_ms: 50,
        packet_loss: 0,
        offline: false,
    };
    
    /// DSL preset
    pub const DSL: Self = Self {
        download_bps: 2_000_000,
        upload_bps: 500_000,
        latency_ms: 50,
        packet_loss: 0,
        offline: false,
    };
    
    /// Offline preset
    pub const OFFLINE: Self = Self {
        download_bps: 0,
        upload_bps: 0,
        latency_ms: 0,
        packet_loss: 100,
        offline: true,
    };
    
    /// No throttling
    pub const NONE: Self = Self {
        download_bps: u64::MAX,
        upload_bps: u64::MAX,
        latency_ms: 0,
        packet_loss: 0,
        offline: false,
    };
    
    /// Custom throttle
    pub fn custom(download_bps: u64, upload_bps: u64, latency_ms: u32) -> Self {
        Self {
            download_bps,
            upload_bps,
            latency_ms,
            packet_loss: 0,
            offline: false,
        }
    }
    
    /// Calculate simulated delay for a given byte count
    pub fn calculate_delay_ms(&self, bytes: usize, is_upload: bool) -> u64 {
        if self.offline {
            return u64::MAX;
        }
        
        let bps = if is_upload { self.upload_bps } else { self.download_bps };
        if bps == 0 || bps == u64::MAX {
            return self.latency_ms as u64;
        }
        
        let transfer_time_ms = (bytes as u64 * 1000) / bps;
        self.latency_ms as u64 + transfer_time_ms
    }
}

/// Network panel
#[derive(Debug)]
pub struct NetworkPanel {
    requests: Vec<NetworkRequest>,
    responses: HashMap<u64, NetworkResponse>,
    next_id: u64,
    recording: bool,
    preserve_log: bool,
    /// Current throttle configuration
    throttle: Option<NetworkThrottle>,
    /// Cookies extracted from responses
    cookies: Vec<Cookie>,
}

impl Default for NetworkPanel {
    fn default() -> Self {
        Self {
            requests: Vec::new(),
            responses: HashMap::new(),
            next_id: 0,
            recording: true,
            preserve_log: false,
            throttle: None,
            cookies: Vec::new(),
        }
    }
}

impl NetworkPanel {
    pub fn new() -> Self { 
        Self {
            recording: true,
            ..Default::default()
        }
    }
    
    /// Start recording
    pub fn start_recording(&mut self) {
        self.recording = true;
    }
    
    /// Stop recording
    pub fn stop_recording(&mut self) {
        self.recording = false;
    }
    
    /// Clear log
    pub fn clear(&mut self) {
        self.requests.clear();
        self.responses.clear();
    }
    
    /// Log request start
    pub fn log_request(&mut self, url: &str, method: &str, headers: HashMap<String, String>) -> u64 {
        if !self.recording {
            return 0;
        }
        
        let id = self.next_id;
        self.next_id += 1;
        
        let request = NetworkRequest {
            id,
            url: url.to_string(),
            method: method.to_string(),
            request_headers: headers,
            request_body: None,
            status: RequestStatus::Pending,
            timing: RequestTiming {
                start_time: current_time_ms(),
                ..Default::default()
            },
        };
        
        self.requests.push(request);
        id
    }
    
    /// Log response
    pub fn log_response(&mut self, request_id: u64, status_code: u16, status_text: &str, headers: HashMap<String, String>) {
        if let Some(req) = self.requests.iter_mut().find(|r| r.id == request_id) {
            req.status = RequestStatus::Complete {
                status_code,
                status_text: status_text.to_string(),
            };
            req.timing.receive_end = Some(current_time_ms());
        }
        
        let response = NetworkResponse {
            request_id,
            status_code,
            status_text: status_text.to_string(),
            headers: headers.clone(),
            body: None,
            content_type: headers.get("content-type").cloned(),
            content_length: headers.get("content-length").and_then(|s| s.parse().ok()),
        };
        
        self.responses.insert(request_id, response);
    }
    
    /// Log error
    pub fn log_error(&mut self, request_id: u64, error: &str) {
        if let Some(req) = self.requests.iter_mut().find(|r| r.id == request_id) {
            req.status = RequestStatus::Failed { error: error.to_string() };
            req.timing.receive_end = Some(current_time_ms());
        }
    }
    
    /// Get all requests
    pub fn get_requests(&self) -> &[NetworkRequest] {
        &self.requests
    }
    
    /// Get response
    pub fn get_response(&self, request_id: u64) -> Option<&NetworkResponse> {
        self.responses.get(&request_id)
    }
    
    /// Filter by type
    pub fn filter_by_type(&self, content_type: &str) -> Vec<&NetworkRequest> {
        self.requests.iter()
            .filter(|r| {
                self.responses.get(&r.id)
                    .and_then(|resp| resp.content_type.as_ref())
                    .map(|ct| ct.contains(content_type))
                    .unwrap_or(false)
            })
            .collect()
    }
    
    /// Get total size
    pub fn get_total_size(&self) -> usize {
        self.responses.values()
            .filter_map(|r| r.content_length)
            .sum()
    }
    
    // === Preview ===
    
    /// Get response preview for display in DevTools
    pub fn get_preview(&self, request_id: u64) -> ResponsePreview {
        match self.responses.get(&request_id) {
            Some(response) => ResponsePreview::from_response(response),
            None => ResponsePreview::None,
        }
    }
    
    /// Set response body (for complete body capture)
    pub fn set_response_body(&mut self, request_id: u64, body: Vec<u8>) {
        if let Some(response) = self.responses.get_mut(&request_id) {
            response.body = Some(body);
        }
    }
    
    // === Cookies ===
    
    /// Get all cookies
    pub fn get_cookies(&self) -> &[Cookie] {
        &self.cookies
    }
    
    /// Get cookies for a specific domain
    pub fn get_cookies_for_domain(&self, domain: &str) -> Vec<&Cookie> {
        self.cookies.iter()
            .filter(|c| domain.ends_with(&c.domain) || c.domain.ends_with(domain))
            .collect()
    }
    
    /// Extract cookies from Set-Cookie headers in a response
    pub fn extract_cookies(&mut self, request_id: u64, domain: &str) {
        if let Some(response) = self.responses.get(&request_id) {
            if let Some(set_cookie) = response.headers.get("set-cookie") {
                if let Some(cookie) = Cookie::parse(set_cookie, domain) {
                    // Remove existing cookie with same name/domain if present
                    self.cookies.retain(|c| !(c.name == cookie.name && c.domain == cookie.domain));
                    self.cookies.push(cookie);
                }
            }
        }
    }
    
    /// Clear all cookies
    pub fn clear_cookies(&mut self) {
        self.cookies.clear();
    }
    
    /// Delete a specific cookie
    pub fn delete_cookie(&mut self, name: &str, domain: &str) {
        self.cookies.retain(|c| !(c.name == name && c.domain == domain));
    }
    
    // === Throttling ===
    
    /// Set network throttle
    pub fn set_throttle(&mut self, throttle: NetworkThrottle) {
        self.throttle = Some(throttle);
    }
    
    /// Clear network throttle
    pub fn clear_throttle(&mut self) {
        self.throttle = None;
    }
    
    /// Get current throttle configuration
    pub fn get_throttle(&self) -> Option<&NetworkThrottle> {
        self.throttle.as_ref()
    }
    
    /// Check if network is offline
    pub fn is_offline(&self) -> bool {
        self.throttle.map(|t| t.offline).unwrap_or(false)
    }
    
    /// Calculate delay for a request/response
    pub fn calculate_delay(&self, bytes: usize, is_upload: bool) -> u64 {
        match &self.throttle {
            Some(t) => t.calculate_delay_ms(bytes, is_upload),
            None => 0,
        }
    }
    
    /// Set preserve log (keep requests on page navigation)
    pub fn set_preserve_log(&mut self, preserve: bool) {
        self.preserve_log = preserve;
    }
    
    /// Get preserve log setting
    pub fn preserve_log(&self) -> bool {
        self.preserve_log
    }
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_network_panel() {
        let mut panel = NetworkPanel::new();
        
        let id = panel.log_request("https://example.com", "GET", HashMap::new());
        panel.log_response(id, 200, "OK", HashMap::new());
        
        assert_eq!(panel.requests.len(), 1);
        assert!(panel.get_response(id).is_some());
    }
}
