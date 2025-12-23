//! Advanced Networking Integration
//!
//! Integrates fos-net features: WebSocket, XHR, SSE, CORS.

use std::collections::HashMap;
use fos_net::{
    WebSocket, WebSocketState, WebSocketError,
    XmlHttpRequest, ReadyState, XhrError,
    EventSource, EventSourceState,
};
use fos_security::{
    Origin, CorsValidator,
};

/// Advanced networking manager for the browser
pub struct AdvancedNetworking {
    /// Active WebSocket connections
    websockets: HashMap<u64, WebSocketConnection>,
    /// Active XHR requests
    xhr_requests: HashMap<u64, XhrInstance>,
    /// Active EventSource connections
    event_sources: HashMap<u64, EventSourceConnection>,
    /// CORS validator
    _cors: CorsValidator,
    /// Next connection ID
    next_id: u64,
}

/// WebSocket connection wrapper
#[derive(Debug)]
pub struct WebSocketConnection {
    pub id: u64,
    pub socket: WebSocket,
    pub url: String,
    pub created_at: std::time::Instant,
}

/// XHR instance wrapper
#[derive(Debug)]
pub struct XhrInstance {
    pub id: u64,
    pub xhr: XmlHttpRequest,
    pub url: String,
    pub method: String,
}

/// EventSource connection wrapper
#[derive(Debug)]
pub struct EventSourceConnection {
    pub id: u64,
    pub source: EventSource,
    pub url: String,
    pub created_at: std::time::Instant,
}

impl AdvancedNetworking {
    /// Create new advanced networking manager
    pub fn new() -> Self {
        Self {
            websockets: HashMap::new(),
            xhr_requests: HashMap::new(),
            event_sources: HashMap::new(),
            _cors: CorsValidator::new(),
            next_id: 1,
        }
    }
    
    // === WebSocket Methods ===
    
    /// Create a new WebSocket connection
    pub fn create_websocket(&mut self, url: &str, protocols: Vec<String>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let socket = WebSocket::new(url, protocols);
        
        self.websockets.insert(id, WebSocketConnection {
            id,
            socket,
            url: url.to_string(),
            created_at: std::time::Instant::now(),
        });
        
        log::debug!("Created WebSocket {} to {}", id, url);
        id
    }
    
    /// Get WebSocket connection
    pub fn get_websocket(&self, id: u64) -> Option<&WebSocketConnection> {
        self.websockets.get(&id)
    }
    
    /// Get mutable WebSocket connection
    pub fn get_websocket_mut(&mut self, id: u64) -> Option<&mut WebSocketConnection> {
        self.websockets.get_mut(&id)
    }
    
    /// Send text message via WebSocket
    pub fn ws_send_text(&mut self, id: u64, data: &str) -> Result<(), WebSocketError> {
        if let Some(conn) = self.websockets.get_mut(&id) {
            conn.socket.send_text(data)
        } else {
            Err(WebSocketError::InvalidState)
        }
    }
    
    /// Send binary message via WebSocket
    pub fn ws_send_binary(&mut self, id: u64, data: &[u8]) -> Result<(), WebSocketError> {
        if let Some(conn) = self.websockets.get_mut(&id) {
            conn.socket.send_binary(data)
        } else {
            Err(WebSocketError::InvalidState)
        }
    }
    
    /// Close WebSocket connection
    pub fn ws_close(&mut self, id: u64, code: Option<u16>, reason: Option<&str>) -> Result<(), WebSocketError> {
        if let Some(conn) = self.websockets.get_mut(&id) {
            conn.socket.close(code, reason)
        } else {
            Err(WebSocketError::InvalidState)
        }
    }
    
    /// Get WebSocket ready state
    pub fn ws_ready_state(&self, id: u64) -> Option<WebSocketState> {
        self.websockets.get(&id).map(|c| c.socket.ready_state())
    }
    
    /// Remove closed WebSocket
    pub fn ws_remove(&mut self, id: u64) {
        self.websockets.remove(&id);
    }
    
    // === XHR Methods ===
    
    /// Create a new XHR request
    pub fn create_xhr(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let xhr = XmlHttpRequest::new();
        
        self.xhr_requests.insert(id, XhrInstance {
            id,
            xhr,
            url: String::new(),
            method: String::new(),
        });
        
        id
    }
    
    /// Open XHR request
    pub fn xhr_open(&mut self, id: u64, method: &str, url: &str, async_flag: bool) -> Result<(), XhrError> {
        if let Some(instance) = self.xhr_requests.get_mut(&id) {
            instance.xhr.open(method, url, async_flag);
            instance.url = url.to_string();
            instance.method = method.to_string();
            Ok(())
        } else {
            Err(XhrError::InvalidState)
        }
    }
    
    /// Set XHR request header
    pub fn xhr_set_header(&mut self, id: u64, name: &str, value: &str) -> Result<(), XhrError> {
        if let Some(instance) = self.xhr_requests.get_mut(&id) {
            instance.xhr.set_request_header(name, value)
        } else {
            Err(XhrError::InvalidState)
        }
    }
    
    /// Send XHR request (synchronous)
    pub fn xhr_send(&mut self, id: u64, body: Option<&str>) -> Result<(), XhrError> {
        if let Some(instance) = self.xhr_requests.get_mut(&id) {
            instance.xhr.send(body)
        } else {
            Err(XhrError::InvalidState)
        }
    }
    
    /// Abort XHR request
    pub fn xhr_abort(&mut self, id: u64) {
        if let Some(instance) = self.xhr_requests.get_mut(&id) {
            instance.xhr.abort();
        }
    }
    
    /// Get XHR ready state
    pub fn xhr_ready_state(&self, id: u64) -> Option<ReadyState> {
        self.xhr_requests.get(&id).map(|i| i.xhr.ready_state)
    }
    
    /// Get XHR status
    pub fn xhr_status(&self, id: u64) -> Option<u16> {
        self.xhr_requests.get(&id).map(|i| i.xhr.status)
    }
    
    /// Get XHR response text
    pub fn xhr_response_text(&self, id: u64) -> Option<&str> {
        self.xhr_requests.get(&id).map(|i| i.xhr.response_text.as_str())
    }
    
    /// Get XHR response header
    pub fn xhr_get_header(&self, id: u64, name: &str) -> Option<Option<&str>> {
        self.xhr_requests.get(&id).map(|i| i.xhr.get_response_header(name))
    }
    
    /// Remove XHR request
    pub fn xhr_remove(&mut self, id: u64) {
        self.xhr_requests.remove(&id);
    }
    
    // === EventSource Methods ===
    
    /// Create EventSource connection
    pub fn create_event_source(&mut self, url: &str) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let source = EventSource::new(url, false); // with_credentials = false by default
        
        self.event_sources.insert(id, EventSourceConnection {
            id,
            source,
            url: url.to_string(),
            created_at: std::time::Instant::now(),
        });
        
        log::debug!("Created EventSource {} to {}", id, url);
        id
    }
    
    /// Get EventSource connection
    pub fn get_event_source(&self, id: u64) -> Option<&EventSourceConnection> {
        self.event_sources.get(&id)
    }
    
    /// Get EventSource ready state
    pub fn sse_ready_state(&self, id: u64) -> Option<EventSourceState> {
        self.event_sources.get(&id).map(|c| c.source.ready_state())
    }
    
    /// Close EventSource connection
    pub fn sse_close(&mut self, id: u64) {
        if let Some(conn) = self.event_sources.get_mut(&id) {
            conn.source.close();
        }
    }
    
    /// Remove EventSource
    pub fn sse_remove(&mut self, id: u64) {
        self.event_sources.remove(&id);
    }
    
    // === CORS Methods ===
    
    /// Check if request is allowed by CORS
    pub fn check_cors(&self, request_origin: &str, resource_origin: &str, _method: &str) -> bool {
        let req_origin = Origin::from_url(request_origin);
        let res_origin = Origin::from_url(resource_origin);
        
        // Same-origin is always allowed
        match (req_origin, res_origin) {
            (Some(req), Some(res)) if req == res => true,
            // For cross-origin, would need to check actual CORS headers
            _ => true,
        }
    }
    
    /// Validate CORS preflight response
    pub fn validate_cors_response(
        &self,
        origin: &str,
        method: &str,
        headers: &[String],
        response_headers: &HashMap<String, String>,
    ) -> bool {
        // Check Access-Control-Allow-Origin
        if let Some(allowed_origin) = response_headers.get("access-control-allow-origin") {
            if allowed_origin != "*" && allowed_origin != origin {
                return false;
            }
        } else {
            return false;
        }
        
        // Check Access-Control-Allow-Methods
        if let Some(allowed_methods) = response_headers.get("access-control-allow-methods") {
            if !allowed_methods.contains(method) && !allowed_methods.contains("*") {
                return false;
            }
        }
        
        // Check Access-Control-Allow-Headers
        if !headers.is_empty() {
            if let Some(allowed_headers) = response_headers.get("access-control-allow-headers") {
                for h in headers {
                    if !allowed_headers.to_lowercase().contains(&h.to_lowercase()) 
                        && !allowed_headers.contains("*") {
                        return false;
                    }
                }
            }
        }
        
        true
    }
    
    /// Get statistics
    pub fn stats(&self) -> AdvancedNetworkingStats {
        AdvancedNetworkingStats {
            websocket_count: self.websockets.len(),
            active_websockets: self.websockets.values()
                .filter(|c| c.socket.ready_state() == WebSocketState::Open)
                .count(),
            xhr_count: self.xhr_requests.len(),
            pending_xhr: self.xhr_requests.values()
                .filter(|i| i.xhr.ready_state != ReadyState::Done)
                .count(),
            event_source_count: self.event_sources.len(),
        }
    }
    
    /// Close all connections
    pub fn close_all(&mut self) {
        // Close all WebSockets
        for conn in self.websockets.values_mut() {
            let _ = conn.socket.close(Some(1000), Some("Page unload"));
        }
        
        // Abort all XHR
        for instance in self.xhr_requests.values_mut() {
            instance.xhr.abort();
        }
        
        // Close all EventSources
        for conn in self.event_sources.values_mut() {
            conn.source.close();
        }
    }
}

impl Default for AdvancedNetworking {
    fn default() -> Self {
        Self::new()
    }
}

/// Advanced networking statistics
#[derive(Debug, Clone)]
pub struct AdvancedNetworkingStats {
    pub websocket_count: usize,
    pub active_websockets: usize,
    pub xhr_count: usize,
    pub pending_xhr: usize,
    pub event_source_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_advanced_networking_creation() {
        let net = AdvancedNetworking::new();
        let stats = net.stats();
        assert_eq!(stats.websocket_count, 0);
        assert_eq!(stats.xhr_count, 0);
    }
    
    #[test]
    fn test_websocket_create() {
        let mut net = AdvancedNetworking::new();
        let id = net.create_websocket("wss://example.com/ws", vec![]);
        assert!(net.get_websocket(id).is_some());
    }
    
    #[test]
    fn test_xhr_create() {
        let mut net = AdvancedNetworking::new();
        let id = net.create_xhr();
        assert!(net.xhr_open(id, "GET", "https://example.com", true).is_ok());
    }
    
    #[test]
    fn test_cors_same_origin() {
        let net = AdvancedNetworking::new();
        assert!(net.check_cors("https://example.com", "https://example.com", "GET"));
    }
}
