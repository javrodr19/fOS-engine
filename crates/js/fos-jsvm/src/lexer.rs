//! Lexer
//!
//! Turns source text into tokens on demand. The parser drives it, because
//! JavaScript's lexical grammar depends on syntactic context: a `/` starts a
//! regular expression only where an expression may begin, and a `}` resumes
//! a template literal only when it closes a `${` substitution. For those
//! cases the parser asks the lexer to rescan the current token.

use std::fmt;

/// Byte range in the source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

/// A decoded string literal (or template chunk). JavaScript strings are
/// sequences of UTF-16 code units and may contain lone surrogates, which
/// Rust strings cannot hold, so literal values are kept as UTF-16.
pub type Utf16 = Box<[u16]>;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Eof,
    Ident(Box<str>),
    /// Reserved word, including contextual ones used as keywords
    Keyword(Kw),
    Num(f64),
    BigInt(Box<str>),
    Str(Utf16),
    /// Template chunk: cooked value (None if it has an invalid escape), raw
    /// text, and whether it ends the template (`` ` ``) rather than a `${`
    Template { cooked: Option<Utf16>, raw: Box<str>, tail: bool },
    Regex { pattern: Box<str>, flags: Box<str> },
    Punct(P),
}

macro_rules! keywords {
    ($($name:ident = $text:literal),* $(,)?) => {
        /// Keywords
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Kw { $($name),* }

        impl Kw {
            pub fn from_str(s: &str) -> Option<Kw> {
                match s { $($text => Some(Kw::$name),)* _ => None }
            }

            pub fn as_str(self) -> &'static str {
                match self { $(Kw::$name => $text),* }
            }
        }
    };
}

keywords! {
    Await = "await", Break = "break", Case = "case", Catch = "catch", Class = "class",
    Const = "const", Continue = "continue", Debugger = "debugger", Default = "default",
    Delete = "delete", Do = "do", Else = "else", Export = "export", Extends = "extends",
    False = "false", Finally = "finally", For = "for", Function = "function", If = "if",
    Import = "import", In = "in", Instanceof = "instanceof", New = "new", Null = "null",
    Return = "return", Super = "super", Switch = "switch", This = "this", Throw = "throw",
    True = "true", Try = "try", Typeof = "typeof", Var = "var", Void = "void",
    While = "while", With = "with", Yield = "yield", Let = "let", Static = "static",
    Enum = "enum",
}

impl Kw {
    /// Keywords that may still be used as identifiers in sloppy code (or
    /// outside generators / async functions)
    pub fn is_contextual(self) -> bool {
        matches!(self, Kw::Await | Kw::Yield | Kw::Let | Kw::Static)
    }
}

macro_rules! puncts {
    ($($name:ident = $text:literal),* $(,)?) => {
        /// Punctuators
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum P { $($name),* }

        impl P {
            pub fn as_str(self) -> &'static str {
                match self { $(P::$name => $text),* }
            }
        }
    };
}

puncts! {
    LBrace = "{", RBrace = "}", LParen = "(", RParen = ")", LBracket = "[", RBracket = "]",
    Dot = ".", Ellipsis = "...", Semi = ";", Comma = ",", Lt = "<", Gt = ">", Le = "<=",
    Ge = ">=", EqEq = "==", Ne = "!=", EqEqEq = "===", NeEq = "!==", Plus = "+", Minus = "-",
    Star = "*", Slash = "/", Percent = "%", StarStar = "**", PlusPlus = "++",
    MinusMinus = "--", Shl = "<<", Sar = ">>", Shr = ">>>", Amp = "&", Pipe = "|",
    Caret = "^", Bang = "!", Tilde = "~", AmpAmp = "&&", PipePipe = "||",
    QuestionQuestion = "??", Question = "?", QuestionDot = "?.", Colon = ":", Eq = "=",
    PlusEq = "+=", MinusEq = "-=", StarEq = "*=", SlashEq = "/=", PercentEq = "%=",
    StarStarEq = "**=", ShlEq = "<<=", SarEq = ">>=", ShrEq = ">>>=", AmpEq = "&=",
    PipeEq = "|=", CaretEq = "^=", AmpAmpEq = "&&=", PipePipeEq = "||=",
    QuestionQuestionEq = "??=", Arrow = "=>", Hash = "#",
}

/// A token with its location. `newline_before` records whether a line
/// terminator precedes it, which automatic semicolon insertion and the
/// "no line terminator here" restrictions depend on.
#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
    pub newline_before: bool,
    /// The identifier or keyword was written with `\u` escapes
    pub escaped: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SyntaxError {
    pub message: String,
    pub pos: u32,
}

impl fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SyntaxError: {} (at offset {})", self.message, self.pos)
    }
}

impl std::error::Error for SyntaxError {}

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

fn is_line_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

fn is_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\u{b}' | '\u{c}' | ' ' | '\u{a0}' | '\u{feff}')
        || (c > '\u{7f}' && c.is_whitespace() && !is_line_terminator(c))
}

/// ID_Start (with `$` and `_`): letters, and the Other_ID_Start symbols
/// such as `℘` and `℮` that minifiers use for short names (the kana voicing
/// marks are the ID_Start characters neither letters nor XID_Start)
pub fn is_id_start(c: char) -> bool {
    c.is_ascii_alphabetic()
        || c == '$'
        || c == '_'
        || (c > '\u{7f}' && (c.is_alphabetic() || unicode_ident::is_xid_start(c) || matches!(c, '\u{309b}' | '\u{309c}')))
}

pub fn is_id_continue(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || c == '$'
        || c == '_'
        || c == '\u{200c}'
        || c == '\u{200d}'
        || (c > '\u{7f}' && (c.is_alphanumeric() || unicode_mark_or_connector(c) || unicode_ident::is_xid_continue(c)))
}

/// Rough check for combining marks and connector punctuation, which may
/// continue identifiers (Unicode categories Mn, Mc, Pc)
fn unicode_mark_or_connector(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F | 0x0483..=0x0487 | 0x0591..=0x05BD | 0x0610..=0x061A |
        0x064B..=0x065F | 0x0900..=0x0903 | 0x093A..=0x094F | 0x1AB0..=0x1AFF |
        0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F | 0x203F | 0x2040 | 0xFE33 |
        0xFE34 | 0xFE4D..=0xFE4F | 0xFF3F)
}

fn push_utf16(out: &mut Vec<u16>, c: char) {
    let mut buf = [0u16; 2];
    out.extend_from_slice(c.encode_utf16(&mut buf));
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self { src, bytes: src.as_bytes(), pos: 0 }
    }

    pub fn source(&self) -> &'a str {
        self.src
    }

    /// Continue scanning from a byte offset
    pub fn seek(&mut self, pos: usize) {
        self.pos = pos.min(self.src.len());
    }

    fn err<T>(&self, message: impl Into<String>, pos: usize) -> Result<T, SyntaxError> {
        Err(SyntaxError { message: message.into(), pos: pos as u32 })
    }

    fn peek_char(&self) -> Option<char> {
        let b = *self.bytes.get(self.pos)?;
        if b < 0x80 { Some(b as char) } else { self.src[self.pos..].chars().next() }
    }

    fn peek_byte(&self, offset: usize) -> u8 {
        self.bytes.get(self.pos + offset).copied().unwrap_or(0)
    }

    fn bump_char(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    /// Skip whitespace and comments; returns whether a line terminator was
    /// crossed
    fn skip_trivia(&mut self) -> Result<bool, SyntaxError> {
        let mut newline = false;
        loop {
            match self.peek_byte(0) {
                b' ' | b'\t' | 0x0b | 0x0c => self.pos += 1,
                b'\n' | b'\r' => {
                    newline = true;
                    self.pos += 1;
                }
                b'/' if self.peek_byte(1) == b'/' => {
                    self.skip_line_comment();
                }
                b'/' if self.peek_byte(1) == b'*' => {
                    let start = self.pos;
                    self.pos += 2;
                    loop {
                        match self.peek_char() {
                            None => return self.err("unterminated comment", start),
                            Some('*') if self.peek_byte(1) == b'/' => {
                                self.pos += 2;
                                break;
                            }
                            Some(c) => {
                                if is_line_terminator(c) {
                                    newline = true;
                                }
                                self.pos += c.len_utf8();
                            }
                        }
                    }
                }
                // HTML-like comments (Annex B): `<!--` anywhere, `-->` at line start
                b'<' if self.src[self.pos..].starts_with("<!--") => self.skip_line_comment(),
                b'-' if (newline || self.pos == 0) && self.src[self.pos..].starts_with("-->") => {
                    self.skip_line_comment()
                }
                b'#' if self.pos == 0 && self.peek_byte(1) == b'!' => self.skip_line_comment(),
                b if b >= 0x80 => {
                    let c = self.peek_char().unwrap_or('\0');
                    if is_line_terminator(c) {
                        newline = true;
                    } else if !is_whitespace(c) {
                        break;
                    }
                    self.pos += c.len_utf8();
                }
                _ => break,
            }
        }
        Ok(newline)
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek_char() {
            if is_line_terminator(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
    }

    /// Scan the next token
    pub fn next_token(&mut self) -> Result<Token, SyntaxError> {
        let newline_before = self.skip_trivia()?;
        let start = self.pos;
        let mut escaped = false;
        let tok = match self.peek_char() {
            None => Tok::Eof,
            Some(c) if is_id_start(c) || c == '\\' => {
                let (name, esc) = self.scan_identifier_name()?;
                escaped = esc;
                match Kw::from_str(&name) {
                    Some(kw) => Tok::Keyword(kw),
                    None => Tok::Ident(name.into()),
                }
            }
            Some(c) if c.is_ascii_digit() => self.scan_number()?,
            Some('.') if self.peek_byte(1).is_ascii_digit() => self.scan_number()?,
            Some('"') | Some('\'') => self.scan_string()?,
            Some('`') => {
                self.pos += 1;
                self.scan_template_chunk()?
            }
            Some(_) => Tok::Punct(self.scan_punct()?),
        };
        Ok(Token { tok, span: Span { start: start as u32, end: self.pos as u32 }, newline_before, escaped })
    }

    fn scan_identifier_name(&mut self) -> Result<(String, bool), SyntaxError> {
        let mut name = String::new();
        let mut escaped = false;
        let mut first = true;
        loop {
            match self.peek_char() {
                Some('\\') => {
                    let at = self.pos;
                    self.pos += 1;
                    if self.peek_byte(0) != b'u' {
                        return self.err("invalid escape in identifier", at);
                    }
                    self.pos += 1;
                    let c = self.scan_unicode_escape_body()?;
                    let c = char::from_u32(c).filter(|&c| if first { is_id_start(c) } else { is_id_continue(c) });
                    match c {
                        Some(c) => name.push(c),
                        None => return self.err("invalid identifier escape", at),
                    }
                    escaped = true;
                }
                Some(c) if (first && is_id_start(c)) || (!first && is_id_continue(c)) => {
                    name.push(c);
                    self.pos += c.len_utf8();
                }
                _ => break,
            }
            first = false;
        }
        Ok((name, escaped))
    }

    /// After `\u`: `XXXX` or `{X...}`
    fn scan_unicode_escape_body(&mut self) -> Result<u32, SyntaxError> {
        let start = self.pos;
        if self.peek_byte(0) == b'{' {
            self.pos += 1;
            let mut value: u32 = 0;
            let mut digits = 0;
            while let Some(d) = (self.peek_byte(0) as char).to_digit(16) {
                value = value.saturating_mul(16).saturating_add(d);
                self.pos += 1;
                digits += 1;
            }
            if digits == 0 || self.peek_byte(0) != b'}' || value > 0x10FFFF {
                return self.err("invalid Unicode escape", start);
            }
            self.pos += 1;
            Ok(value)
        } else {
            let mut value = 0;
            for _ in 0..4 {
                let Some(d) = (self.peek_byte(0) as char).to_digit(16) else {
                    return self.err("invalid Unicode escape", start);
                };
                value = value * 16 + d;
                self.pos += 1;
            }
            Ok(value)
        }
    }

    fn scan_number(&mut self) -> Result<Tok, SyntaxError> {
        let start = self.pos;
        let radix = if self.peek_byte(0) == b'0' {
            match self.peek_byte(1) | 0x20 {
                b'x' => 16,
                b'o' => 8,
                b'b' => 2,
                _ => 10,
            }
        } else {
            10
        };

        let value = if radix != 10 {
            self.pos += 2;
            let digits = self.scan_digits(radix)?;
            if digits.is_empty() {
                return self.err("missing digits after radix prefix", start);
            }
            if self.peek_byte(0) == b'n' {
                self.pos += 1;
                self.check_after_number(start)?;
                return Ok(Tok::BigInt(format!("{}", parse_radix_bigint_text(&digits, radix)).into()));
            }
            digits.chars().fold(0f64, |acc, c| acc * radix as f64 + c.to_digit(radix).unwrap() as f64)
        } else if self.peek_byte(0) == b'0' && self.peek_byte(1).is_ascii_digit() {
            // Legacy octal (sloppy mode) or decimal with a leading zero
            let digits = self.scan_digits(10)?;
            if digits.bytes().all(|b| b < b'8') {
                digits.chars().fold(0f64, |acc, c| acc * 8.0 + c.to_digit(8).unwrap() as f64)
            } else {
                digits.parse::<f64>().unwrap_or(f64::NAN)
            }
        } else {
            let mut text = self.scan_digits(10)?;
            if self.peek_byte(0) == b'n' {
                self.pos += 1;
                self.check_after_number(start)?;
                return Ok(Tok::BigInt(text.into()));
            }
            if self.peek_byte(0) == b'.' {
                self.pos += 1;
                text.push('.');
                text.push_str(&self.scan_digits(10)?);
            }
            if self.peek_byte(0) | 0x20 == b'e' {
                let save = self.pos;
                self.pos += 1;
                let mut exp = String::from("e");
                if matches!(self.peek_byte(0), b'+' | b'-') {
                    exp.push(self.peek_byte(0) as char);
                    self.pos += 1;
                }
                let digits = self.scan_digits(10)?;
                if digits.is_empty() {
                    self.pos = save;
                    return self.err("missing exponent", start);
                }
                exp.push_str(&digits);
                text.push_str(&exp);
            }
            if text.starts_with('.') {
                text.insert(0, '0');
            }
            text.parse::<f64>().unwrap_or(f64::NAN)
        };
        self.check_after_number(start)?;
        Ok(Tok::Num(value))
    }

    /// A number may not be immediately followed by an identifier or digit
    fn check_after_number(&self, start: usize) -> Result<(), SyntaxError> {
        match self.peek_char() {
            Some(c) if is_id_start(c) || c.is_ascii_digit() || c == '\\' => {
                self.err("identifier starts immediately after numeric literal", start)
            }
            _ => Ok(()),
        }
    }

    /// Digits of a radix, with `_` separators removed
    fn scan_digits(&mut self, radix: u32) -> Result<String, SyntaxError> {
        let mut out = String::new();
        let mut last_sep = false;
        loop {
            let b = self.peek_byte(0);
            if b == b'_' {
                if out.is_empty() || last_sep {
                    return self.err("invalid numeric separator", self.pos);
                }
                last_sep = true;
                self.pos += 1;
                continue;
            }
            if (b as char).to_digit(radix).is_none() {
                break;
            }
            out.push(b as char);
            last_sep = false;
            self.pos += 1;
        }
        if last_sep {
            return self.err("trailing numeric separator", self.pos);
        }
        Ok(out)
    }

    fn scan_string(&mut self) -> Result<Tok, SyntaxError> {
        let start = self.pos;
        let quote = self.bump_char().unwrap();
        let mut out: Vec<u16> = Vec::new();
        loop {
            match self.bump_char() {
                None => return self.err("unterminated string", start),
                Some(c) if c == quote => break,
                Some('\\') => {
                    if let Some(unit) = self.scan_escape(false)? {
                        out.extend_from_slice(&unit);
                    }
                }
                Some('\n') | Some('\r') => return self.err("unterminated string", start),
                Some(c) => push_utf16(&mut out, c),
            }
        }
        Ok(Tok::Str(out.into()))
    }

    /// Decode an escape after `\`. Returns the code units (None for a line
    /// continuation). In templates, octal escapes are errors reported via
    /// `Err` so the caller can treat the cooked value as undefined.
    fn scan_escape(&mut self, template: bool) -> Result<Option<Vec<u16>>, SyntaxError> {
        let at = self.pos - 1;
        let Some(c) = self.bump_char() else { return self.err("unterminated escape", at) };
        let unit = |u: u16| Ok(Some(vec![u]));
        match c {
            'n' => unit(0x0a),
            't' => unit(0x09),
            'r' => unit(0x0d),
            'b' => unit(0x08),
            'f' => unit(0x0c),
            'v' => unit(0x0b),
            '0' if !self.peek_byte(0).is_ascii_digit() => unit(0),
            '0'..='7' => {
                if template {
                    return self.err("octal escape in template", at);
                }
                // Legacy octal escape (up to three digits, value <= 0o377)
                let mut value = c.to_digit(8).unwrap();
                for _ in 0..2 {
                    match (self.peek_byte(0) as char).to_digit(8) {
                        Some(d) if value * 8 + d <= 0o377 => {
                            value = value * 8 + d;
                            self.pos += 1;
                        }
                        _ => break,
                    }
                }
                unit(value as u16)
            }
            '8' | '9' => {
                if template {
                    return self.err("invalid escape in template", at);
                }
                unit(c as u16)
            }
            'x' => {
                let hi = (self.peek_byte(0) as char).to_digit(16);
                let lo = (self.peek_byte(1) as char).to_digit(16);
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        self.pos += 2;
                        unit((h * 16 + l) as u16)
                    }
                    _ => self.err("invalid hexadecimal escape", at),
                }
            }
            'u' => {
                let cp = self.scan_unicode_escape_body()?;
                let mut out = Vec::new();
                if cp < 0x10000 {
                    out.push(cp as u16);
                } else {
                    let v = cp - 0x10000;
                    out.push(0xD800 | (v >> 10) as u16);
                    out.push(0xDC00 | (v & 0x3FF) as u16);
                }
                Ok(Some(out))
            }
            '\r' => {
                if self.peek_byte(0) == b'\n' {
                    self.pos += 1;
                }
                Ok(None)
            }
            '\n' | '\u{2028}' | '\u{2029}' => Ok(None),
            other => {
                let mut out = Vec::new();
                push_utf16(&mut out, other);
                Ok(Some(out))
            }
        }
    }

    /// Scan a template chunk; the opening `` ` `` or `}` has been consumed
    fn scan_template_chunk(&mut self) -> Result<Tok, SyntaxError> {
        let start = self.pos;
        let mut cooked: Option<Vec<u16>> = Some(Vec::new());
        let mut raw = String::new();
        loop {
            let chunk_pos = self.pos;
            match self.bump_char() {
                None => return self.err("unterminated template literal", start),
                Some('`') => {
                    return Ok(Tok::Template { cooked: cooked.map(Into::into), raw: raw.into(), tail: true });
                }
                Some('$') if self.peek_byte(0) == b'{' => {
                    self.pos += 1;
                    return Ok(Tok::Template { cooked: cooked.map(Into::into), raw: raw.into(), tail: false });
                }
                Some('\\') => {
                    let escape_start = self.pos;
                    match self.scan_escape(true) {
                        Ok(units) => {
                            if let (Some(out), Some(units)) = (cooked.as_mut(), units) {
                                out.extend_from_slice(&units);
                            }
                        }
                        Err(_) => {
                            // Invalid escapes make the cooked value undefined
                            // (legal only in tagged templates); skip one char
                            cooked = None;
                            self.pos = escape_start;
                            self.bump_char();
                        }
                    }
                    raw.push_str(&self.src[chunk_pos..self.pos].replace("\r\n", "\n").replace('\r', "\n"));
                }
                Some('\r') => {
                    // CRLF and CR normalize to LF in both cooked and raw
                    if self.peek_byte(0) == b'\n' {
                        self.pos += 1;
                    }
                    if let Some(out) = cooked.as_mut() {
                        out.push(0x0a);
                    }
                    raw.push('\n');
                }
                Some(c) => {
                    if let Some(out) = cooked.as_mut() {
                        push_utf16(out, c);
                    }
                    raw.push(c);
                }
            }
        }
    }

    /// Rescan from the `}` that ends a template substitution
    pub fn rescan_template_continuation(&mut self, token: &Token) -> Result<Token, SyntaxError> {
        self.pos = token.span.start as usize + 1;
        let tok = self.scan_template_chunk()?;
        Ok(Token { tok, span: Span { start: token.span.start, end: self.pos as u32 }, newline_before: token.newline_before, escaped: false })
    }

    /// Rescan a `/` or `/=` token as a regular expression literal
    pub fn rescan_regex(&mut self, token: &Token) -> Result<Token, SyntaxError> {
        let start = token.span.start as usize;
        self.pos = start + 1;
        let mut in_class = false;
        loop {
            match self.bump_char() {
                None => return self.err("unterminated regular expression", start),
                Some(c) if is_line_terminator(c) => return self.err("unterminated regular expression", start),
                Some('\\') => {
                    match self.bump_char() {
                        Some(c) if !is_line_terminator(c) => {}
                        _ => return self.err("unterminated regular expression", start),
                    }
                }
                Some('[') => in_class = true,
                Some(']') => in_class = false,
                Some('/') if !in_class => break,
                Some(_) => {}
            }
        }
        let pattern = self.src[start + 1..self.pos - 1].into();
        let flags_start = self.pos;
        while let Some(c) = self.peek_char() {
            if !is_id_continue(c) {
                break;
            }
            self.pos += c.len_utf8();
        }
        let flags = self.src[flags_start..self.pos].into();
        Ok(Token {
            tok: Tok::Regex { pattern, flags },
            span: Span { start: start as u32, end: self.pos as u32 },
            newline_before: token.newline_before,
            escaped: false,
        })
    }

    fn scan_punct(&mut self) -> Result<P, SyntaxError> {
        let s = &self.bytes[self.pos..];
        let b0 = s[0];
        let b1 = s.get(1).copied().unwrap_or(0);
        let b2 = s.get(2).copied().unwrap_or(0);
        let b3 = s.get(3).copied().unwrap_or(0);
        let (p, len) = match b0 {
            b'{' => (P::LBrace, 1),
            b'}' => (P::RBrace, 1),
            b'(' => (P::LParen, 1),
            b')' => (P::RParen, 1),
            b'[' => (P::LBracket, 1),
            b']' => (P::RBracket, 1),
            b';' => (P::Semi, 1),
            b',' => (P::Comma, 1),
            b':' => (P::Colon, 1),
            b'~' => (P::Tilde, 1),
            b'#' => (P::Hash, 1),
            b'.' => if b1 == b'.' && b2 == b'.' { (P::Ellipsis, 3) } else { (P::Dot, 1) },
            b'<' => match (b1, b2) {
                (b'<', b'=') => (P::ShlEq, 3),
                (b'<', _) => (P::Shl, 2),
                (b'=', _) => (P::Le, 2),
                _ => (P::Lt, 1),
            },
            b'>' => match (b1, b2, b3) {
                (b'>', b'>', b'=') => (P::ShrEq, 4),
                (b'>', b'>', _) => (P::Shr, 3),
                (b'>', b'=', _) => (P::SarEq, 3),
                (b'>', _, _) => (P::Sar, 2),
                (b'=', _, _) => (P::Ge, 2),
                _ => (P::Gt, 1),
            },
            b'=' => match (b1, b2) {
                (b'=', b'=') => (P::EqEqEq, 3),
                (b'=', _) => (P::EqEq, 2),
                (b'>', _) => (P::Arrow, 2),
                _ => (P::Eq, 1),
            },
            b'!' => match (b1, b2) {
                (b'=', b'=') => (P::NeEq, 3),
                (b'=', _) => (P::Ne, 2),
                _ => (P::Bang, 1),
            },
            b'+' => match b1 {
                b'+' => (P::PlusPlus, 2),
                b'=' => (P::PlusEq, 2),
                _ => (P::Plus, 1),
            },
            b'-' => match b1 {
                b'-' => (P::MinusMinus, 2),
                b'=' => (P::MinusEq, 2),
                _ => (P::Minus, 1),
            },
            b'*' => match (b1, b2) {
                (b'*', b'=') => (P::StarStarEq, 3),
                (b'*', _) => (P::StarStar, 2),
                (b'=', _) => (P::StarEq, 2),
                _ => (P::Star, 1),
            },
            b'/' => if b1 == b'=' { (P::SlashEq, 2) } else { (P::Slash, 1) },
            b'%' => if b1 == b'=' { (P::PercentEq, 2) } else { (P::Percent, 1) },
            b'&' => match (b1, b2) {
                (b'&', b'=') => (P::AmpAmpEq, 3),
                (b'&', _) => (P::AmpAmp, 2),
                (b'=', _) => (P::AmpEq, 2),
                _ => (P::Amp, 1),
            },
            b'|' => match (b1, b2) {
                (b'|', b'=') => (P::PipePipeEq, 3),
                (b'|', _) => (P::PipePipe, 2),
                (b'=', _) => (P::PipeEq, 2),
                _ => (P::Pipe, 1),
            },
            b'^' => if b1 == b'=' { (P::CaretEq, 2) } else { (P::Caret, 1) },
            b'?' => match (b1, b2) {
                (b'?', b'=') => (P::QuestionQuestionEq, 3),
                (b'?', _) => (P::QuestionQuestion, 2),
                // `?.` but not `?.5` (a conditional followed by a number)
                (b'.', d) if !d.is_ascii_digit() => (P::QuestionDot, 2),
                _ => (P::Question, 1),
            },
            _ => {
                let c = self.peek_char().unwrap_or('\0');
                return self.err(format!("unexpected character {:?}", c), self.pos);
            }
        };
        self.pos += len;
        Ok(p)
    }
}

/// Convert digits in a radix to decimal text (for BigInt literals)
fn parse_radix_bigint_text(digits: &str, radix: u32) -> String {
    // Schoolbook conversion on base-10^9 limbs
    let mut limbs: Vec<u32> = vec![0];
    for c in digits.chars() {
        let mut carry = c.to_digit(radix).unwrap() as u64;
        for limb in limbs.iter_mut() {
            let v = *limb as u64 * radix as u64 + carry;
            *limb = (v % 1_000_000_000) as u32;
            carry = v / 1_000_000_000;
        }
        while carry > 0 {
            limbs.push((carry % 1_000_000_000) as u32);
            carry /= 1_000_000_000;
        }
    }
    let mut out = limbs.last().unwrap().to_string();
    for limb in limbs.iter().rev().skip(1) {
        out.push_str(&format!("{:09}", limb));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok> {
        let mut lexer = Lexer::new(src);
        let mut out = Vec::new();
        loop {
            let t = lexer.next_token().unwrap();
            if t.tok == Tok::Eof {
                break;
            }
            out.push(t.tok);
        }
        out
    }

    fn s(text: &str) -> Tok {
        Tok::Str(text.encode_utf16().collect::<Vec<_>>().into())
    }

    #[test]
    fn test_numbers() {
        assert_eq!(toks("0 42 3.5 .5 1e3 1.5e-2 0x1F 0o17 0b101 1_000 010 019"), vec![
            Tok::Num(0.0), Tok::Num(42.0), Tok::Num(3.5), Tok::Num(0.5), Tok::Num(1000.0),
            Tok::Num(0.015), Tok::Num(31.0), Tok::Num(15.0), Tok::Num(5.0), Tok::Num(1000.0),
            Tok::Num(8.0), Tok::Num(19.0),
        ]);
        assert_eq!(toks("10n 0xffn"), vec![Tok::BigInt("10".into()), Tok::BigInt("255".into())]);
        assert!(Lexer::new("3in").next_token().is_err());
        assert!(Lexer::new("1__0").next_token().is_err());
    }

    #[test]
    fn test_unicode_identifiers() {
        // Letters, Other_ID_Start symbols (as minifiers emit them), marks
        // after the start
        assert_eq!(toks("π ℘ ℮x a\u{301} ゛"), vec![
            Tok::Ident("π".into()), Tok::Ident("℘".into()), Tok::Ident("℮x".into()),
            Tok::Ident("a\u{301}".into()), Tok::Ident("゛".into()),
        ]);
        assert!(Lexer::new("€").next_token().is_err());
    }

    #[test]
    fn test_strings_and_escapes() {
        assert_eq!(toks(r#"'a\nb' "q\"x" '\x41B\u{43}' '\101'"#), vec![
            s("a\nb"), s("q\"x"), s("ABC"), s("A"),
        ]);
        // Astral code points become surrogate pairs; lone surrogates survive
        assert_eq!(toks(r"'\u{1F600}' '\uD800'"), vec![
            Tok::Str(vec![0xD83D, 0xDE00].into()),
            Tok::Str(vec![0xD800].into()),
        ]);
        assert_eq!(toks("'a\\\nb'"), vec![s("ab")]);
        assert!(Lexer::new("'abc").next_token().is_err());
    }

    #[test]
    fn test_punctuators_and_keywords() {
        assert_eq!(toks("a ?. b ?? c ??= d >>>= e ** f => ... x?.5:1"), vec![
            Tok::Ident("a".into()), Tok::Punct(P::QuestionDot), Tok::Ident("b".into()),
            Tok::Punct(P::QuestionQuestion), Tok::Ident("c".into()), Tok::Punct(P::QuestionQuestionEq),
            Tok::Ident("d".into()), Tok::Punct(P::ShrEq), Tok::Ident("e".into()), Tok::Punct(P::StarStar),
            Tok::Ident("f".into()), Tok::Punct(P::Arrow), Tok::Punct(P::Ellipsis), Tok::Ident("x".into()),
            Tok::Punct(P::Question), Tok::Num(0.5), Tok::Punct(P::Colon), Tok::Num(1.0),
        ]);
        assert_eq!(toks("if let \\u0061bc"), vec![
            Tok::Keyword(Kw::If), Tok::Keyword(Kw::Let), Tok::Ident("abc".into()),
        ]);
    }

    #[test]
    fn test_comments_and_newlines() {
        let mut lexer = Lexer::new("a /* x */ b // y\n c /* \n */ d <!-- html\n-->also\ne");
        let a = lexer.next_token().unwrap();
        let b = lexer.next_token().unwrap();
        let c = lexer.next_token().unwrap();
        let d = lexer.next_token().unwrap();
        let e = lexer.next_token().unwrap();
        assert!(!a.newline_before && !b.newline_before);
        assert!(c.newline_before && d.newline_before);
        assert_eq!(e.tok, Tok::Ident("e".into()));
        assert!(e.newline_before);
    }

    #[test]
    fn test_regex_and_template_rescans() {
        let mut lexer = Lexer::new("/a[/]b\\//gi");
        let slash = lexer.next_token().unwrap();
        let re = lexer.rescan_regex(&slash).unwrap();
        assert_eq!(re.tok, Tok::Regex { pattern: "a[/]b\\/".into(), flags: "gi".into() });

        let mut lexer = Lexer::new("`a${x}b\\n${y}`");
        let head = lexer.next_token().unwrap();
        assert!(matches!(&head.tok, Tok::Template { tail: false, raw, .. } if &**raw == "a"));
        assert_eq!(lexer.next_token().unwrap().tok, Tok::Ident("x".into()));
        let close = lexer.next_token().unwrap();
        let middle = lexer.rescan_template_continuation(&close).unwrap();
        match &middle.tok {
            Tok::Template { cooked, raw, tail } => {
                assert!(!tail);
                assert_eq!(&**raw, "b\\n");
                assert_eq!(cooked.as_deref(), Some(&[b'b' as u16, 10][..]));
            }
            other => panic!("{other:?}"),
        }
        lexer.next_token().unwrap();
        let close = lexer.next_token().unwrap();
        let tail = lexer.rescan_template_continuation(&close).unwrap();
        assert!(matches!(tail.tok, Tok::Template { tail: true, .. }));
    }
}
