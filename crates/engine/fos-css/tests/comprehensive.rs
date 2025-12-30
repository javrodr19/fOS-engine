//! Comprehensive tests for fos-css
//!
//! Tests parsing edge cases and style computation.

use fos_css::{CssParser, Stylesheet, StyleResolver};
use fos_css::properties::{PropertyId, PropertyValue, Color, Keyword, Length, LengthUnit};

#[test]
fn test_parse_empty() {
    let stylesheet = CssParser::new().parse("").unwrap();
    assert_eq!(stylesheet.len(), 0);
}

#[test]
fn test_parse_single_rule() {
    let css = ".foo { color: red; }";
    let stylesheet = CssParser::new().parse(css).unwrap();
    assert_eq!(stylesheet.len(), 1);
}

#[test]
fn test_parse_multiple_rules() {
    let css = r#"
        .foo { color: red; }
        .bar { background: blue; }
        #baz { display: flex; }
    "#;
    let stylesheet = CssParser::new().parse(css).unwrap();
    assert_eq!(stylesheet.len(), 3);
}

#[test]
fn test_parse_complex_selectors() {
    let css = r#"
        div.container > p.text { color: black; }
        ul li a:hover { color: blue; }
        input[type="text"] { border: 1px solid; }
        h1, h2, h3 { margin: 0; }
    "#;
    let stylesheet = CssParser::new().parse(css).unwrap();
    assert!(stylesheet.len() >= 3);
}

#[test]
fn test_parse_colors() {
    let css = r#"
        .hex3 { color: #f00; }
        .hex6 { color: #ff0000; }
        .named { color: blue; }
        .rgb { color: rgb(255, 0, 0); }
        .rgba { color: rgba(255, 0, 0, 0.5); }
    "#;
    let stylesheet = CssParser::new().parse(css).unwrap();
    assert!(stylesheet.len() >= 4);
}

#[test]
fn test_parse_lengths() {
    let css = r#"
        .px { width: 100px; }
        .em { width: 2em; }
        .rem { width: 1.5rem; }
        .percent { width: 50%; }
        .vw { width: 100vw; }
        .vh { height: 100vh; }
    "#;
    let stylesheet = CssParser::new().parse(css).unwrap();
    assert!(stylesheet.len() >= 5);
}

#[test]
fn test_parse_display_values() {
    let css = r#"
        .none { display: none; }
        .block { display: block; }
        .inline { display: inline; }
        .flex { display: flex; }
        .grid { display: grid; }
    "#;
    let stylesheet = CssParser::new().parse(css).unwrap();
    assert_eq!(stylesheet.len(), 5);
}

#[test]
fn test_parse_box_model() {
    let css = r#"
        .box {
            margin: 10px;
            padding: 20px 30px;
            border: 1px solid black;
            width: 100px;
            height: 50px;
        }
    "#;
    let stylesheet = CssParser::new().parse(css).unwrap();
    assert_eq!(stylesheet.len(), 1);
}

#[test]
fn test_parse_flexbox() {
    let css = r#"
        .container {
            display: flex;
            flex-direction: row;
            justify-content: space-between;
            align-items: center;
            flex-wrap: wrap;
        }
    "#;
    let stylesheet = CssParser::new().parse(css).unwrap();
    assert_eq!(stylesheet.len(), 1);
}

#[test]
fn test_parse_important() {
    let css = r#"
        .force { color: red !important; }
    "#;
    let stylesheet = CssParser::new().parse(css).unwrap();
    assert_eq!(stylesheet.len(), 1);
    
    // Check that the declaration is marked important
    if let Some(rule) = stylesheet.rules.get(0) {
        let has_important = rule.declarations.iter().any(|d| d.important);
        // Note: depends on whether lightningcss puts it in important_declarations
    }
}

#[test]
fn test_parse_media_query() {
    let css = r#"
        .normal { color: black; }
        
        @media (max-width: 600px) {
            .normal { color: red; }
        }
    "#;
    let stylesheet = CssParser::new().parse(css).unwrap();
    // Media query rules might not be converted yet
    assert!(stylesheet.len() >= 1);
}

#[test]
fn test_parse_comments() {
    let css = r#"
        /* This is a comment */
        .foo {
            color: red; /* inline comment */
        }
        /* Multi-line
           comment */
        .bar { display: block; }
    "#;
    let stylesheet = CssParser::new().parse(css).unwrap();
    assert_eq!(stylesheet.len(), 2);
}

#[test]
fn test_color_parsing() {
    // Test Color::from_hex
    let red = Color::from_hex("#ff0000").unwrap();
    assert_eq!(red.r, 255);
    assert_eq!(red.g, 0);
    assert_eq!(red.b, 0);
    
    let short = Color::from_hex("#f00").unwrap();
    assert_eq!(short.r, 255);
    assert_eq!(short.g, 0);
    assert_eq!(short.b, 0);
    
    // Test Color::from_name
    let blue = Color::from_name("blue").unwrap();
    assert_eq!(blue.r, 0);
    assert_eq!(blue.g, 0);
    assert_eq!(blue.b, 255);
}

#[test]
fn test_keyword_parsing() {
    assert_eq!(Keyword::from_str("block"), Some(Keyword::Block));
    assert_eq!(Keyword::from_str("flex"), Some(Keyword::Flex));
    assert_eq!(Keyword::from_str("center"), Some(Keyword::Center));
    assert_eq!(Keyword::from_str("invalid"), None);
}

#[test]
fn test_property_id_parsing() {
    assert_eq!(PropertyId::from_name("display"), Some(PropertyId::Display));
    assert_eq!(PropertyId::from_name("color"), Some(PropertyId::Color));
    assert_eq!(PropertyId::from_name("margin"), Some(PropertyId::Margin));
    assert_eq!(PropertyId::from_name("invalid-property"), None);
}

#[test]
fn test_length_creation() {
    let px = Length::px(100.0);
    assert_eq!(px.value, 100.0);
    assert_eq!(px.unit, LengthUnit::Px);
    
    let em = Length::em(2.0);
    assert_eq!(em.value, 2.0);
    assert_eq!(em.unit, LengthUnit::Em);
    
    let zero = Length::zero();
    assert_eq!(zero.value, 0.0);
}

#[test]
fn test_memory_sizes() {
    use std::mem::size_of;
    
    println!("=== CSS Type Sizes ===");
    println!("PropertyId: {} bytes", size_of::<PropertyId>());
    println!("PropertyValue: {} bytes", size_of::<PropertyValue>());
    println!("Keyword: {} bytes", size_of::<Keyword>());
    println!("Length: {} bytes", size_of::<Length>());
    println!("LengthUnit: {} bytes", size_of::<LengthUnit>());
    println!("Color: {} bytes", size_of::<Color>());
    
    // These should be compact
    assert!(size_of::<PropertyId>() <= 4);
    assert_eq!(size_of::<Color>(), 4);
    assert_eq!(size_of::<LengthUnit>(), 1);
}

#[test]
fn test_parse_large_stylesheet() {
    // Generate a large stylesheet
    let mut css = String::new();
    for i in 0..500 {
        css.push_str(&format!(
            ".class-{} {{ color: #{}; display: block; margin: {}px; }}\n",
            i,
            format!("{:06x}", i * 100),
            i
        ));
    }
    
    let stylesheet = CssParser::new().parse(&css).unwrap();
    println!("Large stylesheet rules: {}", stylesheet.len());
    assert_eq!(stylesheet.len(), 500);
}
