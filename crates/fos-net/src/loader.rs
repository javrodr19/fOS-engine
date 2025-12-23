//! Resource Loader
//!
//! HTTP client for network requests using custom HTTP client.

use crate::{Response, NetError};
use crate::client::{HttpClient, HttpClientBuilder};
use std::collections::HashMap;

/// HTTP method
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
    Head,
    Options,
    Patch,
}

impl Default for Method {
    fn default() -> Self {
        Method::Get
    }
}

impl Method {
    fn as_str(&self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
            Method::Patch => "PATCH",
        }
    }
}

/// Request configuration
#[derive(Debug, Default)]
pub struct Request {
    pub method: Method,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
}

impl Request {
    pub fn get(url: &str) -> Self {
        Self {
            method: Method::Get,
            url: url.to_string(),
            ..Default::default()
        }
    }
    
    pub fn post(url: &str) -> Self {
        Self {
            method: Method::Post,
            url: url.to_string(),
            ..Default::default()
        }
    }
    
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }
    
    pub fn with_body(mut self, body: Vec<u8>) -> Self {
        self.body = Some(body);
        self
    }
    
    pub fn with_json(self, json: &str) -> Self {
        self.with_header("Content-Type", "application/json")
            .with_body(json.as_bytes().to_vec())
    }
}

/// Load resources from network  
pub struct ResourceLoader {
    client: HttpClient,
}

impl ResourceLoader {
    pub fn new() -> Self {
        let client = HttpClient::builder()
            .user_agent("fOS-Engine/0.1")
            .build();
        
        Self { client }
    }
    
    /// Fetch a URL with GET
    pub async fn fetch(&mut self, url: &str) -> Result<Response, NetError> {
        self.request(Request::get(url)).await
    }
    
    /// Make an HTTP request
    pub async fn request(&mut self, req: Request) -> Result<Response, NetError> {
        tracing::info!("HTTP {:?} {}", req.method, req.url);
        
        let headers: Vec<(String, String)> = req.headers.into_iter().collect();
        
        self.client.request(
            req.method.as_str(),
            &req.url,
            Some(headers),
            req.body,
        )
    }
}

impl Default for ResourceLoader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_request_builder() {
        let req = Request::get("https://example.com")
            .with_header("Accept", "application/json");
        
        assert_eq!(req.method, Method::Get);
        assert_eq!(req.headers.get("Accept").unwrap(), "application/json");
    }
    
    #[test]
    fn test_post_request() {
        let req = Request::post("https://api.example.com")
            .with_json(r#"{"key": "value"}"#);
        
        assert_eq!(req.method, Method::Post);
        assert!(req.body.is_some());
        assert_eq!(req.headers.get("Content-Type").unwrap(), "application/json");
    }
    
    #[test]
    fn test_method_as_str() {
        assert_eq!(Method::Get.as_str(), "GET");
        assert_eq!(Method::Post.as_str(), "POST");
        assert_eq!(Method::Delete.as_str(), "DELETE");
    }
}
