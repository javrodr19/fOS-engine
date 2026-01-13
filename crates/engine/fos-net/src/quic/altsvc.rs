//! Alt-Svc Header Parsing
//!
//! Parse Alt-Svc headers for HTTP/3 discovery per RFC 7838.

use std::collections::HashMap;
use std::time::Duration;

/// An alternative service entry
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltSvcEntry {
    /// Protocol identifier (e.g., "h3", "h3-29", "h2")
    pub protocol: String,
    /// Host (empty means same as origin)
    pub host: String,
    /// Port
    pub port: u16,
    /// Max-age in seconds
    pub max_age: Duration,
    /// Whether to persist across network changes
    pub persist: bool,
}

impl AltSvcEntry {
    /// Check if this entry is for HTTP/3
    pub fn is_h3(&self) -> bool {
        self.protocol == "h3" || self.protocol.starts_with("h3-")
    }
    
    /// Check if this entry is for HTTP/2
    pub fn is_h2(&self) -> bool {
        self.protocol == "h2" || self.protocol == "h2c"
    }
    
    /// Get effective host (falls back to origin if empty)
    pub fn effective_host<'a>(&'a self, origin_host: &'a str) -> &'a str {
        if self.host.is_empty() {
            origin_host
        } else {
            &self.host
        }
    }
}

/// Parsed Alt-Svc header
#[derive(Debug, Clone, Default)]
pub struct AltSvc {
    /// Alternative service entries
    pub entries: Vec<AltSvcEntry>,
    /// Whether this clears all alternatives
    pub clear: bool,
}

impl AltSvc {
    /// Create empty Alt-Svc
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            clear: false,
        }
    }
    
    /// Create a "clear" Alt-Svc (clears all cached alternatives)
    pub fn clear() -> Self {
        Self {
            entries: Vec::new(),
            clear: true,
        }
    }
    
    /// Parse Alt-Svc header value
    pub fn parse(header_value: &str) -> Option<Self> {
        let value = header_value.trim();
        
        // Check for "clear"
        if value == "clear" {
            return Some(Self::clear());
        }
        
        let mut entries = Vec::new();
        
        // Split by comma (but respect quoted strings)
        for entry in split_entries(value) {
            if let Some(parsed) = parse_entry(entry.trim()) {
                entries.push(parsed);
            }
        }
        
        if entries.is_empty() {
            return None;
        }
        
        Some(Self {
            entries,
            clear: false,
        })
    }
    
    /// Get the best HTTP/3 entry
    pub fn best_h3(&self) -> Option<&AltSvcEntry> {
        // Prefer h3 over h3-XX drafts
        self.entries.iter()
            .find(|e| e.protocol == "h3")
            .or_else(|| self.entries.iter().find(|e| e.protocol.starts_with("h3-")))
    }
    
    /// Get all HTTP/3 entries
    pub fn h3_entries(&self) -> impl Iterator<Item = &AltSvcEntry> {
        self.entries.iter().filter(|e| e.is_h3())
    }
    
    /// Check if any HTTP/3 alternatives are available
    pub fn has_h3(&self) -> bool {
        self.entries.iter().any(|e| e.is_h3())
    }
}

/// Split entries respecting quoted strings
fn split_entries(value: &str) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut start = 0;
    let mut in_quotes = false;
    
    for (i, c) in value.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                if i > start {
                    entries.push(&value[start..i]);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    
    if start < value.len() {
        entries.push(&value[start..]);
    }
    
    entries
}

/// Parse a single Alt-Svc entry
fn parse_entry(entry: &str) -> Option<AltSvcEntry> {
    // Format: protocol="host:port"; ma=seconds; persist=1
    let mut parts = entry.splitn(2, '=');
    let protocol = parts.next()?.trim();
    let rest = parts.next()?.trim();
    
    // Parse authority (host:port in quotes)
    let (authority, params) = if rest.starts_with('"') {
        let end_quote = rest[1..].find('"')?;
        let authority = &rest[1..end_quote + 1];
        let params = if rest.len() > end_quote + 2 {
            &rest[end_quote + 2..]
        } else {
            ""
        };
        (authority, params)
    } else {
        // Some servers omit quotes
        let end = rest.find(';').unwrap_or(rest.len());
        (&rest[..end], rest.get(end..)?.trim_start_matches(';'))
    };
    
    // Parse host:port
    let (host, port) = parse_authority(authority)?;
    
    // Parse parameters
    let params = parse_params(params);
    
    let max_age = params
        .get("ma")
        .and_then(|v| v.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(86400)); // Default 24h
    
    let persist = params
        .get("persist")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);
    
    Some(AltSvcEntry {
        protocol: protocol.to_string(),
        host: host.to_string(),
        port,
        max_age,
        persist,
    })
}

/// Parse authority (host:port)
fn parse_authority(authority: &str) -> Option<(&str, u16)> {
    if let Some((host, port_str)) = authority.rsplit_once(':') {
        let port = port_str.parse().ok()?;
        Some((host, port))
    } else {
        // Port is required
        None
    }
}

/// Parse parameters (semicolon-separated key=value pairs)
fn parse_params(params: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    
    for param in params.split(';') {
        let param = param.trim();
        if param.is_empty() {
            continue;
        }
        
        if let Some((key, value)) = param.split_once('=') {
            map.insert(key.trim().to_lowercase(), value.trim().to_string());
        }
    }
    
    map
}

/// Alt-Svc cache for storing discovered alternatives
#[derive(Debug, Clone, Default)]
pub struct AltSvcCache {
    /// Cached entries by origin
    entries: HashMap<String, CachedAltSvc>,
}

/// Cached Alt-Svc with expiration
#[derive(Debug, Clone)]
struct CachedAltSvc {
    /// The parsed Alt-Svc
    alt_svc: AltSvc,
    /// When this entry was cached
    cached_at: std::time::Instant,
}

impl AltSvcCache {
    /// Create a new cache
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }
    
    /// Store Alt-Svc for an origin
    pub fn insert(&mut self, origin: &str, alt_svc: AltSvc) {
        if alt_svc.clear {
            self.entries.remove(origin);
        } else {
            self.entries.insert(origin.to_string(), CachedAltSvc {
                alt_svc,
                cached_at: std::time::Instant::now(),
            });
        }
    }
    
    /// Get Alt-Svc for an origin
    pub fn get(&self, origin: &str) -> Option<&AltSvc> {
        let cached = self.entries.get(origin)?;
        
        // Check if any entry is still valid
        let now = std::time::Instant::now();
        let age = now.duration_since(cached.cached_at);
        
        // Check if at least one entry is still valid
        if cached.alt_svc.entries.iter().any(|e| age < e.max_age) {
            Some(&cached.alt_svc)
        } else {
            None
        }
    }
    
    /// Get best HTTP/3 alternative for an origin
    pub fn get_h3(&self, origin: &str) -> Option<&AltSvcEntry> {
        self.get(origin)?.best_h3()
    }
    
    /// Clear all cached entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }
    
    /// Remove expired entries
    pub fn cleanup(&mut self) {
        let now = std::time::Instant::now();
        self.entries.retain(|_, cached| {
            let age = now.duration_since(cached.cached_at);
            cached.alt_svc.entries.iter().any(|e| age < e.max_age)
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple() {
        let alt_svc = AltSvc::parse(r#"h3=":443""#).unwrap();
        assert!(!alt_svc.clear);
        assert_eq!(alt_svc.entries.len(), 1);
        
        let entry = &alt_svc.entries[0];
        assert_eq!(entry.protocol, "h3");
        assert_eq!(entry.host, "");
        assert_eq!(entry.port, 443);
    }
    
    #[test]
    fn test_parse_with_host() {
        let alt_svc = AltSvc::parse(r#"h3="alt.example.com:8443""#).unwrap();
        let entry = &alt_svc.entries[0];
        assert_eq!(entry.host, "alt.example.com");
        assert_eq!(entry.port, 8443);
    }
    
    #[test]
    fn test_parse_with_params() {
        let alt_svc = AltSvc::parse(r#"h3=":443"; ma=3600; persist=1"#).unwrap();
        let entry = &alt_svc.entries[0];
        assert_eq!(entry.max_age, Duration::from_secs(3600));
        assert!(entry.persist);
    }
    
    #[test]
    fn test_parse_multiple() {
        let alt_svc = AltSvc::parse(r#"h3=":443", h2=":443""#).unwrap();
        assert_eq!(alt_svc.entries.len(), 2);
        assert!(alt_svc.has_h3());
    }
    
    #[test]
    fn test_parse_clear() {
        let alt_svc = AltSvc::parse("clear").unwrap();
        assert!(alt_svc.clear);
        assert!(alt_svc.entries.is_empty());
    }
    
    #[test]
    fn test_best_h3() {
        let alt_svc = AltSvc::parse(r#"h3-29=":443", h3=":443", h2=":443""#).unwrap();
        let best = alt_svc.best_h3().unwrap();
        assert_eq!(best.protocol, "h3"); // Prefer h3 over h3-XX
    }
    
    #[test]
    fn test_is_h3() {
        let entry = AltSvcEntry {
            protocol: "h3".to_string(),
            host: String::new(),
            port: 443,
            max_age: Duration::from_secs(3600),
            persist: false,
        };
        assert!(entry.is_h3());
        assert!(!entry.is_h2());
    }
    
    #[test]
    fn test_cache() {
        let mut cache = AltSvcCache::new();
        let alt_svc = AltSvc::parse(r#"h3=":443""#).unwrap();
        
        cache.insert("example.com", alt_svc);
        
        let cached = cache.get("example.com").unwrap();
        assert!(cached.has_h3());
        
        let h3 = cache.get_h3("example.com").unwrap();
        assert_eq!(h3.port, 443);
    }
}
