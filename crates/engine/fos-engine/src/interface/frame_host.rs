//! Frame Host Interface
//!
//! Remote interface for frame/page operations (Mojo-like).

use std::collections::VecDeque;

use crate::ipc::{IpcChannel, TypedMessage, MessageType, IpcSerialize};

/// Navigation result
#[derive(Debug, Clone)]
pub enum NavigationResult {
    /// Navigation succeeded
    Success {
        /// Final URL (after redirects)
        url: String,
        /// HTTP status code
        status: u16,
    },
    /// Navigation failed
    Failed {
        /// Error message
        error: String,
        /// Error code
        code: NavigationError,
    },
    /// Navigation was cancelled
    Cancelled,
}

/// Navigation error codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationError {
    /// Network error
    NetworkError,
    /// DNS resolution failed
    DnsError,
    /// SSL/TLS error
    SslError,
    /// Server returned error
    HttpError(u16),
    /// Request was blocked
    Blocked,
    /// Timeout
    Timeout,
    /// Unknown error
    Unknown,
}

/// JavaScript value (simplified)
#[derive(Debug, Clone)]
pub enum JsValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Object(String), // JSON representation
    Error(String),
}

impl JsValue {
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }
}

/// Load event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadEvent {
    /// Started loading
    Started,
    /// Committed (received first data)
    Committed,
    /// DOM content loaded
    DomContentLoaded,
    /// Fully loaded
    Loaded,
    /// Failed to load
    Failed,
}

/// Frame host trait - defines operations on a frame/page
pub trait FrameHost {
    /// Navigate to a URL
    fn navigate(&mut self, url: &str) -> NavigationResult;
    
    /// Execute JavaScript
    fn execute_script(&mut self, script: &str) -> JsValue;
    
    /// Get current URL
    fn url(&self) -> &str;
    
    /// Get page title
    fn title(&self) -> &str;
    
    /// Is loading
    fn is_loading(&self) -> bool;
    
    /// Stop loading
    fn stop(&mut self);
    
    /// Reload page
    fn reload(&mut self);
    
    /// Go back in history
    fn go_back(&mut self) -> bool;
    
    /// Go forward in history
    fn go_forward(&mut self) -> bool;
}

/// Frame host proxy - IPC wrapper for remote frame host
#[derive(Debug)]
pub struct FrameHostProxy {
    /// IPC channel to renderer
    channel: IpcChannel,
    /// Request ID counter
    next_request_id: u32,
    /// Pending responses
    pending: VecDeque<(u32, PendingRequest)>,
    /// Current URL
    url: String,
    /// Current title
    title: String,
    /// Loading state
    loading: bool,
}

#[derive(Debug)]
enum PendingRequest {
    Navigate,
    ExecuteScript,
    Reload,
    Stop,
}

impl FrameHostProxy {
    /// Create new proxy
    pub fn new(channel: IpcChannel) -> Self {
        Self {
            channel,
            next_request_id: 1,
            pending: VecDeque::new(),
            url: String::new(),
            title: String::new(),
            loading: false,
        }
    }
    
    /// Create proxy (not connected)
    pub fn disconnected() -> Self {
        Self {
            channel: IpcChannel::new(""),
            next_request_id: 1,
            pending: VecDeque::new(),
            url: String::new(),
            title: String::new(),
            loading: false,
        }
    }
    
    /// Set channel
    pub fn set_channel(&mut self, channel: IpcChannel) {
        self.channel = channel;
    }
    
    /// Is connected
    pub fn is_connected(&self) -> bool {
        self.channel.is_connected()
    }
    
    fn next_request_id(&mut self) -> u32 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        id
    }
    
    fn send_request(&mut self, msg_type: MessageType, payload: &[u8]) -> u32 {
        let request_id = self.next_request_id();
        let msg = TypedMessage::new(msg_type, request_id, payload.to_vec());
        
        let mut buf = Vec::new();
        msg.ipc_serialize(&mut buf);
        
        if self.channel.is_connected() {
            let _ = self.channel.send(&buf);
        }
        
        request_id
    }
}

impl FrameHost for FrameHostProxy {
    fn navigate(&mut self, url: &str) -> NavigationResult {
        self.loading = true;
        self.url = url.to_string();
        
        let request_id = self.send_request(MessageType::Navigate, url.as_bytes());
        self.pending.push_back((request_id, PendingRequest::Navigate));
        
        // In real implementation, this would be async
        NavigationResult::Success {
            url: url.to_string(),
            status: 200,
        }
    }
    
    fn execute_script(&mut self, script: &str) -> JsValue {
        let request_id = self.send_request(MessageType::ExecuteScript, script.as_bytes());
        self.pending.push_back((request_id, PendingRequest::ExecuteScript));
        
        // In real implementation, this would be async
        JsValue::Undefined
    }
    
    fn url(&self) -> &str {
        &self.url
    }
    
    fn title(&self) -> &str {
        &self.title
    }
    
    fn is_loading(&self) -> bool {
        self.loading
    }
    
    fn stop(&mut self) {
        self.loading = false;
        // Send stop message
    }
    
    fn reload(&mut self) {
        self.loading = true;
        let url = self.url.clone();
        self.navigate(&url);
    }
    
    fn go_back(&mut self) -> bool {
        // Would send history navigation message
        false
    }
    
    fn go_forward(&mut self) -> bool {
        // Would send history navigation message
        false
    }
}

/// In-process frame host (for single-process mode)
#[derive(Debug, Default)]
pub struct InProcessFrameHost {
    url: String,
    title: String,
    loading: bool,
    history: Vec<String>,
    history_index: usize,
}

impl InProcessFrameHost {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FrameHost for InProcessFrameHost {
    fn navigate(&mut self, url: &str) -> NavigationResult {
        // Truncate forward history
        if self.history_index < self.history.len() {
            self.history.truncate(self.history_index);
        }
        
        self.history.push(url.to_string());
        self.history_index = self.history.len();
        self.url = url.to_string();
        self.loading = true;
        
        NavigationResult::Success {
            url: url.to_string(),
            status: 200,
        }
    }
    
    fn execute_script(&mut self, _script: &str) -> JsValue {
        // Would execute in JS engine
        JsValue::Undefined
    }
    
    fn url(&self) -> &str {
        &self.url
    }
    
    fn title(&self) -> &str {
        &self.title
    }
    
    fn is_loading(&self) -> bool {
        self.loading
    }
    
    fn stop(&mut self) {
        self.loading = false;
    }
    
    fn reload(&mut self) {
        self.loading = true;
    }
    
    fn go_back(&mut self) -> bool {
        if self.history_index > 1 {
            self.history_index -= 1;
            self.url = self.history[self.history_index - 1].clone();
            self.loading = true;
            true
        } else {
            false
        }
    }
    
    fn go_forward(&mut self) -> bool {
        if self.history_index < self.history.len() {
            self.history_index += 1;
            self.url = self.history[self.history_index - 1].clone();
            self.loading = true;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_in_process_navigate() {
        let mut host = InProcessFrameHost::new();
        
        let result = host.navigate("https://example.com");
        assert!(matches!(result, NavigationResult::Success { .. }));
        assert_eq!(host.url(), "https://example.com");
        assert!(host.is_loading());
    }
    
    #[test]
    fn test_history_navigation() {
        let mut host = InProcessFrameHost::new();
        
        host.navigate("https://example.com");
        host.navigate("https://example.com/page1");
        host.navigate("https://example.com/page2");
        
        assert!(host.go_back());
        assert_eq!(host.url(), "https://example.com/page1");
        
        assert!(host.go_back());
        assert_eq!(host.url(), "https://example.com");
        
        assert!(host.go_forward());
        assert_eq!(host.url(), "https://example.com/page1");
    }
    
    #[test]
    fn test_js_value() {
        let val = JsValue::Error("test error".to_string());
        assert!(val.is_error());
        
        let val = JsValue::Number(42.0);
        assert!(!val.is_error());
    }
}
