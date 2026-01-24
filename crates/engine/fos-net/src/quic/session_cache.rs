//! QUIC Session Ticket Cache
//!
//! Session ticket storage for 0-RTT resumption.
//! Enables fast connection establishment with previously visited servers.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime};

/// Maximum age for session tickets (7 days)
const MAX_TICKET_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Maximum tickets per host
const MAX_TICKETS_PER_HOST: usize = 4;

/// Session ticket for 0-RTT resumption
#[derive(Debug, Clone)]
pub struct SessionTicket {
    /// Ticket data (encrypted)
    pub data: Vec<u8>,
    /// Server name (SNI)
    pub server_name: String,
    /// Application-Layer Protocol Negotiation (ALPN)
    pub alpn: Vec<u8>,
    /// Max early data size (0 if 0-RTT not supported)
    pub max_early_data: u32,
    /// Cipher suite used
    pub cipher_suite: u16,
    /// Ticket lifetime
    pub lifetime: Duration,
    /// Creation time
    pub created: Instant,
    /// Expiration time (absolute)
    pub expires: SystemTime,
    /// Resumption secret
    pub resumption_secret: Vec<u8>,
    /// Ticket age add (obfuscation)
    pub ticket_age_add: u32,
    /// Transport parameters from server
    pub transport_params: Option<TransportParameters>,
}

/// Transport parameters stored with ticket
#[derive(Debug, Clone, Default)]
pub struct TransportParameters {
    /// Initial max stream data (bidi local)
    pub initial_max_stream_data_bidi_local: u64,
    /// Initial max stream data (bidi remote)
    pub initial_max_stream_data_bidi_remote: u64,
    /// Initial max stream data (uni)
    pub initial_max_stream_data_uni: u64,
    /// Initial max streams (bidi)
    pub initial_max_streams_bidi: u64,
    /// Initial max streams (uni)
    pub initial_max_streams_uni: u64,
    /// Initial max data
    pub initial_max_data: u64,
    /// Max idle timeout
    pub max_idle_timeout: u64,
    /// Max UDP payload size
    pub max_udp_payload_size: u64,
}

impl SessionTicket {
    /// Create a new session ticket
    pub fn new(
        data: Vec<u8>,
        server_name: String,
        alpn: Vec<u8>,
        lifetime: Duration,
        resumption_secret: Vec<u8>,
    ) -> Self {
        let now = Instant::now();
        let expires = SystemTime::now() + lifetime.min(MAX_TICKET_AGE);
        
        Self {
            data,
            server_name,
            alpn,
            max_early_data: 0,
            cipher_suite: 0x1301, // TLS_AES_128_GCM_SHA256
            lifetime,
            created: now,
            expires,
            resumption_secret,
            ticket_age_add: rand_u32(),
            transport_params: None,
        }
    }
    
    /// Check if ticket is expired
    pub fn is_expired(&self) -> bool {
        SystemTime::now() > self.expires
    }
    
    /// Check if 0-RTT is supported
    pub fn supports_early_data(&self) -> bool {
        self.max_early_data > 0
    }
    
    /// Get obfuscated ticket age
    pub fn obfuscated_age(&self) -> u32 {
        let age_ms = self.created.elapsed().as_millis() as u32;
        age_ms.wrapping_add(self.ticket_age_add)
    }
    
    /// Set max early data size
    pub fn with_max_early_data(mut self, size: u32) -> Self {
        self.max_early_data = size;
        self
    }
    
    /// Set cipher suite
    pub fn with_cipher_suite(mut self, suite: u16) -> Self {
        self.cipher_suite = suite;
        self
    }
    
    /// Set transport parameters
    pub fn with_transport_params(mut self, params: TransportParameters) -> Self {
        self.transport_params = Some(params);
        self
    }
}

/// Session cache key
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionCacheKey {
    /// Server name
    pub server_name: String,
    /// Port
    pub port: u16,
}

impl SessionCacheKey {
    /// Create a new cache key
    pub fn new(server_name: &str, port: u16) -> Self {
        Self {
            server_name: server_name.to_lowercase(),
            port,
        }
    }
}

/// Session ticket cache for 0-RTT resumption
#[derive(Debug, Default)]
pub struct SessionCache {
    /// Tickets by server
    tickets: HashMap<SessionCacheKey, Vec<SessionTicket>>,
    /// Statistics
    stats: SessionCacheStats,
    /// Maximum total tickets
    max_entries: usize,
}

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct SessionCacheStats {
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Tickets stored
    pub stores: u64,
    /// Tickets evicted (expired)
    pub evictions: u64,
    /// 0-RTT attempts
    pub early_data_attempts: u64,
    /// 0-RTT accepted
    pub early_data_accepted: u64,
    /// 0-RTT rejected
    pub early_data_rejected: u64,
}

impl SessionCacheStats {
    /// Get cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
    
    /// Get 0-RTT success rate
    pub fn early_data_success_rate(&self) -> f64 {
        if self.early_data_attempts == 0 {
            0.0
        } else {
            self.early_data_accepted as f64 / self.early_data_attempts as f64
        }
    }
}

impl SessionCache {
    /// Create a new session cache
    pub fn new() -> Self {
        Self {
            tickets: HashMap::new(),
            stats: SessionCacheStats::default(),
            max_entries: 1000,
        }
    }
    
    /// Create with max entries
    pub fn with_max_entries(max: usize) -> Self {
        Self {
            max_entries: max,
            ..Self::new()
        }
    }
    
    /// Store a session ticket
    pub fn store(&mut self, key: SessionCacheKey, ticket: SessionTicket) {
        self.stats.stores += 1;
        
        // Evict expired tickets first
        self.evict_expired();
        
        let tickets = self.tickets.entry(key).or_default();
        
        // Limit tickets per host
        while tickets.len() >= MAX_TICKETS_PER_HOST {
            tickets.remove(0);
        }
        
        tickets.push(ticket);
        
        // Check total limit
        self.enforce_limit();
    }
    
    /// Get a session ticket for resumption
    pub fn get(&mut self, key: &SessionCacheKey) -> Option<&SessionTicket> {
        self.evict_expired_for_key(key);
        
        if let Some(tickets) = self.tickets.get(key) {
            if let Some(ticket) = tickets.last() {
                if !ticket.is_expired() {
                    self.stats.hits += 1;
                    return Some(ticket);
                }
            }
        }
        
        self.stats.misses += 1;
        None
    }
    
    /// Take a session ticket (removes from cache)
    pub fn take(&mut self, key: &SessionCacheKey) -> Option<SessionTicket> {
        self.evict_expired_for_key(key);
        
        if let Some(tickets) = self.tickets.get_mut(key) {
            if !tickets.is_empty() {
                let ticket = tickets.pop();
                if let Some(ref t) = ticket {
                    if !t.is_expired() {
                        self.stats.hits += 1;
                        return ticket;
                    }
                }
            }
        }
        
        self.stats.misses += 1;
        None
    }
    
    /// Check if a ticket exists (without consuming)
    pub fn contains(&self, key: &SessionCacheKey) -> bool {
        if let Some(tickets) = self.tickets.get(key) {
            tickets.iter().any(|t| !t.is_expired())
        } else {
            false
        }
    }
    
    /// Get statistics
    pub fn stats(&self) -> &SessionCacheStats {
        &self.stats
    }
    
    /// Record 0-RTT attempt
    pub fn record_early_data_attempt(&mut self, accepted: bool) {
        self.stats.early_data_attempts += 1;
        if accepted {
            self.stats.early_data_accepted += 1;
        } else {
            self.stats.early_data_rejected += 1;
        }
    }
    
    /// Clear all tickets
    pub fn clear(&mut self) {
        self.tickets.clear();
    }
    
    /// Get number of cached tickets
    pub fn len(&self) -> usize {
        self.tickets.values().map(|v| v.len()).sum()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.tickets.is_empty()
    }
    
    /// Remove expired tickets
    pub fn evict_expired(&mut self) {
        let mut to_remove = Vec::new();
        
        for (key, tickets) in &mut self.tickets {
            let before = tickets.len();
            tickets.retain(|t| !t.is_expired());
            let evicted = before - tickets.len();
            self.stats.evictions += evicted as u64;
            
            if tickets.is_empty() {
                to_remove.push(key.clone());
            }
        }
        
        for key in to_remove {
            self.tickets.remove(&key);
        }
    }
    
    fn evict_expired_for_key(&mut self, key: &SessionCacheKey) {
        if let Some(tickets) = self.tickets.get_mut(key) {
            let before = tickets.len();
            tickets.retain(|t| !t.is_expired());
            self.stats.evictions += (before - tickets.len()) as u64;
        }
    }
    
    fn enforce_limit(&mut self) {
        while self.len() > self.max_entries {
            // Remove oldest ticket
            let mut oldest_key = None;
            let mut oldest_time = Instant::now();
            
            for (key, tickets) in &self.tickets {
                if let Some(ticket) = tickets.first() {
                    if ticket.created < oldest_time {
                        oldest_time = ticket.created;
                        oldest_key = Some(key.clone());
                    }
                }
            }
            
            if let Some(key) = oldest_key {
                if let Some(tickets) = self.tickets.get_mut(&key) {
                    if !tickets.is_empty() {
                        tickets.remove(0);
                        self.stats.evictions += 1;
                    }
                    if tickets.is_empty() {
                        self.tickets.remove(&key);
                    }
                }
            } else {
                break;
            }
        }
    }
}

/// Generate a pseudo-random u32 for ticket age obfuscation
fn rand_u32() -> u32 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64);
    hasher.finish() as u32
}

/// Early data (0-RTT) buffer
#[derive(Debug, Default)]
pub struct EarlyDataBuffer {
    /// Buffered early data
    data: Vec<u8>,
    /// Maximum size
    max_size: usize,
    /// Whether early data was accepted
    accepted: Option<bool>,
}

impl EarlyDataBuffer {
    /// Create a new early data buffer
    pub fn new(max_size: u32) -> Self {
        Self {
            data: Vec::new(),
            max_size: max_size as usize,
            accepted: None,
        }
    }
    
    /// Write early data
    pub fn write(&mut self, data: &[u8]) -> usize {
        let available = self.max_size.saturating_sub(self.data.len());
        let to_write = data.len().min(available);
        self.data.extend_from_slice(&data[..to_write]);
        to_write
    }
    
    /// Get buffered data
    pub fn data(&self) -> &[u8] {
        &self.data
    }
    
    /// Take all data
    pub fn take(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.data)
    }
    
    /// Mark as accepted
    pub fn set_accepted(&mut self) {
        self.accepted = Some(true);
    }
    
    /// Mark as rejected
    pub fn set_rejected(&mut self) {
        self.accepted = Some(false);
    }
    
    /// Check if accepted
    pub fn is_accepted(&self) -> Option<bool> {
        self.accepted
    }
    
    /// Check if buffer is full
    pub fn is_full(&self) -> bool {
        self.data.len() >= self.max_size
    }
    
    /// Get remaining capacity
    pub fn remaining(&self) -> usize {
        self.max_size.saturating_sub(self.data.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    fn make_ticket(server: &str) -> SessionTicket {
        SessionTicket::new(
            vec![1, 2, 3, 4],
            server.to_string(),
            b"h3".to_vec(),
            Duration::from_secs(3600),
            vec![0; 32],
        )
    }
    
    #[test]
    fn test_session_ticket_creation() {
        let ticket = make_ticket("example.com");
        assert_eq!(ticket.server_name, "example.com");
        assert!(!ticket.is_expired());
        assert!(!ticket.supports_early_data());
    }
    
    #[test]
    fn test_session_ticket_early_data() {
        let ticket = make_ticket("example.com")
            .with_max_early_data(16384);
        assert!(ticket.supports_early_data());
    }
    
    #[test]
    fn test_session_cache_store_get() {
        let mut cache = SessionCache::new();
        let key = SessionCacheKey::new("example.com", 443);
        let ticket = make_ticket("example.com");
        
        cache.store(key.clone(), ticket);
        
        assert!(cache.contains(&key));
        assert!(cache.get(&key).is_some());
        assert_eq!(cache.stats().stores, 1);
        assert_eq!(cache.stats().hits, 1);
    }
    
    #[test]
    fn test_session_cache_take() {
        let mut cache = SessionCache::new();
        let key = SessionCacheKey::new("example.com", 443);
        cache.store(key.clone(), make_ticket("example.com"));
        
        let ticket = cache.take(&key);
        assert!(ticket.is_some());
        assert!(!cache.contains(&key));
    }
    
    #[test]
    fn test_session_cache_miss() {
        let mut cache = SessionCache::new();
        let key = SessionCacheKey::new("unknown.com", 443);
        
        assert!(cache.get(&key).is_none());
        assert_eq!(cache.stats().misses, 1);
    }
    
    #[test]
    fn test_session_cache_limit_per_host() {
        let mut cache = SessionCache::new();
        let key = SessionCacheKey::new("example.com", 443);
        
        for i in 0..10 {
            let mut ticket = make_ticket("example.com");
            ticket.data = vec![i];
            cache.store(key.clone(), ticket);
        }
        
        // Should only keep MAX_TICKETS_PER_HOST
        assert!(cache.len() <= MAX_TICKETS_PER_HOST);
    }
    
    #[test]
    fn test_early_data_buffer() {
        let mut buffer = EarlyDataBuffer::new(100);
        
        let written = buffer.write(b"Hello, World!");
        assert_eq!(written, 13);
        assert_eq!(buffer.data(), b"Hello, World!");
        assert!(!buffer.is_full());
        
        buffer.set_accepted();
        assert_eq!(buffer.is_accepted(), Some(true));
    }
    
    #[test]
    fn test_early_data_buffer_limit() {
        let mut buffer = EarlyDataBuffer::new(10);
        
        let written = buffer.write(b"This is too long");
        assert_eq!(written, 10);
        assert!(buffer.is_full());
        assert_eq!(buffer.remaining(), 0);
    }
    
    #[test]
    fn test_cache_stats() {
        let mut cache = SessionCache::new();
        let key = SessionCacheKey::new("example.com", 443);
        
        cache.store(key.clone(), make_ticket("example.com"));
        cache.get(&key);
        cache.get(&SessionCacheKey::new("miss.com", 443));
        
        cache.record_early_data_attempt(true);
        cache.record_early_data_attempt(false);
        
        let stats = cache.stats();
        assert!(stats.hit_rate() > 0.0);
        assert_eq!(stats.early_data_success_rate(), 0.5);
    }
}
