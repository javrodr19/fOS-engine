//! Network requests made by page scripts (`fetch`, `XMLHttpRequest`)
//!
//! Requests run on a small pool of background threads, spawned only when a
//! page first makes a request and growing with the number in flight, so
//! scripts never block the event loop and pages that never fetch cost
//! nothing. Finished requests wait in a channel until the page's event loop
//! collects them; a waker lets the browser wake up for them at once.
//!
//! The Fetch standard's rules a page must not be able to bypass are
//! enforced here: forbidden request headers are dropped, cross-origin
//! responses are only readable when the server allows it (CORS, with
//! preflight for non-simple requests), cookies follow the credentials
//! mode and SameSite rules (in the browser's shared cookie jar), and
//! secure pages cannot load insecure resources.

use std::collections::VecDeque;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use fos_net::client::HttpClient;
use fos_net::url_util;
use fos_net::{CookieContext, CookieJar, SharedCookieJar};

/// Wakes the browser's event loop when a request finishes
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// Most requests a page has running at once
const MAX_WORKERS: usize = 6;
/// Most redirects followed for one request (as in the Fetch standard)
const MAX_REDIRECTS: u32 = 20;
/// Largest response body kept (larger responses are network errors)
const MAX_BODY: usize = 64 << 20;

/// How a request treats other origins (`Request.mode`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestMode {
    Cors,
    NoCors,
    SameOrigin,
}

/// When cookies are sent and stored (`Request.credentials`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credentials {
    Omit,
    SameOrigin,
    Include,
}

/// What happens on a redirect (`Request.redirect`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectMode {
    Follow,
    Error,
    Manual,
}

/// A request from a page
#[derive(Debug, Clone)]
pub struct ScriptRequest {
    pub id: u32,
    pub method: String,
    /// Absolute URL
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub mode: RequestMode,
    pub credentials: Credentials,
    pub redirect: RedirectMode,
    /// URL of the page making the request (its origin and the referrer)
    pub page_url: String,
}

/// `Response.type`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseType {
    Basic,
    Cors,
    Opaque,
    OpaqueRedirect,
}

impl ResponseType {
    pub fn as_str(self) -> &'static str {
        match self {
            ResponseType::Basic => "basic",
            ResponseType::Cors => "cors",
            ResponseType::Opaque => "opaque",
            ResponseType::OpaqueRedirect => "opaqueredirect",
        }
    }
}

/// A response as the page may see it (already filtered by CORS)
#[derive(Debug, Clone)]
pub struct ScriptResponse {
    pub status: u16,
    pub status_text: String,
    /// Final URL, after redirects
    pub url: String,
    pub redirected: bool,
    pub kind: ResponseType,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// A finished request: its response, or why it failed (a network error,
/// which scripts see as `TypeError: Failed to fetch`)
#[derive(Debug)]
pub struct Completion {
    pub id: u32,
    pub result: Result<ScriptResponse, String>,
}

/// State the workers share
struct Shared {
    requests: Mutex<Receiver<ScriptRequest>>,
    cookies: SharedCookieJar,
    waker: Mutex<Option<Waker>>,
}

/// The requests of one page
pub struct FetchPool {
    shared: Arc<Shared>,
    to_workers: Sender<ScriptRequest>,
    from_workers: Receiver<Completion>,
    completions_tx: Sender<Completion>,
    /// Completions received while waiting for a particular one
    ready: VecDeque<Completion>,
    workers: usize,
    in_flight: usize,
    /// Client for synchronous requests, made on the page's thread
    sync_client: Option<HttpClient>,
}

impl Default for FetchPool {
    fn default() -> Self {
        Self::new(CookieJar::shared())
    }
}

impl FetchPool {
    /// A pool whose requests keep cookies in `cookies` (the browser's jar)
    pub fn new(cookies: SharedCookieJar) -> Self {
        let (to_workers, requests) = mpsc::channel();
        let (completions_tx, from_workers) = mpsc::channel();
        Self {
            shared: Arc::new(Shared {
                requests: Mutex::new(requests),
                cookies,
                waker: Mutex::new(None),
            }),
            to_workers,
            from_workers,
            completions_tx,
            ready: VecDeque::new(),
            workers: 0,
            in_flight: 0,
            sync_client: None,
        }
    }

    /// Call `waker` whenever a request finishes
    pub fn set_waker(&self, waker: Waker) {
        *self.shared.waker.lock().unwrap_or_else(|p| p.into_inner()) = Some(waker);
    }

    /// Start a request; its completion arrives through `poll`
    pub fn start(&mut self, request: ScriptRequest) {
        self.in_flight += 1;
        if self.workers < self.in_flight.min(MAX_WORKERS) {
            self.spawn_worker();
        }
        // Workers only stop when the pool is dropped
        let _ = self.to_workers.send(request);
    }

    fn spawn_worker(&mut self) {
        let shared = self.shared.clone();
        let done = self.completions_tx.clone();
        let spawned = std::thread::Builder::new()
            .name("fos-fetch".into())
            // TLS handshakes and decompression need a little more than the
            // minimum, far less than the 2 MiB default
            .stack_size(512 * 1024)
            .spawn(move || worker(shared, done));
        match spawned {
            Ok(_) => self.workers += 1,
            Err(e) => log::error!("Cannot start a network thread: {e}"),
        }
    }

    /// Run a request on the calling thread (synchronous XMLHttpRequest)
    pub fn run_sync(&mut self, request: ScriptRequest) -> Completion {
        let client = self.sync_client.get_or_insert_with(new_client);
        let id = request.id;
        Completion { id, result: perform(client, &self.shared.cookies, &request) }
    }

    /// Requests started and not yet collected
    pub fn pending(&self) -> usize {
        self.in_flight
    }

    /// The requests that finished since the last call
    pub fn poll(&mut self) -> Vec<Completion> {
        let mut out: Vec<Completion> = self.ready.drain(..).collect();
        out.extend(self.from_workers.try_iter());
        self.in_flight -= out.len().min(self.in_flight);
        out
    }

    /// Block until a request finishes or `timeout` passes (for callers
    /// without an event loop, like tests and headless tools)
    pub fn wait(&mut self, timeout: Duration) -> bool {
        if !self.ready.is_empty() {
            return true;
        }
        if self.in_flight == 0 {
            return false;
        }
        match self.from_workers.recv_timeout(timeout) {
            Ok(c) => {
                self.ready.push_back(c);
                true
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => false,
        }
    }
}

fn new_client() -> HttpClient {
    HttpClient::builder()
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) fOS/0.1 (KHTML, like Gecko)")
        // Redirects and cookies are handled here, per hop, as Fetch requires
        .max_redirects(0)
        .cookie_store(false)
        .request_timeout(Duration::from_secs(60))
        .max_decoded_body_size(MAX_BODY)
        .build()
}

fn worker(shared: Arc<Shared>, done: Sender<Completion>) {
    let mut client = new_client();
    loop {
        let request = {
            let rx = shared.requests.lock().unwrap_or_else(|p| p.into_inner());
            match rx.recv() {
                Ok(r) => r,
                // The page is gone
                Err(_) => return,
            }
        };
        let start = Instant::now();
        let result = perform(&mut client, &shared.cookies, &request);
        log::debug!("{} {} -> {:?} in {:?}", request.method, request.url, result.as_ref().map(|r| r.status), start.elapsed());
        if done.send(Completion { id: request.id, result }).is_err() {
            return;
        }
        let waker = shared.waker.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(w) = waker {
            w();
        }
    }
}

// ---- the fetch algorithm ----

/// `scheme://host[:port]` of `url`, or "null" for opaque origins
pub fn serialize_origin(url: &str) -> String {
    let lower = url.trim_start().to_ascii_lowercase();
    if !(lower.starts_with("http:") || lower.starts_with("https:")) {
        return "null".to_string();
    }
    match url_util::origin(url) {
        Some((scheme, host, port)) => {
            let default = match scheme.as_str() {
                "http" => 80,
                "https" => 443,
                _ => 0,
            };
            if port == default {
                format!("{scheme}://{host}")
            } else {
                format!("{scheme}://{host}:{port}")
            }
        }
        None => "null".to_string(),
    }
}

fn scheme_of(url: &str) -> String {
    url.split_once(':').map(|(s, _)| s.to_ascii_lowercase()).unwrap_or_default()
}

/// Headers a page may not set (the browser controls them)
fn is_forbidden_request_header(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "accept-charset" | "accept-encoding" | "access-control-request-headers" | "access-control-request-method"
            | "connection" | "content-length" | "cookie" | "cookie2" | "date" | "dnt" | "expect" | "host"
            | "keep-alive" | "origin" | "referer" | "set-cookie" | "te" | "trailer" | "transfer-encoding"
            | "upgrade" | "via"
    ) || n.starts_with("proxy-")
        || n.starts_with("sec-")
}

/// Response headers a cross-origin page can read without being told
/// (CORS-safelisted response header names)
fn is_safelisted_response_header(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    matches!(
        n.as_str(),
        "cache-control" | "content-language" | "content-length" | "content-type" | "expires" | "last-modified" | "pragma"
    )
}

/// Whether a cross-origin request can go without a preflight
fn is_simple_request(method: &str, headers: &[(String, String)]) -> bool {
    if !matches!(method, "GET" | "HEAD" | "POST") {
        return false;
    }
    headers.iter().all(|(name, value)| match name.to_ascii_lowercase().as_str() {
        "accept" | "accept-language" | "content-language" => value.len() <= 128,
        "content-type" => {
            let essence = value.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
            matches!(essence.as_str(), "application/x-www-form-urlencoded" | "multipart/form-data" | "text/plain")
        }
        _ => false,
    })
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
}

/// Whether the response allows the page's origin to read it
fn cors_allows(headers: &[(String, String)], origin: &str, credentials: bool) -> bool {
    let Some(allow) = header(headers, "access-control-allow-origin").map(str::trim) else { return false };
    if credentials {
        allow == origin && header(headers, "access-control-allow-credentials").is_some_and(|v| v.trim() == "true")
    } else {
        allow == "*" || allow == origin
    }
}

fn status_text(status: u16) -> &'static str {
    match status {
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        204 => "No Content",
        206 => "Partial Content",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        410 => "Gone",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    }
}

/// Run `request` to completion
fn perform(client: &mut HttpClient, cookies: &SharedCookieJar, request: &ScriptRequest) -> Result<ScriptResponse, String> {
    let origin = serialize_origin(&request.page_url);
    let scheme = scheme_of(&request.url);
    match scheme.as_str() {
        "data" => return data_url(&request.url),
        "file" => {
            // Only local pages may read local files
            if scheme_of(&request.page_url) != "file" {
                return Err(format!("Not allowed to load local resource: {}", request.url));
            }
            return file_url(&request.url);
        }
        "http" | "https" => {}
        "about" if request.url.eq_ignore_ascii_case("about:blank") => {
            return Ok(ScriptResponse {
                status: 200,
                status_text: "OK".into(),
                url: request.url.clone(),
                redirected: false,
                kind: ResponseType::Basic,
                headers: vec![("content-type".into(), "text/html;charset=utf-8".into())],
                body: Vec::new(),
            });
        }
        _ => return Err(format!("URL scheme \"{scheme}\" is not supported")),
    }

    let method = request.method.to_ascii_uppercase();
    if matches!(method.as_str(), "CONNECT" | "TRACE" | "TRACK") {
        return Err(format!("'{method}' HTTP method is unsupported"));
    }
    let headers: Vec<(String, String)> =
        request.headers.iter().filter(|(n, _)| !is_forbidden_request_header(n)).cloned().collect();

    let mut url = request.url.clone();
    let mut method = method;
    let mut body = request.body.clone();
    let mut redirected = false;
    // Once a request leaves the page's origin it stays "cross-origin" (and
    // the origin it presents becomes opaque after a cross-origin redirect)
    let mut tainted = false;
    let mut preflighted = false;
    // Whether the redirect chain went through a site other than the page's
    let mut cross_site_redirect = false;

    for _hop in 0..=MAX_REDIRECTS {
        // Mixed content: a secure page never loads insecure resources
        if scheme_of(&request.page_url) == "https" && scheme_of(&url) == "http" && !is_localhost(&url) {
            return Err(format!("Mixed Content: the page at '{}' requested an insecure resource '{url}'", request.page_url));
        }
        let same_origin = !tainted && serialize_origin(&url) == origin;
        if !same_origin {
            match request.mode {
                RequestMode::SameOrigin => return Err(format!("Request to {url} blocked: mode is 'same-origin'")),
                RequestMode::NoCors if !matches!(method.as_str(), "GET" | "HEAD" | "POST") => {
                    return Err(format!("'{method}' is unsupported in no-cors mode"));
                }
                _ => {}
            }
        }
        let cors = !same_origin && request.mode == RequestMode::Cors;
        let send_credentials = match request.credentials {
            Credentials::Include => true,
            Credentials::SameOrigin => same_origin,
            Credentials::Omit => false,
        };

        if cors && !preflighted && !is_simple_request(&method, &headers) {
            preflight(client, &url, &origin, &method, &headers, send_credentials)?;
            preflighted = true;
        }

        let mut hop_headers = headers.clone();
        if cors || (method != "GET" && method != "HEAD") {
            hop_headers.push(("Origin".into(), if tainted { "null".into() } else { origin.clone() }));
        }
        if let Some(referrer) = referrer_for(&request.page_url, &url) {
            hop_headers.push(("Referer".into(), referrer));
        }
        let mut cookie_context = CookieContext::subresource(Some(&request.page_url), &method);
        cookie_context.cross_site_redirect = cross_site_redirect;
        if send_credentials {
            let jar = cookies.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(c) = jar.cookie_header(&url, &cookie_context) {
                hop_headers.push(("Cookie".into(), c));
            }
        }

        let response = client.request(&method, &url, Some(hop_headers), body.clone()).map_err(|e| e.to_string())?;
        if send_credentials {
            cookies.lock().unwrap_or_else(|p| p.into_inner()).store_response(&url, &response.headers, &cookie_context);
        }
        cookie_context.follow(&url);
        cross_site_redirect = cookie_context.cross_site_redirect;

        let location = matches!(response.status, 301 | 302 | 303 | 307 | 308)
            .then(|| response.header("location").map(str::to_owned))
            .flatten();
        if let Some(location) = location {
            match request.redirect {
                RedirectMode::Error => return Err(format!("Redirect from {url} was blocked (redirect mode 'error')")),
                RedirectMode::Manual => {
                    return Ok(ScriptResponse {
                        status: 0,
                        status_text: String::new(),
                        url: url.clone(),
                        redirected: false,
                        kind: ResponseType::OpaqueRedirect,
                        headers: Vec::new(),
                        body: Vec::new(),
                    });
                }
                RedirectMode::Follow => {}
            }
            // A cross-origin response that redirects must itself pass CORS
            if cors && !cors_allows(&response.headers, if tainted { "null" } else { &origin }, send_credentials) {
                return Err(format!("CORS: redirect from {url} is not allowed for origin {origin}"));
            }
            let mut next = url_util::resolve(&url, &location);
            if !next.contains('#') {
                if let Some(i) = url.find('#') {
                    next.push_str(&url[i..]);
                }
            }
            if !matches!(scheme_of(&next).as_str(), "http" | "https") {
                return Err(format!("Redirect to a non-HTTP URL: {next}"));
            }
            if serialize_origin(&next) != serialize_origin(&url) && serialize_origin(&url) != origin {
                tainted = true;
            }
            let to_get = (response.status == 303 && method != "HEAD") || (matches!(response.status, 301 | 302) && method == "POST");
            if to_get {
                method = "GET".into();
                body = None;
            }
            url = next;
            redirected = true;
            continue;
        }

        // The final response
        let status = response.status;
        let mut kind = ResponseType::Basic;
        let mut headers = response.headers;
        let mut body = response.body;
        if !same_origin {
            if request.mode == RequestMode::NoCors {
                return Ok(ScriptResponse {
                    status: 0,
                    status_text: String::new(),
                    url: String::new(),
                    redirected: false,
                    kind: ResponseType::Opaque,
                    headers: Vec::new(),
                    body: Vec::new(),
                });
            }
            let presented = if tainted { "null" } else { origin.as_str() };
            if !cors_allows(&headers, presented, send_credentials) {
                return Err(format!(
                    "CORS: {url} has no 'Access-Control-Allow-Origin' header allowing origin {origin}"
                ));
            }
            kind = ResponseType::Cors;
            let exposed: Vec<String> = header(&headers, "access-control-expose-headers")
                .map(|v| v.split(',').map(|h| h.trim().to_ascii_lowercase()).collect())
                .unwrap_or_default();
            let expose_all = exposed.iter().any(|h| h == "*") && !send_credentials;
            headers.retain(|(n, _)| is_safelisted_response_header(n) || expose_all || exposed.contains(&n.to_ascii_lowercase()));
        }
        // Scripts never see cookies
        headers.retain(|(n, _)| !n.eq_ignore_ascii_case("set-cookie") && !n.eq_ignore_ascii_case("set-cookie2"));
        if method == "HEAD" {
            body.clear();
        }
        return Ok(ScriptResponse {
            status,
            status_text: status_text(status).to_string(),
            url,
            redirected,
            kind,
            headers,
            body,
        });
    }
    Err(format!("Too many redirects fetching {}", request.url))
}

/// Ask the server whether a non-simple cross-origin request is allowed
fn preflight(
    client: &mut HttpClient,
    url: &str,
    origin: &str,
    method: &str,
    headers: &[(String, String)],
    credentials: bool,
) -> Result<(), String> {
    // The non-safelisted headers the request carries
    let mut names: Vec<String> = headers
        .iter()
        .filter(|(n, v)| !is_simple_request("GET", &[(n.clone(), v.clone())]))
        .map(|(n, _)| n.to_ascii_lowercase())
        .collect();
    names.sort();
    names.dedup();
    let mut pre = vec![
        ("Origin".to_string(), origin.to_string()),
        ("Access-Control-Request-Method".to_string(), method.to_string()),
    ];
    if !names.is_empty() {
        pre.push(("Access-Control-Request-Headers".to_string(), names.join(",")));
    }
    let response = client.request("OPTIONS", url, Some(pre), None).map_err(|e| e.to_string())?;
    if !(200..300).contains(&response.status) || !cors_allows(&response.headers, origin, credentials) {
        return Err(format!("CORS: preflight for {url} was rejected (status {})", response.status));
    }
    let list = |name: &str| -> Vec<String> {
        header(&response.headers, name)
            .map(|v| v.split(',').map(|s| s.trim().to_ascii_lowercase()).filter(|s| !s.is_empty()).collect())
            .unwrap_or_default()
    };
    let methods = list("access-control-allow-methods");
    let wildcard_ok = !credentials;
    let method_ok = matches!(method, "GET" | "HEAD" | "POST")
        || methods.iter().any(|m| m.eq_ignore_ascii_case(method))
        || (wildcard_ok && methods.iter().any(|m| m == "*"));
    if !method_ok {
        return Err(format!("CORS: method {method} is not allowed by {url}"));
    }
    let allowed = list("access-control-allow-headers");
    for n in names {
        if !allowed.contains(&n) && !(wildcard_ok && allowed.iter().any(|h| h == "*")) {
            return Err(format!("CORS: request header '{n}' is not allowed by {url}"));
        }
    }
    Ok(())
}

/// The `Referer` sent with a request (strict-origin-when-cross-origin)
pub(crate) fn referrer_for(page_url: &str, target: &str) -> Option<String> {
    let page_scheme = scheme_of(page_url);
    if !matches!(page_scheme.as_str(), "http" | "https") {
        return None;
    }
    // Never from a secure page to an insecure one
    if page_scheme == "https" && scheme_of(target) == "http" {
        return None;
    }
    let without_fragment = page_url.split('#').next().unwrap_or(page_url);
    if serialize_origin(page_url) == serialize_origin(target) {
        // Credentials in the URL are never sent, and the URL is sent
        // serialized (`https://a.com` as `https://a.com/`: servers check)
        let url = strip_userinfo(without_fragment);
        Some(match url.split_once("://") {
            Some((scheme, rest)) if !rest[rest.find(['/', '?']).unwrap_or(rest.len())..].starts_with('/') => {
                let end = rest.find('?').unwrap_or(rest.len());
                format!("{scheme}://{}/{}", &rest[..end], &rest[end..])
            }
            _ => url,
        })
    } else {
        Some(format!("{}/", serialize_origin(page_url)))
    }
}

fn strip_userinfo(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else { return url.to_string() };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    match authority.rsplit_once('@') {
        Some((_, host)) => format!("{scheme}://{host}{}", &rest[end..]),
        None => url.to_string(),
    }
}

fn is_localhost(url: &str) -> bool {
    url_util::origin(url).is_some_and(|(_, host, _)| host == "localhost" || host == "127.0.0.1" || host == "[::1]")
}

// ---- local schemes ----

fn data_url(url: &str) -> Result<ScriptResponse, String> {
    let rest = &url[5..];
    let (meta, payload) = rest.split_once(',').ok_or_else(|| "Invalid data: URL".to_string())?;
    let base64 = meta.to_ascii_lowercase().trim_end().ends_with(";base64");
    let mime = meta.trim_end().strip_suffix(";base64").or_else(|| meta.trim_end().strip_suffix(";BASE64")).unwrap_or(meta);
    let mime = if mime.trim().is_empty() { "text/plain;charset=US-ASCII".to_string() } else { mime.trim().to_string() };
    let bytes = percent_decode_bytes(payload);
    let body = if base64 {
        let clean: Vec<u8> = bytes.into_iter().filter(|b| !b.is_ascii_whitespace()).collect();
        base64_decode(&clean).ok_or_else(|| "Invalid base64 in data: URL".to_string())?
    } else {
        bytes
    };
    Ok(ScriptResponse {
        status: 200,
        status_text: "OK".into(),
        url: url.to_string(),
        redirected: false,
        kind: ResponseType::Basic,
        headers: vec![("content-type".into(), mime)],
        body,
    })
}

fn file_url(url: &str) -> Result<ScriptResponse, String> {
    let path = crate::loader::file_url_to_path(url).ok_or_else(|| format!("Invalid file URL: {url}"))?;
    let body = std::fs::read(&path).map_err(|e| format!("Cannot read {}: {e}", path.display()))?;
    let mime = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("html" | "htm") => "text/html",
        Some("js" | "mjs") => "text/javascript",
        Some("json") => "application/json",
        Some("css") => "text/css",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("txt") => "text/plain",
        Some("xml") => "application/xml",
        _ => "application/octet-stream",
    };
    Ok(ScriptResponse {
        status: 200,
        status_text: "OK".into(),
        url: url.to_string(),
        redirected: false,
        kind: ResponseType::Basic,
        headers: vec![("content-type".into(), mime.into()), ("content-length".into(), body.len().to_string())],
        body,
    })
}

fn percent_decode_bytes(s: &str) -> Vec<u8> {
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

/// Decode standard base64 (padding optional); `None` if malformed
pub fn base64_decode(input: &[u8]) -> Option<Vec<u8>> {
    let input = input.strip_suffix(b"==").or_else(|| input.strip_suffix(b"=")).unwrap_or(input);
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut bits = 0u32;
    let mut nbits = 0;
    for &c in input {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        };
        bits = bits << 6 | v as u32;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push((bits >> nbits) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins() {
        assert_eq!(serialize_origin("https://a.com/x?y"), "https://a.com");
        assert_eq!(serialize_origin("https://a.com:443/"), "https://a.com");
        assert_eq!(serialize_origin("http://a.com:8080/"), "http://a.com:8080");
        assert_eq!(serialize_origin("file:///tmp/x.html"), "null");
        assert_eq!(serialize_origin("data:text/plain,hi"), "null");
    }

    #[test]
    fn request_classification() {
        assert!(is_simple_request("GET", &[]));
        assert!(is_simple_request("POST", &[("Content-Type".into(), "text/plain; charset=utf-8".into())]));
        assert!(!is_simple_request("POST", &[("Content-Type".into(), "application/json".into())]));
        assert!(!is_simple_request("PUT", &[]));
        assert!(!is_simple_request("GET", &[("X-Token".into(), "1".into())]));
        assert!(is_forbidden_request_header("Cookie"));
        assert!(is_forbidden_request_header("sec-fetch-mode"));
        assert!(!is_forbidden_request_header("X-Requested-With"));
    }

    #[test]
    fn referrers() {
        assert_eq!(referrer_for("https://u:p@a.com/p?q#f", "https://a.com/x").as_deref(), Some("https://a.com/p?q"));
        assert_eq!(referrer_for("https://a.com/p", "https://b.com/x").as_deref(), Some("https://a.com/"));
        assert_eq!(referrer_for("https://a.com", "https://a.com/x").as_deref(), Some("https://a.com/"));
        assert_eq!(referrer_for("https://a.com?q", "https://a.com/x").as_deref(), Some("https://a.com/?q"));
        assert_eq!(referrer_for("https://a.com?r=/x", "https://a.com/x").as_deref(), Some("https://a.com/?r=/x"));
        assert_eq!(referrer_for("https://a.com/p", "http://b.com/x"), None);
        assert_eq!(referrer_for("file:///x.html", "https://b.com/x"), None);
    }

    #[test]
    fn data_urls() {
        let r = data_url("data:text/plain;base64,aGVsbG8=").unwrap();
        assert_eq!(r.body, b"hello");
        assert_eq!(header(&r.headers, "content-type"), Some("text/plain"));
        let r = data_url("data:,a%20b").unwrap();
        assert_eq!(r.body, b"a b");
        assert_eq!(header(&r.headers, "content-type"), Some("text/plain;charset=US-ASCII"));
        assert!(data_url("data:text/plain").is_err());
    }
}
