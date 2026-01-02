//! XMLHttpRequest Implementation
//!
//! Legacy XHR API for HTTP requests.

use std::collections::HashMap;
use super::fetch::{Headers, HttpMethod};

/// XHR ready state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum XhrReadyState {
    #[default]
    Unsent = 0,
    Opened = 1,
    HeadersReceived = 2,
    Loading = 3,
    Done = 4,
}

/// XHR response type
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum XhrResponseType {
    #[default]
    Text,
    ArrayBuffer,
    Blob,
    Document,
    Json,
}

impl XhrResponseType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "arraybuffer" => XhrResponseType::ArrayBuffer,
            "blob" => XhrResponseType::Blob,
            "document" => XhrResponseType::Document,
            "json" => XhrResponseType::Json,
            _ => XhrResponseType::Text,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            XhrResponseType::Text => "",
            XhrResponseType::ArrayBuffer => "arraybuffer",
            XhrResponseType::Blob => "blob",
            XhrResponseType::Document => "document",
            XhrResponseType::Json => "json",
        }
    }
}

/// XMLHttpRequest
#[derive(Debug)]
pub struct XMLHttpRequest {
    /// ID for tracking
    pub id: u64,
    /// Ready state
    pub ready_state: XhrReadyState,
    /// HTTP method
    pub method: HttpMethod,
    /// Request URL
    pub url: String,
    /// Async flag
    pub async_: bool,
    /// Request headers
    request_headers: Headers,
    /// Response headers
    response_headers: Headers,
    /// Response status
    pub status: u16,
    /// Status text
    pub status_text: String,
    /// Response body (bytes)
    response_bytes: Vec<u8>,
    /// Response type
    pub response_type: XhrResponseType,
    /// Timeout in ms (0 = no timeout)
    pub timeout: u32,
    /// With credentials
    pub with_credentials: bool,
    /// Upload object
    pub upload: XhrUpload,
    /// Error flag
    error: bool,
    /// Aborted flag
    aborted: bool,
}

impl Default for XMLHttpRequest {
    fn default() -> Self {
        Self::new()
    }
}

impl XMLHttpRequest {
    /// Create a new XMLHttpRequest
    pub fn new() -> Self {
        static mut NEXT_ID: u64 = 1;
        let id = unsafe {
            let id = NEXT_ID;
            NEXT_ID += 1;
            id
        };

        Self {
            id,
            ready_state: XhrReadyState::Unsent,
            method: HttpMethod::Get,
            url: String::new(),
            async_: true,
            request_headers: Headers::new(),
            response_headers: Headers::new(),
            status: 0,
            status_text: String::new(),
            response_bytes: Vec::new(),
            response_type: XhrResponseType::Text,
            timeout: 0,
            with_credentials: false,
            upload: XhrUpload::new(),
            error: false,
            aborted: false,
        }
    }

    /// Open the request
    pub fn open(&mut self, method: &str, url: &str, async_: bool) {
        self.method = HttpMethod::from_str(method);
        self.url = url.to_string();
        self.async_ = async_;
        self.ready_state = XhrReadyState::Opened;
        self.status = 0;
        self.status_text = String::new();
        self.response_bytes.clear();
        self.error = false;
        self.aborted = false;
    }

    /// Set a request header
    pub fn set_request_header(&mut self, name: &str, value: &str) {
        if self.ready_state != XhrReadyState::Opened {
            return;
        }
        self.request_headers.append(name, value);
    }

    /// Get all response headers as string
    pub fn get_all_response_headers(&self) -> String {
        let mut result = String::new();
        for (name, value) in self.response_headers.entries() {
            result.push_str(name);
            result.push_str(": ");
            result.push_str(value);
            result.push_str("\r\n");
        }
        result
    }

    /// Get a specific response header
    pub fn get_response_header(&self, name: &str) -> Option<&str> {
        self.response_headers.get(name)
    }

    /// Override MIME type
    pub fn override_mime_type(&mut self, _mime: &str) {
        // Would override the MIME type for response handling
    }

    /// Send the request
    pub fn send(&mut self, body: Option<&str>) {
        if self.ready_state != XhrReadyState::Opened {
            return;
        }

        // In a real implementation, this would start the HTTP request
        // For now, simulate a response
        self.ready_state = XhrReadyState::HeadersReceived;
        self.ready_state = XhrReadyState::Loading;

        // Simulate successful response
        self.status = 200;
        self.status_text = "OK".to_string();
        self.response_headers.set("content-type", "text/plain");
        self.response_bytes = b"Response data".to_vec();
        
        self.ready_state = XhrReadyState::Done;
    }

    /// Abort the request
    pub fn abort(&mut self) {
        if self.ready_state == XhrReadyState::Opened || 
           self.ready_state == XhrReadyState::HeadersReceived ||
           self.ready_state == XhrReadyState::Loading {
            self.aborted = true;
            self.ready_state = XhrReadyState::Done;
        }
    }

    /// Get response as text
    pub fn response_text(&self) -> String {
        if self.ready_state != XhrReadyState::Done {
            return String::new();
        }
        String::from_utf8_lossy(&self.response_bytes).to_string()
    }

    /// Get response as XML (returns text for now)
    pub fn response_xml(&self) -> Option<String> {
        if self.ready_state != XhrReadyState::Done {
            return None;
        }
        Some(String::from_utf8_lossy(&self.response_bytes).to_string())
    }

    /// Get response based on responseType
    pub fn response(&self) -> XhrResponse {
        if self.ready_state != XhrReadyState::Done {
            return XhrResponse::Empty;
        }

        match self.response_type {
            XhrResponseType::Text => {
                XhrResponse::Text(String::from_utf8_lossy(&self.response_bytes).to_string())
            }
            XhrResponseType::ArrayBuffer => {
                XhrResponse::ArrayBuffer(self.response_bytes.clone())
            }
            XhrResponseType::Blob => {
                XhrResponse::Blob(self.response_bytes.clone())
            }
            XhrResponseType::Json => {
                let text = String::from_utf8_lossy(&self.response_bytes);
                XhrResponse::Json(text.to_string())
            }
            XhrResponseType::Document => {
                XhrResponse::Document(String::from_utf8_lossy(&self.response_bytes).to_string())
            }
        }
    }

    /// Set response (for testing/internal use)
    pub fn set_response(&mut self, status: u16, headers: Headers, body: Vec<u8>) {
        self.status = status;
        self.response_headers = headers;
        self.response_bytes = body;
        self.ready_state = XhrReadyState::Done;
    }
}

/// XHR response variants
#[derive(Debug, Clone)]
pub enum XhrResponse {
    Empty,
    Text(String),
    ArrayBuffer(Vec<u8>),
    Blob(Vec<u8>),
    Json(String),
    Document(String),
}

/// XHR Upload object
#[derive(Debug, Default)]
pub struct XhrUpload {
    // Event handlers would be stored here
}

impl XhrUpload {
    pub fn new() -> Self {
        Self::default()
    }
}

/// XHR progress event
#[derive(Debug, Clone)]
pub struct XhrProgressEvent {
    /// Is the total length computable?
    pub length_computable: bool,
    /// Bytes loaded
    pub loaded: u64,
    /// Total bytes
    pub total: u64,
}

impl XhrProgressEvent {
    pub fn new(loaded: u64, total: u64) -> Self {
        Self {
            length_computable: total > 0,
            loaded,
            total,
        }
    }
}

/// XHR manager for tracking multiple requests
#[derive(Default)]
pub struct XhrManager {
    requests: HashMap<u64, XMLHttpRequest>,
}

impl XhrManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new XMLHttpRequest and track it
    pub fn create(&mut self) -> u64 {
        let xhr = XMLHttpRequest::new();
        let id = xhr.id;
        self.requests.insert(id, xhr);
        id
    }

    /// Get a request by ID
    pub fn get(&self, id: u64) -> Option<&XMLHttpRequest> {
        self.requests.get(&id)
    }

    /// Get mutable request by ID
    pub fn get_mut(&mut self, id: u64) -> Option<&mut XMLHttpRequest> {
        self.requests.get_mut(&id)
    }

    /// Remove a completed request
    pub fn remove(&mut self, id: u64) {
        self.requests.remove(&id);
    }

    /// Get all done requests
    pub fn get_done(&self) -> Vec<u64> {
        self.requests
            .iter()
            .filter(|(_, xhr)| xhr.ready_state == XhrReadyState::Done)
            .map(|(id, _)| *id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xhr_lifecycle() {
        let mut xhr = XMLHttpRequest::new();
        assert_eq!(xhr.ready_state, XhrReadyState::Unsent);

        xhr.open("GET", "https://example.com/api", true);
        assert_eq!(xhr.ready_state, XhrReadyState::Opened);
        assert_eq!(xhr.method, HttpMethod::Get);

        xhr.set_request_header("Accept", "application/json");
        
        xhr.send(None);
        assert_eq!(xhr.ready_state, XhrReadyState::Done);
        assert_eq!(xhr.status, 200);
    }

    #[test]
    fn test_xhr_response_types() {
        let mut xhr = XMLHttpRequest::new();
        xhr.response_type = XhrResponseType::Json;
        
        xhr.open("POST", "https://api.example.com", true);
        xhr.send(Some(r#"{"data": "test"}"#));
        
        match xhr.response() {
            XhrResponse::Json(s) => assert!(!s.is_empty()),
            _ => panic!("Expected JSON response"),
        }
    }

    #[test]
    fn test_xhr_abort() {
        let mut xhr = XMLHttpRequest::new();
        xhr.open("GET", "https://example.com", true);
        xhr.abort();
        
        assert_eq!(xhr.ready_state, XhrReadyState::Done);
    }

    #[test]
    fn test_xhr_manager() {
        let mut manager = XhrManager::new();
        
        let id = manager.create();
        let xhr = manager.get_mut(id).unwrap();
        xhr.open("GET", "https://example.com", true);
        xhr.send(None);
        
        let done = manager.get_done();
        assert!(done.contains(&id));
    }
}
