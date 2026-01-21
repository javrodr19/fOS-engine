//! PreParser for Fast Function Scanning
//!
//! Performs fast scanning of JavaScript functions without building a full AST.
//! Used for lazy compilation - only fully parse functions when they're called.
//! This can reduce initial parse time by 50%+ for large codebases.

use std::collections::HashSet;

/// Information gathered from pre-parsing a function
#[derive(Debug, Clone, Default)]
pub struct FunctionInfo {
    /// Number of parameters
    pub param_count: u8,
    /// Whether function uses `arguments` keyword
    pub uses_arguments: bool,
    /// Whether function calls `eval()`
    pub uses_eval: bool,
    /// Whether function is in strict mode
    pub is_strict: bool,
    /// Whether function is async
    pub is_async: bool,
    /// Whether function is a generator
    pub is_generator: bool,
    /// Whether function uses `this`
    pub uses_this: bool,
    /// Whether function uses `super`
    pub uses_super: bool,
    /// Source start offset
    pub source_start: u32,
    /// Source end offset
    pub source_end: u32,
    /// Captured variable names (for closure analysis)
    pub captured_vars: Vec<Box<str>>,
}

/// Pre-parser state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreParseState {
    /// Normal code scanning
    Normal,
    /// Inside a string literal
    String(char), // quote character
    /// Inside a template literal
    Template,
    /// Inside a regular expression
    Regex,
    /// Inside a single-line comment
    LineComment,
    /// Inside a block comment
    BlockComment,
}

/// Fast pre-parser for function scanning
///
/// Scans function bodies to gather metadata without building an AST.
/// Uses a state machine to handle strings, comments, and nested braces.
#[derive(Debug)]
pub struct PreParser<'src> {
    /// Source code
    source: &'src str,
    /// Current position in source
    pos: usize,
    /// Current state
    state: PreParseState,
    /// Brace nesting depth
    brace_depth: u32,
    /// Paren nesting depth
    paren_depth: u32,
    /// Bracket nesting depth
    bracket_depth: u32,
    /// Keywords found
    found_arguments: bool,
    found_eval: bool,
    found_this: bool,
    found_super: bool,
    found_use_strict: bool,
}

impl<'src> PreParser<'src> {
    /// Create a new pre-parser
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            pos: 0,
            state: PreParseState::Normal,
            brace_depth: 0,
            paren_depth: 0,
            bracket_depth: 0,
            found_arguments: false,
            found_eval: false,
            found_this: false,
            found_super: false,
            found_use_strict: false,
        }
    }

    /// Scan a function body starting at current position
    /// Returns FunctionInfo and the end position
    pub fn scan_function(&mut self, start: usize) -> FunctionInfo {
        self.pos = start;
        self.state = PreParseState::Normal;
        self.brace_depth = 0;
        self.found_arguments = false;
        self.found_eval = false;
        self.found_this = false;
        self.found_super = false;
        self.found_use_strict = false;

        // Count parameters first
        let param_count = self.count_params();
        
        // Skip to function body
        self.skip_to_body();
        let body_start = self.pos;
        
        // Scan body for keywords
        self.scan_body();
        
        FunctionInfo {
            param_count,
            uses_arguments: self.found_arguments,
            uses_eval: self.found_eval,
            is_strict: self.found_use_strict,
            is_async: false, // Set by caller
            is_generator: false, // Set by caller
            uses_this: self.found_this,
            uses_super: self.found_super,
            source_start: body_start as u32,
            source_end: self.pos as u32,
            captured_vars: Vec::new(),
        }
    }

    /// Count parameters (fast scan)
    fn count_params(&mut self) -> u8 {
        // Find opening paren
        while self.pos < self.source.len() {
            if self.current_char() == Some('(') {
                self.pos += 1;
                break;
            }
            self.pos += 1;
        }

        let mut count = 0u8;
        let mut depth = 1u32;
        let mut in_param = false;

        while self.pos < self.source.len() && depth > 0 {
            match self.current_char() {
                Some('(') => depth += 1,
                Some(')') => {
                    depth -= 1;
                    if depth == 0 && in_param {
                        count = count.saturating_add(1);
                    }
                }
                Some(',') if depth == 1 => {
                    if in_param {
                        count = count.saturating_add(1);
                        in_param = false;
                    }
                }
                Some(c) if !c.is_whitespace() && depth == 1 => {
                    in_param = true;
                }
                _ => {}
            }
            self.pos += 1;
        }

        count
    }

    /// Skip to function body (past the opening brace)
    fn skip_to_body(&mut self) {
        while self.pos < self.source.len() {
            if self.current_char() == Some('{') {
                self.pos += 1;
                self.brace_depth = 1;
                return;
            }
            self.pos += 1;
        }
    }

    /// Scan function body for keywords
    fn scan_body(&mut self) {
        while self.pos < self.source.len() && self.brace_depth > 0 {
            match self.state {
                PreParseState::Normal => self.scan_normal(),
                PreParseState::String(quote) => self.scan_string(quote),
                PreParseState::Template => self.scan_template(),
                PreParseState::Regex => self.scan_regex(),
                PreParseState::LineComment => self.scan_line_comment(),
                PreParseState::BlockComment => self.scan_block_comment(),
            }
        }
    }

    /// Scan in normal mode
    fn scan_normal(&mut self) {
        let c = match self.current_char() {
            Some(c) => c,
            None => return,
        };

        match c {
            '{' => {
                self.brace_depth += 1;
                self.pos += 1;
            }
            '}' => {
                self.brace_depth = self.brace_depth.saturating_sub(1);
                self.pos += 1;
            }
            '(' => {
                self.paren_depth += 1;
                self.pos += 1;
            }
            ')' => {
                self.paren_depth = self.paren_depth.saturating_sub(1);
                self.pos += 1;
            }
            '[' => {
                self.bracket_depth += 1;
                self.pos += 1;
            }
            ']' => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
                self.pos += 1;
            }
            '"' | '\'' => {
                self.state = PreParseState::String(c);
                self.pos += 1;
            }
            '`' => {
                self.state = PreParseState::Template;
                self.pos += 1;
            }
            '/' => {
                self.pos += 1;
                match self.current_char() {
                    Some('/') => {
                        self.state = PreParseState::LineComment;
                        self.pos += 1;
                    }
                    Some('*') => {
                        self.state = PreParseState::BlockComment;
                        self.pos += 1;
                    }
                    _ => {
                        // Could be regex or division - simplified: assume division
                        // Full parser handles this correctly
                    }
                }
            }
            c if c.is_ascii_alphabetic() || c == '_' || c == '$' => {
                self.scan_identifier();
            }
            _ => {
                self.pos += 1;
            }
        }
    }

    /// Scan an identifier and check for keywords
    fn scan_identifier(&mut self) {
        let start = self.pos;
        
        while self.pos < self.source.len() {
            match self.current_char() {
                Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '$' => {
                    self.pos += 1;
                }
                _ => break,
            }
        }

        let ident = &self.source[start..self.pos];
        
        match ident {
            "arguments" => self.found_arguments = true,
            "eval" => self.found_eval = true,
            "this" => self.found_this = true,
            "super" => self.found_super = true,
            _ => {}
        }

        // Check for "use strict" at start of function
        if ident == "use" && self.brace_depth == 1 {
            self.check_use_strict();
        }
    }

    /// Check for "use strict" directive
    fn check_use_strict(&mut self) {
        // Skip whitespace
        while self.pos < self.source.len() && self.current_char().map(|c| c.is_whitespace()).unwrap_or(false) {
            self.pos += 1;
        }

        // Check for string "strict"
        let remaining = &self.source[self.pos..];
        if remaining.starts_with("\"strict\"") || remaining.starts_with("'strict'") {
            self.found_use_strict = true;
        }
    }

    /// Scan string literal
    fn scan_string(&mut self, quote: char) {
        while self.pos < self.source.len() {
            match self.current_char() {
                Some('\\') => {
                    self.pos += 2; // Skip escape sequence
                }
                Some(c) if c == quote => {
                    self.pos += 1;
                    self.state = PreParseState::Normal;
                    return;
                }
                Some('\n') => {
                    // Unterminated string
                    self.state = PreParseState::Normal;
                    return;
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
    }

    /// Scan template literal
    fn scan_template(&mut self) {
        while self.pos < self.source.len() {
            match self.current_char() {
                Some('\\') => {
                    self.pos += 2;
                }
                Some('`') => {
                    self.pos += 1;
                    self.state = PreParseState::Normal;
                    return;
                }
                Some('$') => {
                    self.pos += 1;
                    if self.current_char() == Some('{') {
                        // Template expression - simplified handling
                        self.pos += 1;
                        self.scan_template_expression();
                    }
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
    }

    /// Scan template expression ${...}
    fn scan_template_expression(&mut self) {
        let mut depth = 1u32;
        while self.pos < self.source.len() && depth > 0 {
            match self.current_char() {
                Some('{') => {
                    depth += 1;
                    self.pos += 1;
                }
                Some('}') => {
                    depth -= 1;
                    self.pos += 1;
                }
                Some('"') | Some('\'') => {
                    let quote = self.current_char().unwrap();
                    self.pos += 1;
                    self.scan_string(quote);
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
    }

    /// Scan regex literal
    fn scan_regex(&mut self) {
        while self.pos < self.source.len() {
            match self.current_char() {
                Some('\\') => {
                    self.pos += 2;
                }
                Some('/') => {
                    self.pos += 1;
                    // Skip flags
                    while self.pos < self.source.len() {
                        match self.current_char() {
                            Some(c) if c.is_ascii_alphabetic() => self.pos += 1,
                            _ => break,
                        }
                    }
                    self.state = PreParseState::Normal;
                    return;
                }
                Some('\n') => {
                    self.state = PreParseState::Normal;
                    return;
                }
                _ => {
                    self.pos += 1;
                }
            }
        }
    }

    /// Scan line comment
    fn scan_line_comment(&mut self) {
        while self.pos < self.source.len() {
            if self.current_char() == Some('\n') {
                self.pos += 1;
                self.state = PreParseState::Normal;
                return;
            }
            self.pos += 1;
        }
        self.state = PreParseState::Normal;
    }

    /// Scan block comment
    fn scan_block_comment(&mut self) {
        while self.pos < self.source.len() {
            if self.current_char() == Some('*') {
                self.pos += 1;
                if self.current_char() == Some('/') {
                    self.pos += 1;
                    self.state = PreParseState::Normal;
                    return;
                }
            } else {
                self.pos += 1;
            }
        }
        self.state = PreParseState::Normal;
    }

    /// Get current character
    fn current_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    /// Check if source contains `arguments` keyword (quick check)
    pub fn has_arguments_keyword(source: &str) -> bool {
        source.contains("arguments")
    }

    /// Check if source contains `eval` keyword (quick check)
    pub fn has_eval(source: &str) -> bool {
        source.contains("eval")
    }

    /// Detect strict mode (quick check)
    pub fn detect_strict(source: &str) -> bool {
        let trimmed = source.trim_start();
        trimmed.starts_with("\"use strict\"") || trimmed.starts_with("'use strict'")
    }
}

/// Streaming source buffer for incremental parsing
#[derive(Debug)]
pub struct StreamingSource {
    /// Accumulated source chunks
    chunks: Vec<String>,
    /// Total bytes received
    total_bytes: usize,
    /// Whether stream is complete
    is_complete: bool,
}

impl StreamingSource {
    /// Create new streaming source
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            total_bytes: 0,
            is_complete: false,
        }
    }

    /// Add a chunk of source
    pub fn add_chunk(&mut self, chunk: String) {
        self.total_bytes += chunk.len();
        self.chunks.push(chunk);
    }

    /// Mark stream as complete
    pub fn complete(&mut self) {
        self.is_complete = true;
    }

    /// Check if we have enough to start parsing
    pub fn can_start_parsing(&self) -> bool {
        // Can start when we have at least some content
        self.total_bytes > 0
    }

    /// Get accumulated source
    pub fn source(&self) -> String {
        self.chunks.concat()
    }

    /// Total bytes received
    pub fn bytes_received(&self) -> usize {
        self.total_bytes
    }

    /// Is stream complete
    pub fn is_complete(&self) -> bool {
        self.is_complete
    }
}

impl Default for StreamingSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Arrow function detection heuristics
pub struct ArrowDetector;

impl ArrowDetector {
    /// Check if a sequence of tokens might be an arrow function
    /// Returns true if we should try parsing as arrow function
    pub fn might_be_arrow(source: &str, pos: usize) -> bool {
        let remaining = &source[pos..];
        
        // Check for patterns:
        // () =>
        // (a) =>
        // (a, b) =>
        // a =>
        // async () =>
        // async a =>
        
        let trimmed = remaining.trim_start();
        
        // Simple identifier followed by =>
        if let Some(arrow_pos) = trimmed.find("=>") {
            let before_arrow = trimmed[..arrow_pos].trim_end();
            
            // Check if it's a simple identifier
            if Self::is_simple_identifier(before_arrow) {
                return true;
            }
            
            // Check if it's parenthesized parameters
            if before_arrow.ends_with(')') {
                if let Some(open_paren) = before_arrow.rfind('(') {
                    let params = &before_arrow[open_paren..];
                    if Self::looks_like_params(params) {
                        return true;
                    }
                }
            }
        }
        
        false
    }

    /// Check if string is a simple identifier
    fn is_simple_identifier(s: &str) -> bool {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return false;
        }
        
        let mut chars = trimmed.chars();
        let first = chars.next().unwrap();
        
        if !first.is_ascii_alphabetic() && first != '_' && first != '$' {
            return false;
        }
        
        chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
    }

    /// Check if string looks like arrow function parameters
    fn looks_like_params(s: &str) -> bool {
        // Must start with ( and end with )
        if !s.starts_with('(') || !s.ends_with(')') {
            return false;
        }
        
        let inner = &s[1..s.len()-1];
        
        // Empty params is valid
        if inner.trim().is_empty() {
            return true;
        }
        
        // Check for balanced parens and valid-looking content
        let mut depth = 0;
        for c in inner.chars() {
            match c {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                }
                // These characters shouldn't appear in params (usually)
                ';' | '?' if depth == 0 => return false,
                _ => {}
            }
        }
        
        depth == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preparser_simple_function() {
        let source = "function foo(a, b) { return a + b; }";
        let mut pp = PreParser::new(source);
        let info = pp.scan_function(0);
        
        assert_eq!(info.param_count, 2);
        assert!(!info.uses_arguments);
        assert!(!info.uses_eval);
    }

    #[test]
    fn test_preparser_uses_arguments() {
        let source = "function foo() { return arguments[0]; }";
        let mut pp = PreParser::new(source);
        let info = pp.scan_function(0);
        
        assert_eq!(info.param_count, 0);
        assert!(info.uses_arguments);
    }

    #[test]
    fn test_preparser_uses_eval() {
        let source = "function foo(x) { return eval(x); }";
        let mut pp = PreParser::new(source);
        let info = pp.scan_function(0);
        
        assert!(info.uses_eval);
    }

    #[test]
    fn test_preparser_uses_this() {
        let source = "function foo() { return this.value; }";
        let mut pp = PreParser::new(source);
        let info = pp.scan_function(0);
        
        assert!(info.uses_this);
    }

    #[test]
    fn test_preparser_strict_mode() {
        let source = r#"function foo() { "use strict"; return 1; }"#;
        let mut pp = PreParser::new(source);
        let info = pp.scan_function(0);
        
        assert!(info.is_strict);
    }

    #[test]
    fn test_preparser_ignores_string_content() {
        let source = r#"function foo() { return "arguments eval this"; }"#;
        let mut pp = PreParser::new(source);
        let info = pp.scan_function(0);
        
        // Keywords in strings should NOT be detected
        assert!(!info.uses_arguments);
        assert!(!info.uses_eval);
        assert!(!info.uses_this);
    }

    #[test]
    fn test_preparser_nested_braces() {
        let source = "function foo() { if (true) { { } } return 1; }";
        let mut pp = PreParser::new(source);
        let info = pp.scan_function(0);
        
        assert_eq!(info.param_count, 0);
    }

    #[test]
    fn test_arrow_detector_simple() {
        assert!(ArrowDetector::might_be_arrow("x => x + 1", 0));
        assert!(ArrowDetector::might_be_arrow("() => 42", 0));
        assert!(ArrowDetector::might_be_arrow("(a, b) => a + b", 0));
    }

    #[test]
    fn test_arrow_detector_with_parens() {
        assert!(ArrowDetector::might_be_arrow("(x) => x * 2", 0));
        assert!(ArrowDetector::might_be_arrow("  (a, b, c) => { return a; }", 0));
    }

    #[test]
    fn test_streaming_source() {
        let mut stream = StreamingSource::new();
        assert!(!stream.can_start_parsing());
        
        stream.add_chunk("function foo() {".to_string());
        assert!(stream.can_start_parsing());
        
        stream.add_chunk(" return 1; }".to_string());
        stream.complete();
        
        assert!(stream.is_complete());
        assert_eq!(stream.source(), "function foo() { return 1; }");
    }

    #[test]
    fn test_quick_checks() {
        assert!(PreParser::has_arguments_keyword("function() { return arguments; }"));
        assert!(!PreParser::has_arguments_keyword("function() { return 1; }"));
        
        assert!(PreParser::has_eval("eval('code')"));
        assert!(!PreParser::has_eval("evaluate()"));
        
        assert!(PreParser::detect_strict("\"use strict\"; var x;"));
        assert!(PreParser::detect_strict("'use strict'; var x;"));
        assert!(!PreParser::detect_strict("var x = 1;"));
    }
}
