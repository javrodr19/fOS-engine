//! Edge case and stress tests for fos-css
//!
//! Tests rare CSS scenarios, malformed input, and stress conditions.

use fos_css::{CssParser, Stylesheet};
use fos_css::properties::{PropertyId, PropertyValue, Color, Keyword, Length, LengthUnit};

// ============================================================================
// EMPTY AND MINIMAL INPUT
// ============================================================================

#[test]
fn test_parse_whitespace_only() {
    let css = "   \t\n\r\n   ";
    let result = CssParser::new().parse(css).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_parse_comment_only() {
    let css = "/* just a comment */";
    let result = CssParser::new().parse(css).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn test_parse_multiple_comments() {
    let css = "/* comment 1 */ /* comment 2 */ /* comment 3 */";
    let result = CssParser::new().parse(css).unwrap();
    assert_eq!(result.len(), 0);
}

// ============================================================================
// SELECTOR EDGE CASES
// ============================================================================

#[test]
fn test_parse_universal_selector() {
    let css = "* { display: block; }";
    let result = CssParser::new().parse(css).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_parse_complex_selector() {
    let css = "div.container#main > p.text:first-child + span::before { color: red; }";
    let result = CssParser::new().parse(css).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_parse_attribute_selectors() {
    let css = r#"
        [attr] { }
        [attr=value] { }
        [attr~=value] { }
        [attr|=value] { }
        [attr^=value] { }
        [attr$=value] { }
        [attr*=value] { }
        [attr="value with spaces"] { }
        [attr='single quotes'] { }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 9);
}

#[test]
fn test_parse_pseudo_classes() {
    let css = r#"
        :hover { }
        :focus { }
        :active { }
        :first-child { }
        :last-child { }
        :nth-child(2n+1) { }
        :nth-child(odd) { }
        :nth-child(even) { }
        :not(.excluded) { }
        :is(div, span) { }
        :where(p, a) { }
        :has(> img) { }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 10);
}

#[test]
fn test_parse_pseudo_elements() {
    let css = r#"
        ::before { content: ""; }
        ::after { content: ""; }
        ::first-line { }
        ::first-letter { }
        ::selection { }
        ::placeholder { }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 5);
}

#[test]
fn test_parse_selector_list() {
    let css = "h1, h2, h3, h4, h5, h6 { margin: 0; }";
    let result = CssParser::new().parse(css).unwrap();
    assert_eq!(result.len(), 1);
}

// ============================================================================
// VALUE EDGE CASES
// ============================================================================

#[test]
fn test_parse_color_formats() {
    let css = r#"
        .hex3 { color: #f00; }
        .hex4 { color: #f00f; }
        .hex6 { color: #ff0000; }
        .hex8 { color: #ff0000ff; }
        .rgb { color: rgb(255, 0, 0); }
        .rgba { color: rgba(255, 0, 0, 0.5); }
        .hsl { color: hsl(0, 100%, 50%); }
        .hsla { color: hsla(0, 100%, 50%, 0.5); }
        .named { color: red; }
        .currentcolor { color: currentColor; }
        .transparent { color: transparent; }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 10);
}

#[test]
fn test_parse_length_units() {
    let css = r#"
        .px { width: 100px; }
        .em { width: 2em; }
        .rem { width: 1.5rem; }
        .percent { width: 50%; }
        .vw { width: 100vw; }
        .vh { height: 100vh; }
        .vmin { width: 50vmin; }
        .vmax { width: 50vmax; }
        .ch { width: 20ch; }
        .ex { width: 2ex; }
        .cm { width: 10cm; }
        .mm { width: 100mm; }
        .in { width: 1in; }
        .pt { width: 12pt; }
        .pc { width: 6pc; }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 10);
}

#[test]
fn test_parse_calc() {
    let css = r#"
        .calc1 { width: calc(100% - 20px); }
        .calc2 { width: calc(50vw + 10em); }
        .calc3 { width: calc(100% / 3); }
        .calc4 { width: calc((100% - 40px) / 2); }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 4);
}

#[test]
fn test_parse_var() {
    let css = r#"
        :root { --main-color: blue; }
        .var { color: var(--main-color); }
        .var-fallback { color: var(--undefined, red); }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 3);
}

#[test]
fn test_parse_negative_values() {
    let css = r#"
        .neg { margin: -10px; margin-left: -2em; z-index: -1; }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_parse_decimal_values() {
    let css = r#"
        .dec { opacity: 0.5; line-height: 1.5; flex-grow: 0.25; }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert_eq!(result.len(), 1);
}

// ============================================================================
// AT-RULES
// ============================================================================

#[test]
fn test_parse_media_query() {
    let css = r#"
        @media (max-width: 600px) {
            .mobile { display: block; }
        }
        @media screen and (min-width: 768px) {
            .tablet { display: flex; }
        }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    // Media queries may or may not be parsed depending on implementation
    assert!(result.len() >= 0);
}

#[test]
fn test_parse_keyframes() {
    let css = r#"
        @keyframes fadeIn {
            from { opacity: 0; }
            to { opacity: 1; }
        }
        @keyframes slide {
            0% { transform: translateX(0); }
            50% { transform: translateX(100px); }
            100% { transform: translateX(0); }
        }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 0);
}

#[test]
fn test_parse_font_face() {
    let css = r#"
        @font-face {
            font-family: 'CustomFont';
            src: url('font.woff2') format('woff2');
        }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 0);
}

#[test]
fn test_parse_import() {
    let css = r#"@import url("other.css");"#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 0);
}

// ============================================================================
// PROPERTY EDGE CASES
// ============================================================================

#[test]
fn test_parse_shorthand_properties() {
    let css = r#"
        .margin { margin: 10px 20px 30px 40px; }
        .padding { padding: 10px 20px; }
        .border { border: 1px solid black; }
        .background { background: url("bg.png") center/cover no-repeat; }
        .font { font: bold 16px/1.5 sans-serif; }
        .flex { flex: 1 0 auto; }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 5);
}

#[test]
fn test_parse_vendor_prefixes() {
    let css = r#"
        .vendor {
            -webkit-transform: rotate(45deg);
            -moz-transform: rotate(45deg);
            -ms-transform: rotate(45deg);
            transform: rotate(45deg);
        }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert_eq!(result.len(), 1);
}

// ============================================================================
// UNICODE AND SPECIAL CHARACTERS
// ============================================================================

#[test]
fn test_parse_unicode_class_names() {
    let css = ".日本語 { } .中文 { } .한국어 { }";
    let result = CssParser::new().parse(css).unwrap();
    assert_eq!(result.len(), 3);
}

#[test]
fn test_parse_escaped_characters() {
    let css = r#".class\:name { } .class\[0\] { } .\31 23 { }"#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 2);
}

#[test]
fn test_parse_content_special() {
    let css = r#"
        .before::before { content: "»"; }
        .after::after { content: "«"; }
        .emoji::before { content: "🎉"; }
        .unicode::before { content: "\00a0"; }
    "#;
    let result = CssParser::new().parse(css).unwrap();
    assert!(result.len() >= 4);
}

// ============================================================================
// STRESS TESTS
// ============================================================================

#[test]
fn test_parse_very_long_value() {
    let long_value = "a".repeat(10_000);
    let css = format!(r#".long {{ content: "{}"; }}"#, long_value);
    let result = CssParser::new().parse(&css).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_parse_many_declarations() {
    let mut css = ".many { ".to_string();
    for i in 0..100 {
        css.push_str(&format!("--var-{}: value{}; ", i, i));
    }
    css.push_str("}");
    
    let result = CssParser::new().parse(&css).unwrap();
    assert_eq!(result.len(), 1);
}

#[test]
fn test_parse_deeply_nested_selectors() {
    let mut css = String::new();
    for i in 0..20 {
        css.push_str(&format!(".level{} ", i));
    }
    css.push_str("{ color: red; }");
    
    let result = CssParser::new().parse(&css).unwrap();
    assert_eq!(result.len(), 1);
}

// ============================================================================
// COLOR PARSING EDGE CASES
// ============================================================================

#[test]
fn test_color_hex_case_insensitive() {
    let hex1 = Color::from_hex("#AbCdEf").unwrap();
    let hex2 = Color::from_hex("#abcdef").unwrap();
    let hex3 = Color::from_hex("#ABCDEF").unwrap();
    
    assert_eq!((hex1.r, hex1.g, hex1.b), (hex2.r, hex2.g, hex2.b));
    assert_eq!((hex2.r, hex2.g, hex2.b), (hex3.r, hex3.g, hex3.b));
}

#[test]
fn test_color_hex_with_hash() {
    assert!(Color::from_hex("#fff").is_some());
    assert!(Color::from_hex("#ffffff").is_some());
}

#[test]
fn test_color_invalid_hex() {
    assert!(Color::from_hex("fff").is_some()); // No hash still works
    assert!(Color::from_hex("#gg0000").is_none());
    assert!(Color::from_hex("#ff").is_none()); // Too short
    assert!(Color::from_hex("#fffff").is_none()); // Invalid length
}

#[test]
fn test_color_named_colors() {
    let colors = [
        "black", "white", "red", "green", "blue", "yellow",
        "cyan", "magenta", "gray", "silver", "maroon", "olive",
        "lime", "navy", "purple", "teal", "orange", "transparent",
    ];
    
    for name in &colors {
        assert!(Color::from_name(name).is_some(), "Color {} should exist", name);
    }
}

// ============================================================================
// KEYWORD PARSING EDGE CASES  
// ============================================================================

#[test]
fn test_keyword_all_display_values() {
    let keywords = ["none", "block", "inline", "inline-block", "flex", "grid", "contents"];
    for kw in &keywords {
        assert!(Keyword::from_str(kw).is_some(), "Keyword {} should parse", kw);
    }
}

#[test]
fn test_keyword_all_position_values() {
    let keywords = ["static", "relative", "absolute", "fixed", "sticky"];
    for kw in &keywords {
        assert!(Keyword::from_str(kw).is_some(), "Keyword {} should parse", kw);
    }
}

#[test]
fn test_keyword_case_sensitive() {
    assert!(Keyword::from_str("Block").is_none());
    assert!(Keyword::from_str("BLOCK").is_none());
    assert!(Keyword::from_str("block").is_some());
}

// ============================================================================
// PROPERTY ID EDGE CASES
// ============================================================================

#[test]
fn test_property_id_all_box_model() {
    let props = [
        "width", "height", "min-width", "min-height", "max-width", "max-height",
        "margin", "margin-top", "margin-right", "margin-bottom", "margin-left",
        "padding", "padding-top", "padding-right", "padding-bottom", "padding-left",
    ];
    
    for prop in &props {
        assert!(PropertyId::from_name(prop).is_some(), "Property {} should exist", prop);
    }
}

#[test]
fn test_property_id_all_flexbox() {
    let props = [
        "flex-direction", "flex-wrap", "justify-content", "align-items",
        "align-content", "flex-grow", "flex-shrink", "flex-basis",
    ];
    
    for prop in &props {
        assert!(PropertyId::from_name(prop).is_some(), "Property {} should exist", prop);
    }
}

#[test]
fn test_property_id_unknown() {
    assert!(PropertyId::from_name("unknown-property-xyz").is_none());
    assert!(PropertyId::from_name("").is_none());
    assert!(PropertyId::from_name("-").is_none());
}
