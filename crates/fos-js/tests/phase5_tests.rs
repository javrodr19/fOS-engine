//! Phase 5 Web API tests for fos-js
//!
//! Tests for localStorage, sessionStorage, history, location APIs.

use fos_js::*;
use fos_dom::Document;
use std::sync::{Arc, Mutex};

// ============================================================================
// STORAGE TESTS
// ============================================================================

#[test]
fn test_storage_setitem_getitem() {
    let mut storage = Storage::session();
    
    storage.set_item("key1", "value1");
    assert_eq!(storage.get_item("key1"), Some("value1"));
    
    storage.set_item("key2", "value2");
    assert_eq!(storage.get_item("key2"), Some("value2"));
}

#[test]
fn test_storage_update_item() {
    let mut storage = Storage::session();
    
    storage.set_item("key", "original");
    assert_eq!(storage.get_item("key"), Some("original"));
    
    storage.set_item("key", "updated");
    assert_eq!(storage.get_item("key"), Some("updated"));
    assert_eq!(storage.length(), 1);
}

#[test]
fn test_storage_remove_item() {
    let mut storage = Storage::session();
    
    storage.set_item("key", "value");
    assert_eq!(storage.length(), 1);
    
    storage.remove_item("key");
    assert!(storage.get_item("key").is_none());
    assert_eq!(storage.length(), 0);
}

#[test]
fn test_storage_remove_nonexistent() {
    let mut storage = Storage::session();
    storage.remove_item("nonexistent"); // Should not panic
    assert_eq!(storage.length(), 0);
}

#[test]
fn test_storage_clear() {
    let mut storage = Storage::session();
    
    storage.set_item("a", "1");
    storage.set_item("b", "2");
    storage.set_item("c", "3");
    assert_eq!(storage.length(), 3);
    
    storage.clear();
    assert_eq!(storage.length(), 0);
    assert!(storage.get_item("a").is_none());
}

#[test]
fn test_storage_empty_key() {
    let mut storage = Storage::session();
    storage.set_item("", "empty_key_value");
    assert_eq!(storage.get_item(""), Some("empty_key_value"));
}

#[test]
fn test_storage_empty_value() {
    let mut storage = Storage::session();
    storage.set_item("key", "");
    assert_eq!(storage.get_item("key"), Some(""));
}

#[test]
fn test_storage_unicode() {
    let mut storage = Storage::session();
    storage.set_item("日本語", "こんにちは");
    assert_eq!(storage.get_item("日本語"), Some("こんにちは"));
    
    storage.set_item("emoji", "🎉🎊🎈");
    assert_eq!(storage.get_item("emoji"), Some("🎉🎊🎈"));
}

#[test]
fn test_storage_long_value() {
    let mut storage = Storage::session();
    let long_value = "x".repeat(10000);
    storage.set_item("long", &long_value);
    assert_eq!(storage.get_item("long").unwrap().len(), 10000);
}

#[test]
fn test_storage_many_items() {
    let mut storage = Storage::session();
    
    for i in 0..100 {
        storage.set_item(&format!("key{}", i), &format!("value{}", i));
    }
    
    assert_eq!(storage.length(), 100);
    assert_eq!(storage.get_item("key50"), Some("value50"));
}

// ============================================================================
// HISTORY TESTS
// ============================================================================

#[test]
fn test_history_initial() {
    let history = HistoryManager::new("https://example.com");
    assert_eq!(history.length(), 1);
    assert_eq!(history.current().url, "https://example.com");
}

#[test]
fn test_history_push_state() {
    let mut history = HistoryManager::new("https://example.com/");
    
    history.push_state(None, "Page 1".into(), "/page1".into());
    assert_eq!(history.length(), 2);
    assert_eq!(history.current().url, "/page1");
    
    history.push_state(None, "Page 2".into(), "/page2".into());
    assert_eq!(history.length(), 3);
    assert_eq!(history.current().url, "/page2");
}

#[test]
fn test_history_replace_state() {
    let mut history = HistoryManager::new("https://example.com/old");
    
    history.replace_state(None, "New".into(), "https://example.com/new".into());
    assert_eq!(history.length(), 1);
    assert_eq!(history.current().url, "https://example.com/new");
}

#[test]
fn test_history_back() {
    let mut history = HistoryManager::new("https://example.com/");
    history.push_state(None, "".into(), "/page1".into());
    history.push_state(None, "".into(), "/page2".into());
    
    history.back();
    assert_eq!(history.current().url, "/page1");
    
    history.back();
    assert_eq!(history.current().url, "https://example.com/");
    
    // Back at beginning, should stay
    history.back();
    assert_eq!(history.current().url, "https://example.com/");
}

#[test]
fn test_history_forward() {
    let mut history = HistoryManager::new("https://example.com/");
    history.push_state(None, "".into(), "/page1".into());
    history.push_state(None, "".into(), "/page2".into());
    
    history.back();
    history.back();
    assert_eq!(history.current().url, "https://example.com/");
    
    history.forward();
    assert_eq!(history.current().url, "/page1");
    
    history.forward();
    assert_eq!(history.current().url, "/page2");
    
    // Forward at end, should stay
    history.forward();
    assert_eq!(history.current().url, "/page2");
}

#[test]
fn test_history_go() {
    let mut history = HistoryManager::new("/");
    history.push_state(None, "".into(), "/a".into());
    history.push_state(None, "".into(), "/b".into());
    history.push_state(None, "".into(), "/c".into());
    
    history.go(-2);
    assert_eq!(history.current().url, "/a");
    
    history.go(1);
    assert_eq!(history.current().url, "/b");
    
    history.go(0);
    assert_eq!(history.current().url, "/b");
}

#[test]
fn test_history_push_removes_forward() {
    let mut history = HistoryManager::new("/");
    history.push_state(None, "".into(), "/a".into());
    history.push_state(None, "".into(), "/b".into());
    history.push_state(None, "".into(), "/c".into());
    
    history.go(-2);  // Back to /a
    assert_eq!(history.length(), 4);
    
    history.push_state(None, "".into(), "/new".into());
    assert_eq!(history.length(), 3);  // Forward history removed
    assert_eq!(history.current().url, "/new");
}

// ============================================================================
// LOCATION TESTS
// ============================================================================

#[test]
fn test_location_full_url() {
    let loc = LocationManager::new("https://user:pass@example.com:8080/path/to/page?query=1#section").unwrap();
    
    assert_eq!(loc.protocol(), "https:");
    assert_eq!(loc.hostname(), "example.com");
    assert_eq!(loc.host(), "example.com:8080");
    assert_eq!(loc.port(), "8080");
    assert_eq!(loc.pathname(), "/path/to/page");
    assert_eq!(loc.search(), "?query=1");
    assert_eq!(loc.hash(), "#section");
}

#[test]
fn test_location_simple_url() {
    let loc = LocationManager::new("https://example.com/").unwrap();
    
    assert_eq!(loc.protocol(), "https:");
    assert_eq!(loc.hostname(), "example.com");
    assert_eq!(loc.port(), "");
    assert_eq!(loc.pathname(), "/");
    assert_eq!(loc.search(), "");
    assert_eq!(loc.hash(), "");
}

#[test]
fn test_location_localhost() {
    let loc = LocationManager::new("http://localhost:3000/api").unwrap();
    
    assert_eq!(loc.protocol(), "http:");
    assert_eq!(loc.hostname(), "localhost");
    assert_eq!(loc.port(), "3000");
    assert_eq!(loc.pathname(), "/api");
}

#[test]
fn test_location_set_href() {
    let mut loc = LocationManager::new("https://old.com/").unwrap();
    
    loc.set_href("https://new.com/page").unwrap();
    assert_eq!(loc.hostname(), "new.com");
    assert_eq!(loc.pathname(), "/page");
}

#[test]
fn test_location_origin() {
    let loc = LocationManager::new("https://example.com:443/path").unwrap();
    assert!(loc.origin().contains("example.com"));
}

#[test]
fn test_location_query_params() {
    let loc = LocationManager::new("https://example.com/search?q=rust&page=1&sort=date").unwrap();
    assert_eq!(loc.search(), "?q=rust&page=1&sort=date");
}

#[test]
fn test_location_encoded_chars() {
    let loc = LocationManager::new("https://example.com/path%20with%20spaces").unwrap();
    assert!(loc.pathname().contains("path"));
}

#[test]
fn test_location_file_url() {
    let loc = LocationManager::new("file:///home/user/document.txt").unwrap();
    assert_eq!(loc.protocol(), "file:");
    assert_eq!(loc.pathname(), "/home/user/document.txt");
}

// ============================================================================
// JS CONTEXT WITH WEB APIS
// ============================================================================

#[test]
fn test_context_localStorage_basic() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    ctx.exec("localStorage.setItem('test', 'hello')").unwrap();
    let result = ctx.eval("localStorage.getItem('test')").unwrap();
    
    match result {
        JsValue::String(s) => assert_eq!(s, "hello"),
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_context_localStorage_multiple() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    ctx.exec(r#"
        localStorage.setItem('a', '1');
        localStorage.setItem('b', '2');
        localStorage.setItem('c', '3');
    "#).unwrap();
    
    let result = ctx.eval("localStorage.getLength()").unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 3.0),
        _ => panic!("Expected number"),
    }
}

#[test]
fn test_context_localStorage_remove() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    ctx.exec(r#"
        localStorage.setItem('key', 'value');
        localStorage.removeItem('key');
    "#).unwrap();
    
    let result = ctx.eval("localStorage.getItem('key')").unwrap();
    // getItem returns null or undefined for missing keys
    assert!(matches!(result, JsValue::Null | JsValue::Undefined) || 
            matches!(&result, JsValue::String(s) if s.is_empty()),
            "Expected null/undefined, got {:?}", result);
}

#[test]
fn test_context_sessionStorage() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    ctx.exec("sessionStorage.setItem('session', 'data')").unwrap();
    let result = ctx.eval("sessionStorage.getItem('session')").unwrap();
    
    match result {
        JsValue::String(s) => assert_eq!(s, "data"),
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_context_history_navigation() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::with_url(doc, "https://example.com/").unwrap();
    
    ctx.exec(r#"
        history.pushState(null, '', '/page1');
        history.pushState(null, '', '/page2');
        history.back();
    "#).unwrap();
    
    // After back(), we should be at page1
    let len = ctx.eval("history.getLength()").unwrap();
    match len {
        JsValue::Number(n) => assert_eq!(n, 3.0),
        _ => panic!("Expected number"),
    }
}

#[test]
fn test_context_location_properties() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::with_url(doc, "https://example.com:8080/path?q=1#hash").unwrap();
    
    let protocol = ctx.eval("location.getProtocol()").unwrap();
    match protocol {
        JsValue::String(s) => assert_eq!(s, "https:"),
        _ => panic!("Expected string"),
    }
    
    let hostname = ctx.eval("location.getHostname()").unwrap();
    match hostname {
        JsValue::String(s) => assert_eq!(s, "example.com"),
        _ => panic!("Expected string"),
    }
    
    let pathname = ctx.eval("location.getPathname()").unwrap();
    match pathname {
        JsValue::String(s) => assert_eq!(s, "/path"),
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_context_all_apis_together() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::with_url(doc, "https://app.example.com/").unwrap();
    
    // Use all APIs in one test
    ctx.exec(r#"
        console.log('Testing all APIs');
        
        // Storage
        localStorage.setItem('visited', 'true');
        sessionStorage.setItem('temp', 'data');
        
        // History
        history.pushState(null, '', '/dashboard');
        
        // Document
        document.createElement('div');
    "#).unwrap();
    
    // Verify
    let visited = ctx.eval("localStorage.getItem('visited')").unwrap();
    match visited {
        JsValue::String(s) => assert_eq!(s, "true"),
        _ => panic!("Expected string"),
    }
}
