//! Resource Loader
//!
//! HTTP client using reqwest for network requests.

use crate::{Response, NetError};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;
use std::str::FromStr;

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
    client: reqwest::Client,
}

impl ResourceLoader {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("fOS-Engine/0.1")
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        
        Self { client }
    }
    
    /// Fetch a URL with GET
    pub async fn fetch(&self, url: &str) -> Result<Response, NetError> {
        self.request(Request::get(url)).await
    }
    
    /// Make an HTTP request
    pub async fn request(&self, req: Request) -> Result<Response, NetError> {
        tracing::info!("HTTP {:?} {}", req.method, req.url);
        
        let method = match req.method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Delete => reqwest::Method::DELETE,
            Method::Head => reqwest::Method::HEAD,
            Method::Options => reqwest::Method::OPTIONS,
            Method::Patch => reqwest::Method::PATCH,
        };
        
        let mut builder = self.client.request(method, &req.url);
        
        // Add headers
        for (key, value) in &req.headers {
            if let (Ok(name), Ok(val)) = (
                HeaderName::from_str(key),
                HeaderValue::from_str(value),
            ) {
                builder = builder.header(name, val);
            }
        }
        
        // Add body
        if let Some(body) = req.body {
            builder = builder.body(body);
        }
        
        // Execute request
        let response = builder.send().await.map_err(|e| {
            NetError::Network(e.to_string())
        })?;
        
        let status = response.status().as_u16();
        
        // Convert headers
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str().ok().map(|v| (k.to_string(), v.to_string()))
            })
            .collect();
        
        // Get body
        let body = response.bytes().await.map_err(|e| {
            NetError::Network(e.to_string())
        })?.to_vec();
        
        Ok(Response {
            status,
            headers,
            body,
        })
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
}
