//! Comprehensive tests for fos-html
//!
//! Tests parsing edge cases and memory efficiency.

use fos_html::{HtmlParser, Document};

#[test]
fn test_parse_minimal_html() {
    let doc = HtmlParser::new().parse("");
    assert!(doc.tree().len() >= 1, "Even empty HTML should have root");
}

#[test]
fn test_parse_text_only() {
    let doc = HtmlParser::new().parse("Hello World");
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_self_closing_tags() {
    let html = r#"<br><hr><img src="test.png"><input type="text">"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_nested_structure() {
    let html = r#"
        <html>
            <head>
                <title>Test Page</title>
                <meta charset="utf-8">
            </head>
            <body>
                <div id="container">
                    <h1>Welcome</h1>
                    <p class="intro">This is a test.</p>
                    <ul>
                        <li>Item 1</li>
                        <li>Item 2</li>
                        <li>Item 3</li>
                    </ul>
                </div>
            </body>
        </html>
    "#;
    
    let doc = HtmlParser::new().parse(html);
    println!("Nested structure node count: {}", doc.tree().len());
    assert!(doc.tree().len() > 10);
}

#[test]
fn test_parse_malformed_html() {
    // HTML5 parser should handle malformed HTML gracefully
    let html = r#"
        <div>
            <p>Unclosed paragraph
            <span>Unclosed span
        </div>
        <p>Another paragraph without closing
    "#;
    
    let doc = HtmlParser::new().parse(html);
    // Should not panic, should create some structure
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_with_attributes() {
    let html = r#"
        <div id="main" class="container primary" data-value="123">
            <a href="https://example.com" target="_blank">Link</a>
        </div>
    "#;
    
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_script_and_style() {
    let html = r#"
        <html>
            <head>
                <style>
                    body { background: red; }
                    .foo { color: blue; }
                </style>
                <script>
                    function foo() {
                        return "<div>not parsed</div>";
                    }
                </script>
            </head>
            <body>
                <p>Content</p>
            </body>
        </html>
    "#;
    
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_entities() {
    let html = r#"<p>&lt;tag&gt; &amp; &quot;quotes&quot; &nbsp; &#169;</p>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_unicode() {
    let html = r#"<p>Hello 世界! 🚀 Ñoño</p>"#;
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_comments() {
    let html = r#"
        <!-- This is a comment -->
        <div>
            <!-- Another comment
                 spanning multiple lines -->
            <p>Content</p>
        </div>
    "#;
    
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 1);
}

#[test]
fn test_parse_large_document() {
    // Generate a large document
    let mut html = String::from("<html><body>");
    for i in 0..1000 {
        html.push_str(&format!(
            r#"<div id="div-{}" class="item"><p>Paragraph {}</p></div>"#,
            i, i
        ));
    }
    html.push_str("</body></html>");
    
    let doc = HtmlParser::new().parse(&html);
    
    println!("Large document nodes: {}", doc.tree().len());
    println!("Large document memory: {} bytes", doc.memory_usage());
    println!("Bytes per node: {:.2}", doc.memory_usage() as f64 / doc.tree().len() as f64);
    
    // Should handle 1000 divs with content
    assert!(doc.tree().len() > 2000);
}

#[test]
fn test_parse_table() {
    let html = r#"
        <table>
            <thead>
                <tr><th>Header 1</th><th>Header 2</th></tr>
            </thead>
            <tbody>
                <tr><td>Cell 1</td><td>Cell 2</td></tr>
                <tr><td>Cell 3</td><td>Cell 4</td></tr>
            </tbody>
        </table>
    "#;
    
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 10);
}

#[test]
fn test_parse_forms() {
    let html = r#"
        <form action="/submit" method="post">
            <label for="name">Name:</label>
            <input type="text" id="name" name="name" required>
            <label for="email">Email:</label>
            <input type="email" id="email" name="email">
            <select name="country">
                <option value="us">USA</option>
                <option value="uk">UK</option>
            </select>
            <textarea name="message" rows="5"></textarea>
            <button type="submit">Submit</button>
        </form>
    "#;
    
    let doc = HtmlParser::new().parse(html);
    assert!(doc.tree().len() > 10);
}
