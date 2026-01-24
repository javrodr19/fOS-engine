//! Tiered Connection Pool
//!
//! Memory-efficient connection management with hot/warm/cold tiers.
//! Reduces memory usage while maintaining connection reuse.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Connection key for pool lookup
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConnectionKey {
    /// Host
    pub host: String,
    /// Port
    pub port: u16,
    /// Is secure (HTTPS/TLS)
    pub secure: bool,
    /// HTTP version
    pub http_version: HttpVersion,
}

/// HTTP version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HttpVersion {
    Http1,
    Http2,
    Http3,
}

impl ConnectionKey {
    /// Create a new key
    pub fn new(host: &str, port: u16, secure: bool, http_version: HttpVersion) -> Self {
        Self {
            host: host.to_lowercase(),
            port,
            secure,
            http_version,
        }
    }
    
    /// Create for HTTPS
    pub fn https(host: &str) -> Self {
        Self::new(host, 443, true, HttpVersion::Http2)
    }
    
    /// Create for HTTP
    pub fn http(host: &str) -> Self {
        Self::new(host, 80, false, HttpVersion::Http1)
    }
}

/// Connection state for serialization
#[derive(Debug, Clone)]
pub struct ConnectionState {
    /// Key for restoring
    pub key: ConnectionKey,
    /// Last used time
    pub last_used: Instant,
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_recv: u64,
    /// Connection ID
    pub id: u64,
    /// Session ticket for resumption
    pub session_ticket: Option<Vec<u8>>,
}

/// Pooled connection wrapper
#[derive(Debug)]
pub struct PooledConnection {
    /// Connection ID
    pub id: u64,
    /// Connection key
    pub key: ConnectionKey,
    /// When connection was created
    pub created: Instant,
    /// Last used time
    pub last_used: Instant,
    /// Bytes sent
    pub bytes_sent: u64,
    /// Bytes received
    pub bytes_recv: u64,
    /// Is currently in use
    pub in_use: bool,
    /// Request count on this connection
    pub request_count: u32,
    /// Session resumption data
    pub session_data: Option<Vec<u8>>,
}

impl PooledConnection {
    /// Create a new pooled connection
    pub fn new(id: u64, key: ConnectionKey) -> Self {
        let now = Instant::now();
        Self {
            id,
            key,
            created: now,
            last_used: now,
            bytes_sent: 0,
            bytes_recv: 0,
            in_use: false,
            request_count: 0,
            session_data: None,
        }
    }
    
    /// Mark as used
    pub fn mark_used(&mut self) {
        self.last_used = Instant::now();
        self.request_count += 1;
    }
    
    /// Get idle duration
    pub fn idle_duration(&self) -> Duration {
        self.last_used.elapsed()
    }
    
    /// Get connection age
    pub fn age(&self) -> Duration {
        self.created.elapsed()
    }
    
    /// Serialize to state for cold storage
    pub fn to_state(&self) -> ConnectionState {
        ConnectionState {
            key: self.key.clone(),
            last_used: self.last_used,
            bytes_sent: self.bytes_sent,
            bytes_recv: self.bytes_recv,
            id: self.id,
            session_ticket: self.session_data.clone(),
        }
    }
}

/// LRU cache for hot tier
#[derive(Debug, Default)]
struct LruCache {
    /// Connections in LRU order
    entries: VecDeque<u64>,
    /// Lookup by ID
    lookup: HashMap<u64, PooledConnection>,
    /// Max capacity
    capacity: usize,
}

impl LruCache {
    fn new(capacity: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            lookup: HashMap::new(),
            capacity,
        }
    }
    
    fn get(&mut self, id: u64) -> Option<&mut PooledConnection> {
        if self.lookup.contains_key(&id) {
            // Move to front (most recently used)
            self.entries.retain(|&x| x != id);
            self.entries.push_front(id);
            self.lookup.get_mut(&id)
        } else {
            None
        }
    }
    
    fn insert(&mut self, conn: PooledConnection) -> Option<PooledConnection> {
        let id = conn.id;
        let evicted = if self.entries.len() >= self.capacity {
            // Evict least recently used
            self.entries.pop_back().and_then(|old_id| self.lookup.remove(&old_id))
        } else {
            None
        };
        
        self.entries.push_front(id);
        self.lookup.insert(id, conn);
        evicted
    }
    
    fn remove(&mut self, id: u64) -> Option<PooledConnection> {
        self.entries.retain(|&x| x != id);
        self.lookup.remove(&id)
    }
    
    fn len(&self) -> usize {
        self.lookup.len()
    }
    
    fn iter(&self) -> impl Iterator<Item = &PooledConnection> {
        self.lookup.values()
    }
    
    fn iter_mut(&mut self) -> impl Iterator<Item = &mut PooledConnection> {
        self.lookup.values_mut()
    }
}

/// Tiered connection pool
/// - Hot tier: Active connections in LRU cache
/// - Warm tier: Recently used, may be closed soon
/// - Cold tier: Serialized state for resumption
#[derive(Debug)]
pub struct TieredConnectionPool {
    /// Hot tier: Active connections (LRU)
    hot: LruCache,
    /// Warm tier: Recent but idle connections
    warm: Vec<PooledConnection>,
    /// Cold tier: Serialized connection states
    cold: BTreeMap<ConnectionKey, ConnectionState>,
    /// Next connection ID
    next_id: u64,
    /// Configuration
    config: PoolConfig,
    /// Statistics
    stats: PoolStats,
}

/// Pool configuration
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Max hot tier connections
    pub hot_capacity: usize,
    /// Max warm tier connections
    pub warm_capacity: usize,
    /// Max cold tier entries
    pub cold_capacity: usize,
    /// Idle timeout for warm tier
    pub warm_timeout: Duration,
    /// Idle timeout for cold promotion
    pub cold_timeout: Duration,
    /// Max connection age
    pub max_age: Duration,
    /// Max connections per host
    pub per_host_limit: usize,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            hot_capacity: 64,
            warm_capacity: 128,
            cold_capacity: 256,
            warm_timeout: Duration::from_secs(30),
            cold_timeout: Duration::from_secs(120),
            max_age: Duration::from_secs(3600),
            per_host_limit: 6,
        }
    }
}

/// Pool statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct PoolStats {
    /// Hot tier hits
    pub hot_hits: u64,
    /// Warm tier hits
    pub warm_hits: u64,
    /// Cold tier hits (resumption)
    pub cold_hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Connections created
    pub connections_created: u64,
    /// Connections evicted
    pub evictions: u64,
    /// Current hot count
    pub hot_count: usize,
    /// Current warm count
    pub warm_count: usize,
    /// Current cold count
    pub cold_count: usize,
}

impl PoolStats {
    /// Get hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hot_hits + self.warm_hits + self.cold_hits + self.misses;
        if total == 0 {
            0.0
        } else {
            (self.hot_hits + self.warm_hits + self.cold_hits) as f64 / total as f64
        }
    }
    
    /// Get memory saved by tiering (estimate)
    pub fn estimated_memory_saved(&self) -> usize {
        // Cold tier only stores ~100 bytes vs ~64KB per connection
        self.cold_count * (64 * 1024 - 100)
    }
}

impl Default for TieredConnectionPool {
    fn default() -> Self {
        Self::new(PoolConfig::default())
    }
}

impl TieredConnectionPool {
    /// Create a new tiered pool
    pub fn new(config: PoolConfig) -> Self {
        Self {
            hot: LruCache::new(config.hot_capacity),
            warm: Vec::new(),
            cold: BTreeMap::new(),
            next_id: 1,
            config,
            stats: PoolStats::default(),
        }
    }
    
    /// Acquire a connection for a key
    pub fn acquire(&mut self, key: &ConnectionKey) -> AcquireResult {
        // Try hot tier first
        for conn in self.hot.iter_mut() {
            if &conn.key == key && !conn.in_use {
                conn.in_use = true;
                conn.mark_used();
                self.stats.hot_hits += 1;
                return AcquireResult::Existing(conn.id);
            }
        }
        
        // Try warm tier
        if let Some(pos) = self.warm.iter().position(|c| &c.key == key) {
            let mut conn = self.warm.remove(pos);
            conn.in_use = true;
            conn.mark_used();
            let id = conn.id;
            
            // Promote to hot
            if let Some(evicted) = self.hot.insert(conn) {
                self.demote_to_warm(evicted);
            }
            
            self.stats.warm_hits += 1;
            return AcquireResult::Existing(id);
        }
        
        // Check cold tier for resumption data
        if let Some(state) = self.cold.remove(key) {
            self.stats.cold_hits += 1;
            return AcquireResult::Resume(state);
        }
        
        self.stats.misses += 1;
        AcquireResult::New
    }
    
    /// Create and register a new connection
    pub fn create(&mut self, key: ConnectionKey) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        
        let mut conn = PooledConnection::new(id, key);
        conn.in_use = true;
        
        if let Some(evicted) = self.hot.insert(conn) {
            self.demote_to_warm(evicted);
        }
        
        self.stats.connections_created += 1;
        self.update_counts();
        id
    }
    
    /// Release a connection back to the pool
    pub fn release(&mut self, id: u64) {
        if let Some(conn) = self.hot.get(id) {
            conn.in_use = false;
        }
    }
    
    /// Close a connection
    pub fn close(&mut self, id: u64) {
        if let Some(conn) = self.hot.remove(id) {
            // Store in cold tier if has session data
            if conn.session_data.is_some() {
                let state = conn.to_state();
                self.store_cold(conn.key.clone(), state);
            }
        }
        
        self.warm.retain(|c| c.id != id);
        self.update_counts();
    }
    
    /// Perform maintenance (call periodically)
    pub fn maintain(&mut self) {
        let now = Instant::now();
        
        // Demote idle hot connections to warm
        let mut to_demote = Vec::new();
        for conn in self.hot.iter() {
            if !conn.in_use && conn.idle_duration() > self.config.warm_timeout {
                to_demote.push(conn.id);
            }
        }
        
        for id in to_demote {
            if let Some(conn) = self.hot.remove(id) {
                self.demote_to_warm(conn);
            }
        }
        
        // Demote idle warm connections to cold
        let cold_timeout = self.config.cold_timeout;
        let mut to_cold = Vec::new();
        
        self.warm.retain(|conn| {
            if conn.idle_duration() > cold_timeout {
                to_cold.push(conn.to_state());
                false
            } else {
                true
            }
        });
        
        for state in to_cold {
            self.store_cold(state.key.clone(), state);
        }
        
        // Evict expired cold entries
        self.cold.retain(|_, state| {
            state.last_used.elapsed() < self.config.max_age
        });
        
        self.update_counts();
    }
    
    /// Get statistics
    pub fn stats(&self) -> PoolStats {
        self.stats
    }
    
    /// Get connection count
    pub fn connection_count(&self) -> usize {
        self.hot.len() + self.warm.len()
    }
    
    /// Clear all connections
    pub fn clear(&mut self) {
        self.hot = LruCache::new(self.config.hot_capacity);
        self.warm.clear();
        self.cold.clear();
        self.update_counts();
    }
    
    fn demote_to_warm(&mut self, conn: PooledConnection) {
        if self.warm.len() >= self.config.warm_capacity {
            // Evict oldest warm
            if let Some(evicted) = self.warm.pop() {
                let state = evicted.to_state();
                self.store_cold(evicted.key.clone(), state);
                self.stats.evictions += 1;
            }
        }
        self.warm.push(conn);
    }
    
    fn store_cold(&mut self, key: ConnectionKey, state: ConnectionState) {
        if self.cold.len() >= self.config.cold_capacity {
            // Remove oldest
            if let Some(oldest_key) = self.cold.keys().next().cloned() {
                self.cold.remove(&oldest_key);
            }
        }
        self.cold.insert(key, state);
    }
    
    fn update_counts(&mut self) {
        self.stats.hot_count = self.hot.len();
        self.stats.warm_count = self.warm.len();
        self.stats.cold_count = self.cold.len();
    }
}

/// Result of connection acquisition
#[derive(Debug)]
pub enum AcquireResult {
    /// Existing connection available
    Existing(u64),
    /// No connection, but have resumption data
    Resume(ConnectionState),
    /// Need to create new connection
    New,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_connection_key() {
        let key = ConnectionKey::https("example.com");
        assert_eq!(key.host, "example.com");
        assert_eq!(key.port, 443);
        assert!(key.secure);
    }
    
    #[test]
    fn test_pool_acquire_miss() {
        let mut pool = TieredConnectionPool::default();
        let key = ConnectionKey::https("example.com");
        
        match pool.acquire(&key) {
            AcquireResult::New => {}
            _ => panic!("Expected New"),
        }
        
        assert_eq!(pool.stats().misses, 1);
    }
    
    #[test]
    fn test_pool_create_and_acquire() {
        let mut pool = TieredConnectionPool::default();
        let key = ConnectionKey::https("example.com");
        
        let id = pool.create(key.clone());
        pool.release(id);
        
        match pool.acquire(&key) {
            AcquireResult::Existing(conn_id) => assert_eq!(conn_id, id),
            _ => panic!("Expected Existing"),
        }
        
        assert_eq!(pool.stats().hot_hits, 1);
    }
    
    #[test]
    fn test_pool_close() {
        let mut pool = TieredConnectionPool::default();
        let key = ConnectionKey::https("example.com");
        
        let id = pool.create(key.clone());
        pool.close(id);
        
        assert_eq!(pool.connection_count(), 0);
    }
    
    #[test]
    fn test_pool_stats() {
        let pool = TieredConnectionPool::default();
        let stats = pool.stats();
        
        assert_eq!(stats.hot_count, 0);
        assert_eq!(stats.warm_count, 0);
        assert_eq!(stats.cold_count, 0);
    }
    
    #[test]
    fn test_lru_eviction() {
        let mut pool = TieredConnectionPool::new(PoolConfig {
            hot_capacity: 2,
            ..Default::default()
        });
        
        let key1 = ConnectionKey::https("host1.com");
        let key2 = ConnectionKey::https("host2.com");
        let key3 = ConnectionKey::https("host3.com");
        
        pool.create(key1.clone());
        pool.create(key2.clone());
        let id3 = pool.create(key3.clone());
        
        // Hot should have 2, one evicted to warm
        assert_eq!(pool.stats().hot_count, 2);
        assert!(pool.stats().warm_count > 0 || pool.stats().evictions > 0);
    }
}
