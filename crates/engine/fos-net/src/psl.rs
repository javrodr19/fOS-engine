//! Public suffixes and sites
//!
//! Which part of a host name is a public suffix, under which anyone can
//! register names ("com", "co.uk", "github.io"), and which is the
//! registrable domain one party controls ("example.co.uk"). Cookies use it
//! so a site cannot set cookies for a whole suffix, and to tell same-site
//! requests from cross-site ones.
//!
//! The rules come from the Public Suffix List (data/public_suffix_list.dat,
//! from https://publicsuffix.org/list/), compiled into the binary as one
//! sorted text that lookups binary-search in place: nothing happens at
//! startup, nothing is allocated for it, and only the few pages a lookup
//! touches are ever read.

use std::cmp::Ordering;

use crate::url_util;

/// The rules, one per line, sorted bytewise (see build.rs)
static RULES: &str = include_str!(concat!(env!("OUT_DIR"), "/public_suffixes.txt"));

/// Whether `rule` is on the list
fn listed(rule: &str) -> bool {
    let text = RULES.as_bytes();
    // `lo` and `hi` are always at the start of a line (or the end)
    let (mut lo, mut hi) = (0, text.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let start = text[lo..mid].iter().rposition(|&c| c == b'\n').map_or(lo, |p| lo + p + 1);
        let end = text[start..hi].iter().position(|&c| c == b'\n').map_or(hi, |p| start + p);
        match RULES[start..end].cmp(rule) {
            Ordering::Equal => return true,
            Ordering::Less => lo = (end + 1).min(hi),
            Ordering::Greater => hi = start,
        }
    }
    false
}

/// Whether `host` is an IP address rather than a domain name
pub fn is_ip_address(host: &str) -> bool {
    host.starts_with('[') || host.parse::<std::net::IpAddr>().is_ok()
}

/// The public suffix of a domain name (lowercase): its longest ending
/// that the list makes public, or its last label when no rule matches.
/// IP addresses are returned whole.
pub fn public_suffix(host: &str) -> &str {
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.is_empty() || is_ip_address(host) {
        return host;
    }
    // Where each candidate suffix starts, longest first
    let starts: Vec<usize> = std::iter::once(0).chain(host.match_indices('.').map(|(i, _)| i + 1)).collect();
    let mut rule = String::with_capacity(host.len() + 2);
    // An exception ("!city.kawasaki.jp") wins over any other rule, and
    // makes the suffix one label shorter than itself
    for (k, &s) in starts.iter().enumerate() {
        rule.clear();
        rule.push('!');
        rule.push_str(&host[s..]);
        if listed(&rule) {
            return starts.get(k + 1).map_or("", |&next| &host[next..]);
        }
    }
    // Otherwise the longest matching rule, exact or wildcard ("*.ck")
    for (k, &s) in starts.iter().enumerate() {
        let candidate = &host[s..];
        if listed(candidate) {
            return candidate;
        }
        if let Some(&next) = starts.get(k + 1) {
            rule.clear();
            rule.push_str("*.");
            rule.push_str(&host[next..]);
            if listed(&rule) {
                return candidate;
            }
        }
    }
    // The implicit "*" rule: every top-level domain is public
    &host[*starts.last().unwrap_or(&0)..]
}

/// Whether `domain` is itself a public suffix
pub fn is_public_suffix(domain: &str) -> bool {
    let domain = domain.strip_suffix('.').unwrap_or(domain);
    !domain.is_empty() && !is_ip_address(domain) && public_suffix(domain).len() == domain.len()
}

/// The registrable domain of `host`: its public suffix and one more label
/// ("www.example.co.uk" → "example.co.uk"). None for IP addresses and
/// public suffixes themselves.
pub fn registrable_domain(host: &str) -> Option<&str> {
    let host = host.strip_suffix('.').unwrap_or(host);
    if is_ip_address(host) {
        return None;
    }
    let suffix = public_suffix(host);
    if suffix.len() >= host.len() {
        return None;
    }
    let rest = &host[..host.len() - suffix.len() - 1];
    Some(&host[rest.rfind('.').map_or(0, |p| p + 1)..])
}

/// The site of a URL: its scheme and registrable domain (or whole host
/// when it has none), e.g. `("https", "example.co.uk")`. None for URLs
/// without a host.
pub fn site(url: &str) -> Option<(String, String)> {
    let (scheme, host, _) = url_util::origin(url)?;
    if host.is_empty() {
        return None;
    }
    let domain = registrable_domain(&host).map(str::to_string).unwrap_or_else(|| host.clone());
    Some((scheme, domain))
}

/// Whether two URLs are same-site: same scheme and same registrable
/// domain ("schemeful same-site")
pub fn same_site(a: &str, b: &str) -> bool {
    match (site(a), site(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes_from_the_list() {
        assert_eq!(public_suffix("www.example.com"), "com");
        assert_eq!(public_suffix("www.example.co.uk"), "co.uk");
        assert_eq!(public_suffix("user.github.io"), "github.io");
        assert_eq!(public_suffix("example.unknowntld"), "unknowntld");
        // Wildcard (*.ck) and its exception (!www.ck)
        assert_eq!(public_suffix("a.b.ck"), "b.ck");
        assert_eq!(public_suffix("www.ck"), "ck");
        assert_eq!(public_suffix("a.city.kawasaki.jp"), "kawasaki.jp");
        assert_eq!(public_suffix("x.y.kawasaki.jp"), "y.kawasaki.jp");
        // Internationalized rules, in either form
        assert_eq!(public_suffix("example.公司.cn"), "公司.cn");
        assert_eq!(public_suffix("example.xn--55qx5d.cn"), "xn--55qx5d.cn");
        assert_eq!(public_suffix("example.com."), "com");
    }

    #[test]
    fn registrable_domains() {
        assert_eq!(registrable_domain("www.example.com"), Some("example.com"));
        assert_eq!(registrable_domain("a.b.example.co.uk"), Some("example.co.uk"));
        assert_eq!(registrable_domain("user.github.io"), Some("user.github.io"));
        assert_eq!(registrable_domain("www.ck"), Some("www.ck"));
        assert_eq!(registrable_domain("co.uk"), None);
        assert_eq!(registrable_domain("com"), None);
        assert_eq!(registrable_domain("127.0.0.1"), None);
        assert_eq!(registrable_domain("[::1]"), None);
        assert!(is_public_suffix("co.uk"));
        assert!(is_public_suffix("github.io"));
        assert!(!is_public_suffix("example.com"));
        assert!(!is_public_suffix("127.0.0.1"));
    }

    #[test]
    fn sites() {
        assert!(same_site("https://www.example.com/a", "https://api.example.com:8443/b"));
        assert!(!same_site("https://example.com/", "http://example.com/"));
        assert!(!same_site("https://a.github.io/", "https://b.github.io/"));
        assert!(!same_site("https://a.co.uk/", "https://b.co.uk/"));
        assert!(same_site("http://127.0.0.1:8000/", "http://127.0.0.1:9000/"));
        assert!(!same_site("http://127.0.0.1/", "http://127.0.0.2/"));
        assert!(!same_site("about:blank", "about:blank"));
    }

    #[test]
    fn every_rule_is_found() {
        for rule in RULES.lines() {
            assert!(listed(rule), "{rule}");
            assert!(!listed(&format!("{rule}x")) || RULES.lines().any(|r| r == format!("{rule}x")));
        }
        assert!(!listed(""));
        assert!(!listed("zzzz-not-a-rule"));
    }
}
