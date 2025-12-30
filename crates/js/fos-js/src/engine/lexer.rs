//! JavaScript Lexer
//!
//! Tokenizes JavaScript source code using StringInterner for efficient string handling.

use super::token::{Token, TokenKind, Span, keyword_from_str};
use std::str::Chars;
use std::iter::Peekable;

/// JavaScript Lexer
///
/// Tokenizes ES2023 JavaScript source code.
/// Uses StringInterner from fos-engine for identifier deduplication.
pub struct Lexer<'src> {
    source: &'src str,
    chars: Peekable<Chars<'src>>,
    pos: u32,
    line: u32,
    column: u32,
}

impl<'src> Lexer<'src> {
    /// Create a new lexer for the given source code
    pub fn new(source: &'src str) -> Self {
        Self {
            source,
            chars: source.chars().peekable(),
            pos: 0,
            line: 1,
            column: 1,
        }
    }
    
    /// Get current position
    pub fn position(&self) -> u32 {
        self.pos
    }
    
    /// Get current line number
    pub fn line(&self) -> u32 {
        self.line
    }
    
    /// Get current column number
    pub fn column(&self) -> u32 {
        self.column
    }
    
    /// Peek at the next character
    fn peek(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }
    
    /// Peek at the character after next
    fn peek_next(&self) -> Option<char> {
        let mut iter = self.source[self.pos as usize..].chars();
        iter.next();
        iter.next()
    }
    
    /// Advance to the next character
    fn advance(&mut self) -> Option<char> {
        let c = self.chars.next()?;
        self.pos += c.len_utf8() as u32;
        
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        
        Some(c)
    }
    
    /// Check if at end of input
    fn is_at_end(&mut self) -> bool {
        self.peek().is_none()
    }
    
    /// Skip whitespace and comments
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.peek() {
                // Whitespace
                Some(' ' | '\t' | '\r') => {
                    self.advance();
                }
                // Newline
                Some('\n') => {
                    self.advance();
                }
                // Comments
                Some('/') => {
                    if self.peek_next() == Some('/') {
                        // Single-line comment
                        while self.peek().is_some() && self.peek() != Some('\n') {
                            self.advance();
                        }
                    } else if self.peek_next() == Some('*') {
                        // Multi-line comment
                        self.advance(); // /
                        self.advance(); // *
                        while !self.is_at_end() {
                            if self.peek() == Some('*') && self.peek_next() == Some('/') {
                                self.advance(); // *
                                self.advance(); // /
                                break;
                            }
                            self.advance();
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            }
        }
    }
    
    /// Get the next token
    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();
        
        let start = self.pos;
        
        if self.is_at_end() {
            return Token::new(TokenKind::Eof, Span::new(start, start));
        }
        
        let c = self.advance().unwrap();
        
        let kind = match c {
            // Identifiers and keywords
            'a'..='z' | 'A'..='Z' | '_' | '$' => self.scan_identifier(start),
            
            // Numbers
            '0'..='9' => self.scan_number(start),
            
            // Strings
            '"' | '\'' => self.scan_string(c, start),
            
            // Template literals
            '`' => self.scan_template_literal(start),
            
            // Punctuators and operators
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            ';' => TokenKind::Semicolon,
            ',' => TokenKind::Comma,
            ':' => TokenKind::Colon,
            '~' => TokenKind::Tilde,
            
            '.' => {
                if self.peek() == Some('.') && self.peek_next() == Some('.') {
                    self.advance();
                    self.advance();
                    TokenKind::DotDotDot
                } else if matches!(self.peek(), Some('0'..='9')) {
                    // Number starting with .
                    self.scan_number(start)
                } else {
                    TokenKind::Dot
                }
            }
            
            '?' => {
                if self.peek() == Some('?') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::QuestionQuestionEq
                    } else {
                        TokenKind::QuestionQuestion
                    }
                } else if self.peek() == Some('.') {
                    self.advance();
                    TokenKind::QuestionDot
                } else {
                    TokenKind::Question
                }
            }
            
            '+' => {
                match self.peek() {
                    Some('+') => { self.advance(); TokenKind::PlusPlus }
                    Some('=') => { self.advance(); TokenKind::PlusEq }
                    _ => TokenKind::Plus,
                }
            }
            
            '-' => {
                match self.peek() {
                    Some('-') => { self.advance(); TokenKind::MinusMinus }
                    Some('=') => { self.advance(); TokenKind::MinusEq }
                    _ => TokenKind::Minus,
                }
            }
            
            '*' => {
                if self.peek() == Some('*') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::StarStarEq
                    } else {
                        TokenKind::StarStar
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::StarEq
                } else {
                    TokenKind::Star
                }
            }
            
            '/' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::SlashEq
                } else {
                    TokenKind::Slash
                }
            }
            
            '%' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::PercentEq
                } else {
                    TokenKind::Percent
                }
            }
            
            '<' => {
                if self.peek() == Some('<') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::LShiftEq
                    } else {
                        TokenKind::LShift
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::LessThanEq
                } else {
                    TokenKind::LessThan
                }
            }
            
            '>' => {
                if self.peek() == Some('>') {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        if self.peek() == Some('=') {
                            self.advance();
                            TokenKind::URShiftEq
                        } else {
                            TokenKind::URShift
                        }
                    } else if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::RShiftEq
                    } else {
                        TokenKind::RShift
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::GreaterThanEq
                } else {
                    TokenKind::GreaterThan
                }
            }
            
            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::EqEqEq
                    } else {
                        TokenKind::EqEq
                    }
                } else if self.peek() == Some('>') {
                    self.advance();
                    TokenKind::Arrow
                } else {
                    TokenKind::Eq
                }
            }
            
            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::NotEqEq
                    } else {
                        TokenKind::NotEq
                    }
                } else {
                    TokenKind::Bang
                }
            }
            
            '&' => {
                if self.peek() == Some('&') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::AmpersandAmpersandEq
                    } else {
                        TokenKind::AmpersandAmpersand
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::AmpersandEq
                } else {
                    TokenKind::Ampersand
                }
            }
            
            '|' => {
                if self.peek() == Some('|') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        TokenKind::PipePipeEq
                    } else {
                        TokenKind::PipePipe
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::PipeEq
                } else {
                    TokenKind::Pipe
                }
            }
            
            '^' => {
                if self.peek() == Some('=') {
                    self.advance();
                    TokenKind::CaretEq
                } else {
                    TokenKind::Caret
                }
            }
            
            '#' => {
                // Private identifier
                self.scan_private_identifier(start)
            }
            
            _ => TokenKind::Error(format!("Unexpected character: {}", c).into()),
        };
        
        Token::new(kind, Span::new(start, self.pos))
    }
    
    /// Scan an identifier or keyword
    fn scan_identifier(&mut self, start: u32) -> TokenKind {
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                self.advance();
            } else {
                break;
            }
        }
        
        let text = &self.source[start as usize..self.pos as usize];
        
        // Check for keyword
        keyword_from_str(text).unwrap_or_else(|| TokenKind::Identifier(text.into()))
    }
    
    /// Scan a private identifier (#field)
    fn scan_private_identifier(&mut self, start: u32) -> TokenKind {
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                self.advance();
            } else {
                break;
            }
        }
        
        let text = &self.source[(start + 1) as usize..self.pos as usize];
        TokenKind::PrivateIdentifier(text.into())
    }
    
    /// Scan a number literal
    fn scan_number(&mut self, start: u32) -> TokenKind {
        let first_char = self.source.as_bytes().get(start as usize).copied().unwrap_or(b'0');
        
        // Check for hex, octal, binary
        if first_char == b'0' {
            match self.peek() {
                Some('x' | 'X') => return self.scan_hex_number(start),
                Some('o' | 'O') => return self.scan_octal_number(start),
                Some('b' | 'B') => return self.scan_binary_number(start),
                _ => {}
            }
        }
        
        // Decimal number
        self.scan_decimal_number(start)
    }
    
    fn scan_decimal_number(&mut self, start: u32) -> TokenKind {
        // Integer part
        while matches!(self.peek(), Some('0'..='9' | '_')) {
            self.advance();
        }
        
        // Fractional part
        if self.peek() == Some('.') && matches!(self.peek_next(), Some('0'..='9')) {
            self.advance(); // .
            while matches!(self.peek(), Some('0'..='9' | '_')) {
                self.advance();
            }
        }
        
        // Exponent
        if matches!(self.peek(), Some('e' | 'E')) {
            self.advance();
            if matches!(self.peek(), Some('+' | '-')) {
                self.advance();
            }
            while matches!(self.peek(), Some('0'..='9' | '_')) {
                self.advance();
            }
        }
        
        // Check for BigInt
        if self.peek() == Some('n') {
            self.advance();
            let text = &self.source[start as usize..self.pos as usize - 1];
            return TokenKind::BigInt(text.replace('_', "").into());
        }
        
        let text = &self.source[start as usize..self.pos as usize];
        let num_str = text.replace('_', "");
        
        match num_str.parse::<f64>() {
            Ok(n) => TokenKind::Number(n),
            Err(_) => TokenKind::Error(format!("Invalid number: {}", text).into()),
        }
    }
    
    fn scan_hex_number(&mut self, start: u32) -> TokenKind {
        self.advance(); // x
        while matches!(self.peek(), Some('0'..='9' | 'a'..='f' | 'A'..='F' | '_')) {
            self.advance();
        }
        
        // BigInt?
        if self.peek() == Some('n') {
            self.advance();
            let text = &self.source[start as usize..self.pos as usize - 1];
            return TokenKind::BigInt(text.into());
        }
        
        let text = &self.source[(start + 2) as usize..self.pos as usize];
        let hex_str = text.replace('_', "");
        
        match u64::from_str_radix(&hex_str, 16) {
            Ok(n) => TokenKind::Number(n as f64),
            Err(_) => TokenKind::Error(format!("Invalid hex number").into()),
        }
    }
    
    fn scan_octal_number(&mut self, start: u32) -> TokenKind {
        self.advance(); // o
        while matches!(self.peek(), Some('0'..='7' | '_')) {
            self.advance();
        }
        
        let text = &self.source[(start + 2) as usize..self.pos as usize];
        let oct_str = text.replace('_', "");
        
        match u64::from_str_radix(&oct_str, 8) {
            Ok(n) => TokenKind::Number(n as f64),
            Err(_) => TokenKind::Error(format!("Invalid octal number").into()),
        }
    }
    
    fn scan_binary_number(&mut self, start: u32) -> TokenKind {
        self.advance(); // b
        while matches!(self.peek(), Some('0' | '1' | '_')) {
            self.advance();
        }
        
        let text = &self.source[(start + 2) as usize..self.pos as usize];
        let bin_str = text.replace('_', "");
        
        match u64::from_str_radix(&bin_str, 2) {
            Ok(n) => TokenKind::Number(n as f64),
            Err(_) => TokenKind::Error(format!("Invalid binary number").into()),
        }
    }
    
    /// Scan a string literal
    fn scan_string(&mut self, quote: char, start: u32) -> TokenKind {
        let mut value = String::new();
        
        while let Some(c) = self.peek() {
            if c == quote {
                self.advance();
                return TokenKind::String(value.into());
            } else if c == '\\' {
                self.advance();
                match self.advance() {
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some('\\') => value.push('\\'),
                    Some('\'') => value.push('\''),
                    Some('"') => value.push('"'),
                    Some('0') => value.push('\0'),
                    Some('x') => {
                        // Hex escape
                        if let Some(hex) = self.scan_hex_escape(2) {
                            if let Some(c) = char::from_u32(hex) {
                                value.push(c);
                            }
                        }
                    }
                    Some('u') => {
                        // Unicode escape
                        if self.peek() == Some('{') {
                            self.advance();
                            if let Some(code) = self.scan_unicode_escape_braces() {
                                if let Some(c) = char::from_u32(code) {
                                    value.push(c);
                                }
                            }
                        } else if let Some(code) = self.scan_hex_escape(4) {
                            if let Some(c) = char::from_u32(code) {
                                value.push(c);
                            }
                        }
                    }
                    Some(c) => value.push(c),
                    None => break,
                }
            } else if c == '\n' {
                return TokenKind::Error("Unterminated string".into());
            } else {
                value.push(c);
                self.advance();
            }
        }
        
        TokenKind::Error("Unterminated string".into())
    }
    
    fn scan_hex_escape(&mut self, count: usize) -> Option<u32> {
        let start = self.pos;
        for _ in 0..count {
            if matches!(self.peek(), Some('0'..='9' | 'a'..='f' | 'A'..='F')) {
                self.advance();
            } else {
                return None;
            }
        }
        let hex = &self.source[start as usize..self.pos as usize];
        u32::from_str_radix(hex, 16).ok()
    }
    
    fn scan_unicode_escape_braces(&mut self) -> Option<u32> {
        let start = self.pos;
        while matches!(self.peek(), Some('0'..='9' | 'a'..='f' | 'A'..='F')) {
            self.advance();
        }
        if self.peek() == Some('}') {
            self.advance();
            let hex = &self.source[start as usize..(self.pos - 1) as usize];
            return u32::from_str_radix(hex, 16).ok();
        }
        None
    }
    
    /// Scan a template literal
    fn scan_template_literal(&mut self, start: u32) -> TokenKind {
        let mut value = String::new();
        
        while let Some(c) = self.peek() {
            match c {
                '`' => {
                    self.advance();
                    return TokenKind::NoSubstitutionTemplate(value.into());
                }
                '$' if self.peek_next() == Some('{') => {
                    self.advance(); // $
                    self.advance(); // {
                    return TokenKind::TemplateHead(value.into());
                }
                '\\' => {
                    self.advance();
                    match self.advance() {
                        Some('n') => value.push('\n'),
                        Some('r') => value.push('\r'),
                        Some('t') => value.push('\t'),
                        Some('\\') => value.push('\\'),
                        Some('`') => value.push('`'),
                        Some('$') => value.push('$'),
                        Some(c) => value.push(c),
                        None => break,
                    }
                }
                _ => {
                    value.push(c);
                    self.advance();
                }
            }
        }
        
        TokenKind::Error("Unterminated template literal".into())
    }
    
    /// Tokenize all remaining input
    pub fn tokenize_all(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token();
            let is_eof = matches!(token.kind, TokenKind::Eof);
            tokens.push(token);
            if is_eof {
                break;
            }
        }
        tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_tokens() {
        let mut lexer = Lexer::new("let x = 42;");
        
        assert!(matches!(lexer.next_token().kind, TokenKind::Let));
        assert!(matches!(lexer.next_token().kind, TokenKind::Identifier(_)));
        assert!(matches!(lexer.next_token().kind, TokenKind::Eq));
        assert!(matches!(lexer.next_token().kind, TokenKind::Number(n) if n == 42.0));
        assert!(matches!(lexer.next_token().kind, TokenKind::Semicolon));
        assert!(matches!(lexer.next_token().kind, TokenKind::Eof));
    }
    
    #[test]
    fn test_operators() {
        let mut lexer = Lexer::new("+ ++ += === !== && || ??");
        
        assert!(matches!(lexer.next_token().kind, TokenKind::Plus));
        assert!(matches!(lexer.next_token().kind, TokenKind::PlusPlus));
        assert!(matches!(lexer.next_token().kind, TokenKind::PlusEq));
        assert!(matches!(lexer.next_token().kind, TokenKind::EqEqEq));
        assert!(matches!(lexer.next_token().kind, TokenKind::NotEqEq));
        assert!(matches!(lexer.next_token().kind, TokenKind::AmpersandAmpersand));
        assert!(matches!(lexer.next_token().kind, TokenKind::PipePipe));
        assert!(matches!(lexer.next_token().kind, TokenKind::QuestionQuestion));
    }
    
    #[test]
    fn test_string_literal() {
        let mut lexer = Lexer::new("\"hello world\"");
        let token = lexer.next_token();
        assert!(matches!(token.kind, TokenKind::String(s) if &*s == "hello world"));
    }
    
    #[test]
    fn test_string_escapes() {
        let mut lexer = Lexer::new(r#""\n\t\\""#);
        let token = lexer.next_token();
        assert!(matches!(token.kind, TokenKind::String(s) if &*s == "\n\t\\"));
    }
    
    #[test]
    fn test_numbers() {
        let mut lexer = Lexer::new("123 45.67 0xFF 0b1010 0o777 1_000_000");
        
        assert!(matches!(lexer.next_token().kind, TokenKind::Number(n) if n == 123.0));
        assert!(matches!(lexer.next_token().kind, TokenKind::Number(n) if (n - 45.67).abs() < 0.001));
        assert!(matches!(lexer.next_token().kind, TokenKind::Number(n) if n == 255.0));
        assert!(matches!(lexer.next_token().kind, TokenKind::Number(n) if n == 10.0));
        assert!(matches!(lexer.next_token().kind, TokenKind::Number(n) if n == 511.0));
        assert!(matches!(lexer.next_token().kind, TokenKind::Number(n) if n == 1000000.0));
    }
    
    #[test]
    fn test_comments() {
        let mut lexer = Lexer::new("a // comment\nb /* block */ c");
        
        assert!(matches!(lexer.next_token().kind, TokenKind::Identifier(s) if &*s == "a"));
        assert!(matches!(lexer.next_token().kind, TokenKind::Identifier(s) if &*s == "b"));
        assert!(matches!(lexer.next_token().kind, TokenKind::Identifier(s) if &*s == "c"));
    }
    
    #[test]
    fn test_keywords() {
        let mut lexer = Lexer::new("if else while for function class");
        
        assert!(matches!(lexer.next_token().kind, TokenKind::If));
        assert!(matches!(lexer.next_token().kind, TokenKind::Else));
        assert!(matches!(lexer.next_token().kind, TokenKind::While));
        assert!(matches!(lexer.next_token().kind, TokenKind::For));
        assert!(matches!(lexer.next_token().kind, TokenKind::Function));
        assert!(matches!(lexer.next_token().kind, TokenKind::Class));
    }
    
    #[test]
    fn test_arrow_function() {
        let mut lexer = Lexer::new("(x) => x * 2");
        
        assert!(matches!(lexer.next_token().kind, TokenKind::LParen));
        assert!(matches!(lexer.next_token().kind, TokenKind::Identifier(_)));
        assert!(matches!(lexer.next_token().kind, TokenKind::RParen));
        assert!(matches!(lexer.next_token().kind, TokenKind::Arrow));
        assert!(matches!(lexer.next_token().kind, TokenKind::Identifier(_)));
        assert!(matches!(lexer.next_token().kind, TokenKind::Star));
        assert!(matches!(lexer.next_token().kind, TokenKind::Number(_)));
    }
    
    #[test]
    fn test_template_literal() {
        let mut lexer = Lexer::new("`hello world`");
        let token = lexer.next_token();
        assert!(matches!(token.kind, TokenKind::NoSubstitutionTemplate(s) if &*s == "hello world"));
    }
    
    #[test]
    fn test_private_identifier() {
        let mut lexer = Lexer::new("#privateField");
        let token = lexer.next_token();
        assert!(matches!(token.kind, TokenKind::PrivateIdentifier(s) if &*s == "privateField"));
    }
    
    #[test]
    fn test_spread() {
        let mut lexer = Lexer::new("...args");
        assert!(matches!(lexer.next_token().kind, TokenKind::DotDotDot));
        assert!(matches!(lexer.next_token().kind, TokenKind::Identifier(_)));
    }
    
    #[test]
    fn test_optional_chaining() {
        let mut lexer = Lexer::new("obj?.prop");
        assert!(matches!(lexer.next_token().kind, TokenKind::Identifier(_)));
        assert!(matches!(lexer.next_token().kind, TokenKind::QuestionDot));
        assert!(matches!(lexer.next_token().kind, TokenKind::Identifier(_)));
    }
}
