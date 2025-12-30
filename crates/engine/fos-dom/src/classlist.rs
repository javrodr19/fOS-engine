//! DOMTokenList (classList)
//!
//! Implements classList for CSS class manipulation.

/// DOMTokenList for managing space-separated tokens (e.g., classList)
#[derive(Debug, Clone, Default)]
pub struct DOMTokenList {
    tokens: Vec<String>,
}

impl DOMTokenList {
    /// Create empty token list
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Parse from space-separated string
    pub fn from_string(s: &str) -> Self {
        let tokens = s.split_whitespace()
            .map(|s| s.to_string())
            .collect();
        Self { tokens }
    }
    
    /// Get number of tokens
    pub fn length(&self) -> usize {
        self.tokens.len()
    }
    
    /// Get token at index
    pub fn item(&self, index: usize) -> Option<&str> {
        self.tokens.get(index).map(|s| s.as_str())
    }
    
    /// Check if token exists
    pub fn contains(&self, token: &str) -> bool {
        self.tokens.iter().any(|t| t == token)
    }
    
    /// Add token(s)
    pub fn add(&mut self, tokens: &[&str]) {
        for token in tokens {
            if !token.is_empty() && !self.contains(token) {
                self.tokens.push(token.to_string());
            }
        }
    }
    
    /// Remove token(s)
    pub fn remove(&mut self, tokens: &[&str]) {
        self.tokens.retain(|t| !tokens.contains(&t.as_str()));
    }
    
    /// Toggle token, returns new state
    pub fn toggle(&mut self, token: &str, force: Option<bool>) -> bool {
        match force {
            Some(true) => {
                if !self.contains(token) {
                    self.add(&[token]);
                }
                true
            }
            Some(false) => {
                self.remove(&[token]);
                false
            }
            None => {
                if self.contains(token) {
                    self.remove(&[token]);
                    false
                } else {
                    self.add(&[token]);
                    true
                }
            }
        }
    }
    
    /// Replace token
    pub fn replace(&mut self, old_token: &str, new_token: &str) -> bool {
        if let Some(pos) = self.tokens.iter().position(|t| t == old_token) {
            self.tokens[pos] = new_token.to_string();
            true
        } else {
            false
        }
    }
    
    /// Check if token is valid
    pub fn supports(&self, _token: &str) -> bool {
        true // All tokens supported
    }
    
    /// Get value as string
    pub fn value(&self) -> String {
        self.tokens.join(" ")
    }
    
    /// Set from string
    pub fn set_value(&mut self, value: &str) {
        *self = Self::from_string(value);
    }
    
    /// Iterate over tokens
    pub fn iter(&self) -> impl Iterator<Item = &str> {
        self.tokens.iter().map(|s| s.as_str())
    }
}

impl std::fmt::Display for DOMTokenList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.value())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_from_string() {
        let list = DOMTokenList::from_string("btn btn-primary active");
        assert_eq!(list.length(), 3);
        assert!(list.contains("btn"));
        assert!(list.contains("btn-primary"));
    }
    
    #[test]
    fn test_add_remove() {
        let mut list = DOMTokenList::new();
        list.add(&["foo", "bar"]);
        assert_eq!(list.length(), 2);
        
        list.remove(&["foo"]);
        assert_eq!(list.length(), 1);
        assert!(!list.contains("foo"));
    }
    
    #[test]
    fn test_toggle() {
        let mut list = DOMTokenList::new();
        
        assert!(list.toggle("active", None));
        assert!(list.contains("active"));
        
        assert!(!list.toggle("active", None));
        assert!(!list.contains("active"));
    }
    
    #[test]
    fn test_replace() {
        let mut list = DOMTokenList::from_string("old-class");
        
        assert!(list.replace("old-class", "new-class"));
        assert!(!list.contains("old-class"));
        assert!(list.contains("new-class"));
    }
}
