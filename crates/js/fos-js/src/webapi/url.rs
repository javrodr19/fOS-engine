//! URL and URLSearchParams
//!
//! Web URL parsing and search params using custom RFC 3986 parser.

use fos_dom::url::{Url as CoreUrl, Query, percent_decode, percent_encode};

/// JavaScript URL
#[derive(Debug, Clone)]
pub struct JsUrl {
    inner: CoreUrl,
}

impl JsUrl {
    /// Parse a URL string
    pub fn parse(url: &str) -> Option<Self> {
        CoreUrl::parse(url).ok().map(|inner| Self { inner })
    }
    
    /// Get href
    pub fn href(&self) -> String {
        self.inner.to_string()
    }
    
    /// Get protocol (e.g., "https:")
    pub fn protocol(&self) -> String {
        format!("{}:", self.inner.scheme())
    }
    
    /// Get host (hostname:port)
    pub fn host(&self) -> String {
        self.inner.host_with_port()
    }
    
    /// Get hostname only
    pub fn hostname(&self) -> String {
        self.inner.host_str().unwrap_or("").to_string()
    }
    
    /// Get port as string
    pub fn port(&self) -> String {
        self.inner.port().map(|p| p.to_string()).unwrap_or_default()
    }
    
    /// Get pathname
    pub fn pathname(&self) -> String {
        self.inner.path().to_string()
    }
    
    /// Get search (with leading ?)
    pub fn search(&self) -> String {
        self.inner.query_params()
            .map(|q| format!("?{}", q.to_string()))
            .unwrap_or_default()
    }
    
    /// Get hash (with leading #)
    pub fn hash(&self) -> String {
        self.inner.fragment()
            .map(|f| format!("#{}", f))
            .unwrap_or_default()
    }
    
    /// Get username
    pub fn username(&self) -> String {
        self.inner.username().to_string()
    }
    
    /// Get password
    pub fn password(&self) -> String {
        self.inner.password().unwrap_or("").to_string()
    }
    
    /// Get origin
    pub fn origin(&self) -> String {
        self.inner.origin()
    }
    
    /// Get search params
    pub fn search_params(&self) -> JsUrlSearchParams {
        self.inner.query_params()
            .map(|q| JsUrlSearchParams::from_query(q.clone()))
            .unwrap_or_default()
    }
    
    /// Convert back to string
    pub fn to_string(&self) -> String {
        self.inner.to_string()
    }
}

/// URLSearchParams
#[derive(Debug, Clone, Default)]
pub struct JsUrlSearchParams {
    params: Vec<(String, String)>,
}

impl JsUrlSearchParams {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn from_query(query: Query) -> Self {
        let params = query.entries()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        Self { params }
    }
    
    pub fn parse(search: &str) -> Self {
        let query = search.strip_prefix('?').unwrap_or(search);
        let params = query.split('&')
            .filter(|s| !s.is_empty())
            .filter_map(|pair| {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                Some((percent_decode(k), percent_decode(v)))
            })
            .collect();
        Self { params }
    }
    
    pub fn get(&self, name: &str) -> Option<&str> {
        self.params.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
    
    pub fn get_all(&self, name: &str) -> Vec<&str> {
        self.params.iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
            .collect()
    }
    
    pub fn set(&mut self, name: &str, value: &str) {
        self.delete(name);
        self.append(name, value);
    }
    
    pub fn append(&mut self, name: &str, value: &str) {
        self.params.push((name.to_string(), value.to_string()));
    }
    
    pub fn delete(&mut self, name: &str) {
        self.params.retain(|(k, _)| k != name);
    }
    
    pub fn has(&self, name: &str) -> bool {
        self.params.iter().any(|(k, _)| k == name)
    }
    
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.params.iter().map(|(k, _)| k.as_str())
    }
    
    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.params.iter().map(|(_, v)| v.as_str())
    }
    
    pub fn to_string(&self) -> String {
        self.params.iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_url_parse() {
        let url = JsUrl::parse("https://example.com:8080/path?q=1#hash").unwrap();
        
        assert_eq!(url.protocol(), "https:");
        assert_eq!(url.hostname(), "example.com");
        assert_eq!(url.port(), "8080");
        assert_eq!(url.pathname(), "/path");
        assert_eq!(url.search(), "?q=1");
        assert_eq!(url.hash(), "#hash");
    }
    
    #[test]
    fn test_search_params() {
        let params = JsUrlSearchParams::parse("?foo=bar&baz=qux");
        
        assert_eq!(params.get("foo"), Some("bar"));
        assert_eq!(params.get("baz"), Some("qux"));
    }
}
