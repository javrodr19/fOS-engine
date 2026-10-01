//! Cookies (RFC 6265bis)
//!
//! One jar holds the cookies of a browser session. The HTTP client, the
//! requests page scripts make and `document.cookie` share it (as a
//! [`SharedCookieJar`]), so the session cookie a login page sets goes with
//! that page's `fetch` calls, and a cookie a script sets goes with the next
//! navigation.
//!
//! The jar enforces what keeps cookies from leaking between sites:
//! - **Domain**: without a Domain attribute a cookie is host-only. With
//!   one it also reaches subdomains, but only if the host setting it is
//!   inside that domain and the domain is not a public suffix, so no site
//!   can set cookies for all of "co.uk" or "github.io".
//! - **Path**: the default path comes from the request URL; paths match
//!   whole segments ("/a" matches "/a/b", not "/ab").
//! - **Expiry**: Max-Age wins over Expires, dates are read as browsers read
//!   them, lifetimes are capped at 400 days, and a past expiry deletes.
//! - **Secure** cookies are only set from and sent to secure URLs, and
//!   insecure URLs cannot overwrite or shadow them. The `__Secure-` and
//!   `__Host-` name prefixes are enforced.
//! - **HttpOnly** cookies are invisible to scripts, which cannot overwrite
//!   or shadow them.
//! - **SameSite**: a cross-site request only carries SameSite=None cookies
//!   (which must be Secure); a cross-site top-level navigation with GET
//!   also carries Lax ones. Cookies without the attribute count as Lax,
//!   except that for two minutes after being set they also go with
//!   cross-site top-level POSTs (as in Chromium, so sign-in flows that post
//!   back keep working). Cross-site responses cannot set Lax or Strict
//!   cookies.
//! - **Partitioned** cookies (CHIPS) are kept per top-level site.
//! - **Limits**: 4 KiB per cookie, 180 per domain, 3000 in all; the
//!   oldest go first.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::psl;
use crate::url_util;

/// Largest name plus value
const MAX_COOKIE_SIZE: usize = 4096;
/// Longer attribute values are ignored
const MAX_ATTRIBUTE_SIZE: usize = 1024;
/// Longest lifetime a cookie can ask for
const MAX_AGE_SECS: u64 = 400 * 24 * 60 * 60;
const MAX_PER_DOMAIN: usize = 180;
const MAX_COOKIES: usize = 3000;
/// How long a cookie without SameSite still goes with cross-site
/// top-level POST navigations
const LAX_ALLOWING_UNSAFE_MS: u64 = 2 * 60 * 1000;

/// A cookie jar several threads use
pub type SharedCookieJar = Arc<Mutex<CookieJar>>;

/// The SameSite attribute
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SameSite {
    /// Sent with cross-site requests too (requires Secure)
    None,
    /// Sent with same-site requests and cross-site top-level navigations
    Lax,
    /// Sent with same-site requests only
    Strict,
    /// No (valid) attribute: Lax, with the two-minute POST allowance
    #[default]
    Unset,
}

/// A stored cookie
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cookie {
    pub name: String,
    pub value: String,
    /// Domain it belongs to (lowercase, no leading dot)
    pub domain: String,
    /// Sent only to `domain` itself, not to its subdomains
    pub host_only: bool,
    pub path: String,
    /// When it expires (seconds since the Unix epoch); None for cookies
    /// that last the session
    pub expires: Option<u64>,
    pub secure: bool,
    pub http_only: bool,
    pub same_site: SameSite,
    /// For partitioned cookies, the top-level site they belong to
    /// ("https://example.com")
    pub partition: Option<String>,
    /// When it was first set (milliseconds since the Unix epoch)
    pub created: u64,
}

impl Cookie {
    /// `name=value`, as sent in a Cookie header
    pub fn serialize(&self) -> String {
        if self.name.is_empty() {
            self.value.clone()
        } else {
            format!("{}={}", self.name, self.value)
        }
    }

    fn expired(&self, now: u64) -> bool {
        self.expires.is_some_and(|e| e <= now)
    }

    fn same_entry(&self, other: &Cookie) -> bool {
        self.name == other.name
            && self.domain == other.domain
            && self.host_only == other.host_only
            && self.path == other.path
            && self.partition == other.partition
    }
}

/// What made a request
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CookieSource {
    /// A top-level navigation: an address typed, a link followed, a form
    /// submitted
    Navigation,
    /// A subresource, or a script's `fetch` or `XMLHttpRequest`
    Subresource,
    /// A script reading or writing `document.cookie`
    Script,
}

/// Where a request comes from, which decides the cookies it carries and
/// the ones it may set
#[derive(Debug, Clone, Copy)]
pub struct CookieContext<'a> {
    /// The page behind the request: for navigations, the page a link was
    /// followed from; otherwise the top-level page. None for requests the
    /// user started (an address typed, a bookmark), which are same-site.
    pub initiator: Option<&'a str>,
    pub source: CookieSource,
    /// Request method, which decides whether a cross-site navigation
    /// carries Lax cookies
    pub method: &'a str,
    /// The request was redirected through a URL cross-site to the
    /// initiator (which makes the rest of the chain cross-site)
    pub cross_site_redirect: bool,
}

impl<'a> CookieContext<'a> {
    /// A top-level navigation, from `initiator` (None: started by the user)
    pub fn navigation(initiator: Option<&'a str>, method: &'a str) -> Self {
        Self { initiator, source: CookieSource::Navigation, method, cross_site_redirect: false }
    }

    /// A request a page at `page` makes (subresources, fetch); None for
    /// requests without a page
    pub fn subresource(page: Option<&'a str>, method: &'a str) -> Self {
        Self { initiator: page, source: CookieSource::Subresource, method, cross_site_redirect: false }
    }

    /// `document.cookie` of the page at `page`
    pub fn script(page: &'a str) -> Self {
        Self { initiator: Some(page), source: CookieSource::Script, method: "GET", cross_site_redirect: false }
    }

    /// Whether a request to `url` in this context is same-site
    pub fn is_same_site(&self, url: &str) -> bool {
        match self.initiator {
            None => true,
            Some(initiator) => !self.cross_site_redirect && psl::same_site(initiator, url),
        }
    }

    /// Note that the request is about to go to `url` (a redirect), which
    /// may make it cross-site for the rest of its chain
    pub fn follow(&mut self, url: &str) {
        if !self.is_same_site(url) {
            self.cross_site_redirect = true;
        }
    }

    /// The top-level site a request to `url` is made for (partitions)
    fn top_level_site(&self, url: &str) -> Option<String> {
        let page = match (self.source, self.initiator) {
            (CookieSource::Navigation, _) | (_, None) => url,
            (_, Some(page)) => page,
        };
        psl::site(page).map(|(scheme, site)| format!("{scheme}://{site}"))
    }

    fn safe_method(&self) -> bool {
        matches!(self.method.to_ascii_uppercase().as_str(), "GET" | "HEAD" | "OPTIONS" | "TRACE")
    }
}

/// The parts of a request URL cookies care about
struct Target<'a> {
    host: String,
    path: &'a str,
    secure: bool,
}

impl<'a> Target<'a> {
    fn parse(url: &'a str) -> Option<Self> {
        let (scheme, host, _) = url_util::origin(url)?;
        if !matches!(scheme.as_str(), "http" | "https" | "ws" | "wss") || host.is_empty() {
            return None;
        }
        let host = host.strip_suffix('.').map(str::to_string).unwrap_or(host);
        let secure = matches!(scheme.as_str(), "https" | "wss") || is_loopback(&host);
        Some(Target { host, path: url_util::path(url), secure })
    }
}

/// Loopback hosts count as secure, as browsers treat them
fn is_loopback(host: &str) -> bool {
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "[::1]"
        || host.parse::<std::net::Ipv4Addr>().is_ok_and(|ip| ip.is_loopback())
}

fn now() -> (u64, u64) {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    (d.as_secs(), d.as_millis() as u64)
}

/// Whether `host` is `domain` or a subdomain of it
pub fn domain_match(host: &str, domain: &str) -> bool {
    host == domain
        || (host.len() > domain.len()
            && host.ends_with(domain)
            && host.as_bytes()[host.len() - domain.len() - 1] == b'.'
            && !psl::is_ip_address(host))
}

/// Whether a cookie with path `cookie_path` goes with `request_path`
pub fn path_match(request_path: &str, cookie_path: &str) -> bool {
    request_path == cookie_path
        || (request_path.starts_with(cookie_path)
            && (cookie_path.ends_with('/') || request_path.as_bytes().get(cookie_path.len()) == Some(&b'/')))
}

/// A cookie's path when it does not give one: the request path's directory
fn default_path(path: &str) -> String {
    match path.rfind('/') {
        Some(i) if i > 0 && path.starts_with('/') => path[..i].to_string(),
        _ => "/".to_string(),
    }
}

fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
    s.len() >= prefix.len() && s.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

fn trim_ws(s: &str) -> &str {
    s.trim_matches([' ', '\t'])
}

/// A Set-Cookie value (or a string assigned to `document.cookie`) as
/// received from `target`, before the jar's checks
struct Parsed {
    cookie: Cookie,
    partitioned: bool,
    /// It had a Path attribute
    path_given: bool,
}

fn parse(line: &str, target: &Target, now: u64, now_ms: u64) -> Option<Parsed> {
    // Control characters other than tab invalidate the whole line
    if line.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f) {
        return None;
    }
    let (pair, attributes) = line.split_once(';').unwrap_or((line, ""));
    let (name, value) = match pair.split_once('=') {
        Some((n, v)) => (trim_ws(n), trim_ws(v)),
        None => ("", trim_ws(pair)),
    };
    if (name.is_empty() && value.is_empty()) || name.len() + value.len() > MAX_COOKIE_SIZE {
        return None;
    }

    let mut expires = None;
    let mut max_age: Option<i64> = None;
    let mut domain: Option<String> = None;
    let mut path: Option<String> = None;
    let (mut secure, mut http_only, mut partitioned, mut path_given) = (false, false, false, false);
    let mut same_site = SameSite::Unset;
    for attribute in attributes.split(';') {
        let (key, val) = attribute.split_once('=').unwrap_or((attribute, ""));
        let (key, val) = (trim_ws(key), trim_ws(val));
        if val.len() > MAX_ATTRIBUTE_SIZE {
            continue;
        }
        match key.to_ascii_lowercase().as_str() {
            "expires" => {
                if let Some(t) = parse_cookie_date(val) {
                    expires = Some(t);
                }
            }
            "max-age" => {
                let (negative, digits) = val.strip_prefix('-').map_or((false, val), |d| (true, d));
                if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                    let n = digits.parse::<i64>().unwrap_or(i64::MAX);
                    max_age = Some(if negative { -n } else { n });
                }
            }
            "domain" => {
                domain = (!val.is_empty()).then(|| val.strip_prefix('.').unwrap_or(val).to_ascii_lowercase());
            }
            "path" => {
                path = val.starts_with('/').then(|| val.to_string());
                path_given = true;
            }
            "secure" => secure = true,
            "httponly" => http_only = true,
            "partitioned" => partitioned = true,
            "samesite" => {
                same_site = match val.to_ascii_lowercase().as_str() {
                    "none" => SameSite::None,
                    "lax" => SameSite::Lax,
                    "strict" => SameSite::Strict,
                    _ => SameSite::Unset,
                }
            }
            _ => {}
        }
    }

    let latest = now + MAX_AGE_SECS;
    let expires = match (max_age, expires) {
        (Some(age), _) if age <= 0 => Some(0),
        (Some(age), _) => Some(now.saturating_add(age as u64).min(latest)),
        (None, Some(t)) => Some(t.min(latest)),
        (None, None) => None,
    };

    let host = &target.host;
    let (domain, host_only) = match domain {
        // A public suffix may only name the host itself, as a host-only cookie
        Some(d) if !(d == *host && psl::is_public_suffix(&d)) => {
            if psl::is_public_suffix(&d) || !domain_match(host, &d) {
                return None;
            }
            (d, false)
        }
        _ => (host.clone(), true),
    };

    Some(Parsed {
        cookie: Cookie {
            name: name.to_string(),
            value: value.to_string(),
            domain,
            host_only,
            path: path.unwrap_or_else(|| default_path(target.path)),
            expires,
            secure,
            http_only,
            same_site,
            partition: None,
            created: now_ms,
        },
        partitioned,
        path_given,
    })
}

/// A date in a cookie's Expires attribute (RFC 6265 §5.1.1), which
/// accepts the many formats servers send. Seconds since the Unix epoch.
pub fn parse_cookie_date(s: &str) -> Option<u64> {
    let delimiter =
        |c: u8| c == 0x09 || (0x20..=0x2f).contains(&c) || (0x3b..=0x40).contains(&c) || (0x5b..=0x60).contains(&c) || (0x7b..=0x7e).contains(&c);
    // Leading digits of a token, if there are `min..=max` of them
    let number = |t: &[u8], min: usize, max: usize| -> Option<u32> {
        let n = t.iter().take_while(|b| b.is_ascii_digit()).count();
        (min..=max).contains(&n).then(|| t[..n].iter().fold(0, |acc, b| acc * 10 + u32::from(b - b'0')))
    };
    let time = |t: &[u8]| -> Option<(u32, u32, u32)> {
        let mut fields = [0u32; 3];
        let mut rest = t;
        for (i, field) in fields.iter_mut().enumerate() {
            let n = rest.iter().take_while(|b| b.is_ascii_digit()).count();
            if !(1..=2).contains(&n) {
                return None;
            }
            *field = number(rest, 1, 2)?;
            rest = &rest[n..];
            if i < 2 {
                rest = rest.strip_prefix(b":")?;
            }
        }
        Some((fields[0], fields[1], fields[2]))
    };
    const MONTHS: [&[u8]; 12] = [b"jan", b"feb", b"mar", b"apr", b"may", b"jun", b"jul", b"aug", b"sep", b"oct", b"nov", b"dec"];

    let (mut hms, mut day, mut month, mut year) = (None, None, None, None);
    for token in s.as_bytes().split(|&c| delimiter(c)).filter(|t| !t.is_empty()) {
        if hms.is_none() {
            if let Some(t) = time(token) {
                hms = Some(t);
                continue;
            }
        }
        if day.is_none() {
            if let Some(d) = number(token, 1, 2) {
                day = Some(d);
                continue;
            }
        }
        if month.is_none() && token.len() >= 3 {
            if let Some(m) = MONTHS.iter().position(|m| token[..3].eq_ignore_ascii_case(m)) {
                month = Some(m as u32 + 1);
                continue;
            }
        }
        if year.is_none() {
            if let Some(y) = number(token, 2, 4) {
                year = Some(y);
            }
        }
    }
    let ((h, m, sec), day, month, mut year) = (hms?, day?, month?, year?);
    if (70..=99).contains(&year) {
        year += 1900;
    } else if year <= 69 {
        year += 2000;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month as usize - 1];
    if day < 1 || day > month_days || year < 1601 || h > 23 || m > 59 || sec > 59 {
        return None;
    }
    let days = days_from_civil(i64::from(year), month, day);
    let secs = days * 86_400 + i64::from(h * 3600 + m * 60 + sec);
    Some(secs.max(0) as u64)
}

/// Days from 1970-01-01 to a date of the proleptic Gregorian calendar
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let year_of_era = y - era * 400;
    let month_from_march = (i64::from(month) + 9) % 12;
    let day_of_year = (153 * month_from_march + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The cookies of a browser session
#[derive(Debug, Default)]
pub struct CookieJar {
    /// Cookies by domain, in the order they were first set
    domains: HashMap<String, Vec<Cookie>>,
    count: usize,
}

impl CookieJar {
    pub fn new() -> Self {
        Self::default()
    }

    /// A new, empty jar to share between threads
    pub fn shared() -> SharedCookieJar {
        Arc::new(Mutex::new(Self::new()))
    }

    /// Store a cookie received from `url` (a Set-Cookie value, or a string
    /// a script assigned to `document.cookie`). Returns whether the jar
    /// changed: false when the cookie was rejected.
    pub fn set(&mut self, url: &str, line: &str, context: &CookieContext) -> bool {
        let Some(target) = Target::parse(url) else { return false };
        let (now, now_ms) = now();
        let Some(Parsed { mut cookie, partitioned, path_given }) = parse(line, &target, now, now_ms) else { return false };
        let from_script = context.source == CookieSource::Script;

        if cookie.http_only && from_script {
            return false;
        }
        if cookie.secure && !target.secure {
            return false;
        }
        if starts_with_ignore_case(&cookie.name, "__Secure-") && !cookie.secure {
            return false;
        }
        if starts_with_ignore_case(&cookie.name, "__Host-")
            && !(cookie.secure && cookie.host_only && path_given && cookie.path == "/")
        {
            return false;
        }
        // A nameless cookie must not look like a prefixed one
        if cookie.name.is_empty()
            && (starts_with_ignore_case(&cookie.value, "__Secure-") || starts_with_ignore_case(&cookie.value, "__Host-"))
        {
            return false;
        }
        match cookie.same_site {
            SameSite::None if !cookie.secure => return false,
            SameSite::None => {}
            // Cross-site subresources cannot set Lax or Strict cookies
            _ if context.source != CookieSource::Navigation && !context.is_same_site(url) => return false,
            _ => {}
        }
        if partitioned {
            if !cookie.secure {
                return false;
            }
            cookie.partition = context.top_level_site(url);
        }
        // An insecure URL cannot overwrite or shadow a Secure cookie, nor
        // a script an HttpOnly one (say, to fix a session id)
        let insecure = !target.secure && !cookie.secure;
        if insecure || from_script {
            let shadows = self.domains.values().flatten().any(|old| {
                ((insecure && old.secure) || (from_script && old.http_only))
                    && old.name == cookie.name
                    && (domain_match(&old.domain, &cookie.domain) || domain_match(&cookie.domain, &old.domain))
                    && path_match(&cookie.path, &old.path)
            });
            if shadows {
                return false;
            }
        }

        let expired = cookie.expired(now);
        let list = self.domains.entry(cookie.domain.clone()).or_default();
        if let Some(i) = list.iter().position(|old| old.same_entry(&cookie)) {
            if expired {
                list.remove(i);
                self.count -= 1;
                if list.is_empty() {
                    self.domains.remove(&cookie.domain);
                }
            } else {
                cookie.created = list[i].created;
                list[i] = cookie;
            }
            return true;
        }
        if expired {
            if list.is_empty() {
                self.domains.remove(&cookie.domain);
            }
            return false;
        }
        let domain = cookie.domain.clone();
        list.push(cookie);
        self.count += 1;
        self.enforce_limits(&domain);
        true
    }

    /// Store the cookies a response from `url` sets
    pub fn store_response(&mut self, url: &str, headers: &[(String, String)], context: &CookieContext) {
        for (name, value) in headers {
            if name.eq_ignore_ascii_case("set-cookie") {
                self.set(url, value, context);
            }
        }
    }

    /// The cookies a request to `url` carries, in the order they are sent
    /// (longer paths first, then older first)
    pub fn cookies_for(&self, url: &str, context: &CookieContext) -> Vec<&Cookie> {
        let Some(target) = Target::parse(url) else { return Vec::new() };
        let (now, now_ms) = now();
        let same_site = context.is_same_site(url);
        let navigation = context.source == CookieSource::Navigation;
        let safe = context.safe_method();
        let mut partition: Option<Option<String>> = None;
        let mut out = Vec::new();
        for domain in candidate_domains(&target.host) {
            let Some(list) = self.domains.get(domain) else { continue };
            for cookie in list {
                if (cookie.host_only && domain != target.host)
                    || cookie.expired(now)
                    || !path_match(target.path, &cookie.path)
                    || (cookie.secure && !target.secure)
                    || (cookie.http_only && context.source == CookieSource::Script)
                {
                    continue;
                }
                if let Some(p) = &cookie.partition {
                    if partition.get_or_insert_with(|| context.top_level_site(url)).as_ref() != Some(p) {
                        continue;
                    }
                }
                let allowed = same_site
                    || match cookie.same_site {
                        SameSite::None => true,
                        SameSite::Strict => false,
                        SameSite::Lax => navigation && safe,
                        SameSite::Unset => {
                            navigation && (safe || now_ms.saturating_sub(cookie.created) < LAX_ALLOWING_UNSAFE_MS)
                        }
                    };
                if allowed {
                    out.push(cookie);
                }
            }
        }
        out.sort_by(|a, b| b.path.len().cmp(&a.path.len()).then(a.created.cmp(&b.created)));
        out
    }

    /// The Cookie header for a request to `url`, if it carries any cookies
    pub fn cookie_header(&self, url: &str, context: &CookieContext) -> Option<String> {
        let cookies = self.cookies_for(url, context);
        (!cookies.is_empty()).then(|| cookies.iter().map(|c| c.serialize()).collect::<Vec<_>>().join("; "))
    }

    /// What `document.cookie` reads on the page at `url`
    pub fn document_cookie(&self, url: &str) -> String {
        self.cookie_header(url, &CookieContext::script(url)).unwrap_or_default()
    }

    /// Assign to `document.cookie` on the page at `url`
    pub fn set_document_cookie(&mut self, url: &str, line: &str) -> bool {
        self.set(url, line, &CookieContext::script(url))
    }

    /// Every cookie stored (expired ones included until removed)
    pub fn iter(&self) -> impl Iterator<Item = &Cookie> {
        self.domains.values().flatten()
    }

    /// Drop expired cookies
    pub fn remove_expired(&mut self) {
        let (now, _) = now();
        for list in self.domains.values_mut() {
            list.retain(|c| !c.expired(now));
        }
        self.domains.retain(|_, list| !list.is_empty());
        self.count = self.domains.values().map(Vec::len).sum();
    }

    /// Drop the cookies that only last the session (when the browser closes)
    pub fn remove_session_cookies(&mut self) {
        for list in self.domains.values_mut() {
            list.retain(|c| c.expires.is_some());
        }
        self.domains.retain(|_, list| !list.is_empty());
        self.count = self.domains.values().map(Vec::len).sum();
    }

    /// Drop the cookies of `domain` and its subdomains
    pub fn clear_domain(&mut self, domain: &str) {
        self.domains.retain(|d, _| !domain_match(d, domain));
        self.count = self.domains.values().map(Vec::len).sum();
    }

    pub fn clear(&mut self) {
        self.domains.clear();
        self.count = 0;
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Keep within the per-domain and total limits, dropping expired
    /// cookies first and then the oldest
    fn enforce_limits(&mut self, domain: &str) {
        if self.domains.get(domain).is_some_and(|l| l.len() > MAX_PER_DOMAIN) || self.count > MAX_COOKIES {
            self.remove_expired();
        }
        if let Some(list) = self.domains.get_mut(domain) {
            while list.len() > MAX_PER_DOMAIN {
                let oldest = (0..list.len()).min_by_key(|&i| list[i].created).unwrap_or(0);
                list.remove(oldest);
                self.count -= 1;
            }
        }
        while self.count > MAX_COOKIES {
            let Some((d, i)) = self
                .domains
                .iter()
                .flat_map(|(d, list)| list.iter().enumerate().map(move |(i, c)| (d, i, c.created)))
                .min_by_key(|&(_, _, created)| created)
                .map(|(d, i, _)| (d.clone(), i))
            else {
                break;
            };
            let list = self.domains.get_mut(&d).expect("domain just found");
            list.remove(i);
            self.count -= 1;
            if list.is_empty() {
                self.domains.remove(&d);
            }
        }
    }
}

/// `host` and the domains above it, which may hold its cookies
fn candidate_domains(host: &str) -> impl Iterator<Item = &str> {
    let ip = psl::is_ip_address(host);
    std::iter::once(host).chain(
        host.match_indices('.').map(move |(i, _)| &host[i + 1..]).filter(move |_| !ip),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nav(initiator: Option<&str>) -> CookieContext<'_> {
        CookieContext::navigation(initiator, "GET")
    }

    fn header(jar: &CookieJar, url: &str) -> String {
        jar.cookie_header(url, &nav(None)).unwrap_or_default()
    }

    #[test]
    fn host_only_and_domain_cookies() {
        let mut jar = CookieJar::new();
        assert!(jar.set("https://www.example.com/", "host=1", &nav(None)));
        assert!(jar.set("https://www.example.com/", "dom=2; Domain=.Example.COM", &nav(None)));
        assert_eq!(header(&jar, "https://www.example.com/"), "host=1; dom=2");
        assert_eq!(header(&jar, "https://example.com/"), "dom=2");
        assert_eq!(header(&jar, "https://a.b.example.com/"), "dom=2");
        assert_eq!(header(&jar, "https://notexample.com/"), "");
        // A host cannot set cookies for another domain, a sibling, or a public suffix
        assert!(!jar.set("https://www.example.com/", "x=1; Domain=other.com", &nav(None)));
        assert!(!jar.set("https://www.example.com/", "x=1; Domain=api.example.com", &nav(None)));
        assert!(!jar.set("https://www.example.com/", "x=1; Domain=com", &nav(None)));
        assert!(!jar.set("https://a.example.co.uk/", "x=1; Domain=co.uk", &nav(None)));
        assert!(!jar.set("https://a.github.io/", "x=1; Domain=github.io", &nav(None)));
        // ...though a host that is itself a public suffix gets a host-only cookie
        assert!(jar.set("https://github.io/", "x=1; Domain=github.io", &nav(None)));
        assert_eq!(header(&jar, "https://github.io/"), "x=1");
        assert_eq!(header(&jar, "https://a.github.io/"), "");
        // IP addresses only match exactly
        assert!(jar.set("http://127.0.0.1:8000/", "ip=1", &nav(None)));
        assert_eq!(header(&jar, "http://127.0.0.1:9000/"), "ip=1");
        assert_eq!(header(&jar, "http://0.0.1/"), "");
    }

    #[test]
    fn paths() {
        let mut jar = CookieJar::new();
        // The default path is the request path's directory
        jar.set("https://e.com/docs/page.html", "a=1", &nav(None));
        jar.set("https://e.com/", "b=2; Path=/docs/api", &nav(None));
        jar.set("https://e.com/", "c=3; Path=relative", &nav(None));
        assert_eq!(header(&jar, "https://e.com/docs/api/x?q"), "b=2; a=1; c=3");
        assert_eq!(header(&jar, "https://e.com/docs"), "a=1; c=3");
        assert_eq!(header(&jar, "https://e.com/docsearch"), "c=3");
        assert_eq!(header(&jar, "https://e.com/docs/apix"), "a=1; c=3");
        assert!(path_match("/", "/"));
        assert!(path_match("/a/b", "/a/"));
        assert!(!path_match("/ab", "/a/"));
        assert_eq!(default_path(""), "/");
        assert_eq!(default_path("/x"), "/");
        assert_eq!(default_path("/a/b/c"), "/a/b");
    }

    #[test]
    fn replacing_and_deleting() {
        let mut jar = CookieJar::new();
        jar.set("https://e.com/", "id=1", &nav(None));
        jar.set("https://e.com/", "id=2", &nav(None));
        assert_eq!(header(&jar, "https://e.com/"), "id=2");
        assert_eq!(jar.len(), 1);
        // Same name on another path is another cookie
        jar.set("https://e.com/", "id=3; Path=/x", &nav(None));
        assert_eq!(jar.len(), 2);
        jar.set("https://e.com/", "id=; Max-Age=0", &nav(None));
        jar.set("https://e.com/", "id=; Path=/x; Expires=Thu, 01 Jan 1970 00:00:00 GMT", &nav(None));
        assert!(jar.is_empty());
        // Max-Age wins over Expires, whatever the order
        jar.set("https://e.com/", "k=v; Max-Age=60; Expires=Thu, 01 Jan 1970 00:00:00 GMT", &nav(None));
        assert_eq!(header(&jar, "https://e.com/"), "k=v");
        let expires = jar.iter().next().and_then(|c| c.expires).unwrap();
        assert!(expires > now().0 + 50 && expires <= now().0 + 60);
        // Lifetimes are capped at 400 days
        jar.set("https://e.com/", "long=1; Max-Age=999999999999", &nav(None));
        let long = jar.iter().find(|c| c.name == "long").and_then(|c| c.expires).unwrap();
        assert!(long <= now().0 + MAX_AGE_SECS);
    }

    #[test]
    fn secure_and_prefixes() {
        let mut jar = CookieJar::new();
        assert!(!jar.set("http://e.com/", "s=1; Secure", &nav(None)));
        assert!(jar.set("https://e.com/", "s=1; Secure", &nav(None)));
        assert_eq!(header(&jar, "https://e.com/"), "s=1");
        assert_eq!(header(&jar, "http://e.com/"), "");
        // Insecure URLs cannot overwrite or shadow it
        assert!(!jar.set("http://e.com/", "s=2", &nav(None)));
        assert!(!jar.set("http://sub.e.com/", "s=2; Domain=e.com", &nav(None)));
        // Localhost counts as secure
        assert!(jar.set("http://localhost:3000/", "dev=1; Secure", &nav(None)));
        assert!(!jar.set("https://e.com/", "__Secure-a=1", &nav(None)));
        assert!(jar.set("https://e.com/", "__Secure-a=1; Secure", &nav(None)));
        assert!(!jar.set("https://e.com/", "__Host-b=1; Secure; Domain=e.com; Path=/", &nav(None)));
        assert!(!jar.set("https://e.com/", "__Host-b=1; Secure; Path=/x", &nav(None)));
        assert!(!jar.set("https://e.com/", "__host-b=1", &nav(None)));
        assert!(jar.set("https://e.com/", "__Host-b=1; Secure; Path=/", &nav(None)));
        // __Host- needs an explicit Path=/
        assert!(!jar.set("https://e.com/", "__Host-c=1; Secure", &nav(None)));
        assert!(!jar.set("https://e.com/", "=__Secure-x=1", &nav(None)));
    }

    #[test]
    fn scripts_and_http_only() {
        let mut jar = CookieJar::new();
        jar.set("https://e.com/", "sid=secret; HttpOnly", &nav(None));
        jar.set("https://e.com/", "theme=dark", &nav(None));
        assert_eq!(jar.document_cookie("https://e.com/"), "theme=dark");
        assert_eq!(header(&jar, "https://e.com/"), "sid=secret; theme=dark");
        // Scripts cannot set or overwrite HttpOnly cookies
        assert!(!jar.set_document_cookie("https://e.com/", "x=1; HttpOnly"));
        assert!(!jar.set_document_cookie("https://e.com/", "sid=forged"));
        assert!(!jar.set_document_cookie("https://e.com/", "sid=forged; path=/deeper"));
        assert!(jar.set_document_cookie("https://e.com/", "lang=en; path=/"));
        assert_eq!(header(&jar, "https://e.com/"), "sid=secret; theme=dark; lang=en");
        // Nameless cookies
        assert!(jar.set_document_cookie("https://e.com/", "justavalue"));
        assert!(jar.document_cookie("https://e.com/").ends_with("; justavalue"));
    }

    #[test]
    fn same_site_rules() {
        let mut jar = CookieJar::new();
        let page = "https://shop.example/";
        jar.set(page, "strict=1; SameSite=Strict", &nav(None));
        jar.set(page, "lax=1; SameSite=Lax", &nav(None));
        jar.set(page, "unset=1", &nav(None));
        jar.set(page, "none=1; SameSite=None; Secure", &nav(None));
        // SameSite=None needs Secure
        assert!(!jar.set(page, "insecure=1; SameSite=None", &nav(None)));
        let get = |ctx: &CookieContext| jar.cookie_header(page, ctx).unwrap_or_default();

        // Same-site requests carry everything
        assert_eq!(get(&CookieContext::subresource(Some("https://www.shop.example/a"), "POST")), "strict=1; lax=1; unset=1; none=1");
        // A cross-site link: Lax and unset too, not Strict
        assert_eq!(get(&nav(Some("https://other.example/"))), "lax=1; unset=1; none=1");
        // A cross-site POST navigation: unset ones only while fresh
        assert_eq!(get(&CookieContext::navigation(Some("https://other.example/"), "POST")), "unset=1; none=1");
        // Cross-site subresources and fetches: only None
        assert_eq!(get(&CookieContext::subresource(Some("https://other.example/"), "GET")), "none=1");
        // http and https of one domain are different sites
        assert_eq!(get(&CookieContext::subresource(Some("http://shop.example/"), "GET")), "none=1");
        // A cross-site redirect in the chain makes the rest cross-site
        let mut ctx = CookieContext::subresource(Some(page), "GET");
        ctx.follow("https://tracker.example/");
        ctx.follow(page);
        assert_eq!(get(&ctx), "none=1");

        // Cross-site responses cannot set Lax or Strict cookies
        let cross = CookieContext::subresource(Some("https://other.example/"), "GET");
        assert!(!jar.set(page, "a=1", &cross));
        assert!(!jar.set(page, "a=1; SameSite=Lax", &cross));
        assert!(jar.set(page, "a=1; SameSite=None; Secure", &cross));
        // ...but cross-site navigations can
        assert!(jar.set(page, "b=1", &nav(Some("https://other.example/"))));
    }

    #[test]
    fn partitioned_cookies() {
        let mut jar = CookieJar::new();
        let embed = "https://widget.example/api";
        let on_a = CookieContext::subresource(Some("https://a.example/"), "GET");
        let on_b = CookieContext::subresource(Some("https://b.example/"), "GET");
        assert!(!jar.set(embed, "id=A; SameSite=None; Partitioned", &on_a));
        assert!(jar.set(embed, "id=A; Secure; SameSite=None; Partitioned", &on_a));
        assert!(jar.set(embed, "id=B; Secure; SameSite=None; Partitioned", &on_b));
        assert_eq!(jar.cookie_header(embed, &on_a).as_deref(), Some("id=A"));
        assert_eq!(jar.cookie_header(embed, &on_b).as_deref(), Some("id=B"));
        assert_eq!(jar.cookie_header(embed, &nav(None)), None);
    }

    #[test]
    fn parsing_edge_cases() {
        let mut jar = CookieJar::new();
        assert!(!jar.set("https://e.com/", "", &nav(None)));
        assert!(!jar.set("https://e.com/", "=", &nav(None)));
        assert!(!jar.set("https://e.com/", "a=b\x01c", &nav(None)));
        assert!(!jar.set("https://e.com/", &format!("big={}", "x".repeat(MAX_COOKIE_SIZE)), &nav(None)));
        assert!(jar.set("https://e.com/", "  spaced = v w ; path=/; SECURE; HttpOnly", &nav(None)));
        let c = jar.iter().next().unwrap();
        assert_eq!((c.name.as_str(), c.value.as_str(), c.secure, c.http_only), ("spaced", "v w", true, true));
        // Unknown and malformed attributes are ignored
        assert!(jar.set("https://e.com/", "m=1; Max-Age=abc; Expires=never; Foo=bar", &nav(None)));
        assert_eq!(jar.iter().find(|c| c.name == "m").unwrap().expires, None);
        // Non-HTTP URLs have no cookies
        assert!(!jar.set("file:///tmp/x.html", "f=1", &nav(None)));
        assert_eq!(header(&jar, "data:text/plain,hi"), "");
    }

    #[test]
    fn cookie_dates() {
        let t = 784_111_777; // Sun, 06 Nov 1994 08:49:37 GMT
        assert_eq!(parse_cookie_date("Sun, 06 Nov 1994 08:49:37 GMT"), Some(t));
        assert_eq!(parse_cookie_date("Sunday, 06-Nov-94 08:49:37 GMT"), Some(t));
        assert_eq!(parse_cookie_date("Sun Nov  6 08:49:37 1994"), Some(t));
        assert_eq!(parse_cookie_date("06 November 1994 08:49:37"), Some(t));
        assert_eq!(parse_cookie_date("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        assert_eq!(parse_cookie_date("Wed, 29 Feb 2024 12:00:00 GMT"), Some(1_709_208_000));
        assert_eq!(parse_cookie_date("Fri, 31 Dec 9999 23:59:59 GMT"), Some(253_402_300_799));
        assert_eq!(parse_cookie_date("Thu, 29 Feb 2023 12:00:00 GMT"), None);
        assert_eq!(parse_cookie_date("Sun, 06 Nov 1994 24:00:00 GMT"), None);
        assert_eq!(parse_cookie_date("06 Nov 1994"), None);
        assert_eq!(parse_cookie_date("garbage"), None);
    }

    #[test]
    fn limits() {
        let mut jar = CookieJar::new();
        for i in 0..MAX_PER_DOMAIN + 5 {
            jar.set("https://e.com/", &format!("c{i}=1"), &nav(None));
        }
        assert_eq!(jar.len(), MAX_PER_DOMAIN);
        // The oldest went first
        assert!(!jar.iter().any(|c| c.name == "c0"));
        assert!(jar.iter().any(|c| c.name == format!("c{}", MAX_PER_DOMAIN + 4)));
    }
}
