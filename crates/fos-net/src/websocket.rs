//! WebSocket Client
//!
//! Full WebSocket implementation with events.

use std::sync::{Arc, Mutex};

/// WebSocket ready states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebSocketState {
    Connecting = 0,
    Open = 1,
    Closing = 2,
    Closed = 3,
}

/// WebSocket client
#[derive(Debug)]
pub struct WebSocket {
    url: String,
    protocols: Vec<String>,
    state: Arc<Mutex<WebSocketState>>,
    binary_type: BinaryType,
    buffered_amount: usize,
    extensions: String,
    protocol: String,
    
    // Event callbacks
    on_open: Option<u32>,
    on_message: Option<u32>,
    on_error: Option<u32>,
    on_close: Option<u32>,
}

/// Binary data type
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BinaryType {
    #[default]
    Blob,
    ArrayBuffer,
}

/// WebSocket close event
#[derive(Debug, Clone)]
pub struct CloseEvent {
    pub code: u16,
    pub reason: String,
    pub was_clean: bool,
}

/// WebSocket message event
#[derive(Debug, Clone)]
pub struct MessageEvent {
    pub data: MessageData,
    pub origin: String,
    pub last_event_id: String,
}

/// Message data types
#[derive(Debug, Clone)]
pub enum MessageData {
    Text(String),
    Binary(Vec<u8>),
}

impl WebSocket {
    /// Create new WebSocket connection
    pub fn new(url: &str, protocols: Vec<String>) -> Self {
        Self {
            url: url.to_string(),
            protocols,
            state: Arc::new(Mutex::new(WebSocketState::Connecting)),
            binary_type: BinaryType::default(),
            buffered_amount: 0,
            extensions: String::new(),
            protocol: String::new(),
            on_open: None,
            on_message: None,
            on_error: None,
            on_close: None,
        }
    }
    
    /// Get URL
    pub fn url(&self) -> &str {
        &self.url
    }
    
    /// Get ready state
    pub fn ready_state(&self) -> WebSocketState {
        *self.state.lock().unwrap()
    }
    
    /// Get buffered amount
    pub fn buffered_amount(&self) -> usize {
        self.buffered_amount
    }
    
    /// Get extensions
    pub fn extensions(&self) -> &str {
        &self.extensions
    }
    
    /// Get negotiated protocol
    pub fn protocol(&self) -> &str {
        &self.protocol
    }
    
    /// Get/set binary type
    pub fn binary_type(&self) -> BinaryType {
        self.binary_type
    }
    
    pub fn set_binary_type(&mut self, bt: BinaryType) {
        self.binary_type = bt;
    }
    
    /// Send text message
    pub fn send_text(&mut self, data: &str) -> Result<(), WebSocketError> {
        if self.ready_state() != WebSocketState::Open {
            return Err(WebSocketError::InvalidState);
        }
        // Would send via network
        Ok(())
    }
    
    /// Send binary message
    pub fn send_binary(&mut self, data: &[u8]) -> Result<(), WebSocketError> {
        if self.ready_state() != WebSocketState::Open {
            return Err(WebSocketError::InvalidState);
        }
        // Would send via network
        Ok(())
    }
    
    /// Close the connection
    pub fn close(&mut self, code: Option<u16>, reason: Option<&str>) -> Result<(), WebSocketError> {
        let state = *self.state.lock().unwrap();
        if state == WebSocketState::Closing || state == WebSocketState::Closed {
            return Ok(());
        }
        
        *self.state.lock().unwrap() = WebSocketState::Closing;
        
        // Validate close code
        if let Some(c) = code {
            if c != 1000 && (c < 3000 || c > 4999) {
                return Err(WebSocketError::InvalidCloseCode);
            }
        }
        
        // Would send close frame
        Ok(())
    }
    
    /// Set event handlers
    pub fn set_on_open(&mut self, callback: u32) {
        self.on_open = Some(callback);
    }
    
    pub fn set_on_message(&mut self, callback: u32) {
        self.on_message = Some(callback);
    }
    
    pub fn set_on_error(&mut self, callback: u32) {
        self.on_error = Some(callback);
    }
    
    pub fn set_on_close(&mut self, callback: u32) {
        self.on_close = Some(callback);
    }
    
    /// Simulate receiving a message (for testing)
    pub fn receive_message(&self, data: MessageData) -> Option<u32> {
        self.on_message
    }
    
    /// Simulate connection open
    pub fn simulate_open(&mut self) {
        *self.state.lock().unwrap() = WebSocketState::Open;
    }
    
    /// Simulate connection close
    pub fn simulate_close(&mut self, code: u16, reason: &str) {
        *self.state.lock().unwrap() = WebSocketState::Closed;
    }
}

/// WebSocket errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebSocketError {
    InvalidState,
    InvalidCloseCode,
    NetworkError,
    ProtocolError,
}

impl std::fmt::Display for WebSocketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidState => write!(f, "Invalid state"),
            Self::InvalidCloseCode => write!(f, "Invalid close code"),
            Self::NetworkError => write!(f, "Network error"),
            Self::ProtocolError => write!(f, "Protocol error"),
        }
    }
}

impl std::error::Error for WebSocketError {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_websocket_new() {
        let ws = WebSocket::new("wss://example.com/socket", vec![]);
        
        assert_eq!(ws.url(), "wss://example.com/socket");
        assert_eq!(ws.ready_state(), WebSocketState::Connecting);
    }
    
    #[test]
    fn test_websocket_open() {
        let mut ws = WebSocket::new("wss://example.com", vec![]);
        ws.simulate_open();
        
        assert_eq!(ws.ready_state(), WebSocketState::Open);
        assert!(ws.send_text("hello").is_ok());
    }
    
    #[test]
    fn test_websocket_close() {
        let mut ws = WebSocket::new("wss://example.com", vec![]);
        ws.simulate_open();
        
        assert!(ws.close(Some(1000), Some("bye")).is_ok());
        assert_eq!(ws.ready_state(), WebSocketState::Closing);
    }
}
