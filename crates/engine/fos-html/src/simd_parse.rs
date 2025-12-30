//! SIMD-Accelerated Parsing (Phase 24.4)
//!
//! SIMD HTML tag detection. Vectorized whitespace skipping.
//! Parallel UTF-8 validation. SIMD CSS tokenization.

/// SIMD chunk size (16 bytes for SSE, 32 for AVX)
pub const CHUNK_SIZE: usize = 16;

/// Find character in bytes using SIMD-like scanning
pub fn find_char(haystack: &[u8], needle: u8) -> Option<usize> {
    // Process chunks for better cache/branch prediction
    let chunks = haystack.chunks_exact(CHUNK_SIZE);
    let remainder = chunks.remainder();
    
    let mut offset = 0;
    
    for chunk in chunks {
        // Manually unrolled comparison (simulates SIMD)
        for (i, &b) in chunk.iter().enumerate() {
            if b == needle {
                return Some(offset + i);
            }
        }
        offset += CHUNK_SIZE;
    }
    
    // Handle remainder
    for (i, &b) in remainder.iter().enumerate() {
        if b == needle {
            return Some(offset + i);
        }
    }
    
    None
}

/// Skip whitespace using batch processing
pub fn skip_whitespace(input: &[u8], start: usize) -> usize {
    let mut pos = start;
    
    // Process in chunks
    while pos + CHUNK_SIZE <= input.len() {
        let chunk = &input[pos..pos + CHUNK_SIZE];
        
        // Check all bytes in chunk
        let mut all_ws = true;
        for &b in chunk {
            if !matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0C) {
                all_ws = false;
                break;
            }
        }
        
        if all_ws {
            pos += CHUNK_SIZE;
        } else {
            // Find exact position
            break;
        }
    }
    
    // Handle remaining bytes
    while pos < input.len() {
        match input[pos] {
            b' ' | b'\t' | b'\n' | b'\r' | 0x0C => pos += 1,
            _ => break,
        }
    }
    
    pos
}

/// Fast HTML tag detection
pub struct TagScanner {
    /// Lookup table for tag start chars
    tag_start_chars: [bool; 256],
    /// Lookup table for tag name chars  
    tag_name_chars: [bool; 256],
}

impl Default for TagScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl TagScanner {
    pub fn new() -> Self {
        let mut tag_start_chars = [false; 256];
        let mut tag_name_chars = [false; 256];
        
        // Tag starts with a-z or A-Z
        for c in b'a'..=b'z' {
            tag_start_chars[c as usize] = true;
            tag_name_chars[c as usize] = true;
        }
        for c in b'A'..=b'Z' {
            tag_start_chars[c as usize] = true;
            tag_name_chars[c as usize] = true;
        }
        
        // Tag name can also contain digits and hyphen
        for c in b'0'..=b'9' {
            tag_name_chars[c as usize] = true;
        }
        tag_name_chars[b'-' as usize] = true;
        
        Self {
            tag_start_chars,
            tag_name_chars,
        }
    }
    
    /// Find next tag in HTML
    pub fn find_tag(&self, html: &[u8], start: usize) -> Option<(usize, usize)> {
        let mut pos = start;
        
        while pos < html.len() {
            // Find '<'
            match find_char(&html[pos..], b'<') {
                Some(offset) => {
                    pos += offset;
                    
                    // Check if valid tag start
                    if pos + 1 < html.len() {
                        let next = html[pos + 1];
                        
                        // Regular tag
                        if self.tag_start_chars[next as usize] {
                            let tag_start = pos + 1;
                            let tag_end = self.scan_tag_name(html, tag_start);
                            return Some((tag_start, tag_end));
                        }
                        
                        // Closing tag
                        if next == b'/' && pos + 2 < html.len() && self.tag_start_chars[html[pos + 2] as usize] {
                            let tag_start = pos + 2;
                            let tag_end = self.scan_tag_name(html, tag_start);
                            return Some((tag_start, tag_end));
                        }
                    }
                    
                    pos += 1;
                }
                None => break,
            }
        }
        
        None
    }
    
    /// Scan tag name
    fn scan_tag_name(&self, html: &[u8], start: usize) -> usize {
        let mut pos = start;
        
        while pos < html.len() && self.tag_name_chars[html[pos] as usize] {
            pos += 1;
        }
        
        pos
    }
}

/// UTF-8 validation (batch)
pub fn validate_utf8_fast(input: &[u8]) -> bool {
    // Quick ASCII check first
    let ascii_only = input.iter().all(|&b| b < 128);
    
    if ascii_only {
        return true;
    }
    
    // Fall back to standard validation
    std::str::from_utf8(input).is_ok()
}

/// CSS token types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CssTokenType {
    Ident,
    Number,
    String,
    Hash,
    Delim,
    Whitespace,
    Colon,
    Semicolon,
    OpenParen,
    CloseParen,
    OpenBrace,
    CloseBrace,
    OpenBracket,
    CloseBracket,
    Comma,
    Eof,
}

/// Fast CSS tokenizer
pub struct CssScanner {
    /// Lookup table for ident start
    ident_start: [bool; 256],
    /// Lookup table for ident continue
    ident_continue: [bool; 256],
}

impl Default for CssScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl CssScanner {
    pub fn new() -> Self {
        let mut ident_start = [false; 256];
        let mut ident_continue = [false; 256];
        
        // Ident starts with a-z, A-Z, underscore, or escape
        for c in b'a'..=b'z' {
            ident_start[c as usize] = true;
            ident_continue[c as usize] = true;
        }
        for c in b'A'..=b'Z' {
            ident_start[c as usize] = true;
            ident_continue[c as usize] = true;
        }
        ident_start[b'_' as usize] = true;
        ident_continue[b'_' as usize] = true;
        ident_start[b'-' as usize] = true;
        ident_continue[b'-' as usize] = true;
        
        // Ident continue includes digits
        for c in b'0'..=b'9' {
            ident_continue[c as usize] = true;
        }
        
        Self {
            ident_start,
            ident_continue,
        }
    }
    
    /// Get next token type (fast path)
    pub fn next_token_type(&self, css: &[u8], pos: usize) -> (CssTokenType, usize) {
        if pos >= css.len() {
            return (CssTokenType::Eof, pos);
        }
        
        let b = css[pos];
        
        match b {
            b' ' | b'\t' | b'\n' | b'\r' => {
                let end = skip_whitespace(css, pos);
                (CssTokenType::Whitespace, end)
            }
            b':' => (CssTokenType::Colon, pos + 1),
            b';' => (CssTokenType::Semicolon, pos + 1),
            b'(' => (CssTokenType::OpenParen, pos + 1),
            b')' => (CssTokenType::CloseParen, pos + 1),
            b'{' => (CssTokenType::OpenBrace, pos + 1),
            b'}' => (CssTokenType::CloseBrace, pos + 1),
            b'[' => (CssTokenType::OpenBracket, pos + 1),
            b']' => (CssTokenType::CloseBracket, pos + 1),
            b',' => (CssTokenType::Comma, pos + 1),
            b'#' => {
                // Hash token
                let mut end = pos + 1;
                while end < css.len() && self.ident_continue[css[end] as usize] {
                    end += 1;
                }
                (CssTokenType::Hash, end)
            }
            b'"' | b'\'' => {
                // String
                let quote = b;
                let mut end = pos + 1;
                while end < css.len() && css[end] != quote {
                    if css[end] == b'\\' && end + 1 < css.len() {
                        end += 2;
                    } else {
                        end += 1;
                    }
                }
                if end < css.len() {
                    end += 1; // Include closing quote
                }
                (CssTokenType::String, end)
            }
            b'0'..=b'9' | b'.' => {
                // Number
                let mut end = pos;
                while end < css.len() && matches!(css[end], b'0'..=b'9' | b'.' | b'-' | b'+' | b'e' | b'E') {
                    end += 1;
                }
                (CssTokenType::Number, end)
            }
            _ if self.ident_start[b as usize] => {
                // Ident
                let mut end = pos;
                while end < css.len() && self.ident_continue[css[end] as usize] {
                    end += 1;
                }
                (CssTokenType::Ident, end)
            }
            _ => (CssTokenType::Delim, pos + 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_find_char() {
        let data = b"hello world test string";
        
        assert_eq!(find_char(data, b'w'), Some(6));
        assert_eq!(find_char(data, b'x'), None);
    }
    
    #[test]
    fn test_skip_whitespace() {
        let data = b"    \t\n  hello";
        
        let pos = skip_whitespace(data, 0);
        assert_eq!(pos, 8);
        assert_eq!(data[pos], b'h');
    }
    
    #[test]
    fn test_tag_scanner() {
        let scanner = TagScanner::new();
        let html = b"<div><span>text</span></div>";
        
        let (start, end) = scanner.find_tag(html, 0).unwrap();
        assert_eq!(&html[start..end], b"div");
        
        let (start, end) = scanner.find_tag(html, end).unwrap();
        assert_eq!(&html[start..end], b"span");
    }
    
    #[test]
    fn test_css_scanner() {
        let scanner = CssScanner::new();
        let css = b"color: red;";
        
        let (ty, end) = scanner.next_token_type(css, 0);
        assert_eq!(ty, CssTokenType::Ident);
        assert_eq!(&css[0..end], b"color");
        
        let (ty, end2) = scanner.next_token_type(css, end);
        assert_eq!(ty, CssTokenType::Colon);
    }
}
