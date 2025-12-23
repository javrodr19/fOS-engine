//! HTTP/3 and QUIC Integration
//!
//! Wraps fos-net HTTP/3 implementation for browser use.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Re-export core types from fos-net
pub use fos_net::http3::{
    QuicConnection, QuicState, QuicStream, StreamState,
    Http3Connection as NetHttp3Connection, Http3Request, Http3Response,
    ConnectionPool, PoolStats, QuicError,
};

/// HTTP/3 configuration for browser
#[derive(Debug, Clone)]
pub struct Http3Settings {
    pub max_concurrent_streams: u32,
    pub initial_window_size: u32,
    pub max_header_list_size: u32,
    pub enable_0rtt: bool,
    pub idle_timeout_ms: u64,
}

impl Default for Http3Settings {
    fn default() -> Self {
        Self {
            max_concurrent_streams: 100,
            initial_window_size: 65536,
            max_header_list_size: 16384,
            enable_0rtt: true,
            idle_timeout_ms: 30000,
        }
    }
}

/// Browser HTTP/3 manager wrapping fos-net connection pool
#[derive(Debug)]
pub struct Http3Manager {
    /// Connection pool from fos-net
    pool: ConnectionPool,
    /// Browser-specific settings
    settings: Http3Settings,
    /// Host capability cache (remembers which hosts support HTTP/3)
    http3_capable: HashMap<String, bool>,
}

impl Default for Http3Manager {
    fn default() -> Self {
        Self::new()
    }
}

impl Http3Manager {
    pub fn new() -> Self {
        Self {
            pool: ConnectionPool::new(6), // Max 6 connections per host
            settings: Http3Settings::default(),
            http3_capable: HashMap::new(),
        }
    }
    
    pub fn with_settings(settings: Http3Settings) -> Self {
        Self {
            pool: ConnectionPool::new(settings.max_concurrent_streams as usize),
            settings,
            http3_capable: HashMap::new(),
        }
    }
    
    /// Check if HTTP/3 is available for a host (from Alt-Svc or prior knowledge)
    pub fn supports_http3(&self, host: &str) -> bool {
        self.http3_capable.get(host).copied().unwrap_or(false)
    }
    
    /// Mark a host as HTTP/3 capable (from Alt-Svc header)
    pub fn mark_http3_capable(&mut self, host: &str, capable: bool) {
        self.http3_capable.insert(host.to_string(), capable);
    }
    
    /// Get or create HTTP/3 connection for a host
    pub fn get_connection(&mut self, host: &str) -> Result<&mut NetHttp3Connection, Http3Error> {
        // Check if connection exists
        if self.pool.get_connection(host).is_some() {
            return Ok(self.pool.get_connection(host).unwrap());
        }
        
        // Create new QUIC connection
        let mut quic = QuicConnection::new(
            self.pool.stats().total_connections as u64 + 1,
            &format!("{}:443", host),
        );
        
        // Connect
        quic.connect().map_err(|e| Http3Error::ConnectionFailed(e.to_string()))?;
        
        // Create HTTP/3 connection
        let mut h3 = NetHttp3Connection::new(quic);
        h3.init().map_err(|e| Http3Error::ConnectionFailed(e.to_string()))?;
        
        // Add to pool
        self.pool.add_connection(host, h3);
        self.http3_capable.insert(host.to_string(), true);
        
        Ok(self.pool.get_connection(host).unwrap())
    }
    
    /// Send HTTP/3 request
    pub fn request(
        &mut self,
        host: &str,
        method: &str,
        path: &str,
        headers: HashMap<String, String>,
        body: Option<Vec<u8>>,
    ) -> Result<u64, Http3Error> {
        let conn = self.get_connection(host)?;
        conn.request(method, path, headers, body)
            .map_err(|e| Http3Error::StreamError(e.to_string()))
    }
    
    /// Close idle connections older than specified duration
    pub fn close_idle(&mut self, _older_than_ms: u64) {
        // The pool doesn't track connection age, but we can close all
        // In a real implementation, we'd track last activity time
    }
    
    /// Close all connections
    pub fn close_all(&mut self) {
        self.pool.close_all();
    }
    
    /// Get statistics
    pub fn stats(&self) -> Http3Stats {
        let pool_stats = self.pool.stats();
        Http3Stats {
            active_connections: pool_stats.total_connections,
            active_streams: pool_stats.active_streams,
            http3_capable_hosts: self.http3_capable.len(),
        }
    }
}

/// HTTP/3 statistics
#[derive(Debug, Clone)]
pub struct Http3Stats {
    pub active_connections: usize,
    pub active_streams: usize,
    pub http3_capable_hosts: usize,
}

/// HTTP/3 errors (browser-level)
#[derive(Debug)]
pub enum Http3Error {
    ConnectionFailed(String),
    StreamError(String),
    Timeout,
    NotSupported,
}

impl std::fmt::Display for Http3Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionFailed(msg) => write!(f, "Connection failed: {}", msg),
            Self::StreamError(msg) => write!(f, "Stream error: {}", msg),
            Self::Timeout => write!(f, "Timeout"),
            Self::NotSupported => write!(f, "HTTP/3 not supported by host"),
        }
    }
}

impl std::error::Error for Http3Error {}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_http3_manager_creation() {
        let manager = Http3Manager::new();
        assert_eq!(manager.stats().active_connections, 0);
    }
    
    #[test]
    fn test_http3_capable_tracking() {
        let mut manager = Http3Manager::new();
        
        assert!(!manager.supports_http3("example.com"));
        
        manager.mark_http3_capable("example.com", true);
        assert!(manager.supports_http3("example.com"));
    }
}
