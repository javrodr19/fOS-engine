//! Origin and Same-Origin Policy
//!
//! Web origin model and CORS implementation.

/// Web Origin
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Origin {
    pub scheme: String,
    pub host: String,
    pub port: Option<u16>,
}

impl Origin {
    /// Create new origin
    pub fn new(scheme: &str, host: &str, port: Option<u16>) -> Self {
        Self {
            scheme: scheme.to_lowercase(),
            host: host.to_lowercase(),
            port,
        }
    }
    
    /// Parse from URL
    pub fn from_url(url: &str) -> Option<Self> {
        let url = url.trim();
        
        // Find scheme
        let scheme_end = url.find("://")?;
        let scheme = &url[..scheme_end];
        let rest = &url[scheme_end + 3..];
        
        // Find host and port
        let path_start = rest.find('/').unwrap_or(rest.len());
        let host_port = &rest[..path_start];
        
        let (host, port) = if let Some(colon) = host_port.rfind(':') {
            let potential_port = &host_port[colon + 1..];
            if let Ok(p) = potential_port.parse::<u16>() {
                (&host_port[..colon], Some(p))
            } else {
                (host_port, None)
            }
        } else {
            (host_port, None)
        };
        
        Some(Self::new(scheme, host, port))
    }
    
    /// Check if same origin
    pub fn is_same_origin(&self, other: &Origin) -> bool {
        self.scheme == other.scheme 
            && self.host == other.host 
            && self.effective_port() == other.effective_port()
    }
    
    /// Get effective port (default port for scheme if not specified)
    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or_else(|| match self.scheme.as_str() {
            "http" => 80,
            "https" => 443,
            "ws" => 80,
            "wss" => 443,
            "ftp" => 21,
            _ => 0,
        })
    }
    
    /// Check if opaque origin
    pub fn is_opaque(&self) -> bool {
        self.scheme == "data" || self.scheme == "file" || self.scheme == "blob"
    }
    
    /// Serialize origin
    pub fn serialize(&self) -> String {
        if self.is_opaque() {
            return "null".to_string();
        }
        
        let port_str = match (self.scheme.as_str(), self.port) {
            ("http", Some(80)) | ("https", Some(443)) => String::new(),
            (_, Some(p)) => format!(":{}", p),
            (_, None) => String::new(),
        };
        
        format!("{}://{}{}", self.scheme, self.host, port_str)
    }
}

/// CORS request mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CorsMode {
    #[default]
    NoCors,
    Cors,
    SameOrigin,
    Navigate,
}

/// CORS credentials mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CredentialsMode {
    Omit,
    #[default]
    SameOrigin,
    Include,
}

/// CORS request
#[derive(Debug, Clone)]
pub struct CorsRequest {
    pub origin: Origin,
    pub method: String,
    pub headers: Vec<String>,
    pub credentials: CredentialsMode,
}

/// CORS response
#[derive(Debug, Clone)]
pub struct CorsResponse {
    pub allow_origin: Option<String>,
    pub allow_methods: Vec<String>,
    pub allow_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age: Option<u32>,
    pub expose_headers: Vec<String>,
}

/// CORS validator
#[derive(Debug, Default)]
pub struct CorsValidator {
    pub allowed_origins: Vec<String>,
    pub allowed_methods: Vec<String>,
    pub allowed_headers: Vec<String>,
    pub allow_credentials: bool,
    pub max_age: u32,
}

impl CorsValidator {
    pub fn new() -> Self {
        Self {
            allowed_origins: vec!["*".to_string()],
            allowed_methods: vec!["GET".into(), "POST".into(), "HEAD".into()],
            allowed_headers: Vec::new(),
            allow_credentials: false,
            max_age: 86400,
        }
    }
    
    /// Check if origin is allowed
    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        self.allowed_origins.contains(&"*".to_string()) ||
        self.allowed_origins.iter().any(|o| o == origin)
    }
    
    /// Check if method is allowed
    pub fn is_method_allowed(&self, method: &str) -> bool {
        let method = method.to_uppercase();
        // Simple methods always allowed
        if matches!(method.as_str(), "GET" | "HEAD" | "POST") {
            return true;
        }
        self.allowed_methods.iter().any(|m| m.to_uppercase() == method)
    }
    
    /// Check preflight request
    pub fn check_preflight(&self, request: &CorsRequest) -> CorsResponse {
        let origin = request.origin.serialize();
        let allowed = self.is_origin_allowed(&origin);
        
        CorsResponse {
            allow_origin: if allowed { Some(origin) } else { None },
            allow_methods: self.allowed_methods.clone(),
            allow_headers: self.allowed_headers.clone(),
            allow_credentials: self.allow_credentials,
            max_age: Some(self.max_age),
            expose_headers: Vec::new(),
        }
    }
    
    /// Safe headers (no preflight needed)
    pub fn is_safe_header(name: &str) -> bool {
        let name = name.to_lowercase();
        matches!(name.as_str(), 
            "accept" | "accept-language" | "content-language" | "content-type"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_origin_parse() {
        let origin = Origin::from_url("https://example.com:8080/path").unwrap();
        assert_eq!(origin.scheme, "https");
        assert_eq!(origin.host, "example.com");
        assert_eq!(origin.port, Some(8080));
    }
    
    #[test]
    fn test_same_origin() {
        let o1 = Origin::new("https", "example.com", None);
        let o2 = Origin::new("https", "example.com", Some(443));
        let o3 = Origin::new("http", "example.com", None);
        
        assert!(o1.is_same_origin(&o2)); // Default port
        assert!(!o1.is_same_origin(&o3)); // Different scheme
    }
    
    #[test]
    fn test_cors_validator() {
        let validator = CorsValidator::new();
        assert!(validator.is_origin_allowed("https://any.com"));
        assert!(validator.is_method_allowed("GET"));
    }
}
