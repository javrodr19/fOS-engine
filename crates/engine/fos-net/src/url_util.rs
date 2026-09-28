//! URL reference resolution (RFC 3986 §5)
//!
//! Resolves relative references such as redirect `Location` values or link
//! targets against a base URL, including protocol-relative (`//host/x`),
//! query-only (`?q`), fragment-only (`#f`) and dot-segment (`../x`) forms.

/// Components of a URI reference, borrowed from the input
struct Reference<'a> {
    scheme: Option<&'a str>,
    authority: Option<&'a str>,
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

fn is_scheme(candidate: &str) -> bool {
    let mut chars = candidate.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

fn parse_reference(s: &str) -> Reference<'_> {
    let mut rest = s;

    let mut fragment = None;
    if let Some(i) = rest.find('#') {
        fragment = Some(&rest[i + 1..]);
        rest = &rest[..i];
    }

    let mut query = None;
    if let Some(i) = rest.find('?') {
        query = Some(&rest[i + 1..]);
        rest = &rest[..i];
    }

    let mut scheme = None;
    if let Some(i) = rest.find(':') {
        let candidate = &rest[..i];
        if !candidate.contains('/') && is_scheme(candidate) {
            scheme = Some(candidate);
            rest = &rest[i + 1..];
        }
    }

    let mut authority = None;
    if let Some(r) = rest.strip_prefix("//") {
        let end = r.find('/').unwrap_or(r.len());
        authority = Some(&r[..end]);
        rest = &r[end..];
    }

    Reference { scheme, authority, path: rest, query, fragment }
}

/// Remove `.` and `..` segments from a path (RFC 3986 §5.2.4)
pub fn remove_dot_segments(path: &str) -> String {
    if !path.split('/').any(|seg| seg == "." || seg == "..") {
        return path.to_string();
    }

    let absolute = path.starts_with('/');
    let segments: Vec<&str> = path.split('/').skip(usize::from(absolute)).collect();
    let last = segments.len().saturating_sub(1);

    let mut out: Vec<&str> = Vec::with_capacity(segments.len());
    let mut trailing_slash = false;
    for (i, seg) in segments.iter().enumerate() {
        match *seg {
            "." => trailing_slash = i == last,
            ".." => {
                out.pop();
                trailing_slash = i == last;
            }
            _ => {
                out.push(seg);
                trailing_slash = false;
            }
        }
    }

    let mut result = String::with_capacity(path.len());
    if absolute {
        result.push('/');
    }
    result.push_str(&out.join("/"));
    if trailing_slash && !out.is_empty() {
        result.push('/');
    }
    result
}

/// Resolve `reference` against `base` (RFC 3986 §5.2.2).
///
/// If `base` is not an absolute URL, the reference is returned unchanged.
pub fn resolve(base: &str, reference: &str) -> String {
    let r = parse_reference(reference.trim());
    let b = parse_reference(base.trim());

    if r.scheme.is_none() && b.scheme.is_none() {
        return reference.trim().to_string();
    }

    let (scheme, authority, path, query) = if let Some(scheme) = r.scheme {
        (scheme, r.authority, remove_dot_segments(r.path), r.query)
    } else if r.authority.is_some() {
        (b.scheme.unwrap_or(""), r.authority, remove_dot_segments(r.path), r.query)
    } else if r.path.is_empty() {
        (b.scheme.unwrap_or(""), b.authority, b.path.to_string(), r.query.or(b.query))
    } else if r.path.starts_with('/') {
        (b.scheme.unwrap_or(""), b.authority, remove_dot_segments(r.path), r.query)
    } else {
        // Merge with the base path (RFC 3986 §5.2.3)
        let merged = if b.authority.is_some() && b.path.is_empty() {
            format!("/{}", r.path)
        } else {
            match b.path.rfind('/') {
                Some(i) => format!("{}{}", &b.path[..=i], r.path),
                None => r.path.to_string(),
            }
        };
        (b.scheme.unwrap_or(""), b.authority, remove_dot_segments(&merged), r.query)
    };

    let mut out = String::with_capacity(base.len() + reference.len());
    if !scheme.is_empty() {
        out.push_str(scheme);
        out.push(':');
    }
    if let Some(authority) = authority {
        out.push_str("//");
        out.push_str(authority);
    }
    out.push_str(&path);
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = r.fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// Scheme, host and port of an absolute URL, lowercased, e.g.
/// `("https", "example.com", 443)`. Used for same-origin checks.
pub fn origin(url: &str) -> Option<(String, String, u16)> {
    let r = parse_reference(url.trim());
    let scheme = r.scheme?.to_ascii_lowercase();
    let authority = r.authority?;
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, hp)| hp);

    let (host, port) = if host_port.starts_with('[') {
        let end = host_port.find(']')?;
        let port = host_port[end + 1..].strip_prefix(':').and_then(|p| p.parse().ok());
        (&host_port[..=end], port)
    } else {
        match host_port.rsplit_once(':') {
            Some((h, p)) => (h, p.parse().ok()),
            None => (host_port, None),
        }
    };

    let default_port = match scheme.as_str() {
        "https" | "wss" => 443,
        "http" | "ws" => 80,
        _ => 0,
    };
    Some((scheme, host.to_ascii_lowercase(), port.unwrap_or(default_port)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "http://a/b/c/d;p?q";

    #[test]
    fn test_rfc3986_normal_examples() {
        let cases = [
            ("g:h", "g:h"),
            ("g", "http://a/b/c/g"),
            ("./g", "http://a/b/c/g"),
            ("g/", "http://a/b/c/g/"),
            ("/g", "http://a/g"),
            ("//g", "http://g"),
            ("?y", "http://a/b/c/d;p?y"),
            ("g?y", "http://a/b/c/g?y"),
            ("#s", "http://a/b/c/d;p?q#s"),
            ("g#s", "http://a/b/c/g#s"),
            ("g?y#s", "http://a/b/c/g?y#s"),
            (";x", "http://a/b/c/;x"),
            ("g;x", "http://a/b/c/g;x"),
            ("g;x?y#s", "http://a/b/c/g;x?y#s"),
            ("", "http://a/b/c/d;p?q"),
            (".", "http://a/b/c/"),
            ("./", "http://a/b/c/"),
            ("..", "http://a/b/"),
            ("../", "http://a/b/"),
            ("../g", "http://a/b/g"),
            ("../..", "http://a/"),
            ("../../", "http://a/"),
            ("../../g", "http://a/g"),
        ];
        for (reference, expected) in cases {
            assert_eq!(resolve(BASE, reference), expected, "reference {:?}", reference);
        }
    }

    #[test]
    fn test_rfc3986_abnormal_examples() {
        let cases = [
            ("../../../g", "http://a/g"),
            ("../../../../g", "http://a/g"),
            ("/./g", "http://a/g"),
            ("/../g", "http://a/g"),
            ("g.", "http://a/b/c/g."),
            (".g", "http://a/b/c/.g"),
            ("g..", "http://a/b/c/g.."),
            ("..g", "http://a/b/c/..g"),
            ("./../g", "http://a/b/g"),
            ("./g/.", "http://a/b/c/g/"),
            ("g/./h", "http://a/b/c/g/h"),
            ("g/../h", "http://a/b/c/h"),
            ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
            ("g;x=1/../y", "http://a/b/c/y"),
        ];
        for (reference, expected) in cases {
            assert_eq!(resolve(BASE, reference), expected, "reference {:?}", reference);
        }
    }

    #[test]
    fn test_resolve_against_bare_origin() {
        assert_eq!(resolve("https://example.com", "page.html"), "https://example.com/page.html");
        assert_eq!(resolve("https://example.com", "?q=1"), "https://example.com?q=1");
        assert_eq!(resolve("https://example.com:8443/a/b", "//cdn.example.com/x.js"), "https://cdn.example.com/x.js");
    }

    #[test]
    fn test_origin() {
        assert_eq!(origin("https://Example.com/path"), Some(("https".into(), "example.com".into(), 443)));
        assert_eq!(origin("http://localhost:8080/"), Some(("http".into(), "localhost".into(), 8080)));
        assert_eq!(origin("http://[::1]:3000/x"), Some(("http".into(), "[::1]".into(), 3000)));
        assert_eq!(origin("http://user:pw@host/x"), Some(("http".into(), "host".into(), 80)));
        assert_eq!(origin("relative/path"), None);
    }
}
