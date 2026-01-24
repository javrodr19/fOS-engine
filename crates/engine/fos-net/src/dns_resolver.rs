//! DNS Resolver
//!
//! DNS-over-HTTPS (DoH) and DNS-over-TLS (DoT) resolution.
//! System fallback when secure DNS unavailable.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::time::{Duration, Instant};

/// DNS resolver types
#[derive(Debug, Clone)]
pub enum DnsResolver {
    /// System resolver
    System,
    /// DNS-over-HTTPS
    DoH { endpoint: String },
    /// DNS-over-TLS
    DoT { server: IpAddr },
}

impl Default for DnsResolver {
    fn default() -> Self {
        Self::System
    }
}

/// Well-known DoH providers
pub mod providers {
    /// Cloudflare DNS
    pub const CLOUDFLARE: &str = "https://cloudflare-dns.com/dns-query";
    /// Google DNS
    pub const GOOGLE: &str = "https://dns.google/dns-query";
    /// Quad9 DNS
    pub const QUAD9: &str = "https://dns.quad9.net/dns-query";
    /// NextDNS
    pub const NEXTDNS: &str = "https://dns.nextdns.io/dns-query";
}

/// DNS record type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordType {
    /// IPv4 address
    A,
    /// IPv6 address
    AAAA,
    /// Canonical name
    CNAME,
    /// Mail exchange
    MX,
    /// Text record
    TXT,
    /// HTTPS/SVCB service binding
    HTTPS,
    /// Name server
    NS,
    /// Service locator
    SRV,
}

impl RecordType {
    /// Get type code
    pub fn code(&self) -> u16 {
        match self {
            Self::A => 1,
            Self::AAAA => 28,
            Self::CNAME => 5,
            Self::MX => 15,
            Self::TXT => 16,
            Self::HTTPS => 65,
            Self::NS => 2,
            Self::SRV => 33,
        }
    }
    
    /// From type code
    pub fn from_code(code: u16) -> Option<Self> {
        match code {
            1 => Some(Self::A),
            28 => Some(Self::AAAA),
            5 => Some(Self::CNAME),
            15 => Some(Self::MX),
            16 => Some(Self::TXT),
            65 => Some(Self::HTTPS),
            2 => Some(Self::NS),
            33 => Some(Self::SRV),
            _ => None,
        }
    }
}

/// DNS query
#[derive(Debug, Clone)]
pub struct DnsQuery {
    /// Query name
    pub name: String,
    /// Query type
    pub record_type: RecordType,
    /// Query class (usually IN = 1)
    pub class: u16,
}

impl DnsQuery {
    /// Create an A record query
    pub fn a(name: &str) -> Self {
        Self {
            name: name.to_string(),
            record_type: RecordType::A,
            class: 1,
        }
    }
    
    /// Create an AAAA record query
    pub fn aaaa(name: &str) -> Self {
        Self {
            name: name.to_string(),
            record_type: RecordType::AAAA,
            class: 1,
        }
    }
    
    /// Create an HTTPS record query
    pub fn https(name: &str) -> Self {
        Self {
            name: name.to_string(),
            record_type: RecordType::HTTPS,
            class: 1,
        }
    }
    
    /// Encode as DNS wire format
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        
        // Transaction ID (random)
        let id = rand_u16();
        buf.extend_from_slice(&id.to_be_bytes());
        
        // Flags (standard query, recursion desired)
        buf.extend_from_slice(&0x0100u16.to_be_bytes());
        
        // Questions: 1, Answers: 0, Authority: 0, Additional: 0
        buf.extend_from_slice(&1u16.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
        buf.extend_from_slice(&0u16.to_be_bytes());
        
        // Query name
        for label in self.name.split('.') {
            buf.push(label.len() as u8);
            buf.extend_from_slice(label.as_bytes());
        }
        buf.push(0); // Root label
        
        // Query type
        buf.extend_from_slice(&self.record_type.code().to_be_bytes());
        
        // Query class
        buf.extend_from_slice(&self.class.to_be_bytes());
        
        buf
    }
}

/// DNS response
#[derive(Debug, Clone)]
pub struct DnsResponse {
    /// Query ID
    pub id: u16,
    /// Response code
    pub rcode: ResponseCode,
    /// Answer records
    pub answers: Vec<DnsRecord>,
    /// Authority records
    pub authority: Vec<DnsRecord>,
    /// Additional records
    pub additional: Vec<DnsRecord>,
    /// Whether response is authoritative
    pub authoritative: bool,
    /// Whether recursion is available
    pub recursion_available: bool,
}

/// DNS response code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseCode {
    /// No error
    NoError,
    /// Format error
    FormErr,
    /// Server failure
    ServFail,
    /// Name error (NXDOMAIN)
    NXDomain,
    /// Not implemented
    NotImp,
    /// Refused
    Refused,
    /// Other
    Other(u8),
}

impl ResponseCode {
    /// From raw code
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Self::NoError,
            1 => Self::FormErr,
            2 => Self::ServFail,
            3 => Self::NXDomain,
            4 => Self::NotImp,
            5 => Self::Refused,
            n => Self::Other(n),
        }
    }
    
    /// Is success
    pub fn is_success(&self) -> bool {
        matches!(self, Self::NoError)
    }
}

/// DNS record
#[derive(Debug, Clone)]
pub struct DnsRecord {
    /// Name
    pub name: String,
    /// Record type
    pub record_type: RecordType,
    /// TTL in seconds
    pub ttl: u32,
    /// Record data
    pub data: RecordData,
}

/// Record data
#[derive(Debug, Clone)]
pub enum RecordData {
    /// IPv4 address
    A(Ipv4Addr),
    /// IPv6 address
    AAAA(Ipv6Addr),
    /// Canonical name
    CNAME(String),
    /// Mail exchange
    MX { priority: u16, exchange: String },
    /// Text record
    TXT(String),
    /// HTTPS service binding
    HTTPS { priority: u16, target: String, alpn: Vec<String> },
    /// Raw data
    Raw(Vec<u8>),
}

impl DnsResponse {
    /// Parse from DNS wire format
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 12 {
            return None;
        }
        
        let id = u16::from_be_bytes([data[0], data[1]]);
        let flags = u16::from_be_bytes([data[2], data[3]]);
        
        let authoritative = (flags & 0x0400) != 0;
        let recursion_available = (flags & 0x0080) != 0;
        let rcode = ResponseCode::from_code((flags & 0x000F) as u8);
        
        let qdcount = u16::from_be_bytes([data[4], data[5]]) as usize;
        let ancount = u16::from_be_bytes([data[6], data[7]]) as usize;
        let nscount = u16::from_be_bytes([data[8], data[9]]) as usize;
        let arcount = u16::from_be_bytes([data[10], data[11]]) as usize;
        
        let mut pos = 12;
        
        // Skip questions
        for _ in 0..qdcount {
            pos = Self::skip_name(data, pos)?;
            pos += 4; // Type and class
        }
        
        // Parse answers
        let mut answers = Vec::new();
        for _ in 0..ancount {
            let (record, new_pos) = Self::parse_record(data, pos)?;
            answers.push(record);
            pos = new_pos;
        }
        
        // Parse authority (simplified)
        let mut authority = Vec::new();
        for _ in 0..nscount {
            let (record, new_pos) = Self::parse_record(data, pos)?;
            authority.push(record);
            pos = new_pos;
        }
        
        // Parse additional (simplified)
        let mut additional = Vec::new();
        for _ in 0..arcount {
            if let Some((record, new_pos)) = Self::parse_record(data, pos) {
                additional.push(record);
                pos = new_pos;
            } else {
                break;
            }
        }
        
        Some(Self {
            id,
            rcode,
            answers,
            authority,
            additional,
            authoritative,
            recursion_available,
        })
    }
    
    /// Get IPv4 addresses
    pub fn ipv4_addresses(&self) -> Vec<Ipv4Addr> {
        self.answers
            .iter()
            .filter_map(|r| match &r.data {
                RecordData::A(addr) => Some(*addr),
                _ => None,
            })
            .collect()
    }
    
    /// Get IPv6 addresses
    pub fn ipv6_addresses(&self) -> Vec<Ipv6Addr> {
        self.answers
            .iter()
            .filter_map(|r| match &r.data {
                RecordData::AAAA(addr) => Some(*addr),
                _ => None,
            })
            .collect()
    }
    
    /// Get all IP addresses
    pub fn addresses(&self) -> Vec<IpAddr> {
        let mut addrs = Vec::new();
        for addr in self.ipv4_addresses() {
            addrs.push(IpAddr::V4(addr));
        }
        for addr in self.ipv6_addresses() {
            addrs.push(IpAddr::V6(addr));
        }
        addrs
    }
    
    fn skip_name(data: &[u8], mut pos: usize) -> Option<usize> {
        while pos < data.len() {
            let len = data[pos] as usize;
            if len == 0 {
                return Some(pos + 1);
            }
            if (len & 0xC0) == 0xC0 {
                // Pointer
                return Some(pos + 2);
            }
            pos += len + 1;
        }
        None
    }
    
    fn parse_name(data: &[u8], mut pos: usize) -> Option<(String, usize)> {
        let mut name = String::new();
        let mut jumped = false;
        let start = pos;
        
        while pos < data.len() {
            let len = data[pos] as usize;
            if len == 0 {
                if !jumped {
                    pos += 1;
                }
                break;
            }
            if (len & 0xC0) == 0xC0 {
                // Pointer
                if pos + 1 >= data.len() {
                    return None;
                }
                let offset = (((len & 0x3F) as usize) << 8) | (data[pos + 1] as usize);
                if !jumped {
                    pos += 2;
                    jumped = true;
                }
                let (suffix, _) = Self::parse_name(data, offset)?;
                if !name.is_empty() {
                    name.push('.');
                }
                name.push_str(&suffix);
                break;
            }
            
            if pos + 1 + len > data.len() {
                return None;
            }
            
            if !name.is_empty() {
                name.push('.');
            }
            name.push_str(std::str::from_utf8(&data[pos + 1..pos + 1 + len]).ok()?);
            pos += len + 1;
            
            if !jumped {
                // Advance position
            }
        }
        
        let final_pos = if jumped { start + 2 } else { pos };
        Some((name, final_pos))
    }
    
    fn parse_record(data: &[u8], pos: usize) -> Option<(DnsRecord, usize)> {
        let (name, mut pos) = Self::parse_name(data, pos)?;
        
        if pos + 10 > data.len() {
            return None;
        }
        
        let rtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let _class = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
        let ttl = u32::from_be_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]]);
        let rdlen = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
        pos += 10;
        
        if pos + rdlen > data.len() {
            return None;
        }
        
        let rdata = &data[pos..pos + rdlen];
        let record_type = RecordType::from_code(rtype).unwrap_or(RecordType::A);
        
        let record_data = match record_type {
            RecordType::A if rdlen == 4 => {
                RecordData::A(Ipv4Addr::new(rdata[0], rdata[1], rdata[2], rdata[3]))
            }
            RecordType::AAAA if rdlen == 16 => {
                let mut octets = [0u8; 16];
                octets.copy_from_slice(rdata);
                RecordData::AAAA(Ipv6Addr::from(octets))
            }
            RecordType::CNAME => {
                let (cname, _) = Self::parse_name(data, pos)?;
                RecordData::CNAME(cname)
            }
            _ => RecordData::Raw(rdata.to_vec()),
        };
        
        Some((
            DnsRecord {
                name,
                record_type,
                ttl,
                data: record_data,
            },
            pos + rdlen,
        ))
    }
}

/// DNS cache entry
#[derive(Debug, Clone)]
struct CacheEntry {
    /// Response
    response: DnsResponse,
    /// Inserted time
    inserted: Instant,
    /// TTL
    ttl: Duration,
}

/// DNS cache
#[derive(Debug, Default)]
pub struct DnsCache {
    /// Cache entries
    entries: HashMap<(String, RecordType), CacheEntry>,
    /// Statistics
    stats: DnsCacheStats,
}

/// Cache statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct DnsCacheStats {
    /// Lookups
    pub lookups: u64,
    /// Hits
    pub hits: u64,
    /// Misses
    pub misses: u64,
    /// Entries evicted
    pub evictions: u64,
}

impl DnsCache {
    /// Create new cache
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Store response
    pub fn store(&mut self, query: &DnsQuery, response: DnsResponse) {
        let min_ttl = response.answers
            .iter()
            .map(|r| r.ttl)
            .min()
            .unwrap_or(300);
        
        self.entries.insert(
            (query.name.clone(), query.record_type),
            CacheEntry {
                response,
                inserted: Instant::now(),
                ttl: Duration::from_secs(min_ttl as u64),
            },
        );
    }
    
    /// Lookup response
    pub fn lookup(&mut self, name: &str, record_type: RecordType) -> Option<&DnsResponse> {
        self.stats.lookups += 1;
        
        let key = (name.to_string(), record_type);
        
        if let Some(entry) = self.entries.get(&key) {
            if entry.inserted.elapsed() < entry.ttl {
                self.stats.hits += 1;
                return Some(&entry.response);
            }
            // Entry expired, will be cleaned up
        }
        
        self.stats.misses += 1;
        None
    }
    
    /// Evict expired entries
    pub fn evict_expired(&mut self) {
        let before = self.entries.len();
        self.entries.retain(|_, entry| entry.inserted.elapsed() < entry.ttl);
        self.stats.evictions += (before - self.entries.len()) as u64;
    }
    
    /// Clear cache
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    
    /// Get statistics
    pub fn stats(&self) -> &DnsCacheStats {
        &self.stats
    }
}

/// DoH client
#[derive(Debug)]
pub struct DohClient {
    /// Endpoint URL
    endpoint: String,
    /// DNS cache
    cache: DnsCache,
    /// Statistics
    stats: DohStats,
}

/// DoH statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct DohStats {
    /// Queries made
    pub queries: u64,
    /// Successful responses
    pub successes: u64,
    /// Failures
    pub failures: u64,
    /// Average latency (ms)
    pub avg_latency_ms: u64,
}

impl DohClient {
    /// Create new DoH client
    pub fn new(endpoint: &str) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            cache: DnsCache::new(),
            stats: DohStats::default(),
        }
    }
    
    /// Create with Cloudflare endpoint
    pub fn cloudflare() -> Self {
        Self::new(providers::CLOUDFLARE)
    }
    
    /// Create with Google endpoint
    pub fn google() -> Self {
        Self::new(providers::GOOGLE)
    }
    
    /// Build query bytes for DoH
    pub fn build_query(&self, query: &DnsQuery) -> Vec<u8> {
        query.encode()
    }
    
    /// Get endpoint URL
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }
    
    /// Get cache
    pub fn cache(&self) -> &DnsCache {
        &self.cache
    }
    
    /// Get mutable cache
    pub fn cache_mut(&mut self) -> &mut DnsCache {
        &mut self.cache
    }
    
    /// Get statistics
    pub fn stats(&self) -> &DohStats {
        &self.stats
    }
    
    /// Record successful query
    pub fn record_success(&mut self, latency_ms: u64) {
        self.stats.queries += 1;
        self.stats.successes += 1;
        
        // Update average latency
        let total = self.stats.avg_latency_ms * (self.stats.queries - 1) + latency_ms;
        self.stats.avg_latency_ms = total / self.stats.queries;
    }
    
    /// Record failed query
    pub fn record_failure(&mut self) {
        self.stats.queries += 1;
        self.stats.failures += 1;
    }
}

/// Generate random u16 for query ID
fn rand_u16() -> u16 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    
    let state = RandomState::new();
    let mut hasher = state.build_hasher();
    hasher.write_u64(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64);
    hasher.finish() as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dns_query_encode() {
        let query = DnsQuery::a("example.com");
        let encoded = query.encode();
        
        // Should have header (12 bytes) + question
        assert!(encoded.len() > 12);
        
        // Check flags (standard query, RD=1)
        assert_eq!(encoded[2], 0x01);
        assert_eq!(encoded[3], 0x00);
        
        // Check question count
        assert_eq!(u16::from_be_bytes([encoded[4], encoded[5]]), 1);
    }
    
    #[test]
    fn test_record_type_code() {
        assert_eq!(RecordType::A.code(), 1);
        assert_eq!(RecordType::AAAA.code(), 28);
        assert_eq!(RecordType::HTTPS.code(), 65);
        
        assert_eq!(RecordType::from_code(1), Some(RecordType::A));
        assert_eq!(RecordType::from_code(28), Some(RecordType::AAAA));
    }
    
    #[test]
    fn test_response_code() {
        assert!(ResponseCode::NoError.is_success());
        assert!(!ResponseCode::NXDomain.is_success());
        assert!(!ResponseCode::ServFail.is_success());
    }
    
    #[test]
    fn test_dns_cache() {
        let mut cache = DnsCache::new();
        
        let query = DnsQuery::a("example.com");
        let response = DnsResponse {
            id: 1,
            rcode: ResponseCode::NoError,
            answers: vec![DnsRecord {
                name: "example.com".into(),
                record_type: RecordType::A,
                ttl: 300,
                data: RecordData::A(Ipv4Addr::new(93, 184, 216, 34)),
            }],
            authority: vec![],
            additional: vec![],
            authoritative: false,
            recursion_available: true,
        };
        
        cache.store(&query, response);
        
        let cached = cache.lookup("example.com", RecordType::A);
        assert!(cached.is_some());
        assert!(!cached.unwrap().answers.is_empty());
    }
    
    #[test]
    fn test_doh_client() {
        let client = DohClient::cloudflare();
        assert!(client.endpoint().contains("cloudflare"));
        
        let query = DnsQuery::a("example.com");
        let encoded = client.build_query(&query);
        assert!(!encoded.is_empty());
    }
    
    #[test]
    fn test_providers() {
        assert!(providers::CLOUDFLARE.starts_with("https://"));
        assert!(providers::GOOGLE.starts_with("https://"));
        assert!(providers::QUAD9.starts_with("https://"));
    }
}
