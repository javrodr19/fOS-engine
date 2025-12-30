//! Server-Sent Events (SSE)
//!
//! EventSource for server push.

use std::sync::{Arc, Mutex};

/// EventSource ready states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventSourceState {
    Connecting = 0,
    Open = 1,
    Closed = 2,
}

/// EventSource for Server-Sent Events
#[derive(Debug)]
pub struct EventSource {
    url: String,
    with_credentials: bool,
    state: Arc<Mutex<EventSourceState>>,
    
    // Event callbacks
    on_open: Option<u32>,
    on_message: Option<u32>,
    on_error: Option<u32>,
    event_handlers: Vec<(String, u32)>,
}

/// SSE message event
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event_type: String,
    pub data: String,
    pub origin: String,
    pub last_event_id: String,
}

impl EventSource {
    /// Create new EventSource
    pub fn new(url: &str, with_credentials: bool) -> Self {
        Self {
            url: url.to_string(),
            with_credentials,
            state: Arc::new(Mutex::new(EventSourceState::Connecting)),
            on_open: None,
            on_message: None,
            on_error: None,
            event_handlers: Vec::new(),
        }
    }
    
    /// Get URL
    pub fn url(&self) -> &str {
        &self.url
    }
    
    /// Get ready state
    pub fn ready_state(&self) -> EventSourceState {
        *self.state.lock().unwrap()
    }
    
    /// Check credentials mode
    pub fn with_credentials(&self) -> bool {
        self.with_credentials
    }
    
    /// Close the connection
    pub fn close(&mut self) {
        *self.state.lock().unwrap() = EventSourceState::Closed;
    }
    
    /// Set onopen handler
    pub fn set_on_open(&mut self, callback: u32) {
        self.on_open = Some(callback);
    }
    
    /// Set onmessage handler
    pub fn set_on_message(&mut self, callback: u32) {
        self.on_message = Some(callback);
    }
    
    /// Set onerror handler
    pub fn set_on_error(&mut self, callback: u32) {
        self.on_error = Some(callback);
    }
    
    /// Add event listener for custom event types
    pub fn add_event_listener(&mut self, event_type: &str, callback: u32) {
        self.event_handlers.push((event_type.to_string(), callback));
    }
    
    /// Remove event listener
    pub fn remove_event_listener(&mut self, event_type: &str, callback: u32) {
        self.event_handlers.retain(|(t, c)| t != event_type || *c != callback);
    }
    
    /// Simulate connection open
    pub fn simulate_open(&mut self) {
        *self.state.lock().unwrap() = EventSourceState::Open;
    }
    
    /// Simulate receiving an event
    pub fn simulate_event(&self, event: SseEvent) -> Vec<u32> {
        let mut callbacks = Vec::new();
        
        // Default message handler
        if event.event_type == "message" {
            if let Some(cb) = self.on_message {
                callbacks.push(cb);
            }
        }
        
        // Custom event handlers
        for (event_type, callback) in &self.event_handlers {
            if *event_type == event.event_type {
                callbacks.push(*callback);
            }
        }
        
        callbacks
    }
}

/// Parse SSE stream
pub fn parse_sse_line(line: &str, current_event: &mut SseEvent) -> Option<SseEvent> {
    let line = line.trim_end_matches('\r');
    
    if line.is_empty() {
        // Dispatch event
        if !current_event.data.is_empty() {
            let event = current_event.clone();
            *current_event = SseEvent::default();
            return Some(event);
        }
        return None;
    }
    
    if line.starts_with(':') {
        // Comment, ignore
        return None;
    }
    
    let (field, value) = if let Some(colon) = line.find(':') {
        let value = line[colon + 1..].trim_start_matches(' ');
        (&line[..colon], value)
    } else {
        (line, "")
    };
    
    match field {
        "event" => current_event.event_type = value.to_string(),
        "data" => {
            if !current_event.data.is_empty() {
                current_event.data.push('\n');
            }
            current_event.data.push_str(value);
        }
        "id" => current_event.last_event_id = value.to_string(),
        "retry" => { /* Would update reconnection time */ }
        _ => {}
    }
    
    None
}

impl Default for SseEvent {
    fn default() -> Self {
        Self {
            event_type: "message".to_string(),
            data: String::new(),
            origin: String::new(),
            last_event_id: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_event_source() {
        let es = EventSource::new("http://example.com/events", false);
        
        assert_eq!(es.url(), "http://example.com/events");
        assert_eq!(es.ready_state(), EventSourceState::Connecting);
    }
    
    #[test]
    fn test_parse_sse() {
        let mut event = SseEvent::default();
        
        parse_sse_line("event: update", &mut event);
        assert_eq!(event.event_type, "update");
        
        parse_sse_line("data: hello", &mut event);
        assert_eq!(event.data, "hello");
        
        let result = parse_sse_line("", &mut event);
        assert!(result.is_some());
    }
}
