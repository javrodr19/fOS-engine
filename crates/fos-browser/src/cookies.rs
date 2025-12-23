//! Cookie storage and persistence
//!
//! Manages browser cookies with persistence to disk.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// A browser cookie
#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    pub expires: Option<u64>, // Unix timestamp
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
}

/// SameSite cookie attribute
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SameSite {
    #[default]
    Lax,
    Strict,
    None,
}

impl Cookie {
    /// Create a new session cookie
    pub fn new(name: &str, value: &str, domain: &str) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            domain: domain.to_string(),
            path: "/".to_string(),
            expires: None,
            secure: false,
            http_only: false,
            same_site: SameSite::Lax,
        }
    }
    
    /// Check if cookie is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            expires < now
        } else {
            false // Session cookies don't expire
        }
    }
    
    /// Check if cookie matches a URL
    pub fn matches(&self, url: &url::Url) -> bool {
        // Check domain
        let host = url.host_str().unwrap_or("");
        if !self.domain_matches(host) {
            return false;
        }
        
        // Check path
        let path = url.path();
        if !path.starts_with(&self.path) {
            return false;
        }
        
        // Check secure flag
        if self.secure && url.scheme() != "https" {
            return false;
        }
        
        true
    }
    
    fn domain_matches(&self, host: &str) -> bool {
        if self.domain.starts_with('.') {
            // Wildcard domain
            host.ends_with(&self.domain[1..]) || host == &self.domain[1..]
        } else {
            host == self.domain
        }
    }
    
    /// Parse Set-Cookie header
    pub fn parse(header: &str, default_domain: &str) -> Option<Self> {
        let mut parts = header.split(';');
        let first = parts.next()?.trim();
        
        let (name, value) = first.split_once('=')?;
        let mut cookie = Cookie::new(name.trim(), value.trim(), default_domain);
        
        for part in parts {
            let part = part.trim();
            if let Some((attr, val)) = part.split_once('=') {
                match attr.to_lowercase().as_str() {
                    "domain" => cookie.domain = val.to_string(),
                    "path" => cookie.path = val.to_string(),
                    "expires" => {
                        // Parse HTTP date (simplified)
                        if let Ok(ts) = Self::parse_http_date(val) {
                            cookie.expires = Some(ts);
                        }
                    }
                    "max-age" => {
                        if let Ok(secs) = val.parse::<u64>() {
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .map(|d| d.as_secs())
                                .unwrap_or(0);
                            cookie.expires = Some(now + secs);
                        }
                    }
                    "samesite" => {
                        cookie.same_site = match val.to_lowercase().as_str() {
                            "strict" => SameSite::Strict,
                            "none" => SameSite::None,
                            _ => SameSite::Lax,
                        };
                    }
                    _ => {}
                }
            } else {
                match part.to_lowercase().as_str() {
                    "secure" => cookie.secure = true,
                    "httponly" => cookie.http_only = true,
                    _ => {}
                }
            }
        }
        
        Some(cookie)
    }
    
    fn parse_http_date(s: &str) -> Result<u64, ()> {
        // Very simplified HTTP date parsing
        // Real implementation would handle multiple formats
        // For now, just use current time + 1 year as fallback
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Ok(now + 365 * 24 * 60 * 60)
    }
    
    /// Serialize to Cookie header format
    pub fn to_header(&self) -> String {
        format!("{}={}", self.name, self.value)
    }
}

/// Cookie jar - stores and manages cookies
#[derive(Debug, Default)]
pub struct CookieJar {
    /// Cookies indexed by domain -> name
    cookies: HashMap<String, HashMap<String, Cookie>>,
    /// Storage path for persistence
    storage_path: Option<PathBuf>,
}

impl CookieJar {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create with persistence
    pub fn with_storage(path: PathBuf) -> Self {
        let mut jar = Self {
            cookies: HashMap::new(),
            storage_path: Some(path.clone()),
        };
        jar.load();
        jar
    }
    
    /// Add a cookie
    pub fn add(&mut self, cookie: Cookie) {
        if cookie.is_expired() {
            // Remove expired cookie
            self.remove(&cookie.domain, &cookie.name);
            return;
        }
        
        self.cookies
            .entry(cookie.domain.clone())
            .or_default()
            .insert(cookie.name.clone(), cookie);
    }
    
    /// Remove a cookie
    pub fn remove(&mut self, domain: &str, name: &str) {
        if let Some(domain_cookies) = self.cookies.get_mut(domain) {
            domain_cookies.remove(name);
        }
    }
    
    /// Get cookies for a URL
    pub fn get_for_url(&self, url: &url::Url) -> Vec<&Cookie> {
        let mut matching = Vec::new();
        
        for domain_cookies in self.cookies.values() {
            for cookie in domain_cookies.values() {
                if cookie.matches(url) && !cookie.is_expired() {
                    matching.push(cookie);
                }
            }
        }
        
        matching
    }
    
    /// Build Cookie header for a request
    pub fn cookie_header(&self, url: &url::Url) -> Option<String> {
        let cookies = self.get_for_url(url);
        if cookies.is_empty() {
            return None;
        }
        
        let header: String = cookies
            .iter()
            .map(|c| c.to_header())
            .collect::<Vec<_>>()
            .join("; ");
        
        Some(header)
    }
    
    /// Process Set-Cookie headers from a response
    pub fn process_set_cookies(&mut self, url: &url::Url, headers: &[(String, String)]) {
        let domain = url.host_str().unwrap_or("");
        
        for (name, value) in headers {
            if name.eq_ignore_ascii_case("set-cookie") {
                if let Some(cookie) = Cookie::parse(value, domain) {
                    self.add(cookie);
                }
            }
        }
    }
    
    /// Clear all cookies
    pub fn clear(&mut self) {
        self.cookies.clear();
    }
    
    /// Clear expired cookies
    pub fn clear_expired(&mut self) {
        for domain_cookies in self.cookies.values_mut() {
            domain_cookies.retain(|_, c| !c.is_expired());
        }
        self.cookies.retain(|_, v| !v.is_empty());
    }
    
    /// Save cookies to disk
    pub fn save(&self) {
        let Some(path) = &self.storage_path else { return };
        
        let mut data = String::new();
        for domain_cookies in self.cookies.values() {
            for cookie in domain_cookies.values() {
                // Only persist non-session cookies
                if cookie.expires.is_some() && !cookie.is_expired() {
                    data.push_str(&format!(
                        "{}\t{}\t{}\t{}\t{}\t{}\n",
                        cookie.domain,
                        cookie.path,
                        cookie.name,
                        cookie.value,
                        cookie.expires.unwrap_or(0),
                        if cookie.secure { "1" } else { "0" }
                    ));
                }
            }
        }
        
        let _ = fs::write(path, data);
    }
    
    /// Load cookies from disk
    pub fn load(&mut self) {
        let Some(path) = &self.storage_path else { return };
        
        let data = match fs::read_to_string(path) {
            Ok(d) => d,
            Err(_) => return,
        };
        
        for line in data.lines() {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 6 {
                let mut cookie = Cookie::new(parts[2], parts[3], parts[0]);
                cookie.path = parts[1].to_string();
                cookie.expires = parts[4].parse().ok();
                cookie.secure = parts[5] == "1";
                
                if !cookie.is_expired() {
                    self.add(cookie);
                }
            }
        }
    }
    
    /// Count cookies
    pub fn len(&self) -> usize {
        self.cookies.values().map(|m| m.len()).sum()
    }
    
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cookie_parse() {
        let cookie = Cookie::parse("session=abc123; Path=/; Secure; HttpOnly", "example.com").unwrap();
        assert_eq!(cookie.name, "session");
        assert_eq!(cookie.value, "abc123");
        assert!(cookie.secure);
        assert!(cookie.http_only);
    }
    
    #[test]
    fn test_cookie_jar() {
        let mut jar = CookieJar::new();
        let cookie = Cookie::new("test", "value", "example.com");
        jar.add(cookie);
        
        let url = url::Url::parse("https://example.com/page").unwrap();
        let cookies = jar.get_for_url(&url);
        assert_eq!(cookies.len(), 1);
        assert_eq!(cookies[0].name, "test");
    }
}
