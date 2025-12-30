//! Shared Worker API
//!
//! Workers shared across multiple browsing contexts.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Shared Worker
#[derive(Debug)]
pub struct SharedWorker {
    pub name: String,
    pub port: MessagePort,
}

/// Message Port for communication
#[derive(Debug, Clone)]
pub struct MessagePort {
    id: u32,
    started: bool,
    closed: bool,
    on_message: Option<u32>,
    on_message_error: Option<u32>,
}

/// Message Port transfer
#[derive(Debug, Clone)]
pub struct PortTransfer {
    pub ports: Vec<MessagePort>,
}

impl SharedWorker {
    /// Create new shared worker
    pub fn new(script_url: &str, name: Option<&str>) -> Self {
        Self {
            name: name.unwrap_or("").to_string(),
            port: MessagePort::new(),
        }
    }
    
    /// Get the message port
    pub fn port(&self) -> &MessagePort {
        &self.port
    }
}

impl MessagePort {
    /// Create new port
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
        Self {
            id: COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
            started: false,
            closed: false,
            on_message: None,
            on_message_error: None,
        }
    }
    
    /// Start receiving messages
    pub fn start(&mut self) {
        self.started = true;
    }
    
    /// Close the port
    pub fn close(&mut self) {
        self.closed = true;
    }
    
    /// Post a message
    pub fn post_message(&self, _message: &str, _transfer: Option<PortTransfer>) {
        if !self.closed && self.started {
            // Would send message
        }
    }
    
    /// Set message handler
    pub fn set_on_message(&mut self, callback: u32) {
        self.on_message = Some(callback);
    }
    
    /// Set error handler
    pub fn set_on_message_error(&mut self, callback: u32) {
        self.on_message_error = Some(callback);
    }
    
    /// Check if port is active
    pub fn is_active(&self) -> bool {
        self.started && !self.closed
    }
}

impl Default for MessagePort {
    fn default() -> Self {
        Self::new()
    }
}

/// Message Channel for creating port pairs
#[derive(Debug)]
pub struct MessageChannel {
    pub port1: MessagePort,
    pub port2: MessagePort,
}

impl MessageChannel {
    pub fn new() -> Self {
        Self {
            port1: MessagePort::new(),
            port2: MessagePort::new(),
        }
    }
}

impl Default for MessageChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_shared_worker() {
        let sw = SharedWorker::new("/worker.js", Some("myWorker"));
        assert_eq!(sw.name, "myWorker");
    }
    
    #[test]
    fn test_message_port() {
        let mut port = MessagePort::new();
        port.start();
        
        assert!(port.is_active());
        
        port.close();
        assert!(!port.is_active());
    }
    
    #[test]
    fn test_message_channel() {
        let channel = MessageChannel::new();
        assert_ne!(channel.port1.id, channel.port2.id);
    }
}
