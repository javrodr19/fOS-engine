//! Regular Expression Support
//!
//! JavaScript RegExp implementation for pattern matching.

use std::collections::HashMap;

/// JavaScript RegExp object
#[derive(Debug, Clone)]
pub struct JsRegex {
    pattern: Box<str>,
    flags: RegexFlags,
    last_index: usize,
}

/// Regex flags
#[derive(Debug, Clone, Copy, Default)]
pub struct RegexFlags {
    pub global: bool,      // g
    pub ignore_case: bool, // i
    pub multiline: bool,   // m
    pub dot_all: bool,     // s
    pub unicode: bool,     // u
    pub sticky: bool,      // y
}

impl RegexFlags {
    pub fn from_str(s: &str) -> Self {
        let mut flags = Self::default();
        for c in s.chars() {
            match c {
                'g' => flags.global = true,
                'i' => flags.ignore_case = true,
                'm' => flags.multiline = true,
                's' => flags.dot_all = true,
                'u' => flags.unicode = true,
                'y' => flags.sticky = true,
                _ => {}
            }
        }
        flags
    }
}

impl Default for JsRegex {
    fn default() -> Self { Self::new("", "") }
}

impl JsRegex {
    pub fn new(pattern: &str, flags: &str) -> Self {
        Self {
            pattern: pattern.into(),
            flags: RegexFlags::from_str(flags),
            last_index: 0,
        }
    }
    
    pub fn pattern(&self) -> &str { &self.pattern }
    pub fn flags(&self) -> &RegexFlags { &self.flags }
    pub fn last_index(&self) -> usize { self.last_index }
    pub fn set_last_index(&mut self, idx: usize) { self.last_index = idx; }
    
    /// Test if pattern matches string
    pub fn test(&mut self, input: &str) -> bool {
        if let Some(m) = self.simple_match(input, self.last_index) {
            if self.flags.global {
                self.last_index = m.end;
            }
            true
        } else {
            if self.flags.global {
                self.last_index = 0;
            }
            false
        }
    }
    
    /// Execute regex and return match
    pub fn exec(&mut self, input: &str) -> Option<RegexMatch> {
        if let Some(m) = self.simple_match(input, self.last_index) {
            if self.flags.global {
                self.last_index = m.end;
            }
            Some(m)
        } else {
            if self.flags.global {
                self.last_index = 0;
            }
            None
        }
    }
    
    /// Simple pattern matching (literal match only for now)
    fn simple_match(&self, input: &str, start: usize) -> Option<RegexMatch> {
        let search_str = if start < input.len() {
            &input[start..]
        } else {
            return None;
        };
        
        // Simple literal matching
        if self.flags.ignore_case {
            let lower_pattern = self.pattern.to_lowercase();
            let lower_input = search_str.to_lowercase();
            if let Some(pos) = lower_input.find(&lower_pattern) {
                return Some(RegexMatch {
                    start: start + pos,
                    end: start + pos + self.pattern.len(),
                    value: search_str[pos..pos + self.pattern.len()].into(),
                    groups: Vec::new(),
                });
            }
        } else if let Some(pos) = search_str.find(&*self.pattern) {
            return Some(RegexMatch {
                start: start + pos,
                end: start + pos + self.pattern.len(),
                value: self.pattern.clone(),
                groups: Vec::new(),
            });
        }
        
        None
    }
}

/// Regex match result
#[derive(Debug, Clone)]
pub struct RegexMatch {
    pub start: usize,
    pub end: usize,
    pub value: Box<str>,
    pub groups: Vec<Option<Box<str>>>,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_flags() {
        let flags = RegexFlags::from_str("gi");
        assert!(flags.global);
        assert!(flags.ignore_case);
        assert!(!flags.multiline);
    }
    
    #[test]
    fn test_literal_match() {
        let mut regex = JsRegex::new("hello", "");
        assert!(regex.test("say hello world"));
        assert!(!regex.test("goodbye"));
    }
    
    #[test]
    fn test_case_insensitive() {
        let mut regex = JsRegex::new("hello", "i");
        assert!(regex.test("HELLO WORLD"));
    }
    
    #[test]
    fn test_global_last_index() {
        let mut regex = JsRegex::new("a", "g");
        assert!(regex.test("abab"));
        assert_eq!(regex.last_index(), 1);
        assert!(regex.test("abab"));
        assert_eq!(regex.last_index(), 3);
    }
    
    #[test]
    fn test_exec() {
        let mut regex = JsRegex::new("test", "");
        let m = regex.exec("this is a test string").unwrap();
        assert_eq!(&*m.value, "test");
        assert_eq!(m.start, 10);
    }
}
