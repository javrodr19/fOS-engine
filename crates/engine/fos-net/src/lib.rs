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
pub mod http3;
pub mod network_opt;
pub mod connection_pool;
pub mod tcp;
pub mod tls;
pub mod http1;
pub mod cookies;
pub mod client;
pub mod coalescing;
pub mod prefetch;
pub mod cors;
pub mod streaming;
pub mod priority;
pub mod brotli_dict;
pub mod request_dedup;

pub use loader::{ResourceLoader, Request, Method};
pub use fetch::{fetch, fetch_with_options, FetchOptions, FetchResponse};
pub use websocket::{WebSocket, WebSocketState, WebSocketError, MessageData};
pub use sse::{EventSource, EventSourceState, SseEvent};
pub use beacon::{send_beacon, BeaconData};
pub use http2::{Http2Connection, Stream as Http2Stream, Settings as Http2Settings, Frame as Http2Frame, Http2Error, Http2Event, HpackEncoder, HpackDecoder};
pub use xhr::{XmlHttpRequest, ReadyState, ResponseType, XhrError, FormData, FormDataValue};
pub use http3::{QuicConnection, Http3Connection, QuicError};
pub use network_opt::{RequestCoalescer, PredictiveDns, DeltaSync, CrossTabCache};
pub use connection_pool::{ConnectionPool, PooledConnection, PoolConfig, HostKey, AcquireResult};
pub use client::{HttpClient, HttpClientBuilder, ClientConfig};
pub use cookies::{Cookie, CookieJar, SameSite, PartitionKey, PartitionedCookieJar};
pub use tcp::{TcpConnection, TcpConfig, BufferedTcpConnection};
pub use tls::{TlsStream, TlsConfig, TlsState};
pub use http1::{Http1Request, Http1Response, Http1Parser, HttpVersion};
pub use cors::{CorsHandler, CorsCheck, CorsMode, CredentialsMode, CorsError, PreflightRequest, PreflightResponse, Origin as CorsOrigin};
pub use streaming::{StreamingBody, StreamIterator, StreamState, TransferEncoding, ProgressBody, detect_encoding};
pub use priority::{RequestPriority, PriorityQueue, PrioritizedRequest, ResourceType, BandwidthHints, QueueStats};
pub use brotli_dict::{BrotliSharedDict, DictId, DictionaryBuilder, BrotliDecompressor, DictCache};
pub use request_dedup::{RequestDeduplicator, RequestKey, DeduplicatedResponse, DeduplicationStats, SimpleDeduplicator};


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
