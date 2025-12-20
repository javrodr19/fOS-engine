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

/// Network panel
#[derive(Debug, Default)]
pub struct NetworkPanel {
    requests: Vec<NetworkRequest>,
    responses: HashMap<u64, NetworkResponse>,
    next_id: u64,
    recording: bool,
    preserve_log: bool,
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
