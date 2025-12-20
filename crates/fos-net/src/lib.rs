//! fOS Networking
//!
//! HTTP client, WebSocket, SSE, and resource loading.

pub mod loader;
pub mod fetch;
pub mod cache;
pub mod websocket;
pub mod sse;
pub mod beacon;
pub mod http2;
pub mod xhr;

pub use loader::{ResourceLoader, Request, Method};
pub use fetch::{fetch, fetch_with_options, FetchOptions, FetchResponse};
pub use url::Url;
pub use websocket::{WebSocket, WebSocketState, WebSocketError, MessageData};
pub use sse::{EventSource, EventSourceState, SseEvent};
pub use beacon::{send_beacon, BeaconData};
pub use http2::{Http2Connection, Http2Stream, Http2Settings, Http2Frame};
pub use xhr::{XmlHttpRequest, ReadyState, ResponseType, XhrError, FormData, FormDataValue};

/// HTTP Response
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Response {
    /// Get body as text
    pub fn text(&self) -> Option<String> {
        String::from_utf8(self.body.clone()).ok()
    }
    
    /// Check if response is successful
    pub fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
}

/// Network error
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("HTTP error: {status}")]
    HttpError { status: u16 },
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_response_is_success() {
        let resp = Response {
            status: 200,
            headers: vec![],
            body: vec![],
        };
        assert!(resp.is_success());
        
        let resp = Response {
            status: 404,
            headers: vec![],
            body: vec![],
        };
        assert!(!resp.is_success());
    }
}
