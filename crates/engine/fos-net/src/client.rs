//! HTTP Client
//!
//! Main HTTP client integrating TCP, TLS, HTTP/1.1, HTTP/2, HTTP/3, and cookies.
//! Replaces reqwest with a custom zero-dependency implementation.
//!
//! Connections are kept alive and reused per origin (HTTP/1.1 keep-alive and
//! HTTP/2), so repeat requests skip the TCP and TLS handshakes.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::time::{Duration, Instant};

use crate::tcp::{TcpConnection, TcpConfig};
use crate::tls::{TlsStream, TlsConfig};
use crate::http1::{Http1Request, Http1Parser};
use crate::http2::{Http2Connection, Frame, Http2Event};
use crate::cookies::{CookieContext, CookieJar, SharedCookieJar};
use crate::content_encoding;
use crate::quic::{AltSvc, AltSvcCache};
use crate::url_util;
use crate::{Response, NetError};

/// Read buffer size per connection
const CONNECTION_READ_BUFFER: usize = 8 * 1024;

/// Maximum idle connections kept per origin
const MAX_IDLE_PER_ORIGIN: usize = 2;

/// HTTP client configuration
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// User agent string
    pub user_agent: String,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Request timeout
    pub request_timeout: Duration,
    /// Max redirects to follow (0 = disable)
    pub max_redirects: u32,
    /// Enable cookies
    pub cookies_enabled: bool,
    /// Enable keep-alive
    pub keep_alive: bool,
    /// Default headers
    pub default_headers: Vec<(String, String)>,
    /// Enable HTTP/3 (when available via Alt-Svc)
    pub http3_enabled: bool,
    /// Prefer HTTP/3 over HTTP/2
    pub prefer_http3: bool,
    /// HTTP/3 idle timeout
    pub http3_idle_timeout: Duration,
    /// Maximum idle keep-alive connections kept across all origins
    pub max_idle_connections: usize,
    /// How long an idle connection is kept before it is closed
    pub idle_timeout: Duration,
    /// Largest body accepted after content decoding (gzip, br, zstd...),
    /// which stops decompression bombs
    pub max_decoded_body_size: usize,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            user_agent: "fOS-Engine/0.1".into(),
            connect_timeout: Duration::from_secs(30),
            request_timeout: Duration::from_secs(60),
            max_redirects: 10,
            cookies_enabled: true,
            keep_alive: true,
            default_headers: Vec::new(),
            http3_enabled: true,
            prefer_http3: false,
            http3_idle_timeout: Duration::from_secs(30),
            max_idle_connections: 8,
            idle_timeout: Duration::from_secs(30),
            max_decoded_body_size: content_encoding::DEFAULT_MAX_DECODED_SIZE,
        }
    }
}

/// HTTP client builder
pub struct HttpClientBuilder {
    config: ClientConfig,
    cookie_jar: Option<SharedCookieJar>,
}

impl HttpClientBuilder {
    pub fn new() -> Self {
        Self {
            config: ClientConfig::default(),
            cookie_jar: None,
        }
    }

    /// Keep cookies in `jar`, which other clients may share (by default
    /// each client has its own)
    pub fn cookie_jar(mut self, jar: SharedCookieJar) -> Self {
        self.cookie_jar = Some(jar);
        self
    }

    pub fn user_agent(mut self, ua: &str) -> Self {
        self.config.user_agent = ua.to_string();
        self
    }

    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.config.connect_timeout = timeout;
        self
    }

    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.config.request_timeout = timeout;
        self
    }

    pub fn max_redirects(mut self, max: u32) -> Self {
        self.config.max_redirects = max;
        self
    }

    pub fn cookie_store(mut self, enabled: bool) -> Self {
        self.config.cookies_enabled = enabled;
        self
    }

    pub fn default_header(mut self, name: &str, value: &str) -> Self {
        self.config.default_headers.push((name.to_string(), value.to_string()));
        self
    }

    /// Enable or disable HTTP/3 support
    pub fn http3(mut self, enabled: bool) -> Self {
        self.config.http3_enabled = enabled;
        self
    }

    /// Prefer HTTP/3 over HTTP/2 when available
    pub fn prefer_http3(mut self, prefer: bool) -> Self {
        self.config.prefer_http3 = prefer;
        self
    }

    /// Maximum idle keep-alive connections (0 disables connection reuse)
    pub fn max_idle_connections(mut self, max: usize) -> Self {
        self.config.max_idle_connections = max;
        self
    }

    /// Largest body accepted after content decoding
    pub fn max_decoded_body_size(mut self, max: usize) -> Self {
        self.config.max_decoded_body_size = max;
        self
    }

    pub fn build(self) -> HttpClient {
        let mut client = HttpClient::with_config(self.config);
        if let Some(jar) = self.cookie_jar {
            client.cookies = jar;
        }
        client
    }
}

impl Default for HttpClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Buffered bidirectional stream: reads go through a buffer, writes go
/// straight to the underlying stream.
struct BufStream<S: Read + Write> {
    inner: BufReader<S>,
}

impl<S: Read + Write> BufStream<S> {
    fn new(stream: S) -> Self {
        Self { inner: BufReader::with_capacity(CONNECTION_READ_BUFFER, stream) }
    }
}

impl<S: Read + Write> Read for BufStream<S> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<S: Read + Write> BufRead for BufStream<S> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner.consume(amt)
    }
}

impl<S: Read + Write> Write for BufStream<S> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.get_mut().write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.get_mut().flush()
    }
}

/// Plain TCP or TLS connection
enum Transport {
    Plain(BufStream<TcpConnection>),
    Tls(BufStream<TlsStream>),
}

impl Read for Transport {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(s) => s.read(buf),
            Transport::Tls(s) => s.read(buf),
        }
    }
}

impl BufRead for Transport {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        match self {
            Transport::Plain(s) => s.fill_buf(),
            Transport::Tls(s) => s.fill_buf(),
        }
    }

    fn consume(&mut self, amt: usize) {
        match self {
            Transport::Plain(s) => s.consume(amt),
            Transport::Tls(s) => s.consume(amt),
        }
    }
}

impl Write for Transport {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Transport::Plain(s) => s.write(buf),
            Transport::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Transport::Plain(s) => s.flush(),
            Transport::Tls(s) => s.flush(),
        }
    }
}

/// Application protocol spoken on a connection
enum Protocol {
    Http1,
    Http2(Box<Http2Connection>),
}

/// A keep-alive connection waiting to be reused
struct IdleConnection {
    transport: Transport,
    protocol: Protocol,
    idle_since: Instant,
}

/// HTTP client
pub struct HttpClient {
    /// Configuration
    config: ClientConfig,
    /// Cookie jar (possibly shared with other clients)
    cookies: SharedCookieJar,
    /// Idle keep-alive connections by origin (`https://host:443`)
    idle: HashMap<String, Vec<IdleConnection>>,
    /// Alt-Svc cache for HTTP/3 discovery
    alt_svc_cache: AltSvcCache,
}

impl HttpClient {
    /// Create a new HTTP client with default settings
    pub fn new() -> Self {
        Self::builder().build()
    }

    /// Create a client builder
    pub fn builder() -> HttpClientBuilder {
        HttpClientBuilder::new()
    }

    /// Create with custom config
    pub fn with_config(config: ClientConfig) -> Self {
        Self {
            config,
            cookies: CookieJar::shared(),
            idle: HashMap::new(),
            alt_svc_cache: AltSvcCache::new(),
        }
    }

    /// Make a GET request
    pub fn get(&mut self, url: &str) -> Result<Response, NetError> {
        self.request("GET", url, None, None)
    }

    /// Make a POST request
    pub fn post(&mut self, url: &str, body: Option<Vec<u8>>) -> Result<Response, NetError> {
        self.request("POST", url, None, body)
    }

    /// Make an HTTP request, following redirects, as a navigation the
    /// user started (it carries all of the URL's cookies).
    ///
    /// The returned response's `url` is the final URL after redirects.
    pub fn request(
        &mut self,
        method: &str,
        url: &str,
        headers: Option<Vec<(String, String)>>,
        body: Option<Vec<u8>>,
    ) -> Result<Response, NetError> {
        self.request_in(method, url, headers, body, &CookieContext::navigation(None, method))
    }

    /// Make an HTTP request in `context` (who it is made for), which
    /// decides the cookies each hop carries and the ones it may set.
    pub fn request_in(
        &mut self,
        method: &str,
        url: &str,
        headers: Option<Vec<(String, String)>>,
        body: Option<Vec<u8>>,
        context: &CookieContext,
    ) -> Result<Response, NetError> {
        let mut method = method.to_ascii_uppercase();
        let mut url = url.trim().to_string();
        let mut headers = headers.unwrap_or_default();
        let mut body = body;
        let mut redirects = 0;
        let mut cross_site_redirect = context.cross_site_redirect;

        loop {
            let parsed = UrlParts::parse(&url)?;
            let cookies = CookieContext { method: &method, cross_site_redirect, ..*context };
            let cookie_header = if self.config.cookies_enabled {
                self.jar().cookie_header(&url, &cookies)
            } else {
                None
            };
            let req = self.build_request(&method, &parsed, &headers, body.clone(), cookie_header);
            let mut response = self.execute_request(&parsed, req)?;

            if self.config.cookies_enabled {
                self.jar().store_response(&url, &response.headers, &cookies);
            }
            // A cross-site hop makes the rest of a redirect chain cross-site
            let mut chain = cookies;
            chain.follow(&url);
            cross_site_redirect = chain.cross_site_redirect;

            let location = match response.status {
                301 | 302 | 303 | 307 | 308 if redirects < self.config.max_redirects => {
                    response.header("location").map(str::to_owned)
                }
                _ => None,
            };
            let Some(location) = location else {
                self.decode_content(&method, &mut response)?;
                response.url = url;
                return Ok(response);
            };

            let mut next = url_util::resolve(&url, &location);
            // A Location without a fragment inherits the original one (RFC 9110 §10.2.2)
            if !next.contains('#') {
                if let Some(i) = url.find('#') {
                    next.push_str(&url[i..]);
                }
            }
            if UrlParts::parse(&next).is_err() {
                // Not an HTTP(S) target; let the caller handle the redirect
                self.decode_content(&method, &mut response)?;
                response.url = url;
                return Ok(response);
            }

            // 303 (and 301/302 after POST, as browsers do) switch to GET
            let to_get = (response.status == 303 && method != "HEAD")
                || (matches!(response.status, 301 | 302) && method == "POST");
            if to_get {
                method = "GET".to_string();
                body = None;
                headers.retain(|(name, _)| !is_content_header(name));
            }

            // Never forward credentials to a different origin
            if url_util::origin(&url) != url_util::origin(&next) {
                headers.retain(|(name, _)| {
                    !name.eq_ignore_ascii_case("authorization") && !name.eq_ignore_ascii_case("cookie")
                });
            }

            url = next;
            redirects += 1;
        }
    }

    fn jar(&self) -> std::sync::MutexGuard<'_, CookieJar> {
        self.cookies.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn build_request(
        &self,
        method: &str,
        url: &UrlParts,
        headers: &[(String, String)],
        body: Option<Vec<u8>>,
        cookie_header: Option<String>,
    ) -> Http1Request {
        let has = |name: &str| {
            headers.iter()
                .chain(&self.config.default_headers)
                .any(|(n, _)| n.eq_ignore_ascii_case(name))
        };

        let mut req = Http1Request::new(method, &url.path_and_query())
            .header("Host", &url.host_with_port());

        if !has("user-agent") {
            req = req.header("User-Agent", &self.config.user_agent);
        }
        if !has("accept") {
            req = req.header("Accept", "*/*");
        }
        if !has("accept-encoding") {
            // A range of a compressed body cannot be decoded on its own
            let accept = if has("range") { "identity" } else { content_encoding::accept_encoding(url.is_https) };
            req = req.header("Accept-Encoding", accept);
        }

        for (name, value) in self.config.default_headers.iter().chain(headers) {
            req = req.header(name, value);
        }

        if let Some(cookie_header) = cookie_header.filter(|_| !has("cookie")) {
            req = req.header("Cookie", &cookie_header);
        }

        if !has("connection") {
            req = req.header("Connection", if self.config.keep_alive { "keep-alive" } else { "close" });
        }

        if let Some(b) = body {
            req = req.body(b);
        }

        req
    }

    /// Undo the response's content codings, so callers always get the
    /// representation itself. Headers are left as received, as in Fetch.
    fn decode_content(&self, method: &str, response: &mut Response) -> Result<(), NetError> {
        // These responses have no content, whatever their headers say
        if method == "HEAD" || matches!(response.status, 100..=199 | 204 | 304) {
            return Ok(());
        }
        let Some(coding) = response.header("content-encoding") else {
            return Ok(());
        };
        let coding = coding.to_owned();
        let body = std::mem::take(&mut response.body);
        response.body = content_encoding::decode(body, &coding, self.config.max_decoded_body_size)
            .map_err(|e| NetError::Network(format!("content decoding failed: {e}")))?;
        Ok(())
    }

    fn execute_request(&mut self, url: &UrlParts, req: Http1Request) -> Result<Response, NetError> {
        // Check Alt-Svc cache for HTTP/3 support
        if self.config.http3_enabled && url.is_https {
            if let Some(alt_entry) = self.alt_svc_cache.get_h3(&url.host) {
                let h3_host = alt_entry.effective_host(&url.host);
                let h3_port = alt_entry.port;

                if let Ok(response) = self.try_http3(h3_host, h3_port, url, &req) {
                    return Ok(response);
                }
                // Fall back to HTTP/2 or HTTP/1.1
            }
        }

        let key = url.origin_key();
        let retry_safe = matches!(req.method.as_str(), "GET" | "HEAD" | "OPTIONS" | "PUT" | "DELETE" | "TRACE");

        let response = match self.take_idle(&key) {
            Some(idle) => match self.round_trip(&key, idle.transport, idle.protocol, url, &req) {
                Ok(response) => response,
                // The server may have closed the idle connection in the
                // meantime; idempotent requests are retried once on a new one
                Err(_) if retry_safe => {
                    let (transport, protocol) = self.connect(url)?;
                    self.round_trip(&key, transport, protocol, url, &req)?
                }
                Err(e) => return Err(e),
            },
            None => {
                let (transport, protocol) = self.connect(url)?;
                self.round_trip(&key, transport, protocol, url, &req)?
            }
        };

        // Parse Alt-Svc header for future HTTP/3 discovery
        if self.config.http3_enabled && url.is_https {
            for (name, value) in &response.headers {
                if name.eq_ignore_ascii_case("alt-svc") {
                    if let Some(alt_svc) = AltSvc::parse(value) {
                        self.alt_svc_cache.insert(&url.host, alt_svc);
                    }
                }
            }
        }

        Ok(response)
    }

    /// Open a new connection, negotiating HTTP/2 via ALPN for HTTPS
    fn connect(&self, url: &UrlParts) -> Result<(Transport, Protocol), NetError> {
        let tcp_config = TcpConfig {
            connect_timeout: self.config.connect_timeout,
            read_timeout: Some(self.config.request_timeout),
            write_timeout: Some(self.config.request_timeout),
            ..Default::default()
        };

        let addr = format!("{}:{}", url.host, url.port_or_default());
        let stream = TcpConnection::connect_with_config(&addr, tcp_config)
            .map_err(|e| NetError::Network(format!("Connection to {} failed: {}", addr, e)))?;

        if !url.is_https {
            return Ok((Transport::Plain(BufStream::new(stream)), Protocol::Http1));
        }

        let tls = TlsStream::connect(stream, url.tls_server_name(), TlsConfig::default())
            .map_err(|e| NetError::Network(format!("TLS handshake with {} failed: {}", url.host, e)))?;
        let is_h2 = tls.is_h2();
        let mut transport = Transport::Tls(BufStream::new(tls));

        if is_h2 {
            let mut h2 = Box::new(Http2Connection::new_client());
            h2.send_preface(&mut transport).map_err(h2_error)?;
            Ok((transport, Protocol::Http2(h2)))
        } else {
            Ok((transport, Protocol::Http1))
        }
    }

    /// Send a request and read its response, returning the connection to
    /// the pool when it can be reused
    fn round_trip(
        &mut self,
        key: &str,
        mut transport: Transport,
        protocol: Protocol,
        url: &UrlParts,
        req: &Http1Request,
    ) -> Result<Response, NetError> {
        match protocol {
            Protocol::Http1 => {
                req.write_to(&mut transport)
                    .map_err(|e| NetError::Network(format!("Write failed: {}", e)))?;

                let resp = Http1Parser::parse_response(&mut transport, &req.method)
                    .map_err(|e| NetError::Network(format!("Parse failed: {}", e)))?;

                // Only a response with explicit framing leaves the connection
                // positioned at the start of the next response
                let framed = resp.is_chunked()
                    || resp.content_length().is_some()
                    || req.method == "HEAD"
                    || resp.status == 204
                    || resp.status == 304;
                if self.config.keep_alive && framed && resp.keep_alive() {
                    self.put_idle(key, transport, Protocol::Http1);
                }

                Ok(Response {
                    status: resp.status,
                    headers: resp.headers,
                    body: resp.body,
                    url: String::new(),
                })
            }
            Protocol::Http2(mut h2) => {
                let (response, reusable) = h2_exchange(&mut transport, &mut h2, url, req)?;
                if self.config.keep_alive && reusable {
                    self.put_idle(key, transport, Protocol::Http2(h2));
                }
                Ok(response)
            }
        }
    }

    /// Take an idle connection for `key`, discarding expired ones
    fn take_idle(&mut self, key: &str) -> Option<IdleConnection> {
        let timeout = self.config.idle_timeout;
        let conns = self.idle.get_mut(key)?;
        conns.retain(|c| c.idle_since.elapsed() < timeout);
        let conn = conns.pop();
        if conns.is_empty() {
            self.idle.remove(key);
        }
        conn
    }

    /// Return a connection to the pool, evicting the oldest idle connections
    /// to stay within the configured budget
    fn put_idle(&mut self, key: &str, transport: Transport, protocol: Protocol) {
        let max = self.config.max_idle_connections;
        if max == 0 {
            return;
        }

        let timeout = self.config.idle_timeout;
        self.idle.retain(|_, conns| {
            conns.retain(|c| c.idle_since.elapsed() < timeout);
            !conns.is_empty()
        });

        if let Some(conns) = self.idle.get_mut(key) {
            if conns.len() >= MAX_IDLE_PER_ORIGIN {
                conns.remove(0);
            }
        }

        while self.idle.values().map(Vec::len).sum::<usize>() >= max {
            let oldest = self.idle.iter()
                .filter_map(|(k, conns)| conns.first().map(|c| (k.clone(), c.idle_since)))
                .min_by_key(|(_, since)| *since)
                .map(|(k, _)| k);
            let Some(oldest) = oldest else { break };
            if let Some(conns) = self.idle.get_mut(&oldest) {
                conns.remove(0);
                if conns.is_empty() {
                    self.idle.remove(&oldest);
                }
            }
        }

        self.idle.entry(key.to_string()).or_default().push(IdleConnection {
            transport,
            protocol,
            idle_since: Instant::now(),
        });
    }

    /// Close all idle connections (e.g. under memory pressure)
    pub fn clear_idle_connections(&mut self) {
        self.idle.clear();
    }

    /// Number of idle connections currently kept alive
    pub fn idle_connection_count(&self) -> usize {
        self.idle.values().map(Vec::len).sum()
    }

    /// Try HTTP/3 connection (returns error if not available)
    fn try_http3(&self, _host: &str, _port: u16, _url: &UrlParts, _req: &Http1Request) -> Result<Response, NetError> {
        // HTTP/3 requires async UDP - for now, return error to fall back
        // Full implementation would use smol::block_on with UdpSocket
        Err(NetError::Network("HTTP/3 requires async runtime".into()))
    }

    /// Send HTTP/3 request (for future async implementation)
    #[allow(dead_code)]
    fn build_h3_request(&self, url: &UrlParts, req: &Http1Request) -> Vec<(String, String)> {
        let mut headers = vec![
            (":method".to_string(), req.method.clone()),
            (":scheme".to_string(), if url.is_https { "https" } else { "http" }.to_string()),
            (":authority".to_string(), url.host_with_port()),
            (":path".to_string(), url.path_and_query()),
        ];

        for (name, value) in &req.headers {
            if !name.starts_with(':') && !name.eq_ignore_ascii_case("host") {
                headers.push((name.to_lowercase(), value.clone()));
            }
        }

        headers
    }

    /// Resolve a redirect `Location` against the request URL
    #[cfg(test)]
    fn resolve_redirect(base_url: &str, location: &str) -> String {
        url_util::resolve(base_url, location)
    }

    /// The cookie jar (possibly shared with other clients)
    pub fn cookie_jar(&self) -> &SharedCookieJar {
        &self.cookies
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

fn h2_error(e: impl std::fmt::Display) -> NetError {
    NetError::Network(format!("HTTP/2: {}", e))
}

/// Headers describing a request body, dropped when a redirect turns the
/// request into a body-less GET
fn is_content_header(name: &str) -> bool {
    ["content-type", "content-length", "content-encoding", "content-language", "content-location"]
        .iter()
        .any(|h| name.eq_ignore_ascii_case(h))
}

/// Perform one request/response exchange on an HTTP/2 connection.
///
/// Returns the response and whether the connection can be reused.
fn h2_exchange(
    stream: &mut Transport,
    h2: &mut Http2Connection,
    url: &UrlParts,
    req: &Http1Request,
) -> Result<(Response, bool), NetError> {
    let body = req.body.as_deref().filter(|b| !b.is_empty());

    let stream_id = h2.send_request(
        stream,
        &req.method,
        &url.path_and_query(),
        &url.host_with_port(),
        &req.headers,
        body.is_none(),
    ).map_err(h2_error)?;

    if let Some(body) = body {
        h2.send_data(stream, stream_id, body, true).map_err(h2_error)?;
    }

    let mut status: Option<u16> = None;
    let mut headers = Vec::new();
    let mut response_body = Vec::new();
    let mut reusable = true;
    let max_frame_size = h2.local_settings.max_frame_size;

    loop {
        let frame = Frame::read_from(stream, max_frame_size).map_err(h2_error)?;

        match h2.process_frame(frame).map_err(h2_error)? {
            Some(Http2Event::SettingsReceived) => {
                h2.send_settings_ack(stream).map_err(h2_error)?;
            }
            Some(Http2Event::Headers { stream_id: sid, headers: block, end_stream }) if sid == stream_id => {
                let block_status = block.iter()
                    .find(|(name, _)| name == ":status")
                    .and_then(|(_, value)| value.parse::<u16>().ok());

                match (status, block_status) {
                    // Informational responses (e.g. 103 Early Hints) precede the final one
                    (None, Some(s)) if (100..200).contains(&s) => {}
                    (None, Some(s)) => {
                        status = Some(s);
                        headers.extend(block.into_iter().filter(|(name, _)| !name.starts_with(':')));
                    }
                    (None, None) => return Err(h2_error("response without :status")),
                    // Trailers
                    (Some(_), _) => {
                        headers.extend(block.into_iter().filter(|(name, _)| !name.starts_with(':')));
                    }
                }

                if end_stream {
                    break;
                }
            }
            Some(Http2Event::Data { stream_id: sid, data, end_stream }) => {
                if sid == stream_id {
                    if response_body.is_empty() {
                        response_body = data;
                    } else {
                        response_body.extend_from_slice(&data);
                    }
                    if end_stream {
                        break;
                    }
                }
                // Return flow-control credit so the server keeps sending
                h2.replenish_windows(stream, sid).map_err(h2_error)?;
            }
            Some(Http2Event::Ping { ack: false, data }) => {
                h2.send_ping_ack(stream, data).map_err(h2_error)?;
            }
            Some(Http2Event::GoAway { last_stream_id, error_code }) => {
                reusable = false;
                if stream_id > last_stream_id {
                    return Err(h2_error("GOAWAY before the request was processed"));
                }
                if error_code != 0 {
                    return Err(h2_error(format!("GOAWAY: error {}", error_code)));
                }
                // Graceful shutdown: our stream still completes
            }
            Some(Http2Event::RstStream { stream_id: sid, error_code }) if sid == stream_id => {
                return Err(h2_error(format!("RST_STREAM: error {}", error_code)));
            }
            _ => {}
        }
    }

    h2.remove_stream(stream_id);
    // Keep the connection-level window topped up for the next request
    h2.replenish_windows(stream, 0).map_err(h2_error)?;

    let status = status.ok_or_else(|| h2_error("stream ended without a response"))?;
    Ok((
        Response {
            status,
            headers,
            body: response_body,
            url: String::new(),
        },
        reusable,
    ))
}

/// Simple URL parsing (for internal use)
#[derive(Debug, Clone)]
struct UrlParts {
    is_https: bool,
    /// Lowercased host; IPv6 literals keep their brackets
    host: String,
    port: Option<u16>,
    path: String,
    query: Option<String>,
}

impl UrlParts {
    fn parse(url: &str) -> Result<Self, NetError> {
        let url = url.trim();
        let (scheme, rest) = url.split_once("://")
            .ok_or_else(|| NetError::InvalidUrl(format!("Invalid scheme: {}", url)))?;

        let is_https = if scheme.eq_ignore_ascii_case("https") {
            true
        } else if scheme.eq_ignore_ascii_case("http") {
            false
        } else {
            return Err(NetError::InvalidUrl(format!("Invalid scheme: {}", url)));
        };

        // The fragment is never sent to the server
        let rest = rest.split('#').next().unwrap_or("");

        // Authority ends at the first '/' or '?'
        let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
        let (authority, path_query) = rest.split_at(authority_end);

        // Strip credentials (userinfo)
        let host_port = authority.rsplit_once('@').map_or(authority, |(_, hp)| hp);
        let (host, port) = split_host_port(host_port)?;
        if host.is_empty() {
            return Err(NetError::InvalidUrl(format!("Missing host: {}", url)));
        }

        let (path, query) = match path_query.split_once('?') {
            Some((p, q)) => (p, Some(q)),
            None => (path_query, None),
        };
        let path = if path.is_empty() { "/" } else { path };

        Ok(Self {
            is_https,
            host: host.to_ascii_lowercase(),
            port,
            path: encode_request_target(path),
            query: query.map(encode_request_target),
        })
    }

    fn path_and_query(&self) -> String {
        match &self.query {
            Some(q) => format!("{}?{}", self.path, q),
            None => self.path.clone(),
        }
    }

    fn host_with_port(&self) -> String {
        match self.port {
            Some(p) => format!("{}:{}", self.host, p),
            None => self.host.clone(),
        }
    }

    fn port_or_default(&self) -> u16 {
        self.port.unwrap_or(if self.is_https { 443 } else { 80 })
    }

    /// Connection pool key
    fn origin_key(&self) -> String {
        format!(
            "{}://{}:{}",
            if self.is_https { "https" } else { "http" },
            self.host,
            self.port_or_default()
        )
    }

    /// Host name for SNI and certificate verification (no IPv6 brackets)
    fn tls_server_name(&self) -> &str {
        self.host.trim_start_matches('[').trim_end_matches(']')
    }
}

/// Split `host[:port]`, handling bracketed IPv6 literals
fn split_host_port(s: &str) -> Result<(&str, Option<u16>), NetError> {
    let parse_port = |p: &str| -> Result<Option<u16>, NetError> {
        if p.is_empty() {
            Ok(None)
        } else {
            p.parse().map(Some).map_err(|_| NetError::InvalidUrl(format!("Invalid port: {}", p)))
        }
    };

    if s.starts_with('[') {
        let end = s.find(']')
            .ok_or_else(|| NetError::InvalidUrl(format!("Invalid IPv6 host: {}", s)))?;
        let port = match &s[end + 1..] {
            "" => None,
            rest => parse_port(rest.strip_prefix(':')
                .ok_or_else(|| NetError::InvalidUrl(format!("Invalid host: {}", s)))?)?,
        };
        Ok((&s[..=end], port))
    } else {
        match s.rsplit_once(':') {
            Some((host, port)) => Ok((host, parse_port(port)?)),
            None => Ok((s, None)),
        }
    }
}

/// Percent-encode bytes that may not appear raw in a request target
/// (spaces, controls, non-ASCII, and a few delimiters). Existing escapes
/// are left untouched.
fn encode_request_target(s: &str) -> String {
    let needs_encoding = |b: u8| {
        b <= b' ' || b >= 0x7F || matches!(b, b'"' | b'<' | b'>' | b'\\' | b'^' | b'`' | b'{' | b'|' | b'}')
    };

    if !s.bytes().any(needs_encoding) {
        return s.to_string();
    }

    let mut out = String::with_capacity(s.len() + 16);
    for b in s.bytes() {
        if needs_encoding(b) {
            out.push_str(&format!("%{:02X}", b));
        } else {
            out.push(b as char);
        }
    }
    out
}

// Blocking API for sync contexts
pub mod blocking {
    use super::*;

    /// Blocking HTTP client (for sync code)
    pub struct Client {
        inner: HttpClient,
    }

    impl Client {
        pub fn new() -> Self {
            Self {
                inner: HttpClient::new(),
            }
        }

        pub fn builder() -> ClientBuilder {
            ClientBuilder::new()
        }

        pub fn get(&mut self, url: &str) -> Result<Response, NetError> {
            self.inner.get(url)
        }

        pub fn post(&mut self, url: &str, body: Option<Vec<u8>>) -> Result<Response, NetError> {
            self.inner.post(url, body)
        }

        pub fn request(
            &mut self,
            method: &str,
            url: &str,
            headers: Option<Vec<(String, String)>>,
            body: Option<Vec<u8>>,
        ) -> Result<Response, NetError> {
            self.inner.request(method, url, headers, body)
        }

        /// Make a request in `context` (see [`HttpClient::request_in`])
        pub fn request_in(
            &mut self,
            method: &str,
            url: &str,
            headers: Option<Vec<(String, String)>>,
            body: Option<Vec<u8>>,
            context: &CookieContext,
        ) -> Result<Response, NetError> {
            self.inner.request_in(method, url, headers, body, context)
        }

        /// The cookie jar
        pub fn cookie_jar(&self) -> &SharedCookieJar {
            self.inner.cookie_jar()
        }

        /// Close idle keep-alive connections
        pub fn clear_idle_connections(&mut self) {
            self.inner.clear_idle_connections();
        }
    }

    impl Default for Client {
        fn default() -> Self {
            Self::new()
        }
    }

    pub struct ClientBuilder {
        inner: HttpClientBuilder,
    }

    impl ClientBuilder {
        pub fn new() -> Self {
            Self {
                inner: HttpClientBuilder::new(),
            }
        }

        pub fn user_agent(mut self, ua: &str) -> Self {
            self.inner = self.inner.user_agent(ua);
            self
        }

        pub fn timeout(mut self, timeout: Duration) -> Self {
            self.inner = self.inner.request_timeout(timeout);
            self
        }

        pub fn connect_timeout(mut self, timeout: Duration) -> Self {
            self.inner = self.inner.connect_timeout(timeout);
            self
        }

        pub fn default_header(mut self, name: &str, value: &str) -> Self {
            self.inner = self.inner.default_header(name, value);
            self
        }

        /// Keep cookies in `jar`, shared with other clients
        pub fn cookie_jar(mut self, jar: SharedCookieJar) -> Self {
            self.inner = self.inner.cookie_jar(jar);
            self
        }

        pub fn build(self) -> Result<Client, NetError> {
            Ok(Client {
                inner: self.inner.build(),
            })
        }
    }

    impl Default for ClientBuilder {
        fn default() -> Self {
            Self::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_url_parse() {
        let url = UrlParts::parse("https://example.com/path?query=1").unwrap();
        assert!(url.is_https);
        assert_eq!(url.host, "example.com");
        assert_eq!(url.path, "/path");
        assert_eq!(url.query, Some("query=1".to_string()));
    }

    #[test]
    fn test_url_with_port() {
        let url = UrlParts::parse("http://localhost:8080/api").unwrap();
        assert!(!url.is_https);
        assert_eq!(url.host, "localhost");
        assert_eq!(url.port, Some(8080));
    }

    #[test]
    fn test_url_parse_edge_cases() {
        // Fragments are never sent to the server
        let url = UrlParts::parse("https://en.wikipedia.org/wiki/Rust#History").unwrap();
        assert_eq!(url.path_and_query(), "/wiki/Rust");

        // Query without a path
        let url = UrlParts::parse("https://example.com?q=1").unwrap();
        assert_eq!(url.host, "example.com");
        assert_eq!(url.path_and_query(), "/?q=1");

        // IPv6 literal, with and without port
        let url = UrlParts::parse("http://[::1]:8080/x").unwrap();
        assert_eq!(url.host, "[::1]");
        assert_eq!(url.port, Some(8080));
        assert_eq!(url.tls_server_name(), "::1");
        let url = UrlParts::parse("http://[::1]/x").unwrap();
        assert_eq!(url.port, None);

        // Credentials are stripped, scheme and host are case-insensitive
        let url = UrlParts::parse("HTTPS://user:pw@Example.COM/").unwrap();
        assert!(url.is_https);
        assert_eq!(url.host, "example.com");

        // Unescaped spaces and non-ASCII are percent-encoded
        let url = UrlParts::parse("https://example.com/a b/é?q=a b").unwrap();
        assert_eq!(url.path_and_query(), "/a%20b/%C3%A9?q=a%20b");

        assert!(UrlParts::parse("ftp://example.com/").is_err());
        assert!(UrlParts::parse("https:///path").is_err());
        assert!(UrlParts::parse("http://host:notaport/").is_err());
    }

    #[test]
    fn test_client_builder() {
        let client = HttpClient::builder()
            .user_agent("TestAgent/1.0")
            .max_redirects(5)
            .build();

        assert_eq!(client.config.user_agent, "TestAgent/1.0");
        assert_eq!(client.config.max_redirects, 5);
    }

    #[test]
    fn test_redirect_resolution() {
        // Absolute URL
        assert_eq!(
            HttpClient::resolve_redirect("http://example.com/page", "https://other.com/new"),
            "https://other.com/new"
        );

        // Absolute path
        assert_eq!(
            HttpClient::resolve_redirect("http://example.com/old/path", "/new/path"),
            "http://example.com/new/path"
        );

        // Relative path against a bare origin
        assert_eq!(
            HttpClient::resolve_redirect("https://example.com", "login"),
            "https://example.com/login"
        );

        // Protocol-relative
        assert_eq!(
            HttpClient::resolve_redirect("https://example.com/a", "//www.example.com/b"),
            "https://www.example.com/b"
        );
    }

    /// Read one request head (requests in these tests have no body)
    fn read_request_head(reader: &mut impl BufRead) -> Option<String> {
        let mut head = String::new();
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return None,
                Ok(_) if line == "\r\n" => return Some(head),
                Ok(_) => head.push_str(&line),
            }
        }
    }

    /// Serve canned responses over HTTP/1.1 on a local port. Returns the base
    /// URL and a counter of accepted connections.
    fn serve(responses: Vec<&'static str>, close_after_each: bool) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let connections = Arc::new(AtomicUsize::new(0));
        let counter = connections.clone();

        thread::spawn(move || {
            let mut responses = responses.into_iter();
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                counter.fetch_add(1, Ordering::SeqCst);
                let mut reader = io::BufReader::new(stream.try_clone().unwrap());

                while read_request_head(&mut reader).is_some() {
                    let Some(response) = responses.next() else { return };
                    stream.write_all(response.as_bytes()).unwrap();
                    if close_after_each || response.contains("Connection: close") {
                        break;
                    }
                }
            }
        });

        (base, connections)
    }

    #[test]
    fn test_keep_alive_reuses_connection() {
        let ok = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhi";
        let (base, connections) = serve(vec![ok, ok, ok], false);

        let mut client = HttpClient::new();
        for _ in 0..3 {
            let response = client.get(&format!("{}/page", base)).unwrap();
            assert_eq!(response.body, b"hi");
        }
        assert_eq!(connections.load(Ordering::SeqCst), 1);
        assert_eq!(client.idle_connection_count(), 1);
    }

    #[test]
    fn test_follows_relative_redirect_and_reports_final_url() {
        let (base, connections) = serve(vec![
            "HTTP/1.1 302 Found\r\nLocation: /final?x=1\r\nContent-Length: 0\r\n\r\n",
            "HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\ndone",
        ], false);

        let mut client = HttpClient::new();
        let response = client.get(&format!("{}/start#section", base)).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body, b"done");
        assert_eq!(response.url, format!("{}/final?x=1#section", base));
        assert_eq!(connections.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_body_delimited_by_close_is_not_pooled() {
        let (base, _) = serve(vec![
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\nbody until close",
        ], false);

        let mut client = HttpClient::new();
        let response = client.get(&base).unwrap();
        assert_eq!(response.body, b"body until close");
        assert_eq!(client.idle_connection_count(), 0);
    }

    #[test]
    fn test_stale_pooled_connection_is_retried() {
        // The server silently closes every connection after one response
        let ok = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
        let (base, connections) = serve(vec![ok, ok], true);

        let mut client = HttpClient::new();
        assert_eq!(client.get(&base).unwrap().body, b"ok");
        // Give the server a moment to close its end
        thread::sleep(Duration::from_millis(50));
        assert_eq!(client.get(&base).unwrap().body, b"ok");
        assert_eq!(connections.load(Ordering::SeqCst), 2);
    }

    /// Serve one binary response on a local port. The handle yields the
    /// request head the client sent.
    fn serve_once(response: Vec<u8>) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = io::BufReader::new(stream.try_clone().unwrap());
            let head = read_request_head(&mut reader).unwrap();
            stream.write_all(&response).unwrap();
            head.to_ascii_lowercase()
        });
        (base, server)
    }

    fn encoded_response(coding: &str, body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Encoding: {}\r\nContent-Length: {}\r\n\r\n",
            coding,
            body.len()
        ).into_bytes();
        response.extend_from_slice(body);
        response
    }

    #[test]
    fn test_compressed_body_is_decoded() {
        let sample = include_bytes!("../tests/data/sample.html");
        let gz = include_bytes!("../tests/data/sample.html.gz");
        let (base, server) = serve_once(encoded_response("gzip", gz));

        let response = HttpClient::new().get(&base).unwrap();
        assert_eq!(response.body, sample);
        // Headers stay as received
        assert_eq!(response.header("content-encoding"), Some("gzip"));
        // Brotli and zstd are only offered over HTTPS
        assert!(server.join().unwrap().contains("accept-encoding: gzip, deflate\r\n"));
    }

    #[test]
    fn test_decoded_body_size_is_capped() {
        let gz = include_bytes!("../tests/data/sample.html.gz");
        let (base, _server) = serve_once(encoded_response("gzip", gz));

        let mut client = HttpClient::builder().max_decoded_body_size(1024).build();
        assert!(client.get(&base).is_err());
    }

    #[test]
    fn test_range_requests_ask_for_identity() {
        let (base, server) = serve_once(b"HTTP/1.1 206 Partial Content\r\nContent-Length: 2\r\n\r\nhi".to_vec());

        let headers = vec![("Range".to_string(), "bytes=0-1".to_string())];
        let response = HttpClient::new().request("GET", &base, Some(headers), None).unwrap();
        assert_eq!(response.body, b"hi");
        assert!(server.join().unwrap().contains("accept-encoding: identity\r\n"));
    }

    #[test]
    fn test_redirect_loop_is_bounded() {
        let redirect = "HTTP/1.1 302 Found\r\nLocation: /again\r\nContent-Length: 0\r\n\r\n";
        let (base, _) = serve(vec![redirect; 4], false);

        let mut client = HttpClient::builder().max_redirects(3).build();
        let response = client.get(&base).unwrap();
        assert_eq!(response.status, 302);
    }
}
