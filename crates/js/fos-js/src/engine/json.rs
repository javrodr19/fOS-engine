//! JSON Implementation
//!
//! JSON.parse and JSON.stringify for JavaScript.

use super::value::{JsVal, JsValKind};
use super::object::{JsObject, JsArray};

/// JSON parser
pub struct JsonParser<'a> {
    input: &'a str,
    pos: usize,
}

impl<'a> JsonParser<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0 }
    }
    
    pub fn parse(&mut self) -> Result<JsVal, String> {
        self.skip_whitespace();
        let result = self.parse_value()?;
        self.skip_whitespace();
        if self.pos < self.input.len() {
            return Err("Unexpected characters after JSON".into());
        }
        Ok(result)
    }
    
    fn parse_value(&mut self) -> Result<JsVal, String> {
        self.skip_whitespace();
        match self.peek() {
            Some('"') => self.parse_string(),
            Some('0'..='9') | Some('-') => self.parse_number(),
            Some('t') => self.parse_true(),
            Some('f') => self.parse_false(),
            Some('n') => self.parse_null(),
            Some('[') => self.parse_array(),
            Some('{') => self.parse_object(),
            Some(c) => Err(format!("Unexpected character: {}", c)),
            None => Err("Unexpected end of input".into()),
        }
    }
    
    fn parse_string(&mut self) -> Result<JsVal, String> {
        self.expect('"')?;
        let start = self.pos;
        let mut result = String::new();
        
        while let Some(c) = self.peek() {
            if c == '"' {
                self.advance();
                return Ok(JsVal::String(result.into()));
            } else if c == '\\' {
                self.advance();
                match self.peek() {
                    Some('n') => { result.push('\n'); self.advance(); }
                    Some('r') => { result.push('\r'); self.advance(); }
                    Some('t') => { result.push('\t'); self.advance(); }
                    Some('"') => { result.push('"'); self.advance(); }
                    Some('\\') => { result.push('\\'); self.advance(); }
                    Some('/') => { result.push('/'); self.advance(); }
                    _ => return Err("Invalid escape sequence".into()),
                }
            } else {
                result.push(c);
                self.advance();
            }
        }
        Err("Unterminated string".into())
    }
    
    fn parse_number(&mut self) -> Result<JsVal, String> {
        let start = self.pos;
        if self.peek() == Some('-') { self.advance(); }
        while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) { self.advance(); }
        if self.peek() == Some('.') {
            self.advance();
            while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) { self.advance(); }
        }
        if self.peek() == Some('e') || self.peek() == Some('E') {
            self.advance();
            if self.peek() == Some('+') || self.peek() == Some('-') { self.advance(); }
            while self.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) { self.advance(); }
        }
        let num_str = &self.input[start..self.pos];
        num_str.parse::<f64>().map(JsVal::Number).map_err(|_| "Invalid number".into())
    }
    
    fn parse_true(&mut self) -> Result<JsVal, String> {
        self.expect_str("true")?;
        Ok(JsVal::Bool(true))
    }
    
    fn parse_false(&mut self) -> Result<JsVal, String> {
        self.expect_str("false")?;
        Ok(JsVal::Bool(false))
    }
    
    fn parse_null(&mut self) -> Result<JsVal, String> {
        self.expect_str("null")?;
        Ok(JsVal::Null)
    }
    
    fn parse_array(&mut self) -> Result<JsVal, String> {
        self.expect('[')?;
        let mut elements = Vec::new();
        self.skip_whitespace();
        
        if self.peek() == Some(']') {
            self.advance();
            return Ok(JsVal::Array(0)); // Placeholder - need array registry
        }
        
        loop {
            elements.push(self.parse_value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => { self.advance(); self.skip_whitespace(); }
                Some(']') => { self.advance(); break; }
                _ => return Err("Expected ',' or ']'".into()),
            }
        }
        
        Ok(JsVal::Array(0)) // Placeholder
    }
    
    fn parse_object(&mut self) -> Result<JsVal, String> {
        self.expect('{')?;
        self.skip_whitespace();
        
        if self.peek() == Some('}') {
            self.advance();
            return Ok(JsVal::Object(0)); // Placeholder
        }
        
        loop {
            self.skip_whitespace();
            let key = match self.parse_string()?.as_string() {
                Some(s) => s,
                None => return Err("Expected string key".into()),
            };
            self.skip_whitespace();
            self.expect(':')?;
            let value = self.parse_value()?;
            self.skip_whitespace();
            
            match self.peek() {
                Some(',') => { self.advance(); }
                Some('}') => { self.advance(); break; }
                _ => return Err("Expected ',' or '}'".into()),
            }
        }
        
        Ok(JsVal::Object(0)) // Placeholder
    }
    
    fn peek(&self) -> Option<char> { self.input[self.pos..].chars().next() }
    fn advance(&mut self) { if let Some(c) = self.peek() { self.pos += c.len_utf8(); } }
    fn skip_whitespace(&mut self) { while self.peek().map(|c| c.is_whitespace()).unwrap_or(false) { self.advance(); } }
    fn expect(&mut self, c: char) -> Result<(), String> {
        if self.peek() == Some(c) { self.advance(); Ok(()) }
        else { Err(format!("Expected '{}'", c)) }
    }
    fn expect_str(&mut self, s: &str) -> Result<(), String> {
        for c in s.chars() { self.expect(c)?; }
        Ok(())
    }
}

/// JSON stringifier
pub fn stringify(value: &JsVal) -> String {
    use JsValKind::*;
    match value.kind() {
        Undefined => "undefined".to_string(),
        Null => "null".to_string(),
        Bool(b) => b.to_string(),
        Number(n) => {
            if n.is_nan() { "null".to_string() }
            else if n.is_infinite() { "null".to_string() }
            else { format!("{}", n) }
        }
        String(s) => format!("\"{}\"", escape_string(&s)),
        Object(_) => "{}".to_string(),
        Array(_) => "[]".to_string(),
        Function(_) => "undefined".to_string(),
    }
}

fn escape_string(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c => result.push(c),
        }
    }
    result
}

/// Parse JSON string to JsVal
pub fn parse(input: &str) -> Result<JsVal, String> {
    JsonParser::new(input).parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_null() {
        assert_eq!(parse("null").unwrap(), JsVal::Null);
    }
    
    #[test]
    fn test_parse_bool() {
        assert_eq!(parse("true").unwrap(), JsVal::Bool(true));
        assert_eq!(parse("false").unwrap(), JsVal::Bool(false));
    }
    
    #[test]
    fn test_parse_number() {
        assert_eq!(parse("42").unwrap(), JsVal::Number(42.0));
        assert_eq!(parse("-3.14").unwrap(), JsVal::Number(-3.14));
    }
    
    #[test]
    fn test_parse_string() {
        assert_eq!(parse("\"hello\"").unwrap(), JsVal::String("hello".into()));
    }
    
    #[test]
    fn test_stringify() {
        assert_eq!(stringify(&JsVal::Null), "null");
        assert_eq!(stringify(&JsVal::Number(42.0)), "42");
        assert_eq!(stringify(&JsVal::String("hello".into())), "\"hello\"");
    }
}
