//! Web API Integration
//!
//! Integrates fos-js web APIs: URL, Blob, File, FormData.

use fos_js::{
    JsUrl, JsUrlSearchParams, 
    Blob, FileReader,
    FormData,
};

/// Web API manager for the browser
pub struct WebApiManager {
    /// Created blobs by ID
    blobs: Vec<Blob>,
    /// Next blob ID
    next_blob_id: u32,
}

impl WebApiManager {
    /// Create new Web API manager
    pub fn new() -> Self {
        Self {
            blobs: Vec::new(),
            next_blob_id: 0,
        }
    }
    
    // === URL API ===
    
    /// Parse a URL
    pub fn parse_url(url: &str) -> Option<JsUrl> {
        JsUrl::parse(url)
    }
    
    /// Create URL search params
    pub fn url_search_params(search: &str) -> JsUrlSearchParams {
        JsUrlSearchParams::parse(search)
    }
    
    // === Blob API ===
    
    /// Store a blob and return its ID
    pub fn store_blob(&mut self, blob: Blob) -> u32 {
        let id = self.next_blob_id;
        self.next_blob_id += 1;
        self.blobs.push(blob);
        id
    }
    
    /// Get a blob by ID
    pub fn get_blob(&self, id: u32) -> Option<&Blob> {
        self.blobs.get(id as usize)
    }
    
    /// Create a blob URL
    pub fn create_object_url(&mut self, blob: Blob) -> String {
        let id = self.store_blob(blob);
        format!("blob:{}", id)
    }
    
    /// Revoke a blob URL
    pub fn revoke_object_url(&mut self, url: &str) {
        if let Some(id_str) = url.strip_prefix("blob:") {
            if let Ok(id) = id_str.parse::<usize>() {
                if id < self.blobs.len() {
                    // Mark as revoked (keep in place to preserve indices)
                }
            }
        }
    }
    
    // === FileReader ===
    
    /// Create a file reader
    pub fn create_file_reader() -> FileReader {
        FileReader::new()
    }
    
    // === FormData ===
    
    /// Create form data
    pub fn create_form_data() -> FormData {
        FormData::new()
    }
    
    // === Text Encoding (manual) ===
    
    /// Encode text to UTF-8 bytes
    pub fn encode_text(text: &str) -> Vec<u8> {
        text.as_bytes().to_vec()
    }
    
    /// Decode UTF-8 bytes to text
    pub fn decode_text(bytes: &[u8]) -> String {
        String::from_utf8_lossy(bytes).to_string()
    }
    
    /// Get statistics
    pub fn stats(&self) -> WebApiStats {
        WebApiStats {
            blob_count: self.blobs.len(),
        }
    }
}

impl Default for WebApiManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Web API statistics
#[derive(Debug, Clone)]
pub struct WebApiStats {
    pub blob_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_url_parsing() {
        let url = WebApiManager::parse_url("https://example.com/path?q=1").unwrap();
        assert_eq!(url.hostname, "example.com");
        assert_eq!(url.pathname, "/path");
    }
    
    #[test]
    fn test_search_params() {
        let params = WebApiManager::url_search_params("?foo=bar&baz=qux");
        assert_eq!(params.get("foo"), Some("bar"));
    }
    
    #[test]
    fn test_text_encoding() {
        let text = "Hello, World!";
        let encoded = WebApiManager::encode_text(text);
        let decoded = WebApiManager::decode_text(&encoded);
        assert_eq!(decoded, text);
    }
    
    #[test]
    fn test_blob_url() {
        use fos_js::webapi::blob::{BlobPart, BlobOptions};
        
        let mut manager = WebApiManager::new();
        let blob = Blob::new(
            vec![BlobPart::String("test".to_string())],
            BlobOptions::default()
        );
        
        let url = manager.create_object_url(blob);
        assert!(url.starts_with("blob:"));
    }
}
