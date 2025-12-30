//! Custom URL Parser (RFC 3986)
//!
//! A zero-dependency URL parser with StringInterner integration.

use crate::interner::{InternedString, StringInterner};
use std::fmt;
use std::str::FromStr;

/// URL parse error
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    EmptyUrl,
    InvalidScheme,
    InvalidHost,
    InvalidPort,
    InvalidIpv4,
    InvalidIpv6,
    InvalidPath,
    InvalidPercentEncoding,
    RelativeUrlWithoutBase,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::EmptyUrl => write!(f, "empty URL"),
            ParseError::InvalidScheme => write!(f, "invalid scheme"),
            ParseError::InvalidHost => write!(f, "invalid host"),
            ParseError::InvalidPort => write!(f, "invalid port"),
            ParseError::InvalidIpv4 => write!(f, "invalid IPv4 address"),
            ParseError::InvalidIpv6 => write!(f, "invalid IPv6 address"),
            ParseError::InvalidPath => write!(f, "invalid path"),
            ParseError::InvalidPercentEncoding => write!(f, "invalid percent encoding"),
            ParseError::RelativeUrlWithoutBase => write!(f, "relative URL without base"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Host types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Host {
    /// Domain name (may be punycode-encoded for IDN)
    Domain(String),
    /// IPv4 address
    Ipv4([u8; 4]),
    /// IPv6 address
    Ipv6([u16; 8]),
}

impl Host {
    /// Parse a host string
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        if s.is_empty() {
            return Err(ParseError::InvalidHost);
        }
        
        // Check for IPv6 (enclosed in brackets)
        if s.starts_with('[') && s.ends_with(']') {
            let inner = &s[1..s.len()-1];
            return Self::parse_ipv6(inner);
        }
        
        // Try to parse as IPv4
        if let Ok(ipv4) = Self::parse_ipv4(s) {
            return Ok(ipv4);
        }
        
        // Otherwise it's a domain
        Ok(Host::Domain(s.to_ascii_lowercase()))
    }
    
    fn parse_ipv4(s: &str) -> Result<Self, ParseError> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 4 {
            return Err(ParseError::InvalidIpv4);
        }
        
        let mut octets = [0u8; 4];
        for (i, part) in parts.iter().enumerate() {
            octets[i] = part.parse().map_err(|_| ParseError::InvalidIpv4)?;
        }
        
        Ok(Host::Ipv4(octets))
    }
    
    fn parse_ipv6(s: &str) -> Result<Self, ParseError> {
        // Handle :: expansion
        let mut segments = [0u16; 8];
        let parts: Vec<&str> = s.split(':').collect();
        
        // Find :: position
        let empty_pos = parts.iter().position(|p| p.is_empty());
        
        if let Some(pos) = empty_pos {
            // Fill from start
            for (i, part) in parts[..pos].iter().enumerate() {
                if !part.is_empty() {
                    segments[i] = u16::from_str_radix(part, 16).map_err(|_| ParseError::InvalidIpv6)?;
                }
            }
            // Fill from end
            let remaining = &parts[pos+1..];
            let offset = 8 - remaining.len();
            for (i, part) in remaining.iter().enumerate() {
                if !part.is_empty() {
                    segments[offset + i] = u16::from_str_radix(part, 16).map_err(|_| ParseError::InvalidIpv6)?;
                }
            }
        } else {
            if parts.len() != 8 {
                return Err(ParseError::InvalidIpv6);
            }
            for (i, part) in parts.iter().enumerate() {
                segments[i] = u16::from_str_radix(part, 16).map_err(|_| ParseError::InvalidIpv6)?;
            }
        }
        
        Ok(Host::Ipv6(segments))
    }
    
    /// Get the host as a string
    pub fn as_str(&self) -> String {
        match self {
            Host::Domain(s) => s.clone(),
            Host::Ipv4(octets) => format!("{}.{}.{}.{}", octets[0], octets[1], octets[2], octets[3]),
            Host::Ipv6(segments) => {
                format!("[{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}:{:x}]",
                    segments[0], segments[1], segments[2], segments[3],
                    segments[4], segments[5], segments[6], segments[7])
            }
        }
    }
}

impl fmt::Display for Host {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// User info (username:password)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UserInfo {
    pub username: String,
    pub password: Option<String>,
}

/// Query parameters
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Query {
    params: Vec<(String, String)>,
}

impl Query {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Parse query string (without leading ?)
    pub fn parse(s: &str) -> Self {
        let params = s.split('&')
            .filter(|p| !p.is_empty())
            .map(|pair| {
                let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
                (percent_decode(k), percent_decode(v))
            })
            .collect();
        Self { params }
    }
    
    /// Get first value for key
    pub fn get(&self, key: &str) -> Option<&str> {
        self.params.iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
    
    /// Get all values for key
    pub fn get_all(&self, key: &str) -> Vec<&str> {
        self.params.iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
            .collect()
    }
    
    /// Set a value (replaces existing)
    pub fn set(&mut self, key: &str, value: &str) {
        self.delete(key);
        self.append(key, value);
    }
    
    /// Append a value
    pub fn append(&mut self, key: &str, value: &str) {
        self.params.push((key.to_string(), value.to_string()));
    }
    
    /// Delete all values for key
    pub fn delete(&mut self, key: &str) {
        self.params.retain(|(k, _)| k != key);
    }
    
    /// Check if key exists
    pub fn has(&self, key: &str) -> bool {
        self.params.iter().any(|(k, _)| k == key)
    }
    
    /// Iterate over keys
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.params.iter().map(|(k, _)| k.as_str())
    }
    
    /// Iterate over values
    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.params.iter().map(|(_, v)| v.as_str())
    }
    
    /// Iterate over entries
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.params.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
    
    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.params.is_empty()
    }
    
    /// Serialize to query string (without leading ?)
    pub fn to_string(&self) -> String {
        self.params.iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }
}

/// Parsed URL (RFC 3986)
#[derive(Debug, Clone)]
pub struct Url {
    /// Scheme (http, https, fos, file, etc.)
    scheme: String,
    /// User info (optional)
    userinfo: Option<UserInfo>,
    /// Host (domain or IP)
    host: Option<Host>,
    /// Port number (optional)
    port: Option<u16>,
    /// Path segments
    path: String,
    /// Query string (optional)
    query: Option<Query>,
    /// Fragment (optional)
    fragment: Option<String>,
}

impl Url {
    /// Parse a URL string
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(ParseError::EmptyUrl);
        }
        
        // Extract fragment
        let (rest, fragment) = match input.rsplit_once('#') {
            Some((r, f)) => (r, Some(percent_decode(f))),
            None => (input, None),
        };
        
        // Extract query
        let (rest, query) = match rest.split_once('?') {
            Some((r, q)) => (r, Some(Query::parse(q))),
            None => (rest, None),
        };
        
        // Extract scheme
        let (scheme, rest) = match rest.split_once("://") {
            Some((s, r)) => {
                let scheme = s.to_ascii_lowercase();
                if !is_valid_scheme(&scheme) {
                    return Err(ParseError::InvalidScheme);
                }
                (scheme, r)
            }
            None => {
                // Could be a scheme without authority (e.g., "about:blank")
                if let Some((s, r)) = rest.split_once(':') {
                    if is_valid_scheme(s) && !r.starts_with("//") {
                        return Ok(Url {
                            scheme: s.to_ascii_lowercase(),
                            userinfo: None,
                            host: None,
                            port: None,
                            path: r.to_string(),
                            query,
                            fragment,
                        });
                    }
                }
                return Err(ParseError::InvalidScheme);
            }
        };
        
        // Extract authority (userinfo@host:port) and path
        let (authority, path) = match rest.find('/') {
            Some(pos) => (&rest[..pos], &rest[pos..]),
            None => (rest, "/"),
        };
        
        // Parse authority
        let (userinfo, host_port) = match authority.rsplit_once('@') {
            Some((ui, hp)) => {
                let (username, password) = match ui.split_once(':') {
                    Some((u, p)) => (percent_decode(u), Some(percent_decode(p))),
                    None => (percent_decode(ui), None),
                };
                (Some(UserInfo { username, password }), hp)
            }
            None => (None, authority),
        };
        
        // Parse host and port
        let (host, port) = if host_port.starts_with('[') {
            // IPv6
            match host_port.find(']') {
                Some(pos) => {
                    let host_str = &host_port[..=pos];
                    let rest = &host_port[pos+1..];
                    let port = if rest.starts_with(':') {
                        Some(rest[1..].parse().map_err(|_| ParseError::InvalidPort)?)
                    } else {
                        None
                    };
                    (Some(Host::parse(host_str)?), port)
                }
                None => return Err(ParseError::InvalidIpv6),
            }
        } else {
            match host_port.rsplit_once(':') {
                Some((h, p)) => {
                    let port: u16 = p.parse().map_err(|_| ParseError::InvalidPort)?;
                    (Some(Host::parse(h)?), Some(port))
                }
                None => {
                    if host_port.is_empty() {
                        (None, None)
                    } else {
                        (Some(Host::parse(host_port)?), None)
                    }
                }
            }
        };
        
        Ok(Url {
            scheme,
            userinfo,
            host,
            port,
            path: path.to_string(),
            query,
            fragment,
        })
    }
    
    /// Create a URL from parts
    pub fn from_parts(
        scheme: &str,
        host: &str,
        path: &str,
    ) -> Result<Self, ParseError> {
        Ok(Url {
            scheme: scheme.to_ascii_lowercase(),
            userinfo: None,
            host: Some(Host::parse(host)?),
            port: None,
            path: if path.is_empty() { "/".to_string() } else { path.to_string() },
            query: None,
            fragment: None,
        })
    }
    
    /// Join a relative URL to this base
    pub fn join(&self, relative: &str) -> Result<Self, ParseError> {
        let relative = relative.trim();
        
        // Absolute URL
        if relative.contains("://") {
            return Url::parse(relative);
        }
        
        // Protocol-relative URL
        if relative.starts_with("//") {
            return Url::parse(&format!("{}:{}", self.scheme, relative));
        }
        
        // Absolute path
        if relative.starts_with('/') {
            let mut new_url = self.clone();
            
            // Extract query and fragment from relative
            let (path, rest) = relative.split_once('?')
                .map(|(p, r)| (p, Some(r)))
                .unwrap_or((relative, None));
            
            let (path, fragment) = path.rsplit_once('#')
                .map(|(p, f)| (p, Some(f.to_string())))
                .unwrap_or((path, None));
            
            new_url.path = normalize_path(path);
            new_url.query = rest.and_then(|r| {
                let q = r.split('#').next().unwrap_or(r);
                if q.is_empty() { None } else { Some(Query::parse(q)) }
            });
            new_url.fragment = fragment.or_else(|| {
                rest.and_then(|r| r.split_once('#').map(|(_, f)| f.to_string()))
            });
            
            return Ok(new_url);
        }
        
        // Relative path
        let mut new_url = self.clone();
        
        // Get base path directory
        let base_dir = match self.path.rfind('/') {
            Some(pos) => &self.path[..=pos],
            None => "/",
        };
        
        // Extract query and fragment
        let (rel_path, rest) = relative.split_once('?')
            .map(|(p, r)| (p, Some(r)))
            .unwrap_or((relative, None));
        
        let (rel_path, fragment) = rel_path.rsplit_once('#')
            .map(|(p, f)| (p, Some(f.to_string())))
            .unwrap_or((rel_path, None));
        
        new_url.path = normalize_path(&format!("{}{}", base_dir, rel_path));
        new_url.query = rest.and_then(|r| {
            let q = r.split('#').next().unwrap_or(r);
            if q.is_empty() { None } else { Some(Query::parse(q)) }
        });
        new_url.fragment = fragment.or_else(|| {
            rest.and_then(|r| r.split_once('#').map(|(_, f)| f.to_string()))
        });
        
        Ok(new_url)
    }
    
    // Getters
    
    /// Get scheme
    pub fn scheme(&self) -> &str {
        &self.scheme
    }
    
    /// Get username (if present)
    pub fn username(&self) -> &str {
        self.userinfo.as_ref().map(|u| u.username.as_str()).unwrap_or("")
    }
    
    /// Get password (if present)
    pub fn password(&self) -> Option<&str> {
        self.userinfo.as_ref().and_then(|u| u.password.as_deref())
    }
    
    /// Get host (domain/IP as string)
    pub fn host_str(&self) -> Option<&str> {
        match &self.host {
            Some(Host::Domain(s)) => Some(s),
            _ => None,
        }
    }
    
    /// Get host
    pub fn host(&self) -> Option<&Host> {
        self.host.as_ref()
    }
    
    /// Get port
    pub fn port(&self) -> Option<u16> {
        self.port
    }
    
    /// Get port or default for scheme
    pub fn port_or_default(&self) -> Option<u16> {
        self.port.or_else(|| default_port(&self.scheme))
    }
    
    /// Get path
    pub fn path(&self) -> &str {
        &self.path
    }
    
    /// Get query string (without leading ?)
    pub fn query(&self) -> Option<&str> {
        // We need to return the raw query string
        None // Query is stored as parsed, need to serialize
    }
    
    /// Get query parameters
    pub fn query_params(&self) -> Option<&Query> {
        self.query.as_ref()
    }
    
    /// Get mutable query parameters
    pub fn query_params_mut(&mut self) -> &mut Query {
        self.query.get_or_insert_with(Query::new)
    }
    
    /// Get fragment (without leading #)
    pub fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }
    
    /// Get origin (scheme://host:port)
    pub fn origin(&self) -> String {
        let mut origin = format!("{}://", self.scheme);
        if let Some(host) = &self.host {
            origin.push_str(&host.as_str());
            if let Some(port) = self.port {
                if Some(port) != default_port(&self.scheme) {
                    origin.push_str(&format!(":{}", port));
                }
            }
        }
        origin
    }
    
    /// Get host with port
    pub fn host_with_port(&self) -> String {
        let mut result = String::new();
        if let Some(host) = &self.host {
            result.push_str(&host.as_str());
            if let Some(port) = self.port {
                result.push(':');
                result.push_str(&port.to_string());
            }
        }
        result
    }
    
    /// Convert to string (full URL)
    pub fn as_str(&self) -> String {
        let mut url = String::new();
        
        url.push_str(&self.scheme);
        
        if self.host.is_some() {
            url.push_str("://");
            
            if let Some(ref userinfo) = self.userinfo {
                url.push_str(&percent_encode(&userinfo.username));
                if let Some(ref password) = userinfo.password {
                    url.push(':');
                    url.push_str(&percent_encode(password));
                }
                url.push('@');
            }
            
            if let Some(ref host) = self.host {
                url.push_str(&host.as_str());
            }
            
            if let Some(port) = self.port {
                if Some(port) != default_port(&self.scheme) {
                    url.push(':');
                    url.push_str(&port.to_string());
                }
            }
        } else {
            url.push(':');
        }
        
        url.push_str(&self.path);
        
        if let Some(ref query) = self.query {
            if !query.is_empty() {
                url.push('?');
                url.push_str(&query.to_string());
            }
        }
        
        if let Some(ref fragment) = self.fragment {
            url.push('#');
            url.push_str(&percent_encode(fragment));
        }
        
        url
    }
    
    /// Set the scheme
    pub fn set_scheme(&mut self, scheme: &str) -> Result<(), ParseError> {
        if !is_valid_scheme(scheme) {
            return Err(ParseError::InvalidScheme);
        }
        self.scheme = scheme.to_ascii_lowercase();
        Ok(())
    }
    
    /// Set the host
    pub fn set_host(&mut self, host: &str) -> Result<(), ParseError> {
        self.host = Some(Host::parse(host)?);
        Ok(())
    }
    
    /// Set the port
    pub fn set_port(&mut self, port: Option<u16>) {
        self.port = port;
    }
    
    /// Set the path
    pub fn set_path(&mut self, path: &str) {
        self.path = normalize_path(path);
    }
    
    /// Set the fragment
    pub fn set_fragment(&mut self, fragment: Option<&str>) {
        self.fragment = fragment.map(|s| s.to_string());
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for Url {
    type Err = ParseError;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Url::parse(s)
    }
}

// ============================================================================
// Percent Encoding (RFC 3986)
// ============================================================================

/// Percent-decode a string
pub fn percent_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '%' {
            // Get next two hex digits
            let h1 = chars.next();
            let h2 = chars.next();
            
            if let (Some(c1), Some(c2)) = (h1, h2) {
                if let Ok(byte) = u8::from_str_radix(&format!("{}{}", c1, c2), 16) {
                    result.push(byte as char);
                    continue;
                }
            }
            // Invalid encoding, keep as-is
            result.push('%');
            if let Some(c1) = h1 { result.push(c1); }
            if let Some(c2) = h2 { result.push(c2); }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    
    result
}

/// Percent-encode a string (for query parameters)
pub fn percent_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    
    for c in s.chars() {
        match c {
            // Unreserved characters (RFC 3986)
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                result.push(c);
            }
            ' ' => {
                result.push('+');
            }
            _ => {
                // Encode as UTF-8 bytes
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    
    result
}

/// Percent-encode for path components
pub fn percent_encode_path(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    
    for c in s.chars() {
        match c {
            // Unreserved + sub-delims + : and @ (allowed in path)
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' |
            '!' | '$' | '&' | '\'' | '(' | ')' | '*' | '+' | ',' | ';' | '=' |
            ':' | '@' | '/' => {
                result.push(c);
            }
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    
    result
}

// ============================================================================
// Punycode (RFC 3492) for IDN
// ============================================================================

const PUNYCODE_BASE: u32 = 36;
const PUNYCODE_TMIN: u32 = 1;
const PUNYCODE_TMAX: u32 = 26;
const PUNYCODE_SKEW: u32 = 38;
const PUNYCODE_DAMP: u32 = 700;
const PUNYCODE_INITIAL_BIAS: u32 = 72;
const PUNYCODE_INITIAL_N: u32 = 128;

fn adapt(delta: u32, num_points: u32, first_time: bool) -> u32 {
    let mut delta = if first_time {
        delta / PUNYCODE_DAMP
    } else {
        delta / 2
    };
    
    delta += delta / num_points;
    
    let mut k = 0;
    while delta > ((PUNYCODE_BASE - PUNYCODE_TMIN) * PUNYCODE_TMAX) / 2 {
        delta /= PUNYCODE_BASE - PUNYCODE_TMIN;
        k += PUNYCODE_BASE;
    }
    
    k + (((PUNYCODE_BASE - PUNYCODE_TMIN + 1) * delta) / (delta + PUNYCODE_SKEW))
}

fn encode_digit(d: u32) -> char {
    if d < 26 {
        (d as u8 + b'a') as char
    } else {
        (d as u8 - 26 + b'0') as char
    }
}

fn decode_digit(c: char) -> Option<u32> {
    match c {
        'a'..='z' => Some(c as u32 - 'a' as u32),
        'A'..='Z' => Some(c as u32 - 'A' as u32),
        '0'..='9' => Some(c as u32 - '0' as u32 + 26),
        _ => None,
    }
}

/// Encode a Unicode string to Punycode
pub fn punycode_encode(input: &str) -> String {
    let mut output = String::new();
    
    // Copy ASCII characters
    for c in input.chars() {
        if c.is_ascii() {
            output.push(c.to_ascii_lowercase());
        }
    }
    
    let basic_len = output.len() as u32;
    let mut handled = basic_len;
    
    if basic_len > 0 && basic_len < input.chars().count() as u32 {
        output.push('-');
    }
    
    let input_chars: Vec<u32> = input.chars().map(|c| c as u32).collect();
    let input_len = input_chars.len() as u32;
    
    let mut n = PUNYCODE_INITIAL_N;
    let mut delta = 0u32;
    let mut bias = PUNYCODE_INITIAL_BIAS;
    
    while handled < input_len {
        // Find minimum code point >= n
        let m = input_chars.iter()
            .filter(|&&c| c >= n)
            .min()
            .copied()
            .unwrap_or(n);
        
        delta = delta.saturating_add((m - n).saturating_mul(handled + 1));
        n = m;
        
        for &c in &input_chars {
            if c < n {
                delta = delta.saturating_add(1);
            } else if c == n {
                let mut q = delta;
                let mut k = PUNYCODE_BASE;
                
                loop {
                    let t = if k <= bias {
                        PUNYCODE_TMIN
                    } else if k >= bias + PUNYCODE_TMAX {
                        PUNYCODE_TMAX
                    } else {
                        k - bias
                    };
                    
                    if q < t {
                        break;
                    }
                    
                    output.push(encode_digit(t + ((q - t) % (PUNYCODE_BASE - t))));
                    q = (q - t) / (PUNYCODE_BASE - t);
                    k += PUNYCODE_BASE;
                }
                
                output.push(encode_digit(q));
                bias = adapt(delta, handled + 1, handled == basic_len);
                delta = 0;
                handled += 1;
            }
        }
        
        delta += 1;
        n += 1;
    }
    
    output
}

/// Decode a Punycode string to Unicode
pub fn punycode_decode(input: &str) -> Option<String> {
    let mut output: Vec<char> = Vec::new();
    
    // Find last delimiter
    let (basic, encoded) = match input.rfind('-') {
        Some(pos) => (&input[..pos], &input[pos+1..]),
        None => ("", input),
    };
    
    // Copy basic characters
    for c in basic.chars() {
        output.push(c);
    }
    
    let mut n = PUNYCODE_INITIAL_N;
    let mut i = 0u32;
    let mut bias = PUNYCODE_INITIAL_BIAS;
    
    let mut chars = encoded.chars().peekable();
    
    while chars.peek().is_some() {
        let old_i = i;
        let mut w = 1u32;
        let mut k = PUNYCODE_BASE;
        
        loop {
            let c = chars.next()?;
            let digit = decode_digit(c)?;
            
            i = i.checked_add(digit.checked_mul(w)?)?;
            
            let t = if k <= bias {
                PUNYCODE_TMIN
            } else if k >= bias + PUNYCODE_TMAX {
                PUNYCODE_TMAX
            } else {
                k - bias
            };
            
            if digit < t {
                break;
            }
            
            w = w.checked_mul(PUNYCODE_BASE - t)?;
            k += PUNYCODE_BASE;
        }
        
        let out_len = output.len() as u32 + 1;
        bias = adapt(i - old_i, out_len, old_i == 0);
        n = n.checked_add(i / out_len)?;
        i %= out_len;
        
        output.insert(i as usize, char::from_u32(n)?);
        i += 1;
    }
    
    Some(output.into_iter().collect())
}

/// Encode an IDN domain to ASCII (ACE)
pub fn idn_to_ascii(domain: &str) -> String {
    domain.split('.')
        .map(|label| {
            if label.chars().all(|c| c.is_ascii()) {
                label.to_ascii_lowercase()
            } else {
                format!("xn--{}", punycode_encode(label))
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Decode an ASCII domain to Unicode IDN
pub fn idn_to_unicode(domain: &str) -> String {
    domain.split('.')
        .map(|label| {
            if let Some(encoded) = label.strip_prefix("xn--") {
                punycode_decode(encoded).unwrap_or_else(|| label.to_string())
            } else {
                label.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Check if a scheme is valid
fn is_valid_scheme(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    
    let mut chars = s.chars();
    
    // First char must be a letter
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    
    // Rest can be letter, digit, +, -, .
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

/// Get default port for a scheme
fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" => Some(80),
        "https" => Some(443),
        "ftp" => Some(21),
        "ws" => Some(80),
        "wss" => Some(443),
        _ => None,
    }
}

/// Normalize a path (resolve . and ..)
fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    
    let mut segments: Vec<&str> = Vec::new();
    
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => { segments.pop(); }
            s => segments.push(s),
        }
    }
    
    let result = format!("/{}", segments.join("/"));
    
    // Preserve trailing slash if original had one
    if path.ends_with('/') && !result.ends_with('/') {
        format!("{}/", result)
    } else {
        result
    }
}

// ============================================================================
// URL Interner (Optional memory optimization)
// ============================================================================

/// URL interner for memory-efficient storage
#[derive(Default)]
pub struct UrlInterner {
    schemes: StringInterner,
    hosts: StringInterner,
}

impl UrlInterner {
    pub fn new() -> Self {
        let mut interner = Self::default();
        // Pre-intern common schemes
        for scheme in &["http", "https", "file", "fos", "about", "data", "ws", "wss"] {
            interner.schemes.intern(scheme);
        }
        interner
    }
    
    /// Intern a scheme
    pub fn intern_scheme(&mut self, scheme: &str) -> InternedString {
        self.schemes.intern(scheme)
    }
    
    /// Intern a host
    pub fn intern_host(&mut self, host: &str) -> InternedString {
        self.hosts.intern(host)
    }
    
    /// Get scheme string
    pub fn get_scheme(&self, interned: InternedString) -> &str {
        self.schemes.get(interned)
    }
    
    /// Get host string
    pub fn get_host(&self, interned: InternedString) -> &str {
        self.hosts.get(interned)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple() {
        let url = Url::parse("https://example.com/path").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("example.com"));
        assert_eq!(url.path(), "/path");
    }
    
    #[test]
    fn test_parse_full() {
        let url = Url::parse("https://user:pass@example.com:8080/path?q=1#hash").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.username(), "user");
        assert_eq!(url.password(), Some("pass"));
        assert_eq!(url.host_str(), Some("example.com"));
        assert_eq!(url.port(), Some(8080));
        assert_eq!(url.path(), "/path");
        assert_eq!(url.query_params().unwrap().get("q"), Some("1"));
        assert_eq!(url.fragment(), Some("hash"));
    }
    
    #[test]
    fn test_parse_ipv4() {
        let url = Url::parse("http://192.168.1.1:8080/").unwrap();
        assert!(matches!(url.host(), Some(Host::Ipv4([192, 168, 1, 1]))));
        assert_eq!(url.port(), Some(8080));
    }
    
    #[test]
    fn test_parse_ipv6() {
        let url = Url::parse("http://[::1]:8080/").unwrap();
        assert!(matches!(url.host(), Some(Host::Ipv6(_))));
    }
    
    #[test]
    fn test_parse_about() {
        let url = Url::parse("about:blank").unwrap();
        assert_eq!(url.scheme(), "about");
        assert_eq!(url.path(), "blank");
    }
    
    #[test]
    fn test_join_absolute() {
        let base = Url::parse("https://example.com/dir/page").unwrap();
        let joined = base.join("/other/page").unwrap();
        assert_eq!(joined.as_str(), "https://example.com/other/page");
    }
    
    #[test]
    fn test_join_relative() {
        let base = Url::parse("https://example.com/dir/page").unwrap();
        let joined = base.join("other").unwrap();
        assert_eq!(joined.as_str(), "https://example.com/dir/other");
    }
    
    #[test]
    fn test_join_parent() {
        let base = Url::parse("https://example.com/a/b/c").unwrap();
        let joined = base.join("../d").unwrap();
        assert_eq!(joined.as_str(), "https://example.com/a/d");
    }
    
    #[test]
    fn test_percent_encoding() {
        assert_eq!(percent_encode("hello world"), "hello+world");
        assert_eq!(percent_decode("hello+world"), "hello world");
        assert_eq!(percent_decode("hello%20world"), "hello world");
    }
    
    #[test]
    fn test_query_params() {
        let query = Query::parse("foo=bar&baz=qux&foo=second");
        assert_eq!(query.get("foo"), Some("bar"));
        assert_eq!(query.get_all("foo"), vec!["bar", "second"]);
        assert_eq!(query.get("baz"), Some("qux"));
        assert!(query.has("foo"));
        assert!(!query.has("missing"));
    }
    
    #[test]
    fn test_punycode_encode() {
        // "münchen" -> "mnchen-3ya"
        let encoded = punycode_encode("münchen");
        assert!(encoded.contains("mnchen"));
    }
    
    #[test]
    fn test_idn() {
        let ascii = idn_to_ascii("münchen.de");
        assert!(ascii.starts_with("xn--"));
        assert!(ascii.ends_with(".de"));
    }
    
    #[test]
    fn test_normalize_path() {
        assert_eq!(normalize_path("/a/b/../c"), "/a/c");
        assert_eq!(normalize_path("/a/./b/./c"), "/a/b/c");
        assert_eq!(normalize_path("/../a"), "/a");
    }
    
    #[test]
    fn test_origin() {
        let url = Url::parse("https://example.com:443/path").unwrap();
        assert_eq!(url.origin(), "https://example.com");
        
        let url = Url::parse("https://example.com:8080/path").unwrap();
        assert_eq!(url.origin(), "https://example.com:8080");
    }
    
    #[test]
    fn test_url_interner() {
        let mut interner = UrlInterner::new();
        let s1 = interner.intern_scheme("https");
        let s2 = interner.intern_scheme("https");
        assert_eq!(s1, s2);
    }
}
