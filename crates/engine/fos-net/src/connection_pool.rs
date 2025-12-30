//! Connection Pool Prewarming (Phase 24.7)
//!
//! Warm TLS connections during idle. Instant HTTPS.
//! Connection pooling with keep-alive. Protocol multiplexing.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Connection ID
pub type ConnId = u32;

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnState {
    /// Being established
    Connecting,
    /// Ready for use
    Ready,
    /// In use
    InUse,
    /// Idle (can be reused)
    Idle,
    /// Closed
    Closed,
    /// Error state
    Error,
}

/// Protocol version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Http1,
    Http2,
    Http3,
}

/// TLS state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsState {
    /// No TLS
    None,
    /// Handshaking
    Handshaking,
    /// Session resumption
    Resuming,
    /// Established
    Established,
}

/// Pooled connection
#[derive(Debug)]
pub struct PooledConnection {
    /// Connection ID
    pub id: ConnId,
    /// Target host
    pub host: Box<str>,
    /// Port
    pub port: u16,
    /// Current state
    pub state: ConnState,
    /// Protocol
    pub protocol: Protocol,
    /// TLS state
    pub tls: TlsState,
    /// Created at
    pub created_at: Instant,
    /// Last used at
    pub last_used: Instant,
    /// Request count on this connection
    pub request_count: u32,
    /// Is pre-warmed
    pub is_prewarmed: bool,
}

impl PooledConnection {
    pub fn new(id: ConnId, host: &str, port: u16) -> Self {
        let now = Instant::now();
        Self {
            id,
            host: host.into(),
            port,
            state: ConnState::Connecting,
            protocol: Protocol::Http1,
            tls: TlsState::None,
            created_at: now,
            last_used: now,
            request_count: 0,
            is_prewarmed: false,
        }
    }
    
    /// Check if connection is usable
    pub fn is_usable(&self) -> bool {
        matches!(self.state, ConnState::Ready | ConnState::Idle)
    }
    
    /// Check if connection is stale
    pub fn is_stale(&self, max_idle: Duration) -> bool {
        self.state == ConnState::Idle && self.last_used.elapsed() > max_idle
    }
    
    /// Age of the connection
    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
    
    /// Mark as in use
    pub fn acquire(&mut self) {
        self.state = ConnState::InUse;
        self.last_used = Instant::now();
        self.request_count += 1;
    }
    
    /// Release back to pool
    pub fn release(&mut self) {
        self.state = ConnState::Idle;
        self.last_used = Instant::now();
    }
}

/// Pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Max connections per host
    pub max_per_host: usize,
    /// Max total connections
    pub max_total: usize,
    /// Idle timeout
    pub idle_timeout: Duration,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Enable prewarming
    pub prewarm_enabled: bool,
    /// Max prewarmed connections per host
    pub max_prewarmed: usize,
    /// Max connection age
    pub max_age: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            max_per_host: 6,
            max_total: 100,
            idle_timeout: Duration::from_secs(60),
            connect_timeout: Duration::from_secs(30),
            prewarm_enabled: true,
            max_prewarmed: 2,
            max_age: Duration::from_secs(300),
        }
    }
}

/// Host key for connection pooling
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HostKey {
    pub host: Box<str>,
    pub port: u16,
    pub is_tls: bool,
}

impl HostKey {
    pub fn new(host: &str, port: u16, is_tls: bool) -> Self {
        Self {
            host: host.into(),
            port,
            is_tls,
        }
    }
    
    pub fn from_url(url: &str) -> Option<Self> {
        // Simple URL parsing
        let is_tls = url.starts_with("https://");
        let default_port = if is_tls { 443 } else { 80 };
        
        let host_start = if is_tls { 8 } else { 7 };
        let rest = url.get(host_start..)?;
        
        let (host, port) = if let Some(colon) = rest.find(':') {
            let h = &rest[..colon];
            let p_end = rest.find('/').unwrap_or(rest.len());
            let p: u16 = rest[colon+1..p_end].parse().ok()?;
            (h, p)
        } else {
            let h_end = rest.find('/').unwrap_or(rest.len());
            (&rest[..h_end], default_port)
        };
        
        Some(Self::new(host, port, is_tls))
    }
}

/// Connection pool statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolStats {
    pub connections_created: u64,
    pub connections_reused: u64,
    pub connections_closed: u64,
    pub prewarmed_hits: u64,
    pub prewarmed_misses: u64,
    pub wait_time_total_ms: u64,
    pub tls_resumptions: u64,
}

impl PoolStats {
    pub fn reuse_rate(&self) -> f64 {
        let total = self.connections_created + self.connections_reused;
        if total == 0 {
            0.0
        } else {
            self.connections_reused as f64 / total as f64
        }
    }
    
    pub fn prewarm_hit_rate(&self) -> f64 {
        let total = self.prewarmed_hits + self.prewarmed_misses;
        if total == 0 {
            0.0
        } else {
            self.prewarmed_hits as f64 / total as f64
        }
    }
}

/// Connection pool
#[derive(Debug)]
pub struct ConnectionPool {
    /// All connections
    connections: HashMap<ConnId, PooledConnection>,
    /// Connections by host
    by_host: HashMap<HostKey, Vec<ConnId>>,
    /// Configuration
    config: PoolConfig,
    /// Statistics
    stats: PoolStats,
    /// Next connection ID
    next_id: ConnId,
    /// Hosts to prewarm
    prewarm_queue: Vec<HostKey>,
}

impl Default for ConnectionPool {
    fn default() -> Self {
        Self::new(PoolConfig::default())
    }
}

impl ConnectionPool {
    pub fn new(config: PoolConfig) -> Self {
        Self {
            connections: HashMap::new(),
            by_host: HashMap::new(),
            config,
            stats: PoolStats::default(),
            next_id: 0,
            prewarm_queue: Vec::new(),
        }
    }
    
    /// Get or create a connection for a host
    pub fn acquire(&mut self, host: &HostKey) -> AcquireResult {
        // Try to find an idle connection
        if let Some(conn_ids) = self.by_host.get(host) {
            for &id in conn_ids {
                if let Some(conn) = self.connections.get_mut(&id) {
                    if conn.is_usable() {
                        conn.acquire();
                        self.stats.connections_reused += 1;
                        
                        if conn.is_prewarmed {
                            self.stats.prewarmed_hits += 1;
                        }
                        
                        return AcquireResult::Reused(id);
                    }
                }
            }
        }
        
        self.stats.prewarmed_misses += 1;
        
        // Check limits
        let host_count = self.by_host.get(host).map(|v| v.len()).unwrap_or(0);
        if host_count >= self.config.max_per_host {
            return AcquireResult::WaitForConnection;
        }
        
        if self.connections.len() >= self.config.max_total {
            // Try to evict stale connections
            self.evict_stale();
            
            if self.connections.len() >= self.config.max_total {
                return AcquireResult::PoolExhausted;
            }
        }
        
        // Create new connection
        let id = self.create_connection(host);
        AcquireResult::Created(id)
    }
    
    /// Create a new connection
    fn create_connection(&mut self, host: &HostKey) -> ConnId {
        let id = self.next_id;
        self.next_id += 1;
        
        let mut conn = PooledConnection::new(id, &host.host, host.port);
        conn.tls = if host.is_tls { TlsState::Handshaking } else { TlsState::None };
        conn.acquire();
        
        self.connections.insert(id, conn);
        self.by_host.entry(host.clone()).or_default().push(id);
        self.stats.connections_created += 1;
        
        id
    }
    
    /// Release a connection back to the pool
    pub fn release(&mut self, id: ConnId) {
        if let Some(conn) = self.connections.get_mut(&id) {
            conn.release();
        }
    }
    
    /// Close a connection
    pub fn close(&mut self, id: ConnId) {
        if let Some(mut conn) = self.connections.remove(&id) {
            conn.state = ConnState::Closed;
            self.stats.connections_closed += 1;
            
            // Remove from host index
            let key = HostKey::new(&conn.host, conn.port, conn.tls != TlsState::None);
            if let Some(ids) = self.by_host.get_mut(&key) {
                ids.retain(|&i| i != id);
            }
        }
    }
    
    /// Prewarm connections for a host
    pub fn prewarm(&mut self, host: &HostKey) -> Vec<ConnId> {
        let mut created = Vec::new();
        
        if !self.config.prewarm_enabled {
            return created;
        }
        
        let current = self.by_host.get(host).map(|v| v.len()).unwrap_or(0);
        let to_create = self.config.max_prewarmed.saturating_sub(current);
        
        for _ in 0..to_create {
            if self.connections.len() >= self.config.max_total {
                break;
            }
            
            let id = self.next_id;
            self.next_id += 1;
            
            let mut conn = PooledConnection::new(id, &host.host, host.port);
            conn.tls = if host.is_tls { TlsState::Handshaking } else { TlsState::None };
            conn.is_prewarmed = true;
            conn.state = ConnState::Ready; // Assume instant connection for demo
            
            self.connections.insert(id, conn);
            self.by_host.entry(host.clone()).or_default().push(id);
            self.stats.connections_created += 1;
            
            created.push(id);
        }
        
        created
    }
    
    /// Queue a host for prewarming
    pub fn queue_prewarm(&mut self, host: HostKey) {
        if !self.prewarm_queue.contains(&host) {
            self.prewarm_queue.push(host);
        }
    }
    
    /// Process prewarm queue (call during idle time)
    pub fn process_prewarm_queue(&mut self) -> Vec<ConnId> {
        let mut created = Vec::new();
        
        while let Some(host) = self.prewarm_queue.pop() {
            created.extend(self.prewarm(&host));
        }
        
        created
    }
    
    /// Evict stale connections
    pub fn evict_stale(&mut self) {
        let stale: Vec<_> = self.connections.iter()
            .filter(|(_, conn)| conn.is_stale(self.config.idle_timeout) || conn.age() > self.config.max_age)
            .map(|(&id, _)| id)
            .collect();
        
        for id in stale {
            self.close(id);
        }
    }
    
    /// Mark TLS as established (with possible resumption)
    pub fn tls_established(&mut self, id: ConnId, resumed: bool) {
        if let Some(conn) = self.connections.get_mut(&id) {
            conn.tls = TlsState::Established;
            conn.state = ConnState::Ready;
            
            if resumed {
                self.stats.tls_resumptions += 1;
            }
        }
    }
    
    /// Get connection by ID
    pub fn get(&self, id: ConnId) -> Option<&PooledConnection> {
        self.connections.get(&id)
    }
    
    /// Get mutable connection
    pub fn get_mut(&mut self, id: ConnId) -> Option<&mut PooledConnection> {
        self.connections.get_mut(&id)
    }
    
    /// Get statistics
    pub fn stats(&self) -> &PoolStats {
        &self.stats
    }
    
    /// Total connection count
    pub fn len(&self) -> usize {
        self.connections.len()
    }
    
    /// Is empty
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }
}

/// Result of connection acquire
#[derive(Debug, Clone, Copy)]
pub enum AcquireResult {
    /// Reused existing connection
    Reused(ConnId),
    /// Created new connection
    Created(ConnId),
    /// Need to wait for a connection
    WaitForConnection,
    /// Pool is exhausted
    PoolExhausted,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_host_key_from_url() {
        let key = HostKey::from_url("https://example.com/page").unwrap();
        assert_eq!(key.host.as_ref(), "example.com");
        assert_eq!(key.port, 443);
        assert!(key.is_tls);
        
        let key2 = HostKey::from_url("http://test.com:8080/api").unwrap();
        assert_eq!(key2.host.as_ref(), "test.com");
        assert_eq!(key2.port, 8080);
        assert!(!key2.is_tls);
    }
    
    #[test]
    fn test_connection_pool() {
        let mut pool = ConnectionPool::default();
        
        let host = HostKey::new("example.com", 443, true);
        
        // First acquire creates connection
        match pool.acquire(&host) {
            AcquireResult::Created(id) => {
                assert_eq!(id, 0);
                pool.release(id);
            }
            _ => panic!("Expected Created"),
        }
        
        // Second acquire reuses
        match pool.acquire(&host) {
            AcquireResult::Reused(id) => {
                assert_eq!(id, 0);
            }
            _ => panic!("Expected Reused"),
        }
    }
    
    #[test]
    fn test_prewarming() {
        let config = PoolConfig {
            max_prewarmed: 2,
            ..Default::default()
        };
        
        let mut pool = ConnectionPool::new(config);
        let host = HostKey::new("example.com", 443, true);
        
        let prewarmed = pool.prewarm(&host);
        assert_eq!(prewarmed.len(), 2);
        
        // Acquire should get prewarmed connection
        match pool.acquire(&host) {
            AcquireResult::Reused(id) => {
                let conn = pool.get(id).unwrap();
                assert!(conn.is_prewarmed);
            }
            _ => panic!("Expected Reused"),
        }
        
        assert_eq!(pool.stats().prewarmed_hits, 1);
    }
}
