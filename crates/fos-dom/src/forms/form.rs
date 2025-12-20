//! Form Element Implementation
//!
//! Form container and submission handling.

use std::collections::HashMap;

/// Form element
#[derive(Debug, Clone, Default)]
pub struct FormElement {
    pub name: Option<String>,
    pub id: Option<String>,
    pub action: String,
    pub method: FormMethod,
    pub enctype: FormEnctype,
    pub target: String,
    pub autocomplete: AutocompleteMode,
    pub novalidate: bool,
    
    // Elements (by ID)
    element_ids: Vec<String>,
}

/// Form submission method
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FormMethod {
    #[default]
    Get,
    Post,
    Dialog,
}

impl FormMethod {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "post" => Self::Post,
            "dialog" => Self::Dialog,
            _ => Self::Get,
        }
    }
}

/// Form encoding type
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FormEnctype {
    #[default]
    UrlEncoded,
    Multipart,
    TextPlain,
}

impl FormEnctype {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "multipart/form-data" => Self::Multipart,
            "text/plain" => Self::TextPlain,
            _ => Self::UrlEncoded,
        }
    }
    
    pub fn content_type(&self) -> &'static str {
        match self {
            Self::UrlEncoded => "application/x-www-form-urlencoded",
            Self::Multipart => "multipart/form-data",
            Self::TextPlain => "text/plain",
        }
    }
}

/// Autocomplete mode
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AutocompleteMode {
    #[default]
    On,
    Off,
}

/// Form data for submission
#[derive(Debug, Clone, Default)]
pub struct FormData {
    entries: Vec<(String, FormDataValue)>,
}

/// Form data value
#[derive(Debug, Clone)]
pub enum FormDataValue {
    Text(String),
    File { name: String, content: Vec<u8>, mime_type: String },
}

impl FormData {
    /// Create empty form data
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Append a text value
    pub fn append(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.entries.push((name.into(), FormDataValue::Text(value.into())));
    }
    
    /// Append a file
    pub fn append_file(&mut self, name: impl Into<String>, filename: impl Into<String>, 
                       content: Vec<u8>, mime_type: impl Into<String>) {
        self.entries.push((name.into(), FormDataValue::File {
            name: filename.into(),
            content,
            mime_type: mime_type.into(),
        }));
    }
    
    /// Get a value by name
    pub fn get(&self, name: &str) -> Option<&str> {
        for (n, v) in &self.entries {
            if n == name {
                if let FormDataValue::Text(s) = v {
                    return Some(s);
                }
            }
        }
        None
    }
    
    /// Get all values by name
    pub fn get_all(&self, name: &str) -> Vec<&str> {
        self.entries.iter()
            .filter(|(n, _)| n == name)
            .filter_map(|(_, v)| {
                if let FormDataValue::Text(s) = v {
                    Some(s.as_str())
                } else {
                    None
                }
            })
            .collect()
    }
    
    /// Check if key exists
    pub fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|(n, _)| n == name)
    }
    
    /// Delete all entries with name
    pub fn delete(&mut self, name: &str) {
        self.entries.retain(|(n, _)| n != name);
    }
    
    /// Iterate over entries
    pub fn entries(&self) -> impl Iterator<Item = (&str, &FormDataValue)> {
        self.entries.iter().map(|(n, v)| (n.as_str(), v))
    }
    
    /// Convert to URL-encoded string
    pub fn to_url_encoded(&self) -> String {
        self.entries.iter()
            .filter_map(|(name, value)| {
                if let FormDataValue::Text(v) = value {
                    Some(format!("{}={}", 
                        urlencoding_encode(name), 
                        urlencoding_encode(v)))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("&")
    }
}

fn urlencoding_encode(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                result.push(c);
            }
            ' ' => result.push('+'),
            _ => {
                for byte in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", byte));
                }
            }
        }
    }
    result
}

impl FormElement {
    /// Create a new form
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set action URL
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = action.into();
        self
    }
    
    /// Set method
    pub fn with_method(mut self, method: FormMethod) -> Self {
        self.method = method;
        self
    }
    
    /// Register an element ID
    pub fn register_element(&mut self, id: String) {
        if !self.element_ids.contains(&id) {
            self.element_ids.push(id);
        }
    }
    
    /// Get all element IDs
    pub fn element_ids(&self) -> &[String] {
        &self.element_ids
    }
    
    /// Get length (number of elements)
    pub fn length(&self) -> usize {
        self.element_ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_form_data() {
        let mut data = FormData::new();
        data.append("name", "John");
        data.append("email", "john@example.com");
        
        assert_eq!(data.get("name"), Some("John"));
        assert!(data.has("email"));
    }
    
    #[test]
    fn test_url_encoding() {
        let mut data = FormData::new();
        data.append("q", "hello world");
        
        assert_eq!(data.to_url_encoded(), "q=hello+world");
    }
}
