//! Edge case and stress tests for fos-html
//!
//! Tests rare HTML scenarios, malformed content, and stress conditions.

use fos_html::{HtmlParser, Document};

// ============================================================================
// EMPTY AND MINIMAL INPUT
// ============================================================================

#[test]
fn test_parse_null_bytes() {
    // HTML with null bytes (browsers handle these)
    let html = "Hello\0World";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_only_whitespace() {
    let html = "   \t\n\r\n   ";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() >= 1);
}

#[test]
fn test_parse_only_doctype() {
    let html = "<!DOCTYPE html>";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() >= 1);
}

// ============================================================================
// MALFORMED HTML
// ============================================================================

#[test]
fn test_parse_unclosed_tags() {
    let html = "<div><p><span>text";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_mismatched_tags() {
    let html = "<div><p></div></p>";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_extra_closing_tags() {
    let html = "<div></div></div></div></div>";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_orphan_closing_tag() {
    let html = "</div>";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() >= 1);
}

#[test]
fn test_parse_nested_same_tags() {
    // Many levels of same tag (which is sometimes invalid)
    let html = "<p><p><p><p>text</p></p></p></p>";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_invalid_nesting() {
    // Block inside inline (invalid but common)
    let html = "<span><div>text</div></span>";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

// ============================================================================
// SPECIAL TAGS
// ============================================================================

#[test]
fn test_parse_script_content() {
    let html = r#"<script>
        if (a < b && c > d) {
            console.log("<div>not a tag</div>");
        }
    </script>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_style_content() {
    let html = r#"<style>
        div > p { color: red; }
        .class < .other { display: none; }
    </style>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_textarea_content() {
    let html = r#"<textarea><div>This is not a div</div></textarea>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_title_content() {
    let html = r#"<title>This <b>should</b> not be bold</title>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_xmp_content() {
    let html = r#"<xmp><div>Literal tags</div></xmp>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_plaintext() {
    let html = r#"<plaintext><div>Everything after is literal"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

// ============================================================================
// VOID ELEMENTS
// ============================================================================

#[test]
fn test_parse_void_elements() {
    let html = r#"
        <br>
        <hr>
        <img src="test.png">
        <input type="text">
        <meta charset="utf-8">
        <link rel="stylesheet" href="style.css">
        <base href="/">
        <col>
        <embed>
        <keygen>
        <param>
        <source>
        <track>
        <wbr>
    "#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_void_with_content() {
    // Void elements can't have content, but parsers handle it
    let html = "<br>content after<hr>more content";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_void_self_closing() {
    let html = r#"<br /><hr/><img src="test.png" />"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

// ============================================================================
// ATTRIBUTES
// ============================================================================

#[test]
fn test_parse_attribute_no_value() {
    let html = r#"<input disabled readonly required>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_attribute_empty_value() {
    let html = r#"<input value="" name=''>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_attribute_unquoted() {
    let html = r#"<div id=myid class=myclass>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_attribute_mixed_quotes() {
    let html = r#"<div id="double" class='single' data-x=unquoted>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_attribute_special_chars() {
    let html = r#"<div data-json='{"key": "value"}' data-url="http://example.com?a=1&b=2">"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_many_attributes() {
    // Element with many attributes
    let mut html = "<div ".to_string();
    for i in 0..100 {
        html.push_str(&format!(r#"data-attr{}="value{}" "#, i, i));
    }
    html.push_str(">content</div>");
    
    let doc = HtmlParser::new().parse(&html);
    assert!(doc.tree().len() > 1);
}

// ============================================================================
// ENTITIES
// ============================================================================

#[test]
fn test_parse_named_entities() {
    let html = "&lt;&gt;&amp;&quot;&apos;&nbsp;&copy;&reg;";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_numeric_entities() {
    let html = "&#60;&#62;&#38;&#x3C;&#x3E;&#x26;";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_invalid_entities() {
    let html = "&invalid; &; & isolated &amp incomplete";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

// ============================================================================
// UNICODE AND ENCODING
// ============================================================================

#[test]
fn test_parse_utf8_content() {
    let html = "<p>日本語 中文 한국어 العربية עברית Ελληνικά Русский</p>";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_emoji() {
    let html = "<p>🎉🚀🌍❤️🔥💯👍😀🎨🎵</p>";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_rtl_text() {
    let html = r#"<p dir="rtl">مرحبا بالعالم</p>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

// ============================================================================
// COMMENTS
// ============================================================================

#[test]
fn test_parse_empty_comment() {
    let html = "<!---->";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() >= 1);
}

#[test]
fn test_parse_comment_with_gt() {
    let html = "<!-- > -->";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() >= 1);
}

#[test]
fn test_parse_comment_with_tags() {
    let html = "<!-- <div>commented out</div> -->";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() >= 1);
}

#[test]
fn test_parse_conditional_comment() {
    let html = "<!--[if IE]><p>IE only</p><![endif]-->";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() >= 1);
}

// ============================================================================
// CDATA
// ============================================================================

#[test]
fn test_parse_cdata() {
    let html = "<![CDATA[<div>not parsed</div>]]>";
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() >= 1);
}

// ============================================================================
// STRESS TESTS
// ============================================================================

#[test]
fn test_parse_very_long_text() {
    let long_text = "a".repeat(100_000);
    let html = format!("<p>{}</p>", long_text);
    let doc = HtmlParser::new().parse(&html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_very_long_attribute() {
    let long_value = "x".repeat(10_000);
    let html = format!(r#"<div data-long="{}">"#, long_value);
    let doc = HtmlParser::new().parse(&html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_deeply_nested() {
    let mut html = String::new();
    for _ in 0..100 {
        html.push_str("<div>");
    }
    html.push_str("deepest");
    for _ in 0..100 {
        html.push_str("</div>");
    }
    
    let doc = HtmlParser::new().parse(&html);
    assert!(doc.tree().len() > 100);
}

#[test]
fn test_parse_many_siblings() {
    let mut html = "<ul>".to_string();
    for i in 0..1000 {
        html.push_str(&format!("<li>{}</li>", i));
    }
    html.push_str("</ul>");
    
    let doc = HtmlParser::new().parse(&html);
    assert!(doc.tree().len() > 1000);
}

// ============================================================================
// REAL-WORLD PATTERNS
// ============================================================================

#[test]
fn test_parse_incomplete_svg() {
    let html = r#"<svg><path d="M0 0 L10 10">"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_template_syntax() {
    // Common template patterns that look like HTML
    let html = r#"<div>{{ variable }}</div><span>{% if cond %}</span>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_react_jsx_like() {
    let html = r#"<div className="container" onClick={handleClick}>Content</div>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_data_attributes() {
    let html = r#"<div data-testid="test" data-cy="cypress" data-qa="quality">"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}
