//! Parallel CSS Parser
//!
//! Parse multiple stylesheets in parallel using custom thread primitives.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Parsed stylesheet representation
#[derive(Debug, Clone)]
pub struct Stylesheet {
    /// Source URL or identifier
    pub source: String,
    /// Parsed rules
    pub rules: Vec<Rule>,
    /// Parse errors
    pub errors: Vec<CssParseError>,
}

impl Default for Stylesheet {
    fn default() -> Self {
        Self {
            source: String::new(),
            rules: Vec::new(),
            errors: Vec::new(),
        }
    }
}

/// CSS rule
#[derive(Debug, Clone)]
pub enum Rule {
    /// Style rule with selectors and declarations
    Style(StyleRule),
    /// @import rule
    Import(ImportRule),
    /// @media rule
    Media(MediaRule),
    /// @font-face rule
    FontFace(FontFaceRule),
    /// @keyframes rule
    Keyframes(KeyframesRule),
    /// @supports rule
    Supports(SupportsRule),
    /// @layer rule
    Layer(LayerRule),
    /// @container rule
    Container(ContainerRule),
}

/// Style rule (selector + declarations)
#[derive(Debug, Clone)]
pub struct StyleRule {
    /// Selector text
    pub selectors: Vec<String>,
    /// Declarations
    pub declarations: Vec<Declaration>,
    /// Specificity (a, b, c)
    pub specificity: (u32, u32, u32),
}

/// Property declaration
#[derive(Debug, Clone)]
pub struct Declaration {
    /// Property name
    pub property: String,
    /// Property value
    pub value: String,
    /// Is important
    pub important: bool,
}

/// @import rule
#[derive(Debug, Clone)]
pub struct ImportRule {
    /// URL to import
    pub url: String,
    /// Media query
    pub media: Option<String>,
    /// Layer name
    pub layer: Option<String>,
    /// Supports condition
    pub supports: Option<String>,
}

/// @media rule
#[derive(Debug, Clone)]
pub struct MediaRule {
    /// Media query
    pub query: String,
    /// Nested rules
    pub rules: Vec<Rule>,
}

/// @font-face rule
#[derive(Debug, Clone)]
pub struct FontFaceRule {
    /// Font family name
    pub family: String,
    /// Source URLs
    pub src: Vec<String>,
    /// Font weight
    pub weight: Option<String>,
    /// Font style
    pub style: Option<String>,
    /// Unicode range
    pub unicode_range: Option<String>,
}

/// @keyframes rule
#[derive(Debug, Clone)]
pub struct KeyframesRule {
    /// Animation name
    pub name: String,
    /// Keyframe selectors and declarations
    pub keyframes: Vec<Keyframe>,
}

/// A single keyframe
#[derive(Debug, Clone)]
pub struct Keyframe {
    /// Percentage (0-100) or 'from'/'to'
    pub selector: KeyframeSelector,
    /// Declarations
    pub declarations: Vec<Declaration>,
}

/// Keyframe selector
#[derive(Debug, Clone)]
pub enum KeyframeSelector {
    From,
    To,
    Percentage(f32),
}

/// @supports rule
#[derive(Debug, Clone)]
pub struct SupportsRule {
    /// Condition
    pub condition: String,
    /// Nested rules
    pub rules: Vec<Rule>,
}

/// @layer rule
#[derive(Debug, Clone)]
pub struct LayerRule {
    /// Layer name(s)
    pub names: Vec<String>,
    /// Nested rules (if block rule)
    pub rules: Vec<Rule>,
}

/// @container rule
#[derive(Debug, Clone)]
pub struct ContainerRule {
    /// Container name
    pub name: Option<String>,
    /// Container query
    pub query: String,
    /// Nested rules
    pub rules: Vec<Rule>,
}

/// CSS parse error
#[derive(Debug, Clone)]
pub struct CssParseError {
    /// Error message
    pub message: String,
    /// Line number
    pub line: usize,
    /// Column number
    pub column: usize,
}

/// Parse multiple stylesheets in parallel
pub fn parse_stylesheets_parallel(sheets: Vec<(&str, &str)>) -> Vec<Stylesheet> {
    let num_threads = thread::available_parallelism()
        .map(|p| p.get())
        .unwrap_or(4);
    
    if sheets.len() <= 1 || num_threads <= 1 {
        return sheets
            .into_iter()
            .map(|(source, css)| parse_stylesheet(source, css))
            .collect();
    }
    
    // Convert to owned data for thread safety
    let sheets: Vec<(String, String)> = sheets
        .into_iter()
        .map(|(s, c)| (s.to_string(), c.to_string()))
        .collect();
    
    let chunk_size = (sheets.len() + num_threads - 1) / num_threads;
    let results = Arc::new(Mutex::new(vec![None; sheets.len()]));
    
    let chunks: Vec<Vec<(usize, String, String)>> = sheets
        .into_iter()
        .enumerate()
        .collect::<Vec<_>>()
        .chunks(chunk_size)
        .map(|chunk| chunk.iter().map(|(i, (s, c))| (*i, s.clone(), c.clone())).collect())
        .collect();
    
    let handles: Vec<_> = chunks
        .into_iter()
        .map(|chunk| {
            let results = Arc::clone(&results);
            thread::spawn(move || {
                for (idx, source, css) in chunk {
                    let stylesheet = parse_stylesheet(&source, &css);
                    let mut guard = results.lock().unwrap();
                    guard[idx] = Some(stylesheet);
                }
            })
        })
        .collect();
    
    for handle in handles {
        let _ = handle.join();
    }
    
    let guard = results.lock().unwrap();
    guard.iter()
        .map(|opt| opt.clone().unwrap_or_default())
        .collect()
}

/// Parse a single stylesheet
pub fn parse_stylesheet(source: &str, css: &str) -> Stylesheet {
    let mut stylesheet = Stylesheet {
        source: source.to_string(),
        rules: Vec::new(),
        errors: Vec::new(),
    };
    
    let mut parser = CssParser::new(css);
    
    while !parser.is_eof() {
        parser.skip_whitespace_and_comments();
        
        if parser.is_eof() {
            break;
        }
        
        match parser.parse_rule() {
            Ok(rule) => stylesheet.rules.push(rule),
            Err(err) => {
                stylesheet.errors.push(err);
                // Skip to next rule
                parser.skip_to_next_rule();
            }
        }
    }
    
    stylesheet
}

/// CSS Parser
struct CssParser<'a> {
    input: &'a str,
    pos: usize,
    line: usize,
    column: usize,
}

impl<'a> CssParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            pos: 0,
            line: 1,
            column: 1,
        }
    }
    
    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }
    
    fn current(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }
    
    fn peek(&self, n: usize) -> Option<char> {
        self.input[self.pos..].chars().nth(n)
    }
    
    fn advance(&mut self) {
        if let Some(c) = self.current() {
            self.pos += c.len_utf8();
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        }
    }
    
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }
    
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            self.skip_whitespace();
            
            // Check for comments
            if self.starts_with("/*") {
                self.skip_comment();
            } else {
                break;
            }
        }
    }
    
    fn skip_comment(&mut self) {
        if self.starts_with("/*") {
            self.advance(); // /
            self.advance(); // *
            
            while !self.is_eof() {
                if self.starts_with("*/") {
                    self.advance();
                    self.advance();
                    break;
                }
                self.advance();
            }
        }
    }
    
    fn starts_with(&self, s: &str) -> bool {
        self.input[self.pos..].starts_with(s)
    }
    
    fn consume_until(&mut self, end: char) -> String {
        let mut result = String::new();
        while let Some(c) = self.current() {
            if c == end {
                break;
            }
            result.push(c);
            self.advance();
        }
        result
    }
    
    fn consume_string(&mut self) -> Result<String, CssParseError> {
        let quote = self.current().ok_or_else(|| self.error("Expected string"))?;
        if quote != '"' && quote != '\'' {
            return Err(self.error("Expected quote"));
        }
        
        self.advance(); // opening quote
        
        let mut result = String::new();
        while let Some(c) = self.current() {
            if c == quote {
                self.advance();
                return Ok(result);
            } else if c == '\\' {
                self.advance();
                if let Some(escaped) = self.current() {
                    result.push(escaped);
                    self.advance();
                }
            } else {
                result.push(c);
                self.advance();
            }
        }
        
        Ok(result)
    }
    
    fn consume_ident(&mut self) -> String {
        let mut result = String::new();
        while let Some(c) = self.current() {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                result.push(c);
                self.advance();
            } else {
                break;
            }
        }
        result
    }
    
    fn skip_to_next_rule(&mut self) {
        let mut brace_depth = 0;
        
        while let Some(c) = self.current() {
            if c == '{' {
                brace_depth += 1;
            } else if c == '}' {
                if brace_depth == 0 {
                    self.advance();
                    return;
                }
                brace_depth -= 1;
            } else if c == ';' && brace_depth == 0 {
                self.advance();
                return;
            }
            self.advance();
        }
    }
    
    fn error(&self, message: &str) -> CssParseError {
        CssParseError {
            message: message.to_string(),
            line: self.line,
            column: self.column,
        }
    }
    
    fn parse_rule(&mut self) -> Result<Rule, CssParseError> {
        self.skip_whitespace_and_comments();
        
        // Check for at-rules
        if self.current() == Some('@') {
            return self.parse_at_rule();
        }
        
        // Must be a style rule
        self.parse_style_rule()
    }
    
    fn parse_at_rule(&mut self) -> Result<Rule, CssParseError> {
        self.advance(); // @
        let name = self.consume_ident();
        self.skip_whitespace();
        
        match name.to_lowercase().as_str() {
            "import" => self.parse_import_rule(),
            "media" => self.parse_media_rule(),
            "font-face" => self.parse_font_face_rule(),
            "keyframes" | "-webkit-keyframes" => self.parse_keyframes_rule(),
            "supports" => self.parse_supports_rule(),
            "layer" => self.parse_layer_rule(),
            "container" => self.parse_container_rule(),
            _ => {
                // Unknown at-rule, skip it
                self.skip_to_next_rule();
                Err(self.error(&format!("Unknown at-rule: @{}", name)))
            }
        }
    }
    
    fn parse_import_rule(&mut self) -> Result<Rule, CssParseError> {
        self.skip_whitespace();
        
        let url = if self.current() == Some('"') || self.current() == Some('\'') {
            self.consume_string()?
        } else if self.starts_with("url(") {
            self.parse_url()?
        } else {
            return Err(self.error("Expected URL in @import"));
        };
        
        self.skip_whitespace();
        
        // Optional media/layer/supports
        let mut media = None;
        let mut layer = None;
        let mut supports = None;
        
        while !self.is_eof() && self.current() != Some(';') {
            let ident = self.consume_ident();
            self.skip_whitespace();
            
            if ident == "layer" {
                if self.current() == Some('(') {
                    self.advance();
                    layer = Some(self.consume_until(')'));
                    self.advance();
                }
            } else if ident == "supports" {
                if self.current() == Some('(') {
                    self.advance();
                    supports = Some(self.consume_until(')'));
                    self.advance();
                }
            } else if !ident.is_empty() {
                media = Some(ident);
            }
            
            self.skip_whitespace();
        }
        
        if self.current() == Some(';') {
            self.advance();
        }
        
        Ok(Rule::Import(ImportRule { url, media, layer, supports }))
    }
    
    fn parse_url(&mut self) -> Result<String, CssParseError> {
        // url( ... )
        for _ in 0..4 {
            self.advance(); // u, r, l, (
        }
        
        self.skip_whitespace();
        
        let url = if self.current() == Some('"') || self.current() == Some('\'') {
            self.consume_string()?
        } else {
            self.consume_until(')')
        };
        
        self.skip_whitespace();
        
        if self.current() == Some(')') {
            self.advance();
        }
        
        Ok(url.trim().to_string())
    }
    
    fn parse_media_rule(&mut self) -> Result<Rule, CssParseError> {
        let query = self.consume_until('{').trim().to_string();
        
        if self.current() == Some('{') {
            self.advance();
        }
        
        let rules = self.parse_rule_list()?;
        
        Ok(Rule::Media(MediaRule { query, rules }))
    }
    
    fn parse_font_face_rule(&mut self) -> Result<Rule, CssParseError> {
        self.skip_whitespace();
        
        if self.current() != Some('{') {
            return Err(self.error("Expected { in @font-face"));
        }
        self.advance();
        
        let declarations = self.parse_declarations()?;
        
        let mut rule = FontFaceRule {
            family: String::new(),
            src: Vec::new(),
            weight: None,
            style: None,
            unicode_range: None,
        };
        
        for decl in declarations {
            match decl.property.as_str() {
                "font-family" => rule.family = decl.value.trim_matches(|c| c == '"' || c == '\'').to_string(),
                "src" => rule.src = vec![decl.value],
                "font-weight" => rule.weight = Some(decl.value),
                "font-style" => rule.style = Some(decl.value),
                "unicode-range" => rule.unicode_range = Some(decl.value),
                _ => {}
            }
        }
        
        Ok(Rule::FontFace(rule))
    }
    
    fn parse_keyframes_rule(&mut self) -> Result<Rule, CssParseError> {
        self.skip_whitespace();
        let name = self.consume_ident();
        self.skip_whitespace();
        
        if self.current() != Some('{') {
            return Err(self.error("Expected { in @keyframes"));
        }
        self.advance();
        
        let mut keyframes = Vec::new();
        
        loop {
            self.skip_whitespace_and_comments();
            
            if self.current() == Some('}') || self.is_eof() {
                self.advance();
                break;
            }
            
            // Parse keyframe selector
            let selector_text = self.consume_until('{').trim().to_string();
            let selector = if selector_text == "from" {
                KeyframeSelector::From
            } else if selector_text == "to" {
                KeyframeSelector::To
            } else {
                let pct = selector_text.trim_end_matches('%').parse().unwrap_or(0.0);
                KeyframeSelector::Percentage(pct)
            };
            
            if self.current() == Some('{') {
                self.advance();
            }
            
            let declarations = self.parse_declarations()?;
            keyframes.push(Keyframe { selector, declarations });
        }
        
        Ok(Rule::Keyframes(KeyframesRule { name, keyframes }))
    }
    
    fn parse_supports_rule(&mut self) -> Result<Rule, CssParseError> {
        let condition = self.consume_until('{').trim().to_string();
        
        if self.current() == Some('{') {
            self.advance();
        }
        
        let rules = self.parse_rule_list()?;
        
        Ok(Rule::Supports(SupportsRule { condition, rules }))
    }
    
    fn parse_layer_rule(&mut self) -> Result<Rule, CssParseError> {
        self.skip_whitespace();
        
        let mut names = Vec::new();
        
        // Parse layer names
        loop {
            let name = self.consume_ident();
            if name.is_empty() {
                break;
            }
            names.push(name);
            self.skip_whitespace();
            
            if self.current() == Some(',') {
                self.advance();
                self.skip_whitespace();
            } else {
                break;
            }
        }
        
        if self.current() == Some(';') {
            // Statement form
            self.advance();
            return Ok(Rule::Layer(LayerRule { names, rules: Vec::new() }));
        }
        
        if self.current() == Some('{') {
            self.advance();
            let rules = self.parse_rule_list()?;
            return Ok(Rule::Layer(LayerRule { names, rules }));
        }
        
        Err(self.error("Invalid @layer rule"))
    }
    
    fn parse_container_rule(&mut self) -> Result<Rule, CssParseError> {
        self.skip_whitespace();
        
        let mut name = None;
        let ident = self.consume_ident();
        self.skip_whitespace();
        
        if !ident.is_empty() && self.current() != Some('(') {
            name = Some(ident);
        }
        
        let query = self.consume_until('{').trim().to_string();
        
        if self.current() == Some('{') {
            self.advance();
        }
        
        let rules = self.parse_rule_list()?;
        
        Ok(Rule::Container(ContainerRule { name, query, rules }))
    }
    
    fn parse_rule_list(&mut self) -> Result<Vec<Rule>, CssParseError> {
        let mut rules = Vec::new();
        
        loop {
            self.skip_whitespace_and_comments();
            
            if self.current() == Some('}') || self.is_eof() {
                if self.current() == Some('}') {
                    self.advance();
                }
                break;
            }
            
            match self.parse_rule() {
                Ok(rule) => rules.push(rule),
                Err(_) => {
                    self.skip_to_next_rule();
                }
            }
        }
        
        Ok(rules)
    }
    
    fn parse_style_rule(&mut self) -> Result<Rule, CssParseError> {
        // Parse selectors
        let selectors_text = self.consume_until('{');
        let selectors: Vec<String> = selectors_text
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        if self.current() != Some('{') {
            return Err(self.error("Expected { in style rule"));
        }
        self.advance();
        
        let declarations = self.parse_declarations()?;
        
        // Calculate approximate specificity for first selector
        let specificity = calculate_specificity(selectors.first().map(|s| s.as_str()).unwrap_or(""));
        
        Ok(Rule::Style(StyleRule {
            selectors,
            declarations,
            specificity,
        }))
    }
    
    fn parse_declarations(&mut self) -> Result<Vec<Declaration>, CssParseError> {
        let mut declarations = Vec::new();
        
        loop {
            self.skip_whitespace_and_comments();
            
            if self.current() == Some('}') || self.is_eof() {
                if self.current() == Some('}') {
                    self.advance();
                }
                break;
            }
            
            let property = self.consume_ident();
            if property.is_empty() {
                self.advance();
                continue;
            }
            
            self.skip_whitespace();
            
            if self.current() != Some(':') {
                self.skip_to_next_rule();
                continue;
            }
            self.advance();
            self.skip_whitespace();
            
            // Parse value (handle nested parens for functions)
            let mut value = String::new();
            let mut paren_depth = 0;
            
            while let Some(c) = self.current() {
                if c == '(' {
                    paren_depth += 1;
                    value.push(c);
                    self.advance();
                } else if c == ')' {
                    paren_depth -= 1;
                    value.push(c);
                    self.advance();
                } else if (c == ';' || c == '}') && paren_depth == 0 {
                    break;
                } else {
                    value.push(c);
                    self.advance();
                }
            }
            
            let value = value.trim().to_string();
            let important = value.contains("!important");
            let value = value.replace("!important", "").trim().to_string();
            
            declarations.push(Declaration {
                property,
                value,
                important,
            });
            
            if self.current() == Some(';') {
                self.advance();
            }
        }
        
        Ok(declarations)
    }
}

/// Calculate selector specificity (a, b, c)
fn calculate_specificity(selector: &str) -> (u32, u32, u32) {
    let mut a = 0; // IDs
    let mut b = 0; // Classes, attributes, pseudo-classes
    let mut c = 0; // Elements, pseudo-elements
    
    let mut chars = selector.chars().peekable();
    
    while let Some(ch) = chars.next() {
        match ch {
            '#' => a += 1,
            '.' | '[' => b += 1,
            ':' => {
                if chars.peek() == Some(&':') {
                    chars.next();
                    c += 1; // Pseudo-element
                } else {
                    // Check for pseudo-class exceptions
                    let pseudo: String = chars.by_ref().take_while(|c| c.is_alphanumeric() || *c == '-').collect();
                    if pseudo != "where" && pseudo != "is" {
                        if pseudo == "not" || pseudo == "has" {
                            // These add specificity of their argument
                            b += 1;
                        } else {
                            b += 1;
                        }
                    }
                }
            }
            ch if ch.is_alphabetic() => {
                c += 1;
                // Skip rest of tag name
                while chars.peek().map(|x| x.is_alphanumeric() || *x == '-').unwrap_or(false) {
                    chars.next();
                }
            }
            _ => {}
        }
    }
    
    (a, b, c)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_rule() {
        let css = "div { color: red; }";
        let stylesheet = parse_stylesheet("test.css", css);
        
        assert_eq!(stylesheet.rules.len(), 1);
        if let Rule::Style(rule) = &stylesheet.rules[0] {
            assert_eq!(rule.selectors, vec!["div"]);
            assert_eq!(rule.declarations.len(), 1);
            assert_eq!(rule.declarations[0].property, "color");
            assert_eq!(rule.declarations[0].value, "red");
        } else {
            panic!("Expected style rule");
        }
    }
    
    #[test]
    fn test_parse_multiple_selectors() {
        let css = "h1, h2, h3 { font-weight: bold; }";
        let stylesheet = parse_stylesheet("test.css", css);
        
        assert_eq!(stylesheet.rules.len(), 1);
        if let Rule::Style(rule) = &stylesheet.rules[0] {
            assert_eq!(rule.selectors.len(), 3);
        }
    }
    
    #[test]
    fn test_parse_media_rule() {
        let css = "@media screen and (min-width: 768px) { body { font-size: 16px; } }";
        let stylesheet = parse_stylesheet("test.css", css);
        
        assert!(!stylesheet.rules.is_empty());
        if let Rule::Media(rule) = &stylesheet.rules[0] {
            assert!(rule.query.contains("screen"));
            assert_eq!(rule.rules.len(), 1);
        }
    }
    
    #[test]
    fn test_parse_import() {
        let css = "@import \"reset.css\";";
        let stylesheet = parse_stylesheet("test.css", css);
        
        if let Rule::Import(rule) = &stylesheet.rules[0] {
            assert_eq!(rule.url, "reset.css");
        }
    }
    
    #[test]
    fn test_parse_keyframes() {
        let css = "@keyframes fade { from { opacity: 0; } to { opacity: 1; } }";
        let stylesheet = parse_stylesheet("test.css", css);
        
        if let Rule::Keyframes(rule) = &stylesheet.rules[0] {
            assert_eq!(rule.name, "fade");
            assert_eq!(rule.keyframes.len(), 2);
        }
    }
    
    #[test]
    fn test_parallel_parsing() {
        let sheets = vec![
            ("a.css", "div { color: red; }"),
            ("b.css", "span { color: blue; }"),
            ("c.css", "p { color: green; }"),
        ];
        
        let results = parse_stylesheets_parallel(sheets);
        assert_eq!(results.len(), 3);
        assert_eq!(results[0].source, "a.css");
        assert_eq!(results[1].source, "b.css");
        assert_eq!(results[2].source, "c.css");
    }
    
    #[test]
    fn test_specificity() {
        assert_eq!(calculate_specificity("div"), (0, 0, 1));
        assert_eq!(calculate_specificity(".class"), (0, 1, 0));
        assert_eq!(calculate_specificity("#id"), (1, 0, 0));
        assert_eq!(calculate_specificity("div.class#id"), (1, 1, 1));
    }
}
