//! FormData API
//!
//! Implementation of JavaScript FormData for form submission.

use std::collections::HashMap;

/// FormData entry value
#[derive(Debug, Clone)]
pub enum FormDataValue {
    /// String value
    String(String),
    /// File value
    File(FileEntry),
}

/// File entry in FormData
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// File name
    pub name: String,
    /// MIME type
    pub mime_type: String,
    /// File content
    pub content: Vec<u8>,
    /// Last modified timestamp
    pub last_modified: u64,
}

/// FormData object
#[derive(Debug, Clone, Default)]
pub struct FormData {
    /// Entries (supports multiple values per key)
    entries: Vec<(String, FormDataValue)>,
}

impl FormData {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create FormData from form element data
    pub fn from_entries(entries: Vec<(String, FormDataValue)>) -> Self {
        Self { entries }
    }
    
    /// Append a string value
    pub fn append(&mut self, name: &str, value: &str) {
        self.entries.push((name.to_string(), FormDataValue::String(value.to_string())));
    }
    
    /// Append a file
    pub fn append_file(&mut self, name: &str, file: FileEntry) {
        self.entries.push((name.to_string(), FormDataValue::File(file)));
    }
    
    /// Append a file with custom filename
    pub fn append_file_with_name(&mut self, name: &str, mut file: FileEntry, filename: &str) {
        file.name = filename.to_string();
        self.entries.push((name.to_string(), FormDataValue::File(file)));
    }
    
    /// Delete all entries with name
    pub fn delete(&mut self, name: &str) {
        self.entries.retain(|(k, _)| k != name);
    }
    
    /// Get first value for name
    pub fn get(&self, name: &str) -> Option<&FormDataValue> {
        self.entries.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v)
    }
    
    /// Get all values for name
    pub fn get_all(&self, name: &str) -> Vec<&FormDataValue> {
        self.entries.iter()
            .filter(|(k, _)| k == name)
            .map(|(_, v)| v)
            .collect()
    }
    
    /// Check if key exists
    pub fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == name)
    }
    
    /// Set value (replace existing)
    pub fn set(&mut self, name: &str, value: &str) {
        self.delete(name);
        self.append(name, value);
    }
    
    /// Set file (replace existing)
    pub fn set_file(&mut self, name: &str, file: FileEntry) {
        self.delete(name);
        self.append_file(name, file);
    }
    
    /// Get all keys
    pub fn keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.entries.iter().map(|(k, _)| k.as_str()).collect();
        keys.dedup();
        keys
    }
    
    /// Get all values
    pub fn values(&self) -> impl Iterator<Item = &FormDataValue> {
        self.entries.iter().map(|(_, v)| v)
    }
    
    /// Get all entries
    pub fn entries(&self) -> impl Iterator<Item = (&str, &FormDataValue)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }
    
    /// Number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    
    /// Encode as multipart/form-data
    pub fn to_multipart(&self, boundary: &str) -> Vec<u8> {
        let mut result = Vec::new();
        
        for (name, value) in &self.entries {
            result.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
            
            match value {
                FormDataValue::String(s) => {
                    result.extend_from_slice(
                        format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes()
                    );
                    result.extend_from_slice(s.as_bytes());
                }
                FormDataValue::File(file) => {
                    result.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\n",
                            name, file.name
                        ).as_bytes()
                    );
                    result.extend_from_slice(
                        format!("Content-Type: {}\r\n\r\n", file.mime_type).as_bytes()
                    );
                    result.extend_from_slice(&file.content);
                }
            }
            result.extend_from_slice(b"\r\n");
        }
        
        result.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());
        result
    }
    
    /// Encode as application/x-www-form-urlencoded
    pub fn to_urlencoded(&self) -> String {
        self.entries.iter()
            .filter_map(|(k, v)| {
                match v {
                    FormDataValue::String(s) => {
                        Some(format!("{}={}", 
                            urlencoded_escape(k),
                            urlencoded_escape(s)
                        ))
                    }
                    FormDataValue::File(_) => None, // Files can't be urlencoded
                }
            })
            .collect::<Vec<_>>()
            .join("&")
    }
}

/// URL encode a string
fn urlencoded_escape(s: &str) -> String {
    let mut result = String::new();
    for c in s.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => {
                result.push(c);
            }
            ' ' => result.push('+'),
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_formdata_append() {
        let mut fd = FormData::new();
        fd.append("name", "John");
        fd.append("age", "30");
        
        assert_eq!(fd.len(), 2);
        assert!(fd.has("name"));
    }
    
    #[test]
    fn test_formdata_get() {
        let mut fd = FormData::new();
        fd.append("name", "John");
        
        match fd.get("name") {
            Some(FormDataValue::String(s)) => assert_eq!(s, "John"),
            _ => panic!("Expected string"),
        }
    }
    
    #[test]
    fn test_formdata_multiple_values() {
        let mut fd = FormData::new();
        fd.append("color", "red");
        fd.append("color", "blue");
        
        let values = fd.get_all("color");
        assert_eq!(values.len(), 2);
    }
    
    #[test]
    fn test_formdata_delete() {
        let mut fd = FormData::new();
        fd.append("name", "John");
        fd.delete("name");
        
        assert!(!fd.has("name"));
    }
    
    #[test]
    fn test_urlencoded() {
        let mut fd = FormData::new();
        fd.append("name", "John Doe");
        fd.append("age", "30");
        
        let encoded = fd.to_urlencoded();
        assert!(encoded.contains("name=John+Doe"));
        assert!(encoded.contains("age=30"));
    }
}
