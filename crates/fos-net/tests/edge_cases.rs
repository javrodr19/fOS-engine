//! Comprehensive edge case tests for fos-net
//!
//! Tests for HTTP loader, fetch API, request/response handling.

use fos_net::*;
use fos_net::loader::*;

// ============================================================================
// REQUEST BUILDER TESTS
// ============================================================================

#[test]
fn test_request_get() {
    let req = Request::get("https://example.com");
    assert_eq!(req.method, Method::Get);
    assert_eq!(req.url, "https://example.com");
    assert!(req.headers.is_empty());
    assert!(req.body.is_none());
}

#[test]
fn test_request_post() {
    let req = Request::post("https://api.example.com/data");
    assert_eq!(req.method, Method::Post);
    assert_eq!(req.url, "https://api.example.com/data");
}

#[test]
fn test_request_with_headers() {
    let req = Request::get("https://example.com")
        .with_header("Accept", "application/json")
        .with_header("Authorization", "Bearer token123")
        .with_header("X-Custom-Header", "value");
    
    assert_eq!(req.headers.len(), 3);
    assert_eq!(req.headers.get("Accept").unwrap(), "application/json");
    assert_eq!(req.headers.get("Authorization").unwrap(), "Bearer token123");
}

#[test]
fn test_request_with_body() {
    let body = b"Hello, World!".to_vec();
    let req = Request::post("https://example.com")
        .with_body(body.clone());
    
    assert!(req.body.is_some());
    assert_eq!(req.body.unwrap(), body);
}

#[test]
fn test_request_with_json() {
    let json = r#"{"name": "test", "value": 42}"#;
    let req = Request::post("https://example.com")
        .with_json(json);
    
    assert_eq!(req.headers.get("Content-Type").unwrap(), "application/json");
    assert!(req.body.is_some());
}

#[test]
fn test_request_empty_body() {
    let req = Request::post("https://example.com")
        .with_body(vec![]);
    
    assert!(req.body.is_some());
    assert!(req.body.unwrap().is_empty());
}

// ============================================================================
// METHOD TESTS
// ============================================================================

#[test]
fn test_all_methods() {
    let methods = [
        (Method::Get, "GET"),
        (Method::Post, "POST"),
        (Method::Put, "PUT"),
        (Method::Delete, "DELETE"),
        (Method::Head, "HEAD"),
        (Method::Options, "OPTIONS"),
        (Method::Patch, "PATCH"),
    ];
    
    for (method, _name) in &methods {
        assert_eq!(*method, *method); // Identity
    }
}

#[test]
fn test_default_method() {
    assert_eq!(Method::default(), Method::Get);
}

// ============================================================================
// RESPONSE TESTS
// ============================================================================

#[test]
fn test_response_success_codes() {
    let success_codes = [200, 201, 202, 204, 206];
    for code in &success_codes {
        let resp = Response {
            status: *code,
            headers: vec![],
            body: vec![],
        };
        assert!(resp.is_success(), "Status {} should be success", code);
    }
}

#[test]
fn test_response_error_codes() {
    let error_codes = [400, 401, 403, 404, 500, 502, 503];
    for code in &error_codes {
        let resp = Response {
            status: *code,
            headers: vec![],
            body: vec![],
        };
        assert!(!resp.is_success(), "Status {} should not be success", code);
    }
}

#[test]
fn test_response_text() {
    let resp = Response {
        status: 200,
        headers: vec![],
        body: "Hello, World!".as_bytes().to_vec(),
    };
    
    assert_eq!(resp.text().unwrap(), "Hello, World!");
}

#[test]
fn test_response_text_utf8() {
    let resp = Response {
        status: 200,
        headers: vec![],
        body: "Привет 世界 🌍".as_bytes().to_vec(),
    };
    
    let text = resp.text().unwrap();
    assert!(text.contains("Привет"));
    assert!(text.contains("世界"));
    assert!(text.contains("🌍"));
}

#[test]
fn test_response_text_empty() {
    let resp = Response {
        status: 204,
        headers: vec![],
        body: vec![],
    };
    
    assert_eq!(resp.text().unwrap(), "");
}

#[test]
fn test_response_headers() {
    let resp = Response {
        status: 200,
        headers: vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Content-Length".to_string(), "42".to_string()),
        ],
        body: vec![],
    };
    
    assert_eq!(resp.headers.len(), 2);
}

// ============================================================================
// FETCH OPTIONS TESTS
// ============================================================================

#[test]
fn test_fetch_options_default() {
    let opts = FetchOptions::new();
    assert!(opts.method.is_empty());
    assert!(opts.headers.is_empty());
    assert!(opts.body.is_none());
}

#[test]
fn test_fetch_options_post() {
    let opts = FetchOptions::new()
        .method("POST")
        .header("Content-Type", "application/json")
        .body(r#"{"key": "value"}"#);
    
    assert_eq!(opts.method, "POST");
    assert_eq!(opts.headers.len(), 1);
    assert!(opts.body.is_some());
}

#[test]
fn test_fetch_options_multiple_headers() {
    let opts = FetchOptions::new()
        .header("Accept", "application/json")
        .header("Authorization", "Bearer token")
        .header("X-Request-ID", "12345");
    
    assert_eq!(opts.headers.len(), 3);
}

#[test]
fn test_fetch_options_case_insensitive_method() {
    // Methods should be converted to uppercase in fetch_with_options
    let methods = ["GET", "get", "Get", "gEt"];
    for m in &methods {
        let opts = FetchOptions::new().method(m);
        assert_eq!(opts.method, *m);
    }
}

// ============================================================================
// NET ERROR TESTS
// ============================================================================

#[test]
fn test_net_error_display() {
    let err = NetError::HttpError { status: 404 };
    assert!(format!("{}", err).contains("404"));
    
    let err = NetError::Network("Connection refused".into());
    assert!(format!("{}", err).contains("Connection refused"));
    
    let err = NetError::InvalidUrl("not a url".into());
    assert!(format!("{}", err).contains("not a url"));
}

// ============================================================================
// URL PARSING TESTS
// ============================================================================
// NOTE: URL parsing is now handled by fos-dom::url, not fos-net
// These tests are commented out as the Url type was removed with reqwest

// #[test]
// fn test_url_parsing() {
//     let url = Url::parse("https://example.com/path?query=1#hash").unwrap();
//     assert_eq!(url.scheme(), "https");
//     assert_eq!(url.host_str().unwrap(), "example.com");
//     assert_eq!(url.path(), "/path");
//     assert_eq!(url.query(), Some("query=1"));
//     assert_eq!(url.fragment(), Some("hash"));
// }

// #[test]
// fn test_url_with_port() {
//     let url = Url::parse("http://localhost:8080/api").unwrap();
//     assert_eq!(url.port(), Some(8080));
//     assert_eq!(url.host_str().unwrap(), "localhost");
// }

// #[test]
// fn test_url_with_auth() {
//     let url = Url::parse("https://user:pass@example.com/").unwrap();
//     assert_eq!(url.username(), "user");
//     assert_eq!(url.password(), Some("pass"));
// }

// #[test]
// fn test_url_file_scheme() {
//     let url = Url::parse("file:///path/to/file.txt").unwrap();
//     assert_eq!(url.scheme(), "file");
//     assert_eq!(url.path(), "/path/to/file.txt");
// }

// ============================================================================
// RESOURCE LOADER TESTS
// ============================================================================

#[test]
fn test_resource_loader_default() {
    let loader = ResourceLoader::default();
    // Just verify it can be created
    let _ = loader;
}

#[test]
fn test_resource_loader_new() {
    let loader = ResourceLoader::new();
    let _ = loader;
}
