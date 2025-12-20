//! XMLHttpRequest (Legacy API)
//!
//! Implementation of the XMLHttpRequest API for legacy compatibility.
//! Modern code should use fetch() instead.

use std::collections::HashMap;

/// XMLHttpRequest ready states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ReadyState {
    /// Client has been created, open() not called yet
    #[default]
    Unsent = 0,
    /// open() has been called
    Opened = 1,
    /// send() has been called, headers received
    HeadersReceived = 2,
    /// Downloading, responseText holds partial data
    Loading = 3,
    /// Operation complete
    Done = 4,
}

/// XMLHttpRequest object
#[derive(Debug, Default)]
pub struct XmlHttpRequest {
    pub ready_state: ReadyState,
    pub status: u16,
    pub status_text: String,
    pub response_text: String,
    pub response_headers: HashMap<String, String>,
    pub response_type: ResponseType,
    pub timeout: u32,
    pub with_credentials: bool,
    
    // Internal state
    method: String,
    url: String,
    async_flag: bool,
    request_headers: HashMap<String, String>,
    send_flag: bool,
    error_flag: bool,
}

/// Response types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResponseType {
    #[default]
    Text,
    ArrayBuffer,
    Blob,
    Document,
    Json,
}

impl XmlHttpRequest {
    /// Create a new XMLHttpRequest
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Open the request
    pub fn open(&mut self, method: &str, url: &str, r#async: bool) {
        self.method = method.to_uppercase();
        self.url = url.to_string();
        self.async_flag = r#async;
        self.ready_state = ReadyState::Opened;
        self.send_flag = false;
        self.request_headers.clear();
    }
    
    /// Set request header
    pub fn set_request_header(&mut self, name: &str, value: &str) -> Result<(), XhrError> {
        if self.ready_state != ReadyState::Opened {
            return Err(XhrError::InvalidState);
        }
        if self.send_flag {
            return Err(XhrError::InvalidState);
        }
        
        // Check for forbidden headers
        let lower = name.to_lowercase();
        if is_forbidden_header(&lower) {
            return Err(XhrError::ForbiddenHeader(name.to_string()));
        }
        
        // Combine with existing header if present
        if let Some(existing) = self.request_headers.get_mut(name) {
            existing.push_str(", ");
            existing.push_str(value);
        } else {
            self.request_headers.insert(name.to_string(), value.to_string());
        }
        
        Ok(())
    }
    
    /// Get response header
    pub fn get_response_header(&self, name: &str) -> Option<&str> {
        self.response_headers.get(name).map(|s| s.as_str())
    }
    
    /// Get all response headers as string
    pub fn get_all_response_headers(&self) -> String {
        let mut result = String::new();
        for (name, value) in &self.response_headers {
            result.push_str(name);
            result.push_str(": ");
            result.push_str(value);
            result.push_str("\r\n");
        }
        result
    }
    
    /// Override MIME type
    pub fn override_mime_type(&mut self, mime: &str) -> Result<(), XhrError> {
        if self.ready_state == ReadyState::Loading || self.ready_state == ReadyState::Done {
            return Err(XhrError::InvalidState);
        }
        // Store for later use
        self.request_headers.insert("_override_mime".to_string(), mime.to_string());
        Ok(())
    }
    
    /// Send the request (synchronous simulation for now)
    pub fn send(&mut self, body: Option<&str>) -> Result<(), XhrError> {
        if self.ready_state != ReadyState::Opened {
            return Err(XhrError::InvalidState);
        }
        if self.send_flag {
            return Err(XhrError::InvalidState);
        }
        
        self.send_flag = true;
        
        // In a real implementation, this would make the actual HTTP request
        // For now, we'll simulate the state transitions
        self.ready_state = ReadyState::HeadersReceived;
        self.ready_state = ReadyState::Loading;
        
        // TODO: Actually make the HTTP request using reqwest
        // For now, mark as done with success
        self.ready_state = ReadyState::Done;
        self.status = 200;
        self.status_text = "OK".to_string();
        
        Ok(())
    }
    
    /// Send request with fetch (actual implementation)
    pub async fn send_async(&mut self, body: Option<String>) -> Result<(), XhrError> {
        if self.ready_state != ReadyState::Opened {
            return Err(XhrError::InvalidState);
        }
        
        self.send_flag = true;
        
        // Build request
        let client = reqwest::Client::new();
        let mut request = match self.method.as_str() {
            "GET" => client.get(&self.url),
            "POST" => client.post(&self.url),
            "PUT" => client.put(&self.url),
            "DELETE" => client.delete(&self.url),
            "HEAD" => client.head(&self.url),
            "PATCH" => client.patch(&self.url),
            _ => return Err(XhrError::UnsupportedMethod(self.method.clone())),
        };
        
        // Add headers
        for (name, value) in &self.request_headers {
            if !name.starts_with('_') {
                request = request.header(name.as_str(), value.as_str());
            }
        }
        
        // Add body
        if let Some(b) = body {
            request = request.body(b);
        }
        
        // Set timeout
        if self.timeout > 0 {
            request = request.timeout(std::time::Duration::from_millis(self.timeout as u64));
        }
        
        self.ready_state = ReadyState::HeadersReceived;
        
        // Send request
        match request.send().await {
            Ok(response) => {
                self.status = response.status().as_u16();
                self.status_text = response.status().canonical_reason()
                    .unwrap_or("Unknown")
                    .to_string();
                
                // Copy headers
                for (name, value) in response.headers() {
                    if let Ok(v) = value.to_str() {
                        self.response_headers.insert(name.to_string(), v.to_string());
                    }
                }
                
                self.ready_state = ReadyState::Loading;
                
                // Get body
                match response.text().await {
                    Ok(text) => {
                        self.response_text = text;
                        self.ready_state = ReadyState::Done;
                        Ok(())
                    }
                    Err(e) => {
                        self.error_flag = true;
                        self.ready_state = ReadyState::Done;
                        Err(XhrError::Network(e.to_string()))
                    }
                }
            }
            Err(e) => {
                self.error_flag = true;
                self.ready_state = ReadyState::Done;
                
                if e.is_timeout() {
                    Err(XhrError::Timeout)
                } else {
                    Err(XhrError::Network(e.to_string()))
                }
            }
        }
    }
    
    /// Abort the request
    pub fn abort(&mut self) {
        // Reset state
        if self.ready_state == ReadyState::Unsent 
            || self.ready_state == ReadyState::Opened && !self.send_flag
            || self.ready_state == ReadyState::Done 
        {
            return;
        }
        
        self.ready_state = ReadyState::Done;
        self.send_flag = false;
        self.error_flag = true;
        
        // Clear response
        self.status = 0;
        self.status_text.clear();
        self.response_text.clear();
        self.response_headers.clear();
    }
    
    /// Get response as JSON
    pub fn response_json<T: serde::de::DeserializeOwned>(&self) -> Result<T, XhrError> {
        serde_json::from_str(&self.response_text)
            .map_err(|e| XhrError::ParseError(e.to_string()))
    }
}

/// Check if header is forbidden
fn is_forbidden_header(name: &str) -> bool {
    matches!(name, 
        "accept-charset" | "accept-encoding" | "access-control-request-headers" |
        "access-control-request-method" | "connection" | "content-length" |
        "cookie" | "cookie2" | "date" | "dnt" | "expect" | "host" |
        "keep-alive" | "origin" | "referer" | "te" | "trailer" |
        "transfer-encoding" | "upgrade" | "via"
    ) || name.starts_with("proxy-") || name.starts_with("sec-")
}

/// XHR errors
#[derive(Debug, thiserror::Error)]
pub enum XhrError {
    #[error("Invalid state")]
    InvalidState,
    
    #[error("Forbidden header: {0}")]
    ForbiddenHeader(String),
    
    #[error("Unsupported method: {0}")]
    UnsupportedMethod(String),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Request timeout")]
    Timeout,
    
    #[error("Parse error: {0}")]
    ParseError(String),
}

/// FormData for XHR
#[derive(Debug, Default)]
pub struct FormData {
    entries: Vec<(String, FormDataValue)>,
}

/// FormData value
#[derive(Debug, Clone)]
pub enum FormDataValue {
    Text(String),
    File { name: String, content: Vec<u8>, content_type: String },
}

impl FormData {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn append(&mut self, name: &str, value: &str) {
        self.entries.push((name.to_string(), FormDataValue::Text(value.to_string())));
    }
    
    pub fn append_file(&mut self, name: &str, filename: &str, content: Vec<u8>, content_type: &str) {
        self.entries.push((name.to_string(), FormDataValue::File {
            name: filename.to_string(),
            content,
            content_type: content_type.to_string(),
        }));
    }
    
    pub fn delete(&mut self, name: &str) {
        self.entries.retain(|(n, _)| n != name);
    }
    
    pub fn get(&self, name: &str) -> Option<&FormDataValue> {
        self.entries.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }
    
    pub fn get_all(&self, name: &str) -> Vec<&FormDataValue> {
        self.entries.iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| v)
            .collect()
    }
    
    pub fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|(n, _)| n == name)
    }
    
    pub fn set(&mut self, name: &str, value: &str) {
        self.delete(name);
        self.append(name, value);
    }
    
    /// Convert to multipart form data body
    pub fn to_multipart_body(&self) -> (String, Vec<u8>) {
        let boundary = format!("----FormBoundary{:x}", rand_boundary());
        let mut body = Vec::new();
        
        for (name, value) in &self.entries {
            body.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
            
            match value {
                FormDataValue::Text(text) => {
                    body.extend_from_slice(
                        format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes()
                    );
                    body.extend_from_slice(text.as_bytes());
                }
                FormDataValue::File { name: filename, content, content_type } => {
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n\
                             Content-Type: {}\r\n\r\n",
                            name, filename, content_type
                        ).as_bytes()
                    );
                    body.extend_from_slice(content);
                }
            }
            body.extend_from_slice(b"\r\n");
        }
        
        body.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
        
        let content_type = format!("multipart/form-data; boundary={}", boundary);
        (content_type, body)
    }
}

fn rand_boundary() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(12345)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_xhr_new() {
        let xhr = XmlHttpRequest::new();
        assert_eq!(xhr.ready_state, ReadyState::Unsent);
    }
    
    #[test]
    fn test_xhr_open() {
        let mut xhr = XmlHttpRequest::new();
        xhr.open("GET", "https://example.com", true);
        assert_eq!(xhr.ready_state, ReadyState::Opened);
    }
    
    #[test]
    fn test_xhr_set_header() {
        let mut xhr = XmlHttpRequest::new();
        xhr.open("GET", "https://example.com", true);
        assert!(xhr.set_request_header("X-Custom", "value").is_ok());
    }
    
    #[test]
    fn test_xhr_forbidden_header() {
        let mut xhr = XmlHttpRequest::new();
        xhr.open("GET", "https://example.com", true);
        assert!(xhr.set_request_header("Cookie", "value").is_err());
    }
    
    #[test]
    fn test_form_data() {
        let mut form = FormData::new();
        form.append("name", "value");
        assert!(form.has("name"));
        assert_eq!(
            matches!(form.get("name"), Some(FormDataValue::Text(s)) if s == "value"),
            true
        );
    }
}
