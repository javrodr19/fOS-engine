//! Cookie API
//!
//! HTTP cookie parsing and storage.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Cookie store
#[derive(Debug, Default)]
pub struct CookieStore {
    cookies: HashMap<String, Cookie>,
}

/// HTTP Cookie
#[derive(Debug, Clone)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    pub domain: Option<String>,
    pub path: String,
    pub expires: Option<SystemTime>,
    pub max_age: Option<Duration>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
}

/// SameSite attribute
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SameSite {
    #[default]
    Lax,
    Strict,
    None,
}

impl Cookie {
    /// Create a simple cookie
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            name: name.to_string(),
            value: value.to_string(),
            domain: None,
            path: "/".to_string(),
            expires: None,
            max_age: None,
            secure: false,
            http_only: false,
            same_site: SameSite::default(),
        }
    }
    
    /// Parse from Set-Cookie header
    pub fn parse(header: &str) -> Option<Self> {
        let mut parts = header.split(';');
        let first = parts.next()?.trim();
        let (name, value) = first.split_once('=')?;
        
        let mut cookie = Self::new(name.trim(), value.trim());
        
        for part in parts {
            let part = part.trim();
            if let Some((key, val)) = part.split_once('=') {
                match key.to_lowercase().as_str() {
                    "domain" => cookie.domain = Some(val.to_string()),
                    "path" => cookie.path = val.to_string(),
                    "max-age" => {
                        if let Ok(secs) = val.parse::<u64>() {
                            cookie.max_age = Some(Duration::from_secs(secs));
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
    
    /// Convert to header string
    pub fn to_header(&self) -> String {
        let mut s = format!("{}={}", self.name, self.value);
        
        if let Some(ref domain) = self.domain {
            s.push_str(&format!("; Domain={}", domain));
        }
        s.push_str(&format!("; Path={}", self.path));
        
        if let Some(max_age) = self.max_age {
            s.push_str(&format!("; Max-Age={}", max_age.as_secs()));
        }
        
        if self.secure {
            s.push_str("; Secure");
        }
        if self.http_only {
            s.push_str("; HttpOnly");
        }
        
        match self.same_site {
            SameSite::Strict => s.push_str("; SameSite=Strict"),
            SameSite::None => s.push_str("; SameSite=None"),
            SameSite::Lax => s.push_str("; SameSite=Lax"),
        }
        
        s
    }
    
    /// Check if cookie is expired
    pub fn is_expired(&self) -> bool {
        if let Some(exp) = self.expires {
            return exp < SystemTime::now();
        }
        false
    }
}

impl CookieStore {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set a cookie
    pub fn set(&mut self, cookie: Cookie) {
        self.cookies.insert(cookie.name.clone(), cookie);
    }
    
    /// Get a cookie by name
    pub fn get(&self, name: &str) -> Option<&Cookie> {
        self.cookies.get(name)
    }
    
    /// Delete a cookie
    pub fn delete(&mut self, name: &str) -> bool {
        self.cookies.remove(name).is_some()
    }
    
    /// Get all cookies for a URL
    pub fn get_for_url(&self, url: &str, secure: bool) -> Vec<&Cookie> {
        self.cookies.values()
            .filter(|c| {
                if c.secure && !secure {
                    return false;
                }
                if c.is_expired() {
                    return false;
                }
                true
            })
            .collect()
    }
    
    /// Build Cookie header value
    pub fn to_cookie_header(&self, url: &str, secure: bool) -> String {
        self.get_for_url(url, secure)
            .iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ")
    }
    
    /// Clear all cookies
    pub fn clear(&mut self) {
        self.cookies.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cookie_parse() {
        let cookie = Cookie::parse("session=abc123; Path=/; HttpOnly; Secure").unwrap();
        
        assert_eq!(cookie.name, "session");
        assert_eq!(cookie.value, "abc123");
        assert!(cookie.http_only);
        assert!(cookie.secure);
    }
    
    #[test]
    fn test_cookie_store() {
        let mut store = CookieStore::new();
        store.set(Cookie::new("user", "alice"));
        
        assert!(store.get("user").is_some());
        assert_eq!(store.get("user").unwrap().value, "alice");
    }
}
