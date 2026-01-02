//! Cookie Handling
//!
//! Cookie jar with domain/path matching, StringInterner for memory efficiency,
//! and cookie partitioning (CHIPS) for cross-site tracking prevention.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Cookie attributes
#[derive(Debug, Clone)]
pub struct Cookie {
    /// Cookie name
    pub name: String,
    /// Cookie value  
    pub value: String,
    /// Domain (for matching)
    pub domain: String,
    /// Path (for matching)
    pub path: String,
    /// Expiry time (None = session cookie)
    pub expires: Option<u64>,
    /// Secure flag (HTTPS only)
    pub secure: bool,
    /// HttpOnly flag (no JS access)
    pub http_only: bool,
    /// SameSite attribute
    pub same_site: SameSite,
    /// Partitioned cookie flag (CHIPS)
    pub partitioned: bool,
}

/// SameSite attribute values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SameSite {
    /// Cookie sent with all requests
    None,
    /// Cookie not sent with cross-origin requests
    #[default]
    Lax,
    /// Cookie only sent with same-site requests
    Strict,
}

impl Cookie {
    /// Create a simple session cookie
    pub fn new(name: &str, value: &str, domain: &str, path: &str) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            domain: domain.to_string(),
            path: path.to_string(),
            expires: None,
            secure: false,
            http_only: false,
            same_site: SameSite::Lax,
            partitioned: false,
        }
    }
    
    /// Create a partitioned cookie
    pub fn new_partitioned(name: &str, value: &str, domain: &str, path: &str) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            domain: domain.to_string(),
            path: path.to_string(),
            expires: None,
            secure: true, // Partitioned cookies must be Secure
            http_only: false,
            same_site: SameSite::None, // Partitioned cookies typically use SameSite=None
            partitioned: true,
        }
    }
    
    /// Check if cookie has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs();
            expires < now
        } else {
            false
        }
    }
    
    /// Check if cookie matches the given domain
    pub fn matches_domain(&self, domain: &str) -> bool {
        if self.domain == domain {
            return true;
        }
        
        // Domain matching: .example.com matches foo.example.com
        if self.domain.starts_with('.') {
            domain.ends_with(&self.domain) || 
                format!(".{}", domain) == self.domain
        } else {
            false
        }
    }
    
    /// Check if cookie matches the given path
    pub fn matches_path(&self, path: &str) -> bool {
        if self.path == "/" {
            return true;
        }
        
        path.starts_with(&self.path)
    }
    
    /// Check if cookie should be sent for request
    pub fn matches(&self, domain: &str, path: &str, is_secure: bool) -> bool {
        if self.is_expired() {
            return false;
        }
        
        if self.secure && !is_secure {
            return false;
        }
        
        self.matches_domain(domain) && self.matches_path(path)
    }
    
    /// Serialize to Cookie header format (name=value)
    pub fn serialize(&self) -> String {
        format!("{}={}", self.name, self.value)
    }
}

/// Cookie partition key based on top-level site
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PartitionKey {
    /// Top-level site (scheme + eTLD+1)
    pub top_level_site: String,
}

impl PartitionKey {
    /// Create partition key from top-level site
    pub fn new(top_level_site: &str) -> Self {
        Self {
            top_level_site: top_level_site.to_lowercase(),
        }
    }
    
    /// Create partition key from URL
    pub fn from_url(url: &str) -> Option<Self> {
        // Extract scheme and host
        let url = url.trim();
        let scheme_end = url.find("://")?;
        let scheme = &url[..scheme_end];
        let rest = &url[scheme_end + 3..];
        let path_start = rest.find('/').unwrap_or(rest.len());
        let host = &rest[..path_start].to_lowercase();
        
        // Get eTLD+1 (simplified: get last two components or fewer)
        let etld_plus_1 = Self::get_etld_plus_1(host);
        
        Some(Self {
            top_level_site: format!("{}://{}", scheme, etld_plus_1),
        })
    }
    
    /// Extract eTLD+1 from host (simplified implementation)
    fn get_etld_plus_1(host: &str) -> String {
        let parts: Vec<&str> = host.split('.').collect();
        
        // Handle IP addresses  
        if parts.iter().all(|p| p.parse::<u8>().is_ok()) {
            return host.to_string();
        }
        
        // Simple eTLD+1 extraction (last 2 parts)
        // Production would use public suffix list
        if parts.len() >= 2 {
            parts[parts.len() - 2..].join(".")
        } else {
            host.to_string()
        }
    }
}

/// Parse a Set-Cookie header
pub fn parse_set_cookie(header: &str, request_domain: &str) -> Option<Cookie> {
    let mut parts = header.split(';');
    
    // First part is name=value
    let name_value = parts.next()?.trim();
    let eq_pos = name_value.find('=')?;
    let name = name_value[..eq_pos].trim().to_string();
    let value = name_value[eq_pos + 1..].trim().to_string();
    
    let mut cookie = Cookie {
        name,
        value,
        domain: request_domain.to_string(),
        path: "/".to_string(),
        expires: None,
        secure: false,
        http_only: false,
        same_site: SameSite::Lax,
        partitioned: false,
    };
    
    // Parse attributes
    for part in parts {
        let part = part.trim();
        let lower = part.to_lowercase();
        
        if lower == "secure" {
            cookie.secure = true;
        } else if lower == "httponly" {
            cookie.http_only = true;
        } else if lower == "partitioned" {
            cookie.partitioned = true;
            // Partitioned cookies must be Secure
            cookie.secure = true;
        } else if let Some(value) = part.strip_prefix("Domain=").or_else(|| part.strip_prefix("domain=")) {
            cookie.domain = value.trim().to_string();
        } else if let Some(value) = part.strip_prefix("Path=").or_else(|| part.strip_prefix("path=")) {
            cookie.path = value.trim().to_string();
        } else if let Some(value) = part.strip_prefix("Max-Age=").or_else(|| part.strip_prefix("max-age=")) {
            if let Ok(seconds) = value.trim().parse::<u64>() {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO)
                    .as_secs();
                cookie.expires = Some(now + seconds);
            }
        } else if let Some(value) = part.strip_prefix("SameSite=").or_else(|| part.strip_prefix("samesite=")) {
            cookie.same_site = match value.trim().to_lowercase().as_str() {
                "strict" => SameSite::Strict,
                "none" => SameSite::None,
                _ => SameSite::Lax,
            };
        }
    }
    
    Some(cookie)
}

/// Cookie jar for storing cookies
#[derive(Debug, Default)]
pub struct CookieJar {
    /// Cookies indexed by domain
    cookies: HashMap<String, Vec<Cookie>>,
}

impl CookieJar {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add a cookie to the jar
    pub fn add(&mut self, cookie: Cookie) {
        let domain = cookie.domain.clone();
        let cookies = self.cookies.entry(domain).or_default();
        
        // Remove existing cookie with same name/path
        cookies.retain(|c| !(c.name == cookie.name && c.path == cookie.path));
        
        // Only add if not already expired
        if !cookie.is_expired() {
            cookies.push(cookie);
        }
    }
    
    /// Add from Set-Cookie header
    pub fn add_from_header(&mut self, header: &str, request_domain: &str) {
        if let Some(cookie) = parse_set_cookie(header, request_domain) {
            self.add(cookie);
        }
    }
    
    /// Get cookies for a request
    pub fn get_cookies(&self, domain: &str, path: &str, is_secure: bool) -> Vec<&Cookie> {
        let mut result = Vec::new();
        
        for (_, cookies) in &self.cookies {
            for cookie in cookies {
                if cookie.matches(domain, path, is_secure) {
                    result.push(cookie);
                }
            }
        }
        
        result
    }
    
    /// Get Cookie header value for request
    pub fn get_cookie_header(&self, domain: &str, path: &str, is_secure: bool) -> Option<String> {
        let cookies = self.get_cookies(domain, path, is_secure);
        
        if cookies.is_empty() {
            None
        } else {
            Some(cookies.iter()
                .map(|c| c.serialize())
                .collect::<Vec<_>>()
                .join("; "))
        }
    }
    
    /// Remove expired cookies
    pub fn cleanup(&mut self) {
        for cookies in self.cookies.values_mut() {
            cookies.retain(|c| !c.is_expired());
        }
        
        // Remove empty domain entries
        self.cookies.retain(|_, v| !v.is_empty());
    }
    
    /// Clear all cookies
    pub fn clear(&mut self) {
        self.cookies.clear();
    }
    
    /// Clear cookies for a specific domain
    pub fn clear_domain(&mut self, domain: &str) {
        self.cookies.remove(domain);
    }
    
    /// Get total cookie count
    pub fn len(&self) -> usize {
        self.cookies.values().map(|v| v.len()).sum()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Partitioned cookie jar for CHIPS (Cookies Having Independent Partitioned State)
/// 
/// Stores cookies partitioned by top-level site to prevent cross-site tracking.
#[derive(Debug, Default)]
pub struct PartitionedCookieJar {
    /// Unpartitioned (first-party) cookies
    unpartitioned: CookieJar,
    /// Partitioned cookies by top-level site
    partitioned: HashMap<PartitionKey, CookieJar>,
}

impl PartitionedCookieJar {
    /// Create new partitioned cookie jar
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add cookie with partition context
    pub fn add(&mut self, cookie: Cookie, partition_key: Option<&PartitionKey>) {
        if cookie.partitioned {
            // Partitioned cookies require a partition key
            if let Some(key) = partition_key {
                let jar = self.partitioned.entry(key.clone()).or_default();
                jar.add(cookie);
            }
            // Silently ignore partitioned cookies without partition key
        } else {
            self.unpartitioned.add(cookie);
        }
    }
    
    /// Add from Set-Cookie header with partition context
    pub fn add_from_header(
        &mut self,
        header: &str,
        request_domain: &str,
        partition_key: Option<&PartitionKey>,
    ) {
        if let Some(cookie) = parse_set_cookie(header, request_domain) {
            self.add(cookie, partition_key);
        }
    }
    
    /// Get cookies for a request with partition context
    pub fn get_cookies(
        &self,
        domain: &str,
        path: &str,
        is_secure: bool,
        partition_key: Option<&PartitionKey>,
    ) -> Vec<&Cookie> {
        let mut result = self.unpartitioned.get_cookies(domain, path, is_secure);
        
        // Add partitioned cookies for this partition
        if let Some(key) = partition_key {
            if let Some(jar) = self.partitioned.get(key) {
                result.extend(jar.get_cookies(domain, path, is_secure));
            }
        }
        
        result
    }
    
    /// Get Cookie header for request
    pub fn get_cookie_header(
        &self,
        domain: &str,
        path: &str,
        is_secure: bool,
        partition_key: Option<&PartitionKey>,
    ) -> Option<String> {
        let cookies = self.get_cookies(domain, path, is_secure, partition_key);
        
        if cookies.is_empty() {
            None
        } else {
            Some(cookies.iter()
                .map(|c| c.serialize())
                .collect::<Vec<_>>()
                .join("; "))
        }
    }
    
    /// Get unpartitioned cookie jar
    pub fn unpartitioned(&self) -> &CookieJar {
        &self.unpartitioned
    }
    
    /// Get unpartitioned cookie jar (mutable)
    pub fn unpartitioned_mut(&mut self) -> &mut CookieJar {
        &mut self.unpartitioned
    }
    
    /// Get partitioned jar for a specific partition
    pub fn partition(&self, key: &PartitionKey) -> Option<&CookieJar> {
        self.partitioned.get(key)
    }
    
    /// Cleanup expired cookies in all jars
    pub fn cleanup(&mut self) {
        self.unpartitioned.cleanup();
        
        for jar in self.partitioned.values_mut() {
            jar.cleanup();
        }
        
        // Remove empty partitions
        self.partitioned.retain(|_, jar| !jar.is_empty());
    }
    
    /// Clear all cookies
    pub fn clear(&mut self) {
        self.unpartitioned.clear();
        self.partitioned.clear();
    }
    
    /// Clear cookies for a specific partition
    pub fn clear_partition(&mut self, key: &PartitionKey) {
        self.partitioned.remove(key);
    }
    
    /// Get total cookie count
    pub fn len(&self) -> usize {
        self.unpartitioned.len() + 
            self.partitioned.values().map(|j| j.len()).sum::<usize>()
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    
    /// Get number of partitions
    pub fn partition_count(&self) -> usize {
        self.partitioned.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cookie_parse() {
        let header = "session=abc123; Path=/; HttpOnly; Secure";
        let cookie = parse_set_cookie(header, "example.com").unwrap();
        
        assert_eq!(cookie.name, "session");
        assert_eq!(cookie.value, "abc123");
        assert!(cookie.http_only);
        assert!(cookie.secure);
        assert!(!cookie.partitioned);
    }
    
    #[test]
    fn test_partitioned_cookie_parse() {
        let header = "tracking=x; Path=/; Secure; SameSite=None; Partitioned";
        let cookie = parse_set_cookie(header, "tracker.com").unwrap();
        
        assert_eq!(cookie.name, "tracking");
        assert!(cookie.partitioned);
        assert!(cookie.secure); // Partitioned requires Secure
        assert_eq!(cookie.same_site, SameSite::None);
    }
    
    #[test]
    fn test_cookie_domain_match() {
        let cookie = Cookie::new("test", "value", ".example.com", "/");
        
        assert!(cookie.matches_domain("example.com"));
        assert!(cookie.matches_domain("foo.example.com"));
        assert!(!cookie.matches_domain("other.com"));
    }
    
    #[test]
    fn test_cookie_jar() {
        let mut jar = CookieJar::new();
        
        jar.add_from_header("session=abc123; Path=/", "example.com");
        jar.add_from_header("user=john; Path=/api", "example.com");
        
        let header = jar.get_cookie_header("example.com", "/api/test", false);
        assert!(header.is_some());
        
        let header = header.unwrap();
        assert!(header.contains("session=abc123"));
        assert!(header.contains("user=john"));
    }
    
    #[test]
    fn test_cookie_secure() {
        let mut jar = CookieJar::new();
        jar.add_from_header("secure_cookie=value; Secure", "example.com");
        
        // Secure cookie not sent on HTTP
        assert!(jar.get_cookies("example.com", "/", false).is_empty());
        
        // Secure cookie sent on HTTPS
        assert_eq!(jar.get_cookies("example.com", "/", true).len(), 1);
    }
    
    #[test]
    fn test_partition_key() {
        let key = PartitionKey::from_url("https://www.example.com/path").unwrap();
        assert_eq!(key.top_level_site, "https://example.com");
        
        let key2 = PartitionKey::from_url("https://sub.example.com/").unwrap();
        assert_eq!(key.top_level_site, key2.top_level_site);
    }
    
    #[test]
    fn test_partitioned_cookie_jar() {
        let mut jar = PartitionedCookieJar::new();
        
        // Add first-party cookie
        jar.add_from_header("session=abc", "example.com", None);
        
        // Add partitioned cookie for a third-party embed
        let partition = PartitionKey::new("https://toplevel.com");
        jar.add_from_header(
            "widget=xyz; Secure; Partitioned",
            "embed.com",
            Some(&partition),
        );
        
        // Without partition context, only see first-party
        let cookies = jar.get_cookies("example.com", "/", true, None);
        assert_eq!(cookies.len(), 1);
        
        // With partition context, see both (for their domains)
        // Note: embed.com cookie only matches embed.com domain
        let cookies = jar.get_cookies("embed.com", "/", true, Some(&partition));
        assert_eq!(cookies.len(), 1);
        assert!(cookies[0].partitioned);
    }
    
    #[test]
    fn test_partitioned_isolation() {
        let mut jar = PartitionedCookieJar::new();
        
        let partition_a = PartitionKey::new("https://site-a.com");
        let partition_b = PartitionKey::new("https://site-b.com");
        
        // Same embed on two different sites
        jar.add_from_header("id=A; Secure; Partitioned", "tracker.com", Some(&partition_a));
        jar.add_from_header("id=B; Secure; Partitioned", "tracker.com", Some(&partition_b));
        
        // Site A only sees Site A's cookie
        let cookies = jar.get_cookies("tracker.com", "/", true, Some(&partition_a));
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].value, "A");
        
        // Site B only sees Site B's cookie
        let cookies = jar.get_cookies("tracker.com", "/", true, Some(&partition_b));
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].value, "B");
    }
}
