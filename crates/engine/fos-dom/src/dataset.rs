//! DOMStringMap (dataset)
//!
//! Implements dataset for data-* attribute access.

use std::collections::HashMap;

/// DOMStringMap for data-* attributes
#[derive(Debug, Clone, Default)]
pub struct DOMStringMap {
    data: HashMap<String, String>,
}

impl DOMStringMap {
    /// Create empty string map
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create from data-* attributes
    pub fn from_attributes(attrs: &[(String, String)]) -> Self {
        let mut data = HashMap::new();
        for (name, value) in attrs {
            if let Some(key) = name.strip_prefix("data-") {
                let camel = to_camel_case(key);
                data.insert(camel, value.clone());
            }
        }
        Self { data }
    }
    
    /// Get value by camelCase key
    pub fn get(&self, key: &str) -> Option<&str> {
        self.data.get(key).map(|s| s.as_str())
    }
    
    /// Set value by camelCase key
    pub fn set(&mut self, key: &str, value: &str) {
        self.data.insert(key.to_string(), value.to_string());
    }
    
    /// Delete by key
    pub fn delete(&mut self, key: &str) -> bool {
        self.data.remove(key).is_some()
    }
    
    /// Check if key exists
    pub fn has(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }
    
    /// Get all keys
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.data.keys().map(|s| s.as_str())
    }
    
    /// Get all values
    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.data.values().map(|s| s.as_str())
    }
    
    /// Convert key to attribute name
    pub fn to_attribute_name(key: &str) -> String {
        format!("data-{}", to_kebab_case(key))
    }
    
    /// Get as attribute pairs
    pub fn to_attributes(&self) -> Vec<(String, String)> {
        self.data.iter()
            .map(|(k, v)| (Self::to_attribute_name(k), v.clone()))
            .collect()
    }
}

/// Convert kebab-case to camelCase
fn to_camel_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;
    
    for c in s.chars() {
        if c == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }
    
    result
}

/// Convert camelCase to kebab-case
fn to_kebab_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    
    for c in s.chars() {
        if c.is_ascii_uppercase() {
            result.push('-');
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_from_attributes() {
        let attrs = vec![
            ("data-user-id".to_string(), "123".to_string()),
            ("data-active".to_string(), "true".to_string()),
            ("class".to_string(), "ignored".to_string()),
        ];
        
        let map = DOMStringMap::from_attributes(&attrs);
        
        assert_eq!(map.get("userId"), Some("123"));
        assert_eq!(map.get("active"), Some("true"));
        assert!(!map.has("class"));
    }
    
    #[test]
    fn test_camel_case() {
        assert_eq!(to_camel_case("user-id"), "userId");
        assert_eq!(to_camel_case("first-name"), "firstName");
        assert_eq!(to_camel_case("simple"), "simple");
    }
    
    #[test]
    fn test_kebab_case() {
        assert_eq!(to_kebab_case("userId"), "user-id");
        assert_eq!(to_kebab_case("firstName"), "first-name");
    }
    
    #[test]
    fn test_set_get() {
        let mut map = DOMStringMap::new();
        map.set("userName", "John");
        
        assert_eq!(map.get("userName"), Some("John"));
        assert_eq!(DOMStringMap::to_attribute_name("userName"), "data-user-name");
    }
}
