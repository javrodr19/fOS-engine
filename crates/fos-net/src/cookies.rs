//! Cookie Handling
//!
//! Cookie jar with domain/path matching and StringInterner for memory efficiency.

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
    };
    
    // Parse attributes
    for part in parts {
        let part = part.trim();
        let lower = part.to_lowercase();
        
        if lower == "secure" {
            cookie.secure = true;
        } else if lower == "httponly" {
            cookie.http_only = true;
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
}
