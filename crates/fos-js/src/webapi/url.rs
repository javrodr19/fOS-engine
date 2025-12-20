//! URL and URLSearchParams
//!
//! Web URL parsing and search params.

/// JavaScript URL
#[derive(Debug, Clone)]
pub struct JsUrl {
    pub href: String,
    pub protocol: String,
    pub host: String,
    pub hostname: String,
    pub port: String,
    pub pathname: String,
    pub search: String,
    pub hash: String,
    pub username: String,
    pub password: String,
    pub origin: String,
    pub search_params: JsUrlSearchParams,
}

impl JsUrl {
    /// Parse a URL string
    pub fn parse(url: &str) -> Option<Self> {
        // Simple URL parsing
        let href = url.to_string();
        
        // Extract protocol
        let (protocol, rest) = url.split_once("://")
            .map(|(p, r)| (format!("{}:", p), r))
            .unwrap_or_default();
        
        // Extract hash
        let (rest, hash) = rest.rsplit_once('#')
            .map(|(r, h)| (r, format!("#{}", h)))
            .unwrap_or((rest, String::new()));
        
        // Extract search
        let (rest, search) = rest.split_once('?')
            .map(|(r, s)| (r, format!("?{}", s)))
            .unwrap_or((rest, String::new()));
        
        // Extract auth
        let (auth, rest) = if rest.contains('@') {
            rest.split_once('@').unwrap_or(("", rest))
        } else {
            ("", rest)
        };
        
        let (username, password) = auth.split_once(':')
            .map(|(u, p)| (u.to_string(), p.to_string()))
            .unwrap_or_default();
        
        // Extract host and path
        let (host_port, pathname) = rest.split_once('/')
            .map(|(h, p)| (h, format!("/{}", p)))
            .unwrap_or((rest, "/".to_string()));
        
        let (hostname, port) = host_port.split_once(':')
            .map(|(h, p)| (h.to_string(), p.to_string()))
            .unwrap_or((host_port.to_string(), String::new()));
        
        let host = if port.is_empty() {
            hostname.clone()
        } else {
            format!("{}:{}", hostname, port)
        };
        
        let origin = format!("{}//{}", protocol, host);
        
        let search_params = JsUrlSearchParams::parse(&search);
        
        Some(Self {
            href,
            protocol,
            host,
            hostname,
            port,
            pathname,
            search,
            hash,
            username,
            password,
            origin,
            search_params,
        })
    }
    
    /// Convert back to string
    pub fn to_string(&self) -> String {
        let mut url = format!("{}//{}", self.protocol, self.host);
        url.push_str(&self.pathname);
        url.push_str(&self.search);
        url.push_str(&self.hash);
        url
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
    
    pub fn parse(search: &str) -> Self {
        let query = search.strip_prefix('?').unwrap_or(search);
        let params = query.split('&')
            .filter(|s| !s.is_empty())
            .filter_map(|pair| {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                Some((decode_uri(k), decode_uri(v)))
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
            .map(|(k, v)| format!("{}={}", encode_uri(k), encode_uri(v)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

fn decode_uri(s: &str) -> String {
    s.replace('+', " ")
}

fn encode_uri(s: &str) -> String {
    s.replace(' ', "+")
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_url_parse() {
        let url = JsUrl::parse("https://example.com:8080/path?q=1#hash").unwrap();
        
        assert_eq!(url.protocol, "https:");
        assert_eq!(url.hostname, "example.com");
        assert_eq!(url.port, "8080");
        assert_eq!(url.pathname, "/path");
        assert_eq!(url.search, "?q=1");
        assert_eq!(url.hash, "#hash");
    }
    
    #[test]
    fn test_search_params() {
        let params = JsUrlSearchParams::parse("?foo=bar&baz=qux");
        
        assert_eq!(params.get("foo"), Some("bar"));
        assert_eq!(params.get("baz"), Some("qux"));
    }
}
