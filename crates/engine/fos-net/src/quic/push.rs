//! HTTP/3 Server Push
//!
//! Server push implementation per RFC 9114 §4.6.

use std::collections::HashMap;

/// Push stream state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushState {
    /// Push promise received, waiting for push stream
    Promised,
    /// Push stream opened, receiving headers
    Headers,
    /// Receiving body
    Body,
    /// Push completed
    Complete,
    /// Push was cancelled
    Cancelled,
}

/// A server push
#[derive(Debug)]
pub struct ServerPush {
    /// Push ID (unique identifier)
    pub push_id: u64,
    /// Associated request stream ID
    pub request_stream_id: u64,
    /// Push stream ID (once opened)
    pub push_stream_id: Option<u64>,
    /// Current state
    pub state: PushState,
    /// Promised request headers
    pub promised_headers: Vec<(String, String)>,
    /// Response headers (once received)
    pub response_headers: Option<Vec<(String, String)>>,
    /// Response body chunks
    pub body_chunks: Vec<Vec<u8>>,
    /// Total bytes received
    pub bytes_received: u64,
}

impl ServerPush {
    /// Create a new server push from a PUSH_PROMISE
    pub fn new(push_id: u64, request_stream_id: u64, promised_headers: Vec<(String, String)>) -> Self {
        Self {
            push_id,
            request_stream_id,
            push_stream_id: None,
            state: PushState::Promised,
            promised_headers,
            response_headers: None,
            body_chunks: Vec::new(),
            bytes_received: 0,
        }
    }
    
    /// Get the pushed request URL
    pub fn url(&self) -> Option<String> {
        let mut scheme = None;
        let mut authority = None;
        let mut path = None;
        
        for (name, value) in &self.promised_headers {
            match name.as_str() {
                ":scheme" => scheme = Some(value.as_str()),
                ":authority" => authority = Some(value.as_str()),
                ":path" => path = Some(value.as_str()),
                _ => {}
            }
        }
        
        match (scheme, authority, path) {
            (Some(s), Some(a), Some(p)) => Some(format!("{}://{}{}", s, a, p)),
            _ => None,
        }
    }
    
    /// Associate with a push stream
    pub fn set_push_stream(&mut self, stream_id: u64) {
        self.push_stream_id = Some(stream_id);
        self.state = PushState::Headers;
    }
    
    /// Set response headers
    pub fn set_response_headers(&mut self, headers: Vec<(String, String)>) {
        self.response_headers = Some(headers);
        self.state = PushState::Body;
    }
    
    /// Add body chunk
    pub fn add_body_chunk(&mut self, data: Vec<u8>) {
        self.bytes_received += data.len() as u64;
        self.body_chunks.push(data);
    }
    
    /// Mark as complete
    pub fn complete(&mut self) {
        self.state = PushState::Complete;
    }
    
    /// Cancel this push
    pub fn cancel(&mut self) {
        self.state = PushState::Cancelled;
    }
    
    /// Get full body (if complete)
    pub fn body(&self) -> Option<Vec<u8>> {
        if self.state != PushState::Complete {
            return None;
        }
        
        let mut body = Vec::with_capacity(self.bytes_received as usize);
        for chunk in &self.body_chunks {
            body.extend_from_slice(chunk);
        }
        Some(body)
    }
    
    /// Check if push matches a request
    pub fn matches(&self, method: &str, scheme: &str, authority: &str, path: &str) -> bool {
        let mut h_method = None;
        let mut h_scheme = None;
        let mut h_authority = None;
        let mut h_path = None;
        
        for (name, value) in &self.promised_headers {
            match name.as_str() {
                ":method" => h_method = Some(value.as_str()),
                ":scheme" => h_scheme = Some(value.as_str()),
                ":authority" => h_authority = Some(value.as_str()),
                ":path" => h_path = Some(value.as_str()),
                _ => {}
            }
        }
        
        h_method == Some(method) && 
        h_scheme == Some(scheme) && 
        h_authority == Some(authority) && 
        h_path == Some(path)
    }
}

/// Push manager for handling server pushes
#[derive(Debug)]
pub struct PushManager {
    /// Active pushes by push ID
    pushes: HashMap<u64, ServerPush>,
    /// Push stream ID to push ID mapping
    stream_to_push: HashMap<u64, u64>,
    /// Maximum push ID we've announced
    max_push_id: u64,
    /// Whether push is enabled
    enabled: bool,
    /// Maximum concurrent pushes
    max_concurrent: usize,
}

impl PushManager {
    /// Create a new push manager
    pub fn new() -> Self {
        Self {
            pushes: HashMap::new(),
            stream_to_push: HashMap::new(),
            max_push_id: 0,
            enabled: true,
            max_concurrent: 100,
        }
    }
    
    /// Create push manager with push disabled
    pub fn disabled() -> Self {
        Self {
            pushes: HashMap::new(),
            stream_to_push: HashMap::new(),
            max_push_id: 0,
            enabled: false,
            max_concurrent: 0,
        }
    }
    
    /// Check if push is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    /// Enable or disable push
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }
    
    /// Get maximum push ID
    pub fn max_push_id(&self) -> u64 {
        self.max_push_id
    }
    
    /// Set maximum push ID
    pub fn set_max_push_id(&mut self, max_id: u64) {
        self.max_push_id = max_id;
    }
    
    /// Handle PUSH_PROMISE frame
    pub fn on_push_promise(
        &mut self,
        push_id: u64,
        request_stream_id: u64,
        headers: Vec<(String, String)>,
    ) -> Result<&ServerPush, PushError> {
        if !self.enabled {
            return Err(PushError::PushDisabled);
        }
        
        if push_id > self.max_push_id {
            return Err(PushError::PushIdExceeded);
        }
        
        if self.pushes.len() >= self.max_concurrent {
            return Err(PushError::TooManyPushes);
        }
        
        if self.pushes.contains_key(&push_id) {
            return Err(PushError::DuplicatePushId);
        }
        
        let push = ServerPush::new(push_id, request_stream_id, headers);
        self.pushes.insert(push_id, push);
        
        Ok(self.pushes.get(&push_id).unwrap())
    }
    
    /// Handle push stream opened
    pub fn on_push_stream(&mut self, push_stream_id: u64, push_id: u64) -> Result<(), PushError> {
        let push = self.pushes.get_mut(&push_id)
            .ok_or(PushError::UnknownPushId)?;
        
        push.set_push_stream(push_stream_id);
        self.stream_to_push.insert(push_stream_id, push_id);
        
        Ok(())
    }
    
    /// Get push by push ID
    pub fn get(&self, push_id: u64) -> Option<&ServerPush> {
        self.pushes.get(&push_id)
    }
    
    /// Get push by push ID mutably
    pub fn get_mut(&mut self, push_id: u64) -> Option<&mut ServerPush> {
        self.pushes.get_mut(&push_id)
    }
    
    /// Get push by stream ID
    pub fn get_by_stream(&self, stream_id: u64) -> Option<&ServerPush> {
        let push_id = self.stream_to_push.get(&stream_id)?;
        self.pushes.get(push_id)
    }
    
    /// Get push by stream ID mutably
    pub fn get_by_stream_mut(&mut self, stream_id: u64) -> Option<&mut ServerPush> {
        let push_id = *self.stream_to_push.get(&stream_id)?;
        self.pushes.get_mut(&push_id)
    }
    
    /// Find push that matches a request
    pub fn find_match(
        &self,
        method: &str,
        scheme: &str,
        authority: &str,
        path: &str,
    ) -> Option<&ServerPush> {
        self.pushes.values()
            .find(|p| p.state == PushState::Complete && p.matches(method, scheme, authority, path))
    }
    
    /// Cancel a push
    pub fn cancel(&mut self, push_id: u64) -> Option<()> {
        let push = self.pushes.get_mut(&push_id)?;
        push.cancel();
        Some(())
    }
    
    /// Remove completed or cancelled pushes
    pub fn cleanup(&mut self) {
        let to_remove: Vec<_> = self.pushes
            .iter()
            .filter(|(_, p)| matches!(p.state, PushState::Cancelled))
            .map(|(id, _)| *id)
            .collect();
        
        for id in to_remove {
            if let Some(push) = self.pushes.remove(&id) {
                if let Some(stream_id) = push.push_stream_id {
                    self.stream_to_push.remove(&stream_id);
                }
            }
        }
    }
    
    /// Get number of active pushes
    pub fn count(&self) -> usize {
        self.pushes.len()
    }
}

impl Default for PushManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Push error
#[derive(Debug, Clone, thiserror::Error)]
pub enum PushError {
    #[error("Push is disabled")]
    PushDisabled,
    
    #[error("Push ID exceeds MAX_PUSH_ID")]
    PushIdExceeded,
    
    #[error("Too many concurrent pushes")]
    TooManyPushes,
    
    #[error("Duplicate push ID")]
    DuplicatePushId,
    
    #[error("Unknown push ID")]
    UnknownPushId,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn test_headers() -> Vec<(String, String)> {
        vec![
            (":method".to_string(), "GET".to_string()),
            (":scheme".to_string(), "https".to_string()),
            (":authority".to_string(), "example.com".to_string()),
            (":path".to_string(), "/style.css".to_string()),
        ]
    }
    
    #[test]
    fn test_server_push_creation() {
        let push = ServerPush::new(0, 4, test_headers());
        assert_eq!(push.push_id, 0);
        assert_eq!(push.state, PushState::Promised);
        assert_eq!(push.url(), Some("https://example.com/style.css".to_string()));
    }
    
    #[test]
    fn test_push_lifecycle() {
        let mut push = ServerPush::new(0, 4, test_headers());
        
        push.set_push_stream(2);
        assert_eq!(push.state, PushState::Headers);
        
        push.set_response_headers(vec![
            (":status".to_string(), "200".to_string()),
        ]);
        assert_eq!(push.state, PushState::Body);
        
        push.add_body_chunk(vec![1, 2, 3]);
        push.add_body_chunk(vec![4, 5, 6]);
        assert_eq!(push.bytes_received, 6);
        
        push.complete();
        assert_eq!(push.body(), Some(vec![1, 2, 3, 4, 5, 6]));
    }
    
    #[test]
    fn test_push_matches() {
        let push = ServerPush::new(0, 4, test_headers());
        
        assert!(push.matches("GET", "https", "example.com", "/style.css"));
        assert!(!push.matches("POST", "https", "example.com", "/style.css"));
        assert!(!push.matches("GET", "https", "example.com", "/other.css"));
    }
    
    #[test]
    fn test_push_manager() {
        let mut mgr = PushManager::new();
        mgr.set_max_push_id(10);
        
        mgr.on_push_promise(0, 4, test_headers()).unwrap();
        
        assert!(mgr.get(0).is_some());
        assert_eq!(mgr.count(), 1);
    }
    
    #[test]
    fn test_push_disabled() {
        let mut mgr = PushManager::disabled();
        
        let result = mgr.on_push_promise(0, 4, test_headers());
        assert!(matches!(result, Err(PushError::PushDisabled)));
    }
    
    #[test]
    fn test_push_id_exceeded() {
        let mut mgr = PushManager::new();
        mgr.set_max_push_id(5);
        
        let result = mgr.on_push_promise(10, 4, test_headers());
        assert!(matches!(result, Err(PushError::PushIdExceeded)));
    }
}
