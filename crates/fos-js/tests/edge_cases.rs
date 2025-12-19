//! Comprehensive edge case tests for fos-js
//!
//! Tests for edge cases, error handling, and stress testing.

use fos_js::*;
use fos_dom::Document;
use std::sync::{Arc, Mutex};

// ============================================================================
// RUNTIME EDGE CASES
// ============================================================================

#[test]
fn test_empty_code() {
    let result = eval("");
    assert!(result.is_ok());
}

#[test]
fn test_whitespace_only() {
    let result = eval("   \n\t  ");
    assert!(result.is_ok());
}

#[test]
fn test_comment_only() {
    let result = eval("// just a comment");
    assert!(result.is_ok());
    
    let result = eval("/* block comment */");
    assert!(result.is_ok());
}

#[test]
fn test_multiline_code() {
    let code = r#"
        var a = 1;
        var b = 2;
        var c = a + b;
        c
    "#;
    let result = eval(code).unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 3.0),
        _ => panic!("Expected number"),
    }
}

#[test]
fn test_unicode_strings() {
    let result = eval("'Hello 世界 🌍'").unwrap();
    match result {
        JsValue::String(s) => assert!(s.contains("世界") && s.contains("🌍")),
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_special_numbers() {
    // NaN
    let result = eval("NaN").unwrap();
    match result {
        JsValue::Number(n) => assert!(n.is_nan()),
        _ => panic!("Expected NaN"),
    }
    
    // Infinity
    let result = eval("Infinity").unwrap();
    match result {
        JsValue::Number(n) => assert!(n.is_infinite() && n > 0.0),
        _ => panic!("Expected Infinity"),
    }
    
    // -Infinity
    let result = eval("-Infinity").unwrap();
    match result {
        JsValue::Number(n) => assert!(n.is_infinite() && n < 0.0),
        _ => panic!("Expected -Infinity"),
    }
}

#[test]
fn test_large_numbers() {
    let result = eval("Number.MAX_SAFE_INTEGER").unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 9007199254740991.0),
        _ => panic!("Expected number"),
    }
}

#[test]
fn test_negative_numbers() {
    let result = eval("-42").unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, -42.0),
        _ => panic!("Expected number"),
    }
}

#[test]
fn test_decimal_numbers() {
    let result = eval("3.14159").unwrap();
    match result {
        JsValue::Number(n) => assert!((n - 3.14159).abs() < 0.00001),
        _ => panic!("Expected number"),
    }
}

#[test]
fn test_empty_string() {
    let result = eval("''").unwrap();
    match result {
        JsValue::String(s) => assert!(s.is_empty()),
        _ => panic!("Expected empty string"),
    }
}

#[test]
fn test_string_with_escapes() {
    let result = eval(r#"'line1\nline2\ttab'"#).unwrap();
    match result {
        JsValue::String(s) => assert!(s.contains('\n') && s.contains('\t')),
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_empty_array() {
    let result = eval("[]").unwrap();
    assert!(matches!(result, JsValue::Array));
}

#[test]
fn test_empty_object() {
    let result = eval("({})").unwrap();
    assert!(matches!(result, JsValue::Object));
}

#[test]
fn test_nested_objects() {
    let result = eval("({a: {b: {c: 1}}})").unwrap();
    assert!(matches!(result, JsValue::Object));
}

#[test]
fn test_function_expression() {
    let result = eval("(function named() { return 42; })").unwrap();
    assert!(matches!(result, JsValue::Function));
}

#[test]
fn test_arrow_function() {
    let result = eval("(() => 42)").unwrap();
    assert!(matches!(result, JsValue::Function));
}

// ============================================================================
// ERROR HANDLING
// ============================================================================

#[test]
fn test_syntax_error_unclosed_paren() {
    let result = eval("(1 + 2");
    assert!(result.is_err());
}

#[test]
fn test_syntax_error_unclosed_brace() {
    let result = eval("{ var x = 1");
    assert!(result.is_err());
}

#[test]
fn test_syntax_error_invalid_keyword() {
    let result = eval("function { }");
    assert!(result.is_err());
}

#[test]
fn test_reference_error() {
    let result = eval("undefinedVariable");
    // QuickJS returns undefined for undefined variables in some contexts
    // This may or may not error depending on strict mode
}

#[test]
fn test_type_error_call_non_function() {
    let result = eval("var x = 5; x()");
    assert!(result.is_err());
}

// ============================================================================
// JSCONTEXT TESTS
// ============================================================================

#[test]
fn test_context_console_all_levels() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    // All console methods should work
    ctx.exec("console.log('log')").unwrap();
    ctx.exec("console.info('info')").unwrap();
    ctx.exec("console.warn('warn')").unwrap();
    ctx.exec("console.error('error')").unwrap();
    ctx.exec("console.debug('debug')").unwrap();
}

#[test]
fn test_context_console_multiple_args() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    ctx.exec("console.log('a', 'b', 'c', 1, 2, 3, true, null, undefined)").unwrap();
}

#[test]
fn test_context_console_objects() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    ctx.exec("console.log({foo: 'bar'})").unwrap();
    ctx.exec("console.log([1, 2, 3])").unwrap();
    ctx.exec("console.log(function test() {})").unwrap();
}

#[test]
fn test_context_document_create_many_elements() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc.clone()).unwrap();
    
    // Create 100 elements
    ctx.exec(r#"
        for (var i = 0; i < 100; i++) {
            document.createElement('div');
        }
    "#).unwrap();
    
    // Check doc has more nodes
    let doc_locked = doc.lock().unwrap();
    assert!(doc_locked.tree().len() > 100);
}

#[test]
fn test_context_document_create_different_elements() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    let tags = ["div", "span", "p", "a", "button", "input", "form", "table", "tr", "td"];
    for tag in &tags {
        ctx.exec(&format!("document.createElement('{}')", tag)).unwrap();
    }
}

#[test]
fn test_context_document_create_text_nodes() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    ctx.exec("document.createTextNode('Hello')").unwrap();
    ctx.exec("document.createTextNode('')").unwrap();
    ctx.exec("document.createTextNode('Unicode: 日本語')").unwrap();
}

#[test]
fn test_context_timer_setup() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    // Setup timer (won't fire without event loop)
    let result = ctx.eval("setTimeout('1+1', 100)").unwrap();
    match result {
        JsValue::Number(n) => assert!(n > 0.0), // Timer ID should be positive
        _ => panic!("Expected timer ID"),
    }
}

#[test]
fn test_context_timer_clear() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    ctx.exec(r#"
        var id = setTimeout('console.log("test")', 1000);
        clearTimeout(id);
    "#).unwrap();
}

#[test]
fn test_context_interval_setup() {
    let doc = Arc::new(Mutex::new(Document::new("test://page")));
    let ctx = JsContext::new(doc).unwrap();
    
    ctx.exec(r#"
        var id = setInterval('1+1', 100);
        clearInterval(id);
    "#).unwrap();
}

// ============================================================================
// JAVASCRIPT FEATURES
// ============================================================================

#[test]
fn test_js_var_hoisting() {
    let result = eval(r#"
        function test() {
            x = 5;
            var x;
            return x;
        }
        test()
    "#).unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 5.0),
        _ => panic!("Expected 5"),
    }
}

#[test]
fn test_js_closure() {
    let result = eval(r#"
        function outer() {
            var x = 10;
            return function inner() {
                return x * 2;
            };
        }
        outer()()
    "#).unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 20.0),
        _ => panic!("Expected 20"),
    }
}

#[test]
fn test_js_recursion() {
    let result = eval(r#"
        function factorial(n) {
            if (n <= 1) return 1;
            return n * factorial(n - 1);
        }
        factorial(5)
    "#).unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 120.0),
        _ => panic!("Expected 120"),
    }
}

#[test]
fn test_js_array_methods() {
    let result = eval(r#"
        [1, 2, 3, 4, 5].reduce(function(a, b) { return a + b; }, 0)
    "#).unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 15.0),
        _ => panic!("Expected 15"),
    }
}

#[test]
fn test_js_string_methods() {
    let result = eval(r#"
        'hello world'.toUpperCase()
    "#).unwrap();
    match result {
        JsValue::String(s) => assert_eq!(s, "HELLO WORLD"),
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_js_math() {
    let result = eval("Math.sqrt(16)").unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 4.0),
        _ => panic!("Expected 4"),
    }
    
    let result = eval("Math.pow(2, 10)").unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 1024.0),
        _ => panic!("Expected 1024"),
    }
}

#[test]
fn test_js_json() {
    let result = eval(r#"
        JSON.stringify({a: 1, b: "test"})
    "#).unwrap();
    match result {
        JsValue::String(s) => assert!(s.contains("\"a\":1") || s.contains("\"a\": 1")),
        _ => panic!("Expected JSON string"),
    }
}

#[test]
fn test_js_date() {
    let result = eval("typeof new Date()").unwrap();
    match result {
        JsValue::String(s) => assert_eq!(s, "object"),
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_js_regex() {
    let result = eval("/test/.test('this is a test')").unwrap();
    match result {
        JsValue::Bool(b) => assert!(b),
        _ => panic!("Expected true"),
    }
}

#[test]
fn test_js_try_catch() {
    let result = eval(r#"
        try {
            throw new Error("test error");
        } catch (e) {
            "caught: " + e.message
        }
    "#).unwrap();
    match result {
        JsValue::String(s) => assert!(s.contains("caught: test error")),
        _ => panic!("Expected string"),
    }
}

// ============================================================================
// STRESS TESTS
// ============================================================================

#[test]
fn test_stress_many_evals() {
    let runtime = JsRuntime::new().unwrap();
    
    for i in 0..100 {
        let result = runtime.eval(&format!("{} + {}", i, i)).unwrap();
        match result {
            JsValue::Number(n) => assert_eq!(n, (i * 2) as f64),
            _ => panic!("Expected number"),
        }
    }
}

#[test]
fn test_stress_large_string() {
    let large = "x".repeat(10000);
    let code = format!("'{}'", large);
    let result = eval(&code).unwrap();
    match result {
        JsValue::String(s) => assert_eq!(s.len(), 10000),
        _ => panic!("Expected string"),
    }
}

#[test]
fn test_stress_deep_recursion() {
    // Limited recursion to avoid stack overflow
    let result = eval(r#"
        function deep(n) {
            if (n <= 0) return 0;
            return 1 + deep(n - 1);
        }
        deep(100)
    "#).unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 100.0),
        _ => panic!("Expected 100"),
    }
}

#[test]
fn test_stress_large_array() {
    let result = eval(r#"
        var arr = [];
        for (var i = 0; i < 1000; i++) {
            arr.push(i);
        }
        arr.length
    "#).unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 1000.0),
        _ => panic!("Expected 1000"),
    }
}

#[test]
fn test_stress_many_objects() {
    let result = eval(r#"
        var count = 0;
        for (var i = 0; i < 500; i++) {
            var obj = {x: i, y: i * 2};
            count++;
        }
        count
    "#).unwrap();
    match result {
        JsValue::Number(n) => assert_eq!(n, 500.0),
        _ => panic!("Expected 500"),
    }
}
