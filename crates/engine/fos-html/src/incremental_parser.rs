//! Incremental HTML Parser
//!
//! Parse HTML incrementally without blocking on full document arrival.
//! Enables streaming render of above-the-fold content while rest loads.

use std::collections::VecDeque;

/// Incremental parser state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParsePhase {
    /// Not started
    Initial,
    /// Parsing DOCTYPE
    Doctype,
    /// Parsing <head> section
    Head,
    /// Parsing <body>, can start rendering
    Body,
    /// Document complete
    Complete,
    /// Parse error occurred
    Error,
}

/// Token from incremental tokenization
#[derive(Debug, Clone)]
pub enum Token {
    /// DOCTYPE declaration
    Doctype {
        name: Option<String>,
        public_id: Option<String>,
        system_id: Option<String>,
    },
    /// Start tag
    StartTag {
        name: String,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    },
    /// End tag
    EndTag {
        name: String,
    },
    /// Character data
    Character(char),
    /// Comment
    Comment(String),
    /// End of file
    Eof,
}

/// Tokenizer state machine state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenizerState {
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
    CommentLessThanSign,
    CommentEndDash,
    CommentEnd,
    Doctype,
    BeforeDoctypeName,
    DoctypeName,
    AfterDoctypeName,
}

impl Default for TokenizerState {
    fn default() -> Self {
        Self::Data
    }
}

/// Incremental tokenizer
#[derive(Debug)]
pub struct IncrementalTokenizer {
    /// Current state
    state: TokenizerState,
    /// Input buffer (remaining unparsed bytes)
    buffer: Vec<u8>,
    /// Current position in buffer
    pos: usize,
    /// Temporary buffer for building tokens
    temp: String,
    /// Current tag name
    tag_name: String,
    /// Current attribute name
    attr_name: String,
    /// Current attribute value  
    attr_value: String,
    /// Collected attributes
    attributes: Vec<(String, String)>,
    /// Is self-closing tag
    self_closing: bool,
    /// Is end tag
    is_end_tag: bool,
    /// Output token queue
    tokens: VecDeque<Token>,
    /// Bytes consumed
    bytes_consumed: usize,
}

impl Default for IncrementalTokenizer {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalTokenizer {
    /// Create new tokenizer
    pub fn new() -> Self {
        Self {
            state: TokenizerState::Data,
            buffer: Vec::new(),
            pos: 0,
            temp: String::new(),
            tag_name: String::new(),
            attr_name: String::new(),
            attr_value: String::new(),
            attributes: Vec::new(),
            self_closing: false,
            is_end_tag: false,
            tokens: VecDeque::new(),
            bytes_consumed: 0,
        }
    }

    /// Feed more data to the tokenizer
    pub fn feed(&mut self, data: &[u8]) {
        // Append new data, keeping unprocessed bytes
        if self.pos < self.buffer.len() {
            let remaining: Vec<u8> = self.buffer[self.pos..].to_vec();
            self.buffer = remaining;
            self.buffer.extend_from_slice(data);
        } else {
            self.buffer = data.to_vec();
        }
        self.pos = 0;
    }

    /// Process available input and emit tokens
    pub fn process(&mut self) -> usize {
        let start_pos = self.pos;
        
        while self.pos < self.buffer.len() {
            let c = self.buffer[self.pos] as char;
            
            match self.state {
                TokenizerState::Data => {
                    if c == '<' {
                        self.state = TokenizerState::TagOpen;
                    } else {
                        self.tokens.push_back(Token::Character(c));
                    }
                    self.pos += 1;
                }
                
                TokenizerState::TagOpen => {
                    if c == '/' {
                        self.state = TokenizerState::EndTagOpen;
                        self.pos += 1;
                    } else if c == '!' {
                        self.state = TokenizerState::MarkupDeclarationOpen;
                        self.pos += 1;
                    } else if c.is_ascii_alphabetic() {
                        self.is_end_tag = false;
                        self.tag_name.clear();
                        self.tag_name.push(c.to_ascii_lowercase());
                        self.state = TokenizerState::TagName;
                        self.pos += 1;
                    } else {
                        self.tokens.push_back(Token::Character('<'));
                        self.state = TokenizerState::Data;
                    }
                }
                
                TokenizerState::EndTagOpen => {
                    if c.is_ascii_alphabetic() {
                        self.is_end_tag = true;
                        self.tag_name.clear();
                        self.tag_name.push(c.to_ascii_lowercase());
                        self.state = TokenizerState::TagName;
                        self.pos += 1;
                    } else {
                        self.state = TokenizerState::BogusComment;
                    }
                }
                
                TokenizerState::TagName => {
                    if c.is_whitespace() {
                        self.state = TokenizerState::BeforeAttributeName;
                        self.pos += 1;
                    } else if c == '/' {
                        self.state = TokenizerState::SelfClosingStartTag;
                        self.pos += 1;
                    } else if c == '>' {
                        self.emit_tag();
                        self.state = TokenizerState::Data;
                        self.pos += 1;
                    } else {
                        self.tag_name.push(c.to_ascii_lowercase());
                        self.pos += 1;
                    }
                }
                
                TokenizerState::BeforeAttributeName => {
                    if c.is_whitespace() {
                        self.pos += 1;
                    } else if c == '/' {
                        self.state = TokenizerState::SelfClosingStartTag;
                        self.pos += 1;
                    } else if c == '>' {
                        self.emit_tag();
                        self.state = TokenizerState::Data;
                        self.pos += 1;
                    } else {
                        self.attr_name.clear();
                        self.attr_value.clear();
                        self.attr_name.push(c.to_ascii_lowercase());
                        self.state = TokenizerState::AttributeName;
                        self.pos += 1;
                    }
                }
                
                TokenizerState::AttributeName => {
                    if c.is_whitespace() {
                        self.state = TokenizerState::AfterAttributeName;
                        self.pos += 1;
                    } else if c == '/' {
                        self.push_attribute();
                        self.state = TokenizerState::SelfClosingStartTag;
                        self.pos += 1;
                    } else if c == '=' {
                        self.state = TokenizerState::BeforeAttributeValue;
                        self.pos += 1;
                    } else if c == '>' {
                        self.push_attribute();
                        self.emit_tag();
                        self.state = TokenizerState::Data;
                        self.pos += 1;
                    } else {
                        self.attr_name.push(c.to_ascii_lowercase());
                        self.pos += 1;
                    }
                }
                
                TokenizerState::AfterAttributeName => {
                    if c.is_whitespace() {
                        self.pos += 1;
                    } else if c == '/' {
                        self.push_attribute();
                        self.state = TokenizerState::SelfClosingStartTag;
                        self.pos += 1;
                    } else if c == '=' {
                        self.state = TokenizerState::BeforeAttributeValue;
                        self.pos += 1;
                    } else if c == '>' {
                        self.push_attribute();
                        self.emit_tag();
                        self.state = TokenizerState::Data;
                        self.pos += 1;
                    } else {
                        self.push_attribute();
                        self.attr_name.clear();
                        self.attr_name.push(c.to_ascii_lowercase());
                        self.state = TokenizerState::AttributeName;
                        self.pos += 1;
                    }
                }
                
                TokenizerState::BeforeAttributeValue => {
                    if c.is_whitespace() {
                        self.pos += 1;
                    } else if c == '"' {
                        self.state = TokenizerState::AttributeValueDoubleQuoted;
                        self.pos += 1;
                    } else if c == '\'' {
                        self.state = TokenizerState::AttributeValueSingleQuoted;
                        self.pos += 1;
                    } else if c == '>' {
                        self.push_attribute();
                        self.emit_tag();
                        self.state = TokenizerState::Data;
                        self.pos += 1;
                    } else {
                        self.attr_value.push(c);
                        self.state = TokenizerState::AttributeValueUnquoted;
                        self.pos += 1;
                    }
                }
                
                TokenizerState::AttributeValueDoubleQuoted => {
                    if c == '"' {
                        self.push_attribute();
                        self.state = TokenizerState::AfterAttributeValueQuoted;
                        self.pos += 1;
                    } else {
                        self.attr_value.push(c);
                        self.pos += 1;
                    }
                }
                
                TokenizerState::AttributeValueSingleQuoted => {
                    if c == '\'' {
                        self.push_attribute();
                        self.state = TokenizerState::AfterAttributeValueQuoted;
                        self.pos += 1;
                    } else {
                        self.attr_value.push(c);
                        self.pos += 1;
                    }
                }
                
                TokenizerState::AttributeValueUnquoted => {
                    if c.is_whitespace() {
                        self.push_attribute();
                        self.state = TokenizerState::BeforeAttributeName;
                        self.pos += 1;
                    } else if c == '>' {
                        self.push_attribute();
                        self.emit_tag();
                        self.state = TokenizerState::Data;
                        self.pos += 1;
                    } else {
                        self.attr_value.push(c);
                        self.pos += 1;
                    }
                }
                
                TokenizerState::AfterAttributeValueQuoted => {
                    if c.is_whitespace() {
                        self.state = TokenizerState::BeforeAttributeName;
                        self.pos += 1;
                    } else if c == '/' {
                        self.state = TokenizerState::SelfClosingStartTag;
                        self.pos += 1;
                    } else if c == '>' {
                        self.emit_tag();
                        self.state = TokenizerState::Data;
                        self.pos += 1;
                    } else {
                        self.state = TokenizerState::BeforeAttributeName;
                    }
                }
                
                TokenizerState::SelfClosingStartTag => {
                    if c == '>' {
                        self.self_closing = true;
                        self.emit_tag();
                        self.state = TokenizerState::Data;
                        self.pos += 1;
                    } else {
                        self.state = TokenizerState::BeforeAttributeName;
                    }
                }
                
                TokenizerState::MarkupDeclarationOpen => {
                    // Check for DOCTYPE or comment
                    if self.buffer.len() >= self.pos + 7 {
                        let next7 = std::str::from_utf8(&self.buffer[self.pos..self.pos + 7])
                            .unwrap_or("");
                        if next7.eq_ignore_ascii_case("DOCTYPE") {
                            self.pos += 7;
                            self.state = TokenizerState::Doctype;
                            continue;
                        }
                    }
                    if self.buffer.len() >= self.pos + 2 {
                        if self.buffer[self.pos] == b'-' && self.buffer[self.pos + 1] == b'-' {
                            self.pos += 2;
                            self.temp.clear();
                            self.state = TokenizerState::CommentStart;
                            continue;
                        }
                    }
                    // Need more data
                    if self.buffer.len() < self.pos + 7 {
                        break;
                    }
                    self.state = TokenizerState::BogusComment;
                }
                
                TokenizerState::Doctype => {
                    if c.is_whitespace() {
                        self.state = TokenizerState::BeforeDoctypeName;
                    }
                    self.pos += 1;
                }
                
                TokenizerState::BeforeDoctypeName => {
                    if c.is_whitespace() {
                        self.pos += 1;
                    } else {
                        self.temp.clear();
                        self.temp.push(c.to_ascii_lowercase());
                        self.state = TokenizerState::DoctypeName;
                        self.pos += 1;
                    }
                }
                
                TokenizerState::DoctypeName => {
                    if c.is_whitespace() {
                        self.state = TokenizerState::AfterDoctypeName;
                        self.pos += 1;
                    } else if c == '>' {
                        self.tokens.push_back(Token::Doctype {
                            name: Some(self.temp.clone()),
                            public_id: None,
                            system_id: None,
                        });
                        self.state = TokenizerState::Data;
                        self.pos += 1;
                    } else {
                        self.temp.push(c.to_ascii_lowercase());
                        self.pos += 1;
                    }
                }
                
                TokenizerState::AfterDoctypeName => {
                    if c == '>' {
                        self.tokens.push_back(Token::Doctype {
                            name: Some(self.temp.clone()),
                            public_id: None,
                            system_id: None,
                        });
                        self.state = TokenizerState::Data;
                    }
                    self.pos += 1;
                }
                
                TokenizerState::CommentStart => {
                    if c == '-' {
                        self.state = TokenizerState::CommentStartDash;
                    } else if c == '>' {
                        self.tokens.push_back(Token::Comment(String::new()));
                        self.state = TokenizerState::Data;
                    } else {
                        self.temp.push(c);
                        self.state = TokenizerState::Comment;
                    }
                    self.pos += 1;
                }
                
                TokenizerState::CommentStartDash => {
                    if c == '-' {
                        self.state = TokenizerState::CommentEnd;
                    } else if c == '>' {
                        self.tokens.push_back(Token::Comment(String::new()));
                        self.state = TokenizerState::Data;
                    } else {
                        self.temp.push('-');
                        self.temp.push(c);
                        self.state = TokenizerState::Comment;
                    }
                    self.pos += 1;
                }
                
                TokenizerState::Comment => {
                    if c == '-' {
                        self.state = TokenizerState::CommentEndDash;
                    } else {
                        self.temp.push(c);
                    }
                    self.pos += 1;
                }
                
                TokenizerState::CommentLessThanSign => {
                    self.temp.push(c);
                    self.state = TokenizerState::Comment;
                    self.pos += 1;
                }
                
                TokenizerState::CommentEndDash => {
                    if c == '-' {
                        self.state = TokenizerState::CommentEnd;
                    } else {
                        self.temp.push('-');
                        self.temp.push(c);
                        self.state = TokenizerState::Comment;
                    }
                    self.pos += 1;
                }
                
                TokenizerState::CommentEnd => {
                    if c == '>' {
                        self.tokens.push_back(Token::Comment(self.temp.clone()));
                        self.temp.clear();
                        self.state = TokenizerState::Data;
                    } else if c == '-' {
                        self.temp.push('-');
                    } else {
                        self.temp.push('-');
                        self.temp.push('-');
                        self.temp.push(c);
                        self.state = TokenizerState::Comment;
                    }
                    self.pos += 1;
                }
                
                TokenizerState::BogusComment => {
                    if c == '>' {
                        self.tokens.push_back(Token::Comment(self.temp.clone()));
                        self.temp.clear();
                        self.state = TokenizerState::Data;
                    } else {
                        self.temp.push(c);
                    }
                    self.pos += 1;
                }
            }
        }
        
        let consumed = self.pos - start_pos;
        self.bytes_consumed += consumed;
        consumed
    }
    
    /// Get next token
    pub fn next_token(&mut self) -> Option<Token> {
        self.tokens.pop_front()
    }
    
    /// Check if tokens available
    pub fn has_tokens(&self) -> bool {
        !self.tokens.is_empty()
    }
    
    /// Emit EOF token
    pub fn finish(&mut self) {
        self.tokens.push_back(Token::Eof);
    }
    
    /// Total bytes consumed
    pub fn bytes_consumed(&self) -> usize {
        self.bytes_consumed
    }
    
    fn push_attribute(&mut self) {
        if !self.attr_name.is_empty() {
            self.attributes.push((
                std::mem::take(&mut self.attr_name),
                std::mem::take(&mut self.attr_value),
            ));
        }
    }
    
    fn emit_tag(&mut self) {
        let token = if self.is_end_tag {
            Token::EndTag {
                name: std::mem::take(&mut self.tag_name),
            }
        } else {
            Token::StartTag {
                name: std::mem::take(&mut self.tag_name),
                attributes: std::mem::take(&mut self.attributes),
                self_closing: self.self_closing,
            }
        };
        self.tokens.push_back(token);
        self.self_closing = false;
        self.is_end_tag = false;
    }
}

/// Chunk boundary for incremental parsing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkBoundary {
    /// Safe to chunk here (between elements)
    Safe,
    /// Unsafe (in middle of tag or attribute)
    Unsafe,
    /// At natural break (after closing tag)
    Natural,
}

/// Incremental HTML parser
#[derive(Debug)]
pub struct IncrementalParser {
    /// Tokenizer
    tokenizer: IncrementalTokenizer,
    /// Current phase
    phase: ParsePhase,
    /// Open elements stack (tag names)
    open_elements: Vec<String>,
    /// Node ID counter
    next_id: u32,
    /// Parsed nodes ready for DOM construction
    parsed_nodes: VecDeque<ParsedNode>,
    /// Statistics
    stats: IncrementalParseStats,
}

/// Parsed node ready for DOM construction
#[derive(Debug, Clone)]
pub struct ParsedNode {
    /// Node ID
    pub id: u32,
    /// Parent ID
    pub parent_id: Option<u32>,
    /// Node content
    pub content: ParsedNodeContent,
}

/// Content of parsed node
#[derive(Debug, Clone)]
pub enum ParsedNodeContent {
    /// Document node
    Document,
    /// Element
    Element {
        tag: String,
        attributes: Vec<(String, String)>,
    },
    /// Text content
    Text(String),
    /// Comment
    Comment(String),
}

/// Incremental parse statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct IncrementalParseStats {
    /// Bytes fed
    pub bytes_fed: usize,
    /// Bytes processed
    pub bytes_processed: usize,
    /// Tokens emitted
    pub tokens_emitted: usize,
    /// Nodes created
    pub nodes_created: usize,
    /// Chunks yielded  
    pub chunks_yielded: usize,
}

impl Default for IncrementalParser {
    fn default() -> Self {
        Self::new()
    }
}

impl IncrementalParser {
    /// Create new incremental parser
    pub fn new() -> Self {
        Self {
            tokenizer: IncrementalTokenizer::new(),
            phase: ParsePhase::Initial,
            open_elements: Vec::new(),
            next_id: 0,
            parsed_nodes: VecDeque::new(),
            stats: IncrementalParseStats::default(),
        }
    }
    
    /// Feed data to parser
    pub fn feed(&mut self, data: &[u8]) {
        self.stats.bytes_fed += data.len();
        self.tokenizer.feed(data);
    }
    
    /// Process available data, yielding after `max_tokens` tokens
    pub fn process(&mut self, max_tokens: usize) -> ParseYield {
        let processed = self.tokenizer.process();
        self.stats.bytes_processed += processed;
        
        let mut tokens_processed = 0;
        
        while let Some(token) = self.tokenizer.next_token() {
            self.stats.tokens_emitted += 1;
            tokens_processed += 1;
            
            self.handle_token(token);
            
            if tokens_processed >= max_tokens {
                return ParseYield::Yielded {
                    tokens: tokens_processed,
                    can_render: self.can_render(),
                };
            }
        }
        
        if self.phase == ParsePhase::Complete {
            ParseYield::Complete
        } else {
            ParseYield::NeedMoreData
        }
    }
    
    /// Process all available data
    pub fn process_all(&mut self) -> ParseYield {
        self.process(usize::MAX)
    }
    
    /// Signal end of input
    pub fn finish(&mut self) {
        self.tokenizer.finish();
        self.process_all();
        self.phase = ParsePhase::Complete;
    }
    
    /// Can we start rendering?
    pub fn can_render(&self) -> bool {
        matches!(self.phase, ParsePhase::Body | ParsePhase::Complete)
    }
    
    /// Get current phase
    pub fn phase(&self) -> ParsePhase {
        self.phase
    }
    
    /// Get next parsed node
    pub fn next_node(&mut self) -> Option<ParsedNode> {
        self.parsed_nodes.pop_front()
    }
    
    /// Has nodes ready
    pub fn has_nodes(&self) -> bool {
        !self.parsed_nodes.is_empty()
    }
    
    /// Get statistics
    pub fn stats(&self) -> &IncrementalParseStats {
        &self.stats
    }
    
    /// Check chunk boundary
    pub fn chunk_boundary(&self) -> ChunkBoundary {
        if self.tokenizer.state == TokenizerState::Data && self.open_elements.is_empty() {
            ChunkBoundary::Natural
        } else if self.tokenizer.state == TokenizerState::Data {
            ChunkBoundary::Safe
        } else {
            ChunkBoundary::Unsafe
        }
    }
    
    fn handle_token(&mut self, token: Token) {
        match token {
            Token::Doctype { .. } => {
                self.phase = ParsePhase::Doctype;
            }
            
            Token::StartTag { name, attributes, self_closing } => {
                // Update phase
                match name.as_str() {
                    "head" => self.phase = ParsePhase::Head,
                    "body" => self.phase = ParsePhase::Body,
                    _ => {}
                }
                
                let id = self.alloc_id();
                let parent_id = self.current_parent();
                
                self.parsed_nodes.push_back(ParsedNode {
                    id,
                    parent_id,
                    content: ParsedNodeContent::Element {
                        tag: name.clone(),
                        attributes,
                    },
                });
                self.stats.nodes_created += 1;
                
                // Push to stack if not void element
                if !self_closing && !is_void_element(&name) {
                    self.open_elements.push(name);
                }
            }
            
            Token::EndTag { name } => {
                // Pop matching element
                if let Some(pos) = self.open_elements.iter().rposition(|e| e == &name) {
                    self.open_elements.truncate(pos);
                }
            }
            
            Token::Character(c) => {
                let parent_id = self.current_parent();
                let id = self.alloc_id();
                
                self.parsed_nodes.push_back(ParsedNode {
                    id,
                    parent_id,
                    content: ParsedNodeContent::Text(c.to_string()),
                });
                self.stats.nodes_created += 1;
            }
            
            Token::Comment(text) => {
                let parent_id = self.current_parent();
                let id = self.alloc_id();
                
                self.parsed_nodes.push_back(ParsedNode {
                    id,
                    parent_id,
                    content: ParsedNodeContent::Comment(text),
                });
                self.stats.nodes_created += 1;
            }
            
            Token::Eof => {
                self.phase = ParsePhase::Complete;
            }
        }
    }
    
    fn alloc_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
    
    fn current_parent(&self) -> Option<u32> {
        // Simplified: just return current depth as parent
        if self.open_elements.is_empty() {
            None
        } else {
            Some(self.open_elements.len() as u32 - 1)
        }
    }
}

/// Result of incremental parse step
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseYield {
    /// Need more input data
    NeedMoreData,
    /// Yielded after processing some tokens
    Yielded {
        /// Tokens processed
        tokens: usize,
        /// Can start rendering
        can_render: bool,
    },
    /// Parsing complete
    Complete,
}

/// Check if element is a void element (no closing tag)
fn is_void_element(name: &str) -> bool {
    matches!(
        name,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | 
        "input" | "link" | "meta" | "param" | "source" | "track" | "wbr"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_incremental_parse_simple() {
        let mut parser = IncrementalParser::new();
        parser.feed(b"<html><head></head>");
        parser.process_all();
        
        assert!(parser.has_nodes());
        assert_eq!(parser.phase(), ParsePhase::Head);
    }
    
    #[test]
    fn test_incremental_parse_body() {
        let mut parser = IncrementalParser::new();
        parser.feed(b"<html><body><p>Hello</p></body></html>");
        parser.process_all();
        
        assert!(parser.can_render());
        assert_eq!(parser.phase(), ParsePhase::Body);
    }
    
    #[test]
    fn test_chunked_feeding() {
        let mut parser = IncrementalParser::new();
        
        // Feed in chunks
        parser.feed(b"<html>");
        parser.process_all();
        
        parser.feed(b"<body>");
        parser.process_all();
        
        assert!(parser.can_render());
        
        parser.feed(b"<div>content</div>");
        parser.process_all();
        
        parser.feed(b"</body></html>");
        parser.finish();
        
        assert_eq!(parser.phase(), ParsePhase::Complete);
    }
    
    #[test]
    fn test_yield_after_tokens() {
        let mut parser = IncrementalParser::new();
        parser.feed(b"<html><body><p>1</p><p>2</p><p>3</p></body></html>");
        
        // Process only a few tokens
        let result = parser.process(3);
        
        match result {
            ParseYield::Yielded { tokens, .. } => {
                assert_eq!(tokens, 3);
            }
            _ => panic!("Expected Yielded"),
        }
    }
    
    #[test]
    fn test_tokenizer_attributes() {
        let mut tokenizer = IncrementalTokenizer::new();
        tokenizer.feed(b"<div class=\"foo\" id='bar' disabled>");
        tokenizer.process();
        
        if let Some(Token::StartTag { name, attributes, .. }) = tokenizer.next_token() {
            assert_eq!(name, "div");
            assert_eq!(attributes.len(), 3);
            assert_eq!(attributes[0], ("class".to_string(), "foo".to_string()));
            assert_eq!(attributes[1], ("id".to_string(), "bar".to_string()));
            assert_eq!(attributes[2], ("disabled".to_string(), String::new()));
        } else {
            panic!("Expected StartTag");
        }
    }
}
