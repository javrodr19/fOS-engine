//! HTTP/1.1 Framing
//!
//! Request serialization and response parsing for HTTP/1.1.

use std::io::{self, Read, Write};
use std::collections::HashMap;

/// HTTP/1.1 request
#[derive(Debug, Clone)]
pub struct Http1Request {
    /// HTTP method
    pub method: String,
    /// Request path (e.g., "/api/users")
    pub path: String,
    /// HTTP version (1.0 or 1.1)
    pub version: HttpVersion,
    /// Request headers
    pub headers: Vec<(String, String)>,
    /// Request body
    pub body: Option<Vec<u8>>,
}

/// HTTP version
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HttpVersion {
    Http10,
    #[default]
    Http11,
}

impl std::fmt::Display for HttpVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HttpVersion::Http10 => write!(f, "HTTP/1.0"),
            HttpVersion::Http11 => write!(f, "HTTP/1.1"),
        }
    }
}

impl Http1Request {
    /// Create a new request
    pub fn new(method: &str, path: &str) -> Self {
        Self {
            method: method.to_uppercase(),
            path: path.to_string(),
            version: HttpVersion::Http11,
            headers: Vec::new(),
            body: None,
        }
    }
    
    /// Add a header
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
    
    /// Set body
    pub fn body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }
    
    /// Serialize to bytes
    pub fn serialize(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        
        // Request line
        buf.extend_from_slice(format!("{} {} {}\r\n", self.method, self.path, self.version).as_bytes());
        
        // Headers
        for (name, value) in &self.headers {
            buf.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
        }
        
        // Content-Length if body present
        if let Some(ref body) = self.body {
            if !self.headers.iter().any(|(n, _)| n.eq_ignore_ascii_case("content-length")) {
                buf.extend_from_slice(format!("Content-Length: {}\r\n", body.len()).as_bytes());
            }
        }
        
        // End of headers
        buf.extend_from_slice(b"\r\n");
        
        // Body
        if let Some(ref body) = self.body {
            buf.extend_from_slice(body);
        }
        
        buf
    }
    
    /// Write to a stream
    pub fn write_to<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(&self.serialize())?;
        writer.flush()
    }
}

/// HTTP/1.1 response
#[derive(Debug, Clone)]
pub struct Http1Response {
    /// HTTP version
    pub version: HttpVersion,
    /// Status code
    pub status: u16,
    /// Status reason phrase
    pub reason: String,
    /// Response headers
    pub headers: Vec<(String, String)>,
    /// Response body
    pub body: Vec<u8>,
}

impl Http1Response {
    /// Get header value (case-insensitive)
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    
    /// Get Content-Length
    pub fn content_length(&self) -> Option<usize> {
        self.header("content-length")
            .and_then(|v| v.parse().ok())
    }
    
    /// Check if chunked transfer encoding
    pub fn is_chunked(&self) -> bool {
        self.header("transfer-encoding")
            .map(|v| v.eq_ignore_ascii_case("chunked"))
            .unwrap_or(false)
    }
    
    /// Check if connection should be kept alive
    pub fn keep_alive(&self) -> bool {
        if self.version == HttpVersion::Http10 {
            // HTTP/1.0: keep-alive only if explicitly requested
            self.header("connection")
                .map(|v| v.eq_ignore_ascii_case("keep-alive"))
                .unwrap_or(false)
        } else {
            // HTTP/1.1: keep-alive by default unless "close"
            !self.header("connection")
                .map(|v| v.eq_ignore_ascii_case("close"))
                .unwrap_or(false)
        }
    }
    
    /// Check if response is successful (2xx)
    pub fn is_success(&self) -> bool {
        self.status >= 200 && self.status < 300
    }
    
    /// Check if response is redirect (3xx)
    pub fn is_redirect(&self) -> bool {
        self.status >= 300 && self.status < 400
    }
    
    /// Get redirect location
    pub fn redirect_location(&self) -> Option<&str> {
        self.header("location")
    }
}

/// HTTP/1.1 response parser
pub struct Http1Parser {
    /// Current state
    state: ParseState,
    /// Parsed headers
    headers: Vec<(String, String)>,
    /// Status code
    status: u16,
    /// Reason phrase
    reason: String,
    /// HTTP version
    version: HttpVersion,
    /// Body bytes
    body: Vec<u8>,
    /// Expected content length
    content_length: Option<usize>,
    /// Chunked encoding
    chunked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParseState {
    StatusLine,
    Headers,
    Body,
    ChunkedBody,
    Complete,
}

impl Http1Parser {
    pub fn new() -> Self {
        Self {
            state: ParseState::StatusLine,
            headers: Vec::new(),
            status: 0,
            reason: String::new(),
            version: HttpVersion::Http11,
            body: Vec::new(),
            content_length: None,
            chunked: false,
        }
    }
    
    /// Parse response from a reader
    pub fn parse<R: std::io::BufRead>(reader: &mut R) -> io::Result<Http1Response> {
        let mut parser = Self::new();
        
        // Parse status line
        let mut line = String::new();
        reader.read_line(&mut line)?;
        parser.parse_status_line(&line)?;
        
        // Parse headers
        loop {
            line.clear();
            reader.read_line(&mut line)?;
            
            if line == "\r\n" || line == "\n" || line.is_empty() {
                break;
            }
            
            parser.parse_header_line(&line)?;
        }
        
        // Determine body handling
        parser.content_length = parser.headers.iter()
            .find(|(n, _)| n.eq_ignore_ascii_case("content-length"))
            .and_then(|(_, v)| v.parse().ok());
        
        parser.chunked = parser.headers.iter()
            .any(|(n, v)| n.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked"));
        
        // Read body
        if parser.chunked {
            parser.read_chunked_body(reader)?;
        } else if let Some(len) = parser.content_length {
            parser.body.resize(len, 0);
            reader.read_exact(&mut parser.body)?;
        }
        
        Ok(Http1Response {
            version: parser.version,
            status: parser.status,
            reason: parser.reason,
            headers: parser.headers,
            body: parser.body,
        })
    }
    
    fn parse_status_line(&mut self, line: &str) -> io::Result<()> {
        let line = line.trim_end();
        let mut parts = line.splitn(3, ' ');
        
        // Version
        let version_str = parts.next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing HTTP version"))?;
        
        self.version = match version_str {
            "HTTP/1.0" => HttpVersion::Http10,
            "HTTP/1.1" => HttpVersion::Http11,
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid HTTP version")),
        };
        
        // Status code
        let status_str = parts.next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing status code"))?;
        
        self.status = status_str.parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid status code"))?;
        
        // Reason phrase (optional)
        self.reason = parts.next().unwrap_or("").to_string();
        
        Ok(())
    }
    
    fn parse_header_line(&mut self, line: &str) -> io::Result<()> {
        let line = line.trim_end();
        
        if let Some(colon_pos) = line.find(':') {
            let name = line[..colon_pos].trim().to_string();
            let value = line[colon_pos + 1..].trim().to_string();
            self.headers.push((name, value));
        }
        
        Ok(())
    }
    
    fn read_chunked_body<R: std::io::BufRead>(&mut self, reader: &mut R) -> io::Result<()> {
        loop {
            // Read chunk size line
            let mut line = String::new();
            reader.read_line(&mut line)?;
            
            let size_str = line.trim();
            let size = usize::from_str_radix(size_str, 16)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid chunk size"))?;
            
            if size == 0 {
                // Read trailing CRLF
                reader.read_line(&mut line)?;
                break;
            }
            
            // Read chunk data
            let mut chunk = vec![0u8; size];
            reader.read_exact(&mut chunk)?;
            self.body.extend_from_slice(&chunk);
            
            // Read trailing CRLF
            line.clear();
            reader.read_line(&mut line)?;
        }
        
        Ok(())
    }
}

impl Default for Http1Parser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufReader;
    
    #[test]
    fn test_request_serialize() {
        let req = Http1Request::new("GET", "/api/test")
            .header("Host", "example.com")
            .header("Accept", "application/json");
        
        let bytes = req.serialize();
        let s = String::from_utf8(bytes).unwrap();
        
        assert!(s.starts_with("GET /api/test HTTP/1.1\r\n"));
        assert!(s.contains("Host: example.com\r\n"));
    }
    
    #[test]
    fn test_request_with_body() {
        let body = b"Hello, World!".to_vec();
        let req = Http1Request::new("POST", "/api/data")
            .header("Host", "example.com")
            .body(body.clone());
        
        let bytes = req.serialize();
        let s = String::from_utf8(bytes).unwrap();
        
        assert!(s.contains("Content-Length: 13\r\n"));
        assert!(s.ends_with("Hello, World!"));
    }
    
    #[test]
    fn test_response_parse() {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: 5\r\n\r\nHello";
        let mut reader = BufReader::new(response.as_bytes());
        
        let resp = Http1Parser::parse(&mut reader).unwrap();
        
        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, "OK");
        assert_eq!(resp.header("content-type"), Some("text/html"));
        assert_eq!(resp.body, b"Hello");
    }
    
    #[test]
    fn test_response_redirect() {
        let response = "HTTP/1.1 301 Moved Permanently\r\nLocation: https://new-url.com\r\n\r\n";
        let mut reader = BufReader::new(response.as_bytes());
        
        let resp = Http1Parser::parse(&mut reader).unwrap();
        
        assert!(resp.is_redirect());
        assert_eq!(resp.redirect_location(), Some("https://new-url.com"));
    }
}
