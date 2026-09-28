//! HTTP/1.1 Framing
//!
//! Request serialization and response parsing for HTTP/1.1.

use std::io::{self, BufRead, Read, Write};

/// Maximum size of the status line plus all header lines. Protects against
/// servers (or attackers) streaming unbounded header data.
const MAX_HEADER_BYTES: usize = 256 * 1024;

/// Maximum number of interim (1xx) responses tolerated before the final one
const MAX_INTERIM_RESPONSES: usize = 8;

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
        let headers_len: usize = self.headers.iter().map(|(n, v)| n.len() + v.len() + 4).sum();
        let body_len = self.body.as_ref().map_or(0, Vec::len);
        let mut buf = Vec::with_capacity(self.method.len() + self.path.len() + headers_len + body_len + 64);

        // Request line
        buf.extend_from_slice(format!("{} {} {}\r\n", self.method, self.path, self.version).as_bytes());

        // Headers
        for (name, value) in &self.headers {
            buf.extend_from_slice(name.as_bytes());
            buf.extend_from_slice(b": ");
            buf.extend_from_slice(value.as_bytes());
            buf.extend_from_slice(b"\r\n");
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
            .and_then(parse_content_length)
    }

    /// Check if chunked transfer encoding
    pub fn is_chunked(&self) -> bool {
        self.header("transfer-encoding")
            .map(is_chunked_encoding)
            .unwrap_or(false)
    }

    /// Check if connection should be kept alive
    pub fn keep_alive(&self) -> bool {
        let has_token = |token: &str| {
            self.header("connection")
                .map(|v| v.split(',').any(|t| t.trim().eq_ignore_ascii_case(token)))
                .unwrap_or(false)
        };

        if self.version == HttpVersion::Http10 {
            // HTTP/1.0: keep-alive only if explicitly requested
            has_token("keep-alive")
        } else {
            // HTTP/1.1: keep-alive by default unless "close"
            !has_token("close")
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

/// Whether a Transfer-Encoding value ends with the `chunked` coding
/// (e.g. `gzip, chunked`), which determines message framing.
fn is_chunked_encoding(value: &str) -> bool {
    value.rsplit(',')
        .next()
        .map(|last| last.trim().eq_ignore_ascii_case("chunked"))
        .unwrap_or(false)
}

/// Parse a Content-Length value. Repeated identical values (`42, 42`), which
/// some proxies produce, are accepted; conflicting values are rejected.
fn parse_content_length(value: &str) -> Option<usize> {
    let mut result = None;
    for part in value.split(',') {
        let n: usize = part.trim().parse().ok()?;
        match result {
            Some(prev) if prev != n => return None,
            _ => result = Some(n),
        }
    }
    result
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
    /// Bytes of status line + headers consumed so far
    header_bytes: usize,
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
            header_bytes: 0,
        }
    }

    /// Parse the response to a GET request from a reader
    pub fn parse<R: BufRead>(reader: &mut R) -> io::Result<Http1Response> {
        Self::parse_response(reader, "GET")
    }

    /// Parse a response, using the request method to determine whether a
    /// body follows (RFC 9112 §6.3).
    ///
    /// Interim 1xx responses (e.g. `100 Continue`, `103 Early Hints`) are
    /// skipped. Bodies without Content-Length or chunked framing are read
    /// until the server closes the connection.
    pub fn parse_response<R: BufRead>(reader: &mut R, request_method: &str) -> io::Result<Http1Response> {
        let mut interim = 0;
        loop {
            let mut parser = Self::new();
            parser.read_head(reader)?;

            // Skip interim responses, except 101 Switching Protocols which
            // hands the connection over to another protocol
            if (100..200).contains(&parser.status) && parser.status != 101 {
                interim += 1;
                if interim > MAX_INTERIM_RESPONSES {
                    return Err(invalid_data("too many interim responses"));
                }
                continue;
            }

            let no_body = request_method.eq_ignore_ascii_case("HEAD")
                || (100..200).contains(&parser.status)
                || parser.status == 204
                || parser.status == 304;

            parser.state = ParseState::Body;
            if no_body {
                // No message body
            } else if parser.chunked {
                parser.state = ParseState::ChunkedBody;
                parser.read_chunked_body(reader)?;
            } else if let Some(len) = parser.content_length {
                parser.body.resize(len, 0);
                reader.read_exact(&mut parser.body)?;
            } else {
                // Delimited by connection close
                reader.read_to_end(&mut parser.body)?;
            }
            parser.state = ParseState::Complete;

            return Ok(Http1Response {
                version: parser.version,
                status: parser.status,
                reason: parser.reason,
                headers: parser.headers,
                body: parser.body,
            });
        }
    }

    /// Read the status line and header section
    fn read_head<R: BufRead>(&mut self, reader: &mut R) -> io::Result<()> {
        // Status line (tolerate stray empty lines before it, RFC 9112 §2.2)
        let mut line = String::new();
        loop {
            if !self.read_line(reader, &mut line)? {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "connection closed before response"));
            }
            if !line.trim().is_empty() {
                break;
            }
        }
        self.parse_status_line(&line)?;
        self.state = ParseState::Headers;

        // Headers
        loop {
            if !self.read_line(reader, &mut line)? || line.trim_end_matches(['\r', '\n']).is_empty() {
                break;
            }
            self.parse_header_line(&line)?;
        }

        // Determine body framing. Transfer-Encoding overrides Content-Length.
        self.chunked = self.headers.iter()
            .any(|(n, v)| n.eq_ignore_ascii_case("transfer-encoding") && is_chunked_encoding(v));

        if !self.chunked {
            if let Some((_, v)) = self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("content-length")) {
                self.content_length = Some(parse_content_length(v)
                    .ok_or_else(|| invalid_data("invalid Content-Length"))?);
            }
        }

        Ok(())
    }

    /// Read one line (bytes up to and including `\n`), decoding lossily so a
    /// stray non-UTF-8 byte in a header cannot fail the whole response.
    /// Returns `false` at end of stream.
    fn read_line<R: BufRead>(&mut self, reader: &mut R, line: &mut String) -> io::Result<bool> {
        let mut bytes = Vec::new();
        let n = reader.by_ref()
            .take((MAX_HEADER_BYTES - self.header_bytes + 1) as u64)
            .read_until(b'\n', &mut bytes)?;
        self.header_bytes += n;
        if self.header_bytes > MAX_HEADER_BYTES {
            return Err(invalid_data("response header section too large"));
        }

        line.clear();
        line.push_str(&String::from_utf8_lossy(&bytes));
        Ok(n > 0)
    }

    fn parse_status_line(&mut self, line: &str) -> io::Result<()> {
        let line = line.trim();
        let mut parts = line.splitn(3, ' ');

        // Version
        let version_str = parts.next()
            .ok_or_else(|| invalid_data("Missing HTTP version"))?;

        self.version = match version_str {
            "HTTP/1.0" => HttpVersion::Http10,
            "HTTP/1.1" => HttpVersion::Http11,
            _ => return Err(invalid_data("Invalid HTTP version")),
        };

        // Status code
        let status_str = parts.next()
            .ok_or_else(|| invalid_data("Missing status code"))?;

        self.status = status_str.parse()
            .ok()
            .filter(|s| (100..1000).contains(s))
            .ok_or_else(|| invalid_data("Invalid status code"))?;

        // Reason phrase (optional)
        self.reason = parts.next().unwrap_or("").to_string();

        Ok(())
    }

    fn parse_header_line(&mut self, line: &str) -> io::Result<()> {
        let line = line.trim_end_matches(['\r', '\n']);

        // Obsolete line folding: continuation of the previous header value
        if line.starts_with([' ', '\t']) {
            if let Some((_, value)) = self.headers.last_mut() {
                value.push(' ');
                value.push_str(line.trim());
            }
            return Ok(());
        }

        if let Some(colon_pos) = line.find(':') {
            let name = line[..colon_pos].trim().to_string();
            let value = line[colon_pos + 1..].trim().to_string();
            if !name.is_empty() {
                self.headers.push((name, value));
            }
        }

        Ok(())
    }

    fn read_chunked_body<R: BufRead>(&mut self, reader: &mut R) -> io::Result<()> {
        let mut line = String::new();
        loop {
            // Chunk size line: hex size, optionally followed by extensions
            if !read_raw_line(reader, &mut line)? {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated chunked body"));
            }

            let size_str = line.split(';').next().unwrap_or("").trim();
            let size = usize::from_str_radix(size_str, 16)
                .map_err(|_| invalid_data("Invalid chunk size"))?;

            if size == 0 {
                // Trailer section ends with an empty line
                while read_raw_line(reader, &mut line)? && !line.trim().is_empty() {}
                break;
            }

            // Read chunk data directly into the body buffer
            let start = self.body.len();
            let end = start.checked_add(size)
                .ok_or_else(|| invalid_data("chunk size overflow"))?;
            self.body.resize(end, 0);
            reader.read_exact(&mut self.body[start..])?;

            // Read trailing CRLF
            read_raw_line(reader, &mut line)?;
        }

        Ok(())
    }
}

/// Read a short protocol line (chunk sizes, trailers), lossily decoded
fn read_raw_line<R: BufRead>(reader: &mut R, line: &mut String) -> io::Result<bool> {
    let mut bytes = Vec::new();
    let n = reader.by_ref().take(8 * 1024).read_until(b'\n', &mut bytes)?;
    line.clear();
    line.push_str(&String::from_utf8_lossy(&bytes));
    Ok(n > 0)
}

fn invalid_data(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_string())
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

    fn parse(raw: &[u8]) -> io::Result<Http1Response> {
        Http1Parser::parse(&mut BufReader::new(raw))
    }

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
        let resp = parse(response.as_bytes()).unwrap();

        assert_eq!(resp.status, 200);
        assert_eq!(resp.reason, "OK");
        assert_eq!(resp.header("content-type"), Some("text/html"));
        assert_eq!(resp.body, b"Hello");
    }

    #[test]
    fn test_response_redirect() {
        let response = "HTTP/1.1 301 Moved Permanently\r\nLocation: https://new-url.com\r\nContent-Length: 0\r\n\r\n";
        let resp = parse(response.as_bytes()).unwrap();

        assert!(resp.is_redirect());
        assert_eq!(resp.redirect_location(), Some("https://new-url.com"));
    }

    #[test]
    fn test_body_until_close() {
        // No Content-Length and not chunked: body runs until EOF
        let response = "HTTP/1.0 200 OK\r\nContent-Type: text/html\r\n\r\n<html>page</html>";
        let resp = parse(response.as_bytes()).unwrap();
        assert_eq!(resp.body, b"<html>page</html>");
        assert!(!resp.keep_alive());
    }

    #[test]
    fn test_head_and_304_have_no_body() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 1234\r\n\r\n";
        let resp = Http1Parser::parse_response(&mut BufReader::new(&response[..]), "HEAD").unwrap();
        assert!(resp.body.is_empty());
        assert_eq!(resp.content_length(), Some(1234));

        let response = b"HTTP/1.1 304 Not Modified\r\nETag: \"x\"\r\n\r\n";
        assert!(parse(response).unwrap().body.is_empty());
    }

    #[test]
    fn test_interim_responses_skipped() {
        let response = "HTTP/1.1 100 Continue\r\n\r\n\
                        HTTP/1.1 103 Early Hints\r\nLink: </style.css>; rel=preload\r\n\r\n\
                        HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
        let resp = parse(response.as_bytes()).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"ok");
    }

    #[test]
    fn test_chunked_with_extensions_and_trailers() {
        let response = "HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n\
                        5;ext=1\r\nHello\r\n\
                        7\r\n, World\r\n\
                        0\r\nX-Trailer: yes\r\n\r\n";
        let resp = parse(response.as_bytes()).unwrap();
        assert!(resp.is_chunked());
        assert_eq!(resp.body, b"Hello, World");
    }

    #[test]
    fn test_non_utf8_header_is_tolerated() {
        let mut response = b"HTTP/1.1 200 OK\r\nContent-Disposition: attachment; filename=\"".to_vec();
        response.extend_from_slice(&[0xE9, b'.', b't', b'x', b't']); // Latin-1 'é'
        response.extend_from_slice(b"\"\r\nContent-Length: 2\r\n\r\nok");
        let resp = parse(&response).unwrap();
        assert_eq!(resp.body, b"ok");
        assert!(resp.header("content-disposition").is_some());
    }

    #[test]
    fn test_content_length_variants() {
        assert_eq!(parse_content_length("42"), Some(42));
        assert_eq!(parse_content_length("42, 42"), Some(42));
        assert_eq!(parse_content_length("42, 43"), None);
        assert_eq!(parse_content_length("abc"), None);
    }

    #[test]
    fn test_connection_header_tokens() {
        let resp = parse(b"HTTP/1.1 200 OK\r\nConnection: Keep-Alive, Close\r\nContent-Length: 0\r\n\r\n").unwrap();
        assert!(!resp.keep_alive());
        let resp = parse(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n").unwrap();
        assert!(resp.keep_alive());
    }

    #[test]
    fn test_status_line_without_reason() {
        let resp = parse(b"HTTP/1.1 204\r\n\r\n").unwrap();
        assert_eq!(resp.status, 204);
        assert!(parse(b"HTTP/1.1 abc OK\r\n\r\n").is_err());
    }
}
