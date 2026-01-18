//! Parallel HTML Parser
//!
//! Speculative parallel HTML parsing for improved performance on large documents.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

/// Chunking result for parallel parsing
#[derive(Debug)]
pub struct ParseChunk {
    /// Start byte offset
    pub start: usize,
    /// End byte offset  
    pub end: usize,
    /// The chunk data
    pub data: Vec<u8>,
    /// Is this the first chunk
    pub is_first: bool,
    /// Is this the last chunk
    pub is_last: bool,
}

/// Speculative parse result
#[derive(Debug)]
pub struct SpeculativeResult {
    /// Whether speculation was correct
    pub valid: bool,
    /// Parsed tokens from this chunk
    pub tokens: Vec<HtmlToken>,
    /// End state after parsing
    pub end_state: ParserState,
}

/// Token types for HTML parsing
#[derive(Debug, Clone, PartialEq)]
pub enum HtmlToken {
    /// DOCTYPE declaration
    Doctype(String),
    /// Start tag with name and attributes
    StartTag {
        name: String,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    },
    /// End tag
    EndTag { name: String },
    /// Text content
    Text(String),
    /// Comment
    Comment(String),
    /// Processing instruction
    ProcessingInstruction(String),
}

/// Parser state for speculation validation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ParserState {
    #[default]
    Data,
    TagOpen,
    EndTagOpen,
    TagName,
    BeforeAttributeName,
    AttributeName,
    AfterAttributeName,
    BeforeAttributeValue,
    AttributeValueDoubleQuoted,
    AttributeValueSingleQuoted,
    AttributeValueUnquoted,
    AfterAttributeValueQuoted,
    SelfClosingStartTag,
    BogusComment,
    MarkupDeclarationOpen,
    CommentStart,
    CommentStartDash,
    Comment,
    CommentEndDash,
    CommentEnd,
    Doctype,
    BeforeDoctypeName,
    DoctypeName,
    ScriptData,
    ScriptDataEscaped,
    RawText,
    Plaintext,
    CdataSection,
}

/// Preload scanner results
#[derive(Debug, Clone, Default)]
pub struct PreloadHints {
    /// Scripts to preload
    pub scripts: Vec<ResourceHint>,
    /// Stylesheets to preload
    pub stylesheets: Vec<ResourceHint>,
    /// Images to preload
    pub images: Vec<ResourceHint>,
    /// Fonts to preload
    pub fonts: Vec<ResourceHint>,
    /// Preconnect hints
    pub preconnects: Vec<String>,
}

/// Resource hint from preload scanning
#[derive(Debug, Clone)]
pub struct ResourceHint {
    /// URL of the resource
    pub url: String,
    /// Whether it's async (for scripts)
    pub is_async: bool,
    /// Whether it's defer (for scripts)
    pub is_defer: bool,
    /// Media query (for stylesheets)
    pub media: Option<String>,
    /// Crossorigin attribute
    pub crossorigin: Option<String>,
}

/// Parallel HTML parser with speculative parsing support
pub struct ParallelHtmlParser {
    /// Number of worker threads
    num_workers: usize,
    /// Minimum chunk size for parallel parsing (bytes)
    min_chunk_size: usize,
    /// Enable speculative parsing
    enable_speculation: bool,
    /// Enable preload scanning
    enable_preload: bool,
}

impl Default for ParallelHtmlParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ParallelHtmlParser {
    /// Create a new parallel parser
    pub fn new() -> Self {
        let num_workers = thread::available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        
        Self {
            num_workers,
            min_chunk_size: 100_000, // 100KB
            enable_speculation: true,
            enable_preload: true,
        }
    }
    
    /// Set number of worker threads
    pub fn with_workers(mut self, n: usize) -> Self {
        self.num_workers = n.max(1);
        self
    }
    
    /// Set minimum chunk size for parallelization
    pub fn with_min_chunk_size(mut self, size: usize) -> Self {
        self.min_chunk_size = size;
        self
    }
    
    /// Enable/disable speculative parsing
    pub fn with_speculation(mut self, enable: bool) -> Self {
        self.enable_speculation = enable;
        self
    }
    
    /// Enable/disable preload scanning
    pub fn with_preload(mut self, enable: bool) -> Self {
        self.enable_preload = enable;
        self
    }
    
    /// Parse HTML with parallel processing where beneficial
    pub fn parse(&self, html: &[u8]) -> ParseResult {
        // For small documents, use sequential parsing
        if html.len() < self.min_chunk_size || self.num_workers <= 1 {
            return self.parse_sequential(html);
        }
        
        let html = Arc::new(html.to_vec());
        let result = Arc::new(Mutex::new(ParseResult::default()));
        
        // Start preload scanner in parallel
        let preload_hints = if self.enable_preload {
            let html_clone = Arc::clone(&html);
            let handle = thread::spawn(move || {
                preload_scan(&html_clone)
            });
            Some(handle)
        } else {
            None
        };
        
        // Main parsing
        let tokens = self.parse_with_speculation(&html);
        
        // Wait for preload scanner
        if let Some(handle) = preload_hints {
            if let Ok(hints) = handle.join() {
                result.lock().unwrap().preload_hints = hints;
            }
        }
        
        let mut final_result = result.lock().unwrap().clone();
        final_result.tokens = tokens;
        final_result
    }
    
    /// Sequential parsing for small documents
    fn parse_sequential(&self, html: &[u8]) -> ParseResult {
        let mut result = ParseResult::default();
        
        // Simple tokenization
        let tokens = tokenize(html);
        result.tokens = tokens;
        
        // Preload scan if enabled
        if self.enable_preload {
            result.preload_hints = preload_scan(html);
        }
        
        result
    }
    
    /// Parse with speculation on large documents
    fn parse_with_speculation(&self, html: &[u8]) -> Vec<HtmlToken> {
        // Find safe split points (outside tags, comments, etc.)
        let chunks = self.find_chunk_boundaries(html);
        
        if chunks.len() <= 1 {
            return tokenize(html);
        }
        
        // Parse first chunk authoritatively
        let (first_tokens, first_end_state) = tokenize_with_state(
            &chunks[0].data,
            ParserState::Data,
        );
        
        // Speculative parsing for remaining chunks
        let speculation_valid = Arc::new(AtomicBool::new(true));
        let speculative_results: Vec<_> = chunks[1..]
            .iter()
            .map(|chunk| {
                let data = chunk.data.clone();
                let valid = Arc::clone(&speculation_valid);
                
                thread::spawn(move || {
                    // Speculatively assume Data state at chunk boundaries
                    let (tokens, end_state) = tokenize_with_state(&data, ParserState::Data);
                    SpeculativeResult {
                        valid: valid.load(Ordering::Relaxed),
                        tokens,
                        end_state,
                    }
                })
            })
            .collect();
        
        // Collect results
        let mut all_tokens = first_tokens;
        let mut current_state = first_end_state;
        
        for (i, handle) in speculative_results.into_iter().enumerate() {
            if let Ok(result) = handle.join() {
                // Validate speculation
                if current_state == ParserState::Data {
                    // Speculation was correct, use results
                    all_tokens.extend(result.tokens);
                    current_state = result.end_state;
                } else {
                    // Speculation failed, re-parse sequentially
                    speculation_valid.store(false, Ordering::Relaxed);
                    let (tokens, state) = tokenize_with_state(
                        &chunks[i + 1].data,
                        current_state,
                    );
                    all_tokens.extend(tokens);
                    current_state = state;
                }
            }
        }
        
        all_tokens
    }
    
    /// Find safe boundaries for chunking
    fn find_chunk_boundaries(&self, html: &[u8]) -> Vec<ParseChunk> {
        let num_chunks = self.num_workers.min(html.len() / self.min_chunk_size).max(1);
        let target_chunk_size = html.len() / num_chunks;
        
        let mut chunks = Vec::with_capacity(num_chunks);
        let mut start = 0;
        
        for i in 0..num_chunks {
            let target_end = if i == num_chunks - 1 {
                html.len()
            } else {
                let raw_end = start + target_chunk_size;
                // Find safe boundary (after > or whitespace outside tags)
                find_safe_boundary(html, raw_end)
            };
            
            chunks.push(ParseChunk {
                start,
                end: target_end,
                data: html[start..target_end].to_vec(),
                is_first: i == 0,
                is_last: i == num_chunks - 1,
            });
            
            start = target_end;
        }
        
        chunks
    }
}

/// Parse result
#[derive(Debug, Clone, Default)]
pub struct ParseResult {
    /// Parsed tokens
    pub tokens: Vec<HtmlToken>,
    /// Preload hints discovered
    pub preload_hints: PreloadHints,
    /// Parse errors
    pub errors: Vec<ParseError>,
    /// Stats
    pub stats: ParseStats,
}

/// Parse statistics
#[derive(Debug, Clone, Default)]
pub struct ParseStats {
    /// Total bytes parsed
    pub bytes_parsed: usize,
    /// Number of tokens
    pub token_count: usize,
    /// Whether speculation was used
    pub speculation_used: bool,
    /// Whether speculation succeeded
    pub speculation_success: bool,
}

/// Parse error
#[derive(Debug, Clone)]
pub struct ParseError {
    /// Error message
    pub message: String,
    /// Byte offset
    pub offset: usize,
}

// ============================================================================
// Tokenizer
// ============================================================================

/// Simple HTML tokenizer
fn tokenize(html: &[u8]) -> Vec<HtmlToken> {
    let (tokens, _) = tokenize_with_state(html, ParserState::Data);
    tokens
}

/// Tokenize with starting state, return tokens and end state
fn tokenize_with_state(html: &[u8], start_state: ParserState) -> (Vec<HtmlToken>, ParserState) {
    let mut tokens = Vec::new();
    let mut state = start_state;
    let mut pos = 0;
    let mut buffer = String::new();
    let mut tag_name = String::new();
    let mut attrs: Vec<(String, String)> = Vec::new();
    let mut attr_name = String::new();
    let mut attr_value = String::new();
    let mut self_closing = false;
    let mut is_end_tag = false;
    
    while pos < html.len() {
        let c = html[pos] as char;
        
        match state {
            ParserState::Data => {
                if c == '<' {
                    if !buffer.is_empty() {
                        tokens.push(HtmlToken::Text(std::mem::take(&mut buffer)));
                    }
                    state = ParserState::TagOpen;
                } else {
                    buffer.push(c);
                }
            }
            ParserState::TagOpen => {
                if c == '!' {
                    state = ParserState::MarkupDeclarationOpen;
                } else if c == '/' {
                    state = ParserState::EndTagOpen;
                    is_end_tag = true;
                } else if c == '?' {
                    state = ParserState::BogusComment;
                } else if c.is_ascii_alphabetic() {
                    tag_name.push(c.to_ascii_lowercase());
                    state = ParserState::TagName;
                    is_end_tag = false;
                } else {
                    buffer.push('<');
                    buffer.push(c);
                    state = ParserState::Data;
                }
            }
            ParserState::EndTagOpen => {
                if c.is_ascii_alphabetic() {
                    tag_name.push(c.to_ascii_lowercase());
                    state = ParserState::TagName;
                } else if c == '>' {
                    state = ParserState::Data;
                } else {
                    state = ParserState::BogusComment;
                }
            }
            ParserState::TagName => {
                if c.is_whitespace() {
                    state = ParserState::BeforeAttributeName;
                } else if c == '/' {
                    state = ParserState::SelfClosingStartTag;
                } else if c == '>' {
                    emit_tag(&mut tokens, &tag_name, &attrs, self_closing, is_end_tag);
                    tag_name.clear();
                    attrs.clear();
                    self_closing = false;
                    
                    // Check for raw text elements
                    if !is_end_tag {
                        let name_lower = tag_name.to_ascii_lowercase();
                        if name_lower == "script" || name_lower == "style" {
                            state = ParserState::RawText;
                        } else {
                            state = ParserState::Data;
                        }
                    } else {
                        state = ParserState::Data;
                    }
                } else {
                    tag_name.push(c.to_ascii_lowercase());
                }
            }
            ParserState::BeforeAttributeName => {
                if c.is_whitespace() {
                    // Stay in state
                } else if c == '/' || c == '>' {
                    pos -= 1; // Reconsume
                    state = ParserState::AfterAttributeName;
                } else if c == '=' {
                    attr_name.push(c);
                    state = ParserState::AttributeName;
                } else {
                    attr_name.push(c.to_ascii_lowercase());
                    state = ParserState::AttributeName;
                }
            }
            ParserState::AttributeName => {
                if c.is_whitespace() {
                    state = ParserState::AfterAttributeName;
                } else if c == '/' || c == '>' {
                    pos -= 1;
                    state = ParserState::AfterAttributeName;
                } else if c == '=' {
                    state = ParserState::BeforeAttributeValue;
                } else {
                    attr_name.push(c.to_ascii_lowercase());
                }
            }
            ParserState::AfterAttributeName => {
                if c.is_whitespace() {
                    // Stay
                } else if c == '/' {
                    state = ParserState::SelfClosingStartTag;
                    if !attr_name.is_empty() {
                        attrs.push((std::mem::take(&mut attr_name), String::new()));
                    }
                } else if c == '=' {
                    state = ParserState::BeforeAttributeValue;
                } else if c == '>' {
                    if !attr_name.is_empty() {
                        attrs.push((std::mem::take(&mut attr_name), String::new()));
                    }
                    emit_tag(&mut tokens, &tag_name, &attrs, self_closing, is_end_tag);
                    tag_name.clear();
                    attrs.clear();
                    self_closing = false;
                    state = ParserState::Data;
                } else {
                    if !attr_name.is_empty() {
                        attrs.push((std::mem::take(&mut attr_name), String::new()));
                    }
                    attr_name.push(c.to_ascii_lowercase());
                    state = ParserState::AttributeName;
                }
            }
            ParserState::BeforeAttributeValue => {
                if c.is_whitespace() {
                    // Stay
                } else if c == '"' {
                    state = ParserState::AttributeValueDoubleQuoted;
                } else if c == '\'' {
                    state = ParserState::AttributeValueSingleQuoted;
                } else if c == '>' {
                    attrs.push((std::mem::take(&mut attr_name), String::new()));
                    emit_tag(&mut tokens, &tag_name, &attrs, self_closing, is_end_tag);
                    tag_name.clear();
                    attrs.clear();
                    self_closing = false;
                    state = ParserState::Data;
                } else {
                    attr_value.push(c);
                    state = ParserState::AttributeValueUnquoted;
                }
            }
            ParserState::AttributeValueDoubleQuoted => {
                if c == '"' {
                    attrs.push((std::mem::take(&mut attr_name), std::mem::take(&mut attr_value)));
                    state = ParserState::AfterAttributeValueQuoted;
                } else {
                    attr_value.push(c);
                }
            }
            ParserState::AttributeValueSingleQuoted => {
                if c == '\'' {
                    attrs.push((std::mem::take(&mut attr_name), std::mem::take(&mut attr_value)));
                    state = ParserState::AfterAttributeValueQuoted;
                } else {
                    attr_value.push(c);
                }
            }
            ParserState::AttributeValueUnquoted => {
                if c.is_whitespace() {
                    attrs.push((std::mem::take(&mut attr_name), std::mem::take(&mut attr_value)));
                    state = ParserState::BeforeAttributeName;
                } else if c == '>' {
                    attrs.push((std::mem::take(&mut attr_name), std::mem::take(&mut attr_value)));
                    emit_tag(&mut tokens, &tag_name, &attrs, self_closing, is_end_tag);
                    tag_name.clear();
                    attrs.clear();
                    self_closing = false;
                    state = ParserState::Data;
                } else {
                    attr_value.push(c);
                }
            }
            ParserState::AfterAttributeValueQuoted => {
                if c.is_whitespace() {
                    state = ParserState::BeforeAttributeName;
                } else if c == '/' {
                    state = ParserState::SelfClosingStartTag;
                } else if c == '>' {
                    emit_tag(&mut tokens, &tag_name, &attrs, self_closing, is_end_tag);
                    tag_name.clear();
                    attrs.clear();
                    self_closing = false;
                    state = ParserState::Data;
                } else {
                    pos -= 1;
                    state = ParserState::BeforeAttributeName;
                }
            }
            ParserState::SelfClosingStartTag => {
                if c == '>' {
                    self_closing = true;
                    emit_tag(&mut tokens, &tag_name, &attrs, self_closing, is_end_tag);
                    tag_name.clear();
                    attrs.clear();
                    self_closing = false;
                    state = ParserState::Data;
                } else {
                    pos -= 1;
                    state = ParserState::BeforeAttributeName;
                }
            }
            ParserState::MarkupDeclarationOpen => {
                // Check for comment or DOCTYPE
                if pos + 1 < html.len() && html[pos] == b'-' && html[pos + 1] == b'-' {
                    pos += 1;
                    state = ParserState::CommentStart;
                } else if pos + 6 < html.len() {
                    let next = &html[pos..pos + 7];
                    if next.eq_ignore_ascii_case(b"DOCTYPE") {
                        pos += 6;
                        state = ParserState::Doctype;
                    } else {
                        state = ParserState::BogusComment;
                    }
                } else {
                    state = ParserState::BogusComment;
                }
            }
            ParserState::CommentStart => {
                if c == '-' {
                    state = ParserState::CommentStartDash;
                } else if c == '>' {
                    tokens.push(HtmlToken::Comment(String::new()));
                    state = ParserState::Data;
                } else {
                    buffer.push(c);
                    state = ParserState::Comment;
                }
            }
            ParserState::CommentStartDash => {
                if c == '-' {
                    state = ParserState::CommentEnd;
                } else if c == '>' {
                    tokens.push(HtmlToken::Comment(String::new()));
                    state = ParserState::Data;
                } else {
                    buffer.push('-');
                    buffer.push(c);
                    state = ParserState::Comment;
                }
            }
            ParserState::Comment => {
                if c == '-' {
                    state = ParserState::CommentEndDash;
                } else {
                    buffer.push(c);
                }
            }
            ParserState::CommentEndDash => {
                if c == '-' {
                    state = ParserState::CommentEnd;
                } else {
                    buffer.push('-');
                    buffer.push(c);
                    state = ParserState::Comment;
                }
            }
            ParserState::CommentEnd => {
                if c == '>' {
                    tokens.push(HtmlToken::Comment(std::mem::take(&mut buffer)));
                    state = ParserState::Data;
                } else if c == '-' {
                    buffer.push('-');
                } else {
                    buffer.push('-');
                    buffer.push('-');
                    buffer.push(c);
                    state = ParserState::Comment;
                }
            }
            ParserState::Doctype => {
                if c.is_whitespace() {
                    state = ParserState::BeforeDoctypeName;
                } else {
                    pos -= 1;
                    state = ParserState::BeforeDoctypeName;
                }
            }
            ParserState::BeforeDoctypeName => {
                if c.is_whitespace() {
                    // Stay
                } else if c == '>' {
                    tokens.push(HtmlToken::Doctype(String::new()));
                    state = ParserState::Data;
                } else {
                    buffer.push(c.to_ascii_lowercase());
                    state = ParserState::DoctypeName;
                }
            }
            ParserState::DoctypeName => {
                if c.is_whitespace() || c == '>' {
                    tokens.push(HtmlToken::Doctype(std::mem::take(&mut buffer)));
                    if c == '>' {
                        state = ParserState::Data;
                    } else {
                        // Skip to end of DOCTYPE
                        while pos < html.len() && html[pos] != b'>' {
                            pos += 1;
                        }
                        state = ParserState::Data;
                    }
                } else {
                    buffer.push(c.to_ascii_lowercase());
                }
            }
            ParserState::BogusComment => {
                if c == '>' {
                    tokens.push(HtmlToken::Comment(std::mem::take(&mut buffer)));
                    state = ParserState::Data;
                } else {
                    buffer.push(c);
                }
            }
            ParserState::RawText => {
                // Look for end tag
                buffer.push(c);
                // Simple check for </script> or </style>
                let check_len = tag_name.len() + 3; // </name>
                if buffer.len() >= check_len {
                    let end_check = format!("</{}>", tag_name);
                    if buffer.ends_with(&end_check) {
                        // Remove end tag from buffer and emit
                        let content = buffer[..buffer.len() - check_len].to_string();
                        tokens.push(HtmlToken::Text(content));
                        tokens.push(HtmlToken::EndTag { name: tag_name.clone() });
                        buffer.clear();
                        tag_name.clear();
                        state = ParserState::Data;
                    }
                }
            }
            _ => {
                // Handle other states as data
                buffer.push(c);
                state = ParserState::Data;
            }
        }
        
        pos += 1;
    }
    
    // Emit any remaining text
    if !buffer.is_empty() {
        tokens.push(HtmlToken::Text(buffer));
    }
    
    (tokens, state)
}

fn emit_tag(
    tokens: &mut Vec<HtmlToken>,
    name: &str,
    attrs: &[(String, String)],
    self_closing: bool,
    is_end_tag: bool,
) {
    if is_end_tag {
        tokens.push(HtmlToken::EndTag { name: name.to_string() });
    } else {
        tokens.push(HtmlToken::StartTag {
            name: name.to_string(),
            attributes: attrs.to_vec(),
            self_closing,
        });
    }
}

// ============================================================================
// Preload Scanner
// ============================================================================

/// Quick preload scan of HTML for resource hints
fn preload_scan(html: &[u8]) -> PreloadHints {
    let mut hints = PreloadHints::default();
    let mut pos = 0;
    
    while pos < html.len() {
        // Fast scan for '<'
        if html[pos] != b'<' {
            pos += 1;
            continue;
        }
        
        // Check for interesting tags
        let remaining = &html[pos..];
        
        if starts_with_ignore_case(remaining, b"<script") {
            if let Some(hint) = extract_script_hint(remaining) {
                hints.scripts.push(hint);
            }
        } else if starts_with_ignore_case(remaining, b"<link") {
            if let Some(hint) = extract_link_hint(remaining) {
                if hint.url.ends_with(".css") || hint.media.is_some() {
                    hints.stylesheets.push(hint);
                } else if hint.url.contains("font") {
                    hints.fonts.push(hint);
                }
            }
        } else if starts_with_ignore_case(remaining, b"<img") {
            if let Some(hint) = extract_img_hint(remaining) {
                hints.images.push(hint);
            }
        }
        
        pos += 1;
    }
    
    hints
}

fn starts_with_ignore_case(haystack: &[u8], needle: &[u8]) -> bool {
    if haystack.len() < needle.len() {
        return false;
    }
    haystack[..needle.len()].eq_ignore_ascii_case(needle)
}

fn extract_script_hint(html: &[u8]) -> Option<ResourceHint> {
    let tag_end = html.iter().position(|&b| b == b'>')?;
    let tag = std::str::from_utf8(&html[..tag_end]).ok()?;
    
    let src = extract_attr(tag, "src")?;
    
    Some(ResourceHint {
        url: src,
        is_async: tag.contains("async"),
        is_defer: tag.contains("defer"),
        media: None,
        crossorigin: extract_attr(tag, "crossorigin"),
    })
}

fn extract_link_hint(html: &[u8]) -> Option<ResourceHint> {
    let tag_end = html.iter().position(|&b| b == b'>')?;
    let tag = std::str::from_utf8(&html[..tag_end]).ok()?;
    
    let href = extract_attr(tag, "href")?;
    
    Some(ResourceHint {
        url: href,
        is_async: false,
        is_defer: false,
        media: extract_attr(tag, "media"),
        crossorigin: extract_attr(tag, "crossorigin"),
    })
}

fn extract_img_hint(html: &[u8]) -> Option<ResourceHint> {
    let tag_end = html.iter().position(|&b| b == b'>')?;
    let tag = std::str::from_utf8(&html[..tag_end]).ok()?;
    
    let src = extract_attr(tag, "src")?;
    
    Some(ResourceHint {
        url: src,
        is_async: false,
        is_defer: false,
        media: None,
        crossorigin: extract_attr(tag, "crossorigin"),
    })
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let search = format!("{}=\"", name);
    let search2 = format!("{}='", name);
    
    if let Some(start) = tag.find(&search) {
        let value_start = start + search.len();
        let value_end = tag[value_start..].find('"')? + value_start;
        return Some(tag[value_start..value_end].to_string());
    }
    
    if let Some(start) = tag.find(&search2) {
        let value_start = start + search2.len();
        let value_end = tag[value_start..].find('\'')? + value_start;
        return Some(tag[value_start..value_end].to_string());
    }
    
    None
}

/// Find a safe boundary for chunking (not in the middle of a tag)
fn find_safe_boundary(html: &[u8], target: usize) -> usize {
    let search_range = 1000.min(html.len() - target);
    
    // Look forward for a safe boundary
    for i in 0..search_range {
        let pos = target + i;
        if pos >= html.len() {
            return html.len();
        }
        
        // Safe after '>' followed by whitespace or '<'
        if html[pos] == b'>' {
            if pos + 1 < html.len() {
                let next = html[pos + 1];
                if next == b'<' || next.is_ascii_whitespace() {
                    return pos + 1;
                }
            }
        }
    }
    
    target
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_tokenize() {
        let html = b"<html><head><title>Test</title></head><body>Hello</body></html>";
        let tokens = tokenize(html);
        
        assert!(tokens.iter().any(|t| matches!(t, HtmlToken::StartTag { name, .. } if name == "html")));
        assert!(tokens.iter().any(|t| matches!(t, HtmlToken::Text(s) if s == "Test")));
        assert!(tokens.iter().any(|t| matches!(t, HtmlToken::Text(s) if s == "Hello")));
    }
    
    #[test]
    fn test_doctype() {
        let html = b"<!DOCTYPE html><html></html>";
        let tokens = tokenize(html);
        
        assert!(matches!(&tokens[0], HtmlToken::Doctype(s) if s == "html"));
    }
    
    #[test]
    fn test_comment() {
        let html = b"<!-- this is a comment --><div></div>";
        let tokens = tokenize(html);
        
        assert!(matches!(&tokens[0], HtmlToken::Comment(s) if s.contains("comment")));
    }
    
    #[test]
    fn test_attributes() {
        let html = b"<div id=\"main\" class='container'></div>";
        let tokens = tokenize(html);
        
        if let HtmlToken::StartTag { attributes, .. } = &tokens[0] {
            assert!(attributes.iter().any(|(k, v)| k == "id" && v == "main"));
            assert!(attributes.iter().any(|(k, v)| k == "class" && v == "container"));
        } else {
            panic!("Expected start tag");
        }
    }
    
    #[test]
    fn test_self_closing() {
        let html = b"<br/><img src=\"test.png\"/>";
        let tokens = tokenize(html);
        
        assert!(tokens.iter().any(|t| matches!(t, HtmlToken::StartTag { name, self_closing, .. } if name == "br" && *self_closing)));
    }
    
    #[test]
    fn test_preload_scan() {
        let html = br#"
            <head>
                <script src="app.js" defer></script>
                <link rel="stylesheet" href="style.css">
                <img src="hero.png">
            </head>
        "#;
        
        let hints = preload_scan(html);
        
        assert!(!hints.scripts.is_empty());
        assert_eq!(hints.scripts[0].url, "app.js");
        assert!(hints.scripts[0].is_defer);
        
        assert!(!hints.stylesheets.is_empty());
        assert!(!hints.images.is_empty());
    }
    
    #[test]
    fn test_parallel_parser() {
        let parser = ParallelHtmlParser::new()
            .with_workers(2)
            .with_min_chunk_size(10);
        
        let html = b"<html><body><p>Hello World</p></body></html>";
        let result = parser.parse(html);
        
        assert!(!result.tokens.is_empty());
    }
}
